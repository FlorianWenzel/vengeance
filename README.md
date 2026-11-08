# Vengeance

**A self-hosted security inbox for people who run their own stuff.**

Your scanners already know what is wrong. Vengeance listens to all of them, merges what they
report into one list per app, ranks it by what attackers are actually exploiting, tells you which
upgrade fixes it, and only wakes you up when something needs doing. It also keeps an inventory of
what you run, so it can tell you what *nobody* is scanning.

One container and a Postgres database. AGPL-3.0.

## What it does

- **Ingest** Trivy JSON (image, fs, repo, rootfs, k8s) and SARIF 2.1 from any tool (Semgrep,
  CodeQL, gosec, Checkov, …) through one HTTP endpoint or the `vengeance push` CLI.
- **Deduplicate.** The same CVE in the same package of the same asset is one finding, however many
  scanners report it and however often. A version bump that is still vulnerable keeps its history.
- **Auto-close carefully.** A finding is marked fixed only when the source that reported it rescans
  the same asset, with the same kind of scan, and no longer reports it. It reopens if it comes back.
- **Prioritise.** P1 to P4 from severity, fix availability, [CISA KEV](https://www.cisa.gov/known-exploited-vulnerabilities-catalog)
  (synced daily) and whether the asset is tagged `internet-facing`.
- **Fix, not just find.** Findings are grouped by the upgrade that resolves them, with a
  copy-ready command for npm, Go, pip, cargo, apk, apt and more.
- **Triage.** New → acknowledged → fixed, or accepted risk (always with an end date), or false
  positive (always with a reason). Every change is in the finding's history.
- **Coverage and gaps.** An inventory of repositories, images, hosts, clusters and domains, the
  scans each kind is expected to get (for example "images need vulnerability and secret scans every
  7 days"), and a list of everything that is missing or stale.
- **Notifications** by email, [ntfy](https://ntfy.sh) or signed webhook, with rules for new
  findings, findings that became riskier, sources that went quiet, expired risk acceptances and
  coverage gaps. Instantly or as a daily or weekly digest. A nightly rescan never alerts twice.

## Quick start (local)

```bash
echo "127.0.0.1 dex" | sudo tee -a /etc/hosts   # the dev OIDC provider must resolve the same for browser and server
docker compose up -d --build
open http://localhost:8080                       # sign in as admin@example.com / password
```

Create a source under **Sources**, copy its token, then push a report:

```bash
trivy image -f json -o report.json alpine:3.18
VENGEANCE_URL=http://localhost:8080 VENGEANCE_TOKEN=vg_… vengeance push report.json
# or without the CLI:
curl -X POST "http://localhost:8080/api/v1/ingest?format=trivy" \
  -H "Authorization: Bearer vg_…" --data-binary @report.json
```

Mail sent by the dev stack lands in Mailpit at http://localhost:8025.

## Scanning recipes

```bash
# a repository (vulnerabilities, secrets, misconfigurations)
trivy fs --scanners vuln,secret,misconfig -f json -o repo.json .
vengeance push --scanners vuln,secret,misconfig repo.json        # asset guessed from the git remote

# an image, by digest
trivy image --scanners vuln,secret -f json -o img.json ghcr.io/me/api@sha256:…
vengeance push --asset image:ghcr.io/me/api img.json

# a host
trivy rootfs --scanners vuln -f json -o host.json /
vengeance push --asset host:$(hostname) host.json

# a Kubernetes cluster
trivy k8s --report all -f json -o k8s.json
vengeance push --asset cluster:prod --scanners vuln,misconfig k8s.json
```

Pass `--scanners` whenever you can: auto-close only touches finding kinds the scan claims to cover.

## Configuration

| Variable | Required | Meaning |
|---|---|---|
| `VENGEANCE_DATABASE_URL` | yes | Postgres URL |
| `VENGEANCE_BASE_URL` | yes | public URL, e.g. `https://vengeance.example.com` |
| `VENGEANCE_SECRET` | yes | 32+ random characters, signs the login flow |
| `VENGEANCE_OIDC_ISSUER` | yes | OIDC issuer (Dex, Authelia, Keycloak, Pocket ID, Google, …) |
| `VENGEANCE_OIDC_CLIENT_ID` / `_CLIENT_SECRET` | yes | OIDC client; redirect URI is `$BASE_URL/auth/callback` |
| `VENGEANCE_OIDC_ALLOWED_EMAILS` / `_ALLOWED_DOMAINS` | one of | who may sign in (comma separated) |
| `VENGEANCE_OIDC_ALLOW_ALL` | | `true` if the provider already restricts who can sign in |
| `VENGEANCE_SMTP_HOST`, `_PORT`, `_USER`, `_PASSWORD`, `_FROM`, `_TLS` | for email | `_TLS` is `starttls` (default), `tls` or `none` |
| `VENGEANCE_KEV_URL` | | defaults to the CISA feed; `off` disables the daily sync |
| `VENGEANCE_LISTEN` | | default `:8080` |
| `VENGEANCE_MAX_REPORT_MIB` | | upload limit, default 64 |

Sign-in is OIDC only; there are no local passwords. Ingest uses per-source bearer tokens, stored hashed.

## Development

```bash
docker compose up -d postgres dex mailpit
cd web && npm ci && npm run dev                  # UI on :5173, proxies the API to :8080
VENGEANCE_DATABASE_URL=postgres://vengeance:vengeance@localhost:5433/vengeance?sslmode=disable \
VENGEANCE_SECRET=dev-only-secret-0123456789abcdef0123456789 VENGEANCE_OIDC_ISSUER=http://dex:5556/dex \
VENGEANCE_OIDC_CLIENT_ID=vengeance VENGEANCE_OIDC_CLIENT_SECRET=dev-secret \
VENGEANCE_OIDC_ALLOWED_EMAILS=admin@example.com go run ./cmd/vengeance serve

# tests
VENGEANCE_TEST_DATABASE_URL=postgres://vengeance:vengeance@localhost:5433/vengeance?sslmode=disable go test -p 1 ./...
docker compose up -d --build && (cd e2e && npm ci && npx playwright test)
```

The HTTP API is documented in [docs/api.md](docs/api.md).

## License

[AGPL-3.0](LICENSE). If you run a modified version for others, share your changes.
