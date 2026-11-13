package store_test

import (
	"context"
	"os"
	"testing"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/FlorianWenzel/vengeance/internal/db"
	"github.com/FlorianWenzel/vengeance/internal/ingest"
	"github.com/FlorianWenzel/vengeance/internal/store"
)

// newStore connects to VENGEANCE_TEST_DATABASE_URL and resets the schema.
func newStore(t *testing.T) *store.Store {
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
	t.Cleanup(pool.Close)
	return store.New(pool)
}

func source(t *testing.T, s *store.Store, name string) int64 {
	t.Helper()
	src, err := s.CreateSource(context.Background(), store.SourceInput{Name: &name})
	if err != nil {
		t.Fatal(err)
	}
	return src.ID
}

func vuln(adv, pkg, inst, fixed, sev string) ingest.Finding {
	return ingest.Finding{Kind: "vuln", AdvisoryID: adv, PkgName: pkg, Ecosystem: "npm", Installed: inst, Fixed: fixed,
		Severity: sev, Target: "package-lock.json", Title: adv + " in " + pkg}
}

func report(fs ...ingest.Finding) *ingest.Report {
	return &ingest.Report{Format: "trivy", ScanTypes: []string{"vuln"}, ArtifactType: "repository", ArtifactName: "x", Findings: fs}
}

func TestIngestLifecycle(t *testing.T) {
	s := newStore(t)
	ctx := context.Background()
	ci := source(t, s, "ci")
	opts := store.IngestOptions{Asset: "repository:github.com/me/app", Project: "app"}

	r1, err := s.Ingest(ctx, ci, "ci", opts, report(
		vuln("CVE-1", "multer", "2.0.1", "2.5.0", "high"),
		vuln("CVE-2", "lodash", "4.17.20", "4.17.21", "critical"),
		vuln("CVE-3", "left-pad", "1.0.0", "", "medium"),
	), []byte(`{}`))
	if err != nil {
		t.Fatal(err)
	}
	if r1.New != 3 || r1.Open != 3 || len(r1.NewFindingIDs) != 3 {
		t.Fatalf("first ingest %+v", r1)
	}

	// same report again: nothing new
	r2, err := s.Ingest(ctx, ci, "ci", opts, report(
		vuln("CVE-1", "multer", "2.0.1", "2.5.0", "high"),
		vuln("CVE-2", "lodash", "4.17.20", "4.17.21", "critical"),
		vuln("CVE-3", "left-pad", "1.0.0", "", "medium"),
	), []byte(`{}`))
	if err != nil {
		t.Fatal(err)
	}
	if r2.New != 0 || r2.Fixed != 0 || r2.Open != 3 {
		t.Fatalf("second ingest %+v", r2)
	}

	// another source reports only CVE-1; it must not close anything
	other := source(t, s, "other")
	r3, err := s.Ingest(ctx, other, "other", opts, report(vuln("CVE-1", "multer", "2.0.1", "2.5.0", "high")), nil)
	if err != nil {
		t.Fatal(err)
	}
	if r3.Fixed != 0 || r3.Open != 3 {
		t.Fatalf("other source closed findings: %+v", r3)
	}

	// ci no longer reports CVE-2 (fixed) nor CVE-1; CVE-1 is still reported by "other"
	r4, err := s.Ingest(ctx, ci, "ci", opts, report(vuln("CVE-3", "left-pad", "1.0.0", "", "medium")), nil)
	if err != nil {
		t.Fatal(err)
	}
	if r4.Fixed != 1 || r4.Open != 2 {
		t.Fatalf("auto-close %+v", r4)
	}

	// a secret-only scan must not close vulnerability findings
	sec := &ingest.Report{Format: "trivy", ScanTypes: []string{"secret"}, Findings: nil}
	r5, err := s.Ingest(ctx, ci, "ci", opts, sec, nil)
	if err != nil {
		t.Fatal(err)
	}
	if r5.Fixed != 0 || r5.Open != 2 {
		t.Fatalf("secret scan closed vulns %+v", r5)
	}

	// CVE-2 comes back: reopened
	r6, err := s.Ingest(ctx, ci, "ci", opts, report(
		vuln("CVE-2", "lodash", "4.17.20", "4.17.21", "critical"),
		vuln("CVE-3", "left-pad", "1.0.0", "", "medium"),
	), nil)
	if err != nil {
		t.Fatal(err)
	}
	if r6.Reopened != 1 || r6.New != 0 || r6.Open != 3 {
		t.Fatalf("reopen %+v", r6)
	}

	items, total, err := s.ListFindings(ctx, store.FindingFilter{})
	if err != nil {
		t.Fatal(err)
	}
	if total != 3 || items[0].Priority > items[len(items)-1].Priority {
		t.Fatalf("list %d %+v", total, items)
	}
	var lodash store.Finding
	for _, f := range items {
		if f.AdvisoryID == "CVE-2" {
			lodash = f
		}
		if f.AdvisoryID == "CVE-3" && f.Priority != 4 {
			t.Errorf("no-fix finding priority %d", f.Priority)
		}
	}
	if lodash.Priority != 2 {
		t.Errorf("critical with fix should be P2, got %d", lodash.Priority)
	}
	d, err := s.GetFinding(ctx, lodash.ID)
	if err != nil {
		t.Fatal(err)
	}
	if len(d.Events) != 3 { // new, fixed, new
		t.Errorf("events %+v", d.Events)
	}

	// tagging the asset internet-facing makes critical-with-fix P1
	tags := []string{"internet-facing"}
	if _, err := s.UpdateAsset(ctx, lodash.AssetID, store.AssetInput{Tags: &tags}); err != nil {
		t.Fatal(err)
	}
	d, _ = s.GetFinding(ctx, lodash.ID)
	if d.Priority != 1 {
		t.Errorf("internet-facing critical should be P1, got %d", d.Priority)
	}
}

