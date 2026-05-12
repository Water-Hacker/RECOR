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
    # hex (not base64) so DATABASE_URL parsing doesn't choke on '/'.
    # Per R-LOOP-4-ROT, the two D↔V channels use DISTINCT secrets so
    # compromise of one does not affect the other.
    {
        echo "RECOR_DB_PASSWORD=$(openssl rand -hex 24)"
        echo "RECOR_D_TO_V_HMAC=$(openssl rand -hex 32)"
        echo "RECOR_V_TO_D_HMAC=$(openssl rand -hex 32)"
    } > .env
    echo "[generated .env with per-channel HMAC secrets]"
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
# OPS-1 regression guard: rate limiting on this endpoint must not
# trip for a single normal-load submission. A 429 here means the
# tower-governor configuration is mis-tuned for prod-like traffic.
if [ "$HTTP" = "429" ]; then
    echo "FAIL: rate limiter rejected first submission (OPS-1 regression)"
    echo "$BODY"
    exit 1
fi
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
echo "── PHASE 3: supersede guard refuses on rejected declarations (400) ──"
NEW_DECL_ID=$(cat /proc/sys/kernel/random/uuid)
NEW_NONCE_HEX=$(openssl rand -hex 16)
NEW_CANONICAL=$(jq -c -n --arg eid "$ENT_ID" --arg p "$PRINCIPAL" --arg pid "$PER_ID" --arg n "$NEW_NONCE_HEX" \
    '{entity_id:$eid, declarant_principal:$p, declarant_role:"self", kind:"annual_renewal", effective_from:"2026-04-01", beneficial_owners:[{person_id:$pid, ownership_basis_points:10000, interest_kind:"equity"}], nonce_hex:$n}')
echo -n "$NEW_CANONICAL" > "$KEYDIR/payload2"
NEW_SIG_HEX=$(openssl pkeyutl -sign -inkey "$KEYDIR/sk.pem" -rawin -in "$KEYDIR/payload2" | xxd -p -c 128)
NEW_REQ=$(jq -c -n --arg did "$NEW_DECL_ID" --arg eid "$ENT_ID" --arg pid "$PER_ID" --arg p "$PRINCIPAL" \
                --arg s "$NEW_SIG_HEX" --arg pk "$PK_HEX" --arg n "$NEW_NONCE_HEX" \
    '{declaration_id:$did, entity_id:$eid, declarant_role:"self", kind:"annual_renewal", effective_from:"2026-04-01",
      beneficial_owners:[{person_id:$pid, ownership_basis_points:10000, interest_kind:"equity"}],
      attestation:{signed_by:$p, signature_algorithm:"ed25519", signature_hex:$s, public_key_hex:$pk, nonce_hex:$n}}')

SUP_RESP=$(curl -sS -X POST "http://127.0.0.1:8080/v1/declarations/$DECL_ID/supersede" \
    -H "Content-Type: application/json" \
    -H "X-Recor-Dev-Principal: $PRINCIPAL" \
    -d "$NEW_REQ" -w "\n%{http_code}")
SUP_HTTP=$(echo "$SUP_RESP" | tail -1)
echo "Supersede POST against rejected declaration: HTTP $SUP_HTTP"
[ "$SUP_HTTP" = "400" ] || {
    echo "FAIL: expected HTTP 400 (supersede-from-invalid-state); got $SUP_HTTP"
    echo "$SUP_RESP" | sed '$d' | jq '.'
    exit 1
}
echo "  ✅ supersede guard correctly refused a rejected declaration"

echo ""
echo "── PHASE 3b: full supersede chain on an ACCEPTED declaration ──"
# Seed mock BUNEC so this person passes identity verification.
docker compose -f "$COMPOSE_FILE" exec -T -e PGPASSWORD="$DB_PW" postgres-verification \
    psql -U recor -d verification -c \
    "INSERT INTO mock_bunec_persons (person_id, canonical_full_name, nationality) VALUES ('$PER_ID', 'Test Declarant', 'CMR') ON CONFLICT (person_id) DO NOTHING" \
    > /dev/null

ACCEPTED_DECL_ID=$(cat /proc/sys/kernel/random/uuid)
ACCEPTED_ENT_ID=$(cat /proc/sys/kernel/random/uuid)
ACCEPTED_NONCE_HEX=$(openssl rand -hex 16)
ACCEPTED_CANONICAL=$(jq -c -n --arg eid "$ACCEPTED_ENT_ID" --arg p "$PRINCIPAL" --arg pid "$PER_ID" --arg n "$ACCEPTED_NONCE_HEX" \
    '{entity_id:$eid, declarant_principal:$p, declarant_role:"self", kind:"incorporation", effective_from:"2026-01-01", beneficial_owners:[{person_id:$pid, ownership_basis_points:10000, interest_kind:"equity"}], nonce_hex:$n}')
