# Vengeance HTTP API (v1)

All JSON. camelCase fields. Timestamps are RFC 3339 strings (UTC), nullable ones are `null` when unset.

## Auth

The web UI uses a session cookie (`vg_session`, HttpOnly, SameSite=Lax) set after OIDC login.

- `GET /auth/login?next=/path` → 302 to the OIDC provider.
- `GET /auth/callback` → sets cookie, 302 to `next` (default `/`).
- `POST /auth/logout` → clears the session, `204`.
- `GET /api/v1/me` → `{ "id", "email", "name" }` or `401`.

**CSRF:** every mutating `/api/v1/*` request from the UI (POST, PUT, PATCH, DELETE) must send the header
`X-Vengeance-CSRF: 1`. Missing header → `403`.

Unauthenticated `/api/v1/*` calls return `401 {"error":"unauthenticated"}`; the UI then navigates to
`/auth/login?next=<current path>`.

Errors always look like `{ "error": "human readable message" }` with a 4xx/5xx status.

## Ingest (CI and cron, token auth)

`POST /api/v1/ingest` with `Authorization: Bearer vg_…` (a source token).

Query parameters:

| name | required | meaning |
|---|---|---|
| `format` | yes | `trivy` or `sarif` |
| `asset` | no | `kind:identifier`, e.g. `image:ghcr.io/x/api`, `repository:github.com/x/y`, `host:node1`, `cluster:prod`. Derived from the Trivy report if omitted. |
| `name` | no | display name for a newly created asset |
| `project` | no | project name; created if missing; only applied when the asset is new |
| `scanners` | no | comma list of `vuln,secret,misconfig,sast`. Inferred from the report if omitted. |
| `digest` | no | image digest that was scanned |

Body: the raw report. Response `200`:

```json
{ "scanId": 12, "assetId": 3, "new": 4, "reopened": 0, "fixed": 2, "open": 31 }
```

A Trivy `k8s` report (`trivy k8s -f json`) creates one finding target per workload on the given cluster asset.

## Types

```ts
type Priority = 1 | 2 | 3 | 4            // 1 = most urgent
type State = 'new' | 'acknowledged' | 'fixed' | 'accepted' | 'false_positive'
type Kind = 'vuln' | 'secret' | 'misconfig' | 'sast'
type AssetKind = 'repository' | 'image' | 'host' | 'cluster' | 'domain' | 'registry'
type ScanType = 'vuln' | 'secret' | 'misconfig' | 'sast'

interface Finding {
  id: number
  assetId: number; assetName: string; assetKind: AssetKind; projectName: string | null
  kind: Kind
  advisoryId: string          // CVE-…, GHSA-…, rule id for secrets/misconfig
  title: string
  severity: 'critical' | 'high' | 'medium' | 'low' | 'unknown'
  priority: Priority
  pkgName: string | null; pkgEcosystem: string | null
  installedVersion: string | null; fixedVersion: string | null
  target: string              // file, layer or workload the finding is in
  state: State; stateReason: string | null; acceptedUntil: string | null
  firstSeen: string; lastSeen: string; fixedAt: string | null
  kev: boolean; cvss: number | null
  sources: string[]           // names of sources currently reporting it
}

interface FixGroup {
  key: string
  assetId: number; assetName: string
  pkgName: string; ecosystem: string
  installedVersion: string; targetVersion: string
  command: string             // copy-ready, e.g. "npm install multer@2.5.0"
  findings: number; maxPriority: Priority; kev: boolean
  advisories: string[]
}
```

## Overview

`GET /api/v1/overview`

```json
{
  "open": { "p1": 1, "p2": 4, "p3": 20, "p4": 9, "total": 34 },
  "kev": 1,
  "trend":  [ { "date": "2026-08-10", "open": 40 } ],         // 90 daily points, oldest first
  "weekly": [ { "week": "2026-W33", "new": 5, "fixed": 3 } ], // 12 ISO weeks, oldest first
  "queue": [ FixGroup ],                                      // top 5 by priority
  "coverage": { "score": 0.72, "gaps": 6, "assets": 14 },
  "staleSources": 0
}
```

## Findings

`GET /api/v1/findings` query: `state` (`open` = new+acknowledged, default; `all`; or one State),
`priority` (comma list), `asset` (id), `project` (id), `kind`, `q` (search in advisory id, title, package),
`sort` (`priority` default, `lastSeen`, `firstSeen`), `limit` (default 50, max 500), `offset`.
→ `{ "items": Finding[], "total": number }`

`GET /api/v1/findings/{id}` → `Finding & { description: string, references: string[], publishedAt: string|null,
events: [{ at, actor, fromState, toState, note }] }`

`POST /api/v1/findings/triage` body `{ ids: number[], state: State, reason?: string, until?: "YYYY-MM-DD" }`
→ `{ updated: number }`. Rules: `accepted` requires `until` (future date); `false_positive` requires `reason`;
`fixed` cannot be set by hand; `new` reopens.

## Fixes

`GET /api/v1/fixes?project=ID` → `FixGroup[]`, sorted by maxPriority then findings desc.

## Assets (inventory)

`GET /api/v1/assets?project=&kind=&retired=true` →

