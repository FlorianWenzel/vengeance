<div align="center">

<img src="docs/logo.svg" width="96" alt="Vengeance logo">

# Vengeance

**The security inbox for people who run their own stuff.**

Every scanner you already run reports in. Duplicates merge, fixed findings close themselves,
what attackers actually exploit rises to the top, and you get one message when something needs doing,
with the upgrade that fixes it.

[![CI](https://github.com/FlorianWenzel/vengeance/actions/workflows/ci.yml/badge.svg)](https://github.com/FlorianWenzel/vengeance/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/FlorianWenzel/vengeance?color=f2b04a&label=release)](https://github.com/FlorianWenzel/vengeance/releases)
[![Go](https://img.shields.io/github/go-mod/go-version/FlorianWenzel/vengeance?color=00ADD8&logo=go&logoColor=white)](go.mod)
[![License: AGPL-3.0](https://img.shields.io/badge/license-AGPL--3.0-blue)](LICENSE)
[![Image](https://img.shields.io/badge/ghcr.io-florianwenzel%2Fvengeance-2496ED?logo=docker&logoColor=white)](https://github.com/FlorianWenzel/vengeance/pkgs/container/vengeance)
[![Signed](https://img.shields.io/badge/images-cosign%20signed%20%2B%20SBOM-6b46c1)](#security)
[![MCP](https://img.shields.io/badge/MCP-server-111)](#talk-to-it-from-claude-mcp)

[Quick start](#quick-start) · [Features](#features) · [Feed it](#feed-it) · [Deploy](#deploy) · [Docs](docs/)

<br>

<img src="docs/screenshots/overview.webp" alt="Vengeance overview: open findings by priority, 90-day trend, new vs fixed per week, the fix-this-week queue and scan coverage" width="100%">

</div>

---

## Why

You already run Trivy in CI. Maybe Dependabot. Maybe something on the cluster. Each one reports
somewhere else, in its own format; the same CVE shows up three times; and nobody notices when the
nightly scan quietly stops. The tools that aggregate all this were built for security teams running
audits, not for someone with a few repos and a server who wants to know **what to upgrade this week.**

Vengeance is that answer, in one container and a Postgres database.

## Features

<table>
<tr>
<td width="50%" valign="top">

### One inbox, no duplicates
Trivy (image, fs, repo, rootfs, k8s) and SARIF from any tool (Semgrep, CodeQL, gosec, Checkov…)
land in one list. The same CVE in the same package of the same asset is **one finding**, no matter how
many scanners or rescans report it. A rescan that no longer sees it **closes it**; if it comes back,
it reopens with its history.

</td>
<td width="50%"><img src="docs/screenshots/findings.webp" alt="Findings inbox with a finding's details open"></td>
</tr>
<tr>
<td><img src="docs/screenshots/fixes.webp" alt="Fixes grouped by upgrade with copy-ready commands"></td>
<td valign="top">

### Fixes, not just findings
Findings are grouped by **the upgrade that resolves them**: "bump libexpat to 2.7.0, closes 5 findings"
is one task. Every group comes with a copy-ready command for npm, Go, pip, cargo, apk, apt and more.
Priorities P1–P4 combine severity, fix availability, exposure (`internet-facing`) and
[CISA KEV](https://www.cisa.gov/known-exploited-vulnerabilities-catalog), synced daily.

</td>
</tr>
<tr>
<td valign="top">

### Know what nobody is scanning
Keep an inventory of repositories, images, hosts, clusters and domains, say what each kind needs
("images: vulnerability and secret scans every 7 days"), and see every gap. Scanners can't tell you
about the things they were never pointed at; Vengeance can.

</td>
<td><img src="docs/screenshots/coverage.webp" alt="Coverage score per project and list of gaps"></td>
</tr>
<tr>
<td><img src="docs/screenshots/blueprint-zoom.webp" alt="3D blueprint zoomed into a namespace showing workloads and images with status LEDs"></td>
<td valign="top">

### See your infrastructure
The **Blueprint** draws your servers as racks, apps as rack units and containers as chips, each with
a status light for its worst open finding. Zoom in, click anything, jump to its findings.
Rendered in Rust → WebAssembly → WebGL2, ~150 KB over the wire.

</td>
</tr>
<tr>
<td valign="top">

### …or walk through it
**Walk mode** turns the same data into a noir city at night. Servers are buildings, apps are floors,
workloads are windows. P1s are **fires**, P2s **broken glass**, misconfigurations **open windows**,
leaked secrets **keys left in the lock**, unscanned blocks have their **street lamps out**. Your repos
are books in the library, your images crates in the harbour, your scanners searchlights in the sky.
Aim at anything and it tells you what's wrong.

</td>
<td><img src="docs/screenshots/walk-facade.webp" alt="Walk mode: a building facade at night with a window on fire, broken panes and neon doors for domains"></td>
</tr>
<tr>
<td><img src="docs/screenshots/mcp.webp" alt="AI assistants page with MCP endpoint and tokens"></td>
<td valign="top">

### Talk to it from Claude (MCP)
Vengeance is an MCP server. Ask *"what should I patch first this week?"*, *"which images in the
cluster are known-exploited?"*, or pair it with a GitHub MCP: *"compare my repos with the inventory
and add the missing ones."* Read-only and read-write tokens, revocable per assistant.

</td>
</tr>
</table>

**And:** triage with states that mean something (accepted risk *always* has an end date, false
positives *always* have a reason), notifications by **email, [ntfy](https://ntfy.sh) or signed
webhook** with instant or digest delivery, a "*your nightly scan went quiet*" alarm, and OIDC sign-in.

<p align="center"><img src="docs/screenshots/walk.webp" alt="Walk mode: arriving on the square at night, looking down the main street" width="85%"></p>

## Quick start

Try it locally in two minutes with Docker. The compose file brings Postgres, a dev OIDC provider
(Dex) and Mailpit to catch email.

```bash
git clone https://github.com/FlorianWenzel/vengeance && cd vengeance
echo "127.0.0.1 dex" | sudo tee -a /etc/hosts   # the dev OIDC issuer must resolve the same for browser and server
docker compose up -d --build
open http://localhost:8080                       # sign in as admin@example.com / password
```

Create a source under **Sources**, then push your first report:

```bash
trivy image -f json -o report.json alpine:3.18
curl -X POST "http://localhost:8080/api/v1/ingest?format=trivy" \
  -H "Authorization: Bearer vg_…" --data-binary @report.json
```

## Feed it

Any scanner that writes Trivy JSON or SARIF works. The `vengeance push` CLI (in every
[release](https://github.com/FlorianWenzel/vengeance/releases) and in the image) guesses the asset for you.

```bash
export VENGEANCE_URL=https://vengeance.example.com VENGEANCE_TOKEN=vg_…

# a repository: dependencies, secrets, IaC
trivy fs --scanners vuln,secret,misconfig -f json -o repo.json . && vengeance push --scanners vuln,secret,misconfig repo.json
# an image, by digest
trivy image --scanners vuln,secret -f json -o img.json ghcr.io/me/api@sha256:… && vengeance push --asset image:ghcr.io/me/api img.json
# a host
trivy rootfs --scanners vuln -f json -o host.json / && vengeance push --asset host:$(hostname) host.json
# anything SARIF
semgrep --sarif -o semgrep.sarif && vengeance push --format sarif semgrep.sarif
```

<details>
<summary><b>GitHub Actions</b></summary>

```yaml
- name: Scan and report to Vengeance
  env:
    VENGEANCE_URL: ${{ vars.VENGEANCE_URL }}
    VENGEANCE_TOKEN: ${{ secrets.VENGEANCE_TOKEN }}
  run: |
    docker run --rm -v "$PWD:/src" aquasec/trivy:0.76.2 fs --quiet --scanners vuln,secret,misconfig -f json -o /src/trivy.json /src
    docker run --rm -e VENGEANCE_URL -e VENGEANCE_TOKEN -v "$PWD:/w" --entrypoint vengeance ghcr.io/florianwenzel/vengeance:latest \
      push --asset "repository:github.com/${GITHUB_REPOSITORY}" --scanners vuln,secret,misconfig /w/trivy.json
```
</details>

<details>
<summary><b>A Kubernetes node, every night (host, every running image, cluster config, topology)</b></summary>

[`deploy/host`](deploy/host) has a systemd timer that scans the node with `trivy rootfs`, every image
running in the cluster straight from containerd, the cluster configuration with `trivy k8s`, and
reports the topology for the Blueprint.
</details>

Pass `--scanners` whenever you can: auto-close only touches the finding kinds a scan claims to cover.

## Deploy

One stateless container plus Postgres 16+. Images are multi-arch (amd64, arm64), distroless, run as
non-root and are signed.

```bash
docker run -d --name vengeance -p 8080:8080 \
  -e VENGEANCE_DATABASE_URL=postgres://… \
  -e VENGEANCE_BASE_URL=https://vengeance.example.com \
  -e VENGEANCE_SECRET="$(openssl rand -hex 32)" \
  -e VENGEANCE_OIDC_ISSUER=https://auth.example.com \
  -e VENGEANCE_OIDC_CLIENT_ID=vengeance -e VENGEANCE_OIDC_CLIENT_SECRET=… \
  -e VENGEANCE_OIDC_ALLOWED_EMAILS=you@example.com \
  ghcr.io/florianwenzel/vengeance:latest
```

Migrations run on start. Any OIDC provider works (Dex, Authelia, Keycloak, Pocket ID, Google, …);
the redirect URI is `$VENGEANCE_BASE_URL/auth/callback`.

<details>
<summary><b>All settings</b></summary>

| Variable | Required | Meaning |
|---|---|---|
| `VENGEANCE_DATABASE_URL` | yes | Postgres URL |
| `VENGEANCE_BASE_URL` | yes | public URL, e.g. `https://vengeance.example.com` |
| `VENGEANCE_SECRET` | yes | 32+ random characters, signs the login flow |
| `VENGEANCE_OIDC_ISSUER` | yes | OIDC issuer URL |
| `VENGEANCE_OIDC_CLIENT_ID` / `_CLIENT_SECRET` | yes | OIDC client |
| `VENGEANCE_OIDC_ALLOWED_EMAILS` / `_ALLOWED_DOMAINS` | one of | who may sign in (comma separated) |
| `VENGEANCE_OIDC_ALLOW_ALL` | | `true` if the provider already restricts who can sign in |
| `VENGEANCE_SMTP_HOST`, `_PORT`, `_USER`, `_PASSWORD`, `_FROM`, `_TLS` | for email | `_TLS` is `starttls` (default), `tls` or `none` |
| `VENGEANCE_KEV_URL` | | defaults to CISA's catalogue on GitHub (`cisagov/kev-data`); `off` disables the daily sync |
| `VENGEANCE_LISTEN` | | default `:8080` |
| `VENGEANCE_MAX_REPORT_MIB` | | upload limit, default 64 |
</details>

## Talk to it from Claude (MCP)

Create a token under **AI assistants**, then:

```bash
claude mcp add --transport http vengeance https://vengeance.example.com/mcp \
  --header "Authorization: Bearer vgp_…"
```

| Read | Write (read-write tokens) |
|---|---|
| `vengeance_overview`, `list_findings`, `get_finding`, `list_fixes`, `list_assets`, `get_asset`, `get_coverage`, `get_infrastructure`, `list_projects`, `list_sources` | `triage_findings`, `create_asset`, `update_asset` |

Triage done by an assistant is recorded in the finding's history as `mcp:<token name>`.

## How it works

```mermaid
flowchart LR
  subgraph Sources
    CI[CI: Trivy, SARIF]
    N[Nightly node scan]
  end
  CI --> I[POST /api/v1/ingest]
  N --> I
  N --> T[POST /api/v1/topology]
  I --> D[(Postgres)]
  T --> D
  D --> W[Workers: KEV sync, auto-close,<br>rules, digests, quiet sources]
  W --> M[Email · ntfy · webhook]
  D --> UI[Web UI + Blueprint<br>Vue · Rust/WASM · WebGL2]
  D --> MCP[MCP server]
```

- **Backend:** Go, `net/http` + chi, pgx, embedded migrations; jobs run in-process (no Redis).
- **UI:** Vue 3 + Vite + Tailwind, embedded in the binary. The Blueprint renderer is a Rust crate
  (`viz/`) compiled to WebAssembly with raw WebGL2: no engine, no textures.
- **API:** documented in [docs/api.md](docs/api.md); topology and the Blueprint in [docs/topology.md](docs/topology.md).

## Security

Vengeance holds sensitive information about your infrastructure, so it is built to be boring:

- OIDC sign-in only, no local passwords. PKCE, state and nonce; allow-list by email or domain.
- Source and API tokens are stored as SHA-256 hashes and shown once. Sessions are HttpOnly, SameSite cookies; writes need a CSRF header.
- A strict Content Security Policy (`script-src 'self' 'wasm-unsafe-eval'`, nothing external).
- Container images are distroless, non-root, multi-arch, **cosign-signed (keyless)** with SBOM and provenance attestations. Release checksums are signed too:
  ```bash
  cosign verify ghcr.io/florianwenzel/vengeance:latest \
    --certificate-identity-regexp 'https://github.com/FlorianWenzel/vengeance/' \
    --certificate-oidc-issuer https://token.actions.githubusercontent.com
  ```
- It scans itself: every build is scanned and reported to a Vengeance instance, and Dependabot keeps dependencies current.

Found a vulnerability? Please report it privately, see [SECURITY.md](SECURITY.md).

## Roadmap

- [x] Trivy + SARIF ingest, dedup, auto-close, KEV, fixes, coverage, notifications, OIDC
- [x] MCP server
- [x] Blueprint (3D) and Walk mode (the city)
- [ ] Discovery: GitHub App (every repo), registries, Certificate Transparency for forgotten subdomains
- [ ] EPSS, SLA targets, time-to-fix statistics, Prometheus `/metrics`
- [ ] One-click upgrade pull requests, OpenVEX export
- [ ] Helm chart

## Development

```bash
docker compose up -d postgres dex mailpit
bash scripts/build-viz.sh                        # the WebAssembly renderer (needs rustup target wasm32-unknown-unknown + wasm-bindgen)
cd web && npm ci && npm run dev                  # UI on :5173, proxies the API to :8080
VENGEANCE_DATABASE_URL=postgres://vengeance:vengeance@localhost:5433/vengeance?sslmode=disable \
VENGEANCE_SECRET=dev-only-secret-0123456789abcdef0123456789 VENGEANCE_OIDC_ISSUER=http://dex:5556/dex \
VENGEANCE_OIDC_CLIENT_ID=vengeance VENGEANCE_OIDC_CLIENT_SECRET=dev-secret \
VENGEANCE_OIDC_ALLOWED_EMAILS=admin@example.com go run ./cmd/vengeance serve
```

| Tests | Command |
|---|---|
| Go (needs Postgres) | `VENGEANCE_TEST_DATABASE_URL=postgres://… go test -p 1 ./...` |
| Renderer | `cd viz && cargo test` |
| End to end (Playwright, real stack incl. OIDC and SMTP) | `docker compose up -d --build && cd e2e && npm ci && npx playwright test` |
| README screenshots (on an empty stack) | `cd e2e && SCREENSHOTS=1 npx playwright test demo/`, then convert to WebP: `for f in ../docs/screenshots/*.png; do cwebp -q 82 -resize 1600 0 "$f" -o "${f%.png}.webp" && rm "$f"; done` |

Contributions are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

[AGPL-3.0](LICENSE). Use it, run it, change it. If you offer a modified version to others over a
network, share your changes.
