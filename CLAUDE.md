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

## Status (2026-05-21)

✅ **Faza 1: Wazuh-flow Distribution** — manager+agent+dashboard od strzała przez
`curl install.sh | bash`. mTLS gRPC + HTTPS REST. Linux production-tested,
macOS/Windows installers gotowe (E2E na żywych systemach do zrobienia).
Tag `v0.1.0-rc7` w GitHub Releases.

✅ **Faza 2: Polityki DLP YAML** — `scrooge-policy` crate (parser + msgpack
compile + SHA-256), REST CRUD `/api/v1/policies`, push przez gRPC Stream z
delta-push + `PolicyAck`, dashboard tabela + YAML editor + history +
rollback, `POST /agents/{id}/command` (Send command z UI), agent_version
w heartbeat + outdated widget. **E2E zweryfikowane na żywych VM-kach.**

✅ **Faza 2F: Policy editor dual-mode (Form + YAML)** — wydzielony JS module
`static/policy-editor/` (ES Modules, klasy ES6, zero build step). Form mode
ze split view (pola po lewej, live YAML preview po prawej, debounce 150ms).
Sekcje: Metadata, Targets (segmented all/tags/groups/agent_ids + OS filter),
Rule, Sources (6 typów), Destinations (7 typów), Conditions (collapsible).
ModeSwitch w nagłówku, yamlToForm parsuje przez backend Validate (smart
default: Form jeśli form-friendly, YAML inaczej), ConfirmDialog przy YAML→Form
z unsupported features, klikalne `?` tooltipy z opisem każdego pola/akcji.
Tag `v0.2.0` planowany. Polityki nadal **NIE enforce'owane** (to Phase 3).

🚧 **Faza 3: Detekcja DLP — MVP w toku** — `mod_clipscreen` (Linux X11/Wayland
przez `arboard`) + `LuhnClassifier` (numery kart) + sqlite WAL event queue +
gRPC EventBatch upload + manager → PG events insert + dashboard "Events" tab
(filtry severity/agent/type). Control plane PRODUKCYJNY: enroll + mTLS Stream +
event upload działa E2E na żywej VM.
**Caveat**: clipboard task wymaga GUI sesji (DISPLAY/WAYLAND_DISPLAY env).
Systemd unit jako root nie zobaczy schowka — to expected, demo wymaga osobnej
VM z Ubuntu Desktop. `systemd --user` unit roadmap w v0.3.x.

⏳ **Faza 3 rozszerzenia (v0.3.x)** — macOS/Windows clipboard, PESEL/IBAN/NIP
classifiers, USB hot-plug (`mod_devctl`), filesystem watcher (`mod_filemon`),
`systemd --user` install option, prebuilt deb/rpm w release (zamiast
build-from-source).

🔜 **Backlog post-Phase 2F:**
- **2G: Groups + per-agent tags GUI** — Agents tab edit tagów + `targets`
  preview "matches N agents". Wymaga PG schema update + `PUT /agents/{id}/tags`.
- **Phase 4: Event pipeline + offline buffering** — sqlite WAL na agencie,
  batch upload nieparsowanych eventów po reconnect (manager down = zero loss).
- **Phase 5: Backup & resilience** — auto pg_dump cron, restore z dashboardu,
  external PG opcja.
- **Phase 6: Auto-update flow** — image registry (GHCR), dashboard "Check
  for updates", agent self-update przez Send command.

## Workflow

- **Branches:** feature branches z `dev` (NIE z `main`). Worktree branches
  `claude/<auto>` są scratchpad — push trafia na `dev`, potem PR `dev → main`
  na milestone'ach.
- **Conventional Commits** (`feat(scope):`, `fix(scope):`, `docs:` itp).
  Autor: SQTX `<sssqtx@gmail.com>`.
- **Versioning — minor = phase, patch = fix w fazie:**
  - `v0.1.x` Phase 1 (Wazuh-flow distribution), tagi: `v0.1.0-rc7` etc.
  - `v0.2.x` Phase 2 (DLP policies YAML + dashboard editor)
  - `v0.3.x` Phase 3 (clipboard + classifier enforce)
  - `v0.4.x` Phase 4 (event pipeline + offline buffering)
  - `v0.5.x` Phase 5 (backup & resilience)
  - `v0.6.x` Phase 6 (auto-update + image registry)
  - `v1.0.0` production ready (po wszystkich phases)
  - Sub-fazy (np. 2F, 2G) → patch bump (`v0.2.1`, `v0.2.2`) jeśli dodawane
    po release danej fazy. Jeśli idą razem z major fazą — łączymy w `v0.X.0`.
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

**Phase 2 + 2F zamknięte i zweryfikowane E2E na żywych VM-kach.**
PR `dev → main` + tag `v0.2.0` (bump minor — pierwsza realna funkcjonalność
po MVP).

**Następnie:** Phase 3 (detekcja DLP — clipboard monitor + classifier
Luhn/PESEL/IBAN/NIP), brainstorming na początek.
