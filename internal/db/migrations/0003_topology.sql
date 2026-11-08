-- +goose Up
CREATE TABLE topology (
  cluster      TEXT PRIMARY KEY,
  source_id    BIGINT REFERENCES sources(id) ON DELETE SET NULL,
  payload      JSONB NOT NULL,
  received_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- +goose Down
DROP TABLE topology;
