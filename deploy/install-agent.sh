#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
#
# ScroogeDLP agent — one-liner installer (build-from-source).
#
# Użycie (github raw, jak manager install.sh):
#
#   curl -fsSL https://github.com/SQTX/Scrooge-DLP/raw/main/deploy/install-agent.sh \
#     | MANAGER=192.168.1.10:5443 TOKEN=<UUID> sudo -E bash
#
# Wymagane env vars (przed `bash`, NIE przed curl):
#   MANAGER  — host:port managera dla gRPC mTLS, np. 192.168.1.10:5443
#   TOKEN    — enrollment token z dashboard "Add agent" (UUID v4)
#
# Opcjonalne env vars:
#   INSTALL_REF   — branch/tag git (default: main; pre-release: claude/<auto>)
#   INSTALL_DIR   — gdzie sklonowac repo (default: /opt/scrooge-src)
#   REST_BASE     — base URL HTTPS managera dla CA fetch (default:
#                   https://<host>:55000 — manager REST port; gRPC jest na :5443)
#   REPO_URL      — repo git (default: https://github.com/SQTX/Scrooge-DLP.git)
#   USER_SERVICE  — '1' = systemd --user unit (uruchom agent w sesji usera —
#                   działa clipboard DLP, ale agent gaśnie przy logout).
#                   Default 0 = system unit (root, headless OK, brak schowka).
#                   Wymaga: `loginctl enable-linger $TARGET_USER` dla auto-start.
#   TARGET_USER   — dla USER_SERVICE=1 — który user (default: $SUDO_USER albo `user`).
#   ALLOW_ROOT    — '1' żeby pozwolić uruchomić jako root bez sudo bootstrap'u

set -euo pipefail

# ── Kolory ────────────────────────────────────────────────────────────────
log()  { printf '\033[1;36m▸\033[0m %s\n' "$*"; }
ok()   { printf '\033[1;32m✔\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m⚠\033[0m %s\n' "$*" >&2; }
fail() {
  printf '\033[1;31m✗\033[0m %s\n' "$*" >&2
  printf '\n\033[1;33mℹ\033[0m  Reset state przed retry:\n' >&2
  printf '   sudo systemctl stop scrooge-agent 2>/dev/null\n' >&2
  printf '   sudo rm -rf /etc/scrooge /var/lib/scrooge /opt/scrooge-src\n' >&2
  printf '   cd ~  # shell może być w skasowanym cwd\n' >&2
  exit 1
}

# Bash zachowuje cwd nawet po `rm -rf`. Jeśli cwd nie istnieje, `cd ~`
# przed jakąkolwiek pracą (rustup particularnie wymaga valid cwd).
if ! pwd >/dev/null 2>&1; then
  cd "$HOME"
fi

# ── Walidacja ─────────────────────────────────────────────────────────────
[[ -n "${MANAGER:-}" ]] || fail "MANAGER nie ustawione (np. MANAGER=192.168.1.10:5443)"
[[ -n "${TOKEN:-}" ]]   || fail "TOKEN nie ustawione (z dashboard → Agents → Add agent)"
[[ $EUID -eq 0 ]] || fail "Musisz odpalić jako root (sudo -E bash)"

INSTALL_REF="${INSTALL_REF:-main}"
INSTALL_DIR="${INSTALL_DIR:-/opt/scrooge-src}"
REPO_URL="${REPO_URL:-https://github.com/SQTX/Scrooge-DLP.git}"
USER_SERVICE="${USER_SERVICE:-0}"
TARGET_USER="${TARGET_USER:-${SUDO_USER:-user}}"

# REST_BASE heurystyka: MANAGER=host:5443 (gRPC) → REST na host:55000.
# Override przez explicit REST_BASE env var.
if [[ -z "${REST_BASE:-}" ]]; then
  MGR_HOST="${MANAGER%%:*}"
  REST_BASE="https://${MGR_HOST}:55000"
fi

log "Manager:    $MANAGER"
log "Token:      ${TOKEN:0:8}…"
log "Install ref: $INSTALL_REF"

