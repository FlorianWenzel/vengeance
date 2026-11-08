// Package server is the HTTP layer.
package server

import (
	"encoding/json"
	"errors"
	"io"
	"io/fs"
	"log/slog"
	"net/http"
	"path"
	"strconv"
	"strings"
	"time"

	"github.com/go-chi/chi/v5"
	"github.com/go-chi/chi/v5/middleware"

	"github.com/FlorianWenzel/vengeance/internal/ingest"
	"github.com/FlorianWenzel/vengeance/internal/mcpserver"
	"github.com/FlorianWenzel/vengeance/internal/notify"
	"github.com/FlorianWenzel/vengeance/internal/store"
)

// Config for the server.
type Config struct {
	BaseURL      string
	Secret       []byte
	OIDC         OIDCConfig
	Web          fs.FS // built SPA (index.html at root); nil disables UI
	MaxIngestMiB int64
	Version      string
}

// Server wires everything.
type Server struct {
	store  *store.Store
	engine *notify.Engine
	auth   *auth
	log    *slog.Logger
	cfg    Config
}

// New builds the handler.
func New(st *store.Store, eng *notify.Engine, cfg Config, log *slog.Logger) http.Handler {
	if cfg.MaxIngestMiB == 0 {
		cfg.MaxIngestMiB = 64
	}
	s := &Server{store: st, engine: eng, log: log, cfg: cfg,
		auth: &auth{cfg: cfg.OIDC, base: cfg.BaseURL, secret: cfg.Secret, secure: strings.HasPrefix(cfg.BaseURL, "https://")}}

	r := chi.NewRouter()
	r.Use(middleware.RealIP, middleware.Recoverer, securityHeaders, s.accessLog)
	r.Get("/healthz", func(w http.ResponseWriter, _ *http.Request) { _, _ = w.Write([]byte("ok")) })
	r.Get("/readyz", func(w http.ResponseWriter, r *http.Request) {
		if err := st.DB.Ping(r.Context()); err != nil {
			http.Error(w, "database unreachable", http.StatusServiceUnavailable)
			return
		}
		_, _ = w.Write([]byte("ok"))
	})
	r.Handle("/mcp", mcpserver.Handler(st, cfg.Version, log))
	r.Get("/auth/login", s.login)
	r.Get("/auth/callback", s.callback)
	r.Post("/auth/logout", s.logout)

	r.Route("/api/v1", func(r chi.Router) {
		r.Post("/ingest", s.ingest)
		r.Post("/topology", s.postTopology)
		r.Group(func(r chi.Router) {
			r.Use(s.requireUser)
			r.Get("/me", func(w http.ResponseWriter, r *http.Request) { writeJSON(w, 200, currentUser(r)) })
			r.Get("/overview", s.overview)
			r.Get("/findings", s.listFindings)
			r.Get("/findings/{id}", s.getFinding)
			r.Post("/findings/triage", s.triage)
			r.Get("/fixes", s.fixes)
			r.Get("/assets", s.listAssets)
			r.Post("/assets", s.createAsset)
			r.Get("/assets/{id}", s.getAsset)
			r.Patch("/assets/{id}", s.updateAsset)
			r.Delete("/assets/{id}", s.deleteAsset)
			r.Get("/projects", s.listProjects)
			r.Post("/projects", s.saveProject)
			r.Patch("/projects/{id}", s.saveProject)
			r.Delete("/projects/{id}", s.deleteProject)
			r.Get("/coverage", s.coverage)
			r.Get("/expectations", s.getExpectations)
			r.Put("/expectations", s.putExpectations)
			r.Get("/sources", s.listSources)
			r.Post("/sources", s.createSource)
			r.Patch("/sources/{id}", s.updateSource)
			r.Post("/sources/{id}/rotate", s.rotateSource)
			r.Delete("/sources/{id}", s.deleteSource)
			r.Get("/channels", s.listChannels)
			r.Post("/channels", s.saveChannel)
			r.Patch("/channels/{id}", s.saveChannel)
			r.Delete("/channels/{id}", s.deleteChannel)
			r.Post("/channels/{id}/test", s.testChannel)
			r.Get("/rules", s.listRules)
			r.Post("/rules", s.saveRule)
			r.Patch("/rules/{id}", s.saveRule)
			r.Delete("/rules/{id}", s.deleteRule)
			r.Get("/notifications", s.listNotifications)
			r.Get("/topology", s.getTopology)
			r.Get("/tokens", s.listTokens)
			r.Post("/tokens", s.createToken)
			r.Delete("/tokens/{id}", s.deleteToken)
		})
		r.NotFound(func(w http.ResponseWriter, _ *http.Request) { writeErr(w, 404, "no such endpoint") })
	})
	if cfg.Web != nil {
		r.Handle("/*", spa(cfg.Web))
	}
	return r
}

