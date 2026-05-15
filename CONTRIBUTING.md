# Contributing to Argos

Спасибо за интерес. Argos в фазе pre-alpha (P0), внешние contributions примем после Phase P1 alpha. Пока что эта инструкция для core-команды.

## Local setup

### Required tools

- Rust ≥ 1.78 — `rustup install stable && rustup default stable`
- Node.js ≥ 20 + pnpm ≥ 9 — `corepack enable && corepack prepare pnpm@latest --activate`
- Tauri prerequisites — `https://tauri.app/start/prerequisites/`

### Platform-specific

**macOS:**
```bash
xcode-select --install
brew install gtk+3 libsoup webkitgtk@4.1   # only if cross-compiling for Linux
```

**Linux (Ubuntu/Debian):**
```bash
sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file libssl-dev libayatana-appindicator3-dev librsvg2-dev
```

**Windows:**
```powershell
# Install Microsoft Edge WebView2 Runtime
# Install Visual Studio Build Tools 2022 (C++ build tools)
```

### Bootstrap

```bash
git clone <repo>
cd argos
pnpm install
cargo build
```

### Run

```bash
# Desktop в dev mode
pnpm tauri:dev

# Only UI в браузере (без Tauri shell, для frontend-only работы)
pnpm dev

# CLI
cargo run -p argos-cli -- --help
```

## Workflow

### Branches

- `main` — стабильная ветка, всё что мерджится сюда — релизуется в nightly.
- `feature/<short-name>` — для новой функциональности.
- `fix/<short-name>` — для багов.

### Commits

Conventional commits:

- `feat(scope): ...` — новая функциональность.
- `fix(scope): ...` — багфикс.
- `chore(scope): ...` — поддержка, configs, deps.
- `docs(scope): ...` — документация.
- `test(scope): ...` — тесты.
- `refactor(scope): ...` — рефакторинг без изменения поведения.

`<scope>` — `core`, `cli`, `desktop`, `ui`, `ci`, `docs`.

### PR checklist

Перед мерджем:

- [ ] CI зелёный на 3 ОС (Mac, Win, Linux).
- [ ] `cargo fmt --check` без ошибок.
- [ ] `cargo clippy -- -D warnings` без ошибок.
- [ ] `pnpm lint` и `pnpm typecheck` без ошибок.
- [ ] Юнит-тесты на критичные пути написаны.

## Architecture decisions

Для нетривиальных архитектурных изменений или новых зависимостей — RFC через GitHub Discussion перед PR.

## Code style

- Rust: rustfmt + clippy (см. `rustfmt.toml`, всё дефолтное по clippy).
- TypeScript: prettier + eslint (см. `.prettierrc`).
- YAML: prettier.
- Markdown: prettier (но не ломаем строки в таблицах).

## Tests

- Rust: `cargo test --workspace`.
- TypeScript: `pnpm test` (vitest).
- E2E: `pnpm test:e2e` (playwright).
