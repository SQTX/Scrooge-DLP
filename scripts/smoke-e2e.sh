#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
#
# ScroogeDLP — automated E2E smoke test.
#
# Cel: zweryfikować że pełen pipeline enrollment + heartbeat działa
# po każdej zmianie w manager↔agent komunikacji.
#
# Skrypt jest AGRESYWNY:
#   - Zabija wszystkie aktywne scrooge procesy
#   - Wycina katalog `agent-data/` (force re-enrollment)
#   - Reset'uje dev environment do znanego stanu
#
# UŻYCIE:
#   ./scripts/smoke-e2e.sh                # standardowy run
#   ./scripts/smoke-e2e.sh --skip-build   # użyj istniejących binarki (szybciej)
#   ./scripts/smoke-e2e.sh --keep-running # zostaw manager+agent biegające po teście
#
# WYMAGANIA: docker (running), cargo, jq, curl, bash 4+, Postgres-friendly OS.

set -euo pipefail

# ────────────────────────────────────────────────────────────────────────
# Args
# ────────────────────────────────────────────────────────────────────────
KEEP_RUNNING=0
SKIP_BUILD=0
while [ $# -gt 0 ]; do
    case "$1" in
        --keep-running) KEEP_RUNNING=1 ;;
        --skip-build)   SKIP_BUILD=1 ;;
        -h|--help)
            sed -n '/^# UŻYCIE:/,/^# WYMAGANIA:/p' "$0" | sed 's/^# //;s/^#//'
            exit 0
            ;;
        *) echo "unknown arg: $1" >&2; exit 1 ;;
    esac
    shift
done

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

# ────────────────────────────────────────────────────────────────────────
# Helpers
# ────────────────────────────────────────────────────────────────────────
say()  { printf "\033[36m▸ %s\033[0m\n" "$*"; }
ok()   { printf "\033[32m✔ %s\033[0m\n" "$*"; }
warn() { printf "\033[33m⚠ %s\033[0m\n" "$*"; }
fail() {
    printf "\033[31m✗ %s\033[0m\n" "$*"
    cleanup
    exit 1
}

MGR_PID=""
AGENT_PID=""

cleanup() {
    if [ "$KEEP_RUNNING" = "1" ]; then
        return
    fi
    [ -n "$MGR_PID" ]   && kill -TERM "$MGR_PID"   2>/dev/null || true
    [ -n "$AGENT_PID" ] && kill -TERM "$AGENT_PID" 2>/dev/null || true
    sleep 1
}
trap cleanup EXIT INT TERM

# ────────────────────────────────────────────────────────────────────────
# Stage 1: wymagania
# ────────────────────────────────────────────────────────────────────────
say "checking requirements"
for cmd in cargo jq curl docker; do
    command -v "$cmd" >/dev/null 2>&1 || fail "missing required command: $cmd"
done
docker info >/dev/null 2>&1 || fail "docker daemon not running"
ok "all required tools present"

# ────────────────────────────────────────────────────────────────────────
# Stage 2: reset state
# ────────────────────────────────────────────────────────────────────────
say "resetting state (killing scrooge procs, wiping agent-data/)"
pkill -f "target/(debug|release)/scrooge-" 2>/dev/null || true
sleep 1
rm -rf agent-data
ok "state clean"

# ────────────────────────────────────────────────────────────────────────
# Stage 3: build
# ────────────────────────────────────────────────────────────────────────
if [ "$SKIP_BUILD" = "0" ]; then
    say "building workspace (cargo build --workspace)"
    cargo build --workspace --quiet 2>&1 | tail -5
    ok "build OK"
else
    say "skipping build (--skip-build) — verifying binaries exist"
    [ -x target/debug/scrooge-manager ] || fail "target/debug/scrooge-manager missing"
    [ -x target/debug/scrooge-agent ]   || fail "target/debug/scrooge-agent missing"
    [ -x target/debug/scroogectl ]      || fail "target/debug/scroogectl missing"
    ok "binaries present"
fi

# ────────────────────────────────────────────────────────────────────────
# Stage 4: Postgres + bootstrap (idempotent)
# ────────────────────────────────────────────────────────────────────────
say "starting Postgres + dev certs + bootstrap (idempotent)"
make --silent dev-up >/dev/null
make --silent dev-bootstrap 2>&1 | grep -E "(applied|created|enrollment token)" || true
TOKEN_RAW="$(tr -d '[:space:]' < dev-certs/enrollment-token.txt)"
[ -n "$TOKEN_RAW" ] || fail "no enrollment token in dev-certs/enrollment-token.txt"
ok "bootstrap done (token: ${TOKEN_RAW:0:8}…)"

# Wymuś czysty stan agents (CASCADE skasuje też agent_tags, agent_groups,
# policy_assignments). events zostają — historyczna retencja jest świadoma
# (brak FK do agents).
say "wiping agents table dla deterministycznego stanu"
docker exec scrooge-dev-postgres psql -U scrooge -d scrooge -qc \
    "TRUNCATE agents CASCADE;" >/dev/null
ok "agents table empty"

# ────────────────────────────────────────────────────────────────────────
# Stage 5: start manager
# ────────────────────────────────────────────────────────────────────────
say "starting scrooge-manager in background"
./target/debug/scrooge-manager \
    --config deploy/dev/manager.dev.yaml \
    --log-format pretty > /tmp/scrooge-manager.smoke.log 2>&1 &
