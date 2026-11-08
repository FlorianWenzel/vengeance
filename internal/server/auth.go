package server

import (
	"context"
	"crypto/hmac"
	"crypto/rand"
	"crypto/sha256"
	"encoding/base64"
	"encoding/json"
	"errors"
	"net/http"
	"strings"
	"sync"
	"time"

	"github.com/coreos/go-oidc/v3/oidc"
	"golang.org/x/oauth2"

	"github.com/FlorianWenzel/vengeance/internal/store"
)

// OIDCConfig configures sign-in.
type OIDCConfig struct {
	Issuer         string
	ClientID       string
	ClientSecret   string
	Scopes         []string
	AllowedEmails  []string
	AllowedDomains []string
	AllowAll       bool
}

type auth struct {
	cfg    OIDCConfig
	base   string
	secret []byte
	secure bool

	mu       sync.Mutex
	provider *oidc.Provider
	oauth    *oauth2.Config
	verifier *oidc.IDTokenVerifier
}

const (
	sessionCookie = "vg_session"
	flowCookie    = "vg_oidc"
	sessionTTL    = 14 * 24 * time.Hour
)

// lazily discover so the server starts even if the IdP is briefly down
func (a *auth) init(ctx context.Context) error {
	a.mu.Lock()
	defer a.mu.Unlock()
	if a.provider != nil {
		return nil
	}
	p, err := oidc.NewProvider(ctx, a.cfg.Issuer)
	if err != nil {
		return err
	}
	scopes := a.cfg.Scopes
	if len(scopes) == 0 {
		scopes = []string{oidc.ScopeOpenID, "email", "profile"}
	}
	a.provider = p
	a.oauth = &oauth2.Config{
		ClientID: a.cfg.ClientID, ClientSecret: a.cfg.ClientSecret, Endpoint: p.Endpoint(),
		RedirectURL: strings.TrimRight(a.base, "/") + "/auth/callback", Scopes: scopes,
	}
	a.verifier = p.Verifier(&oidc.Config{ClientID: a.cfg.ClientID})
	return nil
}

type flow struct {
	State    string `json:"s"`
	Nonce    string `json:"n"`
	Verifier string `json:"v"`
	Next     string `json:"x"`
	Exp      int64  `json:"e"`
}

func randStr() string {
	b := make([]byte, 24)
	_, _ = rand.Read(b)
	return base64.RawURLEncoding.EncodeToString(b)
}

func (a *auth) sign(v []byte) string {
	m := hmac.New(sha256.New, a.secret)
	m.Write(v)
	return base64.RawURLEncoding.EncodeToString(v) + "." + base64.RawURLEncoding.EncodeToString(m.Sum(nil))
}

func (a *auth) unsign(s string) ([]byte, bool) {
	p, sig, ok := strings.Cut(s, ".")
	if !ok {
		return nil, false
	}
	v, err1 := base64.RawURLEncoding.DecodeString(p)
	got, err2 := base64.RawURLEncoding.DecodeString(sig)
	if err1 != nil || err2 != nil {
		return nil, false
	}
	m := hmac.New(sha256.New, a.secret)
	m.Write(v)
	return v, hmac.Equal(m.Sum(nil), got)
}

func safeNext(n string) string {
	if n == "" || !strings.HasPrefix(n, "/") || strings.HasPrefix(n, "//") || strings.HasPrefix(n, "/\\") || strings.HasPrefix(n, "/auth/") {
		return "/"
	}
	return n
}

func (s *Server) login(w http.ResponseWriter, r *http.Request) {
	if err := s.auth.init(r.Context()); err != nil {
		s.log.Error("oidc discovery", "err", err)
		http.Error(w, "The sign-in provider is not reachable right now. Try again in a minute.", http.StatusBadGateway)
		return
	}
	f := flow{State: randStr(), Nonce: randStr(), Verifier: oauth2.GenerateVerifier(), Next: safeNext(r.URL.Query().Get("next")),
		Exp: time.Now().Add(10 * time.Minute).Unix()}
	raw, _ := json.Marshal(f)
	http.SetCookie(w, &http.Cookie{Name: flowCookie, Value: s.auth.sign(raw), Path: "/auth/", HttpOnly: true,
		Secure: s.auth.secure, SameSite: http.SameSiteLaxMode, MaxAge: 600})
	url := s.auth.oauth.AuthCodeURL(f.State, oidc.Nonce(f.Nonce), oauth2.S256ChallengeOption(f.Verifier))
	http.Redirect(w, r, url, http.StatusFound)
}

