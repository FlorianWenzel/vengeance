package store

import (
	"context"
	"time"
)

// User is the signed-in person.
type User struct {
	ID    int64  `json:"id"`
	Email string `json:"email"`
	Name  string `json:"name"`
}

// UpsertUser records a login.
func (s *Store) UpsertUser(ctx context.Context, subject, email, name string) (*User, error) {
	u := &User{}
	err := s.DB.QueryRow(ctx, `INSERT INTO users (subject, email, name, last_login) VALUES ($1,$2,$3,now())
		ON CONFLICT (subject) DO UPDATE SET email=EXCLUDED.email, name=EXCLUDED.name, last_login=now()
		RETURNING id, email, name`, subject, email, name).Scan(&u.ID, &u.Email, &u.Name)
	return u, err
}

// CreateSession returns a new session token.
func (s *Store) CreateSession(ctx context.Context, userID int64, ttl time.Duration) (string, error) {
	tok := NewToken("")
	_, err := s.DB.Exec(ctx, `INSERT INTO sessions (token_hash, user_id, expires_at) VALUES ($1,$2,$3)`, HashToken(tok), userID, s.Now().Add(ttl))
	return tok, err
}

// SessionUser resolves a session token.
func (s *Store) SessionUser(ctx context.Context, tok string) (*User, error) {
	u := &User{}
	err := s.DB.QueryRow(ctx, `SELECT u.id, u.email, u.name FROM sessions s JOIN users u ON u.id=s.user_id
		WHERE s.token_hash=$1 AND s.expires_at > $2`, HashToken(tok), s.Now()).Scan(&u.ID, &u.Email, &u.Name)
	if err != nil {
		return nil, notFound(err)
	}
	return u, nil
}

// DeleteSession logs out.
func (s *Store) DeleteSession(ctx context.Context, tok string) error {
	_, err := s.DB.Exec(ctx, `DELETE FROM sessions WHERE token_hash=$1`, HashToken(tok))
	return err
}

// PruneSessions removes expired sessions.
func (s *Store) PruneSessions(ctx context.Context) error {
	_, err := s.DB.Exec(ctx, `DELETE FROM sessions WHERE expires_at < $1`, s.Now())
	return err
}
