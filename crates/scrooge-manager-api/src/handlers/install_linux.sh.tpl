#!/bin/bash
# SPDX-License-Identifier: GPL-2.0-only
# ScroogeDLP - agent installer (Linux).
# Generated server-side by GET /api/v1/install.sh — do NOT edit on disk;
# the manager re-renders this every request.

set -euo pipefail

# ─── Wartości wstrzyknięte przez managera ──────────────────────────────────
MANAGER_ENDPOINT='{{MANAGER_ENDPOINT}}'
INSTALL_BASE_URL='{{INSTALL_BASE_URL}}'
ENROLLMENT_TOKEN='{{ENROLLMENT_TOKEN}}'
RELEASE_TAG='{{RELEASE_TAG}}'
PACKAGE_VERSION='{{PACKAGE_VERSION}}'
RELEASES_BASE='{{RELEASES_BASE}}'
# Phase 3: build-from-source mode (ustawiane przez ?ref=<branch> query param).
# Pusty = standardowy flow z prebuilt .deb/.rpm. Set = clone branch + cargo build.
INSTALL_REF='{{INSTALL_REF}}'
REPO_URL='https://github.com/SQTX/Scrooge-DLP.git'

# ─── Inline CA PEM ─────────────────────────────────────────────────────────
CA_PEM=$(cat <<'__SCROOGE_CA_PEM_EOF__'
{{CA_PEM}}
__SCROOGE_CA_PEM_EOF__
)

# ─── Sanity ────────────────────────────────────────────────────────────────
log() { printf '\033[1;36m▸\033[0m %s\n' "$*"; }
fail() { printf '\033[1;31m✗\033[0m %s\n' "$*" >&2; exit 1; }

[[ $EUID -eq 0 ]] || fail "ScroogeDLP installer must run as root (try: sudo bash)"

if [[ ! -r /etc/os-release ]]; then
  fail "missing /etc/os-release — cannot detect Linux distribution"
fi
# shellcheck disable=SC1091
. /etc/os-release

case "${ID:-}${ID_LIKE:-}" in
  *debian*|*ubuntu*) PKG_FORMAT="deb" ;;
  *rhel*|*fedora*|*rocky*|*alma*|*centos*) PKG_FORMAT="rpm" ;;
  *) fail "unsupported distribution: ${PRETTY_NAME:-unknown}" ;;
esac

RAW_ARCH=$(uname -m)
case "$RAW_ARCH" in
  x86_64|amd64)  DEB_ARCH="amd64"; RPM_ARCH="x86_64"  ;;
  aarch64|arm64) DEB_ARCH="arm64"; RPM_ARCH="aarch64" ;;
  *) fail "unsupported architecture: $RAW_ARCH" ;;
esac

# ─── Tymczasowy katalog (dla pakietu albo source clone) ────────────────────
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

if [[ -n "$INSTALL_REF" ]]; then
  # ════════════════════════════════════════════════════════════════════════
  # BUILD-FROM-SOURCE MODE (Phase 3 dev / pre-release testing).
  # Klonuje branch + cargo build --release. Wymaga Rust toolchain + deps
  # do arboard (X11/Wayland) + aws-lc-sys (nasm + cmake).
  # ════════════════════════════════════════════════════════════════════════
  log "build-from-source mode (ref=$INSTALL_REF)"

  if [[ "$PKG_FORMAT" == "deb" ]]; then
    DEBIAN_FRONTEND=noninteractive apt-get update -qq
    DEBIAN_FRONTEND=noninteractive apt-get install -y \
      build-essential pkg-config nasm cmake git curl ca-certificates \
      protobuf-compiler \
      libxcb1-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev \
      libxkbcommon-dev libssl-dev
  else
    if command -v dnf >/dev/null 2>&1; then DNF=dnf; else DNF=yum; fi
    $DNF install -y gcc gcc-c++ make pkgconfig nasm cmake git curl ca-certificates \
      protobuf-compiler \
      libxcb-devel libxkbcommon-devel openssl-devel
  fi

  if ! command -v cargo >/dev/null 2>&1; then
    log "installing rustup (stable toolchain)"
    curl -fsSL --proto '=https' --tlsv1.2 https://sh.rustup.rs | sh -s -- -y --default-toolchain stable --profile minimal
    export PATH="$HOME/.cargo/bin:$PATH"
  fi

  log "cloning $REPO_URL @ $INSTALL_REF"
  git clone --branch "$INSTALL_REF" --depth 1 "$REPO_URL" "$TMP/repo"
  log "cargo build --release -p scrooge-agent-bin (5-10 min)"
  ( cd "$TMP/repo" && cargo build --release -p scrooge-agent-bin )

  install -m 755 "$TMP/repo/target/release/scrooge-agent" /usr/local/bin/scrooge-agent

  # ── Stwórz user + katalogi ───────────────────────────────────────────
  id scrooge >/dev/null 2>&1 || useradd --system --no-create-home --shell /usr/sbin/nologin scrooge
  install -d -m 750 -o scrooge -g scrooge /var/lib/scrooge
  install -d -m 750 /etc/scrooge
  printf '%s\n' "$CA_PEM" | install -m 644 /dev/stdin /etc/scrooge/ca.pem

  # ── Stwórz agent.yaml ────────────────────────────────────────────────
  cat > /etc/scrooge/agent.yaml <<EOF