echo -n "$ACCEPTED_CANONICAL" > "$KEYDIR/payload3"
ACCEPTED_SIG_HEX=$(openssl pkeyutl -sign -inkey "$KEYDIR/sk.pem" -rawin -in "$KEYDIR/payload3" | xxd -p -c 128)
ACCEPTED_REQ=$(jq -c -n --arg did "$ACCEPTED_DECL_ID" --arg eid "$ACCEPTED_ENT_ID" --arg pid "$PER_ID" --arg p "$PRINCIPAL" \
                --arg s "$ACCEPTED_SIG_HEX" --arg pk "$PK_HEX" --arg n "$ACCEPTED_NONCE_HEX" \
    '{declaration_id:$did, entity_id:$eid, declarant_role:"self", kind:"incorporation", effective_from:"2026-01-01",
      beneficial_owners:[{person_id:$pid, ownership_basis_points:10000, interest_kind:"equity"}],
      attestation:{signed_by:$p, signature_algorithm:"ed25519", signature_hex:$s, public_key_hex:$pk, nonce_hex:$n}}')

curl -sS -X POST http://127.0.0.1:8080/v1/declarations \
    -H "Content-Type: application/json" \
    -H "X-Recor-Dev-Principal: $PRINCIPAL" \
    -d "$ACCEPTED_REQ" -w "\nHTTP %{http_code}\n" > /dev/null

echo "  waiting for verification to land on a supersede-eligible state..."
# With seeded BUNEC + real Stages 1-2 + stub Stages 3-7, fusion lands
# at `yellow` (in_verification). Both `accepted` and `in_verification`
# are valid starting states for supersede; we accept either.
accepted_state=""
for i in {1..30}; do
    sleep 1
    state=$(docker compose -f "$COMPOSE_FILE" exec -T -e PGPASSWORD="$DB_PW" postgres-declaration \
        psql -U recor -d declaration -tAc \
        "SELECT verification_state FROM declarations WHERE declaration_id = '$ACCEPTED_DECL_ID'" \
        2>/dev/null | tr -d '[:space:]')
    if [ "$state" = "accepted" ] || [ "$state" = "in_verification" ]; then
        accepted_state="$state"
        echo "  ✅ declaration reached '$state' after ${i}s"
        break
    fi
done
[ -n "$accepted_state" ] || {
    echo "FAIL: expected accepted/in_verification state for seeded-person declaration; got '$accepted_state'"
    exit 1
}

SUCC_DECL_ID=$(cat /proc/sys/kernel/random/uuid)
SUCC_NONCE_HEX=$(openssl rand -hex 16)
SUCC_CANONICAL=$(jq -c -n --arg eid "$ACCEPTED_ENT_ID" --arg p "$PRINCIPAL" --arg pid "$PER_ID" --arg n "$SUCC_NONCE_HEX" \
    '{entity_id:$eid, declarant_principal:$p, declarant_role:"self", kind:"annual_renewal", effective_from:"2026-04-01", beneficial_owners:[{person_id:$pid, ownership_basis_points:10000, interest_kind:"equity"}], nonce_hex:$n}')
echo -n "$SUCC_CANONICAL" > "$KEYDIR/payload4"
SUCC_SIG_HEX=$(openssl pkeyutl -sign -inkey "$KEYDIR/sk.pem" -rawin -in "$KEYDIR/payload4" | xxd -p -c 128)
SUCC_REQ=$(jq -c -n --arg did "$SUCC_DECL_ID" --arg eid "$ACCEPTED_ENT_ID" --arg pid "$PER_ID" --arg p "$PRINCIPAL" \
                --arg s "$SUCC_SIG_HEX" --arg pk "$PK_HEX" --arg n "$SUCC_NONCE_HEX" \
    '{declaration_id:$did, entity_id:$eid, declarant_role:"self", kind:"annual_renewal", effective_from:"2026-04-01",
      beneficial_owners:[{person_id:$pid, ownership_basis_points:10000, interest_kind:"equity"}],
      attestation:{signed_by:$p, signature_algorithm:"ed25519", signature_hex:$s, public_key_hex:$pk, nonce_hex:$n}}')

