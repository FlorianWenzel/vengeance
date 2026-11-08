package notify

import (
	"context"
	"crypto/hmac"
	"crypto/sha256"
	"encoding/hex"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"github.com/FlorianWenzel/vengeance/internal/store"
)

func TestWebhookSignature(t *testing.T) {
	var got, sig string
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		b, _ := io.ReadAll(r.Body)
		got, sig = string(b), r.Header.Get("X-Vengeance-Signature")
	}))
	defer srv.Close()
	s := &Sender{}
	err := s.Send(context.Background(), store.Channel{Kind: "webhook", Config: map[string]string{"url": srv.URL, "secret": "k"}},
		Message{Subject: "hi", Body: "b"})
	if err != nil {
		t.Fatal(err)
	}
	m := hmac.New(sha256.New, []byte("k"))
	m.Write([]byte(got))
	if sig != "sha256="+hex.EncodeToString(m.Sum(nil)) {
		t.Errorf("bad signature %q", sig)
	}
}

func TestNtfyHeaders(t *testing.T) {
	var path, title, prio, auth string
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		path, title, prio, auth = r.URL.Path, r.Header.Get("Title"), r.Header.Get("Priority"), r.Header.Get("Authorization")
	}))
	defer srv.Close()
	s := &Sender{}
	err := s.Send(context.Background(), store.Channel{Kind: "ntfy", Config: map[string]string{"server": srv.URL, "topic": "abc", "token": "tk"}},
		Message{Subject: "[Vengeance] P1 thing", Body: "b", Priority: 1})
	if err != nil {
		t.Fatal(err)
	}
	if path != "/abc" || !strings.Contains(title, "P1") || prio != "urgent" || auth != "Bearer tk" {
		t.Errorf("path=%q title=%q prio=%q auth=%q", path, title, prio, auth)
	}
}

func TestSendErrorSurfacesStatus(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		http.Error(w, "nope", 403)
	}))
	defer srv.Close()
	err := (&Sender{}).Send(context.Background(), store.Channel{Kind: "webhook", Config: map[string]string{"url": srv.URL}}, Message{})
	if err == nil || !strings.Contains(err.Error(), "403") {
		t.Errorf("err %v", err)
	}
}

func TestCompose(t *testing.T) {
	one := Compose([]store.Notification{{Subject: "New: P2 CVE-1", Body: "x", RuleName: "r", Data: map[string]any{"priority": 2.0, "id": 7.0}}}, false, "https://v.example")
	if one.Subject != "[Vengeance] New: P2 CVE-1" || one.Priority != 2 || one.Link != "https://v.example/findings?id=7" {
		t.Errorf("%+v", one)
	}
	many := Compose([]store.Notification{
		{Subject: "a", RuleName: "r", Data: map[string]any{"priority": 3.0}},
		{Subject: "b", RuleName: "r", Data: map[string]any{"priority": 1.0}},
	}, true, "https://v.example")
	if !strings.Contains(many.Subject, "2 digest") || many.Priority != 1 || !strings.HasPrefix(many.Body, "- b") {
		t.Errorf("%+v", many)
	}
}

func TestDigestSlot(t *testing.T) {
	// Wednesday 2026-11-11 07:00 UTC
	now := time.Date(2026, 11, 11, 7, 0, 0, 0, time.UTC)
	if got := DigestSlot("daily", now); !got.Equal(time.Date(2026, 11, 10, 8, 0, 0, 0, time.UTC)) {
		t.Errorf("daily %v", got)
	}
	if got := DigestSlot("weekly", now); !got.Equal(time.Date(2026, 11, 9, 8, 0, 0, 0, time.UTC)) {
		t.Errorf("weekly %v", got)
	}
}
