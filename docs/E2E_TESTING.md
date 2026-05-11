# E2E Testing

Jak weryfikować że pełny stack ScroogeDLP (manager + agent + DB + REST API + gRPC)
działa po zmianach.

---

## Automatyczny smoke test

```bash
./scripts/smoke-e2e.sh
# lub:
make e2e-test
```

Czas: **~1–2 min** (z buildem), **~30 s** z `--skip-build`.

### Co skrypt robi (8 stages)

| Stage | Co weryfikuje | Fail-condition |
|---|---|---|
| 1. Requirements | `cargo`, `jq`, `curl`, `docker` w PATH; docker daemon żyje | Brak narzędzia / docker offline |
| 2. Reset | Zabija aktywne scrooge procesy, kasuje `agent-data/` | (zawsze przechodzi) |
| 3. Build | `cargo build --workspace` | Compilation error |
| 4. Bootstrap | `make dev-up && make dev-bootstrap` (Postgres + certs + admin + token) | Postgres nie wstał / migracje fail / admin/token błąd |
| 5. Manager start | Spawn `scrooge-manager` w bg → wait `GET /health` 200 ≤15s | Manager nie wstał w 15s |
| 6. Agent start | Spawn `scrooge-agent` w bg → wait `agent-data/agent.cert` istnieje ≤15s | Enrollment fail (token invalid, TLS error, etc.) |
| 7. REST assertions | `POST /auth/login` → JWT; `GET /agents` → exactly 1 agent z matching `agent_id` | Login fail / nie ma 1 agenta / agent_id mismatch |
| 8. Heartbeat | Wait 15s, `GET /agents` ponownie → `last_seen` musi się zwiększyć | Heartbeat nie updateuje `last_seen` |

Wyjście **0 = success**, **1 = fail** (z lokalizacją problemu w stderr).

### Flagi