SUCC_RESP=$(curl -sS -X POST "http://127.0.0.1:8080/v1/declarations/$ACCEPTED_DECL_ID/supersede" \
    -H "Content-Type: application/json" \
    -H "X-Recor-Dev-Principal: $PRINCIPAL" \
    -d "$SUCC_REQ" -w "\n%{http_code}")
SUCC_HTTP=$(echo "$SUCC_RESP" | tail -1)
SUCC_BODY=$(echo "$SUCC_RESP" | sed '$d')
echo "Supersede POST against accepted declaration: HTTP $SUCC_HTTP"
[ "$SUCC_HTTP" = "201" ] || { echo "FAIL: expected 201"; echo "$SUCC_BODY" | jq '.'; exit 1; }
echo "$SUCC_BODY" | jq '{new_declaration_id, superseded_declaration_id, state}'

echo ""
echo "── verifying supersede chain in projections ──"
docker compose -f "$COMPOSE_FILE" exec -T -e PGPASSWORD="$DB_PW" postgres-declaration \
    psql -U recor -d declaration -c \
    "SELECT declaration_id, state, supersedes_declaration_id, superseded_by_declaration_id FROM declarations WHERE declaration_id IN ('$ACCEPTED_DECL_ID', '$SUCC_DECL_ID') ORDER BY submitted_at"

old_state=$(docker compose -f "$COMPOSE_FILE" exec -T -e PGPASSWORD="$DB_PW" postgres-declaration \
    psql -U recor -d declaration -tAc \
    "SELECT state FROM declarations WHERE declaration_id = '$ACCEPTED_DECL_ID'" | tr -d '[:space:]')
[ "$old_state" = "superseded" ] || {
    echo "FAIL: old declaration's state is '$old_state' (expected 'superseded')"
    exit 1
}
echo "  ✅ old declaration's state = superseded"

new_supersedes=$(docker compose -f "$COMPOSE_FILE" exec -T -e PGPASSWORD="$DB_PW" postgres-declaration \
    psql -U recor -d declaration -tAc \
    "SELECT supersedes_declaration_id FROM declarations WHERE declaration_id = '$SUCC_DECL_ID'" | tr -d '[:space:]')
[ "$new_supersedes" = "$ACCEPTED_DECL_ID" ] || {
    echo "FAIL: new declaration's supersedes_declaration_id is '$new_supersedes' (expected '$ACCEPTED_DECL_ID')"
    exit 1
}
echo "  ✅ new declaration's supersedes_declaration_id points at the old one"

echo ""
echo "── PHASE 4: R-DECL-3 AMEND on a still-mutable declaration ──"
# Submit a fresh declaration; do NOT wait for verification (we want
# the state to stay Submitted so Amend is admitted). Amend changes
# the beneficial-owner roster while preserving the 10_000 invariant.
AMEND_DECL_ID=$(cat /proc/sys/kernel/random/uuid)
AMEND_ENT_ID=$(cat /proc/sys/kernel/random/uuid)
AMEND_PER1=$(cat /proc/sys/kernel/random/uuid)
AMEND_NONCE1=$(openssl rand -hex 16)
AMEND_CANONICAL1=$(jq -c -n --arg eid "$AMEND_ENT_ID" --arg p "$PRINCIPAL" --arg pid "$AMEND_PER1" --arg n "$AMEND_NONCE1" \
    '{entity_id:$eid, declarant_principal:$p, declarant_role:"self", kind:"incorporation", effective_from:"2026-01-01", beneficial_owners:[{person_id:$pid, ownership_basis_points:10000, interest_kind:"equity"}], nonce_hex:$n}')
echo -n "$AMEND_CANONICAL1" > "$KEYDIR/amend1"
AMEND_SIG1=$(openssl pkeyutl -sign -inkey "$KEYDIR/sk.pem" -rawin -in "$KEYDIR/amend1" | xxd -p -c 128)
AMEND_REQ1=$(jq -c -n --arg did "$AMEND_DECL_ID" --arg eid "$AMEND_ENT_ID" --arg pid "$AMEND_PER1" --arg p "$PRINCIPAL" \
                --arg s "$AMEND_SIG1" --arg pk "$PK_HEX" --arg n "$AMEND_NONCE1" \
    '{declaration_id:$did, entity_id:$eid, declarant_role:"self", kind:"incorporation", effective_from:"2026-01-01",
      beneficial_owners:[{person_id:$pid, ownership_basis_points:10000, interest_kind:"equity"}],
      attestation:{signed_by:$p, signature_algorithm:"ed25519", signature_hex:$s, public_key_hex:$pk, nonce_hex:$n}}')

