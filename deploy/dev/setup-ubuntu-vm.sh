#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
#
# ScroogeDLP — Ubuntu VM bootstrap script.
#
# Uruchom na świeżej Ubuntu 22.04 LTS VM (najczęściej w UTM na Macu).
# Idempotent — można uruchomić wielokrotnie, kroki które są zrobione są skip'owane.
#
# Wykonuje:
#   1. apt update + install build deps (gcc, libssl-dev, pkg-config, etc.)
#   2. install Rust przez rustup (jako bieżący user, NIE root)
#   3. install Docker (oficjalny repo)
#   4. install protobuf-compiler
#   5. clone repo (jeśli REPO_URL podany)
#   6. utworzenie `scrooge` system usera + /etc/scrooge/ + /var/lib/scrooge/
#   7. install systemd unit /etc/systemd/system/scrooge-agent.service
#   8. skopiuj agent.vm.yaml.example → /etc/scrooge/agent.yaml (template)
#   9. firewall: ufw allow ssh, allow outbound do managera
#  10. wypisz next steps
#
# Po zakończeniu: agent NIE jest uruchomiony — czeka na build + start.

set -euo pipefail

# ──────────────────────────────────────────────────────────────────────────
# Konfiguracja (overridable przez env)
# ──────────────────────────────────────────────────────────────────────────
REPO_URL="${REPO_URL:-https://github.com/SQTX/Scrooge-DLP.git}"
CLONE_DIR="${CLONE_DIR:-$HOME/scrooge-dlp}"
SCROOGE_USER="${SCROOGE_USER:-scrooge}"

# ──────────────────────────────────────────────────────────────────────────
# Pomocnicze
# ──────────────────────────────────────────────────────────────────────────
say()   { printf "\033[36m▸ %s\033[0m\n" "$*"; }
ok()    { printf "\033[32m✔ %s\033[0m\n" "$*"; }
warn()  { printf "\033[33m⚠ %s\033[0m\n" "$*"; }
err()   { printf "\033[31m✗ %s\033[0m\n" "$*"; exit 1; }

# Wymaga sudo (nie root), ale nie wymaga że już zalogowany jako root.
if [ "$EUID" -eq 0 ]; then
    err "uruchom jako zwykły user (skrypt sam wywoła sudo gdy potrzeba)"
fi

if ! command -v sudo >/dev/null 2>&1; then
    err "sudo wymagane — apt install sudo (jako root)"
fi

# ──────────────────────────────────────────────────────────────────────────
# 1. APT deps
# ──────────────────────────────────────────────────────────────────────────
say "apt update + build dependencies"
sudo apt-get update -qq
sudo apt-get install -y --no-install-recommends \
    build-essential \
    pkg-config \
    libssl-dev \
    protobuf-compiler \
    git \
    curl \
    ca-certificates \
    gnupg \
    lsb-release \
    ufw \
    jq
ok "apt deps installed"

# ──────────────────────────────────────────────────────────────────────────
# 2. Rust (rustup, jako bieżący user)
# ──────────────────────────────────────────────────────────────────────────
if command -v rustc >/dev/null 2>&1; then
    ok "rust już zainstalowany: $(rustc --version)"
else
    say "installing rustup + stable toolchain"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | \
        sh -s -- -y --default-toolchain stable --profile minimal
    # shellcheck disable=SC1091
    source "$HOME/.cargo/env"
    ok "rust installed: $(rustc --version)"
fi

# ──────────────────────────────────────────────────────────────────────────
# 3. Docker (oficjalny repo Docker dla Ubuntu — newer than apt's default)
# ──────────────────────────────────────────────────────────────────────────
if command -v docker >/dev/null 2>&1; then
    ok "docker już zainstalowany: $(docker --version)"
else
    say "installing Docker Engine"
    sudo install -m 0755 -d /etc/apt/keyrings
    curl -fsSL https://download.docker.com/linux/ubuntu/gpg | \
        sudo gpg --dearmor -o /etc/apt/keyrings/docker.gpg
    sudo chmod a+r /etc/apt/keyrings/docker.gpg

    UBUNTU_CODENAME="$(lsb_release -cs)"
    echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] \
        https://download.docker.com/linux/ubuntu $UBUNTU_CODENAME stable" | \
        sudo tee /etc/apt/sources.list.d/docker.list > /dev/null

    sudo apt-get update -qq
    sudo apt-get install -y docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin
    sudo usermod -aG docker "$USER"
    ok "docker installed (relogin wymagane do użycia bez sudo)"
fi

# ──────────────────────────────────────────────────────────────────────────
# 4. Repo clone (jeśli nie istnieje)
# ──────────────────────────────────────────────────────────────────────────
if [ -d "$CLONE_DIR/.git" ]; then
    ok "repo już sklonowane w $CLONE_DIR"
else
    say "cloning $REPO_URL → $CLONE_DIR"
    git clone "$REPO_URL" "$CLONE_DIR"
    ok "repo sklonowane"