# ── Disk space check (cargo build ~3GB target/ + sqlx) ────────────────────
# Free space wymagane: ~5GB w /var (sqlx vendor cache + /opt build).
AVAIL_KB=$(df --output=avail / 2>/dev/null | tail -1 | tr -d ' ' || echo "0")
AVAIL_GB=$((AVAIL_KB / 1024 / 1024))
if [[ "$AVAIL_GB" -lt 5 ]]; then
  warn "free disk: ${AVAIL_GB}GB — build wymaga ~5GB. Cleanup zalecany:"
  warn "  sudo apt-get clean && sudo journalctl --vacuum-size=200M"
  warn "Kontynuuję mimo to (może się udać przy <5GB)."
fi

# ── Detect headless (brak GUI/DISPLAY) ────────────────────────────────────
# Phase 3 clipboard DLP wymaga X11/Wayland w sesji usera. System service
# (root, brak DISPLAY) NIE zobaczy schowka — enrollment + heartbeat
# zadziała, ale event'y clipboard nie. Wczesny warn żeby user wiedział
# czego się spodziewać.
if [[ -z "${DISPLAY:-}" ]] && [[ -z "${WAYLAND_DISPLAY:-}" ]] && ! [[ -d /tmp/.X11-unix ]]; then
  warn "headless host (brak DISPLAY/WAYLAND_DISPLAY/X11-unix) — clipboard DLP"
  warn "NIE wygeneruje eventów. Enrollment + heartbeat zadziała."
  warn "Dla pełnego demo Phase 3 użyj VM z Ubuntu Desktop (GUI sesja)."
fi

# ── Detect OS ─────────────────────────────────────────────────────────────
[[ -r /etc/os-release ]] || fail "missing /etc/os-release"
# shellcheck disable=SC1091
. /etc/os-release
case "${ID:-}${ID_LIKE:-}" in
  *debian*|*ubuntu*) PKG=apt ;;
  *rhel*|*fedora*|*rocky*|*alma*|*centos*) PKG=dnf ;;
  *) fail "unsupported distribution: ${PRETTY_NAME:-unknown}" ;;
esac

# ── Deps systemowe ────────────────────────────────────────────────────────
log "installing system deps"
if [[ "$PKG" == "apt" ]]; then
  DEBIAN_FRONTEND=noninteractive apt-get update -qq
  DEBIAN_FRONTEND=noninteractive apt-get install -y \
    build-essential pkg-config nasm cmake git curl ca-certificates \
    protobuf-compiler \
    libxcb1-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev \
    libxkbcommon-dev libssl-dev
else
  ${PKG} install -y gcc gcc-c++ make pkgconfig nasm cmake git curl ca-certificates \
    protobuf-compiler \
    libxcb-devel libxkbcommon-devel openssl-devel
fi

# ── Rustup ────────────────────────────────────────────────────────────────
if ! command -v cargo >/dev/null 2>&1; then
  log "installing rustup (stable, system-wide /usr/local/cargo)"
  # System-wide install — root's PATH widzi cargo, dev user też (przez
  # /usr/local/bin/cargo symlink). Eliminuje "sudo cargo command not found".
  export CARGO_HOME=/usr/local/cargo
  export RUSTUP_HOME=/usr/local/rustup
  curl -fsSL --proto '=https' --tlsv1.2 https://sh.rustup.rs \
    | sh -s -- -y --default-toolchain stable --profile minimal --no-modify-path
  # Symlinki dla world (cargo / rustc / rustup w /usr/local/bin).
  for bin in cargo rustc rustup; do
    if [[ -x "$CARGO_HOME/bin/$bin" ]] && [[ ! -e "/usr/local/bin/$bin" ]]; then
      ln -sf "$CARGO_HOME/bin/$bin" "/usr/local/bin/$bin"
    fi
  done
  export PATH="$CARGO_HOME/bin:$PATH"
fi

# ── Pre-flight checks (fail fast zanim cargo build) ──────────────────────
log "pre-flight: manager reachability"

