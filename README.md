# ScroogeDLP

[![CI](https://github.com/SQTX/Scrooge-DLP/actions/workflows/ci.yml/badge.svg)](https://github.com/SQTX/Scrooge-DLP/actions/workflows/ci.yml)
[![License: GPL v2](https://img.shields.io/badge/License-GPL_v2-blue.svg)](LICENSE)

**ScroogeDLP** to system Data Loss Prevention (DLP) inspirowany architekturą Wazuh,
ale skupiony wyłącznie na DLP. Integruje się z Wazuh przez forwarding syslog
(RFC 5424/5425) dla korelacji SIEM.

> ⚠️ **Status:** Faza 1 (Wazuh-flow Distribution) ukończona — manager+agent+dashboard+mTLS+HTTPS działają E2E. Funkcje DLP (clipboard/USB/network monitoring, polityki YAML) dochodzą w Phase 2+. NIE production-ready.
> Pełny plan: [`PROJECT_BRIEF.md`](PROJECT_BRIEF.md), roadmapa: [`ROADMAP.md`](ROADMAP.md).

## Quick install

Jeden one-liner na czystej VM Debian/Ubuntu (manager):

```bash
curl -fsSL https://github.com/SQTX/Scrooge-DLP/raw/main/deploy/install.sh | bash
```

Skrypt sam ogarnie:
- Auto-instalacja Dockera (`get.docker.com`) gdy brak
- Klon repo + checkout `main`
- Interaktywne pytania: publiczny adres managera, admin username, admin hasło
- Build obrazu managera (multi-stage cargo-chef, ~10 min jednorazowo)
- Generacja Root CA + server cert (mTLS dla agentów + HTTPS dla dashboardu)
- `docker compose up` (Postgres + manager), healthcheck, bootstrap admin

Po zakończeniu dashboard pod `https://<ip>:55000` (self-signed cert → Advanced → Proceed jednorazowo w browserze).

**Dodanie agenta:** w dashboardzie `+ Install agent` → Linux/macOS/Windows → Generate → skopiuj one-liner → wklej na endpoint jako root/admin. Agent enrolluje się automatycznie z `mTLS` (cert kryptograficznie powiązany z managerem przez Root CA).

Env vars dla non-interactive setup (CI / Ansible):

```bash
MANAGER_PUBLIC_ADDR=192.168.1.10 ADMIN_USERNAME=admin ADMIN_PASSWORD=safehash123 \
  curl -fsSL https://github.com/SQTX/Scrooge-DLP/raw/main/deploy/install.sh | bash
```

## Komponenty

- **`scrooge-agent`** — agent endpointowy dla Linux, macOS, Windows (`.deb`/`.rpm`/tarball/`.zip` w GitHub Releases)
- **`scrooge-manager`** — serwer centralny (Docker + quickstart), gRPC mTLS na `:5443` + HTTPS REST/dashboard na `:55000`
- **`scroogectl`** — CLI klient managera (init-ca, migrate, bootstrap-admin, gen-token)
- **Web dashboard** — embedded w binarce managera (vanilla HTML + Tailwind CDN), pod `https://<manager>:55000`

## Architektura (high-level)

```
agents ── gRPC/mTLS (5443) ──► manager ◄── REST/WS (55000) ── dashboard
                                  │
                                  ├── PostgreSQL
                                  └── (opcjonalnie) syslog → Wazuh (6514)
```

Pełny diagram: [PROJECT_BRIEF.md § Architektura](PROJECT_BRIEF.md#architektura-wysokopoziomowa).

## Stack

Rust 2021 (MSRV 1.75), Tokio, tonic (gRPC), axum (REST), sqlx (PostgreSQL + SQLite),
rustls. Multi-crate Cargo workspace.

## Build

```bash
make build           # debug build całego workspace
make check-all       # fmt-check + clippy + testy (pre-push)
make ci-local        # symulacja pełnego CI lokalnie
```

Środowisko developerskie (UTM, Ubuntu VM, manager w Dockerze) będzie opisane
w [`deploy/dev/README.md`](deploy/dev/README.md) (powstaje w Kroku 10 planu).

## Licencja

[GNU General Public License v2.0](LICENSE) — pełny tekst w pliku `LICENSE`.

## Repozytorium

<https://github.com/SQTX/Scrooge-DLP>
