#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
#
# ScroogeDLP - one-shot production setup.
#
# Wykonuje:
#   1. Sprawdza Docker + docker compose
#   2. Pyta o: publiczny adres managera + admin username + admin password
#      (wszystko upfront, żeby nie blokować się w połowie buildu)
#   3. Builduje obraz scrooge-manager (multi-stage z cargo-chef)
#   4. Generuje CA + manager server cert (via scroogectl init-ca)
#   5. Generuje JWT secret + Postgres password (random 32B w .env)
#   6. Pisze manager.yaml (host + JWT + public_* + agent_release_tag)
#   7. `docker compose up -d` (manager + postgres)
#   8. Czeka aż manager będzie healthy (curl /health)
#   9. Bootstrap admin user
#  10. Drukuje URL-e + dane logowania
#
# Non-interactive mode: ustaw zmienne MANAGER_PUBLIC_ADDR, ADMIN_USERNAME,
# ADMIN_PASSWORD przed wywołaniem skryptu — wtedy nic nie pyta.

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

# Quickstart bywa uruchamiany przez `curl … | bash` (z install.sh bootstrap)
# — wtedy stdin pipe'a NIE jest tty, ale /dev/tty wciąż jest dostępne.
# Funkcja `has_tty` pomaga zdecydować czy mozemy pytać interaktywnie.
has_tty() { [ -e /dev/tty ] && [ -r /dev/tty ] && [ -w /dev/tty ]; }
ask() {
    # Wrapper na `read -r -p` ktory czyta z /dev/tty zamiast stdin.
    # Uzycie: ask VAR_NAME "prompt: "
    local __var="$1" __prompt="$2"
    read -r -p "$__prompt" "$__var" </dev/tty
}
ask_silent() {
    # Wrapper na `read -rs -p` ktory czyta z /dev/tty (dla haseł).
    local __var="$1" __prompt="$2"
    read -rs -p "$__prompt" "$__var" </dev/tty
    echo
}

random_hex() {
    # 32 bajty hex (64 znaki) — używane dla jwt_secret i postgres password.
    if command -v openssl >/dev/null 2>&1; then
        openssl rand -hex 32
    else
        head -c 32 /dev/urandom | xxd -p | tr -d '\n'
    fi
}

# ──────────────────────────────────────────────────────────────────────────
# 1. Wymagania — auto-install Dockera jesli brak, auto-relog jesli user
#    nie ma jeszcze swiezej grupy 'docker' w shellu.
#
# Skip auto-install: SKIP_DOCKER_INSTALL=1 ./quickstart.sh
# ──────────────────────────────────────────────────────────────────────────
say "checking requirements"

install_docker() {
    say "instaluje Docker przez oficjalny installer (get.docker.com)…"
    curl -fsSL https://get.docker.com | sudo sh
    if ! id -nG "$USER" | grep -qw docker; then
        say "dodaje $USER do grupy 'docker'…"
        sudo usermod -aG docker "$USER"
    fi
    ok "Docker zainstalowany"
}

if ! command -v docker >/dev/null 2>&1; then
    if [ "${SKIP_DOCKER_INSTALL:-0}" = "1" ]; then
        err "docker not found in PATH (SKIP_DOCKER_INSTALL=1 — zainstaluj recznie)"
    fi
    warn "Docker nie znaleziony w PATH."
    if ! has_tty; then
        err "non-interactive (brak /dev/tty) — zainstaluj recznie: curl -fsSL https://get.docker.com | sudo sh"
    fi
    ask ans "  zainstalowac Docker przez get.docker.com? [Y/n]: "
    case "${ans,,}" in
        ""|y|yes|t|tak) install_docker ;;
        *) err "anulowane przez uzytkownika" ;;
    esac
fi

if ! docker compose version >/dev/null 2>&1; then
    err "'docker compose' (v2) subcommand nie dziala — Ubuntu apt 'docker-compose-plugin' nie wystarczy, odpal: curl -fsSL https://get.docker.com | sudo sh"
fi

