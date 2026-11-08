-- +goose Up
CREATE TABLE users (
  id          BIGSERIAL PRIMARY KEY,
  subject     TEXT NOT NULL UNIQUE,
  email       TEXT NOT NULL,
  name        TEXT NOT NULL DEFAULT '',
  created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
  last_login  TIMESTAMPTZ
);

CREATE TABLE sessions (
  token_hash  BYTEA PRIMARY KEY,
  user_id     BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
  expires_at  TIMESTAMPTZ NOT NULL
);
CREATE INDEX sessions_expires ON sessions (expires_at);

CREATE TABLE projects (
  id          BIGSERIAL PRIMARY KEY,
  name        TEXT NOT NULL UNIQUE,
  created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE sources (
  id                 BIGSERIAL PRIMARY KEY,
  name               TEXT NOT NULL UNIQUE,
  token_hash         BYTEA NOT NULL UNIQUE,
  token_prefix       TEXT NOT NULL,
  stale_after_hours  INT NOT NULL DEFAULT 48,
  last_seen_at       TIMESTAMPTZ,
  created_at         TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE assets (
  id           BIGSERIAL PRIMARY KEY,
  kind         TEXT NOT NULL,
  identifier   TEXT NOT NULL,
  name         TEXT NOT NULL,
  project_id   BIGINT REFERENCES projects(id) ON DELETE SET NULL,
  tags         TEXT[] NOT NULL DEFAULT '{}',
  discovered   BOOLEAN NOT NULL DEFAULT false,
  retired_at   TIMESTAMPTZ,
  last_scan_at TIMESTAMPTZ,
  created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE (kind, identifier)
);

CREATE TABLE scans (
  id            BIGSERIAL PRIMARY KEY,
  source_id     BIGINT REFERENCES sources(id) ON DELETE SET NULL,
  asset_id      BIGINT NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
  format        TEXT NOT NULL,
  scan_types    TEXT[] NOT NULL,
  digest        TEXT,
  finding_count INT NOT NULL DEFAULT 0,
  raw           BYTEA,
  received_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX scans_asset ON scans (asset_id, received_at DESC);

CREATE TABLE advisories (
  id            TEXT PRIMARY KEY,
  title         TEXT NOT NULL DEFAULT '',
  description   TEXT NOT NULL DEFAULT '',
  severity      TEXT NOT NULL DEFAULT 'unknown',
  cvss          REAL,
  published_at  TIMESTAMPTZ,
  refs          TEXT[] NOT NULL DEFAULT '{}',
  kev           BOOLEAN NOT NULL DEFAULT false,
  kev_added     DATE,
  updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE findings (
  id                 BIGSERIAL PRIMARY KEY,
  asset_id           BIGINT NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
  fingerprint        TEXT NOT NULL,
  kind               TEXT NOT NULL,
  advisory_id        TEXT NOT NULL,
  title              TEXT NOT NULL DEFAULT '',
  severity           TEXT NOT NULL DEFAULT 'unknown',
  priority           SMALLINT NOT NULL DEFAULT 4,
  pkg_name           TEXT,
  pkg_ecosystem      TEXT,
  installed_version  TEXT,
  fixed_version      TEXT,
  target             TEXT NOT NULL DEFAULT '',
  state              TEXT NOT NULL DEFAULT 'new',
  state_reason       TEXT,
  accepted_until     DATE,
  first_seen         TIMESTAMPTZ NOT NULL DEFAULT now(),
  last_seen          TIMESTAMPTZ NOT NULL DEFAULT now(),
  fixed_at           TIMESTAMPTZ,
  UNIQUE (asset_id, fingerprint)
);
CREATE INDEX findings_state ON findings (state, priority);
CREATE INDEX findings_advisory ON findings (advisory_id);

CREATE TABLE finding_sources (
  finding_id   BIGINT NOT NULL REFERENCES findings(id) ON DELETE CASCADE,
  source_id    BIGINT NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
  last_seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  PRIMARY KEY (finding_id, source_id)
);

CREATE TABLE finding_events (
  id          BIGSERIAL PRIMARY KEY,
  finding_id  BIGINT NOT NULL REFERENCES findings(id) ON DELETE CASCADE,
  at          TIMESTAMPTZ NOT NULL DEFAULT now(),
  actor       TEXT NOT NULL,
  from_state  TEXT,
  to_state    TEXT NOT NULL,
  note        TEXT
);
CREATE INDEX finding_events_finding ON finding_events (finding_id, at);

CREATE TABLE expectations (
  kind          TEXT NOT NULL,
  scan_type     TEXT NOT NULL,
  max_age_days  INT NOT NULL,
  PRIMARY KEY (kind, scan_type)
);
INSERT INTO expectations (kind, scan_type, max_age_days) VALUES
  ('repository', 'vuln', 7), ('repository', 'secret', 7), ('repository', 'misconfig', 7),
  ('image', 'vuln', 7), ('image', 'secret', 7),
  ('host', 'vuln', 7),
  ('cluster', 'vuln', 7), ('cluster', 'misconfig', 7);

CREATE TABLE channels (
  id          BIGSERIAL PRIMARY KEY,
  name        TEXT NOT NULL,
  kind        TEXT NOT NULL,
  config      JSONB NOT NULL DEFAULT '{}',
  created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE rules (
  id          BIGSERIAL PRIMARY KEY,
  name        TEXT NOT NULL,
  trigger     TEXT NOT NULL,
  filter      JSONB NOT NULL DEFAULT '{}',
  mode        TEXT NOT NULL DEFAULT 'instant',
  channel_id  BIGINT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
  enabled     BOOLEAN NOT NULL DEFAULT true,
  created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE notifications (
  id          BIGSERIAL PRIMARY KEY,
  rule_id     BIGINT REFERENCES rules(id) ON DELETE CASCADE,
  channel_id  BIGINT REFERENCES channels(id) ON DELETE CASCADE,
  dedupe_key  TEXT NOT NULL,
  subject     TEXT NOT NULL,
  body        TEXT NOT NULL,
  data        JSONB NOT NULL DEFAULT '{}',
  status      TEXT NOT NULL DEFAULT 'pending',
  attempts    INT NOT NULL DEFAULT 0,
  error       TEXT,
  created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
  sent_at     TIMESTAMPTZ,
  UNIQUE (rule_id, dedupe_key)
);
CREATE INDEX notifications_status ON notifications (status, created_at);

CREATE TABLE kev (
  cve_id      TEXT PRIMARY KEY,
  date_added  DATE NOT NULL
);

CREATE TABLE kv (
  key    TEXT PRIMARY KEY,
  value  TEXT NOT NULL,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- +goose Down
DROP TABLE kv, kev, notifications, rules, channels, expectations, finding_events, finding_sources,
  findings, advisories, scans, assets, sources, projects, sessions, users;