manager:
  endpoint: "$MANAGER_ENDPOINT"
  enrollment_token: "$ENROLLMENT_TOKEN"
  ca_cert_path: "/etc/scrooge/ca.pem"
agent:
  data_dir: "/var/lib/scrooge"
  heartbeat_interval_secs: 30
EOF
  chmod 640 /etc/scrooge/agent.yaml
  chown root:scrooge /etc/scrooge/agent.yaml

  # ── Stwórz systemd unit (root, system service) ───────────────────────
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

else
  # ════════════════════════════════════════════════════════════════════════
  # STANDARDOWY FLOW: prebuilt .deb / .rpm z GitHub Releases.
  # ════════════════════════════════════════════════════════════════════════
  if [[ "$PKG_FORMAT" == "deb" ]]; then
    PKG_FILE="scrooge-agent_${PACKAGE_VERSION}_${DEB_ARCH}.deb"
  else
    PKG_FILE="scrooge-agent-${PACKAGE_VERSION}.${RPM_ARCH}.rpm"
  fi
  PKG_URL="${RELEASES_BASE}/${RELEASE_TAG}/${PKG_FILE}"

  log "downloading ${PKG_FILE} from ${RELEASE_TAG}"
  curl -fsSL --retry 3 "$PKG_URL" -o "$TMP/$PKG_FILE"

  log "installing $PKG_FILE"
  if [[ "$PKG_FORMAT" == "deb" ]]; then
    DEBIAN_FRONTEND=noninteractive apt-get install -y "$TMP/$PKG_FILE"
  else
    if command -v dnf >/dev/null 2>&1; then
      dnf install -y "$TMP/$PKG_FILE"
    else
      yum install -y "$TMP/$PKG_FILE"
    fi
  fi

  install -d -m 750 /etc/scrooge
  printf '%s\n' "$CA_PEM" | install -m 644 /dev/stdin /etc/scrooge/ca.pem

  CONF=/etc/scrooge/agent.yaml
  if [[ ! -f $CONF ]]; then
    fail "$CONF nie istnieje — pakiet zostawia go w postinst; sprawdz logi"
  fi

  log "configuring $CONF"
  sed -i \
    -e "s|^\(\s*endpoint:\).*|\1 \"$MANAGER_ENDPOINT\"|" \
    -e "s|^\(\s*enrollment_token:\).*|\1 \"$ENROLLMENT_TOKEN\"|" \
    -e "s|^\(\s*ca_cert_path:\).*|\1 \"/etc/scrooge/ca.pem\"|" \
    "$CONF"
fi

# ─── Enable + start (oba tryby) ────────────────────────────────────────────
log "enabling scrooge-agent service"
systemctl daemon-reload
systemctl enable scrooge-agent
systemctl restart scrooge-agent

log "agent installed — connecting to $MANAGER_ENDPOINT"
log "tail logs: journalctl -u scrooge-agent -f"
if [[ -n "$INSTALL_REF" ]]; then
  log "UWAGA: build-from-source — clipboard DLP wymaga DISPLAY w env; jako"
  log "root system service NIE zobaczy schowka GUI. Demo Phase 3: stop"
  log "service i uruchom agenta ręcznie z sesji GUI ('systemctl stop"
  log "scrooge-agent && DISPLAY=:0 /usr/local/bin/scrooge-agent')."
fi
