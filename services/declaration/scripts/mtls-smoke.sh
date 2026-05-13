#!/usr/bin/env bash
# R-LOOP-3 mTLS smoke.
#
# Same end-to-end D↔V round-trip as integration-smoke.sh, but runs
# the loop with AUTH_TRANSPORT=mtls — i.e. the inbound endpoints
# require both a verified peer SPIFFE ID AND the HMAC header.
#
# Pre-requisites:
#   1. SPIRE running. Bring it up with:
#        bash infrastructure/spire/scripts/bootstrap.sh
#   2. The two services share the SPIRE agent socket via the
#      spire-agent-socket named volume (declared in the SPIRE
#      compose). The integration compose mounts that volume into
#      both `declaration` and `verification` containers.
#
# This script is the SKELETON. The full version waits on the
# follow-up that swaps `axum::serve` for `axum-server` + rustls
# bind. Until then this script exercises the SPIFFE bootstrap
# path (services refuse to start without an SVID) and the HMAC
# fallback continues to authenticate the application-layer
# request — that's the defence-in-depth `mtls` posture.

set -euo pipefail
cd "$(dirname "$0")/.."

COMPOSE_FILE="docker-compose.integration.yaml"

# Bring up SPIRE first (idempotent — the bootstrap is safe to re-run).
echo "── bringing up SPIRE ──"
bash ../../infrastructure/spire/scripts/bootstrap.sh

# Generate the per-channel HMAC secrets if not already present;
# AUTH_TRANSPORT=mtls keeps these as the defence-in-depth fallback.
if [ ! -f .env ]; then
    {
        echo "RECOR_DB_PASSWORD=$(openssl rand -hex 24)"
        echo "RECOR_D_TO_V_HMAC=$(openssl rand -hex 32)"
        echo "RECOR_V_TO_D_HMAC=$(openssl rand -hex 32)"
    } > .env
    echo "[generated .env with per-channel HMAC secrets + RECOR_DB_PASSWORD]"
fi

# Flip the transport for both services. The compose file reads
# AUTH_TRANSPORT from the env; if your compose doesn't yet expose
# it, export it here and the services will inherit it from the
# host shell.
export AUTH_TRANSPORT="mtls"
echo "[AUTH_TRANSPORT=$AUTH_TRANSPORT]"

echo ""
echo "── compose up (AUTH_TRANSPORT=mtls) ──"
docker compose -f "$COMPOSE_FILE" up -d --build 2>&1 | tail -5

echo ""
echo "── waiting for both services ──"
for svc_url in http://127.0.0.1:8080/healthz http://127.0.0.1:8081/healthz; do
    healthy=0
    for i in {1..60}; do
        if curl -fsS "$svc_url" >/dev/null 2>&1; then
            echo "  ✅ $svc_url healthy after ${i}s"
            healthy=1
            break
        fi
        sleep 1
    done
    if [ "$healthy" -ne 1 ]; then
        echo ""
        echo "FAIL: $svc_url never became healthy under AUTH_TRANSPORT=mtls"
        echo "  D14 fail-closed: the service refuses to start if the SPIFFE"
        echo "  Workload API is unreachable. Check:"
        echo "    docker compose -f $COMPOSE_FILE logs declaration | tail -30"
        echo "    docker compose -f $COMPOSE_FILE logs verification | tail -30"
        exit 1
    fi
done

# Confirm the SPIFFE SVID-fetch counter ticked at least once for each
# service. We don't yet have the rustls-terminated mTLS so the test
# assertion is on the bootstrap path, not the connection-level mTLS.
echo ""
echo "── confirming recor_spiffe_svid_fetch_total{result=success} ──"
for svc_url in http://127.0.0.1:8080/metrics http://127.0.0.1:8081/metrics; do
    count=$(curl -fsS "$svc_url" \
        | awk '/^recor_spiffe_svid_fetch_total\{result="success"\}/ {print $2}' \
        | head -1)
    if [ -z "$count" ] || [ "$count" = "0" ]; then
        echo "FAIL: $svc_url did not increment recor_spiffe_svid_fetch_total{result=success}"
        echo "  This means the SPIFFE bootstrap path did not run, or the"
        echo "  SPIRE agent rejected the workload's selector. Check"
        echo "    docs/runbooks/spiffe-onboarding.md § debug an SVID-fetch failure"
        exit 1
    fi
    echo "  ✅ $svc_url recor_spiffe_svid_fetch_total{result=\"success\"}=$count"
done

# Hand off to the existing integration-smoke for the round-trip
# assertions. Same HMAC envelopes; AUTH_TRANSPORT=mtls keeps them
# in place as defence-in-depth.
echo ""
echo "── delegating to integration-smoke for the D↔V round-trip ──"
bash "$(dirname "$0")/integration-smoke.sh"

echo ""
echo "✅ R-LOOP-3 MTLS SMOKE: PASS"
echo "   • SPIRE bootstrapped"
echo "   • Both services started under AUTH_TRANSPORT=mtls"
echo "   • SVID-fetch counter incremented for each service"
echo "   • D↔V round-trip succeeded with HMAC (defence-in-depth) layer"
echo ""
echo "Follow-up: when axum::serve is swapped for rustls-terminated"
echo "axum-server, this script will also assert that the HMAC header"
echo "becomes unnecessary under AUTH_TRANSPORT=mtls-only."
