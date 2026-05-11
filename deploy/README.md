# ScroogeDLP — Deployment

Trzy ścieżki deploymentu:

1. **Production / staging** — `./deploy/quickstart.sh` (interaktywny, Docker)
2. **Lokalny dev** — `make dev-up` + `make dev-bootstrap` + `make dev-manager` / `make dev-agent`
3. **Test agenta cross-platform** — Ubuntu w UTM (Krok 10, w przygotowaniu)

---

## 1. Production / staging — `quickstart.sh`

Jednorazowy bootstrap na świeżej maszynie z Dockerem.

### Wymagania

- Docker Engine ≥ 24 + `docker compose` (subcommand, nie `docker-compose` binary)
- Linux x86_64 lub macOS Apple Silicon
- Otwarte porty: 5443 (gRPC dla agentów) + 55000 (REST API dla dashboardu/CLI)
- Min. 2 GB RAM, 10 GB dysk

### Uruchomienie

```bash
git clone https://github.com/SQTX/Scrooge-DLP.git
cd Scrooge-DLP
./deploy/quickstart.sh
```

Skrypt zrobi:

| Krok | Co |
|------|------|
| 1 | Sprawdza docker + `docker compose` |
| 2 | Buduje obraz `scrooge-manager:local` (multi-stage, ~3 min pierwszy raz, ~10s na kolejne builds dzięki `cargo-chef`) |
| 3 | Generuje root CA + manager TLS cert (`scroogectl init-ca`) — zapisuje w `deploy/docker/certs/` |
| 4 | Tworzy losowy JWT secret + Postgres password — zapisuje w `deploy/docker/.env` (chmod 600) |
| 5 | Renderuje `manager.yaml` z template |
| 6 | `docker compose up -d` → manager + Postgres |
| 7 | Czeka na healthcheck managera |
| 8 | Pyta o admin username + hasło (interaktywnie) i tworzy usera w DB |
| 9 | Generuje pierwszy enrollment token (30 dni) |
| 10 | Wypisuje URL'e + token + ścieżkę do `ca.pem` |

### Po uruchomieniu

```bash
# Swagger UI w przeglądarce:
open http://localhost:55000/api/v1/docs

# Login + lista agentów:
TOKEN=$(curl -s -X POST http://localhost:55000/api/v1/auth/login \
  -H 'Content-Type: application/json' \
  -d '{"username":"admin","password":"<your-password>"}' | jq -r .access_token)
curl -H "Authorization: Bearer $TOKEN" http://localhost:55000/api/v1/agents

# Logi managera:
docker compose -f deploy/docker/docker-compose.yml logs -f manager

# Zatrzymanie (volumes z danymi zostają):
docker compose -f deploy/docker/docker-compose.yml down

# Pełny reset (kasuje DB):
docker compose -f deploy/docker/docker-compose.yml down -v
rm -rf deploy/docker/certs deploy/docker/.env deploy/docker/manager.yaml
```

### Co generuje quickstart

```
deploy/docker/
├── certs/
│   ├── ca.pem         ← rozdaj do agentów (TLS pinning)
│   ├── ca.key         ← SEKRET, root CA private key
│   ├── server.pem     ← manager TLS cert (podpisany przez CA)
│   └── server.key     ← SEKRET, manager TLS private key
├── .env               ← SEKRET (chmod 600), Postgres password + JWT secret
└── manager.yaml       ← rendered z manager.yaml.example
```

### Konfiguracja portów / hostname

Przed uruchomieniem `quickstart.sh`:

```bash
export MANAGER_HOSTNAME=scrooge.example.com   # dodaje do SAN cert'a
./deploy/quickstart.sh

# Lub po fakcie - edytuj deploy/docker/.env:
echo "MANAGER_GRPC_PORT=15443" >> deploy/docker/.env
echo "MANAGER_REST_PORT=15500" >> deploy/docker/.env
docker compose -f deploy/docker/docker-compose.yml up -d
```

---

## 2. Lokalny dev (Mac native + Postgres w Dockerze)

Patrz [`MILESTONE_1_PROGRESS.md` § Jak uruchomić](../MILESTONE_1_PROGRESS.md#jak-uruchomić) — `make dev-up`, `make dev-bootstrap`, `make dev-manager`, `make dev-agent`.

Główna różnica: manager + agent są kompilowane natywnie przez `cargo run` (szybsza iteracja), tylko Postgres jest w kontenerze.

---

## 3. Agent na Ubuntu (UTM VM)

→ **Krok 10 — w przygotowaniu.** Doda `deploy/dev/setup-ubuntu-vm.sh` + dokumentację konfiguracji sieci UTM Shared Network.

---

## Troubleshooting

**„cannot connect to docker daemon"** — uruchom Docker Desktop (macOS) lub `sudo systemctl start docker` (Linux).

**„port 5432 already in use"** — masz lokalnego Postgresa na hoście. Dev używa portu 5433. Prod compose używa 5432 — albo wyłącz lokalnego PG, albo zmień port w `.env`.

**„manager nie wystartował w 60s"** — sprawdź `docker compose logs manager`. Najczęściej: brak `manager.yaml` (re-run quickstart) lub niepoprawny cert (regenerate przez `rm certs/* && ./deploy/quickstart.sh`).

**Agent nie może się enrollować** — sprawdź czy `ca.pem` skopiowane do agenta i `manager.endpoint` w `agent.yaml` matchuje SAN cert'a managera (default zawiera `localhost` + hostname maszyny).
