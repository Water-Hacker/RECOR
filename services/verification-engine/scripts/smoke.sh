#!/usr/bin/env bash
# End-to-end smoke for recor-verification-engine.
# Seeds the mock BUNEC, builds two test declarations (one with both
# beneficial owners present in BUNEC, one with a ghost owner), posts
# each, observes the lane decision.

set -euo pipefail
cd "$(dirname "$0")/.."

if [ ! -f .env ]; then
    # hex (not base64) to avoid URL-special characters in DATABASE_URL
    echo "RECOR_DB_PASSWORD=$(openssl rand -hex 24)" > .env
    echo "[generated .env]"
fi

echo "── compose up ──"
docker compose up -d --build

echo "── wait for ready ──"
for i in {1..60}; do
    if curl -fsS http://127.0.0.1:8081/healthz >/dev/null 2>&1; then
        echo "ready after ${i}s"
        break
    fi
    sleep 1
done

PRINCIPAL="spiffe://recor.cm/smoke"
DB_PW="$(grep RECOR_DB_PASSWORD .env | cut -d= -f2-)"

# Three UUIDs.
PERSON_FOUND_1=$(cat /proc/sys/kernel/random/uuid)
PERSON_FOUND_2=$(cat /proc/sys/kernel/random/uuid)
PERSON_GHOST=$(cat /proc/sys/kernel/random/uuid)

# Seed the mock BUNEC.
echo "── seed mock BUNEC ──"
docker compose exec -T -e PGPASSWORD="$DB_PW" postgres \
    psql -U recor -d verification <<EOF
INSERT INTO mock_bunec_persons (person_id, canonical_full_name, nationality)
VALUES
    ('$PERSON_FOUND_1', 'Aïssa Ngo Bidoung', 'CM'),
    ('$PERSON_FOUND_2', 'Patrice Mboko',     'CM')
ON CONFLICT (person_id) DO NOTHING;
SELECT count(*) AS bunec_records FROM mock_bunec_persons;
EOF

submit_case() {
    local label="$1" owners_json="$2" expected_lane_regex="$3"
    local decl_id="$(cat /proc/sys/kernel/random/uuid)"
    local entity_id="$(cat /proc/sys/kernel/random/uuid)"
    echo ""
    echo "── case $label ──"
    local body
    body=$(jq -c -n \
        --arg decl_id "$decl_id" \
        --arg entity_id "$entity_id" \
        --arg principal "$PRINCIPAL" \
        --argjson owners "$owners_json" \
        '{
          declaration: {
            declaration_id: $decl_id,
            entity_id: $entity_id,
            declarant_principal: $principal,
            declarant_role: "self",
            kind: "incorporation",
            effective_from: "2026-01-01",
            beneficial_owners: $owners,
            attestation_signed_by: $principal,
            attestation_signature_hex: "00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
            attestation_public_key_hex: "0000000000000000000000000000000000000000000000000000000000000000",
            receipt_hash_hex: "0000000000000000000000000000000000000000000000000000000000000000",
            correlation_id: $decl_id,
            submitted_at: "2026-05-11T22:00:00Z"
          }
        }')
    local resp
    resp=$(curl -sS -X POST http://127.0.0.1:8081/v1/verifications \
        -H "Content-Type: application/json" \
        -H "X-Recor-Dev-Principal: $PRINCIPAL" \
        -d "$body" \
        -w "\n%{http_code}")
    local http=$(echo "$resp" | tail -1)
    local payload=$(echo "$resp" | sed '$d')
    echo "$payload" | jq '{case_id, lane, authenticity_belief, authenticity_plausibility, risk_belief, total_duration_ms}'
    echo "HTTP $http"
    [ "$http" = "201" ] || { echo "FAIL case $label: expected 201"; exit 1; }
    local lane=$(echo "$payload" | jq -r .lane)
    echo "lane: $lane (expected matching /$expected_lane_regex/)"
    if ! echo "$lane" | grep -qE "$expected_lane_regex"; then
        echo "FAIL case $label: lane '$lane' does not match expected pattern '$expected_lane_regex'"
        exit 1
    fi
    local case_id=$(echo "$payload" | jq -r .case_id)
    echo "── GET /v1/verifications/$case_id (summary) ──"
    curl -sS http://127.0.0.1:8081/v1/verifications/$case_id \
        -H "X-Recor-Dev-Principal: $PRINCIPAL" \
        | jq '{
              case_id,
              lane,
              stage_count: (.stage_outcomes | length),
              stage_summary: [.stage_outcomes[] | {stage_id, kind, duration_ms}]
          }'
}

# Note: the verification engine does NOT re-verify the Ed25519 signature
# — that's the Declaration service's job at intake. Stage 1 only checks
# structural well-formedness (signature is 64-byte hex, public key is
# 32-byte hex). All-zeros filler satisfies that surface.

submit_case "A: both owners in BUNEC" \
    "[{\"person_id\":\"$PERSON_FOUND_1\",\"ownership_basis_points\":6000,\"interest_kind\":\"equity\"},{\"person_id\":\"$PERSON_FOUND_2\",\"ownership_basis_points\":4000,\"interest_kind\":\"equity\"}]" \
    "(green|yellow)"

submit_case "B: BUNEC-ghost owner (red expected)" \
    "[{\"person_id\":\"$PERSON_GHOST\",\"ownership_basis_points\":10000,\"interest_kind\":\"equity\"}]" \
    "red"

echo ""
echo "✅ smoke pass"
