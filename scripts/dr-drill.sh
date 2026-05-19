#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────
# RÉCOR disaster-recovery drill (COMP-5).
#
# Drives the D↔V loop integration stack from "healthy" to "full data
# loss" to "restored from backup", and asserts the freshly-recovered
# platform can serve traffic. The measured RTO is printed at the end.
#
# Phases:
#   1. Compose up the D↔V stack from services/declaration/docker-compose.integration.yaml.
#   2. Seed a deterministic, signed declaration into the declaration service.
#   3. Capture the seed body via GET so the post-restore comparison is byte-exact.
#   4. Snapshot both Postgres volumes via pg_dump (custom format).
#   5. Simulate full data loss — `docker compose down -v` for the
#      postgres services (the named volumes are destroyed).
#   6. Bring the postgres services back up on fresh volumes.
#   7. Restore each DB from the pg_dump snapshot. Restart the app
#      services so they pick up the restored DB state.
#   8. Assert /healthz + /readyz on both services.
#   9. Retrieve the seeded declaration via GET and assert byte-for-byte
#      equality with the pre-loss capture.
#  10. Report elapsed wall time (the observed RTO).
#
# Doctrines:
#   D14 fail-closed — any failed phase exits non-zero with a clear error.
#   D19 reproducible — seeded test data; named volumes; pg_dump custom format.
#   D16 observability — RTO is printed; the script is the metric.
#
# Usage:
#   bash scripts/dr-drill.sh
#
# Prerequisites:
#   - docker + docker compose v2
#   - openssl, jq, curl, xxd (already required by integration-smoke.sh)
#
# The drill is destructive to the integration compose volumes (it
# explicitly tears them down). Do NOT run it against any environment
# you are not happy to wipe.
# ─────────────────────────────────────────────────────────────────────────

set -euo pipefail

# ─── Configuration ───────────────────────────────────────────────────────
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
COMPOSE_DIR="$REPO_ROOT/services/declaration"
COMPOSE_FILE="$COMPOSE_DIR/docker-compose.integration.yaml"
SNAPSHOT_DIR="${DR_DRILL_SNAPSHOT_DIR:-$(mktemp -d -t recor-dr-drill-XXXXXX)}"
DECL_HOST="${DR_DRILL_DECL_HOST:-127.0.0.1}"
DECL_PORT="${DR_DRILL_DECL_PORT:-8080}"
VER_HOST="${DR_DRILL_VER_HOST:-127.0.0.1}"
VER_PORT="${DR_DRILL_VER_PORT:-8081}"
DECL_BASE="http://${DECL_HOST}:${DECL_PORT}"
VER_BASE="http://${VER_HOST}:${VER_PORT}"

DECL_DB="declaration"
VER_DB="verification"
DECL_PG_SVC="postgres-declaration"
VER_PG_SVC="postgres-verification"
DECL_SVC="declaration"
VER_SVC="verification"

START_EPOCH=0
RECOVERY_START_EPOCH=0
DRILL_PASSED=0

# ─── Output helpers ──────────────────────────────────────────────────────
banner() {
    printf '\n==> %s\n' "$*"
}
note() {
    printf '    %s\n' "$*"
}
fail() {
    printf '\n[FAIL] %s\n' "$*" >&2
    exit 1
}

# ─── Cleanup trap ────────────────────────────────────────────────────────
# We deliberately leave the compose stack up on success so an operator
# can poke at the recovered platform. On failure we drop logs to aid
# the post-mortem. The snapshot directory is preserved (it is the
# evidence the restore actually replayed).
cleanup() {
    local rc=$?
    if [ "$DRILL_PASSED" -ne 1 ] && [ "$rc" -ne 0 ]; then
        printf '\n[cleanup] drill failed (rc=%s); dumping recent logs\n' "$rc" >&2
        if [ -f "$COMPOSE_FILE" ]; then
            for svc in "$DECL_PG_SVC" "$VER_PG_SVC" "$DECL_SVC" "$VER_SVC"; do
                printf '\n--- %s (last 40 lines) ---\n' "$svc" >&2
                docker compose -f "$COMPOSE_FILE" logs --no-color "$svc" 2>&1 | tail -40 >&2 || true
            done
        fi
        printf '\n[cleanup] snapshot directory preserved at: %s\n' "$SNAPSHOT_DIR" >&2
    fi
}
trap cleanup EXIT

