#!/usr/bin/env bash
# Focused DLQ smoke for the Verification Engine (R-LOOP-DLQ-3).
#
# Mirror of services/declaration/scripts/dlq-smoke.sh — exercises the
# V-engine's admin endpoints under
#   GET  /v1/internal/verification-outbox-dlq
#   POST /v1/internal/verification-outbox-dlq/{id}/replay
#
# Strategy: stand up postgres-verification + the verification engine
# (only) and point WRITEBACK_URL at a guaranteed-dead host with a
# tight max_attempts so a row dead-letters in seconds.
#
# Per the ticket brief, we use Option (a): inject a synthetic row into
# verification_outbox directly via psql, rather than running the full
# D↔V loop. The V-engine's writeback relay picks it up, fails to
# deliver, and after dispatch_attempts >= max_attempts (2 here) moves
# the row to verification_outbox_dlq. From there we exercise the 6
# admin assertions.
#
# Assertions:
#   1. A non-admin principal is refused (403).
#   2. An admin can list the DLQ row.
#   3. An admin can POST .../replay and get 200.
#   4. Atomic move: row absent from verification_outbox_dlq,
#      present in verification_outbox.
#   5. Replayed row has dispatch_attempts=0 and last_error=NULL.
#   6. Replaying a missing id returns 404.

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

# Backfill missing HMAC vars in a pre-existing .env (the V-engine
# .env may have been seeded with only the DB password by earlier
# tooling). The smoke needs both channel secrets to bring the stack up.
if ! grep -q "^RECOR_D_TO_V_HMAC=" .env; then
    echo "RECOR_D_TO_V_HMAC=$(openssl rand -hex 32)" >> .env
fi
if ! grep -q "^RECOR_V_TO_D_HMAC=" .env; then
    echo "RECOR_V_TO_D_HMAC=$(openssl rand -hex 32)" >> .env
fi

cat > "$COMPOSE_FILE" <<'EOF'
name: recor-ver-dlq-smoke

networks:
  vdlq:
    driver: bridge

volumes:
  pg-vdlq-data:

services:
  postgres-verification:
    image: postgres:17-alpine
    container_name: recor-vdlq-pg
    environment:
      POSTGRES_USER: recor
      POSTGRES_PASSWORD: "${RECOR_DB_PASSWORD:?Set RECOR_DB_PASSWORD}"
      POSTGRES_DB: verification
    volumes: [pg-vdlq-data:/var/lib/postgresql/data]
    networks: [vdlq]
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U recor -d verification"]
      interval: 5s
      timeout: 3s
      retries: 20

  verification:
    build:
      # Workspace root — Dockerfile needs sibling crates.
      context: ../..
      dockerfile: services/verification-engine/Dockerfile
    image: recor/verification-engine:dev
    container_name: recor-vdlq-verification
    depends_on:
      postgres-verification: { condition: service_healthy }
    environment:
      RUST_BACKTRACE: "1"
      BIND_ADDR: "0.0.0.0:8081"
      DATABASE_URL: "postgres://recor:${RECOR_DB_PASSWORD}@postgres-verification:5432/verification"
      DB_POOL_MAX_CONNECTIONS: "5"
      OTLP_ENDPOINT: ""
      LOG_FILTER: "info,recor_verification_engine=debug,sqlx=warn"
      SERVICE_NAME: "recor-verification-engine"
      ENVIRONMENT: "dev"
      HTTP_TIMEOUT_SECONDS: "10"
      RECOR_BASE_URL: "http://localhost:8081"
      # No inbound webhook needed — we inject rows via psql.
      INBOUND_HMAC_SECRET: "${RECOR_D_TO_V_HMAC:?Set RECOR_D_TO_V_HMAC}"
      # DLQ-focused config: dead writeback target + tight max_attempts.
      WRITEBACK_URL: "http://nope-not-here.invalid:9999/sink"
      WRITEBACK_HMAC_SECRET: "${RECOR_V_TO_D_HMAC:?Set RECOR_V_TO_D_HMAC}"
      WRITEBACK_POLL_INTERVAL_SECONDS: "1"
      WRITEBACK_MAX_ATTEMPTS: "2"
      # Admin endpoint allowlist — enables /v1/internal/verification-outbox-dlq.
      ADMIN_PRINCIPALS: "spiffe://recor.cm/dlq-smoke"
    ports: ["127.0.0.1:8089:8081"]
    networks: [vdlq]
    healthcheck:
      test: ["CMD", "curl", "-fsS", "http://localhost:8081/healthz"]
      interval: 5s
      timeout: 3s
      retries: 30
      start_period: 10s
EOF

echo "── compose up (V-engine DLQ smoke) ──"
docker compose -f "$COMPOSE_FILE" up -d --build 2>&1 | tail -5

