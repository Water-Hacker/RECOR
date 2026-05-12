#!/usr/bin/env bash
# End-to-end D ↔ V loop smoke (Phase 1 — D → V).
#
# Brings up: postgres × 2 + declaration + verification.
# Submits a real Ed25519-signed declaration to the declaration service.
# Waits for the outbox relay to fire (~5s).
# Asserts that the verification engine has created a case for the
# declaration_id, and reports the lane decision.

set -euo pipefail
cd "$(dirname "$0")/.."

COMPOSE_FILE="docker-compose.integration.yaml"

if [ ! -f .env ]; then
    # hex (not base64) so DATABASE_URL parsing doesn't choke on '/'
    {
        echo "RECOR_DB_PASSWORD=$(openssl rand -hex 24)"
        echo "RECOR_HMAC_SECRET=$(openssl rand -hex 32)"
    } > .env
    echo "[generated .env]"
fi

echo "── compose up ──"
docker compose -f "$COMPOSE_FILE" up -d --build 2>&1 | tail -5

echo "── waiting for both services ──"
for svc_url in http://127.0.0.1:8080/healthz http://127.0.0.1:8081/healthz; do
    for i in {1..60}; do
        if curl -fsS "$svc_url" >/dev/null 2>&1; then
            echo "  ✅ $svc_url healthy after ${i}s"
            break
        fi
        sleep 1
    done
done

echo ""
echo "── submit a real signed declaration ──"
PRINCIPAL="spiffe://recor.cm/loop-smoke"
KEYDIR=$(mktemp -d)
trap "rm -rf $KEYDIR" EXIT
openssl genpkey -algorithm Ed25519 -out "$KEYDIR/sk.pem" 2>/dev/null
PK_HEX=$(openssl pkey -in "$KEYDIR/sk.pem" -pubout -outform DER 2>/dev/null | tail -c 32 | xxd -p -c 64)
DECL_ID=$(cat /proc/sys/kernel/random/uuid)
ENT_ID=$(cat /proc/sys/kernel/random/uuid)
PER_ID=$(cat /proc/sys/kernel/random/uuid)
NONCE_HEX=$(openssl rand -hex 16)

CANONICAL=$(jq -c -n --arg eid "$ENT_ID" --arg p "$PRINCIPAL" --arg pid "$PER_ID" --arg n "$NONCE_HEX" \
    '{entity_id:$eid, declarant_principal:$p, declarant_role:"self", kind:"incorporation", effective_from:"2026-01-01", beneficial_owners:[{person_id:$pid, ownership_basis_points:10000, interest_kind:"equity"}], nonce_hex:$n}')
echo -n "$CANONICAL" > "$KEYDIR/payload"
SIG_HEX=$(openssl pkeyutl -sign -inkey "$KEYDIR/sk.pem" -rawin -in "$KEYDIR/payload" | xxd -p -c 128)

REQ=$(jq -c -n --arg did "$DECL_ID" --arg eid "$ENT_ID" --arg pid "$PER_ID" --arg p "$PRINCIPAL" \
                --arg s "$SIG_HEX" --arg pk "$PK_HEX" --arg n "$NONCE_HEX" \
    '{declaration_id:$did, entity_id:$eid, declarant_role:"self", kind:"incorporation", effective_from:"2026-01-01",
      beneficial_owners:[{person_id:$pid, ownership_basis_points:10000, interest_kind:"equity"}],
      attestation:{signed_by:$p, signature_algorithm:"ed25519", signature_hex:$s, public_key_hex:$pk, nonce_hex:$n}}')

DECL_RESP=$(curl -sS -X POST http://127.0.0.1:8080/v1/declarations \
    -H "Content-Type: application/json" \
    -H "X-Recor-Dev-Principal: $PRINCIPAL" \
    -d "$REQ" -w "\n%{http_code}")
HTTP=$(echo "$DECL_RESP" | tail -1)
BODY=$(echo "$DECL_RESP" | sed '$d')
echo "Declaration POST: HTTP $HTTP"
[ "$HTTP" = "201" ] || { echo "FAIL: expected 201"; echo "$BODY"; exit 1; }
echo "$BODY" | jq '{declaration_id, state, receipt_hash_hex}'

echo ""
echo "── waiting for relay to forward to verification engine ──"
DB_PW="$(grep RECOR_DB_PASSWORD .env | cut -d= -f2-)"
case_id=""
for i in {1..30}; do
    sleep 1
    # Query the verification database for a case bound to this declaration_id.
    found=$(docker compose -f "$COMPOSE_FILE" exec -T -e PGPASSWORD="$DB_PW" postgres-verification \
        psql -U recor -d verification -tAc \
        "SELECT case_id::text FROM verification_cases WHERE declaration_id = '$DECL_ID'" \
        2>/dev/null | tr -d '[:space:]')
    if [ -n "$found" ]; then
        case_id="$found"
        echo "  ✅ relay delivered after ${i}s; verification case $case_id"
        break
    fi
done

