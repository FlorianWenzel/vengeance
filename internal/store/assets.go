package store

import (
	"context"
	"sort"
	"strings"
	"time"

	"github.com/jackc/pgx/v5"
)

// Asset is the API shape.
type Asset struct {
	ID          int64          `json:"id"`
	Kind        string         `json:"kind"`
	Identifier  string         `json:"identifier"`
	Name        string         `json:"name"`
	ProjectID   *int64         `json:"projectId"`
	ProjectName *string        `json:"projectName"`
	Tags        []string       `json:"tags"`
	RetiredAt   *time.Time     `json:"retiredAt"`
	CreatedAt   time.Time      `json:"createdAt"`
	LastScanAt  *time.Time     `json:"lastScanAt"`
	Discovered  bool           `json:"discovered"`
	Open        Counts         `json:"open"`
	OpenKinds   KindCounts     `json:"openKinds"`
	Coverage    []CoverageCell `json:"coverage"`
	Covered     bool           `json:"covered"`
}

// KindCounts is open findings per kind.
type KindCounts struct {
	Vuln      int `json:"vuln"`
	Secret    int `json:"secret"`
	Misconfig int `json:"misconfig"`
	SAST      int `json:"sast"`
}

func (k *KindCounts) add(kind string, n int) {
	switch kind {
	case "vuln":
		k.Vuln += n
	case "secret":
		k.Secret += n
	case "misconfig":
		k.Misconfig += n
	case "sast":
		k.SAST += n
	}
}

// CoverageCell is one expected scan type on one asset.
type CoverageCell struct {
	ScanType   string     `json:"scanType"`
	LastAt     *time.Time `json:"lastAt"`
	MaxAgeDays int        `json:"maxAgeDays"`
	OK         bool       `json:"ok"`
}

// Expectation is how often an asset kind needs a scan type.
type Expectation struct {
	Kind       string `json:"kind"`
	ScanType   string `json:"scanType"`
	MaxAgeDays int    `json:"maxAgeDays"`
}

// AssetFilter filters ListAssets.
type AssetFilter struct {
	ProjectID      int64
	Kind           string
	IncludeRetired bool
	ID             int64
}

// ListAssets returns assets with counts and coverage.
func (s *Store) ListAssets(ctx context.Context, f AssetFilter) ([]Asset, error) {
	where := []string{"true"}
	args := []any{}
	add := func(cond string, v any) {
		args = append(args, v)
		where = append(where, strings.ReplaceAll(cond, "?", "$"+itoa(len(args))))
	}
	if !f.IncludeRetired && f.ID == 0 {
		where = append(where, "a.retired_at IS NULL")
	}
	if f.ProjectID > 0 {
		add("a.project_id = ?", f.ProjectID)
	}
	if f.Kind != "" {
		add("a.kind = ?", f.Kind)
	}
	if f.ID > 0 {
		add("a.id = ?", f.ID)
	}
	rows, err := s.DB.Query(ctx, `SELECT a.id, a.kind, a.identifier, a.name, a.project_id, p.name, a.tags, a.retired_at, a.created_at,
		a.last_scan_at, a.discovered
		FROM assets a LEFT JOIN projects p ON p.id=a.project_id
		WHERE `+strings.Join(where, " AND ")+` ORDER BY a.retired_at NULLS FIRST, p.name NULLS LAST, a.kind, a.name`, args...)
	if err != nil {
		return nil, err
	}
	assets, err := pgx.CollectRows(rows, func(r pgx.CollectableRow) (Asset, error) {
		var a Asset
		err := r.Scan(&a.ID, &a.Kind, &a.Identifier, &a.Name, &a.ProjectID, &a.ProjectName, &a.Tags, &a.RetiredAt, &a.CreatedAt, &a.LastScanAt, &a.Discovered)
		a.Coverage = []CoverageCell{}
		return a, err
	})
	if err != nil {
		return nil, err
	}
	if len(assets) == 0 {
		return assets, nil
	}
	idx := map[int64]int{}
	ids := make([]int64, len(assets))
	for i, a := range assets {
		idx[a.ID] = i
		ids[i] = a.ID
	}
	// open counts
	crow, err := s.DB.Query(ctx, `SELECT asset_id, priority, kind, count(*) FROM findings
		WHERE asset_id = ANY($1) AND state = ANY($2) GROUP BY asset_id, priority, kind`, ids, OpenStates)
	if err != nil {
		return nil, err
	}
	for crow.Next() {
		var id int64
		var p, n int
		var kind string
		if err := crow.Scan(&id, &p, &kind, &n); err != nil {
			crow.Close()
			return nil, err
		}
		assets[idx[id]].Open.add(p, n)
		assets[idx[id]].OpenKinds.add(kind, n)
	}
	crow.Close()
	// coverage
	exp, err := s.Expectations(ctx)
	if err != nil {
		return nil, err
	}
	last, err := s.lastScans(ctx, ids)
	if err != nil {
		return nil, err
	}
	now := s.Now()
	for i := range assets {
		a := &assets[i]
		a.Covered = true
		for _, e := range exp {
			if e.Kind != a.Kind {
				continue
			}
			c := CoverageCell{ScanType: e.ScanType, MaxAgeDays: e.MaxAgeDays}
			if t, ok := last[a.ID][e.ScanType]; ok {
				tt := t
				c.LastAt = &tt
				c.OK = now.Sub(t) <= time.Duration(e.MaxAgeDays)*24*time.Hour
			}
			if !c.OK {
				a.Covered = false
			}
			a.Coverage = append(a.Coverage, c)
		}
	}
	return assets, nil
}