trap "docker compose -f $COMPOSE_FILE down -v >/dev/null 2>&1; rm -f $COMPOSE_FILE" EXIT

echo "── waiting for verification engine to be healthy ──"
for i in {1..60}; do
    if curl -fsS "http://127.0.0.1:8089/healthz" >/dev/null 2>&1; then
        echo "  ✅ healthy after ${i}s"
        break
    fi
    sleep 1
done

DB_PW="$(grep RECOR_DB_PASSWORD .env | cut -d= -f2-)"

echo ""
echo "── injecting a synthetic verification_outbox row (Option a per brief) ──"
EVENT_ID=$(cat /proc/sys/kernel/random/uuid)
AGG_ID=$(cat /proc/sys/kernel/random/uuid)
ROW_ID=$(cat /proc/sys/kernel/random/uuid)
docker compose -f "$COMPOSE_FILE" exec -T -e PGPASSWORD="$DB_PW" postgres-verification \
    psql -U recor -d verification -v ON_ERROR_STOP=1 \
        -c "INSERT INTO verification_outbox (id, event_id, event_type, event_version,
                                              aggregate_id, partition_key, payload,
                                              created_at, dispatch_attempts)
            VALUES ('$ROW_ID', '$EVENT_ID', 'verification.completed.v1', 1,
                    '$AGG_ID', '$AGG_ID',
                    '{\"case_id\": \"$AGG_ID\", \"lane\": \"green\", \"smoke\": true}'::jsonb,
                    NOW(), 0);" >/dev/null
echo "  ✅ synthetic row injected (id=$ROW_ID, event_id=$EVENT_ID, aggregate_id=$AGG_ID)"

echo ""
echo "── waiting for the relay to dead-letter the row ──"
dlq_count=0
for i in {1..90}; do
    sleep 1
    dlq_count=$(docker compose -f "$COMPOSE_FILE" exec -T -e PGPASSWORD="$DB_PW" postgres-verification \
        psql -U recor -d verification -tAc \
        "SELECT COUNT(*) FROM verification_outbox_dlq WHERE event_id = '$EVENT_ID'" \
        2>/dev/null | tr -d '[:space:]')
    if [ "$dlq_count" -ge "1" ]; then
        echo "  ✅ row appeared in verification_outbox_dlq after ${i}s"
        break
    fi
done

if [ "$dlq_count" -lt "1" ]; then
    echo "FAIL: verification_outbox row never moved to verification_outbox_dlq within 90s"
    echo ""
    echo "─── relay logs (last 30) ───"
    docker compose -f "$COMPOSE_FILE" logs verification | tail -30 || true
    echo "─── outbox state ───"
    docker compose -f "$COMPOSE_FILE" exec -T -e PGPASSWORD="$DB_PW" postgres-verification \
        psql -U recor -d verification -c \
        "SELECT event_type, dispatched_at, dispatch_attempts, last_error FROM verification_outbox WHERE event_id = '$EVENT_ID'" || true
    exit 1
fi

# Find the DLQ id (it's the same as ROW_ID by construction since the
# relay's INSERT-then-DELETE preserves the row id).
dlq_id=$(docker compose -f "$COMPOSE_FILE" exec -T -e PGPASSWORD="$DB_PW" postgres-verification \
    psql -U recor -d verification -tAc \
    "SELECT id FROM verification_outbox_dlq WHERE event_id = '$EVENT_ID'" | tr -d '[:space:]')
echo "  ── DLQ row id = $dlq_id"

echo ""
echo "── assertion 1: non-admin principal → 403 ──"
nonadmin_resp=$(curl -sS -w "\n%{http_code}" \
    "http://127.0.0.1:8089/v1/internal/verification-outbox-dlq" \
    -H "X-Recor-Dev-Principal: spiffe://recor.cm/intruder")
nonadmin_http=$(echo "$nonadmin_resp" | tail -1)
[ "$nonadmin_http" = "403" ] || {
    echo "FAIL: expected 403 for non-admin; got $nonadmin_http"
    echo "$nonadmin_resp" | sed '$d'
    exit 1
}
echo "  ✅ non-admin refused (403)"

echo ""
echo "── assertion 2: admin GET /v1/internal/verification-outbox-dlq ──"
list_resp=$(curl -sS \
    "http://127.0.0.1:8089/v1/internal/verification-outbox-dlq?limit=20" \
    -H "X-Recor-Dev-Principal: spiffe://recor.cm/dlq-smoke")
echo "$list_resp" | jq '{total, items: (.items | map({id, event_type, dispatch_attempts}))}'
dlq_total=$(echo "$list_resp" | jq -r '.total')
[ "$dlq_total" -ge "1" ] || {
    echo "FAIL: expected total >= 1; got $dlq_total"
    exit 1
}
listed_id=$(echo "$list_resp" | jq -r ".items[] | select(.id==\"$dlq_id\") | .id")
[ "$listed_id" = "$dlq_id" ] || {
    echo "FAIL: injected DLQ row id $dlq_id not found in list response"
    exit 1
}
echo "  ✅ admin can list DLQ; total=$dlq_total; our row present"

