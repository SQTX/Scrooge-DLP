#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
#
# ScroogeDLP - one-liner bootstrap installer.
#
# Pobiera repo (lub aktualizuje istniejące), checkoutuje wybrany branch/tag,
# delegowuje wszystko inne do `deploy/quickstart.sh` (który sam zainstaluje
# Docker, postawi manager + Postgres, zapyta o admin user/pass, etc.).
#
# Użycie:
#   curl -fsSL https://github.com/SQTX/Scrooge-DLP/raw/main/deploy/install.sh | bash
#
# Env vars (opcjonalne):
#   INSTALL_DIR   katalog docelowy (default: ~/Scrooge-DLP)
#   INSTALL_REF   branch / tag / commit do checkout (default: main)
#   REPO_URL      źródło git (default: https://github.com/SQTX/Scrooge-DLP.git)
#
# Przykład: stage'owa wersja z brancha
#   curl -fsSL https://github.com/SQTX/Scrooge-DLP/raw/main/deploy/install.sh \
#     | INSTALL_REF=develop bash

set -euo pipefail

REPO_URL="${REPO_URL:-https://github.com/SQTX/Scrooge-DLP.git}"
INSTALL_DIR="${INSTALL_DIR:-$HOME/Scrooge-DLP}"
# TEMP (Sub-faza 1E testing): default = dev żeby Wazuh-flow z mTLS dało
# się testować bez PR dev → main. Po pomyślnym E2E i mergu PR, revert na
# `main`. Override przez `INSTALL_REF=...` env w trybie produkcyjnym.
INSTALL_REF="${INSTALL_REF:-dev}"

say()  { printf '\033[36m▸ %s\033[0m\n' "$*"; }
ok()   { printf '\033[32m✔ %s\033[0m\n' "$*"; }
warn() { printf '\033[33m⚠ %s\033[0m\n' "$*"; }
err()  { printf '\033[31m✗ %s\033[0m\n' "$*" >&2; exit 1; }

# Sudo NIE jest wymagane do uruchomienia tego skryptu — quickstart sam
# w środku wywoła sudo gdzie potrzeba (Docker install, usermod, itp.).
# Jeśli user uruchomi przez `curl … | sudo bash`, $HOME zmienia się na
# /root, co psuje ownership wszystkich plików. Ostrzegamy.
if [ "$(id -u)" = "0" ] && [ -z "${ALLOW_ROOT:-}" ]; then
    cat >&2 <<'EOF'
✗ Nie uruchamiaj install.sh przez sudo (lub jako root).

  Skrypt sam wywoła sudo wewnątrz tam gdzie jest potrzebne (apt install,
  Docker install, usermod). Uruchomienie całości jako root powoduje że
  pliki repo + config + .env wylądują w /root z ownership root:root,
  co potem psuje Docker (group docker nie ma sensu dla roota itd.).

  Uruchom bez sudo:
    curl -fsSL https://github.com/SQTX/Scrooge-DLP/raw/main/deploy/install.sh | bash

  Jeśli świadomie chcesz root: ALLOW_ROOT=1 ... | bash
EOF
    exit 1
fi

# ──────────────────────────────────────────────────────────────────────────
# 1. Wymagania: git + curl
# ──────────────────────────────────────────────────────────────────────────
say "checking prerequisites (git, curl)"

install_pkg() {
    # $1: nazwa narzędzia + pakietu (zazwyczaj te same na Ubuntu/Debian/RHEL)
    local tool="$1"
    if command -v apt-get >/dev/null 2>&1; then
        sudo apt-get update -qq && sudo apt-get install -y -qq "$tool"
    elif command -v dnf >/dev/null 2>&1; then
        sudo dnf install -y "$tool"
    elif command -v yum >/dev/null 2>&1; then
        sudo yum install -y "$tool"
    elif command -v zypper >/dev/null 2>&1; then
        sudo zypper install -y "$tool"
    else
        err "nieznany package manager — zainstaluj '$tool' ręcznie i odpal ponownie"
    fi
}

for tool in git curl; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        warn "'$tool' nie znaleziony — instaluję"
        install_pkg "$tool"
    fi
done
ok "git $(git --version | awk '{print $3}'), curl ready"

# ──────────────────────────────────────────────────────────────────────────
# 2. Klon / update repo
# ──────────────────────────────────────────────────────────────────────────
if [ -d "$INSTALL_DIR/.git" ]; then
    say "$INSTALL_DIR już istnieje — fetch + checkout $INSTALL_REF"
    cd "$INSTALL_DIR"
    git fetch origin --tags --quiet
else
    say "klon $REPO_URL → $INSTALL_DIR"
    git clone --quiet "$REPO_URL" "$INSTALL_DIR"
    cd "$INSTALL_DIR"
fi

say "checkout $INSTALL_REF"
# Obsługuje branch (z origin/), tag, lub commit hash.
if git show-ref --verify --quiet "refs/remotes/origin/$INSTALL_REF"; then
    git checkout --quiet "$INSTALL_REF"
    git reset --hard --quiet "origin/$INSTALL_REF"
else
    git checkout --quiet "$INSTALL_REF"
fi
ok "na $INSTALL_REF ($(git rev-parse --short HEAD))"

# ──────────────────────────────────────────────────────────────────────────
# 3. Deleguj resztę do quickstart.sh
# ──────────────────────────────────────────────────────────────────────────
echo ""
say "uruchamiam deploy/quickstart.sh"
echo ""
exec ./deploy/quickstart.sh "$@"
