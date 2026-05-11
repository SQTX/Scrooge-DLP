# ScroogeDLP

**ScroogeDLP** to system Data Loss Prevention (DLP) inspirowany architekturą Wazuh,
ale skupiony wyłącznie na DLP. Integruje się z Wazuh przez forwarding syslog
(RFC 5424/5425) dla korelacji SIEM.

> ⚠️ **Status:** Wczesna faza rozwoju (MVP — Milestone 1). NIE production-ready.
> Pełny plan i roadmapa: [`PROJECT_BRIEF.md`](PROJECT_BRIEF.md).

## Komponenty

- **`scrooge-agent`** — agent endpointowy dla Linux i macOS (Windows: stub w MVP).
- **`scrooge-manager`** — serwer centralny (Docker + quickstart), gRPC + REST API.
- **`scroogectl`** — CLI klient managera.
- **`scrooge-dashboard`** — web UI (osobne repo, poza tym briefem).

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