func securityHeaders(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		h := w.Header()
		h.Set("Content-Security-Policy", "default-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; font-src 'self' data:; script-src 'self' 'wasm-unsafe-eval'; connect-src 'self'; frame-ancestors 'none'; base-uri 'self'; form-action 'self'")
		h.Set("X-Content-Type-Options", "nosniff")
		h.Set("Referrer-Policy", "same-origin")
		h.Set("X-Frame-Options", "DENY")
		next.ServeHTTP(w, r)
	})
}

func (s *Server) accessLog(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		start := time.Now()
		ww := middleware.NewWrapResponseWriter(w, r.ProtoMajor)
		next.ServeHTTP(ww, r)
		if strings.HasPrefix(r.URL.Path, "/assets/") || r.URL.Path == "/healthz" || r.URL.Path == "/readyz" {
			return
		}
		s.log.Info("http", "method", r.Method, "path", r.URL.Path, "status", ww.Status(), "ms", time.Since(start).Milliseconds())
	})
}

func spa(web fs.FS) http.Handler {
	files := http.FileServer(http.FS(web))
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		p := strings.TrimPrefix(path.Clean(r.URL.Path), "/")
		if p != "" {
			if st, err := fs.Stat(web, p); err == nil && !st.IsDir() {
				if strings.HasPrefix(p, "assets/") {
					w.Header().Set("Cache-Control", "public, max-age=31536000, immutable")
				}
				files.ServeHTTP(w, r)
				return
			}
		}
		b, err := fs.ReadFile(web, "index.html")
		if err != nil {
			http.Error(w, "web UI not built", http.StatusNotFound)
			return
		}
		w.Header().Set("Content-Type", "text/html; charset=utf-8")
		w.Header().Set("Cache-Control", "no-cache")
		_, _ = w.Write(b)
	})
}

// ---------- helpers ----------

func writeJSON(w http.ResponseWriter, status int, v any) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(v)
}

func writeErr(w http.ResponseWriter, status int, msg string) {
	writeJSON(w, status, map[string]string{"error": msg})
}

func (s *Server) internal(w http.ResponseWriter, err error) {
	s.log.Error("internal error", "err", err)
	writeErr(w, 500, "something went wrong on the server")
}

func (s *Server) fail(w http.ResponseWriter, err error) {
	var inv store.Invalid
	switch {
	case errors.As(err, &inv):
		writeErr(w, 400, inv.Msg)
	case errors.Is(err, store.ErrNotFound):
		writeErr(w, 404, "not found")
	case errors.Is(err, store.ErrConflict):
		writeErr(w, 409, "that already exists")
	default:
		s.internal(w, err)
	}
}

func decode(w http.ResponseWriter, r *http.Request, v any) bool {
	dec := json.NewDecoder(http.MaxBytesReader(w, r.Body, 1<<20))
	dec.DisallowUnknownFields()
	if err := dec.Decode(v); err != nil {
		writeErr(w, 400, "invalid JSON body: "+err.Error())
		return false
	}
	return true
}

