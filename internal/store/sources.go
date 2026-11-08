package store

import (
	"context"
	"crypto/rand"
	"crypto/sha256"
	"encoding/base64"
	"strings"
	"time"

	"github.com/jackc/pgx/v5"
)

// Source is the API shape.
type Source struct {
	ID              int64      `json:"id"`
	Name            string     `json:"name"`
	TokenPrefix     string     `json:"tokenPrefix"`
	LastSeenAt      *time.Time `json:"lastSeenAt"`
	StaleAfterHours int        `json:"staleAfterHours"`
	Stale           bool       `json:"stale"`
	Scans           int        `json:"scans"`
	CreatedAt       time.Time  `json:"createdAt"`
	Token           string     `json:"token,omitempty"`
}

// NewToken returns a random token with a recognisable prefix.
func NewToken(prefix string) string {
	b := make([]byte, 32)
	if _, err := rand.Read(b); err != nil {
		panic(err)
	}
	return prefix + base64.RawURLEncoding.EncodeToString(b)
}

// HashToken hashes a bearer token for storage.
func HashToken(t string) []byte {
	h := sha256.Sum256([]byte(t))
	return h[:]
}

func (s *Store) stale(src *Source) {
	ref := src.CreatedAt
	if src.LastSeenAt != nil {
		ref = *src.LastSeenAt
	}
	src.Stale = s.Now().Sub(ref) > time.Duration(src.StaleAfterHours)*time.Hour
}

// ListSources lists all sources.
func (s *Store) ListSources(ctx context.Context) ([]Source, error) {
	rows, err := s.DB.Query(ctx, `SELECT s.id, s.name, s.token_prefix, s.last_seen_at, s.stale_after_hours, s.created_at,
		(SELECT count(*) FROM scans WHERE source_id=s.id) FROM sources s ORDER BY s.name`)
	if err != nil {
		return nil, err
	}
	out, err := pgx.CollectRows(rows, func(r pgx.CollectableRow) (Source, error) {
		var x Source
		return x, r.Scan(&x.ID, &x.Name, &x.TokenPrefix, &x.LastSeenAt, &x.StaleAfterHours, &x.CreatedAt, &x.Scans)
	})
	for i := range out {
		s.stale(&out[i])
	}
	return out, err
}

// SourceInput creates or edits a source.
type SourceInput struct {
	Name            *string `json:"name"`
	StaleAfterHours *int    `json:"staleAfterHours"`
}

func validStale(h *int) (int, error) {
	if h == nil {
		return 48, nil
	}
	if *h < 1 || *h > 24*90 {
		return 0, invalid("stale threshold must be between 1 hour and 90 days")
	}
	return *h, nil
}

// CreateSource creates a source and returns it with its plaintext token.
func (s *Store) CreateSource(ctx context.Context, in SourceInput) (*Source, error) {
	if in.Name == nil || strings.TrimSpace(*in.Name) == "" {
		return nil, invalid("name is required")
	}
	h, err := validStale(in.StaleAfterHours)
	if err != nil {
		return nil, err
	}
	tok := NewToken("vg_")
	var id int64
	err = s.DB.QueryRow(ctx, `INSERT INTO sources (name, token_hash, token_prefix, stale_after_hours) VALUES ($1,$2,$3,$4) RETURNING id`,
		strings.TrimSpace(*in.Name), HashToken(tok), tok[:7], h).Scan(&id)
	if isUnique(err) {
		return nil, ErrConflict
	}
	if err != nil {
		return nil, err
	}
	src, err := s.getSource(ctx, id)
	if err != nil {
		return nil, err
	}
	src.Token = tok
	return src, nil
}

func (s *Store) getSource(ctx context.Context, id int64) (*Source, error) {
	all, err := s.ListSources(ctx)
	if err != nil {
		return nil, err
	}
	for _, x := range all {
		if x.ID == id {
			return &x, nil
		}
	}
	return nil, ErrNotFound
}