# ─── Preflight ───────────────────────────────────────────────────────────
banner "0/10: preflight"
command -v docker >/dev/null 2>&1 || fail "docker not found on PATH"
command -v openssl >/dev/null 2>&1 || fail "openssl not found on PATH"
command -v jq >/dev/null 2>&1 || fail "jq not found on PATH"
command -v curl >/dev/null 2>&1 || fail "curl not found on PATH"
command -v xxd >/dev/null 2>&1 || fail "xxd not found on PATH"
docker compose version >/dev/null 2>&1 || fail "docker compose v2 not available"
[ -f "$COMPOSE_FILE" ] || fail "compose file not found: $COMPOSE_FILE"
note "snapshot dir: $SNAPSHOT_DIR"
mkdir -p "$SNAPSHOT_DIR"

# .env is required by the compose file for the HMAC + DB password.
if [ ! -f "$COMPOSE_DIR/.env" ]; then
    note "generating ephemeral .env (drill-only)"
    {
        echo "RECOR_DB_PASSWORD=$(openssl rand -hex 24)"
        echo "RECOR_D_TO_V_HMAC=$(openssl rand -hex 32)"
        echo "RECOR_V_TO_D_HMAC=$(openssl rand -hex 32)"
    } > "$COMPOSE_DIR/.env"
fi

DB_PW="$(grep '^RECOR_DB_PASSWORD=' "$COMPOSE_DIR/.env" | cut -d= -f2-)"
[ -n "$DB_PW" ] || fail "RECOR_DB_PASSWORD missing from $COMPOSE_DIR/.env"

START_EPOCH=$(date +%s)

# ─── Phase 1: compose up ─────────────────────────────────────────────────
banner "1/10: bringing up the D↔V stack"
docker compose -f "$COMPOSE_FILE" up -d --build 2>&1 | tail -5

wait_for_http() {
    local url="$1"
    local timeout="${2:-90}"
    for ((i = 0; i < timeout; i++)); do
        if curl -fsS "$url" >/dev/null 2>&1; then
            return 0
        fi
        sleep 1
    done
    return 1
}

note "waiting for declaration /healthz"
wait_for_http "${DECL_BASE}/healthz" 120 || fail "declaration never became healthy"
note "waiting for verification /healthz"
wait_for_http "${VER_BASE}/healthz" 120 || fail "verification never became healthy"

# ─── Phase 2: seed a deterministic declaration ───────────────────────────
banner "2/10: seeding a deterministic test declaration"
KEYDIR="$(mktemp -d)"
trap 'rm -rf "$KEYDIR"' RETURN 2>/dev/null || true
openssl genpkey -algorithm Ed25519 -out "$KEYDIR/sk.pem" 2>/dev/null
PK_HEX=$(openssl pkey -in "$KEYDIR/sk.pem" -pubout -outform DER 2>/dev/null | tail -c 32 | xxd -p -c 64)

PRINCIPAL="spiffe://recor.cm/dr-drill"
DECL_ID="$(cat /proc/sys/kernel/random/uuid)"
ENT_ID="$(cat /proc/sys/kernel/random/uuid)"
PER_ID="$(cat /proc/sys/kernel/random/uuid)"
NONCE_HEX="$(openssl rand -hex 16)"

