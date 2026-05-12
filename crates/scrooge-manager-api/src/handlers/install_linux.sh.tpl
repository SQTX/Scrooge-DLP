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

# ─── Pobierz pakiet ────────────────────────────────────────────────────────
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

if [[ "$PKG_FORMAT" == "deb" ]]; then
  PKG_FILE="scrooge-agent_${PACKAGE_VERSION}_${DEB_ARCH}.deb"
else
  PKG_FILE="scrooge-agent-${PACKAGE_VERSION}.${RPM_ARCH}.rpm"
fi
PKG_URL="${RELEASES_BASE}/${RELEASE_TAG}/${PKG_FILE}"

log "downloading ${PKG_FILE} from ${RELEASE_TAG}"
curl -fsSL --retry 3 "$PKG_URL" -o "$TMP/$PKG_FILE"

# ─── Zainstaluj ────────────────────────────────────────────────────────────
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

# ─── Zapisz CA i podpnij do agent.yaml ─────────────────────────────────────
install -d -m 750 /etc/scrooge
printf '%s\n' "$CA_PEM" | install -m 644 /dev/stdin /etc/scrooge/ca.pem

CONF=/etc/scrooge/agent.yaml
if [[ ! -f $CONF ]]; then
  fail "$CONF nie istnieje — pakiet zostawia go w postinst; sprawdz logi"
fi

log "configuring $CONF"
# Idempotent: jezeli linia istnieje, podmieniamy; inaczej dorzucamy do
# manager: section. W MVP zakladamy ze template z postinst ma juz pola.
sed -i \
  -e "s|^\(\s*endpoint:\).*|\1 \"$MANAGER_ENDPOINT\"|" \
  -e "s|^\(\s*enrollment_token:\).*|\1 \"$ENROLLMENT_TOKEN\"|" \
  -e "s|^\(\s*ca_cert_path:\).*|\1 \"/etc/scrooge/ca.pem\"|" \
  "$CONF"

# ─── Enable + start ────────────────────────────────────────────────────────
log "enabling scrooge-agent service"
systemctl daemon-reload
systemctl enable scrooge-agent
systemctl restart scrooge-agent

log "agent installed — connecting to $MANAGER_ENDPOINT"
log "tail logs: journalctl -u scrooge-agent -f"
