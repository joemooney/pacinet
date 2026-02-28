#!/bin/bash
# Generate development TLS certificates for PaciNet mTLS
# For development/testing only — NOT for production use.
# Generates X.509 v3 certificates (required by rustls 0.23+).
set -euo pipefail

CERT_DIR="${1:-certs}"
DAYS=365
SUBJ_PREFIX="/C=US/ST=Dev/L=Local/O=PaciNet"

mkdir -p "$CERT_DIR"
echo "Generating certificates in $CERT_DIR/"

# CA key and certificate (v3 with basicConstraints)
openssl req -x509 -newkey rsa:4096 -days "$DAYS" -nodes \
    -keyout "$CERT_DIR/ca-key.pem" \
    -out "$CERT_DIR/ca.pem" \
    -subj "$SUBJ_PREFIX/CN=PaciNet CA" \
    -addext "basicConstraints=critical,CA:TRUE" \
    -addext "keyUsage=critical,keyCertSign,cRLSign" \
    -addext "subjectKeyIdentifier=hash" \
    2>/dev/null
echo "  CA certificate: $CERT_DIR/ca.pem"

# Server certificate (controller) — v3 with SANs
openssl req -newkey rsa:2048 -nodes \
    -keyout "$CERT_DIR/server-key.pem" \
    -out "$CERT_DIR/server.csr" \
    -subj "$SUBJ_PREFIX/CN=pacinet-server" \
    2>/dev/null
openssl x509 -req -days "$DAYS" \
    -in "$CERT_DIR/server.csr" \
    -CA "$CERT_DIR/ca.pem" -CAkey "$CERT_DIR/ca-key.pem" -CAcreateserial \
    -out "$CERT_DIR/server.pem" \
    -extfile <(printf "subjectAltName=DNS:localhost,IP:127.0.0.1\nbasicConstraints=CA:FALSE\nkeyUsage=digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth") \
    2>/dev/null
echo "  Server cert:    $CERT_DIR/server.pem"

# Agent certificate — v3 with SANs (acts as both client and server)
openssl req -newkey rsa:2048 -nodes \
    -keyout "$CERT_DIR/agent-key.pem" \
    -out "$CERT_DIR/agent.csr" \
    -subj "$SUBJ_PREFIX/CN=pacinet-agent" \
    2>/dev/null
openssl x509 -req -days "$DAYS" \
    -in "$CERT_DIR/agent.csr" \
    -CA "$CERT_DIR/ca.pem" -CAkey "$CERT_DIR/ca-key.pem" -CAcreateserial \
    -out "$CERT_DIR/agent.pem" \
    -extfile <(printf "subjectAltName=DNS:localhost,IP:127.0.0.1,IP:0.0.0.0\nbasicConstraints=CA:FALSE\nkeyUsage=digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth,clientAuth") \
    2>/dev/null
echo "  Agent cert:     $CERT_DIR/agent.pem"

# CLI client certificate — v3
openssl req -newkey rsa:2048 -nodes \
    -keyout "$CERT_DIR/client-key.pem" \
    -out "$CERT_DIR/client.csr" \
    -subj "$SUBJ_PREFIX/CN=pacinet-cli" \
    2>/dev/null
openssl x509 -req -days "$DAYS" \
    -in "$CERT_DIR/client.csr" \
    -CA "$CERT_DIR/ca.pem" -CAkey "$CERT_DIR/ca-key.pem" -CAcreateserial \
    -out "$CERT_DIR/client.pem" \
    -extfile <(printf "basicConstraints=CA:FALSE\nkeyUsage=digitalSignature,keyEncipherment\nextendedKeyUsage=clientAuth") \
    2>/dev/null
echo "  Client cert:    $CERT_DIR/client.pem"

# Clean up CSR files
rm -f "$CERT_DIR"/*.csr "$CERT_DIR"/*.srl

echo ""
echo "Done. Usage:"
echo "  Server:  --ca-cert $CERT_DIR/ca.pem --tls-cert $CERT_DIR/server.pem --tls-key $CERT_DIR/server-key.pem"
echo "  Agent:   --ca-cert $CERT_DIR/ca.pem --tls-cert $CERT_DIR/agent.pem --tls-key $CERT_DIR/agent-key.pem"
echo "  CLI:     --ca-cert $CERT_DIR/ca.pem --tls-cert $CERT_DIR/client.pem --tls-key $CERT_DIR/client-key.pem"
