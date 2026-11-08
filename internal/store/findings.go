package store

import (
	"context"
	"fmt"
	"strconv"
	"strings"
	"time"

	"github.com/jackc/pgx/v5"
)

// Finding is the API shape.
type Finding struct {
	ID               int64      `json:"id"`
	AssetID          int64      `json:"assetId"`
	AssetName        string     `json:"assetName"`
	AssetKind        string     `json:"assetKind"`
	ProjectName      *string    `json:"projectName"`
	Kind             string     `json:"kind"`
	AdvisoryID       string     `json:"advisoryId"`
	Title            string     `json:"title"`
	Severity         string     `json:"severity"`
	Priority         int        `json:"priority"`
	PkgName          *string    `json:"pkgName"`
	PkgEcosystem     *string    `json:"pkgEcosystem"`
	InstalledVersion *string    `json:"installedVersion"`
	FixedVersion     *string    `json:"fixedVersion"`
	Target           string     `json:"target"`
	State            string     `json:"state"`
	StateReason      *string    `json:"stateReason"`
	AcceptedUntil    *string    `json:"acceptedUntil"`
	FirstSeen        time.Time  `json:"firstSeen"`
	LastSeen         time.Time  `json:"lastSeen"`
	FixedAt          *time.Time `json:"fixedAt"`
	KEV              bool       `json:"kev"`
	CVSS             *float32   `json:"cvss"`
	Sources          []string   `json:"sources"`
	// internal, for rule matching
	ProjectID *int64   `json:"-"`
	AssetTags []string `json:"-"`
}

// FindingDetail adds advisory text and history.
type FindingDetail struct {
	Finding
	Description string       `json:"description"`
	References  []string     `json:"references"`
	PublishedAt *time.Time   `json:"publishedAt"`
	Events      []FindingEvt `json:"events"`
}

// FindingEvt is one lifecycle event.
type FindingEvt struct {
	At        time.Time `json:"at"`
	Actor     string    `json:"actor"`
	FromState *string   `json:"fromState"`
	ToState   string    `json:"toState"`
	Note      *string   `json:"note"`
}

const findingSelect = `SELECT f.id, f.asset_id, a.name, a.kind, p.name, f.kind, f.advisory_id, f.title, f.severity, f.priority,
	f.pkg_name, f.pkg_ecosystem, f.installed_version, f.fixed_version, f.target, f.state, f.state_reason,
	to_char(f.accepted_until, 'YYYY-MM-DD'), f.first_seen, f.last_seen, f.fixed_at,
	COALESCE(ad.kev, false), ad.cvss,
	COALESCE((SELECT array_agg(s.name ORDER BY s.name) FROM finding_sources fs JOIN sources s ON s.id=fs.source_id WHERE fs.finding_id=f.id), '{}'),
	a.project_id, a.tags
	FROM findings f
	JOIN assets a ON a.id=f.asset_id
	LEFT JOIN projects p ON p.id=a.project_id
	LEFT JOIN advisories ad ON ad.id=f.advisory_id `

func scanFinding(row pgx.Row) (Finding, error) {
	var f Finding
	err := row.Scan(&f.ID, &f.AssetID, &f.AssetName, &f.AssetKind, &f.ProjectName, &f.Kind, &f.AdvisoryID, &f.Title, &f.Severity, &f.Priority,
		&f.PkgName, &f.PkgEcosystem, &f.InstalledVersion, &f.FixedVersion, &f.Target, &f.State, &f.StateReason,
		&f.AcceptedUntil, &f.FirstSeen, &f.LastSeen, &f.FixedAt, &f.KEV, &f.CVSS, &f.Sources, &f.ProjectID, &f.AssetTags)
	return f, err
}

// FindingFilter is the list query.
type FindingFilter struct {
	State      string
	Priorities []int
	AssetID    int64
	ProjectID  int64
	Kind       string
	Q          string
	Sort       string
	Limit      int
	Offset     int
	IDs        []int64
}

