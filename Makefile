.PHONY: help install dev build test lint typecheck clean tauri-dev tauri-build cli

help:
	@echo "Argos — common dev commands:"
	@echo ""
	@echo "  make install      # Install all dependencies (Rust + JS)"
	@echo "  make dev          # Run UI in browser (no Tauri)"
	@echo "  make tauri-dev    # Run desktop app in dev mode"
	@echo "  make build        # Production builds (Rust + UI)"
	@echo "  make tauri-build  # Build desktop app for current platform"
	@echo "  make cli          # Build & run CLI"
	@echo "  make test         # Run all tests (Rust + JS)"
	@echo "  make lint         # Run linters (clippy + eslint + prettier --check)"
	@echo "  make typecheck    # Run TypeScript typecheck"
	@echo "  make clean        # Remove all build artefacts"

install:
	pnpm install
	cargo fetch

dev:
	pnpm dev

tauri-dev:
	pnpm tauri:dev

build:
	cargo build --release --workspace
	pnpm build

tauri-build:
	pnpm tauri:build

cli:
	cargo run -p argos-cli -- $(ARGS)

test:
	cargo test --workspace
	pnpm test

lint:
	cargo fmt --all -- --check
	cargo clippy --workspace --all-targets -- -D warnings
	pnpm lint

typecheck:
	pnpm typecheck

clean:
	cargo clean
	rm -rf node_modules apps/*/node_modules apps/*/dist
