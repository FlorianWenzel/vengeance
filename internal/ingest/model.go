// Package ingest turns scanner reports into a format-independent list of findings.
package ingest

import (
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"sort"
	"strings"
	"time"
)

// Finding is one normalised result from a scanner report.
type Finding struct {
	Kind        string // vuln, secret, misconfig, sast
	AdvisoryID  string
	Title       string
	Description string
	Severity    string // critical, high, medium, low, unknown
	CVSS        *float64
	PkgName     string
	Ecosystem   string
	Installed   string
	Fixed       string
	Target      string
	// FingerprintTarget replaces Target in the fingerprint when set, so that cosmetic
	// target changes (an OS version in an image target name) keep the same finding.
	FingerprintTarget string
	References        []string
	Published         *time.Time
}

// Fingerprint identifies a finding on one asset across scans and scanners. The installed
// version is deliberately excluded: a bump that is still vulnerable stays the same finding.
func (f Finding) Fingerprint() string {
	t := f.Target
	if f.FingerprintTarget != "" {
		t = f.FingerprintTarget
	}
	h := sha256.Sum256([]byte(strings.Join([]string{f.Kind, f.AdvisoryID, f.Ecosystem, f.PkgName, t}, "\x1f")))
	return hex.EncodeToString(h[:16])
}

// Report is a parsed scanner report.
type Report struct {
	Format       string
	Tool         string
	ScanTypes    []string
	ArtifactName string
	ArtifactType string // container_image, filesystem, repository, vm, cluster
	Digest       string
	Findings     []Finding
}

// Parse dispatches on format.
func Parse(format string, body []byte) (*Report, error) {
	switch strings.ToLower(format) {
	case "trivy":
		return ParseTrivy(body)
	case "sarif":
		return ParseSARIF(body)
	default:
		return nil, fmt.Errorf("unknown format %q (use trivy or sarif)", format)
	}
}

// Dedupe merges findings with the same fingerprint, keeping the richest data.
func Dedupe(in []Finding) []Finding {
	idx := map[string]int{}
	out := make([]Finding, 0, len(in))
	for _, f := range in {
		fp := f.Fingerprint()
		if i, ok := idx[fp]; ok {
			cur := &out[i]
			if SeverityRank(f.Severity) > SeverityRank(cur.Severity) {
				cur.Severity = f.Severity
			}
			if cur.Fixed == "" {
				cur.Fixed = f.Fixed
			}
			if cur.Description == "" {
				cur.Description = f.Description
			}
			continue
		}
		idx[fp] = len(out)
		out = append(out, f)
	}
	return out
}

// SeverityRank orders severities; higher is worse.
func SeverityRank(s string) int {
	switch strings.ToLower(s) {
	case "critical":
		return 4
	case "high":
		return 3
	case "medium":
		return 2
	case "low":
		return 1
	}
	return 0
}

// NormSeverity maps scanner spellings onto our five levels.
func NormSeverity(s string) string {
	switch strings.ToLower(strings.TrimSpace(s)) {
	case "critical":
		return "critical"
	case "high", "error":
		return "high"
	case "medium", "moderate", "warning":
		return "medium"
	case "low", "note", "negligible", "info":
		return "low"
	}
	return "unknown"
}

// SeverityFromScore maps a CVSS-style 0-10 score.
func SeverityFromScore(v float64) string {
	switch {
	case v >= 9:
		return "critical"
	case v >= 7:
		return "high"
	case v >= 4:
		return "medium"
	case v > 0:
		return "low"
	}
	return "unknown"
}

func uniqSorted(in []string) []string {
	m := map[string]bool{}
	for _, s := range in {
		if s != "" {
			m[s] = true
		}
	}
	out := make([]string, 0, len(m))
	for s := range m {
		out = append(out, s)
	}
	sort.Strings(out)
	return out
}