if ! docker info >/dev/null 2>&1; then
    # Daemon dziala, ale user nie ma uprawnien. Czy user jest w grupie docker?
    if id -nG "$USER" | grep -qw docker; then
        # Tak — tylko bieżący shell jeszcze tego nie wie. Re-exec w grupie
        # docker przez `sg`, zeby user nie musial sie wylogowywać.
        warn "user '$USER' jest w grupie 'docker' ale shell tego nie zna — re-launch w nowej grupie…"
        SCRIPT_PATH="$(readlink -f "$0")"
        exec sg docker -c "$SCRIPT_PATH $*"
    else
        cat >&2 <<EOF
✗ Docker daemon dziala, ale '$USER' nie ma uprawnien.

  Dodaj sie do grupy 'docker' (raz w zyciu):
    sudo usermod -aG docker $USER
    newgrp docker

  Lub odpal quickstart przez sudo:
    sudo -E ./deploy/quickstart.sh
EOF
        exit 1
    fi
fi
ok "docker $(docker --version | awk '{print $3}' | tr -d ',') ready"

# ──────────────────────────────────────────────────────────────────────────
# 2. Interaktywne pytania (WSZYSTKIE UPFRONT)
#
# Pytamy o wszystko zanim ruszymy build, żeby user nie czekał 10 min na
# obraz, a potem dopiero dowiedział się że musi wprowadzić hasło. Każde
# pytanie ma env-var override dla CI / non-interactive mode.
# ──────────────────────────────────────────────────────────────────────────
echo ""
say "konfiguracja (wszystkie wartości można też podać przez env vars)"
echo ""

# 2a. Publiczny adres managera (hostname lub IP).
if [ -z "${MANAGER_PUBLIC_ADDR:-}" ]; then
    has_tty || err "non-interactive (brak /dev/tty) — ustaw MANAGER_PUBLIC_ADDR env var"
    DEFAULT_ADDR="$(hostname -f 2>/dev/null || hostname)"
    ask MANAGER_PUBLIC_ADDR "  publiczny adres managera (hostname lub IP) [$DEFAULT_ADDR]: "
    MANAGER_PUBLIC_ADDR="${MANAGER_PUBLIC_ADDR:-$DEFAULT_ADDR}"
fi

# Czy `MANAGER_PUBLIC_ADDR` to IP czy hostname — SAN ma osobno IP: i DNS:.
if [[ "$MANAGER_PUBLIC_ADDR" =~ ^([0-9]{1,3}\.){3}[0-9]{1,3}$ ]]; then
    SAN_ENTRY="$MANAGER_PUBLIC_ADDR"   # init-ca wykryje IP po format'cie
    PUBLIC_KIND="IP"
else
    SAN_ENTRY="$MANAGER_PUBLIC_ADDR"
    PUBLIC_KIND="DNS"
fi
# Sub-faza 1F: REST API zawsze HTTPS (manager wystawia TLS z self-signed
# cert podpisanym przez nasz Root CA). Override REST_SCHEMA=http tylko gdy
# wyłączysz `rest_tls_*` w manager.yaml dla dev mode.
REST_SCHEMA="${REST_SCHEMA:-https}"

# 2b. Admin username.
if [ -z "${ADMIN_USERNAME:-}" ]; then
    has_tty || err "non-interactive (brak /dev/tty) — ustaw ADMIN_USERNAME env var"
    ask ADMIN_USERNAME "  admin username [admin]: "
    ADMIN_USERNAME="${ADMIN_USERNAME:-admin}"
fi

# 2c. Admin password — min 8 znaków, confirm.
if [ -z "${ADMIN_PASSWORD:-}" ]; then
    has_tty || err "non-interactive (brak /dev/tty) — ustaw ADMIN_PASSWORD env var (min 8 znakow)"
    while true; do
        ask_silent ADMIN_PASSWORD "  admin password (min 8 chars): "
        if [ "${#ADMIN_PASSWORD}" -lt 8 ]; then
            warn "za krótkie — minimum 8 znaków"; continue
        fi
        ask_silent ADMIN_PASSWORD2 "  admin password (confirm):    "
        if [ "$ADMIN_PASSWORD" = "$ADMIN_PASSWORD2" ]; then
            break
        fi
        warn "hasła nie pasują — spróbuj jeszcze raz"
    done