CANONICAL=$(jq -c -n \
    --arg eid "$ENT_ID" --arg p "$PRINCIPAL" --arg pid "$PER_ID" --arg n "$NONCE_HEX" \
    '{entity_id:$eid, declarant_principal:$p, declarant_role:"self", kind:"incorporation", effective_from:"2026-01-01", beneficial_owners:[{person_id:$pid, ownership_basis_points:10000, interest_kind:"equity"}], nonce_hex:$n}')
printf '%s' "$CANONICAL" > "$KEYDIR/payload"
SIG_HEX=$(openssl pkeyutl -sign -inkey "$KEYDIR/sk.pem" -rawin -in "$KEYDIR/payload" | xxd -p -c 128)

REQ=$(jq -c -n \
    --arg did "$DECL_ID" --arg eid "$ENT_ID" --arg pid "$PER_ID" --arg p "$PRINCIPAL" \
    --arg s "$SIG_HEX" --arg pk "$PK_HEX" --arg n "$NONCE_HEX" \
    '{declaration_id:$did, entity_id:$eid, declarant_role:"self", kind:"incorporation", effective_from:"2026-01-01",
      beneficial_owners:[{person_id:$pid, ownership_basis_points:10000, interest_kind:"equity"}],
      attestation:{signed_by:$p, signature_algorithm:"ed25519", signature_hex:$s, public_key_hex:$pk, nonce_hex:$n}}')

POST_RESP=$(curl -sS -X POST "${DECL_BASE}/v1/declarations" \
    -H "Content-Type: application/json" \
    -H "X-Recor-Dev-Principal: $PRINCIPAL" \
    -d "$REQ" -w "\n%{http_code}")
POST_HTTP=$(echo "$POST_RESP" | tail -1)
[ "$POST_HTTP" = "201" ] || fail "seed declaration POST returned HTTP $POST_HTTP (expected 201)"
note "seeded declaration_id=$DECL_ID"

# ─── Phase 3: capture pre-loss GET body ──────────────────────────────────
banner "3/10: capturing pre-loss declaration body"
# The declaration projection is mutated asynchronously by the V-engine
# writeback (the internal `/v1/internal/declaration-events` webhook
# bumps aggregate_version + writes the verification_state /
# verification_lane / verified_at fields). If we snapshot at
# verification_state="pending" the post-restore GET will land on the
# terminal state and the byte-equality assertion will spuriously fail
# even though the backup is honest. Wait for a terminal verification
# state — `accepted` or `rejected` — before snapshotting so the
# captured body is what an operator at the moment-of-disaster would
# actually see.
TERMINAL_STATES_RE='^(accepted|rejected)$'
PRE_TIMEOUT=120
PRE_STATE=""
for ((i = 0; i < PRE_TIMEOUT; i++)); do
    if curl -fsS "${DECL_BASE}/v1/declarations/${DECL_ID}" \
        -H "X-Recor-Dev-Principal: $PRINCIPAL" > "$SNAPSHOT_DIR/declaration.pre.json" 2>/dev/null; then
        PRE_STATE="$(jq -r '.verification_state // ""' "$SNAPSHOT_DIR/declaration.pre.json")"
        if [[ "$PRE_STATE" =~ $TERMINAL_STATES_RE ]]; then
            break
        fi
    fi
    sleep 1
done
[ -s "$SNAPSHOT_DIR/declaration.pre.json" ] || fail "could not GET seeded declaration before loss"
if [[ ! "$PRE_STATE" =~ $TERMINAL_STATES_RE ]]; then
    fail "verification_state never reached a terminal value within ${PRE_TIMEOUT}s (last seen: '$PRE_STATE')"
fi
# Canonicalise for byte-equality comparison; the platform doesn't
# guarantee key order across reads from a recovered DB.
jq -S . "$SNAPSHOT_DIR/declaration.pre.json" > "$SNAPSHOT_DIR/declaration.pre.canon.json"
note "pre-loss body captured at verification_state=$PRE_STATE ($(wc -c < "$SNAPSHOT_DIR/declaration.pre.canon.json") bytes)"

