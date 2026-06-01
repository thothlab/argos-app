# argos-web

The thing behind `argos.thothlab.tech`: landing page, Tauri update
manifest, and crash collection endpoint. Single Rust binary
(`axum` + `tower-http::ServeDir`), single Docker container, hosted on
a Mac Mini behind NAT and exposed through a reverse SSH tunnel to a
small public-IP VPS (Caddy on the VPS terminates TLS and proxies to
the tunnel). The same pattern works with any reverse-tunnel setup —
`autossh`, `frp`, Cloudflare Tunnel — pick what you have.

## Routes

| Method | Path                              | Purpose                                                  |
| ------ | --------------------------------- | -------------------------------------------------------- |
| GET    | `/`                               | Landing page (inline HTML, no JS).                       |
| GET    | `/healthz`                        | `200 ok` — Caddy / monit probe.                          |
| GET    | `/docs/*`                         | Astro Starlight site, mounted read-only.                 |
| GET    | `/download/{target}`              | 302 → GitHub Releases.                                   |
| GET    | `/api/update/{target}/{current}`  | Tauri 2 updater manifest. 204 when up to date.           |
| POST   | `/api/crash`                      | Accept + dedup crash report. 64 KB max, 10 req/min/IP.   |

Targets accepted by `/download/{target}` and the updater:
`macos-aarch64`, `macos-x64`, `linux-x64`, `windows-x64`
(Tauri's `darwin-aarch64` / `darwin-x86_64` aliases also work).

## Local dev

```sh
make build-docs       # produce ../docs/dist
make run              # serves on http://127.0.0.1:8743
```

Then:

```sh
curl -fsS http://127.0.0.1:8743/                       | head
curl -fsS http://127.0.0.1:8743/healthz
curl -fsS http://127.0.0.1:8743/api/update/macos-aarch64/0.0.0   # → 503 until manifest exists
```

To exercise the updater locally, drop a `manifest.json` into
`./data/manifest.json`. See `src/routes/update.rs` for the schema or
the deploy section below.

## Deploy

### Prerequisites (one-off)

1. **DNS**: A record for your domain → your VPS IP.
2. **SSH alias** `vps` configured in `~/.ssh/config` (the Makefile
   defaults assume one — override with `VPS_HOST=...` if your alias
   differs).
3. **autossh** installed on the Mac Mini (`brew install autossh`).
4. **tunnel key** present at `~/.ssh/id_ed25519_tunnel` (override
   with `TUNNEL_KEY=...` to point at a different key).

### Steps

```sh
# On the Mac Mini (home server)
cd apps/web
make deploy-all          # build docs, build image, start container

make install-launchagent # register the autossh reverse tunnel
                         # (only once — survives reboots via LaunchAgent)

# From any machine with `ssh vps` (typically your laptop)
make push-caddy          # appends Caddyfile snippet, reloads Caddy
```

Verify externally:

```sh
curl -fsS https://argos.thothlab.tech/healthz       # → ok
curl -fsS https://argos.thothlab.tech/ | head -5    # → <!DOCTYPE html>
```

### Publishing a new update manifest

After cutting a Tauri release (`cargo tauri build` + signing):

```sh
# from the Mac Mini, with the new manifest at /tmp/manifest.json
make push-manifest FILE=/tmp/manifest.json
```

The file is copied into the container's `argos-web-data` volume; the
service reads it on every `/api/update/...` request, so no restart is
needed.

## Configuration

All via environment variables (see `docker-compose.yml`):

| Variable                | Default                                                  | Purpose                                                    |
| ----------------------- | -------------------------------------------------------- | ---------------------------------------------------------- |
| `ARGOS_BIND`            | `0.0.0.0:8743` (in container), `127.0.0.1:8743` (local)  | Listen address.                                            |
| `ARGOS_DATA_DIR`        | `/var/lib/argos-web`                                     | Where crash reports + manifest.json live.                  |
| `ARGOS_DOCS_DIR`        | `/srv/argos-docs`                                        | Read-only docs site export.                                |
| `ARGOS_DOWNLOADS_BASE`  | `https://github.com/thothlab/argos-app/releases/latest/download` | Prefix for `/download/{target}` 302s.                |
| `RUST_LOG`              | `argos_web=info,tower_http=info`                         | tracing filter.                                            |

## Persistence

- `argos-web-data` Docker volume → `data/crashes/<YYYY-MM-DD>/<hash>.json`
  + `<hash>.count` next to it. Survive container rebuilds.
- 90-day retention is *intent* — not auto-enforced in v1. Add a
  cron job to clean up if you care (`find … -mtime +90 -delete`).

## What's not here

- **No CI for builds.** The Dockerfile is meant for hand-rolled
  `docker compose build` on the Mac Mini. Add a GH Actions workflow if
  desired — the image is small (~30 MB) so even slow CI is fine.
- **No metrics / Prometheus.** `tower-http::trace` is the only
  observability. Pull in `axum-prometheus` later if needed.
- **No admin UI for crashes.** Inspect with `ls`, `jq`, or rsync the
  directory off the Mac Mini. v2 idea: a `/api/crashes` listing
  endpoint behind basic auth.
