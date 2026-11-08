package store

import (
	"context"
	"fmt"
	"sort"
	"time"

	"github.com/FlorianWenzel/vengeance/internal/core"
)

// FixGroup is one upgrade that resolves one or more findings.
type FixGroup struct {
	Key              string   `json:"key"`
	AssetID          int64    `json:"assetId"`
	AssetName        string   `json:"assetName"`
	PkgName          string   `json:"pkgName"`
	Ecosystem        string   `json:"ecosystem"`
	InstalledVersion string   `json:"installedVersion"`
	TargetVersion    string   `json:"targetVersion"`
	Command          string   `json:"command"`
	Findings         int      `json:"findings"`
	MaxPriority      int      `json:"maxPriority"`
	KEV              bool     `json:"kev"`
	Advisories       []string `json:"advisories"`
}

// Fixes groups open vulnerability findings with a fix by asset and package.
func (s *Store) Fixes(ctx context.Context, projectID int64) ([]FixGroup, error) {
	q := `SELECT f.asset_id, a.name, f.pkg_name, COALESCE(f.pkg_ecosystem,''), COALESCE(f.installed_version,''), f.fixed_version,
		f.priority, COALESCE(ad.kev,false), f.advisory_id
		FROM findings f JOIN assets a ON a.id=f.asset_id LEFT JOIN advisories ad ON ad.id=f.advisory_id
		WHERE f.state = ANY($1) AND f.kind='vuln' AND f.fixed_version IS NOT NULL AND f.pkg_name IS NOT NULL AND a.retired_at IS NULL`
	args := []any{OpenStates}
	if projectID > 0 {
		q += ` AND a.project_id=$2`
		args = append(args, projectID)
	}
	rows, err := s.DB.Query(ctx, q, args...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	groups := map[string]*FixGroup{}
	for rows.Next() {
		var assetID int64
		var assetName, pkg, eco, inst, fixed, adv string
		var prio int
		var kev bool
		if err := rows.Scan(&assetID, &assetName, &pkg, &eco, &inst, &fixed, &prio, &kev, &adv); err != nil {
			return nil, err
		}
		key := fmt.Sprintf("%d|%s|%s|%s", assetID, eco, pkg, inst)
		g := groups[key]
		if g == nil {
			g = &FixGroup{Key: key, AssetID: assetID, AssetName: assetName, PkgName: pkg, Ecosystem: eco, InstalledVersion: inst, MaxPriority: 4}
			groups[key] = g
		}
		target := core.PickFix(inst, fixed)
		if target == "" {
			continue
		}
		g.TargetVersion = core.MaxVersion(g.TargetVersion, target)
		g.Findings++
		if prio < g.MaxPriority {
			g.MaxPriority = prio
		}
		g.KEV = g.KEV || kev
		g.Advisories = append(g.Advisories, adv)
	}
	if err := rows.Err(); err != nil {
		return nil, err
	}
	out := make([]FixGroup, 0, len(groups))
	for _, g := range groups {
		if g.Findings == 0 {
			continue
		}
		sort.Strings(g.Advisories)
		g.Command = core.Command(g.Ecosystem, g.PkgName, g.TargetVersion)
		out = append(out, *g)
	}
	sort.Slice(out, func(i, j int) bool {
		a, b := out[i], out[j]
		if a.MaxPriority != b.MaxPriority {
			return a.MaxPriority < b.MaxPriority
		}
		if a.Findings != b.Findings {
			return a.Findings > b.Findings
		}
		return a.Key < b.Key
	})
	return out, nil
}

// TrendPoint is open findings on a day.
type TrendPoint struct {
	Date string `json:"date"`
	Open int    `json:"open"`
}

// WeekPoint is new vs fixed per ISO week.
type WeekPoint struct {
	Week  string `json:"week"`
	New   int    `json:"new"`
	Fixed int    `json:"fixed"`
}

// Overview is the dashboard payload.
type Overview struct {
	Open     Counts       `json:"open"`
	KEV      int          `json:"kev"`
	Trend    []TrendPoint `json:"trend"`
	Weekly   []WeekPoint  `json:"weekly"`
	Queue    []FixGroup   `json:"queue"`
	Coverage struct {
		Score  float64 `json:"score"`
		Gaps   int     `json:"gaps"`
		Assets int     `json:"assets"`
	} `json:"coverage"`
	StaleSources int `json:"staleSources"`
}

// Overview builds the dashboard.
func (s *Store) Overview(ctx context.Context) (*Overview, error) {
	o := &Overview{Trend: []TrendPoint{}, Weekly: []WeekPoint{}}
	rows, err := s.DB.Query(ctx, `SELECT f.priority, count(*) FROM findings f JOIN assets a ON a.id=f.asset_id
		WHERE f.state = ANY($1) AND a.retired_at IS NULL GROUP BY f.priority`, OpenStates)
	if err != nil {
		return nil, err
	}
	for rows.Next() {
		var p, n int
		if err := rows.Scan(&p, &n); err != nil {
			rows.Close()
			return nil, err
		}
		o.Open.add(p, n)
	}
	rows.Close()
	if err := s.DB.QueryRow(ctx, `SELECT count(*) FROM findings f JOIN advisories ad ON ad.id=f.advisory_id JOIN assets a ON a.id=f.asset_id
		WHERE ad.kev AND f.state = ANY($1) AND a.retired_at IS NULL`, OpenStates).Scan(&o.KEV); err != nil {
		return nil, err
	}

	// Trend: a finding counts as open on day d if first_seen <= end of d and it was not
	// fixed/accepted/false-positive by then. Closure time for triaged states comes from events.
	now := s.Now().UTC()
	start := time.Date(now.Year(), now.Month(), now.Day(), 0, 0, 0, 0, time.UTC).AddDate(0, 0, -89)
	trows, err := s.DB.Query(ctx, `WITH days AS (
		  SELECT generate_series($1::date, $2::date, '1 day')::date AS d
		), closed AS (
		  SELECT f.id, f.first_seen,
		    CASE WHEN f.state IN ('new','acknowledged') THEN NULL
		         ELSE COALESCE(f.fixed_at, (SELECT max(e.at) FROM finding_events e WHERE e.finding_id=f.id AND e.to_state=f.state)) END AS closed_at
		  FROM findings f JOIN assets a ON a.id=f.asset_id WHERE a.retired_at IS NULL
		)
		SELECT to_char(d,'YYYY-MM-DD'),
		  (SELECT count(*) FROM closed c WHERE c.first_seen < d + 1 AND (c.closed_at IS NULL OR c.closed_at >= d + 1))
		FROM days ORDER BY d`, start, now)
	if err != nil {
		return nil, err
	}
	for trows.Next() {
		var p TrendPoint
		if err := trows.Scan(&p.Date, &p.Open); err != nil {
			trows.Close()
			return nil, err
		}
		o.Trend = append(o.Trend, p)
	}
	trows.Close()

	wstart := now.AddDate(0, 0, -7*11)
	wrows, err := s.DB.Query(ctx, `WITH weeks AS (
		  SELECT generate_series(date_trunc('week', $1::timestamptz), date_trunc('week', $2::timestamptz), '1 week') AS w
		)
		SELECT to_char(w, 'IYYY-"W"IW'),
		  (SELECT count(*) FROM findings f WHERE f.first_seen >= w AND f.first_seen < w + interval '1 week'),
		  (SELECT count(*) FROM findings f WHERE f.fixed_at >= w AND f.fixed_at < w + interval '1 week')
		FROM weeks ORDER BY w`, wstart, now)
	if err != nil {
		return nil, err
	}
	for wrows.Next() {
		var p WeekPoint
		if err := wrows.Scan(&p.Week, &p.New, &p.Fixed); err != nil {
			wrows.Close()
			return nil, err
		}
		o.Weekly = append(o.Weekly, p)
	}
	wrows.Close()

	fixes, err := s.Fixes(ctx, 0)
	if err != nil {
		return nil, err
	}
	if len(fixes) > 5 {
		fixes = fixes[:5]
	}
	o.Queue = fixes
	cov, err := s.Coverage(ctx)
	if err != nil {
		return nil, err
	}
	o.Coverage.Score, o.Coverage.Gaps, o.Coverage.Assets = cov.Score, len(cov.Gaps), cov.Assets
	srcs, err := s.ListSources(ctx)
	if err != nil {
		return nil, err
	}
	for _, x := range srcs {
		if x.Stale {
			o.StaleSources++
		}
	}
	return o, nil
}