func TestTriageAndExpiry(t *testing.T) {
	s := newStore(t)
	ctx := context.Background()
	ci := source(t, s, "ci")
	r, err := s.Ingest(ctx, ci, "ci", store.IngestOptions{Asset: "image:ghcr.io/me/api"}, report(
		vuln("CVE-1", "openssl", "3.0.0", "3.0.1", "high"), vuln("CVE-2", "zlib", "1.0", "1.1", "low")), nil)
	if err != nil {
		t.Fatal(err)
	}
	ids := r.NewFindingIDs
	if _, err := s.Triage(ctx, "user:a", store.TriageInput{IDs: ids, State: "accepted"}); err == nil {
		t.Error("accepted without date allowed")
	}
	if _, err := s.Triage(ctx, "user:a", store.TriageInput{IDs: ids, State: "false_positive"}); err == nil {
		t.Error("false positive without reason allowed")
	}
	if _, err := s.Triage(ctx, "user:a", store.TriageInput{IDs: ids, State: "fixed"}); err == nil {
		t.Error("manual fixed allowed")
	}
	n, err := s.Triage(ctx, "user:a", store.TriageInput{IDs: ids[:1], State: "acknowledged"})
	if err != nil || n != 1 {
		t.Fatalf("ack %d %v", n, err)
	}
	until := time.Now().AddDate(0, 0, 3).Format("2006-01-02")
	n, err = s.Triage(ctx, "user:a", store.TriageInput{IDs: ids[1:], State: "accepted", Until: until, Reason: "internal only"})
	if err != nil || n != 1 {
		t.Fatalf("accept %d %v", n, err)
	}
	_, open, _ := s.ListFindings(ctx, store.FindingFilter{})
	if open != 1 {
		t.Errorf("open after accept = %d", open)
	}
	// time travel past the acceptance date
	s.Now = func() time.Time { return time.Now().AddDate(0, 0, 4) }
	exp, err := s.ExpireAcceptances(ctx)
	if err != nil || len(exp) != 1 {
		t.Fatalf("expire %v %v", exp, err)
	}
	_, open, _ = s.ListFindings(ctx, store.FindingFilter{})
	if open != 2 {
		t.Errorf("open after expiry = %d", open)
	}
}

