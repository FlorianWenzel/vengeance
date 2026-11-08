package store

import (
	"bytes"
	"compress/gzip"
	"context"
	"fmt"
	"strings"
	"time"

	"github.com/jackc/pgx/v5"

	"github.com/FlorianWenzel/vengeance/internal/core"
	"github.com/FlorianWenzel/vengeance/internal/ingest"
)

// IngestOptions come from the ingest request.
type IngestOptions struct {
	Asset     string // kind:identifier, optional
	Name      string
	Project   string
	ScanTypes []string
	Digest    string
}

// IngestResult is returned to the uploader.
type IngestResult struct {
	ScanID   int64 `json:"scanId"`
	AssetID  int64 `json:"assetId"`
	New      int   `json:"new"`
	Reopened int   `json:"reopened"`
	Fixed    int   `json:"fixed"`
	Open     int   `json:"open"`
	// NewFindingIDs feeds the notifier (new and reopened findings); not serialised.
	NewFindingIDs []int64 `json:"-"`
}

// AssetKinds are the valid asset kinds.
var AssetKinds = []string{"repository", "image", "host", "cluster", "domain", "registry"}

// ScanTypes are the valid scan types.
var ScanTypes = []string{"vuln", "secret", "misconfig", "sast"}

// ParseAssetRef splits "kind:identifier".
func ParseAssetRef(ref string) (kind, ident string, err error) {
	kind, ident, ok := strings.Cut(ref, ":")
	if !ok || ident == "" || !contains(AssetKinds, kind) {
		return "", "", invalid(fmt.Sprintf("asset must look like kind:identifier with kind one of %s", strings.Join(AssetKinds, ", ")))
	}
	return kind, strings.TrimSpace(ident), nil
}

// AssetFromReport derives the asset when the uploader did not name one.
func AssetFromReport(r *ingest.Report) (kind, ident string, err error) {
	switch r.ArtifactType {
	case "container_image":
		return "image", ImageRepo(r.ArtifactName), nil
	case "cluster":
		return "cluster", r.ArtifactName, nil
	case "repository", "filesystem", "vm":
		if r.ArtifactName != "" {
			return "repository", r.ArtifactName, nil
		}
	}
	return "", "", invalid("cannot tell which asset this report is for; pass ?asset=kind:identifier")
}

// ImageRepo strips tag and digest from an image reference.
func ImageRepo(ref string) string {
	if i := strings.Index(ref, "@"); i >= 0 {
		ref = ref[:i]
	}
	slash := strings.LastIndex(ref, "/")
	if i := strings.LastIndex(ref, ":"); i > slash {
		ref = ref[:i]
	}
	return ref
}

func contains(list []string, s string) bool {
	for _, v := range list {
		if v == s {
			return true
		}
	}
	return false
}

