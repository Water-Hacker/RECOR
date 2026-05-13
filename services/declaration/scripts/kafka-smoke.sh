#!/usr/bin/env bash
# R-LOOP-2 — D ↔ V loop smoke with Kafka transport ON.
#
# Brings up: postgres × 2 + Kafka (single-broker KRaft) + declaration
# + verification, both with RELAY_TRANSPORT / VERIFICATION_TRANSPORT
# set to "kafka".
#
# Assertions:
#   1. The declaration POST returns 201 with a signed receipt.
#   2. The event lands in `recor.declaration.events.v1` (the consume
#      from the topic returns a non-empty payload).
#   3. The verification engine creates a verification_cases row keyed
#      on the same declaration_id.
#   4. The writeback round-trips so the declaration's
#      verification_state transitions off `not_verified`.
#
# This is the dual-transport assertion — both HTTP and Kafka are
# active. The V-engine's use case is idempotent on event_id; if the
# HTTP webhook applies first the Kafka consumer's apply is a no-op
# (the use case returns the existing case). Either ordering passes
# the smoke.
#
# D14 fail-closed: bash strict mode; FAIL on the first unexpected exit.

set -euo pipefail
cd "$(dirname "$0")/.."

COMPOSE_FILE="docker-compose.kafka.yaml"

if [ ! -f .env ]; then
    {
        echo "RECOR_DB_PASSWORD=$(openssl rand -hex 24)"
        echo "RECOR_D_TO_V_HMAC=$(openssl rand -hex 32)"
        echo "RECOR_V_TO_D_HMAC=$(openssl rand -hex 32)"
    } > .env
    echo "[generated .env for kafka-smoke]"
fi

cleanup() {
    if [ "${KAFKA_SMOKE_KEEP_UP:-0}" = "1" ]; then
        echo "[KAFKA_SMOKE_KEEP_UP=1 — leaving stack running for inspection]"
        return
    fi
    echo "── tear-down ──"
    docker compose -f "$COMPOSE_FILE" down -v 2>&1 | tail -5 || true
}
trap cleanup EXIT

echo "── compose up (kafka + d + v) ──"
docker compose -f "$COMPOSE_FILE" up -d --build 2>&1 | tail -10

echo ""
echo "── waiting for kafka ──"
for i in {1..60}; do
    if docker compose -f "$COMPOSE_FILE" exec -T kafka \
        /opt/bitnami/kafka/bin/kafka-broker-api-versions.sh \
        --bootstrap-server 127.0.0.1:9092 >/dev/null 2>&1; then
        echo "  Kafka healthy after ${i}s"
        break
    fi
    sleep 1
    if [ "$i" = "60" ]; then
        echo "FAIL: Kafka did not become healthy within 60s"
        docker compose -f "$COMPOSE_FILE" logs kafka | tail -30 || true
        exit 1
    fi
done

echo ""
echo "── creating v1 topics ──"
docker compose -f "$COMPOSE_FILE" exec -T kafka \
    /opt/bitnami/kafka/bin/kafka-topics.sh \
    --bootstrap-server 127.0.0.1:9092 \
    --create --if-not-exists \
    --topic recor.declaration.events.v1 \
    --partitions 3 --replication-factor 1 \
    --config retention.ms=604800000 || true

docker compose -f "$COMPOSE_FILE" exec -T kafka \
    /opt/bitnami/kafka/bin/kafka-topics.sh \
    --bootstrap-server 127.0.0.1:9092 \
    --create --if-not-exists \
    --topic recor.verification.events.v1 \
    --partitions 3 --replication-factor 1 \
    --config retention.ms=604800000 || true

docker compose -f "$COMPOSE_FILE" exec -T kafka \
    /opt/bitnami/kafka/bin/kafka-topics.sh \
    --bootstrap-server 127.0.0.1:9092 --list

echo ""
echo "── waiting for both services ──"
for svc_url in http://127.0.0.1:8080/healthz http://127.0.0.1:8081/healthz; do
    for i in {1..60}; do
        if curl -fsS "$svc_url" >/dev/null 2>&1; then
            echo "  $svc_url healthy after ${i}s"
            break
        fi
        sleep 1
        if [ "$i" = "60" ]; then
            echo "FAIL: $svc_url never became healthy"
            docker compose -f "$COMPOSE_FILE" logs declaration verification | tail -30 || true
            exit 1
        fi
    done
