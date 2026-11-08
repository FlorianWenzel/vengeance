package server_test

import (
	"context"
	"encoding/json"
	"io"
	"log/slog"
	"net/http"
	"net/http/httptest"
	"os"
	"strings"
	"testing"
	"testing/fstest"

	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/FlorianWenzel/vengeance/internal/db"
	"github.com/FlorianWenzel/vengeance/internal/notify"
	"github.com/FlorianWenzel/vengeance/internal/server"
	"github.com/FlorianWenzel/vengeance/internal/store"
)

func setup(t *testing.T) (*httptest.Server, *store.Store) {
	t.Helper()
	url := os.Getenv("VENGEANCE_TEST_DATABASE_URL")
	if url == "" {
		t.Skip("VENGEANCE_TEST_DATABASE_URL not set")
	}
	ctx := context.Background()
	pool, err := pgxpool.New(ctx, url)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := pool.Exec(ctx, `DROP SCHEMA public CASCADE; CREATE SCHEMA public;`); err != nil {
		t.Fatal(err)
	}
	if err := db.Migrate(ctx, pool); err != nil {
		t.Fatal(err)
	}
	st := store.New(pool)
	log := slog.New(slog.NewTextHandler(io.Discard, nil))
	eng := &notify.Engine{Store: st, Sender: &notify.Sender{}, BaseURL: "http://x", Log: log}
	web := fstest.MapFS{"index.html": {Data: []byte("<html>spa</html>")}, "assets/a.js": {Data: []byte("js")}}
	h := server.New(st, eng, server.Config{BaseURL: "http://x", Secret: []byte("0123456789abcdef0123456789abcdef"),
		OIDC: server.OIDCConfig{Issuer: "http://127.0.0.1:1", ClientID: "x", AllowAll: true}, Web: web}, log)
	ts := httptest.NewServer(h)
	t.Cleanup(func() { ts.Close(); pool.Close() })
	return ts, st
}

func do(t *testing.T, method, url, body string, hdr map[string]string) (int, string) {
	t.Helper()
	req, _ := http.NewRequest(method, url, strings.NewReader(body))
	for k, v := range hdr {
		req.Header.Set(k, v)
	}
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	b, _ := io.ReadAll(resp.Body)
	return resp.StatusCode, string(b)
}