// Ingest stores one scan for one asset and reconciles findings: creates new ones, reopens
// fixed ones that came back, and closes ones this source no longer reports.
func (s *Store) Ingest(ctx context.Context, sourceID int64, sourceName string, opts IngestOptions, rep *ingest.Report, raw []byte) (*IngestResult, error) {
	var kind, ident string
	var err error
	if opts.Asset != "" {
		kind, ident, err = ParseAssetRef(opts.Asset)
	} else {
		kind, ident, err = AssetFromReport(rep)
	}
	if err != nil {
		return nil, err
	}
	types := rep.ScanTypes
	if len(opts.ScanTypes) > 0 {
		for _, t := range opts.ScanTypes {
			if !contains(ScanTypes, t) {
				return nil, invalid("unknown scanner " + t + "; use " + strings.Join(ScanTypes, ","))
			}
		}
		types = opts.ScanTypes
	}
	digest := opts.Digest
	if digest == "" {
		digest = rep.Digest
	}
	var gz bytes.Buffer
	zw := gzip.NewWriter(&gz)
	_, _ = zw.Write(raw)
	_ = zw.Close()

	actor := "source:" + sourceName
	res := &IngestResult{}
	now := s.Now()
	err = s.tx(ctx, func(tx pgx.Tx) error {
		// asset
		var tags []string
		err := tx.QueryRow(ctx, `SELECT id, tags FROM assets WHERE kind=$1 AND identifier=$2`, kind, ident).Scan(&res.AssetID, &tags)
		if err == pgx.ErrNoRows {
			var projectID *int64
			if opts.Project != "" {
				var pid int64
				if err := tx.QueryRow(ctx, `INSERT INTO projects (name) VALUES ($1)
					ON CONFLICT (name) DO UPDATE SET name=EXCLUDED.name RETURNING id`, opts.Project).Scan(&pid); err != nil {
					return err
				}
				projectID = &pid
			}
			name := opts.Name
			if name == "" {
				name = defaultAssetName(kind, ident)
			}
			if err := tx.QueryRow(ctx, `INSERT INTO assets (kind, identifier, name, project_id, discovered)
				VALUES ($1,$2,$3,$4,true) RETURNING id, tags`, kind, ident, name, projectID).Scan(&res.AssetID, &tags); err != nil {
				return err
			}
		} else if err != nil {
			return err
		}

		if err := tx.QueryRow(ctx, `INSERT INTO scans (source_id, asset_id, format, scan_types, digest, finding_count, raw, received_at)
			VALUES ($1,$2,$3,$4,NULLIF($5,''),$6,$7,$8) RETURNING id`,
			sourceID, res.AssetID, rep.Format, types, digest, len(rep.Findings), gz.Bytes(), now).Scan(&res.ScanID); err != nil {
			return err
		}

		seen := make([]string, 0, len(rep.Findings))
		for _, f := range rep.Findings {
			fp := f.Fingerprint()
			seen = append(seen, fp)
			kev := false
			if f.Kind == "vuln" {
				if err := tx.QueryRow(ctx, `INSERT INTO advisories (id, title, description, severity, cvss, published_at, refs, kev, kev_added, updated_at)
					VALUES ($1,$2,$3,$4,$5,$6,$7, EXISTS(SELECT 1 FROM kev WHERE cve_id=$1), (SELECT date_added FROM kev WHERE cve_id=$1), now())
					ON CONFLICT (id) DO UPDATE SET
					  title = CASE WHEN EXCLUDED.title <> '' THEN EXCLUDED.title ELSE advisories.title END,
					  description = CASE WHEN EXCLUDED.description <> '' THEN EXCLUDED.description ELSE advisories.description END,
					  cvss = COALESCE(EXCLUDED.cvss, advisories.cvss),
					  published_at = COALESCE(EXCLUDED.published_at, advisories.published_at),
					  refs = CASE WHEN cardinality(EXCLUDED.refs) > 0 THEN EXCLUDED.refs ELSE advisories.refs END,
					  updated_at = now()
					RETURNING kev`,
					f.AdvisoryID, f.Title, f.Description, f.Severity, f.CVSS, f.Published, nonNil(f.References)).Scan(&kev); err != nil {
					return fmt.Errorf("advisory %s: %w", f.AdvisoryID, err)
				}
			}
			prio := core.Priority(core.PriorityInput{Kind: f.Kind, Severity: f.Severity, HasFix: f.Fixed != "", KEV: kev, AssetTags: tags})
			var id int64
			var state string
			var inserted bool
			if err := tx.QueryRow(ctx, `INSERT INTO findings (asset_id, fingerprint, kind, advisory_id, title, severity, priority,
					pkg_name, pkg_ecosystem, installed_version, fixed_version, target, first_seen, last_seen)
				VALUES ($1,$2,$3,$4,$5,$6,$7,NULLIF($8,''),NULLIF($9,''),NULLIF($10,''),NULLIF($11,''),$12,$13,$13)
				ON CONFLICT (asset_id, fingerprint) DO UPDATE SET
				  title=EXCLUDED.title, severity=EXCLUDED.severity, priority=EXCLUDED.priority,
				  installed_version=EXCLUDED.installed_version, fixed_version=EXCLUDED.fixed_version,
				  target=EXCLUDED.target, last_seen=EXCLUDED.last_seen
				RETURNING id, state, (xmax = 0)`,
				res.AssetID, fp, f.Kind, f.AdvisoryID, f.Title, f.Severity, prio,
				f.PkgName, f.Ecosystem, f.Installed, f.Fixed, f.Target, now).Scan(&id, &state, &inserted); err != nil {
				return fmt.Errorf("finding %s: %w", f.AdvisoryID, err)
			}
			switch {
			case inserted:
				res.New++
				res.NewFindingIDs = append(res.NewFindingIDs, id)
				if err := event(ctx, tx, id, actor, "", "new", "first seen"); err != nil {
					return err
				}
			case state == "fixed":
				res.Reopened++
				res.NewFindingIDs = append(res.NewFindingIDs, id)
				if _, err := tx.Exec(ctx, `UPDATE findings SET state='new', fixed_at=NULL, state_reason=NULL, accepted_until=NULL WHERE id=$1`, id); err != nil {
					return err
				}
				if err := event(ctx, tx, id, actor, "fixed", "new", "seen again"); err != nil {
					return err
				}
			}
			if _, err := tx.Exec(ctx, `INSERT INTO finding_sources (finding_id, source_id, last_seen_at) VALUES ($1,$2,$3)
				ON CONFLICT (finding_id, source_id) DO UPDATE SET last_seen_at=EXCLUDED.last_seen_at`, id, sourceID, now); err != nil {
				return err
			}
		}

		// Auto-close: this source scanned this asset for these types and no longer reports these.
		rows, err := tx.Query(ctx, `DELETE FROM finding_sources fs USING findings f
			WHERE fs.finding_id=f.id AND f.asset_id=$1 AND fs.source_id=$2 AND f.kind = ANY($3) AND NOT (f.fingerprint = ANY($4))
			RETURNING f.id`, res.AssetID, sourceID, types, seen)
		if err != nil {
			return err
		}
		gone, err := pgx.CollectRows(rows, pgx.RowTo[int64])
		if err != nil {
			return err
		}
		if len(gone) > 0 {
			rows, err := tx.Query(ctx, `UPDATE findings f SET state='fixed', fixed_at=$2,
					state_reason=NULL, accepted_until=NULL
				FROM (SELECT id, state AS old FROM findings WHERE id = ANY($1) FOR UPDATE) o
				WHERE f.id=o.id AND f.state <> 'fixed'
				  AND NOT EXISTS (SELECT 1 FROM finding_sources s WHERE s.finding_id=f.id)
				RETURNING f.id, o.old`, gone, now)
			if err != nil {
				return err
			}
			type closed struct {
				id  int64
				old string
			}
			var cl []closed
			for rows.Next() {
				var c closed
				if err := rows.Scan(&c.id, &c.old); err != nil {
					return err
				}
				cl = append(cl, c)
			}
			if err := rows.Err(); err != nil {
				return err
			}
			for _, c := range cl {
				if err := event(ctx, tx, c.id, actor, c.old, "fixed", "no longer reported"); err != nil {
					return err
				}
			}
			res.Fixed = len(cl)
		}

		if _, err := tx.Exec(ctx, `UPDATE assets SET last_scan_at=$2 WHERE id=$1`, res.AssetID, now); err != nil {
			return err
		}
		if _, err := tx.Exec(ctx, `UPDATE sources SET last_seen_at=$2 WHERE id=$1`, sourceID, now); err != nil {
			return err
		}
		return tx.QueryRow(ctx, `SELECT count(*) FROM findings WHERE asset_id=$1 AND state = ANY($2)`, res.AssetID, OpenStates).Scan(&res.Open)
	})
	if err != nil {
		return nil, err
	}
	return res, nil
}

func defaultAssetName(kind, ident string) string {
	if kind == "image" || kind == "repository" {
		if i := strings.LastIndex(ident, "/"); i >= 0 && i < len(ident)-1 {
			return ident[i+1:]
		}
	}
	return ident
}

func nonNil(s []string) []string {
	if s == nil {
		return []string{}
	}
	return s
}

func event(ctx context.Context, tx pgx.Tx, findingID int64, actor, from, to, note string) error {
	_, err := tx.Exec(ctx, `INSERT INTO finding_events (finding_id, actor, from_state, to_state, note) VALUES ($1,$2,NULLIF($3,''),$4,NULLIF($5,''))`,
		findingID, actor, from, to, note)
	return err
}

// PruneRaw drops raw reports older than the retention window.
func (s *Store) PruneRaw(ctx context.Context, keep time.Duration) error {
	_, err := s.DB.Exec(ctx, `UPDATE scans SET raw=NULL WHERE raw IS NOT NULL AND received_at < $1`, s.Now().Add(-keep))
	return err
}