func (s *Server) callback(w http.ResponseWriter, r *http.Request) {
	fail := func(msg string, err error) {
		s.log.Warn("login failed", "reason", msg, "err", err)
		w.Header().Set("Content-Type", "text/html; charset=utf-8")
		w.WriteHeader(http.StatusForbidden)
		_, _ = w.Write([]byte(`<!doctype html><meta charset="utf-8"><title>Sign-in failed</title><body style="font-family:system-ui;background:#0b0d12;color:#e7e9ee;padding:3rem">
<h1>Sign-in failed</h1><p>` + htmlEscape(msg) + `</p><p><a style="color:#f2b84b" href="/auth/login">Try again</a></p>`))
	}
	if err := s.auth.init(r.Context()); err != nil {
		fail("The sign-in provider is not reachable.", err)
		return
	}
	c, err := r.Cookie(flowCookie)
	if err != nil {
		fail("The sign-in took too long or cookies are blocked. Start again.", err)
		return
	}
	raw, ok := s.auth.unsign(c.Value)
	var f flow
	if !ok || json.Unmarshal(raw, &f) != nil || time.Now().Unix() > f.Exp {
		fail("The sign-in link expired. Start again.", nil)
		return
	}
	http.SetCookie(w, &http.Cookie{Name: flowCookie, Value: "", Path: "/auth/", MaxAge: -1, HttpOnly: true, Secure: s.auth.secure})
	if e := r.URL.Query().Get("error"); e != "" {
		fail("The provider refused the sign-in: "+e, nil)
		return
	}
	if subtleNe(r.URL.Query().Get("state"), f.State) {
		fail("The sign-in state did not match. Start again.", nil)
		return
	}
	tok, err := s.auth.oauth.Exchange(r.Context(), r.URL.Query().Get("code"), oauth2.VerifierOption(f.Verifier))
	if err != nil {
		fail("Could not complete the sign-in with the provider.", err)
		return
	}
	rawID, _ := tok.Extra("id_token").(string)
	idt, err := s.auth.verifier.Verify(r.Context(), rawID)
	if err != nil || idt.Nonce != f.Nonce {
		fail("The provider's identity token was not valid.", err)
		return
	}
	var claims struct {
		Email         string `json:"email"`
		EmailVerified *bool  `json:"email_verified"`
		Name          string `json:"name"`
		PreferredUser string `json:"preferred_username"`
	}
	if err := idt.Claims(&claims); err != nil {
		fail("The identity token had no usable claims.", err)
		return
	}
	email := strings.ToLower(strings.TrimSpace(claims.Email))
	if email == "" || (claims.EmailVerified != nil && !*claims.EmailVerified) {
		fail("Your account has no verified email address.", nil)
		return
	}
	if !s.auth.allowed(email) {
		fail(email+" is not allowed to use this Vengeance instance.", nil)
		return
	}
	name := claims.Name
	if name == "" {
		name = claims.PreferredUser
	}
	u, err := s.store.UpsertUser(r.Context(), idt.Issuer+"|"+idt.Subject, email, name)
	if err != nil {
		fail("Could not save your account.", err)
		return
	}
	sess, err := s.store.CreateSession(r.Context(), u.ID, sessionTTL)
	if err != nil {
		fail("Could not start a session.", err)
		return
	}
	http.SetCookie(w, &http.Cookie{Name: sessionCookie, Value: sess, Path: "/", HttpOnly: true, Secure: s.auth.secure,
		SameSite: http.SameSiteLaxMode, MaxAge: int(sessionTTL.Seconds())})
	http.Redirect(w, r, safeNext(f.Next), http.StatusFound)
}

func subtleNe(a, b string) bool { return !hmac.Equal([]byte(a), []byte(b)) || a == "" }

func (a *auth) allowed(email string) bool {
	if a.cfg.AllowAll {
		return true
	}
	for _, e := range a.cfg.AllowedEmails {
		if strings.EqualFold(strings.TrimSpace(e), email) {
			return true
		}
	}
	at := strings.LastIndex(email, "@")
	for _, d := range a.cfg.AllowedDomains {
		if strings.EqualFold(strings.TrimPrefix(strings.TrimSpace(d), "@"), email[at+1:]) {
			return true
		}
	}
	return false
}

func (s *Server) logout(w http.ResponseWriter, r *http.Request) {
	if c, err := r.Cookie(sessionCookie); err == nil {
		_ = s.store.DeleteSession(r.Context(), c.Value)
	}
	http.SetCookie(w, &http.Cookie{Name: sessionCookie, Value: "", Path: "/", MaxAge: -1, HttpOnly: true, Secure: s.auth.secure})
	w.WriteHeader(http.StatusNoContent)
}

type ctxKey int

const userKey ctxKey = 1

func (s *Server) requireUser(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		c, err := r.Cookie(sessionCookie)
		if err != nil {
			writeErr(w, http.StatusUnauthorized, "unauthenticated")
			return
		}
		u, err := s.store.SessionUser(r.Context(), c.Value)
		if errors.Is(err, store.ErrNotFound) {
			writeErr(w, http.StatusUnauthorized, "unauthenticated")
			return
		}
		if err != nil {
			s.internal(w, err)
			return
		}
		if r.Method != http.MethodGet && r.Method != http.MethodHead && r.Header.Get("X-Vengeance-CSRF") != "1" {
			writeErr(w, http.StatusForbidden, "missing X-Vengeance-CSRF header")
			return
		}
		next.ServeHTTP(w, r.WithContext(context.WithValue(r.Context(), userKey, u)))
	})
}

func currentUser(r *http.Request) *store.User {
	u, _ := r.Context().Value(userKey).(*store.User)
	return u
}

func htmlEscape(s string) string {
	return strings.NewReplacer("&", "&amp;", "<", "&lt;", ">", "&gt;", `"`, "&quot;").Replace(s)
}
