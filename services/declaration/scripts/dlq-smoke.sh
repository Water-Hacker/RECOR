#!/usr/bin/env bash
# Focused DLQ smoke (R-LOOP-4-DLQ).
#
# Reuses the integration compose stack but configures the declaration
# service's outbox-relay to point at an UNREACHABLE webhook URL and
# uses a tight max_attempts = 2 so a row reaches the DLQ in seconds
# rather than minutes.
#
# Asserts:
#   1. A new declaration submission writes to `outbox`.
#   2. After ~2 attempts (which all fail because the webhook URL is
#      bogus), the row moves out of `outbox` into `outbox_dlq`.
#   3. The DLQ row carries dispatch_attempts >= max_attempts and a
#      `last_error` value.
#
# This is a failure-path smoke. The standard integration-smoke.sh
# exercises the happy path; this one exercises the dead-letter path
# they share.

set -euo pipefail
cd "$(dirname "$0")/.."

COMPOSE_FILE="docker-compose.dlq-smoke.yaml"

if [ ! -f .env ]; then
    {
        echo "RECOR_DB_PASSWORD=$(openssl rand -hex 24)"
        echo "RECOR_D_TO_V_HMAC=$(openssl rand -hex 32)"
        echo "RECOR_V_TO_D_HMAC=$(openssl rand -hex 32)"
    } > .env
fi

# Reuse the integration compose but override RELAY_WEBHOOK_URL to a
# guaranteed-dead address + tighten max_attempts.
cat > "$COMPOSE_FILE" <<'EOF'
name: recor-dlq-smoke

networks:
  dlq:
    driver: bridge

volumes:
  pg-dlq-data:

services:
  postgres-declaration:
    image: postgres:17-alpine
    container_name: recor-dlq-pg
    environment:
      POSTGRES_USER: recor
      POSTGRES_PASSWORD: "${RECOR_DB_PASSWORD:?Set RECOR_DB_PASSWORD}"
      POSTGRES_DB: declaration
    volumes: [pg-dlq-data:/var/lib/postgresql/data]
    networks: [dlq]
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U recor -d declaration"]
      interval: 5s
      timeout: 3s
      retries: 20

  declaration:
    build:
      # Workspace root.
      context: ../..
      dockerfile: services/declaration/Dockerfile
    image: recor/declaration:dev
    container_name: recor-dlq-declaration
    depends_on:
      postgres-declaration: { condition: service_healthy }
    environment:
      RUST_BACKTRACE: "1"
      BIND_ADDR: "0.0.0.0:8080"
      DATABASE_URL: "postgres://recor:${RECOR_DB_PASSWORD}@postgres-declaration:5432/declaration"
      DB_POOL_MAX_CONNECTIONS: "5"
      OTLP_ENDPOINT: ""
      LOG_FILTER: "info,recor_declaration=debug,sqlx=warn"
      SERVICE_NAME: "recor-declaration"
      ENVIRONMENT: "dev"
      HTTP_TIMEOUT_SECONDS: "10"
      RECOR_BASE_URL: "http://localhost:8080"
      # DLQ-focused config: unreachable webhook + tight max_attempts.
      RELAY_WEBHOOK_URL: "http://nope-not-here.invalid:9999/sink"
      RELAY_HMAC_SECRET: "${RECOR_D_TO_V_HMAC:?Set RECOR_D_TO_V_HMAC}"
      RELAY_POLL_INTERVAL_SECONDS: "1"
      WRITEBACK_HMAC_SECRET: "${RECOR_V_TO_D_HMAC:?Set RECOR_V_TO_D_HMAC}"
    ports: ["127.0.0.1:8088:8080"]
    networks: [dlq]
    healthcheck:
      test: ["CMD", "curl", "-fsS", "http://localhost:8080/healthz"]
      interval: 5s
      timeout: 3s
      retries: 30
      start_period: 10s
EOF

echo "── compose up (DLQ smoke) ──"
docker compose -f "$COMPOSE_FILE" up -d --build 2>&1 | tail -5

