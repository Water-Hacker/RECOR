#!/usr/bin/env bash
# tools/ci/check-openapi-drift.sh
#
# DOC-1: assert that the committed OpenAPI snapshot
# (`docs/openapi/declaration.json`) matches what the current build
# produces. Run on every PR (see `.github/workflows/required-checks.yaml`,
# job `api / openapi-drift`).
#
# Exit codes:
#   0 — snapshot matches build
#   1 — drift detected (the diff is printed)
#   2 — environment or tooling problem (e.g. cargo missing,
#       binary failed to produce output)
#
# Fail-closed per D14: any failure to regenerate the spec is treated
# as drift, never silently passed. There is no `--allow-drift` flag.
# The fix is always "run the build, copy the spec, commit it".
#
# Usage:
#   ./tools/ci/check-openapi-drift.sh           # one-shot check
#   UPDATE=1 ./tools/ci/check-openapi-drift.sh  # regenerate the snapshot
#                                                in place (developer
#                                                ergonomics; CI never
#                                                sets UPDATE=1)

set -euo pipefail

REPO_ROOT=$(cd "$(dirname "$0")/../.." && pwd)
cd "$REPO_ROOT"

# R-DECL-7: declaration crate's sqlx::query! macros need either a live
# DATABASE_URL or the committed offline cache at services/declaration/.sqlx/.
# This script is pure-build (no DB), so force offline mode.
export SQLX_OFFLINE="${SQLX_OFFLINE:-true}"

# FIND-013: V-engine joins declaration in the drift gate. Add an entry
# here for each service that ships an OpenAPI snapshot. Tuple shape:
#   <service-display-name>:<cargo-package>:<cargo-bin>:<snapshot-path>
SERVICES=(
    "declaration:recor-declaration:dump-openapi:docs/openapi/declaration.json"
    "verification-engine:recor-verification-engine:dump-openapi-verification-engine:docs/openapi/verification-engine.json"
)

GENERATED=$(mktemp -t recor-openapi.XXXXXX.json)
trap 'rm -f "$GENERATED"' EXIT

drift_detected=0

for entry in "${SERVICES[@]}"; do
    IFS=':' read -r service pkg bin snapshot <<<"$entry"

    if [[ ! -f "$snapshot" ]]; then
        echo "::error::committed OpenAPI snapshot not found at $snapshot" >&2
        echo "       seed it with: UPDATE=1 $0" >&2
        exit 2
    fi

    echo "==> [$service] regenerating OpenAPI spec via 'cargo run --bin $bin'"

    if ! cargo run \
            --quiet \
            --manifest-path Cargo.toml \
            --package "$pkg" \
            --bin "$bin" \
            > "$GENERATED"; then
        echo "::error::[$service] $bin binary failed; cannot check drift" >&2
        exit 2
    fi

    if ! python3 -c "import json,sys; json.load(open('$GENERATED'))" 2>/dev/null; then
        echo "::error::[$service] $bin produced non-JSON output" >&2
        echo "first 20 lines of output:" >&2
        head -20 "$GENERATED" >&2
        exit 2
    fi

    if [[ "${UPDATE:-0}" == "1" ]]; then
        cp "$GENERATED" "$snapshot"
        echo "==> [$service] updated $snapshot"
        continue
    fi

    if diff -u "$snapshot" "$GENERATED"; then
        echo "==> [$service] OpenAPI snapshot matches build output: OK"
    else
        echo "" >&2
        echo "::error::[$service] OpenAPI spec drift detected" >&2
        echo "        the committed snapshot at $snapshot is out of date." >&2
        echo "        regenerate locally and commit:" >&2
        echo "" >&2
        echo "            UPDATE=1 $0" >&2
        echo "            git add $snapshot" >&2
        echo "" >&2
        drift_detected=1
    fi
done

if [[ "${UPDATE:-0}" == "1" ]]; then
    exit 0
fi
exit "$drift_detected"
