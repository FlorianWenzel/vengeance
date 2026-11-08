package store

import (
	"context"
	"time"

	"github.com/jackc/pgx/v5"
)

// KEVEntry is one CISA KEV row.
type KEVEntry struct {
	CVE       string
	DateAdded time.Time
}

// SyncKEV replaces the KEV list and returns open findings whose advisory just entered it.
func (s *Store) SyncKEV(ctx context.Context, entries []KEVEntry) (newlyKEV []int64, err error) {
	err = s.tx(ctx, func(tx pgx.Tx) error {
		if _, err := tx.Exec(ctx, `CREATE TEMP TABLE kev_in (cve_id TEXT PRIMARY KEY, date_added DATE) ON COMMIT DROP`); err != nil {
			return err
		}
		rowsIn := make([][]any, 0, len(entries))
		seen := map[string]bool{}
		for _, e := range entries {
			if e.CVE == "" || seen[e.CVE] {
				continue
			}
			seen[e.CVE] = true
			rowsIn = append(rowsIn, []any{e.CVE, e.DateAdded})
		}
		if _, err := tx.CopyFrom(ctx, pgx.Identifier{"kev_in"}, []string{"cve_id", "date_added"}, pgx.CopyFromRows(rowsIn)); err != nil {
			return err
		}
		if _, err := tx.Exec(ctx, `DELETE FROM kev WHERE cve_id NOT IN (SELECT cve_id FROM kev_in)`); err != nil {
			return err
		}
		if _, err := tx.Exec(ctx, `INSERT INTO kev SELECT * FROM kev_in ON CONFLICT (cve_id) DO UPDATE SET date_added=EXCLUDED.date_added`); err != nil {
			return err
		}
		rows, err := tx.Query(ctx, `UPDATE advisories ad SET kev=true, kev_added=k.date_added FROM kev k
			WHERE k.cve_id=ad.id AND NOT ad.kev RETURNING ad.id`)
		if err != nil {
			return err
		}
		advs, err := pgx.CollectRows(rows, pgx.RowTo[string])
		if err != nil {
			return err
		}
		if _, err := tx.Exec(ctx, `UPDATE advisories SET kev=false, kev_added=NULL WHERE kev AND id NOT IN (SELECT cve_id FROM kev)`); err != nil {
			return err
		}
		if len(advs) == 0 {
			return nil
		}
		if err := recomputePriorities(ctx, tx, "f.advisory_id = ANY($1)", advs); err != nil {
			return err
		}
		rows, err = tx.Query(ctx, `SELECT id, state FROM findings WHERE advisory_id = ANY($1) AND state = ANY($2)`, advs, OpenStates)
		if err != nil {
			return err
		}
		type fs struct {
			id    int64
			state string
		}
		list, err := pgx.CollectRows(rows, func(r pgx.CollectableRow) (fs, error) {
			var x fs
			return x, r.Scan(&x.id, &x.state)
		})
		if err != nil {
			return err
		}
		for _, x := range list {
			newlyKEV = append(newlyKEV, x.id)
			if err := event(ctx, tx, x.id, "system", x.state, x.state, "added to CISA KEV (known exploited), priority raised"); err != nil {
				return err
			}
		}
		return nil
	})
	return newlyKEV, err
}