func (s *Store) lastScans(ctx context.Context, ids []int64) (map[int64]map[string]time.Time, error) {
	rows, err := s.DB.Query(ctx, `SELECT asset_id, t, max(received_at) FROM scans, unnest(scan_types) t
		WHERE asset_id = ANY($1) GROUP BY asset_id, t`, ids)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	out := map[int64]map[string]time.Time{}
	for rows.Next() {
		var id int64
		var t string
		var at time.Time
		if err := rows.Scan(&id, &t, &at); err != nil {
			return nil, err
		}
		if out[id] == nil {
			out[id] = map[string]time.Time{}
		}
		out[id][t] = at
	}
	return out, rows.Err()
}

// GetAsset returns one asset (retired included).
func (s *Store) GetAsset(ctx context.Context, id int64) (*Asset, error) {
	as, err := s.ListAssets(ctx, AssetFilter{ID: id})
	if err != nil {
		return nil, err
	}
	if len(as) == 0 {
		return nil, ErrNotFound
	}
	return &as[0], nil
}

// AssetInput creates or edits an asset.
type AssetInput struct {
	Kind       string    `json:"kind"`
	Identifier string    `json:"identifier"`
	Name       *string   `json:"name"`
	ProjectID  *int64    `json:"projectId"`
	Tags       *[]string `json:"tags"`
	Retired    *bool     `json:"retired"`
}

func cleanTags(t []string) []string {
	out := []string{}
	seen := map[string]bool{}
	for _, v := range t {
		v = strings.ToLower(strings.TrimSpace(v))
		if v != "" && !seen[v] {
			seen[v] = true
			out = append(out, v)
		}
	}
	sort.Strings(out)
	return out
}

// CreateAsset adds an asset by hand.
func (s *Store) CreateAsset(ctx context.Context, in AssetInput) (*Asset, error) {
	in.Identifier = strings.TrimSpace(in.Identifier)
	if !contains(AssetKinds, in.Kind) {
		return nil, invalid("kind must be one of " + strings.Join(AssetKinds, ", "))
	}
	if in.Identifier == "" {
		return nil, invalid("identifier is required")
	}
	if in.Kind == "image" {
		in.Identifier = ImageRepo(in.Identifier)
	}
	name := defaultAssetName(in.Kind, in.Identifier)
	if in.Name != nil && strings.TrimSpace(*in.Name) != "" {
		name = strings.TrimSpace(*in.Name)
	}
	tags := []string{}
	if in.Tags != nil {
		tags = cleanTags(*in.Tags)
	}
	var id int64
	err := s.DB.QueryRow(ctx, `INSERT INTO assets (kind, identifier, name, project_id, tags) VALUES ($1,$2,$3,$4,$5) RETURNING id`,
		in.Kind, in.Identifier, name, in.ProjectID, tags).Scan(&id)
	if isUnique(err) {
		return nil, ErrConflict
	}
	if err != nil {
		return nil, err
	}
	return s.GetAsset(ctx, id)
}