func TestKEVFixesCoverage(t *testing.T) {
	s := newStore(t)
	ctx := context.Background()
	ci := source(t, s, "ci")
	_, err := s.Ingest(ctx, ci, "ci", store.IngestOptions{Asset: "repository:github.com/me/app"}, report(
		vuln("CVE-1", "multer", "2.0.1", "2.4.0", "low"),
		vuln("CVE-9", "multer", "2.0.1", "2.5.0", "high"),
		vuln("CVE-3", "lodash", "4.17.20", "4.17.21, 5.0.1", "medium"),
	), nil)
	if err != nil {
		t.Fatal(err)
	}
	ids, err := s.SyncKEV(ctx, []store.KEVEntry{{CVE: "CVE-1", DateAdded: time.Now()}, {CVE: "CVE-404", DateAdded: time.Now()}})
	if err != nil || len(ids) != 1 {
		t.Fatalf("kev %v %v", ids, err)
	}
	items, _, _ := s.ListFindings(ctx, store.FindingFilter{Priorities: []int{1}})
	if len(items) != 1 || !items[0].KEV || items[0].AdvisoryID != "CVE-1" {
		t.Fatalf("kev priority %+v", items)
	}
	// syncing again reports nothing new
	ids, _ = s.SyncKEV(ctx, []store.KEVEntry{{CVE: "CVE-1", DateAdded: time.Now()}})
	if len(ids) != 0 {
		t.Errorf("kev re-sync reported %v", ids)
	}

	fixes, err := s.Fixes(ctx, 0)
	if err != nil {
		t.Fatal(err)
	}
	if len(fixes) != 2 {
		t.Fatalf("fix groups %+v", fixes)
	}
	m := fixes[0]
	if m.PkgName != "multer" || m.TargetVersion != "2.5.0" || m.Findings != 2 || !m.KEV || m.MaxPriority != 1 || m.Command != "npm install multer@2.5.0" {
		t.Errorf("multer group %+v", m)
	}
	if fixes[1].TargetVersion != "4.17.21" {
		t.Errorf("lodash target %q", fixes[1].TargetVersion)
	}

	// coverage: repository expects vuln, secret, misconfig; only vuln ran
	if _, err := s.CreateAsset(ctx, store.AssetInput{Kind: "host", Identifier: "node1"}); err != nil {
		t.Fatal(err)
	}
	if _, err := s.CreateAsset(ctx, store.AssetInput{Kind: "host", Identifier: "node1"}); err != store.ErrConflict {
		t.Errorf("duplicate asset: %v", err)
	}
	cov, err := s.Coverage(ctx)
	if err != nil {
		t.Fatal(err)
	}
	// pairs: repo 3 (1 ok) + host 1 (0 ok) = 1/4
	if cov.Score != 0.25 || len(cov.Gaps) != 3 {
		t.Errorf("coverage %+v", cov)
	}
	o, err := s.Overview(ctx)
	if err != nil {
		t.Fatal(err)
	}
	if o.Open.Total != 3 || o.KEV != 1 || len(o.Trend) != 90 || len(o.Weekly) < 12 || o.Trend[89].Open != 3 {
		t.Errorf("overview open=%+v kev=%d trend=%d weekly=%d last=%+v", o.Open, o.KEV, len(o.Trend), len(o.Weekly), o.Trend[len(o.Trend)-1])
	}
}

