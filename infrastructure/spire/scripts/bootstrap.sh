#!/usr/bin/env bash
# infrastructure/spire/scripts/bootstrap.sh
#
# Bootstraps the dev SPIRE deployment:
#   1. Brings the server + agent up (idempotent — re-running is safe).
#   2. Generates a single-use join token from the server.
#   3. Feeds the token to the agent so it can attest on first start.
#   4. Loads every registration entry under
#      infrastructure/spire/registration-entries/ via
#      `spire-server entry create`.
#
# Re-runnability: existing entries are detected via
# `spire-server entry show -spiffeID …` and skipped; the script never
# re-creates an entry that already exists. The join-token step is
# idempotent because the token is consumed on first agent attestation;
# subsequent agent restarts use the persisted SVID.
#
# Usage:
#   bash infrastructure/spire/scripts/bootstrap.sh
#
# Requires: docker, docker compose. No host-side spire-server binary
# is needed — every spire-server command runs inside the server
# container via `docker compose exec`.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SPIRE_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
COMPOSE_FILE="$SPIRE_DIR/docker-compose.yaml"
ENTRIES_DIR="$SPIRE_DIR/registration-entries"
TRUST_DOMAIN="recor.cm"
AGENT_ID_SUFFIX="recor-dev-agent"

echo "── SPIRE bootstrap (dev) ──"
echo "  compose file:  $COMPOSE_FILE"
echo "  trust domain:  $TRUST_DOMAIN"
echo "  entries dir:   $ENTRIES_DIR"
echo ""

# ─── 1. Compose up ────────────────────────────────────────────────────
echo "── compose up server (agent waits on server health) ──"
docker compose -f "$COMPOSE_FILE" up -d spire-server

# Wait for the server's healthcheck.
echo "── waiting for spire-server ──"
for i in {1..30}; do
    if docker compose -f "$COMPOSE_FILE" exec -T spire-server \
        /opt/spire/bin/spire-server healthcheck \
        -socketPath /tmp/spire-server/private/api.sock >/dev/null 2>&1; then
        echo "  spire-server healthy after ${i}s"
        break
    fi
    sleep 1
done

# ─── 2. Generate the agent's join token (idempotent) ──────────────────
# If the agent has already attested (persisted SVID under
# /run/spire/data inside the agent volume), this step is unnecessary
# but harmless: the second token simply never gets consumed.
echo ""
echo "── generating join token for agent ($AGENT_ID_SUFFIX) ──"
TOKEN_OUT=$(docker compose -f "$COMPOSE_FILE" exec -T spire-server \
    /opt/spire/bin/spire-server token generate \
    -spiffeID "spiffe://${TRUST_DOMAIN}/spire/agent/join_token/${AGENT_ID_SUFFIX}" \
    -socketPath /tmp/spire-server/private/api.sock || true)
TOKEN=$(echo "$TOKEN_OUT" | awk '/Token:/ {print $2}' | tr -d '\r\n')
if [ -z "$TOKEN" ]; then
    echo "  WARN: failed to generate join token; assuming agent already attested"
else
    echo "  token: ${TOKEN:0:8}…"
    # Write the token into the agent's data volume so it picks it up
    # on first start. We do this via a short-lived helper container
    # that mounts the same volume.
    docker run --rm \
        -v "$(docker compose -f "$COMPOSE_FILE" config --volumes | grep agent-data | head -1 || echo recor-spire_spire-agent-data):/data" \
        alpine:3.19 sh -c "echo -n '$TOKEN' > /data/agent.token" || \
        echo "  WARN: token write failed; agent may already be attested"
fi

# ─── 3. Bring the agent up ─────────────────────────────────────────────
echo ""
echo "── compose up agent ──"
docker compose -f "$COMPOSE_FILE" up -d spire-agent

echo "── waiting for spire-agent ──"
for i in {1..30}; do
    if docker compose -f "$COMPOSE_FILE" exec -T spire-agent \
        /opt/spire/bin/spire-agent healthcheck \
        -socketPath /tmp/spire-agent/public/api.sock >/dev/null 2>&1; then
        echo "  spire-agent healthy after ${i}s"
        break
    fi
    sleep 1
done

# ─── 4. Load registration entries ──────────────────────────────────────
echo ""
echo "── loading registration entries ──"
for f in "$ENTRIES_DIR"/*.json; do
    [ -f "$f" ] || continue
    # Skip metadata-only files.
    name="$(basename "$f")"
    spiffe_id=$(grep -E '"spiffe_id"' "$f" | head -1 | sed -E 's/.*"spiffe_id"[[:space:]]*:[[:space:]]*"([^"]+)".*/\1/')
    parent_id=$(grep -E '"parent_id"' "$f" | head -1 | sed -E 's/.*"parent_id"[[:space:]]*:[[:space:]]*"([^"]+)".*/\1/')
    if [ -z "$spiffe_id" ] || [ -z "$parent_id" ]; then
        echo "  $name: malformed (missing spiffe_id or parent_id); skipping"
        continue
    fi

    # Extract every selector value into the -selector args.
    selectors=()
    while IFS= read -r sel; do
        selectors+=("-selector" "$sel")
    done < <(grep -oE '"docker:[^"]+"|"unix:[^"]+"|"k8s:[^"]+"' "$f" | tr -d '"')

    # Idempotency: skip if an entry with this SPIFFE ID already exists.
    if docker compose -f "$COMPOSE_FILE" exec -T spire-server \
        /opt/spire/bin/spire-server entry show \
        -spiffeID "$spiffe_id" \
        -socketPath /tmp/spire-server/private/api.sock 2>/dev/null \
        | grep -q "Entry ID"; then
        echo "  $name: entry for $spiffe_id already exists; skipping"
        continue
    fi

    echo "  $name: creating entry for $spiffe_id"
    docker compose -f "$COMPOSE_FILE" exec -T spire-server \
        /opt/spire/bin/spire-server entry create \
        -spiffeID "$spiffe_id" \
        -parentID "$parent_id" \
        "${selectors[@]}" \
        -socketPath /tmp/spire-server/private/api.sock \
        || echo "  $name: entry create returned non-zero (may already exist)"
done

echo ""
echo "── done ──"
echo "  spire-agent Workload API socket (inside the spire network):"
echo "    unix:///tmp/spire-agent/public/api.sock"
echo "  for host-side cargo-run workloads, mount the named volume"
echo "  spire-agent-socket to the same path."