fi

# ──────────────────────────────────────────────────────────────────────────
# 5. System user `scrooge` + katalogi
# ──────────────────────────────────────────────────────────────────────────
if id -u "$SCROOGE_USER" >/dev/null 2>&1; then
    ok "user $SCROOGE_USER już istnieje"
else
    say "creating system user $SCROOGE_USER"
    sudo useradd --system --create-home \
        --home-dir /var/lib/scrooge \
        --shell /usr/sbin/nologin \
        "$SCROOGE_USER"
    ok "user $SCROOGE_USER created"
fi

sudo install -d -o "$SCROOGE_USER" -g "$SCROOGE_USER" -m 0750 /etc/scrooge
sudo install -d -o "$SCROOGE_USER" -g "$SCROOGE_USER" -m 0750 /var/lib/scrooge
ok "katalogi /etc/scrooge/ i /var/lib/scrooge/ gotowe"

# ──────────────────────────────────────────────────────────────────────────
# 6. systemd unit
# ──────────────────────────────────────────────────────────────────────────
SYSTEMD_UNIT="/etc/systemd/system/scrooge-agent.service"
if [ -f "$SYSTEMD_UNIT" ]; then
    ok "$SYSTEMD_UNIT już istnieje (nie nadpisuję)"
else
    say "instaluję systemd unit"
    sudo cp "$CLONE_DIR/deploy/dev/scrooge-agent.service" "$SYSTEMD_UNIT"
    sudo systemctl daemon-reload
    ok "systemd unit gotowy (włącz przez: sudo systemctl enable scrooge-agent)"
fi

# ──────────────────────────────────────────────────────────────────────────
# 7. Config template
# ──────────────────────────────────────────────────────────────────────────
CONFIG_TARGET="/etc/scrooge/agent.yaml"
if [ -f "$CONFIG_TARGET" ]; then
    ok "$CONFIG_TARGET już istnieje (nie nadpisuję)"
else
    say "kopiuję template agent.yaml"
    sudo cp "$CLONE_DIR/deploy/dev/agent.vm.yaml.example" "$CONFIG_TARGET"
    sudo chown "$SCROOGE_USER:$SCROOGE_USER" "$CONFIG_TARGET"
    sudo chmod 0640 "$CONFIG_TARGET"
    warn "edytuj $CONFIG_TARGET przed startem agenta!"
    warn "  - manager.endpoint: IP/hostname Maca w UTM (zwykle 192.168.64.1:5443)"
    warn "  - manager.enrollment_token: token z 'scroogectl gen-token' na Macu"
fi

# ──────────────────────────────────────────────────────────────────────────
# 8. Firewall (ufw): allow SSH only, outbound nielimitowany
# ──────────────────────────────────────────────────────────────────────────
say "konfiguruję ufw (allow SSH, deny rest inbound)"
sudo ufw --force reset >/dev/null
sudo ufw default deny incoming >/dev/null
sudo ufw default allow outgoing >/dev/null
sudo ufw allow 22/tcp comment 'ssh' >/dev/null
sudo ufw --force enable >/dev/null
ok "ufw aktywny"

# ──────────────────────────────────────────────────────────────────────────
# 9. Next steps
# ──────────────────────────────────────────────────────────────────────────
cat <<EOF

$(say "Setup gotowy! Co dalej:")

1. Skopiuj CA cert z Maca na VM (Mac przygotowuje go przez 'make dev-certs'):
     scp <mac-user>@<mac-ip>:~/Projects/App/ScroogeDLP/dev-certs/ca.pem ~
     sudo install -m 0644 ~/ca.pem /etc/scrooge/ca.pem
     sudo chown ${SCROOGE_USER}:${SCROOGE_USER} /etc/scrooge/ca.pem

2. Na Macu wygeneruj enrollment token:
     make dev-bootstrap   # już zrobione - patrz dev-certs/enrollment-token.txt
     # lub świeży:
     MANAGER_CONFIG=deploy/dev/manager.dev-vm.yaml \\
       ./target/debug/scroogectl gen-token --description "ubuntu-vm"

3. Edytuj /etc/scrooge/agent.yaml na VM:
     sudo nano /etc/scrooge/agent.yaml
     # - manager.endpoint: "192.168.64.1:5443"   (lub IP Maca w UTM)
     # - manager.enrollment_token: "<TOKEN>"
     # - manager.ca_cert_path: "/etc/scrooge/ca.pem"

4. Na Macu uruchom managera w trybie VM (nasłuchuje na 0.0.0.0):
     make dev-manager-vm

5. Pierwszy build + start agenta na VM:
     cd ${CLONE_DIR}
     cargo build --release -p scrooge-agent-bin
     sudo install -m 0755 target/release/scrooge-agent /usr/bin/
     sudo systemctl enable --now scrooge-agent
     sudo journalctl -u scrooge-agent -f

6. Daily workflow:
     ${CLONE_DIR}/deploy/dev/rebuild-and-restart-agent.sh

EOF
