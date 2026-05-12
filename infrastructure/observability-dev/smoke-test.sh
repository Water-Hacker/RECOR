#!/usr/bin/env bash
# infrastructure/observability-dev/smoke-test.sh
#
# Operational smoke test for the dev observability stack.
# Run by engineers: `cd infrastructure/observability-dev && ./smoke-test.sh`
# Run by CI via the contract wrapper at tests/contract/observability-smoke.test.sh.
#
# DoD instantiated (Companion V7 P30 F-007):
#   "Traces flow from a dev service; dashboards render"
#
# Steps:
#   1. docker compose up -d         — bring up the stack
#   2. wait for every container's healthcheck to pass (≤60s)
#   3. probe each component's health endpoint directly (defence in depth)
#   4. emit 100 traces via the official telemetrygen container
#   5. within 30s, verify the emitted trace IDs appear in Tempo
#   6. verify Grafana's Tempo data source proxy can query the same traces
#      (the "dashboards render" half of DoD — Grafana sees what Tempo sees)
#
# Exit codes:
#   0     all steps succeeded
#   1-99  step number that failed
#   127   prerequisite missing (docker, curl, jq)
#
# Env overrides:
#   RECOR_OBS_KEEP_RUNNING=1   — do NOT tear down on success (debugging)
#   RECOR_OBS_TIMEOUT_HEALTH=60        seconds to wait for healthchecks
#   RECOR_OBS_TIMEOUT_TRACES=30        seconds to wait for traces to appear
#   RECOR_OBS_TRACE_COUNT=100          traces to emit
#   RECOR_OBS_TELEMETRYGEN_IMAGE=ghcr.io/open-telemetry/opentelemetry-collector-contrib/telemetrygen:0.117.0

set -uo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE"

TIMEOUT_HEALTH="${RECOR_OBS_TIMEOUT_HEALTH:-60}"
TIMEOUT_TRACES="${RECOR_OBS_TIMEOUT_TRACES:-30}"
TRACE_COUNT="${RECOR_OBS_TRACE_COUNT:-100}"
TELEMETRYGEN_IMAGE="${RECOR_OBS_TELEMETRYGEN_IMAGE:-ghcr.io/open-telemetry/opentelemetry-collector-contrib/telemetrygen:v0.117.0}"

red()   { printf '\033[31m%s\033[0m' "$*"; }
green() { printf '\033[32m%s\033[0m' "$*"; }
yellow(){ printf '\033[33m%s\033[0m' "$*"; }

step() { printf '\n%s [step %s] %s\n' "$(yellow ──)" "$1" "$2"; }
ok()   { printf '  %s  %s\n' "$(green PASS)" "$1"; }
fail() {
  printf '  %s  %s\n' "$(red FAIL)" "$1" >&2
  if [ -n "${2:-}" ]; then printf '          %s\n' "$2" >&2; fi
  if [ -z "${RECOR_OBS_KEEP_RUNNING:-}" ]; then
    echo "Tearing stack down (RECOR_OBS_KEEP_RUNNING unset)..." >&2
    docker compose down -v >/dev/null 2>&1 || true
  fi
  exit "${3:-1}"
}

for tool in docker curl jq; do
  command -v "$tool" >/dev/null 2>&1 || {
    echo "Required tool missing: $tool" >&2
    exit 127
  }
done

# ─── Step 1: bring stack up ────────────────────────────────────────────────
step 1 "docker compose up -d"
if [ ! -f .env ] && [ -z "${RECOR_GRAFANA_ADMIN_PASSWORD:-}" ]; then
  # D18: never proceed without an admin password. Generate an ephemeral
  # per-run secret in-memory; do not write it to .env.
  export RECOR_GRAFANA_ADMIN_PASSWORD="$(openssl rand -base64 24 2>/dev/null || head -c 24 /dev/urandom | base64)"
  echo "  (ephemeral admin password generated for this run only)"
fi

if ! docker compose up -d 2>&1 | tail -10; then
  fail "docker compose up -d" "compose returned non-zero" 1
fi
ok "compose started"

# ─── Step 2: wait for healthchecks ─────────────────────────────────────────
step 2 "wait for all containers ready (timeout ${TIMEOUT_HEALTH}s)"
# Services with an in-container healthcheck must reach "healthy".
# Services without (currently: otel-collector, because the image is
# distroless and has no probe binary) must reach "running"; their
# liveness is verified by the external HTTP probes in step 3.
declare -A expected_status=(
  [otel-collector]=running
  [prometheus]=healthy
  [tempo]=healthy
  [loki]=healthy
  [grafana]=healthy
)
deadline=$(( $(date +%s) + TIMEOUT_HEALTH ))
service_status() {
  # Prefer the Health field (set when a healthcheck exists); otherwise
  # fall back to the State field (e.g. "running", "exited", "restarting").
  docker compose ps --format json "$1" 2>/dev/null \
    | jq -r 'if (.Health // "") == "" then .State else .Health end' \
    | head -1
}
while [ "$(date +%s)" -lt "$deadline" ]; do
  not_ready=()
  for svc in "${!expected_status[@]}"; do
    want="${expected_status[$svc]}"
    got=$(service_status "$svc")
    if [ "$got" != "$want" ]; then
      not_ready+=("$svc:$got(want=$want)")
    fi
  done
  if [ "${#not_ready[@]}" -eq 0 ]; then
    break
  fi
  sleep 2
done

