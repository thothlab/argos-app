# Argos web service — build / deploy / operate.
#
# Tested on macOS + Homebrew; the install-launchagent target is
# Mac Mini specific (the home server side). The deploy targets that
# touch the VPS use the existing `ssh vps` alias from
# ~/.ssh/config (see self-hosted-tunnel/setup-laptop.sh).

PORT ?= 8743
VPS_HOST ?= vps
VPS_IP ?= 153.80.185.242
TUNNEL_KEY ?= $(HOME)/.ssh/id_ed25519_tunnel
AUTOSSH_BIN ?= /opt/homebrew/bin/autossh
LAUNCH_PLIST := $(HOME)/Library/LaunchAgents/com.argos.tunnel.plist
LAUNCH_TEMPLATE := deploy/com.argos.tunnel.plist.tmpl

DOCS_DIST := ../docs/dist

.PHONY: help build test run docker-build docker-up docker-down \
        install-launchagent uninstall-launchagent \
        push-manifest push-caddy build-docs deploy-all

help:
	@echo "argos-web — common commands"
	@echo
	@echo "  make build              cargo build --release"
	@echo "  make test               cargo test"
	@echo "  make run                cargo run (ARGOS_DATA_DIR=./data)"
	@echo "  make build-docs         pnpm --filter @argos/docs build"
	@echo "  make docker-build       docker compose build"
	@echo "  make docker-up          docker compose up -d"
	@echo "  make docker-down        docker compose down"
	@echo
	@echo "  make install-launchagent      Mac Mini: register autossh tunnel"
	@echo "  make uninstall-launchagent    Mac Mini: drop the tunnel"
	@echo "  make push-caddy               VPS: append Caddyfile snippet"
	@echo "  make push-manifest FILE=...   VPS-or-Mac: copy a new manifest.json"
	@echo "  make deploy-all               build docs, build image, restart container"

# ---- build / test --------------------------------------------------------

build:
	cargo build --release -p argos-web

test:
	cargo test -p argos-web

run:
	ARGOS_DATA_DIR=$$(pwd)/data \
	ARGOS_DOCS_DIR=$$(pwd)/$(DOCS_DIST) \
	ARGOS_BIND=127.0.0.1:$(PORT) \
	cargo run -p argos-web

build-docs:
	pnpm --filter @argos/docs build

# ---- docker --------------------------------------------------------------

docker-build:
	docker compose build

docker-up:
	docker compose up -d

docker-down:
	docker compose down

# ---- LaunchAgent (Mac Mini) ----------------------------------------------

install-launchagent: $(LAUNCH_TEMPLATE)
	@echo "Generating $(LAUNCH_PLIST) from template..."
	@sed \
	  -e 's|@AUTOSSH_BIN@|$(AUTOSSH_BIN)|g' \
	  -e 's|@TUNNEL_KEY@|$(TUNNEL_KEY)|g' \
	  -e 's|@PORT@|$(PORT)|g' \
	  -e 's|@VPS_IP@|$(VPS_IP)|g' \
	  $(LAUNCH_TEMPLATE) > $(LAUNCH_PLIST)
	@echo "Loading LaunchAgent..."
	launchctl unload $(LAUNCH_PLIST) 2>/dev/null || true
	launchctl load $(LAUNCH_PLIST)
	@sleep 2
	@if launchctl list | grep -q com.argos.tunnel; then \
	    echo "✓ tunnel running"; \
	else \
	    echo "✗ tunnel failed — see /tmp/argos-tunnel.err"; \
	    exit 1; \
	fi
	@echo "Verifying port on VPS..."
	ssh $(VPS_HOST) "ss -ltn 'sport = :$(PORT)' | grep -q LISTEN" \
	  && echo "✓ VPS sees :$(PORT) listening" \
	  || (echo "✗ VPS does not see :$(PORT) — tunnel may be partial"; exit 1)

uninstall-launchagent:
	launchctl unload $(LAUNCH_PLIST) 2>/dev/null || true
	rm -f $(LAUNCH_PLIST)
	@echo "✓ LaunchAgent removed"

# ---- VPS side ------------------------------------------------------------

push-caddy: deploy/Caddyfile.snippet
	@echo "Appending Caddyfile snippet to $(VPS_HOST):/etc/caddy/Caddyfile"
	scp deploy/Caddyfile.snippet $(VPS_HOST):/tmp/argos-caddy.snippet
	ssh $(VPS_HOST) "sudo tee -a /etc/caddy/Caddyfile < /tmp/argos-caddy.snippet > /dev/null"
	ssh $(VPS_HOST) "sudo caddy validate --config /etc/caddy/Caddyfile && sudo systemctl reload caddy"
	@echo "✓ Caddy reloaded"

# Copy a new manifest.json (typically produced by `cargo tauri build` or
# `tauri-action` in CI) into the data volume. Usage:
#   make push-manifest FILE=./manifest.json
push-manifest:
	@test -n "$(FILE)" || (echo "Usage: make push-manifest FILE=./manifest.json"; exit 1)
	@docker compose ps -q argos-web > /dev/null 2>&1 || (echo "container not running"; exit 1)
	docker compose cp $(FILE) argos-web:/var/lib/argos-web/manifest.json
	@echo "✓ manifest copied into argos-web-data volume"

# ---- end-to-end ----------------------------------------------------------

deploy-all: build-docs docker-build docker-up
	@echo
	@echo "Next manual steps (one-offs):"
	@echo "  1. make install-launchagent     (this Mac Mini)"
	@echo "  2. make push-caddy              (run from a machine with 'ssh vps')"
	@echo "  3. make push-manifest FILE=...  (after the first release)"
	@echo
	@echo "Then verify externally:"
	@echo "  curl -fsS https://argos.thothlab.tech/healthz"
