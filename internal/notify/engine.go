package notify

import (
	"context"
	"encoding/json"
	"fmt"
	"log/slog"
	"net/http"
	"sort"
	"strings"
	"time"

	"github.com/FlorianWenzel/vengeance/internal/store"
)

// Engine evaluates rules and runs the background jobs.
type Engine struct {
	Store   *store.Store
	Sender  *Sender
	BaseURL string
	KEVURL  string // "" disables KEV sync
	Log     *slog.Logger
}

func (e *Engine) link(path string) string { return strings.TrimRight(e.BaseURL, "/") + path }

func matchFinding(r store.Rule, f store.Finding) bool {
	if f.Priority > r.Filter.MaxPriority {
		return false
	}
	if r.Filter.ProjectID != nil && (f.ProjectID == nil || *f.ProjectID != *r.Filter.ProjectID) {
		return false
	}
	if len(r.Filter.Tags) > 0 && !anyTag(r.Filter.Tags, f.AssetTags) {
		return false
	}
	if r.Filter.FixAvailable != nil && *r.Filter.FixAvailable != (f.FixedVersion != nil) {
		return false
	}
	return true
}

func anyTag(want, have []string) bool {
	for _, w := range want {
		for _, h := range have {
			if w == h {
				return true
			}
		}
	}
	return false
}

func describe(f store.Finding) string {
	s := fmt.Sprintf("P%d %s", f.Priority, f.AdvisoryID)
	if f.PkgName != nil {
		s += " in " + *f.PkgName
		if f.InstalledVersion != nil {
			s += " " + *f.InstalledVersion
		}
		if f.FixedVersion != nil {
			s += " (fixed in " + *f.FixedVersion + ")"
		}
	} else if f.Title != "" {
		s += ": " + f.Title
	}
	s += " on " + f.AssetName
	if f.KEV {
		s += " [known exploited]"
	}
	return s
}

func findingData(f store.Finding) map[string]any {
	b, _ := json.Marshal(f)
	m := map[string]any{}
	_ = json.Unmarshal(b, &m)
	return m
}

// OnFindings queues messages for findings that became new/reopened (trigger new_finding),
// riskier (risk_increase) or whose acceptance expired (accepted_expired).
func (e *Engine) OnFindings(ctx context.Context, trigger string, ids []int64) {
	if len(ids) == 0 {
		return
	}
	rules, err := e.Store.ListRules(ctx, trigger)
	if err != nil || len(rules) == 0 {
		if err != nil {
			e.Log.Error("load rules", "err", err)
		}
		return
	}
	for start := 0; start < len(ids); start += 500 {
		end := min(start+500, len(ids))
		fs, err := e.Store.FindingsByIDs(ctx, ids[start:end])
		if err != nil {
			e.Log.Error("load findings", "err", err)
			return
		}
		for _, r := range rules {
			for _, f := range fs {
				if !matchFinding(r, f) {
					continue
				}
				verb := map[string]string{"new_finding": "New", "risk_increase": "Riskier", "accepted_expired": "Acceptance expired"}[trigger]
				subj := verb + ": " + describe(f)
				key := fmt.Sprintf("%s:%d:%s", trigger, f.ID, f.LastSeen.UTC().Format("2006-01-02"))
				if trigger == "new_finding" {
					key = fmt.Sprintf("%s:%d:%d", trigger, f.ID, f.FirstSeen.Unix())
					// reopened findings get a fresh key via their reopen time
					key += ":" + f.LastSeen.UTC().Format(time.RFC3339)
				}
				data := findingData(f)
				data["priority"] = f.Priority
				if _, err := e.Store.Enqueue(ctx, r, key, subj, describe(f), data); err != nil {
					e.Log.Error("enqueue", "err", err)
				}
			}
		}
	}
}

// Deliver sends due instant notifications, one combined message per rule.
func (e *Engine) Deliver(ctx context.Context) {
	due, err := e.Store.DueNotifications(ctx)
	if err != nil {
		e.Log.Error("load due notifications", "err", err)
		return
	}
	byRule := map[int64][]store.Notification{}
	order := []int64{}
	for _, n := range due {
		if _, ok := byRule[n.RuleID]; !ok {
			order = append(order, n.RuleID)
		}
		byRule[n.RuleID] = append(byRule[n.RuleID], n)
	}
	for _, rid := range order {
		e.sendBatch(ctx, byRule[rid], false)
	}
}

func (e *Engine) sendBatch(ctx context.Context, ns []store.Notification, digest bool) {
	if len(ns) == 0 {
		return
	}
	ch, err := e.Store.GetChannel(ctx, ns[0].ChannelID)
	ids := make([]int64, len(ns))
	for i, n := range ns {
		ids[i] = n.ID
	}
	if err != nil {
		_ = e.Store.MarkNotifications(ctx, ids, fmt.Errorf("channel: %w", err))
		return
	}
	m := Compose(ns, digest, e.BaseURL)
	err = e.Sender.Send(ctx, *ch, m)
	if err != nil {
		e.Log.Warn("notification failed", "channel", ch.Name, "err", err)
	}
	if merr := e.Store.MarkNotifications(ctx, ids, err); merr != nil {
		e.Log.Error("mark notifications", "err", merr)
	}
}

// Compose turns queued parts into one message.
func Compose(ns []store.Notification, digest bool, baseURL string) Message {
	m := Message{Link: strings.TrimRight(baseURL, "/") + "/findings", Event: "batch"}
	prio := 4
	for _, n := range ns {
		if p, ok := n.Data["priority"].(float64); ok && int(p) < prio {
			prio = int(p)
		}
		m.Items = append(m.Items, n.Data)
	}
	m.Priority = prio
	sort.SliceStable(ns, func(i, j int) bool { return dataPrio(ns[i]) < dataPrio(ns[j]) })
	if len(ns) == 1 && !digest {
		m.Subject = "[Vengeance] " + ns[0].Subject
		m.Body = ns[0].Body
		m.Event = ns[0].RuleName
		if id, ok := ns[0].Data["id"].(float64); ok {
			m.Link = fmt.Sprintf("%s/findings?id=%d", strings.TrimRight(baseURL, "/"), int64(id))
		}
		return m
	}
	kind := "updates"
	if digest {
		kind = "digest"
	}
	m.Subject = fmt.Sprintf("[Vengeance] %d %s from %s", len(ns), kind, ns[0].RuleName)
	var b strings.Builder
	for i, n := range ns {
		if i == 40 {
			fmt.Fprintf(&b, "…and %d more\n", len(ns)-40)
			break
		}
		b.WriteString("- " + n.Subject + "\n")
	}
	m.Body = b.String()
	return m
}

func dataPrio(n store.Notification) int {
	if p, ok := n.Data["priority"].(float64); ok {
		return int(p)
	}
	return 5
}

// Test sends a test message to a channel.
func (e *Engine) Test(ctx context.Context, ch store.Channel) error {
	return e.Sender.Send(ctx, ch, Message{
		Subject: "[Vengeance] Test notification", Event: "test", Priority: 3, Link: e.link("/notifications"),
		Body: "This channel works. Real alerts will look like this one.",
	})
}

// HTTPClient is used for KEV downloads; overridable in tests.
var HTTPClient = &http.Client{Timeout: 60 * time.Second}
