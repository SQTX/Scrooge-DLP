# ScroogeDLP — Claude Code context

> Ten plik to wskazówka dla Claude Code przy każdej sesji w tym repo.
> User-level memory siedzi w `~/.claude/projects/.../memory/`.

## Co to jest

System DLP (Data Loss Prevention) inspirowany Wazuh, ale skupiony na DLP
(monitorowanie clipboard/USB/filesystem/network, klasyfikacja wrażliwych
danych, blokowanie exfiltracji). Trzy komponenty:

- **`scrooge-agent`** — endpoint agent (Linux/macOS/Windows). Łączy się z managerem
  przez gRPC mTLS, dostaje polityki, w przyszłości wykrywa eventy DLP.
- **`scrooge-manager`** — centralny serwer (Docker + quickstart). Zarządza
  politykami, certami (Root CA), eventami, HTTPS dashboard.
- **`scroogectl`** — CLI klient managera (init-ca, migrate, bootstrap-admin, gen-token).

Stack: **Rust 2021** (MSRV 1.75), Tokio, **tonic** (gRPC), **axum** (REST),
**sqlx** (PostgreSQL), **rustls** (TLS). Multi-crate workspace w `crates/`.

## Status (2026-05-15)

✅ **Faza 1: Wazuh-flow Distribution** — manager+agent+dashboard od strzała przez
`curl install.sh | bash`. mTLS gRPC + HTTPS REST. Linux production-tested,
macOS/Windows installers gotowe (E2E na żywych systemach do zrobienia).
Tag `v0.1.0-rc7` w GitHub Releases.

✅ **Faza 2: Polityki DLP YAML** — `scrooge-policy` crate (parser + msgpack
compile + SHA-256), REST CRUD `/api/v1/policies`, push przez gRPC Stream z
delta-push + `PolicyAck`, dashboard tabela + YAML editor + history +
rollback, `POST /agents/{id}/command` (Send command z UI), agent_version
w heartbeat + outdated widget. Polityki są zarządzane ale **NIE
enforce'owane jeszcze** (to Phase 3).

⏳ **Faza 3: Detekcja DLP** — `mod_clipscreen` (clipboard monitor),
`mod_classifier` (Luhn/PESEL/IBAN/NIP), agent zaczyna realnie patrzeć
na wrażliwe dane i blokować/logować.

## Workflow

- **Branches:** feature branches z `dev` (NIE z `main`). Worktree branches
  `claude/<auto>` są scratchpad — push trafia na `dev`, potem PR `dev → main`
  na milestone'ach.
- **Conventional Commits** (`feat(scope):`, `fix(scope):`, `docs:` itp).
  Autor: SQTX `<sssqtx@gmail.com>`.
- **Lints:** `cargo fmt --all -- --check`, `cargo clippy --workspace
  --all-targets -- -D warnings`, `cargo test --workspace`. Wszystko zielone
  przed commitem.
- **MSRV 1.75** ale Dockerfile.manager używa Rust 1.90 (deps wymagają
  edition 2024).

## Co gdzie

- **`PROJECT_BRIEF.md`** — pełen plan, schema polityk YAML, schema bazy,
  format eventów. Source of truth dla decyzji projektowych.
- **`MILESTONE_1_PROGRESS.md`** — Faza 1 status (legacy, niepotrzebny po M2).
- **`ROADMAP.md`** — wszystkie fazy do Phase 6.
- **`crates/scrooge-policy/`** — Phase 2 parser+compile.
- **`crates/scrooge-manager-bin/src/grpc.rs`** — Stream handler (mTLS + push
  polityk + bridge dla commands).
- **`crates/scrooge-manager-api/src/handlers/`** — REST endpointy (polityki,
  agenty, install API, dashboard).
- **`crates/scrooge-manager-api/static/index.html`** — embedded dashboard
  (vanilla HTML/JS + Tailwind CDN, ~1300 linii).
- **`deploy/install.sh`** + **`deploy/quickstart.sh`** — bootstrap one-liner
  flow (`curl ... raw/main/deploy/install.sh | bash`).
- **`migrations/`** — schema PostgreSQL (6 + 1 dla Phase 2).
- **`proto/scrooge.proto`** — gRPC schema (Heartbeat, PolicyUpdate, Command,
  Enroll, Stream).

## Konwencje code'u

- **Komentarze po polsku** (user pisze po polsku, code comments PL/EN OK).
- **Nie dodawaj fallbacks/validacji dla scenariuszy które nie mogą się zdarzyć.**
  Boundary validation tak, defensive everywhere nie.
- **`thiserror` dla error types**, `anyhow` dla bin entrypoint.
- **Brak `unsafe`**, `clippy::pedantic` z 5 wyjątkami workspace-level.
- **mTLS jest authoritative dla agent identity** — Subject CN cert =
  agent_id UUID. `agent-id` w metadata zostało usunięte w Sub-fazie 1E.
- **Polityki: PostgreSQL = źródło prawdy**, YAML to import/export.

## Setup deweloperski

- **Stacja:** macOS Apple Silicon (`aarch64`)
- **Postgres lokalnie:** Docker `scrooge-dev-postgres` na `:5433`
- **Dev VM testowa:** Ubuntu UTM (do real-deployment smoke), gateway
  `192.168.64.1`
- **Codzienne:** `make dev-up && make dev-bootstrap && make dev-manager &
  && make dev-agent &`
- **Smoke E2E:** `make e2e-test`

## Aktualnie

**Phase 2 zamknięta**, czeka na PR `dev → main` po manualnym E2E na żywych VM-kach
(Manager z Docker + agent enrolled + create policy w dashboard + verify
push do agenta przez gRPC Stream + verify msgpack na agencie + verify
PolicyAck w manager logach).

`dev` ma `d57bb02`, `main` ma `e832bc0` (= rc7 + README).

**Następnie:** PR dev→main, tag v0.1.1 (bump minor, pierwsza realna
funkcjonalność po MVP), później Phase 3 (detekcja DLP).