# ─── Phase 4: snapshot both DBs via pg_dump ──────────────────────────────
banner "4/10: snapshotting both Postgres databases via pg_dump"
dump_db() {
    local svc="$1"
    local db="$2"
    local out="$3"
    docker compose -f "$COMPOSE_FILE" exec -T -e PGPASSWORD="$DB_PW" "$svc" \
        pg_dump -U recor -d "$db" -Fc --no-owner --no-privileges \
        > "$out"
    [ -s "$out" ] || return 1
}

dump_db "$DECL_PG_SVC" "$DECL_DB" "$SNAPSHOT_DIR/${DECL_DB}.dump" || fail "pg_dump $DECL_DB failed"
dump_db "$VER_PG_SVC" "$VER_DB" "$SNAPSHOT_DIR/${VER_DB}.dump" || fail "pg_dump $VER_DB failed"
note "declaration dump: $(wc -c < "$SNAPSHOT_DIR/${DECL_DB}.dump") bytes"
note "verification dump: $(wc -c < "$SNAPSHOT_DIR/${VER_DB}.dump") bytes"

# ─── Phase 5: simulate full data loss ────────────────────────────────────
banner "5/10: simulating full data loss (tearing down DB volumes)"
# Mark the start of the recovery window — RTO is measured from here
# (the destructive event) until /readyz returns 200 against the
# recovered stack and the GET assertion succeeds.
RECOVERY_START_EPOCH=$(date +%s)

# Stop the app services first so they don't thrash against a missing
# DB while volumes are wiped. We use `stop` (not `down`) on the app
# services so their container state is preserved.
docker compose -f "$COMPOSE_FILE" stop "$DECL_SVC" "$VER_SVC" >/dev/null 2>&1 || true

# Wipe the postgres services + their named volumes. `docker compose
# down -v` against the whole project would also wipe the app images;
# we want only the data layer destroyed.
docker compose -f "$COMPOSE_FILE" rm -sfv "$DECL_PG_SVC" "$VER_PG_SVC" >/dev/null 2>&1 || true
# `rm -sfv` removes anonymous volumes but not named ones — drop those
# explicitly. The compose project name is "recor-dv-loop" per the
# compose file's `name:` directive.
COMPOSE_PROJECT="$(docker compose -f "$COMPOSE_FILE" config --format json 2>/dev/null \
    | jq -r '.name // "recor-dv-loop"')"
for vol in "${COMPOSE_PROJECT}_pg-decl-data" "${COMPOSE_PROJECT}_pg-ver-data"; do
    if docker volume inspect "$vol" >/dev/null 2>&1; then
        docker volume rm -f "$vol" >/dev/null 2>&1 || fail "could not destroy volume $vol"
        note "destroyed volume $vol"
    fi
done

# ─── Phase 6: bring DBs back up on fresh volumes ─────────────────────────
banner "6/10: bringing DBs back up on fresh volumes"
docker compose -f "$COMPOSE_FILE" up -d "$DECL_PG_SVC" "$VER_PG_SVC" >/dev/null
# Wait until both postgres containers report healthy.
wait_for_pg_ready() {
    local svc="$1"
    local db="$2"
    for ((i = 0; i < 60; i++)); do
        if docker compose -f "$COMPOSE_FILE" exec -T "$svc" \
            pg_isready -U recor -d "$db" >/dev/null 2>&1; then
            return 0
        fi
        sleep 1
    done
    return 1
}
wait_for_pg_ready "$DECL_PG_SVC" "$DECL_DB" || fail "fresh $DECL_PG_SVC never became ready"
wait_for_pg_ready "$VER_PG_SVC" "$VER_DB" || fail "fresh $VER_PG_SVC never became ready"
note "fresh DBs are ready"