// UpdateSource edits a source.
func (s *Store) UpdateSource(ctx context.Context, id int64, in SourceInput) (*Source, error) {
	if in.Name != nil {
		n := strings.TrimSpace(*in.Name)
		if n == "" {
			return nil, invalid("name cannot be empty")
		}
		if _, err := s.DB.Exec(ctx, `UPDATE sources SET name=$2 WHERE id=$1`, id, n); err != nil {
			if isUnique(err) {
				return nil, ErrConflict
			}
			return nil, err
		}
	}
	if in.StaleAfterHours != nil {
		h, err := validStale(in.StaleAfterHours)
		if err != nil {
			return nil, err
		}
		if _, err := s.DB.Exec(ctx, `UPDATE sources SET stale_after_hours=$2 WHERE id=$1`, id, h); err != nil {
			return nil, err
		}
	}
	return s.getSource(ctx, id)
}

// RotateSource issues a new token.
func (s *Store) RotateSource(ctx context.Context, id int64) (string, error) {
	tok := NewToken("vg_")
	ct, err := s.DB.Exec(ctx, `UPDATE sources SET token_hash=$2, token_prefix=$3 WHERE id=$1`, id, HashToken(tok), tok[:7])
	if err != nil {
		return "", err
	}
	if ct.RowsAffected() == 0 {
		return "", ErrNotFound
	}
	return tok, nil
}

// DeleteSource removes a source; its scans stay, unattributed.
func (s *Store) DeleteSource(ctx context.Context, id int64) error {
	ct, err := s.DB.Exec(ctx, `DELETE FROM sources WHERE id=$1`, id)
	if err == nil && ct.RowsAffected() == 0 {
		return ErrNotFound
	}
	return err
}

// SourceByToken authenticates an ingest call.
func (s *Store) SourceByToken(ctx context.Context, tok string) (int64, string, error) {
	var id int64
	var name string
	err := s.DB.QueryRow(ctx, `SELECT id, name FROM sources WHERE token_hash=$1`, HashToken(tok)).Scan(&id, &name)
	return id, name, notFound(err)
}

// Project is the API shape.
type Project struct {
	ID     int64  `json:"id"`
	Name   string `json:"name"`
	Assets int    `json:"assets"`
	Open   int    `json:"open"`
}

// ListProjects lists projects with counts.
func (s *Store) ListProjects(ctx context.Context) ([]Project, error) {
	rows, err := s.DB.Query(ctx, `SELECT p.id, p.name,
		(SELECT count(*) FROM assets a WHERE a.project_id=p.id AND a.retired_at IS NULL),
		(SELECT count(*) FROM findings f JOIN assets a ON a.id=f.asset_id WHERE a.project_id=p.id AND f.state = ANY($1))
		FROM projects p ORDER BY p.name`, OpenStates)
	if err != nil {
		return nil, err
	}
	return pgx.CollectRows(rows, func(r pgx.CollectableRow) (Project, error) {
		var p Project
		return p, r.Scan(&p.ID, &p.Name, &p.Assets, &p.Open)
	})
}

// SaveProject creates (id 0) or renames a project.
func (s *Store) SaveProject(ctx context.Context, id int64, name string) (*Project, error) {
	name = strings.TrimSpace(name)
	if name == "" {
		return nil, invalid("name is required")
	}
	var err error
	if id == 0 {
		err = s.DB.QueryRow(ctx, `INSERT INTO projects (name) VALUES ($1) RETURNING id`, name).Scan(&id)
	} else {
		var ct interface{ RowsAffected() int64 }
		ct, err = s.DB.Exec(ctx, `UPDATE projects SET name=$2 WHERE id=$1`, id, name)
		if err == nil && ct.RowsAffected() == 0 {
			return nil, ErrNotFound
		}
	}
	if isUnique(err) {
		return nil, ErrConflict
	}
	if err != nil {
		return nil, err
	}
	all, err := s.ListProjects(ctx)
	if err != nil {
		return nil, err
	}
	for _, p := range all {
		if p.ID == id {
			return &p, nil
		}
	}
	return nil, ErrNotFound
}

// DeleteProject removes a project; assets become unassigned.
func (s *Store) DeleteProject(ctx context.Context, id int64) error {
	ct, err := s.DB.Exec(ctx, `DELETE FROM projects WHERE id=$1`, id)
	if err == nil && ct.RowsAffected() == 0 {
		return ErrNotFound
	}
	return err
}