```ts
interface Asset {
  id: number; kind: AssetKind; identifier: string; name: string
  projectId: number | null; projectName: string | null
  tags: string[]; retiredAt: string | null; createdAt: string; lastScanAt: string | null
  discovered: boolean          // created by an ingest rather than by hand
  open: { p1: number; p2: number; p3: number; p4: number; total: number }
  coverage: { scanType: ScanType; lastAt: string | null; maxAgeDays: number; ok: boolean }[]
  covered: boolean             // every expected scan type is fresh
}
```

- `POST /api/v1/assets` `{ kind, identifier, name?, projectId?, tags? }` → Asset (`409` if kind+identifier exists)
- `PATCH /api/v1/assets/{id}` `{ name?, projectId?, tags?, retired?: boolean }` → Asset
- `DELETE /api/v1/assets/{id}` → `204` (deletes its findings and scans)
- `GET /api/v1/assets/{id}` → `Asset & { scans: [{ id, sourceName, format, scanTypes, digest, receivedAt, findingCount }] }`

Tag `internet-facing` raises priority (see Prioritisation).

## Projects

- `GET /api/v1/projects` → `[{ id, name, assets, open }]`
- `POST /api/v1/projects` `{ name }`, `PATCH /api/v1/projects/{id}` `{ name }`, `DELETE /api/v1/projects/{id}` (assets keep existing, unassigned)

## Coverage

`GET /api/v1/coverage` →

```json
{
  "score": 0.72,
  "byProject": [ { "projectId": 1, "name": "slowbloom", "score": 0.8, "assets": 10, "gaps": 2 } ],
  "gaps": [ { "assetId": 3, "assetName": "ghost", "assetKind": "image", "scanType": "secret", "lastAt": null, "maxAgeDays": 7 } ]
}
```

Score = fresh (asset, expected scan type) pairs / all pairs, over non-retired assets. `projectId` null = unassigned.

- `GET /api/v1/expectations` → `[{ kind: AssetKind, scanType: ScanType, maxAgeDays: number }]`
- `PUT /api/v1/expectations` body: the full list → same list

## Sources

- `GET /api/v1/sources` → `[{ id, name, tokenPrefix, lastSeenAt, staleAfterHours, stale, scans, createdAt }]`
- `POST /api/v1/sources` `{ name, staleAfterHours? (default 48) }` → source `& { token }` (only time the token is shown)
- `PATCH /api/v1/sources/{id}` `{ name?, staleAfterHours? }`
- `POST /api/v1/sources/{id}/rotate` → `{ token }`
- `DELETE /api/v1/sources/{id}`

## Notifications

Channels:

- `GET /api/v1/channels` → `[{ id, name, kind: 'email'|'ntfy'|'webhook', config, createdAt }]`
  config: email `{ to }`, ntfy `{ server, topic, token }`, webhook `{ url, secret }`. Secrets (`token`, `secret`)
  come back as `"••••"` when set; sending `"••••"` back on PATCH keeps the stored value.
- `POST /api/v1/channels` `{ name, kind, config }`, `PATCH /api/v1/channels/{id}`, `DELETE /api/v1/channels/{id}`
- `POST /api/v1/channels/{id}/test` → `{ ok: true }` or `502 { error }`

Rules:

- `GET /api/v1/rules` → `[{ id, name, trigger, filter, mode, channelId, channelName, enabled, createdAt }]`
  - trigger: `new_finding` | `risk_increase` | `stale_source` | `accepted_expired` | `coverage_gap`
  - filter: `{ maxPriority: 1-4, projectId: number|null, tags: string[], fixAvailable: boolean|null }`
    (`maxPriority: 2` means P1 and P2; filters other than projectId/tags are ignored for `stale_source`)
  - mode: `instant` | `daily` | `weekly` (digests go out 08:00 UTC; weekly on Mondays)
- `POST /api/v1/rules`, `PATCH /api/v1/rules/{id}`, `DELETE /api/v1/rules/{id}`

Log: `GET /api/v1/notifications?limit=50` →
`[{ id, ruleName, channelName, subject, status: 'pending'|'digest'|'sent'|'failed', createdAt, sentAt, error }]`

## API tokens and MCP

Personal tokens for AI assistants and scripts (`vgp_…`, stored hashed):

- `GET /api/v1/tokens` → `[{ id, name, tokenPrefix, scope: 'read'|'write', createdBy, createdAt, lastUsedAt }]`
- `POST /api/v1/tokens` `{ name, scope }` → token `& { token }` (only time it is shown)
- `DELETE /api/v1/tokens/{id}` → `204`

`/mcp` is a stateless streamable-HTTP MCP endpoint authenticated with `Authorization: Bearer vgp_…`.
Read tokens see only read tools; write tokens add `triage_findings`, `create_asset` and `update_asset`.
Triage done over MCP is recorded with actor `mcp:<token name>`.

## Prioritisation

- P1: advisory is in CISA KEV, or severity critical with a fix on an asset tagged `internet-facing`
- P2: critical or high with a fix
- P3: anything else with a fix (medium/low/unknown), and every secret finding of medium or below
- P4: no fix available

Secrets: critical/high secrets are P1 regardless of fix. Misconfigurations: critical/high = P2, else P3.

## Health

`GET /healthz` (process up), `GET /readyz` (database reachable).
