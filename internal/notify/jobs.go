package notify

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"strconv"
	"time"

	"github.com/FlorianWenzel/vengeance/internal/store"
)

// Run starts the background loops until ctx ends.
func (e *Engine) Run(ctx context.Context) {
	tick := time.NewTicker(15 * time.Second)
	defer tick.Stop()
	var lastHourly time.Time
	e.hourly(ctx)
	lastHourly = time.Now()
	for {
		select {
		case <-ctx.Done():
			return
		case <-tick.C:
			e.Deliver(ctx)
			e.Digests(ctx)
			if time.Since(lastHourly) > time.Hour {
				e.hourly(ctx)
				lastHourly = time.Now()
			}
		}
	}
}

func (e *Engine) hourly(ctx context.Context) {
	if e.KEVURL != "" {
		last, _ := time.Parse(time.RFC3339, e.Store.KVGet(ctx, "kev:synced"))
		if time.Since(last) > 12*time.Hour {
			if err := e.SyncKEV(ctx); err != nil {
				e.Log.Warn("kev sync failed", "err", err)
			}
		}
	}
	if ids, err := e.Store.ExpireAcceptances(ctx); err != nil {
		e.Log.Error("expire acceptances", "err", err)
	} else {
		e.OnFindings(ctx, "accepted_expired", ids)
	}
	e.StaleSources(ctx)
	e.CoverageGaps(ctx)
	_ = e.Store.PruneSessions(ctx)
	_ = e.Store.PruneRaw(ctx, 30*24*time.Hour)
}

// SyncKEV downloads the CISA catalogue and raises priorities.
func (e *Engine) SyncKEV(ctx context.Context) error {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, e.KEVURL, nil)
	if err != nil {
		return err
	}
	req.Header.Set("User-Agent", "Vengeance (+https://github.com/FlorianWenzel/vengeance)")
	resp, err := HTTPClient.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()
	if resp.StatusCode != 200 {
		return fmt.Errorf("KEV feed answered %d", resp.StatusCode)
	}
	var feed struct {
		Vulnerabilities []struct {
			CveID     string `json:"cveID"`
			DateAdded string `json:"dateAdded"`
		} `json:"vulnerabilities"`
	}
	if err := json.NewDecoder(io.LimitReader(resp.Body, 64<<20)).Decode(&feed); err != nil {
		return fmt.Errorf("KEV feed: %w", err)
	}
	if len(feed.Vulnerabilities) < 100 {
		return fmt.Errorf("KEV feed looks truncated (%d entries)", len(feed.Vulnerabilities))
	}
	entries := make([]store.KEVEntry, 0, len(feed.Vulnerabilities))
	for _, v := range feed.Vulnerabilities {
		d, err := time.Parse("2006-01-02", v.DateAdded)
		if err != nil {
			continue
		}
		entries = append(entries, store.KEVEntry{CVE: v.CveID, DateAdded: d})
	}
	ids, err := e.Store.SyncKEV(ctx, entries)
	if err != nil {
		return err
	}
	_ = e.Store.KVSet(ctx, "kev:synced", time.Now().UTC().Format(time.RFC3339))
	_ = e.Store.KVSet(ctx, "kev:count", strconv.Itoa(len(entries)))
	e.Log.Info("kev synced", "entries", len(entries), "newly_exploited_findings", len(ids))
	e.OnFindings(ctx, "risk_increase", ids)
	return nil
}

// StaleSources notifies once per silence period for each quiet source.
func (e *Engine) StaleSources(ctx context.Context) {
	rules, err := e.Store.ListRules(ctx, "stale_source")
	if err != nil || len(rules) == 0 {
		return
	}
	srcs, err := e.Store.ListSources(ctx)
	if err != nil {
		return
	}
	for _, s := range srcs {
		if !s.Stale {
			continue
		}
		last := "never"
		ref := s.CreatedAt
		if s.LastSeenAt != nil {
			ref = *s.LastSeenAt
			last = ref.UTC().Format("2006-01-02 15:04 UTC")
		}
		subj := fmt.Sprintf("Source %q went quiet", s.Name)
		body := fmt.Sprintf("%s has not reported for more than %d hours (last report: %s). The scan job may have stopped.", s.Name, s.StaleAfterHours, last)
		for _, r := range rules {
			key := fmt.Sprintf("stale:%d:%d", s.ID, ref.Unix())
			_, _ = e.Store.Enqueue(ctx, r, key, subj, body, map[string]any{"source": s.Name, "lastSeenAt": s.LastSeenAt, "priority": 2})
		}
	}
}

// CoverageGaps notifies once per (asset, scan type, last scan) gap.
func (e *Engine) CoverageGaps(ctx context.Context) {
	rules, err := e.Store.ListRules(ctx, "coverage_gap")
	if err != nil || len(rules) == 0 {
		return
	}
	cov, err := e.Store.Coverage(ctx)
	if err != nil {
		return
	}
	for _, g := range cov.Gaps {
		last := "never"
		var ref int64
		if g.LastAt != nil {
			last = g.LastAt.UTC().Format("2006-01-02")
			ref = g.LastAt.Unix()
		}
		subj := fmt.Sprintf("Coverage gap: %s %s has no %s scan", g.AssetKind, g.AssetName, g.ScanType)
		body := fmt.Sprintf("%s (%s) needs a %s scan every %d days. Last one: %s.", g.AssetName, g.AssetKind, g.ScanType, g.MaxAgeDays, last)
		for _, r := range rules {
			if r.Filter.ProjectID != nil && (g.ProjectID == nil || *g.ProjectID != *r.Filter.ProjectID) {
				continue
			}
			key := fmt.Sprintf("gap:%d:%s:%d", g.AssetID, g.ScanType, ref)
			_, _ = e.Store.Enqueue(ctx, r, key, subj, body, map[string]any{"assetId": g.AssetID, "scanType": g.ScanType, "priority": 3})
		}
	}
}

// Digests sends daily (08:00 UTC) and weekly (Monday 08:00 UTC) digests.
func (e *Engine) Digests(ctx context.Context) {
	rules, err := e.Store.ListRules(ctx, "")
	if err != nil {
		return
	}
	now := e.Store.Now().UTC()
	for _, r := range rules {
		if r.Mode == "instant" || !r.Enabled {
			continue
		}
		slot := DigestSlot(r.Mode, now)
		key := "digest:" + strconv.FormatInt(r.ID, 10)
		if e.Store.KVGet(ctx, key) == slot.Format(time.RFC3339) || now.Before(slot) {
			continue
		}
		ns, err := e.Store.DigestNotifications(ctx, r.ID)
		if err != nil {
			continue
		}
		e.sendBatch(ctx, ns, true)
		_ = e.Store.KVSet(ctx, key, slot.Format(time.RFC3339))
	}
}

// DigestSlot returns the most recent digest time at or before now.
func DigestSlot(mode string, now time.Time) time.Time {
	now = now.UTC()
	slot := time.Date(now.Year(), now.Month(), now.Day(), 8, 0, 0, 0, time.UTC)
	if now.Before(slot) {
		slot = slot.AddDate(0, 0, -1)
	}
	if mode == "weekly" {
		for slot.Weekday() != time.Monday {
			slot = slot.AddDate(0, 0, -1)
		}
	}
	return slot
}