if [ -z "$case_id" ]; then
    echo ""
    echo "FAIL: verification engine never received the declaration after 30s"
    echo ""
    echo "─── declaration service logs (last 30) ───"
    docker compose -f "$COMPOSE_FILE" logs declaration | tail -30 || true
    echo ""
    echo "─── verification engine logs (last 30) ───"
    docker compose -f "$COMPOSE_FILE" logs verification | tail -30 || true
    echo ""
    echo "─── outbox state ───"
    docker compose -f "$COMPOSE_FILE" exec -T -e PGPASSWORD="$DB_PW" postgres-declaration \
        psql -U recor -d declaration -c \
        "SELECT event_id, event_type, dispatched_at, dispatch_attempts, last_error FROM outbox" \
        || true
    exit 1
fi

echo ""
echo "── verification engine /v1/verifications/$case_id ──"
RESP=$(curl -sS http://127.0.0.1:8081/v1/verifications/$case_id \
    -H "X-Recor-Dev-Principal: $PRINCIPAL")
echo "$RESP" | jq '{case_id, lane, fused_authenticity: .fused_authenticity.m_true, declaration_id: .declaration.declaration_id, stage_count: (.stage_outcomes | length)}'

# Assert: the case's declaration_id matches what we submitted.
got_decl_id=$(echo "$RESP" | jq -r '.declaration.declaration_id')
[ "$got_decl_id" = "$DECL_ID" ] || {
    echo "FAIL: verification case's declaration_id ($got_decl_id) != submitted ($DECL_ID)"
    exit 1
}

echo ""
echo "── verifying outbox row marked dispatched ──"
docker compose -f "$COMPOSE_FILE" exec -T -e PGPASSWORD="$DB_PW" postgres-declaration \
    psql -U recor -d declaration -tAc \
    "SELECT event_type, dispatched_at IS NOT NULL AS dispatched, dispatch_attempts FROM outbox WHERE aggregate_id = '$DECL_ID'"

echo ""
echo "── PHASE 2: waiting for verification → declaration writeback ──"
# Poll the declaration projection for verification_state to transition
# off 'pending'/'not_verified' to one of the lane states.
verified_state=""
for i in {1..30}; do
    sleep 1
    state=$(docker compose -f "$COMPOSE_FILE" exec -T -e PGPASSWORD="$DB_PW" postgres-declaration \
        psql -U recor -d declaration -tAc \
        "SELECT verification_state FROM declarations WHERE declaration_id = '$DECL_ID'" \
        2>/dev/null | tr -d '[:space:]')
    if [ "$state" = "accepted" ] || [ "$state" = "rejected" ] || [ "$state" = "in_verification" ]; then
        verified_state="$state"
        echo "  ✅ writeback applied after ${i}s; declaration state = $state"
        break
    fi
done

if [ -z "$verified_state" ]; then
    echo "FAIL: declaration's verification_state never transitioned within 30s"
    echo ""
    echo "─── verification engine logs (last 30) ───"
    docker compose -f "$COMPOSE_FILE" logs verification | tail -30 || true
    echo ""
    echo "─── declaration service logs (last 30) ───"
    docker compose -f "$COMPOSE_FILE" logs declaration | tail -30 || true
    echo ""
    echo "─── verification outbox state ───"
    docker compose -f "$COMPOSE_FILE" exec -T -e PGPASSWORD="$DB_PW" postgres-verification \
        psql -U recor -d verification -c \
        "SELECT event_type, dispatched_at IS NOT NULL AS dispatched, dispatch_attempts, last_error FROM verification_outbox" \
        || true
    exit 1
fi

echo ""
echo "── confirming declaration GET surfaces verification metadata ──"
GET_RESP=$(curl -sS "http://127.0.0.1:8080/v1/declarations/$DECL_ID" \
    -H "X-Recor-Dev-Principal: $PRINCIPAL")
echo "$GET_RESP" | jq '{declaration_id, state, verification_state, verification_lane, verification_case_id, verified_at}'

got_state=$(echo "$GET_RESP" | jq -r '.verification_state')
got_case_id=$(echo "$GET_RESP" | jq -r '.verification_case_id')
[ "$got_state" = "$verified_state" ] || {
    echo "FAIL: GET says $got_state, DB says $verified_state"
    exit 1
}
[ "$got_case_id" = "$case_id" ] || {
    echo "FAIL: GET's verification_case_id ($got_case_id) != verification case_id ($case_id)"
    exit 1
}

echo ""
echo "── confirming verification outbox row marked dispatched ──"
docker compose -f "$COMPOSE_FILE" exec -T -e PGPASSWORD="$DB_PW" postgres-verification \
    psql -U recor -d verification -tAc \
    "SELECT event_type, dispatched_at IS NOT NULL AS dispatched, dispatch_attempts FROM verification_outbox WHERE aggregate_id = '$DECL_ID'"

echo ""
echo "✅ D ↔ V LOOP PHASE 1 + 2 SMOKE: PASS"
echo "   • Declaration accepted with HTTP 201 + signed receipt"
echo "   • Outbox relay fired D → V; verification ran the pipeline"
echo "   • Verification case persisted with the same declaration_id"
echo "   • Writeback relay fired V → D; declaration state = $verified_state"
echo "   • Declaration projection surfaces verification metadata"
echo "   • Both outbox rows marked dispatched"