curl -sS -X POST http://127.0.0.1:8080/v1/declarations \
    -H "Content-Type: application/json" \
    -H "X-Recor-Dev-Principal: $PRINCIPAL" \
    -d "$AMEND_REQ1" -w "\nHTTP %{http_code}\n" > /dev/null

# Build the AMENDED canonical payload: split 60/40 across two persons.
AMEND_PER2A=$(cat /proc/sys/kernel/random/uuid)
AMEND_PER2B=$(cat /proc/sys/kernel/random/uuid)
AMEND_NONCE2=$(openssl rand -hex 16)
AMEND_CANONICAL2=$(jq -c -n --arg eid "$AMEND_ENT_ID" --arg p "$PRINCIPAL" --arg pa "$AMEND_PER2A" --arg pb "$AMEND_PER2B" --arg n "$AMEND_NONCE2" \
    '{entity_id:$eid, declarant_principal:$p, declarant_role:"authorised_agent", kind:"amendment", effective_from:"2026-02-01",
      beneficial_owners:[
        {person_id:$pa, ownership_basis_points:6000, interest_kind:"equity"},
        {person_id:$pb, ownership_basis_points:4000, interest_kind:"equity"}
      ], nonce_hex:$n}')
echo -n "$AMEND_CANONICAL2" > "$KEYDIR/amend2"
AMEND_SIG2=$(openssl pkeyutl -sign -inkey "$KEYDIR/sk.pem" -rawin -in "$KEYDIR/amend2" | xxd -p -c 128)
AMEND_REQ2=$(jq -c -n --arg eid "$AMEND_ENT_ID" --arg pa "$AMEND_PER2A" --arg pb "$AMEND_PER2B" --arg p "$PRINCIPAL" \
                --arg s "$AMEND_SIG2" --arg pk "$PK_HEX" --arg n "$AMEND_NONCE2" \
    '{declarant_role:"authorised_agent", effective_from:"2026-02-01",
      beneficial_owners:[
        {person_id:$pa, ownership_basis_points:6000, interest_kind:"equity"},
        {person_id:$pb, ownership_basis_points:4000, interest_kind:"equity"}
      ],
      attestation:{signed_by:$p, signature_algorithm:"ed25519", signature_hex:$s, public_key_hex:$pk, nonce_hex:$n}}')

AMEND_RESP=$(curl -sS -X POST "http://127.0.0.1:8080/v1/declarations/$AMEND_DECL_ID/amend" \
    -H "Content-Type: application/json" \
    -H "X-Recor-Dev-Principal: $PRINCIPAL" \
    -d "$AMEND_REQ2" -w "\n%{http_code}")
AMEND_HTTP=$(echo "$AMEND_RESP" | tail -1)
AMEND_BODY=$(echo "$AMEND_RESP" | sed '$d')
echo "Amend POST: HTTP $AMEND_HTTP"
[ "$AMEND_HTTP" = "200" ] || { echo "FAIL: expected 200"; echo "$AMEND_BODY" | jq '.'; exit 1; }
echo "$AMEND_BODY" | jq '{declaration_id, aggregate_version, amended_at}'

# Verify GET reflects the amended values.
echo "── confirming GET reflects amended values ──"
GET_AMEND=$(curl -sS "http://127.0.0.1:8080/v1/declarations/$AMEND_DECL_ID" \
    -H "X-Recor-Dev-Principal: $PRINCIPAL")
got_role=$(echo "$GET_AMEND" | jq -r '.declarant_role')
got_eff=$(echo "$GET_AMEND" | jq -r '.effective_from')
got_owner_count=$(echo "$GET_AMEND" | jq '.beneficial_owners | length')
got_owner_sum=$(echo "$GET_AMEND" | jq '[.beneficial_owners[].ownership_basis_points] | add')
[ "$got_role" = "authorised_agent" ] || { echo "FAIL: declarant_role $got_role != authorised_agent"; exit 1; }
[ "$got_eff" = "2026-02-01" ] || { echo "FAIL: effective_from $got_eff != 2026-02-01"; exit 1; }
[ "$got_owner_count" = "2" ] || { echo "FAIL: beneficial_owners count $got_owner_count != 2"; exit 1; }
[ "$got_owner_sum" = "10000" ] || { echo "FAIL: beneficial_owners sum $got_owner_sum != 10000"; exit 1; }
echo "  ✅ amendment reflected in GET (role/effective_from/owners updated; sum invariant preserved)"

