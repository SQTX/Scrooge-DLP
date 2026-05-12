# Milestone 1 — Status pracy

> **Stan na:** 2026-05-12
> **Branch:** `main` (worktree `claude/unruffled-jennings-c55c25` przed merge'm)
> **Status:** ✅ **MILESTONE 1 COMPLETE** + bonusy + **Cała Faza 1 (Wazuh-flow Distribution)** ukończona: 1A (Release CI) + 1B (Install API Linux) + 1C (macOS + Windows installer) + 1D (Dashboard wizard).
> **Aktualnie:** Pełna architektura agentowa cross-platform gotowa od kodu. Otwarte tylko **manualne E2E** (curl-flow trzeba odpalić na czystych VM-kach: Ubuntu, macOS host, Windows VM) — to verification, nie implementation.
> **Następnie:** Phase 2 / Milestone 2 — polityki DLP w YAML (`scrooge-policy` crate + REST CRUD + push do agentów).

---

## Co zostało zrobione (Kroki 1–9)

| Krok | Commit | Co zawiera |
|------|--------|------------|
| 1 | `2a141ce` | Cargo workspace + 14 crate skeletons, GPLv2, rust-toolchain, rustfmt, README |
| 2 | `b7c0e55` | gRPC schema (`proto/scrooge.proto`, 22 messages + 9 enums, tonic-build) |
| 3 | `b182fcb` | `scrooge-common`: error / config / CA / crypto + 14 testów jednostkowych |
| 4 | `f00b2ef` | 6 migracji PostgreSQL (agents, policies, events partycjonowane, auth) |
| 5–6 | `4ad3bad` | `scrooge-manager-bin` (gRPC + REST + JWT auth) + `scrooge-manager-api` + `scroogectl` CLI |
| 7–8 | `2d0e0df` | `scrooge-agent-core` (PlatformAgent trait) + Linux/macOS/Windows + `scrooge-agent-bin` (enrollment + persistent Stream + heartbeat + reconnect) |
| 6/8 dev | `2dc39fd` | Dev tooling: docker-compose, dev certs script, sample configs, Makefile `dev-*` targety |
| 9 | `49205cb` + `2520d6b` (fix) | `scroogectl init-ca` + `Dockerfile.manager` multi-stage cargo-chef + production `docker-compose.yml` + interaktywny `quickstart.sh` + `deploy/README.md` |
| 10 | `7d27966` | UTM VM workflow: `setup-ubuntu-vm.sh` + `rebuild-and-restart-agent.sh` + `scrooge-agent.service` (systemd) + `agent.vm.yaml.example` + `manager.dev-vm.yaml` (0.0.0.0 listen) + `dev-manager-vm` Makefile target + cert SAN z UTM Mac IP + `deploy/dev/README.md` (200-line UTM guide) |
| 11 | `f65dc21` | `.github/workflows/ci.yml` (fmt + clippy + test + build, matrix linux/macos, concurrency cancel-in-progress, Swatinem cache) + README CI/GPLv2 badges |
| 12 | _pending_ | `scripts/smoke-e2e.sh` (8-stage automated test) + `docs/E2E_TESTING.md` + `make e2e-test` / `e2e-test-keep` Makefile targety |

**Status weryfikacji:**
- `cargo build --workspace` ✅
- `cargo fmt --all -- --check` ✅
- `cargo clippy --workspace -- -D warnings` ✅
- `cargo test --workspace` ✅ — **16 testów** (14 w `scrooge-common`, 2 w `scrooge-agent-core`)
- **E2E lokalny** (Mac native + Postgres w Dockerze): ✅ — agent enrolled, heartbeats lecą, agent widoczny w `GET /api/v1/agents`

---

## Milestone 1 — KOMPLET ✅

Wszystkie 12 kroków zaimplementowane, zmergowane na `main`, zweryfikowane lokalnie.

## Po Milestone 1 (commits na `main`)

| Commit | Co |
|---|---|
| `a927bed` | fix Swagger `bearer_auth` security scheme (Authorize button działa) |
| `93ed515` | embedded web dashboard pod `GET /` (vanilla HTML + Tailwind CDN) |
| `27d216d` | konfigurowalny dashboard (auto-refresh + session timeouts via `manager.yaml`) |
| `40af82a` | ROADMAP.md — pełna mapa Phase 0-6 |
| `d19c652` | **Sub-faza 1A**: release CI workflow + .deb/.rpm metadata |
| `c8ca537` + `c1fb049` | fixy cargo-generate-rpm (package path + asset paths) |

## Sub-faza 1C — COMPLETE ✅

macOS + Windows installer przez curl-flow (bez code signing — patrz
`memory/project_distribution_policy.md`).

- **`TargetOs` enum** w `install.rs` — central registry: każdy wariant wie
  swój template, MIME, ścieżkę endpointa i format one-linera. Trzy thin
  handlery wrappują wspólny `render_for()` helper.
- **`GET /api/v1/install-macos.sh`** — bash. `uname -m` → tarball z
  GitHub Releases (`scrooge-agent-{x86_64|aarch64}-apple-darwin.tar.gz`),
  rozpakuje do `/usr/local/bin/scrooge-agent`, inline `agent.yaml` +
  `ca.pem` do `/usr/local/etc/scrooge/`, launchd plist
  `/Library/LaunchDaemons/com.sqtx.scrooge-agent.plist` + `launchctl
  bootstrap`. Idempotent (jeśli service istnieje, najpierw `launchctl
  bootout`).
- **`GET /api/v1/install.ps1`** — PowerShell 5.1+ `#Requires
  -RunAsAdministrator`. `Invoke-WebRequest` zip, `Expand-Archive` do
  `C:\Program Files\ScroogeDLP\`, agent.yaml + ca.pem w
  `C:\ProgramData\ScroogeDLP\` (UTF-8 bez BOM), `New-Service` z
  `-StartupType Automatic` + `sc.exe failure` (restart po 60s, max 3
  próby). Idempotent (Stop-Service + sc.exe delete przed re-install).
- **POST /agents/install** — install_command per OS:
  - Linux/macOS: `curl -fsSL '…' | sudo bash`
  - Windows: `iwr -UseBasicParsing '…' | iex`
- **Dashboard wizard** — macOS/Windows odblokowane w dropdownie. Stage 2
  pokazuje per-OS hint (Windows: „wklej w PowerShell (Administrator)").
- **4 nowe testy** (lacznie 23): macos/windows render, one_liner format,
  parse aliasów (`darwin` → `MacOs`).

**Smoke E2E** (curl na Macu): wszystkie 3 endpointy zwracają poprawny
content-type, no leftover placeholderów. `bash -n install-macos.sh` valid.
PowerShell parse niezweryfikowany — wymagałoby `brew install --cask
powershell`, do zrobienia ręcznie razem z faktycznym test'em na czystym
Windowsie.

**Co nie jest objęte tym commitem** (osobne sub-fazy / Phase 5):
- Uninstall scripts (`uninstall-{linux,macos,windows}.sh`/.ps1)
- macOS ARM Windows binarka (release.yml tylko x86_64-pc-windows-msvc)
- Auto-update agentów
- Code signing — celowo pominięte na MVP.

## Sub-faza 1D — COMPLETE ✅

Frontendowy klocek Wazuh-flow w embedded dashboardzie.

- „+ Install agent" w toolbarze → modal wizard.
- Stage 1: target_os dropdown (linux aktywny; macos/windows disabled
  z labelką „wkrótce") + opcjonalny `description`.
- Stage 2: `<pre>` z one-linerem + **Copy** button (navigator.clipboard)
  + niebieski indicator „Waiting for agent connection…".
- Wizard śledzi nowych agentów — gdy `renderAgents` zobaczy ID spoza
  `knownAgentIds`, robi toast „Agent connected: {hostname}" i zamienia
  indicator na zielony „✓ Connected: {hostname}". `hostname` wstawiany
  przez safe DOM (textContent), bo agent może go deklarować dowolnie.
- Wymaga `server.public_rest_base_url` w configu — gdy puste, wizard
  pokazuje błąd zamiast crashować.

## Sub-faza 1B — COMPLETE ✅

Wazuh-flow install API. Admin klika „Add agent" w dashboardzie → dostaje
`curl … | sudo bash` → agent sam się instaluje i rejestruje.

- `POST /api/v1/agents/install` (admin only) — generuje single-use enrollment
  token (UUID v4, SHA-256 w DB, 24h TTL, `max_uses=1`) + zwraca gotowy
  one-liner z URL'em do `/install.sh`.
- `GET /api/v1/install.sh?token=XYZ` (public — auth przez sam token) —
  server-rendered bash. Detect distro/arch, pobiera `.deb` / `.rpm` z
  GitHub Releases, instaluje, zapisuje CA do `/etc/scrooge/ca.pem`, sed'uje
  `agent.yaml`, enable+restart `scrooge-agent.service`. Idempotent.
- Nowy config: `server.public_grpc_endpoint` (gdzie agent łączy gRPC),
  `server.public_rest_base_url` (gdzie installer się ładuje),
  `server.agent_release_tag` (z którego GH release pobierać binarki).
- Nowy wariant błędu `ApiError::ServiceUnavailable` (503) — zwracany gdy
  któreś z 3 publicznych pól nie ustawione.

**Lokalna weryfikacja** (manager + Postgres na Macu):
- POST → response zawiera `install_command` z pełnym one-linerem
- GET → 200 `text/x-shellscript`, ~5 KB, `bash -n` valid, brak placeholderów
- GET z nieznanym tokenem → 401
- GET bez tokena → 400 (axum query deser)

**Pozostała weryfikacja** (do Sub-fazy 1C): faktyczne `curl … | sudo bash`
na czystym Ubuntu w UTM — uruchomić agenta, sprawdzić że enroll'uje i
heartbeat'y lecą do managera.

## Sub-faza 1A — COMPLETE ✅

GitHub Actions Release pipeline:
- Tag `v*` triggers build dla 5 platform: Linux x86_64/aarch64, macOS Intel/ARM, Windows
- `.deb` + `.rpm` dla Linux (cargo-deb + cargo-generate-rpm)
- Maintainer scripts: preinst (user), postinst (config), prerm, postrm (purge cleanup)
- Auto-upload do GitHub Releases

**Zweryfikowane:** `v0.1.0-rc3` release. `.deb` instalowany przez `apt install` na Ubuntu 24.04 (Docker test, 2026-05-12):
- user `scrooge` utworzony (uid 999, system)
- `/usr/bin/scrooge-agent` 4.7 MB, executable
- `/lib/systemd/system/scrooge-agent.service`
- `/etc/scrooge/agent.yaml` (auto-skopiowane z template przez postinst)
- `--version` zwraca poprawnie

## Znane TODO (przed Sub-fazą 1B)

_(brak — Sub-faza 1A w pełni zamknięta)_

Wcześniejsze TODO „Ubuntu 22.04 runners w release.yml" zostało zaadresowane —
build i package-linux jobs leca teraz na `ubuntu-22.04` / `ubuntu-22.04-arm`,
co daje broader glibc compat (Ubuntu 22.04 + 24.04 + Debian 12 + RHEL 9
itd.). Job `release` (publikacja artifacts) zostaje na `ubuntu-latest`, bo
nie linkuje binarek.

---

## Stan lokalnego dev environment

| Komponent | Stan | Gdzie |
|---|---|---|
| Postgres (Docker) | działa | `localhost:5433`, user `scrooge`, db `scrooge`, hasło `scrooge-dev-password` |
| Dev CA + manager cert | wygenerowane | `dev-certs/{ca,server}.{pem,key}` (gitignored) |
| Admin user | utworzony | `admin` / `admin123` (default dev) |
| Enrollment token | aktywny | `dev-certs/enrollment-token.txt` (gitignored) |
| Agent enrolled | tak | `agent_id: 734f33cb-aec7-4598-a13a-c5330b18fa08` |
| Agent state | persistent | `agent-data/agent.{cert,key,id}` (gitignored) |

### Jak uruchomić

```bash
# pierwszy raz (już zrobione):
make dev-up           # Postgres + dev certs
make dev-bootstrap    # migracje + admin + enrollment token

# codzienne:
make dev-manager      # terminal 1 (blokuje)
make dev-agent        # terminal 2 (blokuje)

# weryfikacja:
TOKEN=$(curl -s -X POST http://127.0.0.1:55000/api/v1/auth/login \
  -H 'Content-Type: application/json' \
  -d '{"username":"admin","password":"admin123"}' | jq -r .access_token)
curl -s -H "Authorization: Bearer $TOKEN" http://127.0.0.1:55000/api/v1/agents | jq .

# sprzątanie:
make dev-down                 # zatrzymaj Postgres (volume zostaje)
make dev-agent-reset          # wyczyść agent-data (force re-enrollment)
```

---

## Kluczowe decyzje projektowe (już podjęte)

1. **RPC `Stream` zamiast `Connect`** — tonic generuje `connect()` jako konstruktor klienta, kolizja nazw.
2. **`PRIMARY KEY (id, timestamp)`** w `events` — wymóg PG dla partycjonowanych tabel.
3. **Brak FK `events → agents`** — zachowujemy historię eventów z usuniętych agentów (forensics).
4. **`TEXT + CHECK` zamiast PG ENUM** — łatwiej dodawać wartości bez `ALTER TYPE`.
5. **Enum `_UNSPECIFIED`** sentinele w proto (proto3 best practice / Google AIP-126).
6. **`sqlx::query_as` non-macro** zamiast `query!` macro — nie wymaga DATABASE_URL przy kompilacji. Migracja na compile-time check przy pipeline z dev DB.
7. **mTLS substytut: `agent-id` w gRPC metadata** — pełne client cert validation w późniejszym refactorze (Enroll z definicji jest pre-cert).
8. **rcgen + PKCS#8 klucze** — `openssl ecparam -genkey` daje legacy SEC1 którego rcgen 0.13 nie parsuje; używamy `openssl genpkey -algorithm EC`.
9. **clippy::pedantic z 5 wyjątkami** — `module_name_repetitions`, `must_use_candidate`, `missing_errors_doc`, `missing_panics_doc`, `doc_markdown`.
10. **Manager binary natywnie w dev** (`cargo run`), tylko Postgres w Dockerze — szybsza iteracja niż docker build. Pełny Docker manager w Kroku 9.

---

## Znany dług techniczny (TODO)

- **mTLS validation client cert** dla `Stream` RPC (dziś tylko `agent-id` metadata)
- **sqlx::query! compile-time check** — refactor po stworzeniu dev DB pipeline (`cargo sqlx prepare`)
- **Auto-create przyszłych partition'ów `events`** — w MVP tylko `events_2026_01` + default; produkcja wymaga `pg_partman` lub crona
- **Agent SQLite local queue** — brief Kroku 8 wspomina, ale w MVP wystarczy pliki. Pełny SQLite event queue offline w Milestone 3+
- **CA roundtrip nie jest bit-exact** — `RootCa::load_from_files` re-issuuje cert (zachowuje cryptographic identity przez ten sam key + DN). OK dla pinningu po public key.
- **`os_info` parsuje na macOS jako "Mac OS 26.4.1"** — minor, może warto użyć "macOS" jako prefix.

---

## Architektura wysokopoziomowa (działa już teraz)

```
┌─────────────────────────────────────────────────────────────┐
│           Mac (dev)                                          │
│                                                              │
│  cargo run scrooge-agent ──┐                                │
│   (target/debug)            │                                │
│                             │ gRPC/TLS                       │
│                             │ z agent-id metadata            │
│                             ▼                                │
│                   cargo run scrooge-manager                  │
│                    :5443 (gRPC)   :55000 (REST + Swagger UI) │
│                          │              │                    │
│                          └──┬───────────┘                    │
│                             ▼                                │
│                    Docker: postgres:16-alpine                │
│                    :5433 (zewnątrz) → :5432 (kontener)       │
└─────────────────────────────────────────────────────────────┘
```

---

## Co Claude (next session) musi wiedzieć żeby kontynuować

1. **Repo**: <https://github.com/SQTX/Scrooge-DLP>, branch `main`
2. **Conventional commits**, autor `SQTX <sssqtx@gmail.com>`
3. **Workflow z briefu**: krok po kroku, pokazać propozycję, czekać na akceptację. Po Kroku 7 user przeszedł na minimalistyczne potwierdzenia („lecimy", „dawaj") — można skompresować propozycje.
4. **Aktualnie zatrzymani na**: Krok 9 — pytanie CA gen (Rust vs openssl)
5. **PROJECT_BRIEF.md** w roocie ma pełny plan i wszystkie decyzje pre-resolved
6. **Dev env już postawione** — Postgres działa w Dockerze, admin user istnieje, agent enrolled. Nie trzeba rebuilduować przy każdym restarcie sesji.

---

*Plik aktualizowany ręcznie po każdym kroku — jeśli widzisz że nie zgadza się z `git log`, sprawdź który jest źródłem prawdy.*
