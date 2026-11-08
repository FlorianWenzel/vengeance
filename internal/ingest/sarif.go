package ingest

import (
	"encoding/json"
	"fmt"
	"regexp"
	"strconv"
	"strings"
)

type sarifLog struct {
	Version string     `json:"version"`
	Runs    []sarifRun `json:"runs"`
}

type sarifRun struct {
	Tool struct {
		Driver struct {
			Name  string      `json:"name"`
			Rules []sarifRule `json:"rules"`
		} `json:"driver"`
	} `json:"tool"`
	Results []sarifResult `json:"results"`
}

type sarifText struct {
	Text string `json:"text"`
}

type sarifRule struct {
	ID               string         `json:"id"`
	Name             string         `json:"name"`
	ShortDescription sarifText      `json:"shortDescription"`
	FullDescription  sarifText      `json:"fullDescription"`
	Help             sarifText      `json:"help"`
	HelpURI          string         `json:"helpUri"`
	Properties       map[string]any `json:"properties"`
	DefaultConfig    struct {
		Level string `json:"level"`
	} `json:"defaultConfiguration"`
}

type sarifResult struct {
	RuleID    string    `json:"ruleId"`
	RuleIndex *int      `json:"ruleIndex"`
	Level     string    `json:"level"`
	Message   sarifText `json:"message"`
	Locations []struct {
		PhysicalLocation struct {
			ArtifactLocation struct {
				URI string `json:"uri"`
			} `json:"artifactLocation"`
		} `json:"physicalLocation"`
	} `json:"locations"`
}

var (
	advisoryRe  = regexp.MustCompile(`^(CVE-\d{4}-\d+|GHSA-[\w-]+|GO-\d{4}-\d+|PYSEC-\d{4}-\d+|RUSTSEC-\d{4}-\d+|OSV-[\w-]+)$`)
	pkgRe       = regexp.MustCompile(`(?m)Package:\s*(\S+)`)
	installedRe = regexp.MustCompile(`(?m)Installed Version:\s*(\S+)`)
	fixedRe     = regexp.MustCompile(`(?m)Fixed Version:\s*([^\n]*)`)
)

// ParseSARIF reads SARIF 2.1.0 logs from any tool. Results whose rule id looks like an
// advisory (CVE, GHSA, ...) become vulnerability findings, the rest static-analysis findings.
func ParseSARIF(body []byte) (*Report, error) {
	var log sarifLog
	if err := json.Unmarshal(body, &log); err != nil {
		return nil, fmt.Errorf("not a SARIF log: %w", err)
	}
	if log.Runs == nil {
		return nil, fmt.Errorf("not a SARIF log: no runs")
	}
	r := &Report{Format: "sarif"}
	types := []string{}
	tools := []string{}
	for _, run := range log.Runs {
		tool := run.Tool.Driver.Name
		tools = append(tools, tool)
		rules := map[string]sarifRule{}
		for _, ru := range run.Tool.Driver.Rules {
			rules[ru.ID] = ru
		}
		isTrivy := strings.EqualFold(tool, "trivy")
		if len(run.Results) == 0 {
			if isTrivy {
				types = append(types, "vuln")
			} else {
				types = append(types, "sast")
			}
		}
		for _, res := range run.Results {
			ru, ok := rules[res.RuleID]
			if !ok && res.RuleIndex != nil && *res.RuleIndex < len(run.Tool.Driver.Rules) {
				ru = run.Tool.Driver.Rules[*res.RuleIndex]
			}
			id := res.RuleID
			if id == "" {
				id = ru.ID
			}
			target := ""
			if len(res.Locations) > 0 {
				target = res.Locations[0].PhysicalLocation.ArtifactLocation.URI
			}
			f := Finding{AdvisoryID: id, Target: target, Title: firstNonEmpty(ru.ShortDescription.Text, ru.Name, id)}
			f.Description = firstNonEmpty(ru.FullDescription.Text, ru.Help.Text, res.Message.Text)
			if ru.HelpURI != "" {
				f.References = []string{ru.HelpURI}
			}
			f.Severity = sarifSeverity(res, ru)
			if s, ok := scoreProp(ru.Properties); ok {
				f.CVSS = &s
			}
			if advisoryRe.MatchString(id) {
				f.Kind = "vuln"
				msg := res.Message.Text
				if m := pkgRe.FindStringSubmatch(msg); m != nil {
					f.PkgName = m[1]
				}
				if m := installedRe.FindStringSubmatch(msg); m != nil {
					f.Installed = m[1]
				}
				if m := fixedRe.FindStringSubmatch(msg); m != nil {
					f.Fixed = strings.TrimSpace(m[1])
				}
			} else if isTrivy && hasTag(ru.Properties, "secret") {
				f.Kind = "secret"
			} else if isTrivy && hasTag(ru.Properties, "misconfiguration") {
				f.Kind = "misconfig"
			} else {
				f.Kind = "sast"
			}
			types = append(types, f.Kind)
			r.Findings = append(r.Findings, f)
		}
	}
	r.Tool = strings.Join(uniqSorted(tools), ", ")
	r.ScanTypes = uniqSorted(types)
	r.Findings = Dedupe(r.Findings)
	return r, nil
}

func sarifSeverity(res sarifResult, ru sarifRule) string {
	if s, ok := scoreProp(ru.Properties); ok {
		return SeverityFromScore(s)
	}
	level := res.Level
	if level == "" {
		level = ru.DefaultConfig.Level
	}
	return NormSeverity(level)
}

func scoreProp(p map[string]any) (float64, bool) {
	switch v := p["security-severity"].(type) {
	case string:
		f, err := strconv.ParseFloat(v, 64)
		return f, err == nil && f > 0
	case float64:
		return v, v > 0
	}
	return 0, false
}

func hasTag(p map[string]any, tag string) bool {
	tags, _ := p["tags"].([]any)
	for _, t := range tags {
		if s, ok := t.(string); ok && strings.EqualFold(s, tag) {
			return true
		}
	}
	return false
}

func firstNonEmpty(s ...string) string {
	for _, v := range s {
		if strings.TrimSpace(v) != "" {
			return strings.TrimSpace(v)
		}
	}
	return ""
}