# 1. REST_BASE — sprawdź czy /ca.pem zwraca PEM. Najczęstszy bug: user
#    podał MANAGER=host:5443 (gRPC), REST_BASE auto-zgaduje :55000, ale
#    admin ma inny port → 404 / connection refused.
CA_PROBE=$(curl -sk --connect-timeout 5 --max-time 10 "$REST_BASE/ca.pem" 2>&1 || true)
if ! grep -q "BEGIN CERTIFICATE" <<<"$CA_PROBE"; then
  fail "REST $REST_BASE/ca.pem nie zwraca PEM (sprawdź port + dostępność managera).
  Override: REST_BASE=https://<host>:<rest_port>
  Output (first 200 bytes): ${CA_PROBE:0:200}"
fi
ok "REST manager reachable: $REST_BASE"

# 2. gRPC port — czysty TCP probe (nie pełen TLS handshake — to robi
#    agent przy enroll). Wykrywa typowe network/firewall issues.
MGR_HOST="${MANAGER%%:*}"
MGR_PORT="${MANAGER##*:}"
if ! timeout 5 bash -c "exec 3<>/dev/tcp/$MGR_HOST/$MGR_PORT" 2>/dev/null; then
  fail "gRPC $MANAGER nieosiągalny (firewall? wrong port?)."
fi
ok "gRPC manager reachable: $MANAGER"

# ── Pobierz CA managera (bootstrap TLS trust) ─────────────────────────────
# Dir 755 żeby world-read na ca.pem działał (agent.yaml zostaje 640).
install -d -m 755 /etc/scrooge
log "fetching CA from $REST_BASE/ca.pem"
# `-k` na initial fetch — manager używa self-signed; po tym krok'u
# wszystkie kolejne połączenia (gRPC enroll + Stream) są pełen mTLS.
curl -fsSLk "$REST_BASE/ca.pem" -o /etc/scrooge/ca.pem
chmod 644 /etc/scrooge/ca.pem

# Sanity: czy to faktyczny PEM Root CA?
if ! grep -q "BEGIN CERTIFICATE" /etc/scrooge/ca.pem; then
  fail "CA fetch z $REST_BASE/ca.pem nie zwrócił PEM (port może być gRPC zamiast REST?). \
Ustaw explicit REST_BASE=https://<host>:<rest_port>."
fi

# ── Clone + build ─────────────────────────────────────────────────────────
log "cloning $REPO_URL @ $INSTALL_REF → $INSTALL_DIR"
if [[ -d "$INSTALL_DIR/.git" ]]; then
  git -C "$INSTALL_DIR" fetch --depth 1 origin "$INSTALL_REF"
  git -C "$INSTALL_DIR" checkout "$INSTALL_REF"
  git -C "$INSTALL_DIR" reset --hard "origin/$INSTALL_REF"
else
  git clone --branch "$INSTALL_REF" --depth 1 "$REPO_URL" "$INSTALL_DIR"
fi

log "cargo build --release -p scrooge-agent-bin (~5-10 min)"
( cd "$INSTALL_DIR" && cargo build --release -p scrooge-agent-bin )

install -m 755 "$INSTALL_DIR/target/release/scrooge-agent" /usr/local/bin/scrooge-agent
ok "binary → /usr/local/bin/scrooge-agent"

# Chown $INSTALL_DIR na dev user (SUDO_USER) — install ran przez `sudo`,
# więc clone + target/ owned by root. Bez tego późniejsze manualne `cargo
# build` jako dev user fail'uje z "Permission denied .cargo-lock".
if [[ -n "${SUDO_USER:-}" ]] && id "$SUDO_USER" >/dev/null 2>&1; then
  chown -R "$SUDO_USER":"$SUDO_USER" "$INSTALL_DIR" 2>/dev/null || true
  log "$INSTALL_DIR ownership → $SUDO_USER (manual rebuild możliwy bez sudo)"
fi

# ── User + katalogi ───────────────────────────────────────────────────────
id scrooge >/dev/null 2>&1 || useradd --system --no-create-home --shell /usr/sbin/nologin scrooge
install -d -m 750 -o scrooge -g scrooge /var/lib/scrooge