func TestNotificationsQueue(t *testing.T) {
	s := newStore(t)
	ctx := context.Background()
	name := "hook"
	kind := "webhook"
	ch, err := s.SaveChannel(ctx, 0, store.ChannelInput{Name: &name, Kind: &kind, Config: map[string]string{"url": "https://example.com/h", "secret": "s3"}})
	if err != nil {
		t.Fatal(err)
	}
	if ch.Masked().Config["secret"] != store.Masked {
		t.Error("secret not masked")
	}
	// masked value round-trips to the stored secret
	ch, err = s.SaveChannel(ctx, ch.ID, store.ChannelInput{Config: map[string]string{"url": "https://example.com/h2", "secret": store.Masked}})
	if err != nil || ch.Config["secret"] != "s3" || ch.Config["url"] != "https://example.com/h2" {
		t.Fatalf("masked update %+v %v", ch, err)
	}
	rn, tr := "p1", "new_finding"
	r, err := s.SaveRule(ctx, 0, store.RuleInput{Name: &rn, Trigger: &tr, ChannelID: &ch.ID, Filter: &store.RuleFilter{MaxPriority: 2}})
	if err != nil {
		t.Fatal(err)
	}
	ok, err := s.Enqueue(ctx, *r, "k1", "subj", "body", map[string]any{"priority": 1})
	if err != nil || !ok {
		t.Fatal(ok, err)
	}
	ok, _ = s.Enqueue(ctx, *r, "k1", "subj", "body", nil)
	if ok {
		t.Error("duplicate enqueue accepted")
	}
	due, err := s.DueNotifications(ctx)
	if err != nil || len(due) != 1 {
		t.Fatalf("due %v %v", due, err)
	}
	if err := s.MarkNotifications(ctx, []int64{due[0].ID}, nil); err != nil {
		t.Fatal(err)
	}
	due, _ = s.DueNotifications(ctx)
	if len(due) != 0 {
		t.Error("sent notification still due")
	}
	bad := "x"
	if _, err := s.SaveRule(ctx, 0, store.RuleInput{Name: &bad, Trigger: &bad, ChannelID: &ch.ID}); err == nil {
		t.Error("bad trigger accepted")
	}
}