MGR_PID=$!
echo "  manager pid=$MGR_PID, log=/tmp/scrooge-manager.smoke.log"

for i in $(seq 1 30); do
    if curl -s -f http://127.0.0.1:55000/health >/dev/null 2>&1; then
        ok "manager listening on :55000 (after $((i * 500))ms)"
        break
    fi
    sleep 0.5
    [ "$i" = "30" ] && fail "manager nie wstał w 15s; tail logów: $(tail -3 /tmp/scrooge-manager.smoke.log)"
done

# ────────────────────────────────────────────────────────────────────────
# Stage 6: start agent
# ────────────────────────────────────────────────────────────────────────
say "starting scrooge-agent in background"
ENROLLMENT_TOKEN="$TOKEN_RAW" \
./target/debug/scrooge-agent \
    --config deploy/dev/agent.dev.yaml \
    --log-format pretty > /tmp/scrooge-agent.smoke.log 2>&1 &
AGENT_PID=$!
echo "  agent pid=$AGENT_PID, log=/tmp/scrooge-agent.smoke.log"

# Czekamy aż enrollment się skończy (pojawi się agent.cert na dysku).
for i in $(seq 1 30); do
    if [ -f agent-data/agent.cert ] && [ -f agent-data/agent.id ]; then
        ok "agent enrolled (agent-data/ populated after $((i * 500))ms)"
        break
    fi
    sleep 0.5
    [ "$i" = "30" ] && fail "agent nie zenrollował się w 15s; tail: $(tail -3 /tmp/scrooge-agent.smoke.log)"
done

AGENT_ID_FROM_FILE="$(tr -d '[:space:]' < agent-data/agent.id)"

# ────────────────────────────────────────────────────────────────────────
# Stage 7: REST assertions
# ────────────────────────────────────────────────────────────────────────
say "fetching JWT via /auth/login"
LOGIN_RESP=$(curl -s -X POST http://127.0.0.1:55000/api/v1/auth/login \
    -H 'Content-Type: application/json' \
    -d '{"username":"admin","password":"admin123"}')
JWT=$(echo "$LOGIN_RESP" | jq -r .access_token)
[ -n "$JWT" ] && [ "$JWT" != "null" ] || fail "login failed: $LOGIN_RESP"
ok "JWT acquired (${#JWT} chars)"

say "fetching /api/v1/agents"
AGENTS=$(curl -s -H "Authorization: Bearer $JWT" http://127.0.0.1:55000/api/v1/agents)
COUNT=$(echo "$AGENTS" | jq 'length')
[ "$COUNT" = "1" ] || fail "expected 1 agent in registry, got $COUNT. Response: $AGENTS"

API_AGENT_ID=$(echo "$AGENTS" | jq -r '.[0].id')
[ "$API_AGENT_ID" = "$AGENT_ID_FROM_FILE" ] || \
    fail "agent_id mismatch: API=$API_AGENT_ID, file=$AGENT_ID_FROM_FILE"
ok "agent in registry, id matches local agent-data/agent.id"

API_HOST=$(echo "$AGENTS" | jq -r '.[0].hostname')
echo "  hostname (API): $API_HOST"
echo "  hostname (sys): $(hostname)"

FIRST_SEEN=$(echo "$AGENTS" | jq -r '.[0].last_seen')
ok "initial last_seen: $FIRST_SEEN"

# ────────────────────────────────────────────────────────────────────────
# Stage 8: heartbeat verification
# ────────────────────────────────────────────────────────────────────────
say "waiting 15s dla następnego heartbeat (agent.dev.yaml interval=10s)…"
sleep 15

AGENTS2=$(curl -s -H "Authorization: Bearer $JWT" http://127.0.0.1:55000/api/v1/agents)
SECOND_SEEN=$(echo "$AGENTS2" | jq -r '.[0].last_seen')

if [ "$SECOND_SEEN" = "$FIRST_SEEN" ]; then
    fail "last_seen nie zmieniło się po 15s (oba: $FIRST_SEEN). Heartbeat nie działa?"
fi
ok "heartbeat updates last_seen: $FIRST_SEEN → $SECOND_SEEN"

# ────────────────────────────────────────────────────────────────────────
# Final
# ────────────────────────────────────────────────────────────────────────
echo ""
echo "════════════════════════════════════════════════════════════"
printf "  \033[32m✔ E2E SMOKE TEST PASSED\033[0m\n"
echo "════════════════════════════════════════════════════════════"
echo ""
echo "  Manager log:    /tmp/scrooge-manager.smoke.log"
echo "  Agent log:      /tmp/scrooge-agent.smoke.log"
echo "  Agent state:    agent-data/agent.{cert,key,id}"
echo "  Postgres:       127.0.0.1:5433 (running, persisted)"
echo ""

if [ "$KEEP_RUNNING" = "1" ]; then
    echo "  --keep-running: manager (pid $MGR_PID) + agent (pid $AGENT_PID) lecą dalej."
    echo "  Zatrzymanie ręczne:  kill $MGR_PID $AGENT_PID"
    echo "  Swagger UI:          http://localhost:55000/api/v1/docs"
    MGR_PID=""
    AGENT_PID=""
fi
