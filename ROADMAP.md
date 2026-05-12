# ScroogeDLP — Roadmap

Pełna mapa rozwoju projektu, podzielona na fazy. Każda faza ma konkretne
zadania z opisem. Hierarchia jest 2-poziomowa (zadanie + szczegóły),
dzięki czemu łatwo skopiować do Apple Reminders.

---

## Phase 0: Fundament (DONE)

Pełna infrastruktura komunikacji agent ↔ manager, REST API z auth,
embedded dashboard. Nie ma jeszcze funkcji DLP samych w sobie.

### Milestone 1 — Manager + Agent skeleton
- [x] Krok 1: Cargo workspace z 14 crates + GPLv2 + rust-toolchain
- [x] Krok 2: gRPC schema (22 messages, 9 enums, tonic-build)
- [x] Krok 3: scrooge-common (error, config, CA, crypto + 14 testów)
- [x] Krok 4: 6 migracji PostgreSQL (agents, policies, events partycjonowane, auth)
- [x] Krok 5: Manager-bin (tokio + gRPC server stubs + sqlx migrate)
- [x] Krok 6: REST API (axum + JWT auth + Swagger UI + scroogectl)
- [x] Krok 7: PlatformAgent trait + Linux/macOS/Windows stubs
- [x] Krok 8: Agent enrollment + persistent Stream + heartbeat + reconnect
- [x] Krok 9: Production Docker + quickstart.sh
- [x] Krok 10: UTM VM workflow (setup script + systemd unit + dev guide)
- [x] Krok 11: GitHub Actions CI (fmt + clippy + test + build matrix)
- [x] Krok 12: E2E smoke test script + docs/E2E_TESTING.md

### Bonusy po Milestone 1
- [x] Embedded web dashboard pod `GET /` (vanilla HTML + Tailwind CDN)
- [x] Fix Swagger UI bearer_auth security scheme
- [x] Konfigurowalny dashboard: auto-refresh + session timeouts via `manager.yaml`

---

## Phase 1: Wazuh-flow Distribution (W TRAKCIE — 1A done)

Cel: instalacja agenta przez **jeden one-liner** — admin klika „Add agent"
w dashboardzie, dostaje `curl ... | sudo bash`, klejsze na końcówce, agent
sam się instaluje i zarejestruje. Tak jak Wazuh.

### Sub-faza 1A — Release CI pipeline ✅ DONE
- [x] Utwórz `.github/workflows/release.yml` (trigger: tag `v*`)
- [x] Matrix build: Linux x86_64 + Linux aarch64
- [x] Matrix build: macOS x86_64 (Intel) + macOS aarch64 (Apple Silicon)
- [x] Matrix build: Windows x86_64
- [x] Packaging Linux: `.deb` przez cargo-deb
- [x] Packaging Linux: `.rpm` przez cargo-generate-rpm
- [x] Maintainer scripts (preinst/postinst/prerm/postrm) — auto-systemd setup po `apt install`
- [x] Upload wszystkich artifacts do GitHub Release on tag
- [x] **Zweryfikowano `v0.1.0-rc3`** — `.deb` instalowany przez `apt install` na Ubuntu 24.04
- [ ] **TODO**: zmień runner z `ubuntu-latest` / `ubuntu-24.04-arm` na `ubuntu-22.04*` (broader glibc compat — aktualnie `.deb` wymaga 24.04+)
- [ ] Packaging macOS: `.pkg` przez pkgbuild (Sub-faza 1C — wymaga code signing)

### Sub-faza 1B — Install API
- [ ] `POST /api/v1/agents/install` — generuje token (max_uses=1, 24h) + zwraca one-liner
- [ ] `GET /api/v1/install.sh?token=XYZ` — server-rendered bash z embedded manager endpoint i CA
- [ ] Walidacja: target_os enum (linux/macos/windows)
- [ ] Auth: tylko admin może wywołać (Claims with role check)

### Sub-faza 1C — Installer scripts
- [ ] `deploy/installers/install-linux.sh` — detect arch, download binary, systemd unit, config
- [ ] `deploy/installers/install-macos.sh` — launchd plist + .pkg fallback
- [ ] `deploy/installers/install-windows.ps1` — PowerShell, Windows service
- [ ] Test każdy installer ręcznie na świeżej VM/maszynie