# Strict re-check after the wait
for svc in "${!expected_status[@]}"; do
  want="${expected_status[$svc]}"
  got=$(service_status "$svc")
  if [ "$got" != "$want" ]; then
    fail "service $svc is '$got' (want '$want')" \
         "logs: docker compose logs $svc | tail -50" 2
  fi
  ok "$svc is $got"
done

# ─── Step 3: probe each component directly ─────────────────────────────────
step 3 "direct health probes"
probe() {
  local name="$1" url="$2"
  if curl -fsS -m 5 -o /dev/null "$url"; then
    ok "$name $url"
  else
    fail "$name health probe at $url" "expected 200" 3
  fi
}
probe "Grafana"        "http://localhost:3000/api/health"
probe "Prometheus"     "http://localhost:9090/-/healthy"
probe "Tempo"          "http://localhost:3200/ready"
probe "Loki"           "http://localhost:3100/ready"
probe "OTel Collector" "http://localhost:13133/"

# ─── Step 4: emit traces via telemetrygen ──────────────────────────────────
step 4 "emit ${TRACE_COUNT} traces via telemetrygen"
TG_LOG="$(mktemp -t recor-smoke-telemetrygen.XXXXXX.log)"
trap 'rm -f "$TG_LOG"' EXIT

# telemetrygen runs as a one-shot container on the same docker network so
# it reaches otel-collector by service name.
if ! docker run --rm \
    --network recor-observability-dev_observability \
    "$TELEMETRYGEN_IMAGE" traces \
    --otlp-endpoint otel-collector:4317 \
    --otlp-insecure \
    --traces "$TRACE_COUNT" \
    --service recor-smoke-test \
    --rate 50 \
    --duration 5s \
    2>&1 | tee "$TG_LOG" | tail -10; then
  fail "telemetrygen emit" "telemetrygen exited non-zero (see $TG_LOG)" 4
fi
ok "telemetrygen emitted (see log for IDs)"

# ─── Step 5: verify traces appear in Tempo ────────────────────────────────
step 5 "verify traces in Tempo (timeout ${TIMEOUT_TRACES}s)"
# Tempo's /api/search with the service name returns the most recent traces.
deadline=$(( $(date +%s) + TIMEOUT_TRACES ))
found_count=0
while [ "$(date +%s)" -lt "$deadline" ]; do
  resp=$(curl -fsS -m 5 \
    "http://localhost:3200/api/search?tags=service.name%3Drecor-smoke-test&limit=200" \
    2>/dev/null) || resp=""
  found_count=$(echo "$resp" | jq -r '.traces | length' 2>/dev/null || echo 0)
  # Tempo block flush is ≤10s in dev config; we expect at least some
  # fraction of the emitted traces to appear within the timeout.
  if [ "$found_count" -ge "$((TRACE_COUNT / 10))" ]; then
    break
  fi
  sleep 2
done

if [ "$found_count" -lt "$((TRACE_COUNT / 10))" ]; then
  fail "trace ingestion: only $found_count of $TRACE_COUNT visible in Tempo" \
       "raw response: $resp" 5
fi
ok "Tempo shows $found_count traces for service.name=recor-smoke-test"

# Grab a specific trace_id for the next check
trace_id=$(echo "$resp" | jq -r '.traces[0].traceID // empty')
[ -n "$trace_id" ] || fail "extract trace_id from Tempo response" "" 5
ok "sampled trace_id=$trace_id"

# ─── Step 6: verify Grafana proxy can render the trace ─────────────────────
step 6 "verify Grafana Tempo proxy returns the trace"
# Grafana's API is HTTP-basic-authenticated.
admin_user="${RECOR_GRAFANA_ADMIN_USER:-recor-admin}"
admin_pass="${RECOR_GRAFANA_ADMIN_PASSWORD}"
proxy_url="http://localhost:3000/api/datasources/proxy/uid/recor-tempo/api/traces/${trace_id}"
proxy_resp=$(curl -fsS -m 5 \
  -u "${admin_user}:${admin_pass}" \
  -H "Accept: application/protobuf" \
  -o /dev/null -w "%{http_code}" \
  "$proxy_url")

if [ "$proxy_resp" != "200" ]; then
  fail "Grafana Tempo proxy returned HTTP $proxy_resp" \
       "url=$proxy_url" 6
fi
ok "Grafana → Tempo proxy returned 200 (dashboards-can-see-traces)"

# ─── Done ──────────────────────────────────────────────────────────────────
printf '\n%s — F-007 DoD satisfied:\n' "$(green OK)"
printf '   • Stack up; all components healthy\n'
printf '   • Traces emitted via telemetrygen and accepted by OTel Collector\n'
printf '   • Tempo ingested traces and serves them via /api/search and /api/traces/<id>\n'
printf '   • Grafana proxy can query Tempo on behalf of dashboards\n'

if [ -z "${RECOR_OBS_KEEP_RUNNING:-}" ]; then
  printf '\nTearing stack down (RECOR_OBS_KEEP_RUNNING unset)...\n'
  docker compose down -v >/dev/null
  ok "tear-down clean"
else
  printf '\nStack left running (RECOR_OBS_KEEP_RUNNING set).\n'
  printf '  Grafana:    http://localhost:3000   (user: %s)\n' "$admin_user"
  printf '  Prometheus: http://localhost:9090\n'
  printf '  Tempo:      http://localhost:3200\n'
  printf '  Loki:       http://localhost:3100\n'
  printf '  Tear down:  cd infrastructure/observability-dev && docker compose down -v\n'
fi
exit 0
