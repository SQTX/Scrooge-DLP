# SPDX-License-Identifier: GPL-2.0-only
# ScroogeDLP - Makefile z targetami dev/CI

.DEFAULT_GOAL := help
.PHONY: help build build-release check fmt fmt-check lint test check-all \
        clean docs proto ci-local install-tools \
        dev-up dev-down dev-certs dev-bootstrap dev-manager dev-manager-vm dev-agent dev-agent-reset dev-logs \
        dev-build-agent-linux

# ── Help ─────────────────────────────────────────────────────────────────────
help:  ## Pokaż dostępne targety
	@awk 'BEGIN {FS = ":.*?## "} /^[a-zA-Z_-]+:.*?## / \
		{printf "  \033[36m%-26s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)

# ── Build ────────────────────────────────────────────────────────────────────
build:        ## Debug build całego workspace
	cargo build --workspace --all-targets

build-release: ## Release build całego workspace
	cargo build --workspace --release

check:        ## cargo check (szybsze niż build)
	cargo check --workspace --all-targets

# ── Code quality ─────────────────────────────────────────────────────────────
fmt:          ## Sformatuj cały kod
	cargo fmt --all

fmt-check:    ## Sprawdź formatowanie (CI)
	cargo fmt --all -- --check

lint:         ## clippy w trybie strict (CI-equivalent)
	cargo clippy --workspace --all-targets -- -D warnings

# ── Tests ────────────────────────────────────────────────────────────────────
test:         ## Uruchom wszystkie testy
	cargo test --workspace --all-targets

# ── Agregat ──────────────────────────────────────────────────────────────────
check-all: fmt-check lint test  ## fmt-check + lint + test (pre-push)

ci-local: check-all build-release  ## Symuluj pełne CI lokalnie

# ── Dev workflow (lokalne uruchomienie managera) ─────────────────────────────
dev-up: dev-certs  ## Postaw lokalne dev env (Postgres w Dockerze + certy)
	docker compose -f deploy/dev/docker-compose.yml up -d postgres
	@echo ""
	@echo "▸ czekam aż Postgres będzie ready…"
	@until docker exec scrooge-dev-postgres pg_isready -U scrooge -q; do sleep 1; done
	@echo "✔ Postgres ready on 127.0.0.1:5432"
	@echo ""
	@echo "Next steps:"
	@echo "  1. make dev-bootstrap   # utwórz admin + enrollment token"
	@echo "  2. make dev-manager     # odpal scrooge-manager"

dev-down: ## Zatrzymaj lokalne dev env (zachowuje volume z danymi)
	docker compose -f deploy/dev/docker-compose.yml down

dev-certs: ## Wygeneruj dev CA + server cert (jeśli jeszcze nie istnieją)
	@if [ -f dev-certs/server.pem ]; then \
		echo "▸ dev-certs/server.pem already exists — skipping (delete to regenerate)"; \
	else \
		bash deploy/dev/generate-dev-certs.sh; \
	fi

dev-bootstrap: ## Apply migrations + utwórz admin user + przykładowy enrollment token
	@echo "▸ building scroogectl…"
	@cargo build -p scrooge-cli --quiet
	@echo ""
	@echo "▸ applying migrations…"
	@MANAGER_CONFIG=deploy/dev/manager.dev.yaml ./target/debug/scroogectl migrate
	@echo ""
	@echo "▸ bootstrapping admin user (admin / admin123)…"
	@MANAGER_CONFIG=deploy/dev/manager.dev.yaml ADMIN_PASSWORD=admin123 \
		./target/debug/scroogectl bootstrap-admin --username admin
	@echo ""
	@echo "▸ generating enrollment token (30 days)…"
	@MANAGER_CONFIG=deploy/dev/manager.dev.yaml \
		./target/debug/scroogectl gen-token --description "dev token" | \
		tee dev-certs/enrollment-token.txt
	@echo "  → token saved to dev-certs/enrollment-token.txt"

dev-manager: ## Uruchom scrooge-manager z dev config'iem (pretty logi, listen 127.0.0.1)
	cargo run --bin scrooge-manager -- \
		--config deploy/dev/manager.dev.yaml \
		--log-format pretty

dev-manager-vm: ## Uruchom scrooge-manager listenujący na 0.0.0.0 (dla testów z UTM VM)
	cargo run --bin scrooge-manager -- \
		--config deploy/dev/manager.dev-vm.yaml \
		--log-format pretty

dev-agent: ## Uruchom scrooge-agent z dev config'iem (token z dev-certs/enrollment-token.txt)
	@if [ ! -f dev-certs/enrollment-token.txt ]; then \
		echo "ERROR: brak dev-certs/enrollment-token.txt — uruchom najpierw 'make dev-bootstrap'"; \
		exit 1; \
	fi
	@TOKEN=$$(cat dev-certs/enrollment-token.txt) && \
	  ENROLLMENT_TOKEN=$$TOKEN \
	  cargo run --bin scrooge-agent -- \
	    --config deploy/dev/agent.dev.yaml \
	    --log-format pretty

dev-agent-reset: ## Wyczyść agent-data (force re-enrollment)
	rm -rf agent-data

dev-logs: ## Tail logów Postgresa
	docker compose -f deploy/dev/docker-compose.yml logs -f postgres

# ── Cross-build agenta (przyszłość) ──────────────────────────────────────────
dev-build-agent-linux:  ## Cross-compile agenta na Linux x86_64
	cross build --target x86_64-unknown-linux-gnu --release -p scrooge-agent-bin

# ── Docs / proto / clean ─────────────────────────────────────────────────────
docs:         ## Wygeneruj cargo docs
	cargo doc --workspace --no-deps --open

proto:        ## Rebuild protobuf (force rebuild scrooge-proto)
	cargo clean -p scrooge-proto && cargo build -p scrooge-proto

clean:        ## Usuń artefakty buildu
	cargo clean

install-tools: ## Zainstaluj narzędzia dev (cargo-watch, sqlx-cli, cross)
	cargo install cargo-watch sqlx-cli cross --locked
