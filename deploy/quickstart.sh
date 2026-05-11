#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
#
# ScroogeDLP - one-shot production setup.
#
# Wykonuje:
#   1. Sprawdza Docker + docker compose
#   2. Generuje CA + manager server cert (przez `scroogectl init-ca` w build stage)
#   3. Generuje JWT secret + Postgres password (random 32B)
#   4. Pisze `.env` + `manager.yaml` z wartościami
#   5. `docker compose build` + `up -d` (manager + postgres)
#   6. Czeka aż manager będzie healthy
#   7. Tworzy admin user (interactive password prompt)
#   8. Generuje pierwszy enrollment token
#   9. Drukuje URL'e do Swagger UI + gRPC + token

set -euo pipefail

# Katalogi relatywne do tej skryptu.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
DOCKER_DIR="$SCRIPT_DIR/docker"
CERTS_DIR="$DOCKER_DIR/certs"

cd "$REPO_ROOT"

# ──────────────────────────────────────────────────────────────────────────
# Pomocnicze
# ──────────────────────────────────────────────────────────────────────────
say()   { printf "\033[36m▸ %s\033[0m\n" "$*"; }
ok()    { printf "\033[32m✔ %s\033[0m\n" "$*"; }
warn()  { printf "\033[33m⚠ %s\033[0m\n" "$*"; }
err()   { printf "\033[31m✗ %s\033[0m\n" "$*"; exit 1; }

random_hex() {
    # 32 bajty hex (64 znaki) — używane dla jwt_secret i postgres password.
    if command -v openssl >/dev/null 2>&1; then
        openssl rand -hex 32
    else
        head -c 32 /dev/urandom | xxd -p | tr -d '\n'
    fi
}

# ──────────────────────────────────────────────────────────────────────────
# 1. Wymagania
# ──────────────────────────────────────────────────────────────────────────
say "checking requirements"
command -v docker >/dev/null 2>&1 || err "docker not found in PATH"
docker compose version >/dev/null 2>&1 || err "'docker compose' subcommand not available"
ok "docker $(docker --version | awk '{print $3}' | tr -d ',') ready"

# ──────────────────────────────────────────────────────────────────────────
# 2. Build image (potrzebne żeby scroogectl init-ca działał)
# ──────────────────────────────────────────────────────────────────────────
say "building scrooge-manager image (multi-stage, cache via cargo-chef)…"
docker compose -f "$DOCKER_DIR/docker-compose.yml" build manager >/dev/null
ok "image built"

# ──────────────────────────────────────────────────────────────────────────
# 3. CA + server cert (chyba że już są)
# ──────────────────────────────────────────────────────────────────────────
mkdir -p "$CERTS_DIR"
if [ -f "$CERTS_DIR/ca.pem" ] && [ -f "$CERTS_DIR/server.pem" ]; then
    warn "CA + server cert już istnieją w $CERTS_DIR — używam ich (delete i odpal ponownie żeby zregenerować)"
else
    say "generating CA + manager server cert via scroogectl init-ca…"
    HOSTNAME_VALUE="${MANAGER_HOSTNAME:-$(hostname -f 2>/dev/null || hostname)}"
    docker run --rm \
        -v "$CERTS_DIR:/out" \
        scrooge-manager:local \
        scroogectl init-ca \
            --output /out \
            --server-cn "$HOSTNAME_VALUE" \
            --san "localhost,scrooge-manager,127.0.0.1,$HOSTNAME_VALUE"
    ok "certs w $CERTS_DIR"
fi

# ──────────────────────────────────────────────────────────────────────────
# 4. Env vars (random secrets w .env, podstawiamy w manager.yaml)
# ──────────────────────────────────────────────────────────────────────────
ENV_FILE="$DOCKER_DIR/.env"
if [ -f "$ENV_FILE" ]; then
    warn ".env już istnieje — czytam istniejące sekrety"
    # shellcheck disable=SC1090
    source "$ENV_FILE"
else
    say "generating random secrets (jwt_secret + postgres password)…"
    JWT_SECRET="$(random_hex)"
    POSTGRES_PASSWORD="$(random_hex)"
    POSTGRES_USER="scrooge"
    POSTGRES_DB="scrooge"
    cat > "$ENV_FILE" <<EOF