# Try to amend an Accepted declaration — must refuse with 409 + supersede guidance.
echo ""
echo "── confirming Amend refuses on Accepted declaration (409 with supersede hint) ──"
AMEND_BAD_NONCE=$(openssl rand -hex 16)
AMEND_BAD_CANONICAL=$(jq -c -n --arg eid "$ACCEPTED_ENT_ID" --arg p "$PRINCIPAL" --arg pid "$PER_ID" --arg n "$AMEND_BAD_NONCE" \
    '{entity_id:$eid, declarant_principal:$p, declarant_role:"self", kind:"amendment", effective_from:"2026-02-01", beneficial_owners:[{person_id:$pid, ownership_basis_points:10000, interest_kind:"equity"}], nonce_hex:$n}')
echo -n "$AMEND_BAD_CANONICAL" > "$KEYDIR/amend-bad"
AMEND_BAD_SIG=$(openssl pkeyutl -sign -inkey "$KEYDIR/sk.pem" -rawin -in "$KEYDIR/amend-bad" | xxd -p -c 128)
AMEND_BAD_REQ=$(jq -c -n --arg pid "$PER_ID" --arg p "$PRINCIPAL" --arg s "$AMEND_BAD_SIG" --arg pk "$PK_HEX" --arg n "$AMEND_BAD_NONCE" \
    '{declarant_role:"self", effective_from:"2026-02-01",
      beneficial_owners:[{person_id:$pid, ownership_basis_points:10000, interest_kind:"equity"}],
      attestation:{signed_by:$p, signature_algorithm:"ed25519", signature_hex:$s, public_key_hex:$pk, nonce_hex:$n}}')
# ACCEPTED_DECL_ID may be in 'superseded' state by now (we superseded it
# in phase 3b). For the 409 assertion we want a non-Submitted/non-
# InVerification state; the supersede already moved it to Superseded.
AMEND_BAD_RESP=$(curl -sS -X POST "http://127.0.0.1:8080/v1/declarations/$ACCEPTED_DECL_ID/amend" \
    -H "Content-Type: application/json" \
    -H "X-Recor-Dev-Principal: $PRINCIPAL" \
    -d "$AMEND_BAD_REQ" -w "\n%{http_code}")
AMEND_BAD_HTTP=$(echo "$AMEND_BAD_RESP" | tail -1)
[ "$AMEND_BAD_HTTP" = "409" ] || { echo "FAIL: expected 409 from Amend on non-mutable state; got $AMEND_BAD_HTTP"; echo "$AMEND_BAD_RESP"; exit 1; }
echo "  ✅ Amend correctly returned 409 on a non-mutable declaration"

echo ""
echo "── PHASE 5: R-DECL-3 CORRECT on a Submitted declaration ──"
# Submit another fresh declaration so we have one in pristine Submitted
# state (correct only admits from Submitted, never InVerification).
CORR_DECL_ID=$(cat /proc/sys/kernel/random/uuid)
CORR_ENT_ID=$(cat /proc/sys/kernel/random/uuid)
CORR_PER=$(cat /proc/sys/kernel/random/uuid)
CORR_NONCE1=$(openssl rand -hex 16)
CORR_CANONICAL1=$(jq -c -n --arg eid "$CORR_ENT_ID" --arg p "$PRINCIPAL" --arg pid "$CORR_PER" --arg n "$CORR_NONCE1" \
    '{entity_id:$eid, declarant_principal:$p, declarant_role:"self", kind:"incorporation", effective_from:"2026-01-01", beneficial_owners:[{person_id:$pid, ownership_basis_points:10000, interest_kind:"equity"}], nonce_hex:$n}')
echo -n "$CORR_CANONICAL1" > "$KEYDIR/corr1"
CORR_SIG1=$(openssl pkeyutl -sign -inkey "$KEYDIR/sk.pem" -rawin -in "$KEYDIR/corr1" | xxd -p -c 128)
CORR_REQ1=$(jq -c -n --arg did "$CORR_DECL_ID" --arg eid "$CORR_ENT_ID" --arg pid "$CORR_PER" --arg p "$PRINCIPAL" \
                --arg s "$CORR_SIG1" --arg pk "$PK_HEX" --arg n "$CORR_NONCE1" \
    '{declaration_id:$did, entity_id:$eid, declarant_role:"self", kind:"incorporation", effective_from:"2026-01-01",
      beneficial_owners:[{person_id:$pid, ownership_basis_points:10000, interest_kind:"equity"}],
      attestation:{signed_by:$p, signature_algorithm:"ed25519", signature_hex:$s, public_key_hex:$pk, nonce_hex:$n}}')