// UpdateAsset edits name, project, tags or retirement. Tag changes recompute priorities.
func (s *Store) UpdateAsset(ctx context.Context, id int64, in AssetInput) (*Asset, error) {
	err := s.tx(ctx, func(tx pgx.Tx) error {
		var exists bool
		if err := tx.QueryRow(ctx, `SELECT true FROM assets WHERE id=$1 FOR UPDATE`, id).Scan(&exists); err != nil {
			return notFound(err)
		}
		if in.Name != nil {
			n := strings.TrimSpace(*in.Name)
			if n == "" {
				return invalid("name cannot be empty")
			}
			if _, err := tx.Exec(ctx, `UPDATE assets SET name=$2 WHERE id=$1`, id, n); err != nil {
				return err
			}
		}
		if in.ProjectID != nil {
			var pid *int64
			if *in.ProjectID > 0 {
				pid = in.ProjectID
			}
			if _, err := tx.Exec(ctx, `UPDATE assets SET project_id=$2 WHERE id=$1`, id, pid); err != nil {
				return err
			}
		}
		if in.Tags != nil {
			if _, err := tx.Exec(ctx, `UPDATE assets SET tags=$2 WHERE id=$1`, id, cleanTags(*in.Tags)); err != nil {
				return err
			}
			if err := recomputePriorities(ctx, tx, "f.asset_id = $1", id); err != nil {
				return err
			}
		}
		if in.Retired != nil {
			if _, err := tx.Exec(ctx, `UPDATE assets SET retired_at = CASE WHEN $2 THEN COALESCE(retired_at, now()) ELSE NULL END WHERE id=$1`, id, *in.Retired); err != nil {
				return err
			}
		}
		return nil
	})
	if err != nil {
		return nil, err
	}
	return s.GetAsset(ctx, id)
}

// DeleteAsset removes an asset, its scans and findings.
func (s *Store) DeleteAsset(ctx context.Context, id int64) error {
	ct, err := s.DB.Exec(ctx, `DELETE FROM assets WHERE id=$1`, id)
	if err == nil && ct.RowsAffected() == 0 {
		return ErrNotFound
	}
	return err
}

// ScanInfo is one row of an asset's scan history.
type ScanInfo struct {
	ID           int64     `json:"id"`
	SourceName   *string   `json:"sourceName"`
	Format       string    `json:"format"`
	ScanTypes    []string  `json:"scanTypes"`
	Digest       *string   `json:"digest"`
	ReceivedAt   time.Time `json:"receivedAt"`
	FindingCount int       `json:"findingCount"`
}

// AssetScans lists recent scans.
func (s *Store) AssetScans(ctx context.Context, id int64) ([]ScanInfo, error) {
	rows, err := s.DB.Query(ctx, `SELECT sc.id, so.name, sc.format, sc.scan_types, sc.digest, sc.received_at, sc.finding_count
		FROM scans sc LEFT JOIN sources so ON so.id=sc.source_id WHERE sc.asset_id=$1 ORDER BY sc.received_at DESC LIMIT 100`, id)
	if err != nil {
		return nil, err
	}
	return pgx.CollectRows(rows, func(r pgx.CollectableRow) (ScanInfo, error) {
		var x ScanInfo
		return x, r.Scan(&x.ID, &x.SourceName, &x.Format, &x.ScanTypes, &x.Digest, &x.ReceivedAt, &x.FindingCount)
	})
}

// recomputePriorities re-derives priority for open findings matching cond (with $1 bound to arg).
func recomputePriorities(ctx context.Context, tx pgx.Tx, cond string, arg any) error {
	_, err := tx.Exec(ctx, `UPDATE findings f SET priority = CASE
		  WHEN f.kind='secret' THEN CASE WHEN f.severity IN ('critical','high') THEN 1 ELSE 3 END
		  WHEN f.kind IN ('misconfig','sast') THEN CASE WHEN f.severity IN ('critical','high') THEN 2 ELSE 3 END
		  WHEN COALESCE(ad.kev,false) THEN 1
		  WHEN f.fixed_version IS NULL THEN 4
		  WHEN f.severity='critical' AND 'internet-facing' = ANY(a.tags) THEN 1
		  WHEN f.severity IN ('critical','high') THEN 2
		  ELSE 3 END
		FROM assets a, findings f2 LEFT JOIN advisories ad ON ad.id=f2.advisory_id
		WHERE a.id=f.asset_id AND f2.id=f.id AND `+cond, arg)
	return err
}

// Expectations lists the expected scans per asset kind.
func (s *Store) Expectations(ctx context.Context) ([]Expectation, error) {
	rows, err := s.DB.Query(ctx, `SELECT kind, scan_type, max_age_days FROM expectations ORDER BY kind, scan_type`)
	if err != nil {
		return nil, err
	}
	return pgx.CollectRows(rows, func(r pgx.CollectableRow) (Expectation, error) {
		var e Expectation
		return e, r.Scan(&e.Kind, &e.ScanType, &e.MaxAgeDays)
	})
}

