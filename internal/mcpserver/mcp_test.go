package mcpserver_test

import (
	"context"
	"io"
	"log/slog"
	"net/http"
	"net/http/httptest"
	"os"
	"slices"
	"strings"
	"testing"

	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/modelcontextprotocol/go-sdk/mcp"

	"github.com/FlorianWenzel/vengeance/internal/db"
	"github.com/FlorianWenzel/vengeance/internal/ingest"
	"github.com/FlorianWenzel/vengeance/internal/mcpserver"
	"github.com/FlorianWenzel/vengeance/internal/store"
)

type bearer struct {
	tok  string
	base http.RoundTripper
}

func (b bearer) RoundTrip(r *http.Request) (*http.Response, error) {
	r = r.Clone(r.Context())
	r.Header.Set("Authorization", "Bearer "+b.tok)
	return b.base.RoundTrip(r)
}

func setup(t *testing.T) (*store.Store, string) {
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
	ts := httptest.NewServer(mcpserver.Handler(st, "test", slog.New(slog.NewTextHandler(io.Discard, nil))))
	t.Cleanup(func() { ts.Close(); pool.Close() })
	return st, ts.URL
}

func connect(t *testing.T, url, tok string) *mcp.ClientSession {
	t.Helper()
	c := mcp.NewClient(&mcp.Implementation{Name: "test", Version: "1"}, nil)
	cs, err := c.Connect(context.Background(), &mcp.StreamableClientTransport{Endpoint: url,
		HTTPClient: &http.Client{Transport: bearer{tok, http.DefaultTransport}}, DisableStandaloneSSE: true, MaxRetries: -1}, nil)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { cs.Close() })
	return cs
}

func call(t *testing.T, cs *mcp.ClientSession, name string, args map[string]any) (string, bool) {
	t.Helper()
	res, err := cs.CallTool(context.Background(), &mcp.CallToolParams{Name: name, Arguments: args})
	if err != nil {
		t.Fatalf("%s: %v", name, err)
	}
	var b strings.Builder
	for _, c := range res.Content {
		if tc, ok := c.(*mcp.TextContent); ok {
			b.WriteString(tc.Text)
		}
	}
	return b.String(), res.IsError
}

func toolNames(t *testing.T, cs *mcp.ClientSession) []string {
	t.Helper()
	res, err := cs.ListTools(context.Background(), nil)
	if err != nil {
		t.Fatal(err)
	}
	var out []string
	for _, tl := range res.Tools {
		out = append(out, tl.Name)
	}
	return out
}

func TestMCP(t *testing.T) {
	st, url := setup(t)
	ctx := context.Background()
	u, _ := st.UpsertUser(ctx, "iss|1", "a@example.com", "A")
	readTok, err := st.CreateAPIToken(ctx, u.ID, "claude read", "read")
	if err != nil {
		t.Fatal(err)
	}
	writeTok, _ := st.CreateAPIToken(ctx, u.ID, "claude", "write")
	name := "ci"
	src, _ := st.CreateSource(ctx, store.SourceInput{Name: &name})
	_, err = st.Ingest(ctx, src.ID, "ci", store.IngestOptions{Asset: "image:ghcr.io/me/api"}, &ingest.Report{Format: "trivy", ScanTypes: []string{"vuln"},
		Findings: []ingest.Finding{{Kind: "vuln", AdvisoryID: "CVE-2026-1", PkgName: "openssl", Ecosystem: "alpine", Installed: "3.0.0", Fixed: "3.0.1", Severity: "high", Title: "boom"}}}, nil)
	if err != nil {
		t.Fatal(err)
	}

	// no / bad token
	resp, _ := http.Post(url, "application/json", strings.NewReader(`{}`))
	if resp.StatusCode != 401 {
		t.Errorf("no token: %d", resp.StatusCode)
	}
	req, _ := http.NewRequest("POST", url, strings.NewReader(`{}`))
	req.Header.Set("Authorization", "Bearer vgp_nope")
	resp, _ = http.DefaultClient.Do(req)
	if resp.StatusCode != 401 {
		t.Errorf("bad token: %d", resp.StatusCode)
	}

	// read token: read tools only
	rd := connect(t, url, readTok.Token)
	names := toolNames(t, rd)
	if !slices.Contains(names, "list_findings") || slices.Contains(names, "triage_findings") || slices.Contains(names, "create_asset") {
		t.Errorf("read tools: %v", names)
	}
	out, isErr := call(t, rd, "vengeance_overview", nil)
	if isErr || !strings.Contains(out, `"p2": 1`) {
		t.Errorf("overview: %s", out)
	}
	out, _ = call(t, rd, "list_findings", map[string]any{"query": "openssl"})
	if !strings.Contains(out, "CVE-2026-1") || !strings.Contains(out, `"total": 1`) {
		t.Errorf("findings: %s", out)
	}
	out, _ = call(t, rd, "list_fixes", nil)
	if !strings.Contains(out, "apk upgrade --no-cache openssl") {
		t.Errorf("fixes: %s", out)
	}

	// write token: triage and inventory
	wr := connect(t, url, writeTok.Token)
	if !slices.Contains(toolNames(t, wr), "create_asset") {
		t.Fatal("write token lacks write tools")
	}
	out, isErr = call(t, wr, "create_asset", map[string]any{"kind": "repository", "identifier": "github.com/me/app", "project": "apps", "tags": []string{"prod"}})
	if isErr || !strings.Contains(out, `"projectName": "apps"`) {
		t.Fatalf("create_asset: %s", out)
	}
	out, isErr = call(t, wr, "create_asset", map[string]any{"kind": "repository", "identifier": "github.com/me/app"})
	if !isErr || !strings.Contains(out, "already exists") {
		t.Errorf("duplicate create_asset: %v %s", isErr, out)
	}
	out, isErr = call(t, wr, "create_asset", map[string]any{"kind": "spaceship", "identifier": "x"})
	if !isErr {
		t.Errorf("bad kind accepted: %s", out)
	}
	items, _, _ := st.ListFindings(ctx, store.FindingFilter{})
	out, isErr = call(t, wr, "triage_findings", map[string]any{"ids": []int64{items[0].ID}, "state": "false_positive"})
	if !isErr || !strings.Contains(out, "reason") {
		t.Errorf("fp without reason: %s", out)
	}
	out, isErr = call(t, wr, "triage_findings", map[string]any{"ids": []int64{items[0].ID}, "state": "acknowledged"})
	if isErr || !strings.Contains(out, `"updated": 1`) {
		t.Errorf("triage: %s", out)
	}
	d, _ := st.GetFinding(ctx, items[0].ID)
	if d.State != "acknowledged" || d.Events[len(d.Events)-1].Actor != "mcp:claude" {
		t.Errorf("state %s events %+v", d.State, d.Events)
	}
	// read token cannot call write tools even by name
	_, err = rd.CallTool(ctx, &mcp.CallToolParams{Name: "triage_findings", Arguments: map[string]any{"ids": []int64{1}, "state": "new"}})
	if err == nil {
		t.Error("read token could call triage_findings")
	}
	// revoked token stops working
	_ = st.DeleteAPIToken(ctx, readTok.ID)
	req, _ = http.NewRequest("POST", url, strings.NewReader(`{"jsonrpc":"2.0","id":1,"method":"tools/list"}`))
	req.Header.Set("Authorization", "Bearer "+readTok.Token)
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Accept", "application/json, text/event-stream")
	resp, _ = http.DefaultClient.Do(req)
	if resp.StatusCode != 401 {
		t.Errorf("revoked token: %d", resp.StatusCode)
	}
}
