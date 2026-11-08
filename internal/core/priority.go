// Package core holds the pure rules: priority, fix versions and upgrade commands.
package core

import (
	"slices"
	"strings"
)

// PriorityInput is what priority depends on.
type PriorityInput struct {
	Kind       string
	Severity   string
	HasFix     bool
	KEV        bool
	AssetTags  []string
}

// Priority returns 1 (most urgent) to 4. See docs/api.md "Prioritisation".
func Priority(p PriorityInput) int {
	sev := strings.ToLower(p.Severity)
	switch p.Kind {
	case "secret":
		if sev == "critical" || sev == "high" {
			return 1
		}
		return 3
	case "misconfig", "sast":
		if sev == "critical" || sev == "high" {
			return 2
		}
		return 3
	}
	if p.KEV {
		return 1
	}
	if !p.HasFix {
		return 4
	}
	if sev == "critical" && slices.Contains(p.AssetTags, "internet-facing") {
		return 1
	}
	if sev == "critical" || sev == "high" {
		return 2
	}
	return 3
}