// ListFindings returns a page and the total.
func (s *Store) ListFindings(ctx context.Context, f FindingFilter) ([]Finding, int, error) {
	var where []string
	var args []any
	arg := func(v any) string { args = append(args, v); return "$" + strconv.Itoa(len(args)) }
	switch f.State {
	case "", "open":
		where = append(where, "f.state = ANY("+arg(OpenStates)+")")
	case "all":
	case "new", "acknowledged", "fixed", "accepted", "false_positive":
		where = append(where, "f.state = "+arg(f.State))
	default:
		return nil, 0, invalid("unknown state " + f.State)
	}
	if len(f.Priorities) > 0 {
		where = append(where, "f.priority = ANY("+arg(f.Priorities)+")")
	}
	if f.AssetID > 0 {
		where = append(where, "f.asset_id = "+arg(f.AssetID))
	}
	if f.ProjectID > 0 {
		where = append(where, "a.project_id = "+arg(f.ProjectID))
	}
	if f.Kind != "" {
		where = append(where, "f.kind = "+arg(f.Kind))
	}
	if len(f.IDs) > 0 {
		where = append(where, "f.id = ANY("+arg(f.IDs)+")")
	}
	if q := strings.TrimSpace(f.Q); q != "" {
		p := arg("%" + escapeLike(q) + "%")
		where = append(where, fmt.Sprintf("(f.advisory_id ILIKE %[1]s OR f.title ILIKE %[1]s OR f.pkg_name ILIKE %[1]s OR a.name ILIKE %[1]s)", p))
	}
	w := ""
	if len(where) > 0 {
		w = "WHERE " + strings.Join(where, " AND ")
	}
	order := "f.priority, f.severity_rank DESC, f.last_seen DESC, f.id"
	switch f.Sort {
	case "", "priority":
	case "lastSeen":
		order = "f.last_seen DESC, f.id"
	case "firstSeen":
		order = "f.first_seen DESC, f.id"
	default:
		return nil, 0, invalid("unknown sort " + f.Sort)
	}
	order = strings.Replace(order, "f.severity_rank DESC", sevRankSQL+" DESC", 1)
	if f.Limit <= 0 || f.Limit > 500 {
		if f.Limit > 500 {
			f.Limit = 500
		} else {
			f.Limit = 50
		}
	}
	var total int
	if err := s.DB.QueryRow(ctx, `SELECT count(*) FROM findings f JOIN assets a ON a.id=f.asset_id `+w, args...).Scan(&total); err != nil {
		return nil, 0, err
	}
	q := findingSelect + w + " ORDER BY " + order + " LIMIT " + arg(f.Limit) + " OFFSET " + arg(f.Offset)
	rows, err := s.DB.Query(ctx, q, args...)
	if err != nil {
		return nil, 0, err
	}
	defer rows.Close()
	out := []Finding{}
	for rows.Next() {
		fi, err := scanFinding(rows)
		if err != nil {
			return nil, 0, err
		}
		out = append(out, fi)
	}
	return out, total, rows.Err()
}

const sevRankSQL = `CASE f.severity WHEN 'critical' THEN 4 WHEN 'high' THEN 3 WHEN 'medium' THEN 2 WHEN 'low' THEN 1 ELSE 0 END`