func idParam(w http.ResponseWriter, r *http.Request) (int64, bool) {
	id, err := strconv.ParseInt(chi.URLParam(r, "id"), 10, 64)
	if err != nil || id <= 0 {
		writeErr(w, 400, "invalid id")
		return 0, false
	}
	return id, true
}

func qInt(r *http.Request, k string) int64 {
	v, _ := strconv.ParseInt(r.URL.Query().Get(k), 10, 64)
	return v
}

// ---------- ingest ----------

// sourceAuth authenticates a collector by its source token; it writes the error itself.
func (s *Server) sourceAuth(w http.ResponseWriter, r *http.Request) (int64, string, bool) {
	tok, ok := strings.CutPrefix(r.Header.Get("Authorization"), "Bearer ")
	if !ok || tok == "" {
		writeErr(w, 401, "send the source token as Authorization: Bearer <token>")
		return 0, "", false
	}
	srcID, srcName, err := s.store.SourceByToken(r.Context(), strings.TrimSpace(tok))
	if err != nil {
		if errors.Is(err, store.ErrNotFound) {
			writeErr(w, 401, "unknown source token")
			return 0, "", false
		}
		s.internal(w, err)
		return 0, "", false
	}
	return srcID, srcName, true
}

func (s *Server) postTopology(w http.ResponseWriter, r *http.Request) {
	srcID, srcName, ok := s.sourceAuth(w, r)
	if !ok {
		return
	}
	var in store.TopologyIn
	dec := json.NewDecoder(http.MaxBytesReader(w, r.Body, 16<<20))
	if err := dec.Decode(&in); err != nil {
		writeErr(w, 400, "invalid topology JSON: "+err.Error())
		return
	}
	if err := s.store.SaveTopology(r.Context(), srcID, in); err != nil {
		s.fail(w, err)
		return
	}
	s.log.Info("topology", "source", srcName, "cluster", in.Cluster, "nodes", len(in.Nodes), "workloads", len(in.Workloads))
	writeJSON(w, 200, map[string]bool{"ok": true})
}

func (s *Server) getTopology(w http.ResponseWriter, r *http.Request) {
	sc, err := s.store.Scene(r.Context())
	if err != nil {
		s.fail(w, err)
		return
	}
	writeJSON(w, 200, sc)
}

func (s *Server) ingest(w http.ResponseWriter, r *http.Request) {
	srcID, srcName, ok := s.sourceAuth(w, r)
	if !ok {
		return
	}
	q := r.URL.Query()
	body, err := io.ReadAll(http.MaxBytesReader(w, r.Body, s.cfg.MaxIngestMiB<<20))
	if err != nil {
		writeErr(w, 413, "report too large")
		return
	}
	rep, err := ingest.Parse(q.Get("format"), body)
	if err != nil {
		writeErr(w, 400, err.Error())
		return
	}
	var types []string
	if v := q.Get("scanners"); v != "" {
		for _, t := range strings.Split(v, ",") {
			if t = strings.TrimSpace(t); t != "" {
				types = append(types, t)
			}
		}
	}
	res, err := s.store.Ingest(r.Context(), srcID, srcName, store.IngestOptions{
		Asset: q.Get("asset"), Name: q.Get("name"), Project: q.Get("project"), ScanTypes: types, Digest: q.Get("digest"),
	}, rep, body)
	if err != nil {
		s.fail(w, err)
		return
	}
	s.log.Info("ingest", "source", srcName, "asset", res.AssetID, "new", res.New, "fixed", res.Fixed, "open", res.Open)
	s.engine.OnFindings(r.Context(), "new_finding", res.NewFindingIDs)
	writeJSON(w, 200, res)
}

// ---------- read endpoints ----------

func (s *Server) overview(w http.ResponseWriter, r *http.Request) {
	o, err := s.store.Overview(r.Context())
	if err != nil {
		s.fail(w, err)
		return
	}
	writeJSON(w, 200, o)
}

