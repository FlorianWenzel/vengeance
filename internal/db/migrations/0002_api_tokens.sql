-- +goose Up
CREATE TABLE api_tokens (
  id            BIGSERIAL PRIMARY KEY,
  name          TEXT NOT NULL,
  token_hash    BYTEA NOT NULL UNIQUE,
  token_prefix  TEXT NOT NULL,
  scope         TEXT NOT NULL CHECK (scope IN ('read', 'write')),
  user_id       BIGINT REFERENCES users(id) ON DELETE CASCADE,
  created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
  last_used_at  TIMESTAMPTZ
);

-- +goose Down
DROP TABLE api_tokens;