### Sub-faza 1D — Dashboard wizard
- [ ] „Add agent" modal: dropdown OS + opcjonalny hostname
- [ ] Po submit: wywołanie POST /agents/install + wyświetlenie one-liner
- [ ] Copy-to-clipboard button
- [ ] Live indicator „Waiting for agent connection..."
- [ ] Auto-detect nowego agenta + toast „Agent connected: {hostname}"

---

## Phase 2: Polityki DLP (Milestone 2)

Pierwsza prawdziwa DLP-specific warstwa. Po tym manager zarządza
politykami, ale jeszcze nic ich nie enforce'uje (to faza 3).

### Backend
- [ ] `scrooge-policy` crate: pełen parser YAML (przykład w PROJECT_BRIEF)
- [ ] Walidacja struktury polityki (apiVersion, kind, metadata, targets, rules)
- [ ] Kompilacja YAML → binarna forma (msgpack) + SHA-256 hash
- [ ] Wersjonowanie polityk + zapis do `policy_history`

### REST API
- [ ] `GET /api/v1/policies` — lista
- [ ] `POST /api/v1/policies` — create (admin only)
- [ ] `GET/PUT/DELETE /api/v1/policies/{name}`
- [ ] `GET /api/v1/policies/{name}/history`
- [ ] `POST /api/v1/policies/{name}/rollback/{version}`
- [ ] `POST /api/v1/policies/validate` — dry-run walidacji YAML

### Push do agentów
- [ ] Mechanizm delta-push (manager wysyła tylko polityki których agent nie ma — porównanie po hash)
- [ ] Agent: walidacja przychodzącej polityki, zapis do local store, ack
- [ ] `PolicyAck` message → manager update'uje `policy_assignments.acked_version`

### Dashboard
- [ ] Tabela polityk z kolumnami: name, version, enabled, priority, last updated
- [ ] YAML editor (Monaco lub plain textarea z syntax hint)
- [ ] „Affected agents" preview przed save
- [ ] Rollback button per wersja

---

## Phase 3: Detekcja DLP (Milestones 3-4)

Pierwsze realne eventy DLP zaczynają płynąć od agentów. Klasyfikator
wykrywa wrażliwe dane.

### Milestone 3 — Clipboard, USB, lokalna kolejka
- [ ] `mod_clipscreen` (Linux + macOS) — monitoring schowka przez `arboard`
- [ ] Detekcja PrintScreen + popularnych narzędzi screenshot'ów
- [ ] `mod_devctl` (Linux) — USB hot-plug przez `tokio-udev`
- [ ] `mod_devctl` (macOS) — USB przez IOKit
- [ ] SQLite local event queue (offline mode dla agenta)
- [ ] Batch upload eventów do managera (gdy online)

### Milestone 4 — Filesystem + classifier
- [ ] `mod_filemon` (Linux + macOS) — file events przez `notify` crate
- [ ] `mod_classifier`: regex + walidator Luhn (karty kredytowe)
- [ ] `mod_classifier`: walidator PESEL (mod-11)
- [ ] `mod_classifier`: walidator IBAN (checksum)
- [ ] `mod_classifier`: walidator NIP
- [ ] Skanowanie zawartości plików przy file event (gdy reguła wymaga)
- [ ] Manager: zapis eventów do partycjonowanej tabeli `events`
- [ ] Dashboard: lista eventów z filtrami (severity, type, agent, time range)

---

## Phase 4: Sieć + Akcje (Milestones 5-7)

Pełne DLP — wykrywanie próby exfiltracji i blokowanie.

### Milestone 5 — Wazuh integration
- [ ] `scrooge-syslog` crate: formatter RFC 5424
- [ ] TLS connection (RFC 5425) z fallbackiem TCP
- [ ] Filtrowanie eventów po severity i `forward_to_siem` flag
- [ ] Wazuh decoder XML + rules XML w `deploy/wazuh-integration/`
- [ ] Dokumentacja instalacji w Wazuh

### Milestone 6 — Network monitoring
- [ ] `mod_netinsp` (Linux) — eBPF lub netfilter
- [ ] `mod_netinsp` (macOS) — NetworkExtension (wymaga entitlements)
- [ ] Detekcja uploadów do Dropbox / GoogleDrive / WeTransfer po SNI
- [ ] Klasyfikacja destynacji (cloud storage, email, transfer service)

### Milestone 7 — Egzekucja akcji
- [ ] `mod_response`: LOG_ONLY, ALERT, WARN_USER, BLOCK, QUARANTINE, KILL_PROCESS, LOCK_WORKSTATION
- [ ] Quarantine: AES-256-GCM encryption + zapis do `/var/lib/scrooge/quarantine/`
- [ ] User notification (desktop notification per OS)
- [ ] Clipboard clear przy detekcji wrażliwej zawartości