func (s *Server) listFindings(w http.ResponseWriter, r *http.Request) {
	q := r.URL.Query()
	f := store.FindingFilter{State: q.Get("state"), AssetID: qInt(r, "asset"), ProjectID: qInt(r, "project"), Kind: q.Get("kind"),
		Q: q.Get("q"), Sort: q.Get("sort"), Limit: int(qInt(r, "limit")), Offset: int(qInt(r, "offset"))}
	if f.Offset < 0 {
		f.Offset = 0
	}
	for _, p := range strings.Split(q.Get("priority"), ",") {
		if n, err := strconv.Atoi(strings.TrimSpace(p)); err == nil && n >= 1 && n <= 4 {
			f.Priorities = append(f.Priorities, n)
		}
	}
	items, total, err := s.store.ListFindings(r.Context(), f)
	if err != nil {
		s.fail(w, err)
		return
	}
	writeJSON(w, 200, map[string]any{"items": items, "total": total})
}

func (s *Server) getFinding(w http.ResponseWriter, r *http.Request) {
	id, ok := idParam(w, r)
	if !ok {
		return
	}
	f, err := s.store.GetFinding(r.Context(), id)
	if err != nil {
		s.fail(w, err)
		return
	}
	writeJSON(w, 200, f)
}

func (s *Server) triage(w http.ResponseWriter, r *http.Request) {
	var in store.TriageInput
	if !decode(w, r, &in) {
		return
	}
	n, err := s.store.Triage(r.Context(), "user:"+currentUser(r).Email, in)
	if err != nil {
		s.fail(w, err)
		return
	}
	writeJSON(w, 200, map[string]int{"updated": n})
}

func (s *Server) fixes(w http.ResponseWriter, r *http.Request) {
	f, err := s.store.Fixes(r.Context(), qInt(r, "project"))
	if err != nil {
		s.fail(w, err)
		return
	}
	writeJSON(w, 200, f)
}

func (s *Server) listAssets(w http.ResponseWriter, r *http.Request) {
	a, err := s.store.ListAssets(r.Context(), store.AssetFilter{ProjectID: qInt(r, "project"), Kind: r.URL.Query().Get("kind"),
		IncludeRetired: r.URL.Query().Get("retired") == "true"})
	if err != nil {
		s.fail(w, err)
		return
	}
	writeJSON(w, 200, a)
}

func (s *Server) getAsset(w http.ResponseWriter, r *http.Request) {
	id, ok := idParam(w, r)
	if !ok {
		return
	}
	a, err := s.store.GetAsset(r.Context(), id)
	if err != nil {
		s.fail(w, err)
		return
	}
	scans, err := s.store.AssetScans(r.Context(), id)
	if err != nil {
		s.fail(w, err)
		return
	}
	writeJSON(w, 200, struct {
		*store.Asset
		Scans []store.ScanInfo `json:"scans"`
	}{a, scans})
}

func (s *Server) createAsset(w http.ResponseWriter, r *http.Request) {
	var in store.AssetInput
	if !decode(w, r, &in) {
		return
	}
	a, err := s.store.CreateAsset(r.Context(), in)
	if err != nil {
		s.fail(w, err)
		return
	}
	writeJSON(w, 201, a)
}

func (s *Server) updateAsset(w http.ResponseWriter, r *http.Request) {
	id, ok := idParam(w, r)
	if !ok {
		return
	}
	var in store.AssetInput
	if !decode(w, r, &in) {
		return
	}
	a, err := s.store.UpdateAsset(r.Context(), id, in)
	if err != nil {
		s.fail(w, err)
		return
	}
	writeJSON(w, 200, a)
}

func (s *Server) deleteAsset(w http.ResponseWriter, r *http.Request) {
	id, ok := idParam(w, r)
	if !ok {
		return
	}
	if err := s.store.DeleteAsset(r.Context(), id); err != nil {
		s.fail(w, err)
		return
	}
	w.WriteHeader(204)
}