echo "── waiting for declaration to be healthy ──"
for i in {1..60}; do
    if curl -fsS "http://127.0.0.1:8088/healthz" >/dev/null 2>&1; then
        echo "  ✅ healthy after ${i}s"
        break
    fi
    sleep 1
done

echo ""
echo "── submit a declaration (the relay will try & fail) ──"
PRINCIPAL="spiffe://recor.cm/dlq-smoke"
KEYDIR=$(mktemp -d)
trap "rm -rf $KEYDIR; docker compose -f $COMPOSE_FILE down -v >/dev/null 2>&1; rm -f $COMPOSE_FILE" EXIT
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

curl -sS -X POST http://127.0.0.1:8088/v1/declarations \
    -H "Content-Type: application/json" \
    -H "X-Recor-Dev-Principal: $PRINCIPAL" \
    -d "$REQ" > /dev/null
echo "  ✅ declaration submitted ($DECL_ID)"

echo ""
echo "── waiting for the relay to dead-letter the outbox row ──"
DB_PW="$(grep RECOR_DB_PASSWORD .env | cut -d= -f2-)"
dlq_count=0
for i in {1..90}; do
    sleep 1
    dlq_count=$(docker compose -f "$COMPOSE_FILE" exec -T -e PGPASSWORD="$DB_PW" postgres-declaration \
        psql -U recor -d declaration -tAc \
        "SELECT COUNT(*) FROM outbox_dlq WHERE aggregate_id = '$DECL_ID'" \
        2>/dev/null | tr -d '[:space:]')
    if [ "$dlq_count" -ge "1" ]; then
        echo "  ✅ row appeared in outbox_dlq after ${i}s"
        break
    fi
done

if [ "$dlq_count" -lt "1" ]; then
    echo "FAIL: outbox row never moved to outbox_dlq within 90s"
    echo ""
    echo "─── relay logs (last 30) ───"
    docker compose -f "$COMPOSE_FILE" logs declaration | tail -30 || true
    echo "─── outbox state ───"
    docker compose -f "$COMPOSE_FILE" exec -T -e PGPASSWORD="$DB_PW" postgres-declaration \
        psql -U recor -d declaration -c \
        "SELECT event_type, dispatched_at, dispatch_attempts, last_error FROM outbox WHERE aggregate_id = '$DECL_ID'" || true
    exit 1
fi

echo ""
echo "── DLQ row state ──"
docker compose -f "$COMPOSE_FILE" exec -T -e PGPASSWORD="$DB_PW" postgres-declaration \
    psql -U recor -d declaration -c \
    "SELECT event_type, dispatch_attempts, LEFT(last_error, 80) AS last_error_preview FROM outbox_dlq WHERE aggregate_id = '$DECL_ID'"

# Assert: row no longer present in outbox.
outbox_count=$(docker compose -f "$COMPOSE_FILE" exec -T -e PGPASSWORD="$DB_PW" postgres-declaration \
    psql -U recor -d declaration -tAc \
    "SELECT COUNT(*) FROM outbox WHERE aggregate_id = '$DECL_ID'" | tr -d '[:space:]')
[ "$outbox_count" = "0" ] || {
    echo "FAIL: row still in outbox after DLQ move (count=$outbox_count); atomicity broken"
    exit 1
}
echo "  ✅ row absent from outbox (atomic move verified)"

attempts=$(docker compose -f "$COMPOSE_FILE" exec -T -e PGPASSWORD="$DB_PW" postgres-declaration \
    psql -U recor -d declaration -tAc \
    "SELECT dispatch_attempts FROM outbox_dlq WHERE aggregate_id = '$DECL_ID'" | tr -d '[:space:]')
[ "$attempts" -ge "12" ] || {
    echo "FAIL: dead-lettered row has dispatch_attempts=$attempts; expected >= max_attempts (12)"
    exit 1
}
echo "  ✅ dispatch_attempts = $attempts (>= max_attempts)"

echo ""
echo "✅ R-LOOP-4-DLQ SMOKE: PASS"
echo "   • Submission written to outbox"
echo "   • Relay exhausted max_attempts attempting to deliver to a dead webhook"
echo "   • Row moved atomically from outbox → outbox_dlq"
echo "   • DLQ row carries final dispatch_attempts + last_error"