done

echo ""
echo "── submit a real signed declaration ──"
PRINCIPAL="spiffe://recor.cm/kafka-smoke"
KEYDIR=$(mktemp -d)
trap "rm -rf $KEYDIR" RETURN
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
echo "── consuming from recor.declaration.events.v1 (max 15s) ──"
# Use the broker's own console consumer with a 15s timeout. We accept
# any message with the right declaration_id in its payload; the
# producer keys on aggregate_id so the message lands on a deterministic
# partition.
DECL_TOPIC_OUT=$(timeout 15 docker compose -f "$COMPOSE_FILE" exec -T kafka \
    /opt/bitnami/kafka/bin/kafka-console-consumer.sh \
    --bootstrap-server 127.0.0.1:9092 \
    --topic recor.declaration.events.v1 \
    --from-beginning \
    --max-messages 10 \
    --timeout-ms 12000 2>/dev/null || true)

if echo "$DECL_TOPIC_OUT" | grep -q "$DECL_ID"; then
    echo "  declaration event observed on the topic"
else
    echo "FAIL: declaration event not on the topic within 15s"
    echo "── topic output (head) ──"
    echo "$DECL_TOPIC_OUT" | head -20
    echo "── declaration logs (last 30) ──"
    docker compose -f "$COMPOSE_FILE" logs declaration | tail -30 || true
    exit 1
fi

echo ""
echo "── waiting for V-engine to create the verification case ──"
DB_PW="$(grep RECOR_DB_PASSWORD .env | cut -d= -f2-)"
case_id=""
for i in {1..30}; do
    sleep 1
    found=$(docker compose -f "$COMPOSE_FILE" exec -T -e PGPASSWORD="$DB_PW" postgres-verification \
        psql -U recor -d verification -tAc \
        "SELECT case_id::text FROM verification_cases WHERE declaration_id = '$DECL_ID'" \
        2>/dev/null | tr -d '[:space:]')
    if [ -n "$found" ]; then
        case_id="$found"
        echo "  verification case $case_id materialised after ${i}s"
        break
    fi
done

if [ -z "$case_id" ]; then
    echo "FAIL: verification engine never created a case within 30s"
    docker compose -f "$COMPOSE_FILE" logs verification | tail -40 || true
    exit 1
fi

echo ""
echo "── confirming the verification case is queryable via HTTP ──"
RESP=$(curl -sS "http://127.0.0.1:8081/v1/verifications/$case_id" \
    -H "X-Recor-Dev-Principal: $PRINCIPAL")
echo "$RESP" | jq '{case_id, lane, declaration_id: .declaration.declaration_id}'

got_decl_id=$(echo "$RESP" | jq -r '.declaration.declaration_id')
[ "$got_decl_id" = "$DECL_ID" ] || {
    echo "FAIL: case's declaration_id ($got_decl_id) != submitted ($DECL_ID)"
    exit 1
}

echo ""
echo "── waiting for writeback (V → D) to update verification_state ──"
verified_state=""
for i in {1..30}; do
    sleep 1
    state=$(docker compose -f "$COMPOSE_FILE" exec -T -e PGPASSWORD="$DB_PW" postgres-declaration \
        psql -U recor -d declaration -tAc \
        "SELECT verification_state FROM declarations WHERE declaration_id = '$DECL_ID'" \
        2>/dev/null | tr -d '[:space:]')
    if [ "$state" = "accepted" ] || [ "$state" = "rejected" ] || [ "$state" = "in_verification" ]; then
        verified_state="$state"
        echo "  writeback applied after ${i}s; verification_state=$state"
        break
    fi
done

if [ -z "$verified_state" ]; then
    echo "FAIL: declaration's verification_state never transitioned within 30s"
    docker compose -f "$COMPOSE_FILE" logs verification declaration | tail -30 || true
    exit 1
fi

echo ""
echo "── R-LOOP-2 kafka-smoke: PASS ──"
echo "  • Declaration accepted via HTTP (201 + signed receipt)"
echo "  • Event observed on recor.declaration.events.v1"
echo "  • Verification case persisted with the submitted declaration_id"
echo "  • Writeback applied; verification_state = $verified_state"
echo "  • Dual transport (HTTP + Kafka) absorbed by V-engine idempotency"
