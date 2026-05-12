#!/usr/bin/env bash
# scripts/smoke.sh — manual end-to-end smoke for recor-declaration.
#
# Brings up docker-compose, exercises POST + GET against the service,
# tears down. Run from the services/declaration/ directory.
#
# Requires: docker, openssl, jq, curl. The smoke uses the dev-only
# X-Recor-Dev-Principal auth shortcut; it does NOT exercise the JWT path.

set -euo pipefail

cd "$(dirname "$0")/.."

if [ ! -f .env ]; then
    echo "RECOR_DB_PASSWORD=$(openssl rand -base64 24)" > .env
    echo "Generated .env with random RECOR_DB_PASSWORD"
fi

echo "── Bringing up compose stack ──"
docker compose up -d --build

echo "── Waiting for health ──"
for i in {1..30}; do
    if curl -fsS http://127.0.0.1:8080/healthz >/dev/null 2>&1; then
        echo "service healthy after ${i}s"
        break
    fi
    sleep 1
done

PRINCIPAL="spiffe://recor.cm/smoke-test"

# Generate an Ed25519 keypair via openssl, extract the 32-byte raw key.
KEYDIR=$(mktemp -d)
trap 'rm -rf "$KEYDIR"' EXIT
openssl genpkey -algorithm Ed25519 -out "$KEYDIR/sk.pem" 2>/dev/null
SK_HEX=$(openssl asn1parse -in "$KEYDIR/sk.pem" -strparse 14 -noout -out "$KEYDIR/sk.raw" 2>/dev/null \
         && head -c 32 "$KEYDIR/sk.raw" | xxd -p -c 64)
PK_HEX=$(openssl pkey -in "$KEYDIR/sk.pem" -pubout -outform DER 2>/dev/null \
         | tail -c 32 | xxd -p -c 64)

# UUIDs (prefer /proc kernel UUID; fall back to uuidgen if available).
gen_uuid() {
    if [ -r /proc/sys/kernel/random/uuid ]; then
        cat /proc/sys/kernel/random/uuid
    elif command -v uuidgen >/dev/null 2>&1; then
        uuidgen | tr 'A-Z' 'a-z'
    else
        echo "ERROR: cannot generate UUID (no /proc/sys/kernel/random/uuid, no uuidgen)" >&2
        exit 1
    fi
}
DECLARATION_ID=$(gen_uuid)
ENTITY_ID=$(gen_uuid)
PERSON_ID=$(gen_uuid)
NONCE_HEX=$(openssl rand -hex 16)

# Canonical payload (matches the server's canonicalisation order).
CANONICAL=$(jq -c -n \
    --arg entity_id "$ENTITY_ID" \
    --arg principal "$PRINCIPAL" \
    --arg person_id "$PERSON_ID" \
    --arg nonce_hex "$NONCE_HEX" \
    '{
      entity_id: $entity_id,
      declarant_principal: $principal,
      declarant_role: "self",
      kind: "incorporation",
      effective_from: "2026-01-01",
      beneficial_owners: [{
        person_id: $person_id,
        ownership_basis_points: 10000,
        interest_kind: "equity"
      }],
      nonce_hex: $nonce_hex
    }')

# Sign with the Ed25519 key.
echo -n "$CANONICAL" > "$KEYDIR/payload"
SIG_HEX=$(openssl pkeyutl -sign -inkey "$KEYDIR/sk.pem" -rawin -in "$KEYDIR/payload" \
          | xxd -p -c 128)

REQ=$(jq -c -n \
    --arg decl_id "$DECLARATION_ID" \
    --arg entity_id "$ENTITY_ID" \
    --arg person_id "$PERSON_ID" \
    --arg principal "$PRINCIPAL" \
    --arg sig_hex "$SIG_HEX" \
    --arg pk_hex "$PK_HEX" \
    --arg nonce_hex "$NONCE_HEX" \
    '{
      declaration_id: $decl_id,
      entity_id: $entity_id,
      declarant_role: "self",
      kind: "incorporation",
      effective_from: "2026-01-01",
      beneficial_owners: [{
        person_id: $person_id,
        ownership_basis_points: 10000,
        interest_kind: "equity"
      }],
      attestation: {
        signed_by: $principal,
        signature_algorithm: "ed25519",
        signature_hex: $sig_hex,
        public_key_hex: $pk_hex,
        nonce_hex: $nonce_hex
      }
    }')

echo ""
echo "── POST /v1/declarations ──"
RESP=$(curl -sS -X POST http://127.0.0.1:8080/v1/declarations \
    -H "Content-Type: application/json" \
    -H "X-Recor-Dev-Principal: $PRINCIPAL" \
    -d "$REQ" \
    -w "\n%{http_code}")
HTTP=$(echo "$RESP" | tail -1)
BODY=$(echo "$RESP" | sed '$d')
echo "$BODY" | jq .
echo "HTTP $HTTP"
[ "$HTTP" = "201" ] || { echo "FAIL: expected 201"; exit 1; }

echo ""
echo "── GET /v1/declarations/$DECLARATION_ID ──"
RESP=$(curl -sS http://127.0.0.1:8080/v1/declarations/$DECLARATION_ID \
    -H "X-Recor-Dev-Principal: $PRINCIPAL" \
    -w "\n%{http_code}")
HTTP=$(echo "$RESP" | tail -1)
BODY=$(echo "$RESP" | sed '$d')
echo "$BODY" | jq .
echo "HTTP $HTTP"
[ "$HTTP" = "200" ] || { echo "FAIL: expected 200"; exit 1; }

echo ""
echo "✅ smoke pass"
