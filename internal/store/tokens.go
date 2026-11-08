package store

import (
	"context"
	"strings"
	"time"

	"github.com/jackc/pgx/v5"
)

// APIToken lets a person's tools (an MCP client, scripts) use Vengeance on their behalf.
type APIToken struct {
	ID          int64      `json:"id"`
	Name        string     `json:"name"`
	TokenPrefix string     `json:"tokenPrefix"`
	Scope       string     `json:"scope"`
	CreatedBy   *string    `json:"createdBy"`
	CreatedAt   time.Time  `json:"createdAt"`
	LastUsedAt  *time.Time `json:"lastUsedAt"`
	Token       string     `json:"token,omitempty"`
}

// ListAPITokens lists tokens (never their secrets).
func (s *Store) ListAPITokens(ctx context.Context) ([]APIToken, error) {
	rows, err := s.DB.Query(ctx, `SELECT t.id, t.name, t.token_prefix, t.scope, u.email, t.created_at, t.last_used_at
		FROM api_tokens t LEFT JOIN users u ON u.id=t.user_id ORDER BY t.created_at DESC, t.id DESC`)
	if err != nil {
		return nil, err
	}
	return pgx.CollectRows(rows, func(r pgx.CollectableRow) (APIToken, error) {
		var t APIToken
		return t, r.Scan(&t.ID, &t.Name, &t.TokenPrefix, &t.Scope, &t.CreatedBy, &t.CreatedAt, &t.LastUsedAt)
	})
}

// CreateAPIToken returns the new token with its plaintext value (shown once).
func (s *Store) CreateAPIToken(ctx context.Context, userID int64, name, scope string) (*APIToken, error) {
	name = strings.TrimSpace(name)
	if name == "" {
		return nil, invalid("name is required")
	}
	if scope != "read" && scope != "write" {
		return nil, invalid("scope must be read or write")
	}
	tok := NewToken("vgp_")
	var id int64
	if err := s.DB.QueryRow(ctx, `INSERT INTO api_tokens (name, token_hash, token_prefix, scope, user_id) VALUES ($1,$2,$3,$4,$5) RETURNING id`,
		name, HashToken(tok), tok[:8], scope, userID).Scan(&id); err != nil {
		return nil, err
	}
	all, err := s.ListAPITokens(ctx)
	if err != nil {
		return nil, err
	}
	for _, t := range all {
		if t.ID == id {
			t.Token = tok
			return &t, nil
		}
	}
	return nil, ErrNotFound
}

// DeleteAPIToken revokes a token.
func (s *Store) DeleteAPIToken(ctx context.Context, id int64) error {
	ct, err := s.DB.Exec(ctx, `DELETE FROM api_tokens WHERE id=$1`, id)
	if err == nil && ct.RowsAffected() == 0 {
		return ErrNotFound
	}
	return err
}

// APITokenByValue authenticates a bearer token and records its use.
func (s *Store) APITokenByValue(ctx context.Context, tok string) (*APIToken, error) {
	var t APIToken
	err := s.DB.QueryRow(ctx, `UPDATE api_tokens SET last_used_at=$2 WHERE token_hash=$1
		RETURNING id, name, token_prefix, scope, created_at, last_used_at`, HashToken(tok), s.Now()).
		Scan(&t.ID, &t.Name, &t.TokenPrefix, &t.Scope, &t.CreatedAt, &t.LastUsedAt)
	if err != nil {
		return nil, notFound(err)
	}
	return &t, nil
}

// ProjectIDByName finds or creates a project.
func (s *Store) ProjectIDByName(ctx context.Context, name string) (int64, error) {
	name = strings.TrimSpace(name)
	if name == "" {
		return 0, invalid("project name is empty")
	}
	var id int64
	err := s.DB.QueryRow(ctx, `INSERT INTO projects (name) VALUES ($1) ON CONFLICT (name) DO UPDATE SET name=EXCLUDED.name RETURNING id`, name).Scan(&id)
	return id, err
}
