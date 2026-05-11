# ScroogeDLP - Project Brief

## Cel projektu

Buduję od zera kompleksowy system **Data Loss Prevention (DLP)** o nazwie **ScroogeDLP**, inspirowany architekturą Wazuh, ale skupiony wyłącznie na funkcjach DLP (bez SIEM/log analysis - to robi już Wazuh, z którym się integrujemy przez syslog).

System składa się z trzech komponentów:
1. **Agent** (`scrooge-agent`) - instalowany na końcówkach (Windows, macOS, Linux), zbiera dane i egzekwuje polityki
2. **Manager** (`scrooge-manager`) - serwer centralny na Linuksie (Docker + skrypt quickstart), zarządza agentami, politykami, eventami, eksponuje REST API i opcjonalnie forwarduje syslog do Wazuha
3. **Dashboard** (`scrooge-dashboard`) - osobne repo, React + TypeScript, **NIE jest częścią tego briefu**

## Licencja

**GPLv2** (jak Wazuh). Każdy plik źródłowy ma na początku SPDX identifier i header licencji GPLv2.

## Repozytorium

- **Publiczne**, na GitHubie
- Branche: `main` (stabilne wydania), `develop` (integracja), `feature/*` (per milestone lub feature)
- Bez force push na `main` / `develop`
- Conventional Commits: `feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `chore:`
- Tagi SemVer dla wydań: `v0.1.0-mvp.1`

## Stack technologiczny

- **Język główny**: Rust (edition 2021, MSRV 1.75+)
- **Async runtime**: Tokio (full features)
- **Komunikacja agent ↔ manager**: gRPC (tonic 0.12) + mTLS, bidirectional streaming
- **Schemat**: Protocol Buffers (prost 0.13)
- **REST API (manager)**: axum 0.7 + utoipa (OpenAPI 3.0 generation)
- **WebSocket** (manager → dashboard real-time): axum WebSocket
- **Auth dashboardu**: JWT lokalny (login/hasło, bcrypt), refresh tokens, RBAC (admin/analyst/viewer)
- **Serializacja konfiguracji/polityk**: serde + serde_yaml
- **Baza managera**: PostgreSQL 16 (sqlx z compile-time checks)
- **Baza lokalna agenta**: SQLite (offline cache eventów, sqlx)
- **Logging**: tracing + tracing-subscriber (JSON output w produkcji)
- **TLS**: rustls 0.23
- **Crypto**: ring 0.17, rcgen do generowania certów
- **Syslog out (do Wazuh)**: TCP z TLS (RFC 5425), domyślnie WYŁĄCZONY
- **Build**: Cargo workspace, multi-crate
- **CI/CD**: GitHub Actions (build + clippy + test, matrix Linux/macOS)
- **Deployment managera**: Docker Compose + skrypt quickstart.sh
- **Instalatory agentów**: WiX/MSI (Windows), cargo-deb/cargo-generate-rpm (Linux), pkgbuild (macOS) - faza późniejsza

## Architektura wysokopoziomowa

```
                            ┌─────────────────────┐
                            │ ScroogeDLP Dashboard│
                            │ (React, osobne repo)│
                            └──────────┬──────────┘
                                       │ REST + WebSocket (mTLS)
                                       │ port 55000
                            ┌──────────▼──────────┐
                            │   Manager API       │
                            │   (axum)            │
                            └──────────┬──────────┘
                                       │
       ┌───────────────────────────────┼───────────────────────────────┐
       │                               │                               │
┌──────▼──────┐               ┌────────▼────────┐              ┌──────▼──────┐
│ gRPC server │◄─────────────►│  Core Manager   │─────────────►│ Syslog Out  │
│ port 5443   │               │  - policy eng   │   (opt)      │ RFC 5425    │
│ (mTLS)      │               │  - correlator   │              │ TCP+TLS     │
└──────┬──────┘               │  - event store  │              │ port 6514   │
       │                      └────────┬────────┘              └──────┬──────┘
       │                               │                              │
       │                      ┌────────▼────────┐                     │
       │                      │   PostgreSQL    │                     │
       │                      └─────────────────┘                     │
       │                                                              │
┌──────▼──────────────────────────────┐                       ┌──────▼──────┐
│ Agents: macOS / Linux / (Windows)   │                       │   Wazuh     │
│ - mod_filemon, mod_devctl,          │                       │  Manager    │
│ - mod_clipscreen, mod_netinsp,      │                       │ + ScroogeDLP│
│ - mod_classifier, mod_response      │                       │   decoder   │
└─────────────────────────────────────┘                       └─────────────┘
```

## Single-tenancy

MVP jest **single-tenant** - jeden manager obsługuje jedną organizację. Multi-tenancy nie jest na ten moment przedmiotem prac.

## Środowisko deweloperskie i testowe

### Workflow programisty

- **Stacja deweloperska**: macOS (Apple Silicon lub Intel) - tu piszę kod
- **Środowisko testowe**: Ubuntu 22.04 LTS na **UTM** (https://mac.getutm.app) - tu testuję agenta
- **Git jako most**: kod na Macu → `git push` → na VM `git pull` → build & run

### Priorytety platformowe w MVP

1. **macOS** - rozwój i pierwsze testy lokalne (agent + manager build na Macu)
2. **Linux (Ubuntu)** - testy agenta na UTM, manager w Dockerze
3. **Windows** - **NIE w MVP**. Kod platform-specific dla Windows jest **stub'owany** (cfg-gated, kompiluje się ale metody zwracają `unimplemented!()`), pełna implementacja w późniejszej fazie kiedy będzie dostępna maszyna Windows

To znaczy: crate `scrooge-agent-windows/` istnieje (struktura, trait impl), ale CI go nie buduje na `windows-latest` w MVP. Włączenie tego to osobny milestone.

### Setup UTM VM (Ubuntu 22.04)

**Wymagania VM**:
- Ubuntu 22.04 LTS Server (ARM64 dla Apple Silicon, AMD64 dla Intel Mac)
- Min 2 vCPU, 4GB RAM, 20GB dysk
- Sieć: tryb **Shared Network** w UTM (NAT z dostępem host→VM i VM→internet)
  - VM dostaje IP w `192.168.64.x` (typowo)
  - Mac jest osiągalny dla VM jako `192.168.64.1` (gateway)
- SSH dostęp z Maca (klucz SSH wgrany do VM)

**Konfiguracja sieci w UTM** (dokumentacja w `deploy/dev/README.md` ma to opisać):
1. Nowa VM → Virtualize → Linux
2. Pobierz Ubuntu 22.04 Server ISO (ARM64 dla M1/M2/M3)
3. Network: `Shared Network` (default)
4. Po instalacji: `ip addr` w VM pokazuje IP, np. `192.168.64.5`
5. Z Maca: `ssh ubuntu@192.168.64.5`
6. Mac w VM jako `192.168.64.1` (gateway)

### Cross-platform Cargo cfg

Agent automatycznie wybiera moduły per OS przez `cfg(target_os = ...)`:

```toml
# crates/scrooge-agent-bin/Cargo.toml
[target.'cfg(target_os = "linux")'.dependencies]
scrooge-agent-linux = { path = "../scrooge-agent-linux" }