# Assertion 6 (404 for missing id) is checked here, BEFORE the live
# replay, so the verification container can still serve the request.
# After the real replay we stop the container to freeze post-state
# (otherwise the relay re-picks-up the replayed row before assertion 5
# can observe its just-replayed dispatch_attempts=0/last_error=NULL
# state).
echo ""
echo "── assertion 6 (early): replaying a missing id → 404 ──"
missing_id="00000000-0000-0000-0000-000000000000"
missing_resp=$(curl -sS -X POST -w "\n%{http_code}" \
    "http://127.0.0.1:8089/v1/internal/verification-outbox-dlq/$missing_id/replay" \
    -H "X-Recor-Dev-Principal: spiffe://recor.cm/dlq-smoke")
missing_http=$(echo "$missing_resp" | tail -1)
[ "$missing_http" = "404" ] || {
    echo "FAIL: expected 404 for missing replay; got $missing_http"
    exit 1
}
echo "  ✅ missing replay correctly returned 404"

echo ""
echo "── assertion 3: admin POST .../replay → 200 ──"
replay_resp=$(curl -sS -X POST -w "\n%{http_code}" \
    "http://127.0.0.1:8089/v1/internal/verification-outbox-dlq/$dlq_id/replay" \
    -H "X-Recor-Dev-Principal: spiffe://recor.cm/dlq-smoke")
replay_http=$(echo "$replay_resp" | tail -1)
replay_body=$(echo "$replay_resp" | sed '$d')
[ "$replay_http" = "200" ] || {
    echo "FAIL: expected 200 from replay; got $replay_http"
    echo "$replay_body"
    exit 1
}
echo "$replay_body" | jq '.'
echo "  ✅ replay endpoint returned 200"

# Stop the verification container so the relay can't re-process the
# replayed row before assertions 4/5 observe its just-replayed state.
# (Postgres remains up so psql still works.)
echo ""
echo "── stopping verification container to freeze post-replay state ──"
docker compose -f "$COMPOSE_FILE" stop verification >/dev/null 2>&1
echo "  ✅ verification stopped"

echo ""
echo "── assertion 4: atomic move verified ──"
post_dlq=$(docker compose -f "$COMPOSE_FILE" exec -T -e PGPASSWORD="$DB_PW" postgres-verification \
    psql -U recor -d verification -tAc \
    "SELECT COUNT(*) FROM verification_outbox_dlq WHERE id = '$dlq_id'" | tr -d '[:space:]')
post_outbox=$(docker compose -f "$COMPOSE_FILE" exec -T -e PGPASSWORD="$DB_PW" postgres-verification \
    psql -U recor -d verification -tAc \
    "SELECT COUNT(*) FROM verification_outbox WHERE id = '$dlq_id'" | tr -d '[:space:]')
[ "$post_dlq" = "0" ] || {
    echo "FAIL: row $dlq_id still in verification_outbox_dlq after replay"
    exit 1
}
[ "$post_outbox" = "1" ] || {
    echo "FAIL: row $dlq_id not in verification_outbox after replay (count=$post_outbox)"
    exit 1
}
echo "  ✅ row absent from verification_outbox_dlq, present in verification_outbox"

echo ""
echo "── assertion 5: replayed row reset to dispatch_attempts=0, last_error=NULL ──"
post_attempts=$(docker compose -f "$COMPOSE_FILE" exec -T -e PGPASSWORD="$DB_PW" postgres-verification \
    psql -U recor -d verification -tAc \
    "SELECT dispatch_attempts, COALESCE(last_error, '__NULL__') FROM verification_outbox WHERE id = '$dlq_id'" \
    | tr -d '[:space:]')
[ "$post_attempts" = "0|__NULL__" ] || {
    echo "FAIL: expected dispatch_attempts=0, last_error=NULL after replay; got '$post_attempts'"
    exit 1
}
echo "  ✅ replayed row reset to dispatch_attempts=0, last_error=NULL"

echo ""
echo "✅ R-LOOP-DLQ-3 SMOKE: PASS"
echo "   • Synthetic verification_outbox row injected"
echo "   • Relay exhausted max_attempts attempting to deliver to a dead host"
echo "   • Row moved atomically into verification_outbox_dlq"
echo "   • Non-admin principal refused (403)"
echo "   • Admin GET /v1/internal/verification-outbox-dlq listed the DLQ row"
echo "   • Admin POST .../replay atomically moved row back to verification_outbox"
echo "   • Replayed row has dispatch_attempts=0, last_error=NULL"
echo "   • Replay against missing id returns 404"