func TestTopologyScene(t *testing.T) {
	s := newStore(t)
	ctx := context.Background()
	ci := source(t, s, "node")
	tags := []string{"prod"}
	if _, err := s.CreateAsset(ctx, store.AssetInput{Kind: "host", Identifier: "node1", Tags: &tags}); err != nil {
		t.Fatal(err)
	}
	if _, err := s.CreateAsset(ctx, store.AssetInput{Kind: "host", Identifier: "spare"}); err != nil {
		t.Fatal(err)
	}
	if _, err := s.CreateAsset(ctx, store.AssetInput{Kind: "repository", Identifier: "github.com/me/app"}); err != nil {
		t.Fatal(err)
	}
	// postgres image with a P2, scanned under its normalised name
	if _, err := s.Ingest(ctx, ci, "node", store.IngestOptions{Asset: "image:postgres"}, report(vuln("CVE-9", "openssl", "1", "2", "high")), nil); err != nil {
		t.Fatal(err)
	}
	// a misconfiguration on the cluster that belongs to web/site, and a leaked secret in the site image
	if _, err := s.Ingest(ctx, ci, "node", store.IngestOptions{Asset: "cluster:prod", ScanTypes: []string{"misconfig"}},
		&ingest.Report{Format: "trivy", ScanTypes: []string{"misconfig"}, Findings: []ingest.Finding{
			{Kind: "misconfig", AdvisoryID: "KSV-0014", Severity: "high", Target: "web/deployment/site: Deployment/site", Title: "ro fs"}}}, nil); err != nil {
		t.Fatal(err)
	}
	if _, err := s.Ingest(ctx, ci, "node", store.IngestOptions{Asset: "image:ghcr.io/me/site", ScanTypes: []string{"secret"}},
		&ingest.Report{Format: "trivy", ScanTypes: []string{"secret"}, Findings: []ingest.Finding{
			{Kind: "secret", AdvisoryID: "aws-key", Severity: "critical", Target: "/app/.env", Title: "AWS key"}}}, nil); err != nil {
		t.Fatal(err)
	}
	if _, err := s.CreateAsset(ctx, store.AssetInput{Kind: "image", Identifier: "ghcr.io/me/idle"}); err != nil {
		t.Fatal(err)
	}
	err := s.SaveTopology(ctx, ci, store.TopologyIn{Cluster: "prod",
		Routes: []store.TopoRoute{{Namespace: "web", Hostnames: []string{"Shop.example.com", "shop.example.com"}}},
		Nodes:  []store.TopoNode{{Name: "n1", Host: "host:node1", CPU: "4", Memory: "8388608Ki", OS: "Ubuntu 24.04", Kubelet: "v1.37.2+k3s1"}},
		Workloads: []store.TopoWorkld{
			{Namespace: "db", Name: "postgres", Kind: "StatefulSet", Node: "n1", Ready: 1, Desired: 1,
				Containers: []store.TopoContainer{{Name: "pg", Image: "docker.io/library/postgres:17.11-alpine"}}},
			{Namespace: "web", Name: "site", Kind: "Deployment", Node: "n1", Ready: 0, Desired: 1,
				Containers: []store.TopoContainer{{Name: "app", Image: "ghcr.io/me/site:1"}, {Name: "app2", Image: "ghcr.io/me/site:1"}}},
			{Namespace: "x", Name: "orphan", Kind: "Pod", Node: "elsewhere"},
		}})
	if err != nil {
		t.Fatal(err)
	}
	if err := s.SaveTopology(ctx, ci, store.TopologyIn{}); err == nil {
		t.Error("empty cluster accepted")
	}
	sc, err := s.Scene(ctx)
	if err != nil {
		t.Fatal(err)
	}
	if len(sc.Hosts) != 2 {
		t.Fatalf("hosts %+v", sc.Hosts)
	}
	h := sc.Hosts[0]
	if h.Name != "node1" || h.Subtitle != "Ubuntu 24.04 · 4 CPU · 8.0 GiB · k3s v1.37.2" || len(h.Groups) != 2 {
		t.Fatalf("host %+v", h)
	}
	db, web := h.Groups[0], h.Groups[1]
	if db.Name != "db" || db.Status.Level != "p2" || db.Workloads[0].Images[0].AssetID == nil || db.Workloads[0].Images[0].Name != "postgres" {
		t.Errorf("db group %+v", db)
	}
	site := web.Workloads[0]
	if site.Status.Level != "p1" || site.Status.Kinds.Secret != 1 || site.Misconfig != 1 || site.Ready != 0 {
		t.Errorf("site workload %+v", site)
	}
	if len(web.Domains) != 1 || web.Domains[0] != "shop.example.com" {
		t.Errorf("domains %v", web.Domains)
	}
	run := map[string]bool{}
	for _, im := range sc.Images {
		run[im.Name] = im.Running
	}
	if !run["site"] || !run["postgres"] || run["idle"] {
		t.Errorf("images running %v", run)
	}
	if len(sc.Sources) != 1 || sc.Sources[0].Name != "node" {
		t.Errorf("sources %+v", sc.Sources)
	}
	if sc.Hosts[1].Name != "spare" || sc.Hosts[1].Subtitle != "no topology reported" {
		t.Errorf("spare host %+v", sc.Hosts[1])
	}
	if len(sc.External) != 1 || sc.External[0].Kind != "repository" {
		t.Errorf("external %+v", sc.External)
	}
	// a new snapshot replaces the old one
	if err := s.SaveTopology(ctx, ci, store.TopologyIn{Cluster: "prod", Nodes: []store.TopoNode{{Name: "n1", Host: "host:node1"}}}); err != nil {
		t.Fatal(err)
	}
	sc, _ = s.Scene(ctx)
	if len(sc.Hosts[0].Groups) != 0 {
		t.Errorf("snapshot not replaced: %+v", sc.Hosts[0].Groups)
	}
}
