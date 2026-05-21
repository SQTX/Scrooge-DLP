#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
#
# ScroogeDLP manager — one-liner update.
#
# Użycie:
#   curl -fsSL https://github.com/SQTX/Scrooge-DLP/raw/main/deploy/update-manager.sh \
#     | sudo bash
#
# Robi:
#   1. cd ~/Scrooge-DLP
#   2. git pull (na current branchu)
#   3. docker compose build manager
#   4. docker compose up -d --force-recreate manager
#   5. waituje aż health-check pass'uje
#
# Env vars (opcjonalne):
#   INSTALL_DIR  — gdzie repo jest (default: $HOME/Scrooge-DLP, /opt/scrooge-src)
#   INSTALL_REF  — wymuś branch/tag git przed build'em
#   HEALTH_URL   — endpoint do post-update health check (default: skip)

set -euo pipefail

log()  { printf '\033[1;36m▸\033[0m %s\n' "$*"; }
ok()   { printf '\033[1;32m✔\033[0m %s\n' "$*"; }
fail() { printf '\033[1;31m✗\033[0m %s\n' "$*" >&2; exit 1; }

# ── Znajdź repo ───────────────────────────────────────────────────────────
INSTALL_DIR="${INSTALL_DIR:-}"
if [[ -z "$INSTALL_DIR" ]]; then
  for cand in "$HOME/Scrooge-DLP" "/opt/scrooge-src" "/opt/Scrooge-DLP"; do
    [[ -d "$cand/.git" ]] && INSTALL_DIR="$cand" && break
  done
fi
[[ -n "$INSTALL_DIR" && -d "$INSTALL_DIR/.git" ]] \
  || fail "nie znalazłem Scrooge-DLP repo. Ustaw INSTALL_DIR=<path>."

log "repo: $INSTALL_DIR"
cd "$INSTALL_DIR"

# ── Optional checkout ─────────────────────────────────────────────────────
if [[ -n "${INSTALL_REF:-}" ]]; then
  log "git fetch + checkout $INSTALL_REF"
  git fetch --quiet origin
  git checkout "$INSTALL_REF"
  git reset --hard "origin/$INSTALL_REF"
else
  log "git pull (current branch: $(git rev-parse --abbrev-ref HEAD))"
  git pull --ff-only
fi

# ── Locate docker-compose.yml ─────────────────────────────────────────────
COMPOSE_DIR=""
for cand in "$INSTALL_DIR/deploy/docker" "$INSTALL_DIR/deploy/dev" "$INSTALL_DIR"; do
  if [[ -f "$cand/docker-compose.yml" ]] || [[ -f "$cand/compose.yaml" ]]; then
    COMPOSE_DIR="$cand"; break
  fi
done
[[ -n "$COMPOSE_DIR" ]] || fail "nie znalazłem docker-compose.yml w $INSTALL_DIR/deploy/*"
log "compose: $COMPOSE_DIR"

cd "$COMPOSE_DIR"

# ── Rebuild + restart ─────────────────────────────────────────────────────
log "docker compose build manager"
docker compose build manager

log "docker compose up -d --force-recreate manager"
docker compose up -d --force-recreate manager

# ── Health check ──────────────────────────────────────────────────────────
if [[ -n "${HEALTH_URL:-}" ]]; then
  log "health check: $HEALTH_URL"
  for i in 1 2 3 4 5 6 7 8 9 10; do
    if curl -fsSLk "$HEALTH_URL" >/dev/null 2>&1; then
      ok "manager up ($i/10)"; exit 0
    fi
    sleep 2
  done
  fail "health check timeout"
fi

sleep 3
docker compose ps manager
ok "update done"