func (s *Server) listProjects(w http.ResponseWriter, r *http.Request) {
	p, err := s.store.ListProjects(r.Context())
	if err != nil {
		s.fail(w, err)
		return
	}
	writeJSON(w, 200, p)
}

func (s *Server) saveProject(w http.ResponseWriter, r *http.Request) {
	var id int64
	if chi.URLParam(r, "id") != "" {
		var ok bool
		if id, ok = idParam(w, r); !ok {
			return
		}
	}
	var in struct {
		Name string `json:"name"`
	}
	if !decode(w, r, &in) {
		return
	}
	p, err := s.store.SaveProject(r.Context(), id, in.Name)
	if err != nil {
		s.fail(w, err)
		return
	}
	status := 200
	if id == 0 {
		status = 201
	}
	writeJSON(w, status, p)
}

func (s *Server) deleteProject(w http.ResponseWriter, r *http.Request) {
	id, ok := idParam(w, r)
	if !ok {
		return
	}
	if err := s.store.DeleteProject(r.Context(), id); err != nil {
		s.fail(w, err)
		return
	}
	w.WriteHeader(204)
}

func (s *Server) coverage(w http.ResponseWriter, r *http.Request) {
	c, err := s.store.Coverage(r.Context())
	if err != nil {
		s.fail(w, err)
		return
	}
	writeJSON(w, 200, c)
}

func (s *Server) getExpectations(w http.ResponseWriter, r *http.Request) {
	e, err := s.store.Expectations(r.Context())
	if err != nil {
		s.fail(w, err)
		return
	}
	writeJSON(w, 200, e)
}

func (s *Server) putExpectations(w http.ResponseWriter, r *http.Request) {
	var in []store.Expectation
	if !decode(w, r, &in) {
		return
	}
	if err := s.store.SetExpectations(r.Context(), in); err != nil {
		s.fail(w, err)
		return
	}
	s.getExpectations(w, r)
}

func (s *Server) listSources(w http.ResponseWriter, r *http.Request) {
	x, err := s.store.ListSources(r.Context())
	if err != nil {
		s.fail(w, err)
		return
	}
	writeJSON(w, 200, x)
}

func (s *Server) createSource(w http.ResponseWriter, r *http.Request) {
	var in store.SourceInput
	if !decode(w, r, &in) {
		return
	}
	x, err := s.store.CreateSource(r.Context(), in)
	if err != nil {
		s.fail(w, err)
		return
	}
	writeJSON(w, 201, x)
}

func (s *Server) updateSource(w http.ResponseWriter, r *http.Request) {
	id, ok := idParam(w, r)
	if !ok {
		return
	}
	var in store.SourceInput
	if !decode(w, r, &in) {
		return
	}
	x, err := s.store.UpdateSource(r.Context(), id, in)
	if err != nil {
		s.fail(w, err)
		return
	}
	writeJSON(w, 200, x)
}

func (s *Server) rotateSource(w http.ResponseWriter, r *http.Request) {
	id, ok := idParam(w, r)
	if !ok {
		return
	}
	tok, err := s.store.RotateSource(r.Context(), id)
	if err != nil {
		s.fail(w, err)
		return
	}
	writeJSON(w, 200, map[string]string{"token": tok})
}

func (s *Server) deleteSource(w http.ResponseWriter, r *http.Request) {
	id, ok := idParam(w, r)
	if !ok {
		return
	}
	if err := s.store.DeleteSource(r.Context(), id); err != nil {
		s.fail(w, err)
		return
	}
	w.WriteHeader(204)
}

func (s *Server) listChannels(w http.ResponseWriter, r *http.Request) {
	cs, err := s.store.ListChannels(r.Context())
	if err != nil {
		s.fail(w, err)
		return
	}
	out := make([]store.Channel, len(cs))
	for i, c := range cs {
		out[i] = c.Masked()
	}
	writeJSON(w, 200, out)
}

