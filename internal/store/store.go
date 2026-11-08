// Package store holds all database access and the domain operations built on it.
package store

import (
	"context"
	"errors"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgconn"
	"github.com/jackc/pgx/v5/pgxpool"
)

// Store wraps the pool.
type Store struct {
	DB *pgxpool.Pool
	// Now is overridable in tests.
	Now func() time.Time
}

// New returns a Store.
func New(pool *pgxpool.Pool) *Store { return &Store{DB: pool, Now: time.Now} }

// Errors the HTTP layer maps to status codes.
var (
	ErrNotFound = errors.New("not found")
	ErrConflict = errors.New("already exists")
)

// Invalid is a validation error with a user-facing message.
type Invalid struct{ Msg string }

func (e Invalid) Error() string { return e.Msg }

func invalid(msg string) error { return Invalid{msg} }

func isUnique(err error) bool {
	var pe *pgconn.PgError
	return errors.As(err, &pe) && pe.Code == "23505"
}

func notFound(err error) error {
	if errors.Is(err, pgx.ErrNoRows) {
		return ErrNotFound
	}
	return err
}

func (s *Store) tx(ctx context.Context, fn func(pgx.Tx) error) error {
	return pgx.BeginFunc(ctx, s.DB, fn)
}

// KVGet reads a key; "" when missing.
func (s *Store) KVGet(ctx context.Context, key string) string {
	var v string
	_ = s.DB.QueryRow(ctx, `SELECT value FROM kv WHERE key=$1`, key).Scan(&v)
	return v
}

// KVSet writes a key.
func (s *Store) KVSet(ctx context.Context, key, value string) error {
	_, err := s.DB.Exec(ctx, `INSERT INTO kv (key,value,updated_at) VALUES ($1,$2,now())
		ON CONFLICT (key) DO UPDATE SET value=EXCLUDED.value, updated_at=now()`, key, value)
	return err
}

// Counts is open findings by priority.
type Counts struct {
	P1    int `json:"p1"`
	P2    int `json:"p2"`
	P3    int `json:"p3"`
	P4    int `json:"p4"`
	Total int `json:"total"`
}

func (c *Counts) add(p, n int) {
	switch p {
	case 1:
		c.P1 += n
	case 2:
		c.P2 += n
	case 3:
		c.P3 += n
	default:
		c.P4 += n
	}
	c.Total += n
}

// OpenStates are the states counted as open.
var OpenStates = []string{"new", "acknowledged"}