else
    if [ "${#ADMIN_PASSWORD}" -lt 8 ]; then
        err "ADMIN_PASSWORD env var ma mniej niż 8 znaków"
    fi
fi

# Podsumowanie konfiguracji — user widzi co zatwierdza.
echo ""
ok "konfiguracja zebrana:"
echo "    publiczny adres:  $MANAGER_PUBLIC_ADDR ($PUBLIC_KIND, schema=$REST_SCHEMA)"
echo "    admin username:   $ADMIN_USERNAME"
echo "    admin password:   [hidden, ${#ADMIN_PASSWORD} znaków]"
echo ""

# ──────────────────────────────────────────────────────────────────────────
# 3. Build image
#
# UŻYWAMY `docker build` bezpośrednio (nie `docker compose build`) — compose
# parsuje całą definicję, w tym `${POSTGRES_PASSWORD:?…}` validation z
# `postgres` service, którego env jeszcze nie znamy na tym etapie.
# ──────────────────────────────────────────────────────────────────────────
say "building scrooge-manager image (multi-stage, cache via cargo-chef)…"
docker build \
    -f "$DOCKER_DIR/Dockerfile.manager" \
    -t scrooge-manager:local \
    "$REPO_ROOT" >/dev/null
ok "image built"

# ──────────────────────────────────────────────────────────────────────────
# 4. CA + server cert (chyba że już są)
# ──────────────────────────────────────────────────────────────────────────
mkdir -p "$CERTS_DIR"
if [ -f "$CERTS_DIR/ca.pem" ] && [ -f "$CERTS_DIR/server.pem" ]; then
    warn "CA + server cert już istnieją w $CERTS_DIR — używam ich (delete i odpal ponownie żeby zregenerować)"
else
    say "generating CA + manager server cert via scroogectl init-ca…"
    # `--entrypoint scroogectl` jest konieczne bo Dockerfile.manager ma
    # ENTRYPOINT na 'scrooge-manager' (dla production usage); bez tego
    # docker run interpretuje 'scroogectl init-ca' jako argumenty do
    # scrooge-manager binarki, ktora rzuca:
    #   error: unexpected argument 'scroogectl' found
    docker run --rm \
        --entrypoint scroogectl \
        -v "$CERTS_DIR:/out" \
        scrooge-manager:local \
        init-ca \
            --output /out \
            --server-cn "$MANAGER_PUBLIC_ADDR" \
            --san "localhost,scrooge-manager,127.0.0.1,$SAN_ENTRY"
    ok "certs w $CERTS_DIR (CN=$MANAGER_PUBLIC_ADDR, SAN += $PUBLIC_KIND:$SAN_ENTRY)"
fi

# ──────────────────────────────────────────────────────────────────────────
# 5. Env vars (random secrets w .env, podstawiamy w manager.yaml)
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
PUBLIC_GRPC="${MANAGER_PUBLIC_ADDR}:${MANAGER_GRPC_PORT:-5443}"
PUBLIC_REST="${REST_SCHEMA}://${MANAGER_PUBLIC_ADDR}:${MANAGER_REST_PORT:-55000}"
AGENT_RELEASE_TAG="${AGENT_RELEASE_TAG:-v0.1.0-rc7}"

sed \
    -e "s|\${POSTGRES_USER}|${POSTGRES_USER}|g" \
    -e "s|\${POSTGRES_PASSWORD}|${POSTGRES_PASSWORD}|g" \
    -e "s|\${POSTGRES_DB}|${POSTGRES_DB}|g" \
    -e "s|\${JWT_SECRET}|${JWT_SECRET}|g" \
    -e "s|public_grpc_endpoint: \".*\"|public_grpc_endpoint: \"${PUBLIC_GRPC}\"|" \
    -e "s|public_rest_base_url: \".*\"|public_rest_base_url: \"${PUBLIC_REST}\"|" \
    -e "s|agent_release_tag:    \".*\"|agent_release_tag:    \"${AGENT_RELEASE_TAG}\"|" \
    "$TEMPLATE" > "$TARGET"
