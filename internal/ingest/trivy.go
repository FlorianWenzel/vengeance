package ingest

import (
	"encoding/json"
	"fmt"
	"strings"
	"time"
)

type trivyReport struct {
	ArtifactName string        `json:"ArtifactName"`
	ArtifactType string        `json:"ArtifactType"`
	Metadata     trivyMetadata `json:"Metadata"`
	Results      []trivyResult `json:"Results"`
	// trivy k8s
	ClusterName string          `json:"ClusterName"`
	Resources   []trivyResource `json:"Resources"`
}

type trivyMetadata struct {
	RepoDigests []string `json:"RepoDigests"`
	ImageID     string   `json:"ImageID"`
}

type trivyResource struct {
	Namespace string        `json:"Namespace"`
	Kind      string        `json:"Kind"`
	Name      string        `json:"Name"`
	Results   []trivyResult `json:"Results"`
}

type trivyResult struct {
	Target            string         `json:"Target"`
	Class             string         `json:"Class"`
	Type              string         `json:"Type"`
	Vulnerabilities   []trivyVuln    `json:"Vulnerabilities"`
	Secrets           []trivySecret  `json:"Secrets"`
	Misconfigurations []trivyMisconf `json:"Misconfigurations"`
}

type trivyVuln struct {
	VulnerabilityID  string                    `json:"VulnerabilityID"`
	PkgName          string                    `json:"PkgName"`
	InstalledVersion string                    `json:"InstalledVersion"`
	FixedVersion     string                    `json:"FixedVersion"`
	SeveritySource   string                    `json:"SeveritySource"`
	PrimaryURL       string                    `json:"PrimaryURL"`
	Title            string                    `json:"Title"`
	Description      string                    `json:"Description"`
	Severity         string                    `json:"Severity"`
	CVSS             map[string]map[string]any `json:"CVSS"`
	References       []string                  `json:"References"`
	PublishedDate    *time.Time                `json:"PublishedDate"`
}

type trivySecret struct {
	RuleID    string `json:"RuleID"`
	Category  string `json:"Category"`
	Severity  string `json:"Severity"`
	Title     string `json:"Title"`
	StartLine int    `json:"StartLine"`
}

type trivyMisconf struct {
	ID          string   `json:"ID"`
	AVDID       string   `json:"AVDID"`
	Title       string   `json:"Title"`
	Description string   `json:"Description"`
	Message     string   `json:"Message"`
	Resolution  string   `json:"Resolution"`
	Severity    string   `json:"Severity"`
	PrimaryURL  string   `json:"PrimaryURL"`
	References  []string `json:"References"`
	Status      string   `json:"Status"`
}

// ParseTrivy reads `trivy <image|fs|repo|rootfs|k8s> -f json` output.
func ParseTrivy(body []byte) (*Report, error) {
	var tr trivyReport
	if err := json.Unmarshal(body, &tr); err != nil {
		return nil, fmt.Errorf("not a Trivy JSON report: %w", err)
	}
	if tr.ArtifactName == "" && tr.ClusterName == "" && tr.Results == nil && tr.Resources == nil {
		return nil, fmt.Errorf("not a Trivy JSON report: no ArtifactName, ClusterName or Results")
	}
	r := &Report{Format: "trivy", Tool: "Trivy", ArtifactName: tr.ArtifactName, ArtifactType: tr.ArtifactType}
	if len(tr.Metadata.RepoDigests) > 0 {
		if i := strings.LastIndex(tr.Metadata.RepoDigests[0], "@"); i >= 0 {
			r.Digest = tr.Metadata.RepoDigests[0][i+1:]
		}
	}
	types := []string{}
	add := func(prefix string, results []trivyResult) {
		for _, res := range results {
			switch res.Class {
			case "os-pkgs", "lang-pkgs":
				types = append(types, "vuln")
			case "secret":
				types = append(types, "secret")
			case "config":
				types = append(types, "misconfig")
			}
			target := res.Target
			if prefix != "" {
				target = prefix + ": " + res.Target
			}
			fpTarget := ""
			if res.Class == "os-pkgs" {
				// "registry/x:tag (alpine 3.20.3)" changes with every base image bump.
				fpTarget = prefix + "os:" + res.Type
			}
			for _, v := range res.Vulnerabilities {
				f := Finding{
					Kind: "vuln", AdvisoryID: v.VulnerabilityID, Title: v.Title, Description: v.Description,
					Severity: NormSeverity(v.Severity), PkgName: v.PkgName, Ecosystem: res.Type,
					Installed: v.InstalledVersion, Fixed: v.FixedVersion, Target: target, FingerprintTarget: fpTarget,
					References: v.References, Published: v.PublishedDate, CVSS: trivyScore(v),
				}
				if f.Title == "" {
					f.Title = v.VulnerabilityID + " in " + v.PkgName
				}
				if v.PrimaryURL != "" {
					f.References = append([]string{v.PrimaryURL}, f.References...)
				}
				r.Findings = append(r.Findings, f)
			}
			for _, s := range res.Secrets {
				r.Findings = append(r.Findings, Finding{
					Kind: "secret", AdvisoryID: s.RuleID, Title: s.Title, Severity: NormSeverity(s.Severity),
					Target: target, Description: fmt.Sprintf("%s secret found in %s at line %d.", s.Category, target, s.StartLine),
				})
			}
			for _, m := range res.Misconfigurations {
				if m.Status != "" && m.Status != "FAIL" {
					continue
				}
				id := m.AVDID
				if id == "" {
					id = m.ID
				}
				desc := strings.TrimSpace(m.Description + "\n\n" + m.Message + "\n\nResolution: " + m.Resolution)
				refs := m.References
				if m.PrimaryURL != "" {
					refs = append([]string{m.PrimaryURL}, refs...)
				}
				r.Findings = append(r.Findings, Finding{
					Kind: "misconfig", AdvisoryID: id, Title: m.Title, Description: desc,
					Severity: NormSeverity(m.Severity), Target: target, References: refs,
				})
			}
		}
	}
	add("", tr.Results)
	if tr.Resources != nil {
		r.ArtifactType = "cluster"
		if r.ArtifactName == "" {
			r.ArtifactName = tr.ClusterName
		}
		for _, res := range tr.Resources {
			add(fmt.Sprintf("%s/%s/%s", res.Namespace, strings.ToLower(res.Kind), res.Name), res.Results)
		}
	}
	if len(types) == 0 {
		types = []string{"vuln"} // a clean vulnerability scan has no classes to infer from
	}
	r.ScanTypes = uniqSorted(types)
	r.Findings = Dedupe(r.Findings)
	return r, nil
}

func trivyScore(v trivyVuln) *float64 {
	pick := func(src string) *float64 {
		if m, ok := v.CVSS[src]; ok {
			if s, ok := m["V3Score"].(float64); ok && s > 0 {
				return &s
			}
			if s, ok := m["V40Score"].(float64); ok && s > 0 {
				return &s
			}
		}
		return nil
	}
	if s := pick(v.SeveritySource); s != nil {
		return s
	}
	for _, src := range []string{"nvd", "ghsa", "redhat"} {
		if s := pick(src); s != nil {
			return s
		}
	}
	return nil
}
