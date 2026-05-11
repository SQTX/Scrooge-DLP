#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
#
# generate-dev-certs.sh
#
# Generuje dev CA + server cert dla lokalnego managera. NIE używać w produkcji.
# Quickstart.sh (Krok 9) zrobi to samo, ale przez Rust binary z lepszym UX.

set -euo pipefail

CERT_DIR="${CERT_DIR:-./dev-certs}"
mkdir -p "$CERT_DIR"
cd "$CERT_DIR"

echo "▸ generating dev CA (ECDSA P-256, valid 10y)…"
# `openssl genpkey` daje PKCS#8 (rcgen 0.13 nie parsuje legacy SEC1).
openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:P-256 -out ca.key
openssl req -new -x509 -key ca.key -out ca.pem \
    -days 3650 -subj "/CN=ScroogeDLP Dev CA/O=ScroogeDLP/OU=Dev"

# Mac IP w sieci UTM Shared Network (gateway dla VM) — żeby cert był ważny
# z perspektywy agentów na VM łączących się do hosta. Override przez env.
UTM_MAC_IP="${UTM_MAC_IP:-192.168.64.1}"

echo "▸ generating server cert (signed by CA, valid 1y, SAN: localhost,127.0.0.1,${UTM_MAC_IP})…"
openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:P-256 -out server.key
openssl req -new -key server.key -out server.csr \
    -subj "/CN=localhost/O=ScroogeDLP/OU=Dev"

cat > server.ext <<EOF
subjectAltName=DNS:localhost,IP:127.0.0.1,IP:${UTM_MAC_IP}
keyUsage=digitalSignature,keyEncipherment
extendedKeyUsage=serverAuth,clientAuth
EOF

openssl x509 -req -in server.csr -CA ca.pem -CAkey ca.key -CAcreateserial \
    -out server.pem -days 365 -extfile server.ext -sha256

rm -f server.csr server.ext ca.srl

chmod 600 ./*.key
chmod 644 ca.pem server.pem

echo ""
echo "✔ dev certs written to $(pwd):"
ls -l ca.pem ca.key server.pem server.key