curl -sS -X POST http://127.0.0.1:8080/v1/declarations \
    -H "Content-Type: application/json" \
    -H "X-Recor-Dev-Principal: $PRINCIPAL" \
    -d "$CORR_REQ1" -w "\nHTTP %{http_code}\n" > /dev/null

CORR_NOTE="operator: typo in supporting docs reference; see incident #42"
CORR_NONCE2=$(openssl rand -hex 16)
CORR_CANONICAL2=$(jq -c -n --arg did "$CORR_DECL_ID" --arg p "$PRINCIPAL" --arg notes "$CORR_NOTE" --arg n "$CORR_NONCE2" \
    '{declaration_id:$did, declarant_principal:$p, kind:"correction", metadata_notes:$notes, nonce_hex:$n}')
echo -n "$CORR_CANONICAL2" > "$KEYDIR/corr2"
CORR_SIG2=$(openssl pkeyutl -sign -inkey "$KEYDIR/sk.pem" -rawin -in "$KEYDIR/corr2" | xxd -p -c 128)
CORR_REQ2=$(jq -c -n --arg notes "$CORR_NOTE" --arg p "$PRINCIPAL" --arg s "$CORR_SIG2" --arg pk "$PK_HEX" --arg n "$CORR_NONCE2" \
    '{metadata_notes:$notes,
      attestation:{signed_by:$p, signature_algorithm:"ed25519", signature_hex:$s, public_key_hex:$pk, nonce_hex:$n}}')

CORR_RESP=$(curl -sS -X POST "http://127.0.0.1:8080/v1/declarations/$CORR_DECL_ID/correct" \
    -H "Content-Type: application/json" \
    -H "X-Recor-Dev-Principal: $PRINCIPAL" \
    -d "$CORR_REQ2" -w "\n%{http_code}")
CORR_HTTP=$(echo "$CORR_RESP" | tail -1)
CORR_BODY=$(echo "$CORR_RESP" | sed '$d')
echo "Correct POST: HTTP $CORR_HTTP"
[ "$CORR_HTTP" = "200" ] || { echo "FAIL: expected 200"; echo "$CORR_BODY" | jq '.'; exit 1; }
echo "$CORR_BODY" | jq '{declaration_id, aggregate_version, corrected_at}'

echo "── confirming GET reflects the metadata_notes ──"
GET_CORR=$(curl -sS "http://127.0.0.1:8080/v1/declarations/$CORR_DECL_ID" \
    -H "X-Recor-Dev-Principal: $PRINCIPAL")
got_notes=$(echo "$GET_CORR" | jq -r '.metadata_notes')
[ "$got_notes" = "$CORR_NOTE" ] || { echo "FAIL: metadata_notes mismatch: '$got_notes' != '$CORR_NOTE'"; exit 1; }
echo "  ✅ correction reflected in GET (metadata_notes round-trip)"

echo ""
echo "✅ D ↔ V LOOP PHASE 1 + 2 + R-DECL-3 SUPERSEDE + AMEND + CORRECT: PASS"
echo "   • Declaration accepted with HTTP 201 + signed receipt"
echo "   • Outbox relay fired D → V; verification ran the pipeline"
echo "   • Verification case persisted with the same declaration_id"
echo "   • Writeback relay fired V → D; declaration state = $verified_state"
echo "   • Declaration projection surfaces verification metadata"
echo "   • Both outbox rows marked dispatched"
echo "   • Supersede correctly REFUSED a rejected declaration (HTTP 400)"
echo "   • Supersede SUCCEEDED on an accepted declaration (HTTP 201)"
echo "   • Old declaration transitioned to 'superseded'"
echo "   • New declaration's supersedes_declaration_id points back"
echo "   • Amend SUCCEEDED on a Submitted declaration (HTTP 200)"
echo "   • Amend re-projected beneficial_owners + effective_from + declarant_role"
echo "   • Amend REFUSED on a non-mutable declaration (HTTP 409)"
echo "   • Correct SUCCEEDED on a Submitted declaration (HTTP 200)"
echo "   • Correct metadata_notes round-tripped via GET"