# ── Config ────────────────────────────────────────────────────────────────
cat > /etc/scrooge/agent.yaml <<EOF
manager:
  endpoint: "$MANAGER"
  enrollment_token: "$TOKEN"
  ca_cert_path: "/etc/scrooge/ca.pem"
agent:
  data_dir: "/var/lib/scrooge"
  heartbeat_interval_secs: 30
EOF
chmod 640 /etc/scrooge/agent.yaml
chown root:scrooge /etc/scrooge/agent.yaml
ok "config → /etc/scrooge/agent.yaml"

# ── systemd unit ──────────────────────────────────────────────────────────
if [[ "$USER_SERVICE" == "1" ]]; then
  # systemd --user unit — działa w sesji TARGET_USER, ma DISPLAY/WAYLAND
  # przy zalogowaniu → clipboard DLP funkcjonalny. Auto-start wymaga
  # `loginctl enable-linger $TARGET_USER`.
  id "$TARGET_USER" >/dev/null 2>&1 || fail "TARGET_USER=$TARGET_USER nie istnieje"
  USER_HOME=$(getent passwd "$TARGET_USER" | cut -d: -f6)
  install -d -m 755 -o "$TARGET_USER" -g "$TARGET_USER" "$USER_HOME/.config/systemd/user"

  # agent.yaml musi być readable dla TARGET_USER (nie tylko scrooge group).
  usermod -aG scrooge "$TARGET_USER" 2>/dev/null || true

  cat > "$USER_HOME/.config/systemd/user/scrooge-agent.service" <<EOF
[Unit]
Description=ScroogeDLP endpoint agent (user session)
After=graphical-session.target

[Service]
Type=simple
ExecStart=/usr/local/bin/scrooge-agent --config /etc/scrooge/agent.yaml
Restart=on-failure
RestartSec=5
Environment=LOG_FORMAT=json

[Install]
WantedBy=default.target
EOF
  chown "$TARGET_USER":"$TARGET_USER" "$USER_HOME/.config/systemd/user/scrooge-agent.service"

  # Enable lingering — agent startuje przy boot bez wymagania login'u.
  loginctl enable-linger "$TARGET_USER"

  # Start jako TARGET_USER przez machinectl/systemctl-as-user.
  sudo -u "$TARGET_USER" XDG_RUNTIME_DIR="/run/user/$(id -u "$TARGET_USER")" \
    systemctl --user daemon-reload
  sudo -u "$TARGET_USER" XDG_RUNTIME_DIR="/run/user/$(id -u "$TARGET_USER")" \
    systemctl --user enable scrooge-agent
  sudo -u "$TARGET_USER" XDG_RUNTIME_DIR="/run/user/$(id -u "$TARGET_USER")" \
    systemctl --user restart scrooge-agent

  ok "scrooge-agent (user service for $TARGET_USER) started"
  echo
  ok "Phase 3 clipboard DLP AKTYWNE — agent ma DISPLAY z sesji $TARGET_USER."
  log "Tail logs: sudo -u $TARGET_USER journalctl --user -u scrooge-agent -f"
else
  # Default — system unit (root). Headless OK, ale brak schowka GUI.
  cat > /etc/systemd/system/scrooge-agent.service <<'EOF'
[Unit]
Description=ScroogeDLP endpoint agent
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart=/usr/local/bin/scrooge-agent --config /etc/scrooge/agent.yaml
Restart=on-failure
RestartSec=5
User=root
Environment=LOG_FORMAT=json

[Install]
WantedBy=multi-user.target
EOF

  systemctl daemon-reload
  systemctl enable scrooge-agent
  systemctl restart scrooge-agent

  ok "scrooge-agent (system unit, root) started — tail: journalctl -u scrooge-agent -f"
  echo
  warn "Phase 3 clipboard DLP wymaga DISPLAY env. System unit (root, no GUI)"
  warn "NIE zobaczy schowka. Dla clipboard demo: USER_SERVICE=1 install"
  warn "albo manualnie: DISPLAY=:0 /usr/local/bin/scrooge-agent ..."
fi