func (s *Server) saveChannel(w http.ResponseWriter, r *http.Request) {
	var id int64
	if chi.URLParam(r, "id") != "" {
		var ok bool
		if id, ok = idParam(w, r); !ok {
			return
		}
	}
	var in store.ChannelInput
	if !decode(w, r, &in) {
		return
	}
	if id == 0 && in.Kind == nil {
		writeErr(w, 400, "kind is required")
		return
	}
	c, err := s.store.SaveChannel(r.Context(), id, in)
	if err != nil {
		s.fail(w, err)
		return
	}
	status := 200
	if id == 0 {
		status = 201
	}
	writeJSON(w, status, c.Masked())
}

func (s *Server) deleteChannel(w http.ResponseWriter, r *http.Request) {
	id, ok := idParam(w, r)
	if !ok {
		return
	}
	if err := s.store.DeleteChannel(r.Context(), id); err != nil {
		s.fail(w, err)
		return
	}
	w.WriteHeader(204)
}

func (s *Server) testChannel(w http.ResponseWriter, r *http.Request) {
	id, ok := idParam(w, r)
	if !ok {
		return
	}
	c, err := s.store.GetChannel(r.Context(), id)
	if err != nil {
		s.fail(w, err)
		return
	}
	if err := s.engine.Test(r.Context(), *c); err != nil {
		writeErr(w, 502, "Test failed: "+err.Error())
		return
	}
	writeJSON(w, 200, map[string]bool{"ok": true})
}

func (s *Server) listRules(w http.ResponseWriter, r *http.Request) {
	x, err := s.store.ListRules(r.Context(), "")
	if err != nil {
		s.fail(w, err)
		return
	}
	writeJSON(w, 200, x)
}

func (s *Server) saveRule(w http.ResponseWriter, r *http.Request) {
	var id int64
	if chi.URLParam(r, "id") != "" {
		var ok bool
		if id, ok = idParam(w, r); !ok {
			return
		}
	}
	var in store.RuleInput
	if !decode(w, r, &in) {
		return
	}
	x, err := s.store.SaveRule(r.Context(), id, in)
	if err != nil {
		s.fail(w, err)
		return
	}
	status := 200
	if id == 0 {
		status = 201
	}
	writeJSON(w, status, x)
}

func (s *Server) deleteRule(w http.ResponseWriter, r *http.Request) {
	id, ok := idParam(w, r)
	if !ok {
		return
	}
	if err := s.store.DeleteRule(r.Context(), id); err != nil {
		s.fail(w, err)
		return
	}
	w.WriteHeader(204)
}

func (s *Server) listNotifications(w http.ResponseWriter, r *http.Request) {
	x, err := s.store.ListNotifications(r.Context(), int(qInt(r, "limit")))
	if err != nil {
		s.fail(w, err)
		return
	}
	writeJSON(w, 200, x)
}

func (s *Server) listTokens(w http.ResponseWriter, r *http.Request) {
	x, err := s.store.ListAPITokens(r.Context())
	if err != nil {
		s.fail(w, err)
		return
	}
	writeJSON(w, 200, x)
}

func (s *Server) createToken(w http.ResponseWriter, r *http.Request) {
	var in struct {
		Name  string `json:"name"`
		Scope string `json:"scope"`
	}
	if !decode(w, r, &in) {
		return
	}
	t, err := s.store.CreateAPIToken(r.Context(), currentUser(r).ID, in.Name, in.Scope)
	if err != nil {
		s.fail(w, err)
		return
	}
	writeJSON(w, 201, t)
}

func (s *Server) deleteToken(w http.ResponseWriter, r *http.Request) {
	id, ok := idParam(w, r)
	if !ok {
		return
	}
	if err := s.store.DeleteAPIToken(r.Context(), id); err != nil {
		s.fail(w, err)
		return
	}
	w.WriteHeader(204)
}