POSTGRES_USER=$POSTGRES_USER
POSTGRES_PASSWORD=$POSTGRES_PASSWORD
POSTGRES_DB=$POSTGRES_DB
JWT_SECRET=$JWT_SECRET
MANAGER_GRPC_PORT=5443
MANAGER_REST_PORT=55000
EOF
    chmod 600 "$ENV_FILE"
    ok "secrets zapisane w $ENV_FILE (chmod 600)"
fi

# Render manager.yaml z template z env substitution.
say "rendering manager.yaml z manager.yaml.example…"
TEMPLATE="$DOCKER_DIR/manager.yaml.example"
TARGET="$DOCKER_DIR/manager.yaml"
sed \
    -e "s|\${POSTGRES_USER}|${POSTGRES_USER}|g" \
    -e "s|\${POSTGRES_PASSWORD}|${POSTGRES_PASSWORD}|g" \
    -e "s|\${POSTGRES_DB}|${POSTGRES_DB}|g" \
    -e "s|\${JWT_SECRET}|${JWT_SECRET}|g" \
    "$TEMPLATE" > "$TARGET"
chmod 600 "$TARGET"
ok "manager.yaml gotowy"

# ──────────────────────────────────────────────────────────────────────────
# 5. Up
# ──────────────────────────────────────────────────────────────────────────
say "starting stack (docker compose up -d)…"
docker compose -f "$DOCKER_DIR/docker-compose.yml" --env-file "$ENV_FILE" up -d
ok "containers started"

# ──────────────────────────────────────────────────────────────────────────
# 6. Wait for health
# ──────────────────────────────────────────────────────────────────────────
say "waiting for manager to be healthy (max 60s)…"
for i in $(seq 1 60); do
    if docker inspect --format '{{.State.Health.Status}}' scrooge-manager 2>/dev/null | grep -q healthy; then
        ok "manager healthy after ${i}s"
        break
    fi
    sleep 1
    [ "$i" = "60" ] && err "manager nie wystartował w 60s — sprawdź 'docker compose -f $DOCKER_DIR/docker-compose.yml logs manager'"
done

# ──────────────────────────────────────────────────────────────────────────
# 7. Bootstrap admin (interactive)
# ──────────────────────────────────────────────────────────────────────────
say "bootstrap admin user"
read -r -p "  username [admin]: " ADMIN_USER
ADMIN_USER="${ADMIN_USER:-admin}"
while true; do
    read -rs -p "  password (min 8 chars): " ADMIN_PASS && echo
    if [ "${#ADMIN_PASS}" -lt 8 ]; then
        warn "za krótkie — minimum 8 znaków"; continue
    fi
    read -rs -p "  password (confirm):    " ADMIN_PASS2 && echo
    if [ "$ADMIN_PASS" = "$ADMIN_PASS2" ]; then
        break
    fi
    warn "hasła nie pasują — spróbuj jeszcze raz"
done

docker compose -f "$DOCKER_DIR/docker-compose.yml" exec -T \
    -e ADMIN_PASSWORD="$ADMIN_PASS" \
    manager scroogectl bootstrap-admin --username "$ADMIN_USER"

# ──────────────────────────────────────────────────────────────────────────
# 8. Pierwszy enrollment token
# ──────────────────────────────────────────────────────────────────────────
say "generating first enrollment token (30 days)…"
TOKEN=$(docker compose -f "$DOCKER_DIR/docker-compose.yml" exec -T manager \
    scroogectl gen-token --description "quickstart bootstrap" 2>/dev/null | head -1)

# ──────────────────────────────────────────────────────────────────────────
# 9. Podsumowanie
# ──────────────────────────────────────────────────────────────────────────
echo ""
ok "ScroogeDLP gotowy do użycia!"
echo ""
echo "  Swagger UI:    http://localhost:${MANAGER_REST_PORT:-55000}/api/v1/docs"
echo "  REST API:      http://localhost:${MANAGER_REST_PORT:-55000}/api/v1"
echo "  gRPC (agenci): localhost:${MANAGER_GRPC_PORT:-5443}"
echo "  Admin:         $ADMIN_USER"
echo ""
echo "  Pierwszy enrollment token (do agent.yaml):"
echo "    $TOKEN"
echo ""
echo "  CA cert dla agentów: $CERTS_DIR/ca.pem"
echo "  (kopiuj na każdą maszynę z agentem)"
echo ""
echo "  Logi managera:  docker compose -f $DOCKER_DIR/docker-compose.yml logs -f manager"
echo "  Stop:           docker compose -f $DOCKER_DIR/docker-compose.yml down"
