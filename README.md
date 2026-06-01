# Argos

> Fast, git-native API client. Free for individuals; managed CRDT relay for teams comes later.

**Status:** P1 MVP closed-alpha. v0.1.5 released for macOS (Apple Silicon), Linux x86_64 (`.deb` / `.rpm` / `.AppImage`), and Windows x86_64. Self-test loop is active — expect rough edges; please file issues if you hit any. See [argos.thothlab.tech](https://argos.thothlab.tech/) for download links and the in-app updater for follow-up builds.

## Why Argos

В «Одиссее» Гомера Argos — собака Одиссея, которая ждала хозяина 20 лет и узнала его в маскировке. Loyalty + recognition — метафора tool, который ты используешь годами и который «узнаёт» твои workflows.

## What's in v0.1.5

- **Multi-protocol** — REST / GraphQL / WebSocket в одном паттерне (gRPC / SSE / MQTT — позже).
- **Git-native workspace** — каждая коллекция, environment, request — это plain YAML на диске. Diffs review like code; secrets stay out of cloud.
- **Lightweight** — Tauri-based desktop ~10–20 MB (vs 150–250 MB у Postman / Insomnia / Bruno).
- **Universal import** — Postman v2.1, Insomnia v4, Bruno, OpenAPI 3.x **и Swagger 2.0**, curl, plus **AI-powered log import** (BYOK — your own Anthropic / OpenAI / Ollama key).
- **CLI runner** — `argos run ./collections` с reporters (junit / json / html) для CI.
- **Auto-updater** — подписанные бинарники + stable / beta / nightly каналы.
- **Crash reporter** — opt-in, server-side PII scrubbing.
- **Local-first** — никакого обязательного cloud, telemetry off by default.

### On the roadmap (P2+, не в этом релизе)

- gRPC / SSE / MQTT protocols.
- Time-travel + diff между запусками.
- Encrypted secrets in git (sops + age).
- Local mock-server из OpenAPI / real traffic.
- CRDT collaboration без cloud (P2P + self-hosted relay).
- Web mode (PWA), VS Code extension, browser capture extension.

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
│   ├── docs/              # Astro Starlight site (argos.thothlab.tech/docs)
│   ├── ui/                # Solid.js UI shared between desktop and web
│   └── web/               # Landing + Tauri update / crash backend
└── examples/              # Sample workspaces + CI integration
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

User-facing docs live at **[argos.thothlab.tech/docs](https://argos.thothlab.tech/docs/)** (source in [`apps/docs/`](apps/docs/)).

## License

Apache License 2.0 — see [`LICENSE`](LICENSE).

## Contributing

Прочитай [`CONTRIBUTING.md`](CONTRIBUTING.md).

Закрытая альфа — пока мы сами прокликиваем v0.1.5 и чиним очевидные баги, формальный recruiting alpha-тестировщиков отложен. **Issues и PR welcome уже сейчас**, но имей в виду:

- Большие фичи (новый протокол, новый формат импорта, refactor) — сначала открой issue на обсуждение.
- Маленькие фиксы (опечатки, очевидные баги, локальный UX-фидбек) — PR без согласования ок.
- Багрепорты с реальным workspace + воспроизведением — на вес золота.