---

## Phase 5: Production polish

Dojrzałość, security hardening, observability.

### Security
- [ ] Pełne mTLS validation (replace `agent-id` metadata substytut)
- [ ] Cert rotation dla agentów (auto-renew przed expiry)
- [ ] Audit log w DB dla każdej akcji admina (login, policy change, token gen)
- [ ] Rate limiting REST API per IP / token
- [ ] Sealed secrets w configu (lub integracja z Vault)

### Observability
- [ ] Prometheus metrics endpoint `/metrics` na managerze
- [ ] Tracing spans przez requesty (OpenTelemetry)
- [ ] Structured logging best practices (correlation IDs)
- [ ] Health checks (readiness vs liveness)

### Operations
- [ ] Backup / restore CLI dla DB (`scroogectl db backup`)
- [ ] Auto-cleanup wygasłych refresh tokens (cron job)
- [ ] Auto-cleanup starych eventów (retention policy)
- [ ] Sample policies w `policies/` (finance, healthcare, dev shop, generic)
- [ ] Performance benchmarks (heartbeat throughput, event ingestion rate)
- [ ] Documentation site (mdBook lub Docusaurus)

---

## Phase 6: Beyond MVP

Long-term features. Nie planowane na konkretną datę, dochodzą gdy projekt
dojrzeje.

### Platforma
- [ ] Windows agent — pełna implementacja (Milestone 8+, wymaga maszyny Windows do testów)
- [ ] Windows minifilter driver w C/WDK (osobny milestone, znaczna praca)
- [ ] Linux eBPF advanced (kernel hooks dla file access poniżej user-space)

### Skala
- [ ] HA setup (multiple managers za load balancerem, shared DB)
- [ ] Multi-tenancy (jeden manager → wiele organizacji izolowanych)
- [ ] Read replicas dla query-heavy operations
- [ ] Event ingestion przez Kafka (gdy SQL bottleneck)

### Funkcjonalność zaawansowana
- [ ] Auto-update agentów (manager pushuje nową wersję, agent restartuje się)
- [ ] TLS interception (MITM SSL inspection — wymaga deployment cert na endpointach)
- [ ] Microsoft Sensitivity Labels integration (czytanie metadanych Office365)
- [ ] Document fingerprinting (SHA-256 + rolling hash dla podobnych dokumentów)
- [ ] Self-protection / anti-tamper (watchdog process pilnuje agenta)

### Dashboard rozszerzenia
- [ ] Real-time WebSocket dla live updates (zamiast HTTP poll)
- [ ] Charts / metrics dashboard (Recharts lub Chart.js)
- [ ] Incident timeline view
- [ ] Rule editor wizard (UI zamiast YAML)
- [ ] User management UI (admin/analyst/viewer roles)

### Ekosystem
- [ ] CLI rozszerzenia: `scroogectl events list`, `policies push`, `agent stats`
- [ ] SDK / client libraries: Python, Go, TypeScript
- [ ] Mobile app (read-only — incident alerts + agent status)
- [ ] Slack / Teams integration (alert notifications)
- [ ] SIEM integrations beyond Wazuh: Splunk, Elastic, Sentinel

---

## Jak skopiować do Apple Reminders

**Sposób 1 — manualnie (najprostszy):**
1. Otwórz Reminders → utwórz nową listę „ScroogeDLP"
2. Skopiuj nagłówek fazy → wklej jako zadanie
3. Pod każdym zadaniem Tab + paste sub-zadań

**Sposób 2 — Shortcuts (Apple):**
1. Skrót: „Get Contents of Clipboard" → „Split text by new line" → loop „Add new Reminder"
2. Sub-tasks zachowują się jeśli linie zaczynają się od „  - "

**Sposób 3 — przepisanie selektywne:**
- Najprościej: tylko aktualna faza jako lista zadań w Reminders
- Następne fazy zostawić tu w pliku jako reference

**Tip:** każdy `- [ ]` to zadanie. Wcięcia (2 spacje) wyznaczają hierarchię.
Tytuły faz (`## Phase X`) są dobre jako nazwy list lub osobne projekty.

---

## Aktualizacja tego pliku

Po zakończeniu każdej sub-fazy edytuj ten plik (`- [ ]` → `- [x]`) i commit
jako `docs: roadmap update`. Status w tym pliku jest źródłem prawdy
oprócz `git log`.
