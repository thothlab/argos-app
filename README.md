# Argos

> Fast, git-native API client. Free for individuals, monetised through managed CRDT relay for teams.

**Status:** Phase P0 (Bootstrap) — pre-alpha. Not usable yet.

## Why Argos

В «Одиссее» Гомера Argos — собака Одиссея, которая ждала хозяина 20 лет и узнала его в маскировке. Loyalty + recognition — метафора tool, который ты используешь годами и который «узнаёт» твои workflows.

## What we're building

- **Lightweight** — Tauri-based desktop ~10–20 MB (vs 150–250 MB у Postman / Insomnia / Bruno).
- **Multi-protocol unified UX** — REST / GraphQL / gRPC / WebSocket / SSE / MQTT в одном паттерне.
- **Git-native** — коллекции в plain text, commit-friendly.
- **Local-first** — никакого обязательного cloud, telemetry off by default.
- **Local mock-server** — из OpenAPI или real traffic.
- **Time-travel + diff** — между запусками, средами, запросами.
- **Encrypted secrets** — sops/age в git без боли.
- **CRDT collaboration** — без cloud (P2P + self-hosted relay).
- **Universal** — desktop + web (PWA) + VS Code extension + CLI с одним файл-форматом.

## Repository structure

```
.
├── crates/
│   ├── core/              # argos-core (Rust shared core)
│   ├── core-wasm/         # WASM bindings for web / VS Code
│   ├── cli/               # argos-cli (CLI binary)
│   └── desktop/           # Tauri desktop app
│       └── src-tauri/
├── apps/
│   └── ui/                # Solid.js UI shared between desktop and web
├── docs/                  # Project documentation (BRD, ТЗ, design specs)
├── tasks/                 # Operational task tracker (per-epic markdowns)
└── examples/              # Sample workspaces
```

## Getting started

### Prerequisites

- **Rust** ≥ 1.78 (`rustup install stable`).
- **Node.js** ≥ 20 + **pnpm** ≥ 9 (`corepack enable && corepack prepare pnpm@latest --activate`).
- **Tauri** prerequisites for your OS — see [tauri.app/start/prerequisites](https://tauri.app/start/prerequisites/).

### Local development

```bash
# Install JS deps
pnpm install

# Build Rust core
cargo build

# Run the desktop app in dev mode (Tauri)
pnpm tauri:dev

# Run only the UI in browser (without Tauri shell)
pnpm dev

# Run CLI
cargo run -p argos-cli -- --help
```

### Tests, lint, typecheck

```bash
make test      # Rust tests + JS tests
make lint      # cargo clippy + eslint + prettier --check
make typecheck # tsc --noEmit
```

## Documentation

- Business requirements: [`docs/03_business_requirements.md`](docs/03_business_requirements.md)
- Developer specification: [`docs/05_developer_specification.md`](docs/05_developer_specification.md)
- Designer specification: [`docs/06_designer_specification.md`](docs/06_designer_specification.md)
- Full implementation plan: [`docs/09_implementation_plan.md`](docs/09_implementation_plan.md)
- Task tracker: [`tasks/README.md`](tasks/README.md)

## License

Apache License 2.0 — see [`LICENSE`](LICENSE).

## Contributing

Прочитай [`CONTRIBUTING.md`](CONTRIBUTING.md). Issues и PR welcome после того, как пройдём Phase P1 alpha.