func escapeLike(s string) string {
	return strings.NewReplacer(`\`, `\\`, `%`, `\%`, `_`, `\_`).Replace(s)
}

// GetFinding returns one finding with details.
func (s *Store) GetFinding(ctx context.Context, id int64) (*FindingDetail, error) {
	f, err := scanFinding(s.DB.QueryRow(ctx, findingSelect+"WHERE f.id=$1", id))
	if err != nil {
		return nil, notFound(err)
	}
	d := &FindingDetail{Finding: f, References: []string{}}
	_ = s.DB.QueryRow(ctx, `SELECT description, refs, published_at FROM advisories WHERE id=$1`, f.AdvisoryID).
		Scan(&d.Description, &d.References, &d.PublishedAt)
	if d.Description == "" {
		// secrets and misconfigurations carry no advisory row; fall back to the title
		d.Description = f.Title
	}
	rows, err := s.DB.Query(ctx, `SELECT at, actor, from_state, to_state, note FROM finding_events WHERE finding_id=$1 ORDER BY at, id`, id)
	if err != nil {
		return nil, err
	}
	evs, err := pgx.CollectRows(rows, func(r pgx.CollectableRow) (FindingEvt, error) {
		var e FindingEvt
		return e, r.Scan(&e.At, &e.Actor, &e.FromState, &e.ToState, &e.Note)
	})
	if err != nil {
		return nil, err
	}
	d.Events = evs
	return d, nil
}

// TriageInput is a bulk state change.
type TriageInput struct {
	IDs    []int64 `json:"ids"`
	State  string  `json:"state"`
	Reason string  `json:"reason"`
	Until  string  `json:"until"`
}

// Triage changes the state of findings and records events. Returns how many changed.
func (s *Store) Triage(ctx context.Context, actor string, in TriageInput) (int, error) {
	if len(in.IDs) == 0 {
		return 0, invalid("select at least one finding")
	}
	if len(in.IDs) > 1000 {
		return 0, invalid("at most 1000 findings at once")
	}
	var until *time.Time
	switch in.State {
	case "new", "acknowledged":
	case "accepted":
		t, err := time.Parse("2006-01-02", in.Until)
		if err != nil {
			return 0, invalid("accepting a risk needs an end date (YYYY-MM-DD)")
		}
		if !t.After(s.Now()) {
			return 0, invalid("the end date must be in the future")
		}
		until = &t
	case "false_positive":
		if strings.TrimSpace(in.Reason) == "" {
			return 0, invalid("marking a false positive needs a reason")
		}
	case "fixed":
		return 0, invalid("findings are marked fixed automatically when a scan no longer reports them")
	default:
		return 0, invalid("unknown state " + in.State)
	}
	reason := strings.TrimSpace(in.Reason)
	n := 0
	err := s.tx(ctx, func(tx pgx.Tx) error {
		rows, err := tx.Query(ctx, `UPDATE findings f SET state=$2, state_reason=NULLIF($3,''), accepted_until=$4
			FROM (SELECT id, state AS old FROM findings WHERE id = ANY($1) FOR UPDATE) o
			WHERE f.id=o.id AND (f.state <> $2 OR f.accepted_until IS DISTINCT FROM $4 OR f.state_reason IS DISTINCT FROM NULLIF($3,''))
			  AND f.state <> 'fixed'
			RETURNING f.id, o.old`, in.IDs, in.State, reason, until)
		if err != nil {
			return err
		}
		type ch struct {
			id  int64
			old string
		}
		var chs []ch
		for rows.Next() {
			var c ch
			if err := rows.Scan(&c.id, &c.old); err != nil {
				return err
			}
			chs = append(chs, c)
		}
		if err := rows.Err(); err != nil {
			return err
		}
		note := reason
		if until != nil {
			note = strings.TrimSpace("until " + in.Until + " " + reason)
		}
		for _, c := range chs {
			if err := event(ctx, tx, c.id, actor, c.old, in.State, note); err != nil {
				return err
			}
		}
		n = len(chs)
		return nil
	})
	return n, err
}

// ExpireAcceptances reopens accepted risks whose date has passed and returns their ids.
func (s *Store) ExpireAcceptances(ctx context.Context) ([]int64, error) {
	var ids []int64
	err := s.tx(ctx, func(tx pgx.Tx) error {
		rows, err := tx.Query(ctx, `UPDATE findings SET state='new', accepted_until=NULL, state_reason=NULL
			WHERE state='accepted' AND accepted_until <= $1::date RETURNING id`, s.Now())
		if err != nil {
			return err
		}
		ids, err = pgx.CollectRows(rows, pgx.RowTo[int64])
		if err != nil {
			return err
		}
		for _, id := range ids {
			if err := event(ctx, tx, id, "system", "accepted", "new", "accepted risk expired"); err != nil {
				return err
			}
		}
		return nil
	})
	return ids, err
}

// FindingsByIDs loads findings for notifications.
func (s *Store) FindingsByIDs(ctx context.Context, ids []int64) ([]Finding, error) {
	if len(ids) == 0 {
		return nil, nil
	}
	out, _, err := s.ListFindings(ctx, FindingFilter{State: "all", IDs: ids, Limit: 500})
	return out, err
}
