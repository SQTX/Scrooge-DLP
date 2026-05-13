#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
#
# ScroogeDLP — daily VM workflow.
# Uruchamiane na VM po `git push` z Maca.
#
#   git pull → cargo build --release → install binary → restart service → tail logs

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

say() { printf "\033[36m▸ %s\033[0m\n" "$*"; }
ok()  { printf "\033[32m✔ %s\033[0m\n" "$*"; }

cd "$REPO_ROOT"

say "git pull"
git pull --ff-only

say "cargo build --release -p scrooge-agent-bin"
# shellcheck disable=SC1091
[ -f "$HOME/.cargo/env" ] && source "$HOME/.cargo/env"
cargo build --release -p scrooge-agent-bin

say "install binary do /usr/bin/"
sudo install -m 0755 target/release/scrooge-agent /usr/bin/scrooge-agent
ok "binary updated"

say "restart scrooge-agent.service"
sudo systemctl restart scrooge-agent

ok "agent restarted — następuje tail journalu (Ctrl-C żeby wyjść)"
echo ""
sudo journalctl -u scrooge-agent -f