// SetExpectations replaces the whole list.
func (s *Store) SetExpectations(ctx context.Context, list []Expectation) error {
	for _, e := range list {
		if !contains(AssetKinds, e.Kind) || !contains(ScanTypes, e.ScanType) {
			return invalid("unknown kind or scan type: " + e.Kind + "/" + e.ScanType)
		}
		if e.MaxAgeDays < 1 || e.MaxAgeDays > 365 {
			return invalid("max age must be between 1 and 365 days")
		}
	}
	return s.tx(ctx, func(tx pgx.Tx) error {
		if _, err := tx.Exec(ctx, `DELETE FROM expectations`); err != nil {
			return err
		}
		for _, e := range list {
			if _, err := tx.Exec(ctx, `INSERT INTO expectations (kind, scan_type, max_age_days) VALUES ($1,$2,$3)
				ON CONFLICT (kind, scan_type) DO UPDATE SET max_age_days=EXCLUDED.max_age_days`, e.Kind, e.ScanType, e.MaxAgeDays); err != nil {
				return err
			}
		}
		return nil
	})
}

// Gap is one missing or stale expected scan.
type Gap struct {
	AssetID    int64      `json:"assetId"`
	AssetName  string     `json:"assetName"`
	AssetKind  string     `json:"assetKind"`
	ProjectID  *int64     `json:"-"`
	ScanType   string     `json:"scanType"`
	LastAt     *time.Time `json:"lastAt"`
	MaxAgeDays int        `json:"maxAgeDays"`
}

// ProjectCoverage is one project's score.
type ProjectCoverage struct {
	ProjectID *int64  `json:"projectId"`
	Name      string  `json:"name"`
	Score     float64 `json:"score"`
	Assets    int     `json:"assets"`
	Gaps      int     `json:"gaps"`
}

// Coverage is the coverage report.
type Coverage struct {
	Score     float64           `json:"score"`
	ByProject []ProjectCoverage `json:"byProject"`
	Gaps      []Gap             `json:"gaps"`
	Assets    int               `json:"-"`
}

// Coverage computes scores and gaps over non-retired assets.
func (s *Store) Coverage(ctx context.Context) (*Coverage, error) {
	assets, err := s.ListAssets(ctx, AssetFilter{})
	if err != nil {
		return nil, err
	}
	cov := &Coverage{ByProject: []ProjectCoverage{}, Gaps: []Gap{}, Assets: len(assets)}
	type acc struct {
		pc        ProjectCoverage
		ok, total int
	}
	byP := map[string]*acc{}
	order := []string{}
	ok, total := 0, 0
	for _, a := range assets {
		key, name := "", "Unassigned"
		if a.ProjectID != nil {
			key, name = itoa(int(*a.ProjectID)), *a.ProjectName
		}
		p := byP[key]
		if p == nil {
			p = &acc{pc: ProjectCoverage{ProjectID: a.ProjectID, Name: name}}
			byP[key] = p
			order = append(order, key)
		}
		p.pc.Assets++
		for _, c := range a.Coverage {
			total++
			p.total++
			if c.OK {
				ok++
				p.ok++
			} else {
				p.pc.Gaps++
				cov.Gaps = append(cov.Gaps, Gap{AssetID: a.ID, AssetName: a.Name, AssetKind: a.Kind, ProjectID: a.ProjectID,
					ScanType: c.ScanType, LastAt: c.LastAt, MaxAgeDays: c.MaxAgeDays})
			}
		}
	}
	cov.Score = ratio(ok, total)
	for _, k := range order {
		p := byP[k]
		p.pc.Score = ratio(p.ok, p.total)
		cov.ByProject = append(cov.ByProject, p.pc)
	}
	return cov, nil
}

func ratio(a, b int) float64 {
	if b == 0 {
		return 1
	}
	return float64(int(float64(a)/float64(b)*1000+0.5)) / 1000
}

func itoa(i int) string {
	const digits = "0123456789"
	if i == 0 {
		return "0"
	}
	neg := i < 0
	if neg {
		i = -i
	}
	var b []byte
	for i > 0 {
		b = append([]byte{digits[i%10]}, b...)
		i /= 10
	}
	if neg {
		b = append([]byte{'-'}, b...)
	}
	return string(b)
}