func TestIngestAndAPI(t *testing.T) {
	ts, st := setup(t)
	ctx := context.Background()
	name := "ci"
	src, err := st.CreateSource(ctx, store.SourceInput{Name: &name})
	if err != nil {
		t.Fatal(err)
	}
	report, _ := os.ReadFile("../ingest/testdata/image.json")

	code, _ := do(t, "POST", ts.URL+"/api/v1/ingest?format=trivy", string(report), nil)
	if code != 401 {
		t.Errorf("ingest without token: %d", code)
	}
	code, _ = do(t, "POST", ts.URL+"/api/v1/ingest?format=trivy", string(report), map[string]string{"Authorization": "Bearer vg_wrong"})
	if code != 401 {
		t.Errorf("ingest with bad token: %d", code)
	}
	code, body := do(t, "POST", ts.URL+"/api/v1/ingest?format=nessus", string(report), map[string]string{"Authorization": "Bearer " + src.Token})
	if code != 400 {
		t.Errorf("bad format: %d %s", code, body)
	}
	code, body = do(t, "POST", ts.URL+"/api/v1/ingest?format=trivy&project=demo", string(report), map[string]string{"Authorization": "Bearer " + src.Token})
	if code != 200 {
		t.Fatalf("ingest: %d %s", code, body)
	}
	var res store.IngestResult
	_ = json.Unmarshal([]byte(body), &res)
	if res.New < 40 {
		t.Errorf("ingest result %+v", res)
	}

	// UI API needs a session
	code, _ = do(t, "GET", ts.URL+"/api/v1/findings", "", nil)
	if code != 401 {
		t.Errorf("unauthenticated list: %d", code)
	}
	u, _ := st.UpsertUser(ctx, "iss|sub", "a@example.com", "A")
	sess, _ := st.CreateSession(ctx, u.ID, 3600e9)
	cookie := map[string]string{"Cookie": "vg_session=" + sess}
	code, body = do(t, "GET", ts.URL+"/api/v1/findings?limit=5", "", cookie)
	if code != 200 || !strings.Contains(body, `"total"`) {
		t.Fatalf("list: %d %s", code, body)
	}
	var list struct {
		Items []store.Finding
		Total int
	}
	_ = json.Unmarshal([]byte(body), &list)
	if len(list.Items) != 5 || list.Total != res.New || list.Items[0].AssetName != "python" || list.Items[0].ProjectName == nil {
		t.Errorf("list %+v", list)
	}

	// CSRF header required for writes
	tri := `{"ids":[` + itoa(list.Items[0].ID) + `],"state":"acknowledged"}`
	code, _ = do(t, "POST", ts.URL+"/api/v1/findings/triage", tri, cookie)
	if code != 403 {
		t.Errorf("triage without csrf: %d", code)
	}
	cookie["X-Vengeance-CSRF"] = "1"
	code, body = do(t, "POST", ts.URL+"/api/v1/findings/triage", tri, cookie)
	if code != 200 || !strings.Contains(body, `"updated":1`) {
		t.Errorf("triage: %d %s", code, body)
	}
	code, body = do(t, "POST", ts.URL+"/api/v1/findings/triage", `{"ids":[1],"state":"accepted"}`, cookie)
	if code != 400 || !strings.Contains(body, "end date") {
		t.Errorf("accept without date: %d %s", code, body)
	}
	for _, p := range []string{"/api/v1/overview", "/api/v1/fixes", "/api/v1/assets", "/api/v1/coverage", "/api/v1/sources",
		"/api/v1/channels", "/api/v1/rules", "/api/v1/notifications", "/api/v1/projects", "/api/v1/expectations", "/api/v1/me",
		"/api/v1/findings/" + itoa(list.Items[0].ID)} {
		code, body = do(t, "GET", ts.URL+p, "", cookie)
		if code != 200 {
			t.Errorf("GET %s: %d %s", p, code, body)
		}
	}
	code, body = do(t, "POST", ts.URL+"/api/v1/assets", `{"kind":"host","identifier":"node1","tags":["Prod"," prod "]}`, cookie)
	if code != 201 || !strings.Contains(body, `"tags":["prod"]`) {
		t.Errorf("create asset: %d %s", code, body)
	}
	code, _ = do(t, "POST", ts.URL+"/api/v1/assets", `{"kind":"host","identifier":"node1"}`, cookie)
	if code != 409 {
		t.Errorf("duplicate asset: %d", code)
	}
	code, body = do(t, "POST", ts.URL+"/api/v1/assets", `{"kind":"spaceship","identifier":"x"}`, cookie)
	if code != 400 {
		t.Errorf("bad kind: %d %s", code, body)
	}
	code, _ = do(t, "GET", ts.URL+"/api/v1/nope", "", cookie)
	if code != 404 {
		t.Errorf("unknown api: %d", code)
	}

	// SPA fallback and assets
	code, body = do(t, "GET", ts.URL+"/findings?x=1", "", nil)
	if code != 200 || !strings.Contains(body, "spa") {
		t.Errorf("spa fallback: %d %s", code, body)
	}
	code, body = do(t, "GET", ts.URL+"/assets/a.js", "", nil)
	if code != 200 || body != "js" {
		t.Errorf("asset: %d %s", code, body)
	}
	// logout kills the session
	code, _ = do(t, "POST", ts.URL+"/auth/logout", "", cookie)
	if code != 204 {
		t.Errorf("logout %d", code)
	}
	code, _ = do(t, "GET", ts.URL+"/api/v1/me", "", cookie)
	if code != 401 {
		t.Errorf("after logout %d", code)
	}
}

func itoa(i int64) string {
	b, _ := json.Marshal(i)
	return string(b)
}
