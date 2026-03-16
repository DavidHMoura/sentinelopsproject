#!/usr/bin/env bash
# Generates the complete dev mTLS PKI for SentinelOps.
# Usage: ./scripts/gen-dev-certs.sh [AGENT_UUID]
# AGENT_UUID defaults to "agent-uuid-1234" if not provided.
#
# WARNING: For development ONLY. Never use these certs in production.
# Production: HashiCorp Vault PKI Engine or AWS Private CA.

set -euo pipefail

CERTS_DIR="certs"
AGENT_UUID="${1:-agent-uuid-1234}"
CA_DAYS=3650
CERT_DAYS=365

# Enforce lowercase UUID format (Zero Trust invariant: cert CN must equal AGENT_ID env var,
# both must be lowercase to avoid case-sensitivity mismatches in the Java interceptor)
AGENT_UUID="${AGENT_UUID,,}"

mkdir -p "$CERTS_DIR"
cd "$CERTS_DIR"

echo "==> [1/4] Generating development CA..."
openssl genrsa -out ca.key 4096
openssl req -new -x509 -days "$CA_DAYS" -key ca.key -out ca.pem \
  -subj "/C=BR/O=SentinelOps Dev/CN=SentinelOps Dev CA"

echo "==> [2/4] Generating Control Plane server cert..."
openssl genrsa -out server.key 2048
openssl req -new -key server.key -out server.csr \
  -subj "/C=BR/O=SentinelOps/CN=sentinelops-control.internal"

cat > server-ext.cnf <<EOF
[req_ext]
subjectAltName=DNS:sentinelops-control.internal,DNS:localhost,IP:127.0.0.1
EOF

openssl x509 -req -days "$CERT_DAYS" \
  -in server.csr -CA ca.pem -CAkey ca.key -CAcreateserial \
  -out server.pem -extfile server-ext.cnf -extensions req_ext

echo "==> [3/4] Generating Agent cert (CN=${AGENT_UUID})..."
openssl genrsa -out agent.key 2048
openssl req -new -key agent.key -out agent.csr \
  -subj "/C=BR/O=SentinelOps/CN=${AGENT_UUID}"
openssl x509 -req -days "$CERT_DAYS" \
  -in agent.csr -CA ca.pem -CAkey ca.key -CAcreateserial \
  -out agent.pem

echo "==> [4/4] Verifying trust chain..."
openssl verify -CAfile ca.pem server.pem && echo "  server.pem OK"
openssl verify -CAfile ca.pem agent.pem  && echo "  agent.pem  OK"

rm -f server.csr server-ext.cnf agent.csr ./*.srl

cat <<SUMMARY

Certificates written to ./${CERTS_DIR}/
  ca.pem         — trust anchor (distribute to both Rust agent and Java server)
  server.pem/key — Java Control Plane
  agent.pem/key  — Rust Agent (CN=${AGENT_UUID})

Zero Trust invariant:
  Set AGENT_ID=${AGENT_UUID} in sentinelops-agent/.env
  Java interceptor extracts CN from mTLS cert and validates against agent_id payload field.
  CN != agent_id → PERMISSION_DENIED.

Production: use Vault PKI with 24h TTL + cert-manager rotation.
SUMMARY