| Flaga | Co robi |
|---|---|
| `--skip-build` | Pomiń stage 3 (użyj istniejących binarki w `target/debug/`) — szybsze przy iteracjach |
| `--keep-running` | Zostaw manager + agent biegające po pomyślnym teście (do manualnego curl'a / Swagger UI) |
| `--help` | Krótka pomoc |

### Output (success case)

```text
▸ checking requirements
✔ all required tools present
▸ resetting state (killing scrooge procs, wiping agent-data/)
✔ state clean
▸ building workspace (cargo build --workspace)
✔ build OK
▸ starting Postgres + dev certs + bootstrap (idempotent)
✔ bootstrap done (token: 4d7becef…)
▸ starting scrooge-manager in background
  manager pid=12345, log=/tmp/scrooge-manager.smoke.log
✔ manager listening on :55000 (after 1000ms)
▸ starting scrooge-agent in background
  agent pid=12346, log=/tmp/scrooge-agent.smoke.log
✔ agent enrolled (agent-data/ populated after 2500ms)
▸ fetching JWT via /auth/login
✔ JWT acquired (185 chars)
▸ fetching /api/v1/agents
✔ agent in registry, id matches local agent-data/agent.id
  hostname (API): MacBook-Air.local
  hostname (sys): MacBook-Air.local
✔ initial last_seen: 2026-05-11T18:16:32.232Z
▸ waiting 15s dla następnego heartbeat (agent.dev.yaml interval=10s)…
✔ heartbeat updates last_seen: 2026-05-11T18:16:32.232Z → 2026-05-11T18:16:47.412Z

════════════════════════════════════════════════════════════
  ✔ E2E SMOKE TEST PASSED
════════════════════════════════════════════════════════════
```

---

## Co skrypt NIE testuje (manual scenarios)

Te przypadki wymagają ręcznej weryfikacji (lub osobnych testów w przyszłości):

| Scenariusz | Jak przetestować ręcznie |
|---|---|
| **Cross-platform agent (Linux na UTM VM)** | [`deploy/dev/README.md`](../deploy/dev/README.md) §6–7 |
| **Production Docker deployment** | `./deploy/quickstart.sh` na świeżej VM |
| **Auto-reconnect po disconnect** | Manager → Ctrl-C → wait 30s → restart → agent powinien się reconnect w ≤60s |
| **Invalid token rejection** | `gen-token --max-uses 1`, użyj 2x → drugie enrollment powinno fail z `unauthenticated` |
| **TLS pinning fail** | Zmień `ca_cert_path` na nieprawidłowy PEM → agent powinien fail przy connect |
| **JWT expiry** | Czekaj 1h po loginie, użyj access_token → powinno zwrócić 401 |
| **Swagger UI** | `open http://127.0.0.1:55000/api/v1/docs` → klikaj `Authorize` + endpointy |
| **Refresh token rotation** | `POST /auth/refresh` → stary refresh token powinien być revoked w DB |
| **Multiple agents** | Uruchom drugiego agenta z innym `data_dir` → `GET /agents` powinno zwrócić 2 |

---

## Po nieudanym teście — co sprawdzić

### „manager nie wstał w 15s"

```bash
tail -50 /tmp/scrooge-manager.smoke.log
```

Najczęstsze przyczyny:
- **Port collision**: `lsof -i :55000` — jest tam inny proces
- **DB unreachable**: Postgres nie chodzi → `make dev-up`
- **Cert mismatch**: regenerate `make dev-certs` po zmianie SAN

### „agent nie zenrollował się w 15s"

```bash
tail -50 /tmp/scrooge-agent.smoke.log
```

Najczęstsze:
- **Invalid token**: token w `dev-certs/enrollment-token.txt` wygasł — `make dev-bootstrap`
- **TLS pinning fail**: agent ma stary `ca.pem`, manager nowy → wyczyść `dev-certs/` i `make dev-up`
- **Manager port blocked**: agent łączy do `localhost:5443`, sprawdź `lsof -i :5443`

### „last_seen nie zmieniło się"

Heartbeat task w agencie nie wysyła lub manager nie aktualizuje:
- `grep -i heartbeat /tmp/scrooge-agent.smoke.log` — czy agent loguje heartbeat?
- `grep -i heartbeat /tmp/scrooge-manager.smoke.log` — czy manager je odbiera?
- Sprawdź DB: `docker exec scrooge-dev-postgres psql -U scrooge -d scrooge -c "SELECT id, last_seen FROM agents;"`

---

## Integracja z CI

Smoke E2E **NIE jest aktualnie częścią `.github/workflows/ci.yml`** — wymaga:
- Postgres service (długi setup w GitHub Actions runner)
- TLS certs (generate-on-the-fly)
- ~1–2 min wykonania

Dla MVP wystarczy gate `fmt + clippy + test + build` na każdym PR. E2E uruchamiamy lokalnie przed merge'em na `main`.

### Future: dodać do CI jako oddzielny job

W `.github/workflows/ci.yml` można dodać:

```yaml
e2e:
  runs-on: ubuntu-latest
  needs: [test, build]                    # tylko jeśli unit testy pass
  services:
    postgres:
      image: postgres:16-alpine
      env:
        POSTGRES_PASSWORD: scrooge-dev-password
      ports: [5433:5432]
  steps:
    - uses: actions/checkout@v4
    - uses: dtolnay/rust-toolchain@stable
    - uses: arduino/setup-protoc@v3
    - run: sudo apt-get install -y jq openssl
    - run: ./scripts/smoke-e2e.sh
```

Wymaga adaptacji `manager.dev.yaml` żeby DB url'a używał env var.

---

## Lokalna iteracja (deweloperska)

Workflow przy zmianach w komunikacji manager↔agent:

```bash
# 1. Edytuj kod (np. proto / grpc.rs / enroll.rs)
$EDITOR crates/scrooge-manager-bin/src/grpc.rs

# 2. Smoke test
make e2e-test           # albo ./scripts/smoke-e2e.sh

# 3. Jeśli fail → fix → goto 2
# 4. Jeśli pass → commit
git add . && git commit -m "feat(grpc): ..."

# 5. Push → CI runs full gate (fmt + clippy + test + build na 2 OS)
git push
```