[target.'cfg(target_os = "macos")'.dependencies]
scrooge-agent-macos = { path = "../scrooge-agent-macos" }

[target.'cfg(target_os = "windows")'.dependencies]
scrooge-agent-windows = { path = "../scrooge-agent-windows" }
```

Trait `PlatformAgent` w `scrooge-agent-core` definiuje API platformowe, każdy crate platformowy go implementuje.

### Build strategia: native na VM (MVP)

Zaczynamy od **najprostszej opcji**:
- Programujesz na Macu, pushujesz do gita
- Na Ubuntu VM: `git pull && cargo build --release && systemctl restart scrooge-agent`
- Build na VM trwa dłużej, ale setup jest trywialny

**Cross-compilation z macOS** (przez `cross`) jest opcją późniejszą - skrypty są przygotowane ale defaultem jest native build na VM.

### Dev scripts do przygotowania (Claude Code zrobi w Kroku 10)

1. **`deploy/dev/setup-ubuntu-vm.sh`** - idempotentny, do uruchomienia na świeżej VM:
   - Instaluje Rust (rustup), Docker, Docker Compose, build-essentials, libssl-dev, pkg-config, protobuf-compiler, git
   - Klonuje repo z GitHuba (parametr URL)
   - Tworzy systemd service unit `/etc/systemd/system/scrooge-agent.service`
   - Konfiguruje firewall (`ufw`) z otwartymi portami testowymi
   - NIE uruchamia jeszcze agenta - czeka na pierwszy build

2. **`deploy/dev/rebuild-and-restart-agent.sh`** - na VM, po `git pull`:
```bash
   git pull
   cargo build --release -p scrooge-agent-bin
   sudo systemctl stop scrooge-agent
   sudo cp target/release/scrooge-agent /usr/local/bin/
   sudo systemctl start scrooge-agent
   sudo journalctl -u scrooge-agent -f
```

3. **`deploy/dev/run-manager-local.sh`** - na Macu, do testów managera lokalnie:
```bash
   docker compose -f deploy/docker/docker-compose.yml up
```
   Manager dostępny na `localhost:5443` (gRPC) i `localhost:55000` (REST).

4. **`Makefile`** w roocie projektu:
```makefile
   dev-manager:
       ./deploy/dev/run-manager-local.sh
   
   dev-build-agent-linux:
       cross build --target x86_64-unknown-linux-gnu --release -p scrooge-agent-bin
   
   fmt:
       cargo fmt --all
   
   lint:
       cargo clippy --workspace --all-targets -- -D warnings
   
   test:
       cargo test --workspace
   
   check-all: fmt lint test
```

5. **`deploy/dev/README.md`** - kompletna instrukcja:
   - Jak skonfigurować UTM (krok po kroku, screenshots opcjonalnie)
   - Jak skonfigurować sieć (UTM Shared Network)
   - Jak ustawić SSH key
   - First-time setup VM
   - Daily workflow (push z Maca → pull na VM → build → test)
   - Troubleshooting (VM nie widzi Maca, agent nie łączy się z managerem)

### Scenariusz testowy E2E (po Milestone 1)

To powinno działać po skończeniu M1:

**Na Macu**:
```bash
git push origin develop
make dev-manager  # manager startuje lokalnie w Dockerze, port 5443 + 55000
./deploy/quickstart.sh  # generuje admin usera, enrollment token
```

**Na VM Ubuntu (UTM)**:
```bash
ssh ubuntu@192.168.64.5
cd ~/scrooge-dlp
git pull
./deploy/dev/rebuild-and-restart-agent.sh
```

**Konfiguracja agenta na VM** (`/etc/scrooge/agent.yaml`):
```yaml
manager:
  endpoint: "192.168.64.1:5443"  # Mac jako gateway w UTM
  enrollment_token: "<TOKEN_Z_QUICKSTART>"
  verify_tls: true
  ca_cert_path: "/etc/scrooge/ca.pem"
agent:
  data_dir: "/var/lib/scrooge"
  log_level: "info"