# ─── Phase 7: restore from snapshot ──────────────────────────────────────
banner "7/10: restoring both DBs from the pg_dump snapshots"
restore_db() {
    local svc="$1"
    local db="$2"
    local src="$3"
    # pg_restore from stdin into the freshly-created (empty) DB. The
    # postgres image's entrypoint already created $db on first boot;
    # pg_restore drops/recreates objects as the dump dictates.
    docker compose -f "$COMPOSE_FILE" exec -T -e PGPASSWORD="$DB_PW" "$svc" \
        pg_restore -U recor -d "$db" --no-owner --no-privileges --exit-on-error \
        < "$src"
}
restore_db "$DECL_PG_SVC" "$DECL_DB" "$SNAPSHOT_DIR/${DECL_DB}.dump" \
    || fail "pg_restore $DECL_DB failed"
note "declaration DB restored"
restore_db "$VER_PG_SVC" "$VER_DB" "$SNAPSHOT_DIR/${VER_DB}.dump" \
    || fail "pg_restore $VER_DB failed"
note "verification DB restored"

# Restart the app services so they reconnect to the restored DB.
docker compose -f "$COMPOSE_FILE" up -d "$DECL_SVC" "$VER_SVC" >/dev/null

# ─── Phase 8: assert /healthz + /readyz ──────────────────────────────────
banner "8/10: asserting health + readiness on the recovered stack"
wait_for_http "${DECL_BASE}/healthz" 120 || fail "declaration /healthz never returned 200 post-restore"
wait_for_http "${VER_BASE}/healthz" 120 || fail "verification /healthz never returned 200 post-restore"
wait_for_http "${DECL_BASE}/readyz" 120 || fail "declaration /readyz never returned 200 post-restore"
wait_for_http "${VER_BASE}/readyz" 120 || fail "verification /readyz never returned 200 post-restore"
note "all health + readiness probes pass"

# ─── Phase 9: assert byte-equal GET for the seeded declaration ───────────
banner "9/10: asserting seeded declaration is byte-identical post-restore"
for i in {1..30}; do
    if curl -fsS "${DECL_BASE}/v1/declarations/${DECL_ID}" \
        -H "X-Recor-Dev-Principal: $PRINCIPAL" > "$SNAPSHOT_DIR/declaration.post.json" 2>/dev/null; then
        break
    fi
    sleep 1
done
[ -s "$SNAPSHOT_DIR/declaration.post.json" ] || fail "could not GET seeded declaration post-restore"
jq -S . "$SNAPSHOT_DIR/declaration.post.json" > "$SNAPSHOT_DIR/declaration.post.canon.json"

if ! diff -u "$SNAPSHOT_DIR/declaration.pre.canon.json" "$SNAPSHOT_DIR/declaration.post.canon.json" > "$SNAPSHOT_DIR/declaration.diff" 2>&1; then
    printf '\n[FAIL] post-restore declaration body differs from pre-loss capture:\n' >&2
    cat "$SNAPSHOT_DIR/declaration.diff" >&2
    fail "byte-equality assertion failed"
fi
note "post-restore body matches pre-loss body byte-for-byte"

# ─── Phase 10: report observed RTO ───────────────────────────────────────
banner "10/10: drill complete"
END_EPOCH=$(date +%s)
TOTAL=$((END_EPOCH - START_EPOCH))
RTO=$((END_EPOCH - RECOVERY_START_EPOCH))

printf '\n'
printf '┌──────────────────────────────────────────────────────────┐\n'
printf '│ RÉCOR DR drill: PASS                                     │\n'
printf '├──────────────────────────────────────────────────────────┤\n'
printf '│ Total elapsed (drill end-to-end): %ss\n' "$TOTAL"
printf '│ RTO observed (loss → traffic-ready): %ss\n' "$RTO"
printf '│ Snapshot evidence: %s\n' "$SNAPSHOT_DIR"
printf '└──────────────────────────────────────────────────────────┘\n'
printf 'RTO observed: %ss\n' "$RTO"

DRILL_PASSED=1
exit 0
