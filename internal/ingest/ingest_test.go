package ingest

import (
	"os"
	"testing"
)

func load(t *testing.T, name string) []byte {
	t.Helper()
	b, err := os.ReadFile("testdata/" + name)
	if err != nil {
		t.Fatal(err)
	}
	return b
}

func count(fs []Finding, kind string) int {
	n := 0
	for _, f := range fs {
		if f.Kind == kind {
			n++
		}
	}
	return n
}

func TestTrivyImage(t *testing.T) {
	r, err := ParseTrivy(load(t, "image.json"))
	if err != nil {
		t.Fatal(err)
	}
	if r.ArtifactType != "container_image" || r.ArtifactName != "python:3.9-alpine3.18" {
		t.Errorf("artifact %q %q", r.ArtifactType, r.ArtifactName)
	}
	if r.Digest == "" || r.Digest[:7] != "sha256:" {
		t.Errorf("digest %q", r.Digest)
	}
	if len(r.ScanTypes) != 1 || r.ScanTypes[0] != "vuln" {
		t.Errorf("scan types %v", r.ScanTypes)
	}
	if count(r.Findings, "vuln") < 40 {
		t.Errorf("only %d vulns", count(r.Findings, "vuln"))
	}
	var os *Finding
	for i, f := range r.Findings {
		if f.Ecosystem == "alpine" {
			os = &r.Findings[i]
			break
		}
	}
	if os == nil || os.FingerprintTarget != "os:alpine" {
		t.Fatalf("os finding missing or fingerprint target wrong: %+v", os)
	}
	for _, f := range r.Findings {
		if f.Severity == "" || f.AdvisoryID == "" || f.PkgName == "" {
			t.Fatalf("incomplete finding %+v", f)
		}
	}
}

func TestTrivyFS(t *testing.T) {
	r, err := ParseTrivy(load(t, "fs.json"))
	if err != nil {
		t.Fatal(err)
	}
	want := []string{"misconfig", "secret", "vuln"}
	if len(r.ScanTypes) != 3 {
		t.Fatalf("scan types %v want %v", r.ScanTypes, want)
	}
	for i := range want {
		if r.ScanTypes[i] != want[i] {
			t.Errorf("scan types %v want %v", r.ScanTypes, want)
		}
	}
	if count(r.Findings, "secret") != 1 || count(r.Findings, "misconfig") == 0 || count(r.Findings, "vuln") != 12 {
		t.Errorf("counts secret=%d misconfig=%d vuln=%d", count(r.Findings, "secret"), count(r.Findings, "misconfig"), count(r.Findings, "vuln"))
	}
}

func TestFingerprintStableAcrossVersions(t *testing.T) {
	a := Finding{Kind: "vuln", AdvisoryID: "CVE-1", Ecosystem: "alpine", PkgName: "openssl", Installed: "3.1.0", Target: "img (alpine 3.18.6)", FingerprintTarget: "os:alpine"}
	b := a
	b.Installed = "3.1.4"
	b.Target = "img (alpine 3.18.9)"
	if a.Fingerprint() != b.Fingerprint() {
		t.Error("fingerprint changed with version/target")
	}
	c := a
	c.AdvisoryID = "CVE-2"
	if a.Fingerprint() == c.Fingerprint() {
		t.Error("different advisories share a fingerprint")
	}
}

func TestSARIFFromTrivy(t *testing.T) {
	r, err := ParseSARIF(load(t, "image.sarif"))
	if err != nil {
		t.Fatal(err)
	}
	img, _ := ParseTrivy(load(t, "image.json"))
	if count(r.Findings, "vuln") == 0 {
		t.Fatal("no vulns from SARIF")
	}
	withPkg := 0
	for _, f := range r.Findings {
		if f.PkgName != "" && f.Installed != "" {
			withPkg++
		}
	}
	if withPkg == 0 {
		t.Error("package details not parsed from SARIF messages")
	}
	if r.Tool != "Trivy" {
		t.Errorf("tool %q", r.Tool)
	}
	_ = img
}

func TestSARIFGeneric(t *testing.T) {
	body := []byte(`{"version":"2.1.0","runs":[{"tool":{"driver":{"name":"Semgrep","rules":[
	 {"id":"go.lang.security.sqli","shortDescription":{"text":"SQL injection"},"properties":{"security-severity":"8.1"}}]}},
	 "results":[{"ruleId":"go.lang.security.sqli","level":"error","message":{"text":"tainted"},
	 "locations":[{"physicalLocation":{"artifactLocation":{"uri":"internal/db.go"}}}]}]}]}`)
	r, err := ParseSARIF(body)
	if err != nil {
		t.Fatal(err)
	}
	if len(r.Findings) != 1 || r.Findings[0].Kind != "sast" || r.Findings[0].Severity != "high" || r.Findings[0].Target != "internal/db.go" {
		t.Fatalf("%+v", r.Findings)
	}
}

func TestTrivyK8s(t *testing.T) {
	body := []byte(`{"ClusterName":"prod","Resources":[{"Namespace":"web","Kind":"Deployment","Name":"api","Results":[
	 {"Target":"ghcr.io/x/api:1 (alpine 3.20)","Class":"os-pkgs","Type":"alpine","Vulnerabilities":[
	  {"VulnerabilityID":"CVE-2026-1","PkgName":"musl","InstalledVersion":"1.2.4-r0","FixedVersion":"1.2.4-r1","Severity":"HIGH"}]},
	 {"Target":"Deployment/api","Class":"config","Type":"kubernetes","Misconfigurations":[
	  {"ID":"KSV001","AVDID":"AVD-KSV-0001","Title":"Privilege escalation","Severity":"MEDIUM","Status":"FAIL"},
	  {"ID":"KSV002","AVDID":"AVD-KSV-0002","Title":"ok","Severity":"LOW","Status":"PASS"}]}]}]}`)
	r, err := ParseTrivy(body)
	if err != nil {
		t.Fatal(err)
	}
	if r.ArtifactType != "cluster" || r.ArtifactName != "prod" {
		t.Errorf("%q %q", r.ArtifactType, r.ArtifactName)
	}
	if len(r.Findings) != 2 {
		t.Fatalf("want 2 findings (PASS skipped), got %d", len(r.Findings))
	}
	if r.Findings[0].Target != "web/deployment/api: ghcr.io/x/api:1 (alpine 3.20)" {
		t.Errorf("target %q", r.Findings[0].Target)
	}
}

func TestRejectGarbage(t *testing.T) {
	if _, err := ParseTrivy([]byte(`{"hello":1}`)); err == nil {
		t.Error("accepted non-trivy JSON")
	}
	if _, err := ParseSARIF([]byte(`{"x":1}`)); err == nil {
		t.Error("accepted non-sarif JSON")
	}
	if _, err := Parse("nessus", nil); err == nil {
		t.Error("accepted unknown format")
	}
}
