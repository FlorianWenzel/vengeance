package main

import (
	"context"
	"crypto/sha256"
	"errors"
	"fmt"
	"log/slog"
	"net/http"
	"os"
	"os/signal"
	"strconv"
	"strings"
	"syscall"
	"time"

	"github.com/FlorianWenzel/vengeance/internal/db"
	"github.com/FlorianWenzel/vengeance/internal/notify"
	"github.com/FlorianWenzel/vengeance/internal/server"
	"github.com/FlorianWenzel/vengeance/internal/store"
	"github.com/FlorianWenzel/vengeance/web"
)

const defaultKEV = "https://www.cisa.gov/sites/default/files/feeds/known_exploited_vulnerabilities.json"

func env(k, def string) string {
	if v := strings.TrimSpace(os.Getenv(k)); v != "" {
		return v
	}
	return def
}

func list(k string) []string {
	var out []string
	for _, v := range strings.Split(os.Getenv(k), ",") {
		if v = strings.TrimSpace(v); v != "" {
			out = append(out, v)
		}
	}
	return out
}

func serve() error {
	log := slog.New(slog.NewJSONHandler(os.Stdout, nil))
	dbURL := env("VENGEANCE_DATABASE_URL", "")
	base := strings.TrimRight(env("VENGEANCE_BASE_URL", "http://localhost:8080"), "/")
	secret := env("VENGEANCE_SECRET", "")
	issuer := env("VENGEANCE_OIDC_ISSUER", "")
	var missing []string
	for k, v := range map[string]string{"VENGEANCE_DATABASE_URL": dbURL, "VENGEANCE_SECRET": secret, "VENGEANCE_OIDC_ISSUER": issuer,
		"VENGEANCE_OIDC_CLIENT_ID": env("VENGEANCE_OIDC_CLIENT_ID", "")} {
		if v == "" {
			missing = append(missing, k)
		}
	}
	if len(missing) > 0 {
		return fmt.Errorf("missing required settings: %s", strings.Join(missing, ", "))
	}
	if len(secret) < 32 {
		return errors.New("VENGEANCE_SECRET must be at least 32 characters")
	}
	oidcCfg := server.OIDCConfig{
		Issuer: issuer, ClientID: env("VENGEANCE_OIDC_CLIENT_ID", ""), ClientSecret: env("VENGEANCE_OIDC_CLIENT_SECRET", ""),
		Scopes: list("VENGEANCE_OIDC_SCOPES"), AllowedEmails: list("VENGEANCE_OIDC_ALLOWED_EMAILS"),
		AllowedDomains: list("VENGEANCE_OIDC_ALLOWED_DOMAINS"), AllowAll: env("VENGEANCE_OIDC_ALLOW_ALL", "") == "true",
	}
	if !oidcCfg.AllowAll && len(oidcCfg.AllowedEmails) == 0 && len(oidcCfg.AllowedDomains) == 0 {
		return errors.New("set VENGEANCE_OIDC_ALLOWED_EMAILS or VENGEANCE_OIDC_ALLOWED_DOMAINS (or VENGEANCE_OIDC_ALLOW_ALL=true if the provider itself restricts who can sign in)")
	}

	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()

	var pool interface{ Close() }
	var st *store.Store
	for attempt := 1; ; attempt++ {
		p, err := db.Open(ctx, dbURL)
		if err == nil {
			pool, st = p, store.New(p)
			break
		}
		if attempt >= 30 || ctx.Err() != nil {
			return err
		}
		log.Warn("waiting for database", "err", err, "attempt", attempt)
		time.Sleep(2 * time.Second)
	}
	defer pool.Close()

	kevURL := env("VENGEANCE_KEV_URL", defaultKEV)
	if kevURL == "off" {
		kevURL = ""
	}
	sender := &notify.Sender{SMTP: notify.SMTP{
		Host: env("VENGEANCE_SMTP_HOST", ""), Port: env("VENGEANCE_SMTP_PORT", ""), User: env("VENGEANCE_SMTP_USER", ""),
		Password: env("VENGEANCE_SMTP_PASSWORD", ""), From: env("VENGEANCE_SMTP_FROM", ""), TLS: env("VENGEANCE_SMTP_TLS", "starttls"),
	}}
	eng := &notify.Engine{Store: st, Sender: sender, BaseURL: base, KEVURL: kevURL, Log: log}
	go eng.Run(ctx)

	maxMiB, _ := strconv.ParseInt(env("VENGEANCE_MAX_REPORT_MIB", "64"), 10, 64)
	key := sha256.Sum256([]byte("vengeance-cookie:" + secret))
	h := server.New(st, eng, server.Config{BaseURL: base, Secret: key[:], OIDC: oidcCfg, Web: web.Dist(), MaxIngestMiB: maxMiB, Version: version}, log)
	srv := &http.Server{Addr: env("VENGEANCE_LISTEN", ":8080"), Handler: h, ReadHeaderTimeout: 10 * time.Second,
		ReadTimeout: 5 * time.Minute, WriteTimeout: 5 * time.Minute, IdleTimeout: 2 * time.Minute}
	go func() {
		<-ctx.Done()
		sctx, cancel := context.WithTimeout(context.Background(), 15*time.Second)
		defer cancel()
		_ = srv.Shutdown(sctx)
	}()
	log.Info("vengeance listening", "addr", srv.Addr, "base", base, "version", version)
	if err := srv.ListenAndServe(); err != nil && !errors.Is(err, http.ErrServerClosed) {
		return err
	}
	return nil
}