chmod 600 "$TARGET"
ok "manager.yaml gotowy (public_grpc=$PUBLIC_GRPC, public_rest=$PUBLIC_REST, release=$AGENT_RELEASE_TAG)"

# ──────────────────────────────────────────────────────────────────────────
# 6. Up
# ──────────────────────────────────────────────────────────────────────────
say "starting stack (docker compose up -d)…"
docker compose -f "$DOCKER_DIR/docker-compose.yml" --env-file "$ENV_FILE" up -d
ok "containers started"

# ──────────────────────────────────────────────────────────────────────────
# 7. Wait for health
#
# Healthcheck uzywa curl wewnatrz kontenera (sprawdza GET /health). Jezeli
# curl odpowiada 200, manager jest faktycznie ready do uzycia, a nie tylko
# proces ze startuje. start_period=5s + interval 10s — pierwsze sprawdzenie
# po 5-15s.
# ──────────────────────────────────────────────────────────────────────────
say "waiting for manager to be healthy (max 120s)…"
for i in $(seq 1 120); do
    STATUS="$(docker inspect --format '{{.State.Health.Status}}' scrooge-manager 2>/dev/null || echo unknown)"
    case "$STATUS" in
        healthy)
            ok "manager healthy after ${i}s"
            break
            ;;
        unhealthy)
            err "manager unhealthy — sprawdź 'docker compose -f $DOCKER_DIR/docker-compose.yml logs manager'"
            ;;
    esac
    sleep 1
    [ "$i" = "120" ] && err "manager nie wystartował w 120s — sprawdź 'docker compose -f $DOCKER_DIR/docker-compose.yml logs manager'"
done

# ──────────────────────────────────────────────────────────────────────────
# 8. Bootstrap admin (już mamy ADMIN_USERNAME + ADMIN_PASSWORD z section 2)
# ──────────────────────────────────────────────────────────────────────────
say "bootstrap admin user '$ADMIN_USERNAME'"
docker compose -f "$DOCKER_DIR/docker-compose.yml" exec -T \
    -e ADMIN_PASSWORD="$ADMIN_PASSWORD" \
    manager scroogectl bootstrap-admin --username "$ADMIN_USERNAME"

# ──────────────────────────────────────────────────────────────────────────
# 9. Podsumowanie
# ──────────────────────────────────────────────────────────────────────────
echo ""
ok "ScroogeDLP gotowy do użycia!"
echo ""
echo "  Dashboard:     ${REST_SCHEMA}://${MANAGER_PUBLIC_ADDR}:${MANAGER_REST_PORT:-55000}"
echo "  Swagger UI:    ${REST_SCHEMA}://${MANAGER_PUBLIC_ADDR}:${MANAGER_REST_PORT:-55000}/api/v1/docs"
echo "  REST API:      ${REST_SCHEMA}://${MANAGER_PUBLIC_ADDR}:${MANAGER_REST_PORT:-55000}/api/v1"
echo "  gRPC (agenci): ${MANAGER_PUBLIC_ADDR}:${MANAGER_GRPC_PORT:-5443}"
echo ""
echo "  Login:    $ADMIN_USERNAME / [hasło które właśnie ustawiłeś]"
echo ""
echo "  Dodaj pierwszego agenta:"
echo "    1. Otwórz dashboard w przeglądarce"
echo "    2. Klik '+ Install agent' → wybierz OS → 'Generate one-liner'"
echo "    3. Skopiuj one-liner i wklej na endpoincie jako root"
echo ""
echo "  Logi managera:  docker compose -f $DOCKER_DIR/docker-compose.yml logs -f manager"
echo "  Stop:           docker compose -f $DOCKER_DIR/docker-compose.yml down"