```

**Weryfikacja na Macu**:
```bash
# Login do API
TOKEN=$(curl -s -X POST http://localhost:55000/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"..."}' | jq -r .access_token)

# Lista agentów
curl -H "Authorization: Bearer $TOKEN" http://localhost:55000/api/v1/agents
# Powinno zwrócić zarejestrowanego agenta z VM
```

## CI/CD - GitHub Actions

Plik `.github/workflows/ci.yml` w MVP:

```yaml
name: CI

on:
  push:
    branches: [main, develop]
  pull_request:
    branches: [main, develop]

env:
  CARGO_TERM_COLOR: always
  RUSTFLAGS: "-D warnings"

jobs:
  fmt:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt
      - run: cargo fmt --all -- --check

  clippy:
    strategy:
      matrix:
        os: [ubuntu-latest, macos-latest]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy
      - uses: arduino/setup-protoc@v3
      - uses: Swatinem/rust-cache@v2
      - run: cargo clippy --workspace --all-targets -- -D warnings

  test:
    strategy:
      matrix:
        os: [ubuntu-latest, macos-latest]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: arduino/setup-protoc@v3
      - uses: Swatinem/rust-cache@v2
      - run: cargo test --workspace
  
  build:
    strategy:
      matrix:
        os: [ubuntu-latest, macos-latest]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: arduino/setup-protoc@v3
      - uses: Swatinem/rust-cache@v2
      - run: cargo build --workspace --release
```

**Co to robi**:
- Na każdy push i PR do `main` lub `develop`
- Sprawdza format kodu (`cargo fmt --check`)
- Uruchamia clippy na Linux i macOS
- Uruchamia testy na Linux i macOS
- Buduje release na Linux i macOS
- Cache zależności przez `Swatinem/rust-cache`

**Windows w CI**: NIE w MVP, dodajemy później razem z implementacją `scrooge-agent-windows`.

## Funkcjonalności DLP do zaimplementowania

### Agent - moduły monitorujące

1. **mod_filemon** - operacje na plikach
   - User-mode na MVP: crate `notify` (cross-platform inotify/FSEvents/ReadDirectoryChangesW)
   - Windows dodatkowo: USN Journal poll dla NTFS
   - Eventy: create, read, write, delete, rename, copy, move
   - Docelowo Windows: minifilter driver w C/WDK (osobna faza, NIE w MVP)

2. **mod_devctl** - kontrola nośników
   - USB, dyski zewnętrzne, MTP, drukarki
   - macOS: IOKit framework
   - Linux: udev (przez `tokio-udev` lub `udev` crate)
   - Windows: SetupAPI (po MVP)
   - Whitelist po serial number / device instance ID
   - Blokowanie zapisu na niezatwierdzone nośniki (faza po MVP - na razie tylko detekcja)

3. **mod_clipscreen** - schowek i screenshoty
   - Monitoring schowka (cross-platform: `arboard` crate) + skanowanie classifierem
   - Detekcja PrintScreen i narzędzi screenshotowych (platform-specific)
   - Możliwość wyczyszczenia schowka przy detekcji wrażliwych danych

4. **mod_netinsp** - inspekcja sieci
   - Metadane połączeń: WFP (Windows), eBPF (Linux), NetworkExtension (macOS)
   - Detekcja uploadów do chmur (Dropbox, Google Drive, OneDrive, WeTransfer) po SNI
   - TLS interception: NIE w MVP, faza późniejsza

5. **mod_classifier** - klasyfikacja treści
   - Reguły YAML z patternami regex
   - Walidatory: Luhn (karty kredytowe), PESEL (mod-11), NIP, IBAN
   - Fingerprinting dokumentów (SHA-256 + rolling hash) - faza po MVP
   - Wsparcie dla Microsoft Sensitivity Labels - faza po MVP

6. **mod_response** - egzekucja akcji
   - LOG_ONLY, ALERT, WARN_USER, BLOCK, QUARANTINE, KILL_PROCESS, LOCK_WORKSTATION
   - Lokalna kwarantanna w `/var/lib/scrooge/quarantine/` (Linux/macOS) lub `%PROGRAMDATA%\ScroogeDLP\quarantine\` (Windows), zaszyfrowana AES-256-GCM

### Manager - komponenty

1. **agent-registry** - rejestracja agentów (CSR + wystawianie cert przez wewnętrzne CA)
2. **policy-engine** - kompilacja polityk z DB do binarnego formatu, wersjonowanie, push do agentów (delta po hashu)
3. **event-collector** - odbiór eventów, deduplikacja, batch insert do PostgreSQL
4. **alert-correlator** - reguły korelacyjne (np. "10 plików z `\Finanse\` na USB w 2 min = incident high")
5. **response-orchestrator** - wysyłanie komend do agentów
6. **api-rest** - REST API (axum + utoipa) dla dashboardu i CLI
7. **websocket-broker** - real-time stream eventów i statusów do dashboardu
8. **syslog-forwarder** - opcjonalny export eventów do Wazuha w formacie RFC 5424 over TCP+TLS (RFC 5425), DOMYŚLNIE WYŁĄCZONY

## Polityki - źródło prawdy

**Baza danych (PostgreSQL) jest źródłem prawdy.** Polityki tworzy się przez REST API / dashboard. YAML jest formatem **import/export** - można wkleić polityki do edytora w dashboardzie albo wysłać przez API jako YAML, manager parsuje i zapisuje do DB.

Możliwość przejścia do hybrydy z file watcherem (GitOps-style) jest planowana jako późniejszy milestone, ale NIE jest w MVP.

## Format polityki YAML (import/export)

```yaml
apiVersion: scroogedlp.io/v1
kind: Policy
metadata:
  name: finance-strict
  description: "Strict policy for finance department"
  version: 12
  
targets:
  match:
    os: ["windows", "macos"]
    tags:
      department: "finance"
    groups:
      - "finance-team"
      - "executives"
  exclude:
    hostnames: ["finance-test-vm-01"]

priority: 100

rules:
  inbound:
    - id: finance-incoming-monitor
      name: "Monitor data coming into finance folders"
      enabled: true
      severity: medium
      sources:
        - type: usb
        - type: network_download
          domains_except: ["sharepoint.company.com"]
        - type: email_attachment
      destinations:
        - type: directory
          paths:
            - "C:\\Finance\\**"
            - "%USERPROFILE%\\Documents\\Finance\\**"
          recursive: true
      conditions:
        - file_size_min: 1KB
        - file_extensions: ["xlsx", "xls", "csv", "pdf", "docx"]
      action: log_and_alert
      forward_to_siem: false
      
  outbound:
    - id: finance-no-usb-export
      name: "Block finance files copied to USB"
      enabled: true
      severity: high
      sources:
        - type: directory
          paths: ["C:\\Finance\\**"]
          recursive: true
        - type: file_match
          classifiers:
            - "credit-card-numbers"
            - "polish-pesel"
            - "iban-numbers"
      destinations:
        - type: usb
          except_serials: ["AA1234567890"]
        - type: network_upload
          domains: ["*.dropbox.com", "wetransfer.com"]
        - type: clipboard
        - type: print
      action: block
      notify_user: true
      message: "Kopiowanie plików finansowych poza firmę jest zabronione"
      forward_to_siem: true
      
    - id: confidential-filename-pattern
      name: "Block files with confidential naming patterns"
      enabled: true
      severity: critical
      sources:
        - type: any
      conditions:
        - filename_regex: "^(CONFIDENTIAL|POUFNE|TAJNE)_.*\\.(docx|pdf|xlsx)$"
        - path_regex: ".*\\\\(secret|tajne|poufne)\\\\.*"
      destinations:
        - type: any_external
      action: block
      forward_to_siem: true

monitored_paths:
  - path: "C:\\Finance"
    recursive: true
    events: [create, modify, delete, rename, access]
    
  - path: "%USERPROFILE%\\Desktop"
    recursive: false
    events: [create, modify]
    file_patterns:
      include: ["*.xlsx", "*.docx", "*.pdf"]
      exclude: ["~$*", "*.tmp"]

classifiers:
  - id: credit-card-numbers
    type: regex_with_validator
    pattern: '\b(?:\d[ -]*?){13,19}\b'
    validator: luhn
    min_matches: 1
    
  - id: polish-pesel
    type: regex_with_validator
    pattern: '\b\d{11}\b'
    validator: pesel_mod11
    min_matches: 1
    
  - id: iban-numbers
    type: regex_with_validator
    pattern: '\b[A-Z]{2}\d{2}[A-Z0-9]{1,30}\b'
    validator: iban_checksum
    min_matches: 1
```

**Kluczowe założenia formatu**:
- Deklaratywny, K8s-like (apiVersion, kind, metadata, spec)
- Classifiers reusable między regułami
- Wersjonowanie polityk (`version`) - manager pushuje delty po hashu
- Inbound vs outbound jako separate sections
- Per-rule `forward_to_siem` (domyślnie false) - granularna kontrola syslog forward
- Multiple match types: ścieżka, regex nazwy pliku, regex zawartości, klasyfikator

## Wazuh integration (syslog forwarder)

**Domyślnie WYŁĄCZONY**. Konfiguracja przez plik managera (`manager.yaml`):

```yaml
syslog_forwarder:
  enabled: false
  destination:
    host: wazuh.internal
    port: 6514
    protocol: tcp_tls    # tcp_tls (RFC 5425) | tcp | udp
    tls:
      verify: true
      ca_cert_path: /etc/scrooge/wazuh-ca.pem
      client_cert_path: /etc/scrooge/scrooge-client.pem
      client_key_path: /etc/scrooge/scrooge-client.key
  filters:
    min_severity: medium               # low | medium | high | critical
    respect_per_rule_flag: true        # honoruj `forward_to_siem` z polityki
    include_event_types: []
    exclude_event_types: []
```

**Format wysyłanych logów**: RFC 5424 z structured data:

```
<134>1 2026-05-09T14:30:22.123Z scrooge-manager scrooge-dlp 12345 EVENT [event@32473 agent_id="abc-123" hostname="WS-FIN-01" event_type="file_blocked" severity="high" policy="finance-strict" rule="finance-no-usb-export" direction="outbound" path="C:\\Finance\\Q4_report.xlsx" destination="USB:AA1234567890" user="jan.kowalski" matched_classifier="credit-card-numbers"] DLP blocked file copy to USB
```

**Wazuh decoder** (`deploy/wazuh-integration/decoders/0490-scroogedlp_decoders.xml`):

```xml
<decoder name="scroogedlp">
  <prematch>scrooge-dlp</prematch>
</decoder>

<decoder name="scroogedlp-event">
  <parent>scroogedlp</parent>
  <regex>agent_id="(\S+)" hostname="(\S+)" event_type="(\S+)" severity="(\S+)"</regex>
  <order>scrooge.agent_id, scrooge.hostname, scrooge.event_type, scrooge.severity</order>
</decoder>

<decoder name="scroogedlp-policy">
  <parent>scroogedlp</parent>
  <regex>policy="(\S+)" rule="(\S+)" direction="(\S+)"</regex>
  <order>scrooge.policy, scrooge.rule, scrooge.direction</order>
</decoder>
```

**Wazuh rules** (`deploy/wazuh-integration/rules/0490-scroogedlp_rules.xml`):

```xml
<group name="dlp,scroogedlp,">
  <rule id="100500" level="0">
    <decoded_as>scroogedlp</decoded_as>
    <description>ScroogeDLP event</description>
  </rule>
  
  <rule id="100501" level="10">
    <if_sid>100500</if_sid>
    <field name="scrooge.event_type">file_blocked</field>
    <field name="scrooge.severity">high</field>
    <description>ScroogeDLP: blocked sensitive file transfer to $(scrooge.destination)</description>
    <group>data_loss_prevention,pci_dss_10.6.1,</group>
  </rule>
  
  <rule id="100502" level="12">
    <if_sid>100500</if_sid>
    <field name="scrooge.severity">critical</field>
    <description>ScroogeDLP CRITICAL: $(scrooge.event_type) on $(scrooge.hostname)</description>
    <group>data_loss_prevention,</group>
  </rule>
</group>
```

## Struktura projektu (Cargo workspace)

```
scrooge-dlp/
├── Cargo.toml                    # workspace root
├── Cargo.lock
├── rust-toolchain.toml
├── rustfmt.toml
├── .gitignore
├── LICENSE                       # GPLv2 (pełny tekst)
├── README.md
├── PROJECT_BRIEF.md              # ten plik
├── Makefile
├── .github/
│   └── workflows/
│       └── ci.yml                # GitHub Actions
├── crates/
│   ├── scrooge-proto/            # protobuf schema, generowane typy
│   ├── scrooge-common/           # crypto, config, error - shared agent+manager
│   ├── scrooge-classifier/       # silnik reguł (regex, walidatory) - shared
│   ├── scrooge-policy/           # parsing/walidacja/kompilacja YAML - shared
│   ├── scrooge-syslog/           # RFC 5424/5425 formatter - tylko manager
│   ├── scrooge-agent-core/       # core agenta + trait PlatformAgent
│   ├── scrooge-agent-windows/    # platform impl Windows (STUB w MVP)
│   ├── scrooge-agent-linux/      # platform impl Linux (MVP: pełne)
│   ├── scrooge-agent-macos/      # platform impl macOS (MVP: pełne)
│   ├── scrooge-agent-bin/        # binarka `scrooge-agent`
│   ├── scrooge-manager-core/     # logika managera
│   ├── scrooge-manager-api/      # REST API + WebSocket (axum)
│   ├── scrooge-manager-bin/      # binarka `scrooge-manager`
│   └── scrooge-cli/              # CLI `scroogectl`
├── deploy/
│   ├── docker/
│   │   ├── Dockerfile.manager
│   │   └── docker-compose.yml
│   ├── quickstart.sh
│   ├── dev/
│   │   ├── setup-ubuntu-vm.sh
│   │   ├── rebuild-and-restart-agent.sh
│   │   ├── run-manager-local.sh
│   │   └── README.md             # UTM setup guide
│   ├── wazuh-integration/
│   │   ├── decoders/
│   │   │   └── 0490-scroogedlp_decoders.xml
│   │   ├── rules/
│   │   │   └── 0490-scroogedlp_rules.xml
│   │   └── README.md
│   └── agent-installers/         # faza późniejsza
├── policies/                     # przykładowe polityki
│   ├── default.yaml
│   ├── finance-strict.yaml
│   └── developer-friendly.yaml
├── proto/
│   └── scrooge.proto
├── migrations/                   # PostgreSQL migrations (sqlx)
│   ├── 20260101000001_initial.sql
│   ├── 20260101000002_agents.sql
│   └── ...
└── docs/
    ├── ARCHITECTURE.md
    ├── PROTOCOL.md
    ├── POLICY_FORMAT.md
    ├── REST_API.md
    └── WAZUH_INTEGRATION.md
```

## Schemat bazy danych (PostgreSQL)

```sql
-- Agenci
CREATE TABLE agents (
    id UUID PRIMARY KEY,
    hostname TEXT NOT NULL,
    os TEXT NOT NULL,                  -- 'windows', 'linux', 'macos'
    os_version TEXT,
    arch TEXT,
    agent_version TEXT,
    enrolled_at TIMESTAMPTZ NOT NULL,
    last_seen TIMESTAMPTZ,
    status TEXT NOT NULL DEFAULT 'active',
    cert_fingerprint TEXT NOT NULL UNIQUE
);

CREATE TABLE agent_tags (
    agent_id UUID REFERENCES agents(id) ON DELETE CASCADE,
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    PRIMARY KEY (agent_id, key)
);

CREATE TABLE groups (
    name TEXT PRIMARY KEY,
    description TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE agent_groups (
    agent_id UUID REFERENCES agents(id) ON DELETE CASCADE,
    group_name TEXT REFERENCES groups(name) ON DELETE CASCADE,
    PRIMARY KEY (agent_id, group_name)
);

-- Polityki (DB jako źródło prawdy)
CREATE TABLE policies (
    name TEXT PRIMARY KEY,
    version INT NOT NULL,
    content_yaml TEXT NOT NULL,
    content_compiled JSONB NOT NULL,
    content_hash TEXT NOT NULL,
    targets JSONB NOT NULL,
    priority INT NOT NULL DEFAULT 100,
    enabled BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by UUID,
    updated_by UUID
);

CREATE TABLE policy_history (
    id BIGSERIAL PRIMARY KEY,
    policy_name TEXT NOT NULL,
    version INT NOT NULL,
    content_yaml TEXT NOT NULL,
    changed_by UUID,
    changed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    change_reason TEXT
);

CREATE TABLE policy_assignments (
    agent_id UUID REFERENCES agents(id) ON DELETE CASCADE,
    policy_name TEXT REFERENCES policies(name) ON DELETE CASCADE,
    assigned_version INT NOT NULL,
    acked_version INT,
    last_pushed_at TIMESTAMPTZ,
    PRIMARY KEY (agent_id, policy_name)
);

-- Eventy
CREATE TABLE events (
    id UUID PRIMARY KEY,
    timestamp TIMESTAMPTZ NOT NULL,
    agent_id UUID NOT NULL,
    event_type TEXT NOT NULL,
    severity TEXT NOT NULL,
    direction TEXT,
    matched_policy TEXT,
    matched_rule TEXT,
    action_taken TEXT,
    details JSONB NOT NULL,
    forwarded_to_syslog BOOLEAN NOT NULL DEFAULT false
) PARTITION BY RANGE (timestamp);

-- Auth
CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    username TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    role TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_login TIMESTAMPTZ,
    enabled BOOLEAN NOT NULL DEFAULT true
);

CREATE TABLE refresh_tokens (
    token_hash TEXT PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    revoked BOOLEAN NOT NULL DEFAULT false
);

CREATE TABLE enrollment_tokens (
    token_hash TEXT PRIMARY KEY,
    description TEXT,
    expires_at TIMESTAMPTZ,
    max_uses INT,
    uses_count INT NOT NULL DEFAULT 0,
    created_by UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

## REST API (manager) - kluczowe endpointy

```
# Auth
POST   /api/v1/auth/login
POST   /api/v1/auth/refresh
POST   /api/v1/auth/logout

# Agents
GET    /api/v1/agents
GET    /api/v1/agents/{id}
PATCH  /api/v1/agents/{id}
DELETE /api/v1/agents/{id}
POST   /api/v1/agents/{id}/command

# Groups
GET    /api/v1/groups
POST   /api/v1/groups
GET    /api/v1/groups/{name}
PATCH  /api/v1/groups/{name}
DELETE /api/v1/groups/{name}

# Policies
GET    /api/v1/policies
POST   /api/v1/policies
GET    /api/v1/policies/{name}
PUT    /api/v1/policies/{name}
DELETE /api/v1/policies/{name}
GET    /api/v1/policies/{name}/history
POST   /api/v1/policies/{name}/rollback/{version}
POST   /api/v1/policies/validate
GET    /api/v1/policies/{name}/affected-agents

# Events
GET    /api/v1/events
GET    /api/v1/events/{id}
GET    /api/v1/events/stats

# Real-time
WS     /api/v1/ws/events
WS     /api/v1/ws/agent-status

# Enrollment
POST   /api/v1/enrollment/tokens
GET    /api/v1/enrollment/tokens
DELETE /api/v1/enrollment/tokens/{id}

# Users (admin only)
GET    /api/v1/users
POST   /api/v1/users
PATCH  /api/v1/users/{id}
DELETE /api/v1/users/{id}

# Manager config
GET    /api/v1/config/syslog
PUT    /api/v1/config/syslog
```

OpenAPI 3.0 spec generowany przez `utoipa`, eksponowany na `/api/v1/openapi.json`, Swagger UI na `/api/v1/docs`.

## Schemat protobuf (`proto/scrooge.proto`) - kluczowe wiadomości

```protobuf
syntax = "proto3";
package scrooge.v1;

service AgentService {
  rpc Enroll(EnrollRequest) returns (EnrollResponse);
  rpc Connect(stream AgentMessage) returns (stream ManagerMessage);
}

message EnrollRequest {
  string enrollment_token = 1;
  AgentInfo info = 2;
  bytes csr_pem = 3;
}

message EnrollResponse {
  string agent_id = 1;
  bytes cert_pem = 2;
  bytes ca_chain_pem = 3;
}

message AgentInfo {
  string hostname = 1;
  string os = 2;
  string os_version = 3;
  string arch = 4;
  string agent_version = 5;
  map<string, string> initial_tags = 6;
}

message AgentMessage {
  oneof payload {
    Heartbeat heartbeat = 1;
    Event event = 2;
    EventBatch events = 3;
    PolicyAck policy_ack = 4;
    CommandResult command_result = 5;
  }
}

message ManagerMessage {
  oneof payload {
    PolicyUpdate policy = 1;
    Command command = 2;
    Heartbeat heartbeat_ack = 3;
  }
}

message Heartbeat {
  int64 timestamp_ns = 1;
  AgentStatus status = 2;
  repeated string active_policy_ids = 3;
  map<string, uint32> active_policy_versions = 4;
}

message AgentStatus {
  uint64 events_in_queue = 1;
  uint64 events_dropped = 2;
  bool offline_mode = 3;
}

message Event {
  string event_id = 1;
  int64 timestamp_ns = 2;
  string agent_id = 3;
  EventType type = 4;
  Severity severity = 5;
  Direction direction = 6;
  
  oneof details {
    FileEvent file = 10;
    DeviceEvent device = 11;
    ClipboardEvent clipboard = 12;
    NetworkEvent network = 13;
    ScreenEvent screen = 14;
    ProcessEvent process = 15;
  }
  
  string matched_policy_id = 30;
  string matched_rule_id = 31;
  string action_taken = 32;
  repeated Match matches = 33;
  bool forward_to_siem = 34;
}

enum Direction {
  DIRECTION_UNKNOWN = 0;
  INBOUND = 1;
  OUTBOUND = 2;
  INTERNAL = 3;
}

enum Severity {
  SEVERITY_UNKNOWN = 0;
  LOW = 1;
  MEDIUM = 2;
  HIGH = 3;
  CRITICAL = 4;
}

message PolicyUpdate {
  repeated PolicyRef policies = 1;
  bool full_replace = 2;
}

message PolicyRef {
  string policy_id = 1;
  uint32 version = 2;
  bytes content_compiled = 3;
  string content_hash = 4;
}

message Command {
  string command_id = 1;
  CommandType type = 2;
  bytes payload = 3;
}

enum CommandType {
  COMMAND_UNKNOWN = 0;
  REFRESH_POLICIES = 1;
  COLLECT_DIAGNOSTICS = 2;
  UPDATE_TAGS = 3;
  RESTART_AGENT = 4;
}
```

(Rozszerzenia: pełne definicje `FileEvent`, `DeviceEvent`, `ClipboardEvent`, `NetworkEvent`, `ScreenEvent`, `ProcessEvent`, `Match`, `EventBatch`, `PolicyAck`, `CommandResult`, `EventType` - generowane w trakcie pracy w Kroku 2.)

## Roadmapa milestonów

### Milestone 1: Fundament komunikacji (BIEŻĄCY)
- Cargo workspace + wszystkie crates (puste szkielety)
- protobuf schema + tonic-build setup
- `scrooge-common`: CA, config, error types
- `scrooge-manager-bin`: gRPC server (Enroll + Connect stuby), bootstrap z PostgreSQL
- `scrooge-agent-bin`: enrollment flow + persistent connection (auto-reconnect)
- `scrooge-manager-api`: minimalne REST API (health, login, list agents)
- Auth JWT (login/password z bcrypt)
- docker-compose.yml + quickstart.sh
- Migracje DB (sqlx migrate)
- Dev scripts dla UTM workflow
- GitHub Actions CI
- **NIE implementujemy funkcji DLP w tym milestone**

### Milestone 2: Polityki i targeting
- Pełen schemat polityk YAML w `scrooge-policy`
- Parser + walidacja YAML
- Kompilator polityk do binarnego formatu (msgpack)
- API endpointy do CRUD polityk
- API endpointy do CRUD groups + tags
- Mechanizm push polityk do agentów (delta po hashu)
- Agent: odbiór polityk, walidacja, ack do managera

### Milestone 3: Pierwsze moduły monitorujące
- `mod_clipscreen` (cross-platform: arboard crate)
- `mod_devctl` (USB detection: udev na Linux, IOKit na macOS)
- Lokalna kolejka eventów w SQLite
- Batch upload do managera

### Milestone 4: File monitor + classifier
- `mod_filemon` user-mode (notify crate)
- `mod_classifier` z regex + walidatorami (Luhn, PESEL, NIP, IBAN)
- Skanowanie zawartości plików przy detekcji eventu

### Milestone 5: Wazuh syslog forwarder
- `scrooge-syslog`: formatter RFC 5424
- TLS connection (RFC 5425) z TCP fallback
- Filtrowanie eventów (severity threshold, per-rule flag)
- Wazuh decoder XML + rules XML w `deploy/wazuh-integration/`
- Dokumentacja instalacji w Wazuh

### Milestone 6: Network monitoring
- `mod_netinsp` na Linux (eBPF lub netfilter)
- `mod_netinsp` na macOS (NetworkExtension - wymaga entitlements)
- Detekcja uploadów do chmur po SNI

### Milestone 7: Egzekucja akcji (BLOCK)
- `mod_response` z pełnym setem akcji
- Blokowanie file operations (user-mode na MVP)
- Quarantine z szyfrowaniem AES-256-GCM

### Milestone 8 i dalej (poza tym briefem)
- Windows: pełna implementacja `scrooge-agent-windows`
- Minifilter driver Windows (C/WDK)
- TLS interception
- Self-protection / anti-tamper
- Multi-tenancy
- Hybryda DB + GitOps dla polityk
- Dashboard (osobne repo)

## Wymagania niefunkcjonalne

- **Bezpieczeństwo komunikacji**: mTLS obowiązkowo, cert pinning, rotacja certów
- **Self-protection agenta**: watchdog process pilnujący main service (faza późniejsza)
- **Resource footprint**: agent < 100MB RAM idle, < 2% CPU średnio
- **Offline operation**: agent działa lokalnie gdy manager niedostępny, kolejkuje eventy w SQLite
- **Cross-platform**: jeden codebase, cfg-gated platform code
- **Observability**: structured logging (JSON output), metryki Prometheus na managerze (`/metrics`)
- **Configuration as code**: konfig managera w YAML, hot-reload polityk
- **Database migrations**: sqlx migrate, idempotentne, każda zmiana w osobnym pliku

## Standardy kodu

- **Edition 2021**, MSRV 1.75+
- **Zero `unwrap()`** w kodzie produkcyjnym (tylko w testach lub z komentarzem `// SAFETY: ...`)
- **Error handling**: `anyhow::Result` w binarkach, `thiserror` w bibliotekach
- **Async wszystko**: nigdy `std::fs` w hot path, zawsze `tokio::fs`
- **Logging**: `tracing` zamiast `println!` / `log` - structured fields
- **Dokumentacja**: doc-comments na wszystkich publicznych itemach
- **Testy**: jednostkowe (`#[cfg(test)]`) + integracyjne (`tests/` per crate)
- **Lints**: `#![warn(clippy::all, clippy::pedantic)]`, `#![deny(unsafe_code)]` poza miejscami gdzie unsafe jest uzasadniony (z komentarzem)
- **Format**: `cargo fmt` z domyślnym `rustfmt.toml`, w CI obowiązkowy
- **GPLv2 header**: każdy plik źródłowy ma SPDX identifier i header licencji GPLv2

## Plan pracy dla Claude Code - Milestone 1

Pracujemy iteracyjnie. **Po każdym kroku zatrzymaj się i pokaż co zostało zrobione - czekaj na akceptację przed kolejnym krokiem.**

### Krok 1: Inicjalizacja workspace
- `Cargo.toml` workspace z wszystkimi crates + workspace dependencies
- `rust-toolchain.toml` (stable, components: rustfmt, clippy)
- `rustfmt.toml`
- `.gitignore` (target/, *.db, .env, certs/, *.pem, *.key, data/, logs/)
- `LICENSE` (pełny tekst GPLv2)
- `README.md` (placeholder z opisem ScroogeDLP + link do PROJECT_BRIEF.md)
- `Makefile` z targetami dev

### Krok 2: Schemat protobuf
- `proto/scrooge.proto` z pełnym schematem (rozszerz to co jest w briefie do kompletnej formy)
- `crates/scrooge-proto/Cargo.toml` + `build.rs` z `tonic-build`
- `crates/scrooge-proto/src/lib.rs` reeksportujące generowane typy
- Test kompilacji (`cargo build -p scrooge-proto`)

### Krok 3: `scrooge-common`
- Moduł `error`: wspólne `thiserror` enums
- Moduł `config`: structs + loading YAML (manager_config + agent_config)
- Moduł `ca`: wewnętrzne CA, generowanie root CA, podpisywanie CSR (rcgen)
- Moduł `crypto`: pomocnicze funkcje (bcrypt wrapper, AES-GCM placeholder)
- Testy jednostkowe dla CA i config loading

### Krok 4: Migracje DB
- `migrations/` z plikami sqlx-style
- Wszystkie tabele z briefu
- Indeksy na często używanych kolumnach (agent_id, timestamp, event_type)

### Krok 5: `scrooge-manager-bin` szkielet
- `main.rs`: tokio runtime, parsowanie config, połączenie do PG, sqlx migrate
- `grpc.rs`: tonic server na 5443 (stuby `Enroll` i `Connect`)
- TLS configuration (rustls) z certyfikatami z CA module
- Tracing setup (JSON output)
- Graceful shutdown (SIGINT/SIGTERM)

### Krok 6: `scrooge-manager-api` szkielet
- axum router na 55000
- Endpoint `GET /health`
- Endpoint `POST /api/v1/auth/login` (bcrypt verify, JWT generation)
- Middleware do walidacji JWT
- Endpoint `GET /api/v1/agents` (lista z DB)
- utoipa setup, OpenAPI na `/api/v1/openapi.json`, Swagger UI na `/api/v1/docs`

### Krok 7: `scrooge-agent-core` + trait PlatformAgent
- Trait `PlatformAgent` z metodami dla wszystkich modułów (na razie puste/stuby)
- Każdy crate platformowy (`scrooge-agent-linux`, `scrooge-agent-macos`, `scrooge-agent-windows`) implementuje trait
- Linux i macOS: stuby zwracające `Ok(())` lub puste eventy
- Windows: `unimplemented!()` w każdej metodzie (cfg-gated, nie buduje się w CI w MVP)

### Krok 8: `scrooge-agent-bin` szkielet
- `main.rs`: tokio runtime, parsowanie config, lokalny SQLite init
- Detekcja first-run vs already-enrolled (po obecności pliku `agent.cert`)
- First-run: enrollment flow (token z config, generuje keypair + CSR, wywołuje gRPC `Enroll`, zapisuje cert)
- After-run: persistent connection przez `Connect` (bidirectional streaming)
- Auto-reconnect z exponential backoff
- Heartbeat co 30s
- Tracing setup
- Graceful shutdown

### Krok 9: Docker + quickstart
- `deploy/docker/Dockerfile.manager` (multi-stage build z `cargo chef` dla cache)
- `deploy/docker/docker-compose.yml` (manager + postgres + volumes + healthchecks)
- `deploy/quickstart.sh`:
  - Sprawdza Docker
  - Pobiera compose
  - Generuje root CA
  - Tworzy admin usera (interaktywnie)
  - Generuje pierwszy enrollment token
  - Uruchamia compose
- `deploy/README.md`

### Krok 10: Dev environment dla UTM workflow
- `deploy/dev/setup-ubuntu-vm.sh`
- `deploy/dev/rebuild-and-restart-agent.sh`
- `deploy/dev/run-manager-local.sh`
- `deploy/dev/README.md` - kompletna instrukcja UTM setup, sieć, SSH, first-time, daily workflow, troubleshooting

### Krok 11: GitHub Actions CI
- `.github/workflows/ci.yml` zgodnie z briefem (fmt, clippy, test, build, matrix Linux+macOS)
- README badge dla CI status

### Krok 12: Smoke test E2E
- Skrypt który automatyzuje:
  - Build manager + agent na Macu
  - Uruchom manager przez docker-compose
  - Wygeneruj enrollment token
  - (Opcjonalnie) ssh na VM, build agent, start z tokenem
  - Sprawdź czy agent się zarejestrował (przez REST API)
  - Sprawdź heartbeat w logach managera
- Dokumentacja w `docs/E2E_TESTING.md`

## Pytania projektowe które już są rozwiązane (NIE pytaj)

- Auth: **JWT lokalny** (bcrypt, refresh tokens, RBAC admin/analyst/viewer)
- Polityki source of truth: **DB (PostgreSQL)**, YAML jako import/export
- Syslog transport: **TCP+TLS (RFC 5425)**, **domyślnie wyłączony**
- Per-rule syslog forward: **TAK** (`forward_to_siem` flag w polityce)
- Multi-tenancy: **NIE** w MVP
- Licencja: **GPLv2**
- Nazwa projektu: **ScroogeDLP**
- Binarki: `scrooge-manager`, `scrooge-agent`, `scroogectl`
- gRPC port: **5443**
- REST API port: **55000**
- Syslog out port: **6514** (TCP+TLS)
- Środowisko dev: **macOS** (rozwój) + **Ubuntu 22.04 na UTM** (testy)
- Windows w MVP: **TYLKO stub** (kod się kompiluje, ale nie testowany)
- CI: **GitHub Actions**, matrix Linux + macOS, bez Windows w MVP
- Repo: **publiczne na GitHubie**
- Build strategia: **native na VM** (cross-compile to opcja przyszła)

## Co teraz

Zaczynamy od **Kroku 1**. Pokaż mi proponowaną zawartość plików:
- `Cargo.toml` (workspace) z wszystkimi crates i workspace dependencies
- `rust-toolchain.toml`
- `.gitignore`
- `rustfmt.toml`
- `Makefile`

**NIE twórz jeszcze plików** - najpierw pokaż propozycję, omówmy, dopiero potem implementacja. Jeśli masz wątpliwości projektowe które nie są rozwiązane w tym briefie - **zapytaj zanim zaczniesz pisać kod**.

Po akceptacji Kroku 1 przechodzimy do Kroku 2 (protobuf), potem dalej według planu.