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

SNAPSHOT="docs/openapi/declaration.json"

if [[ ! -f "$SNAPSHOT" ]]; then
    echo "::error::committed OpenAPI snapshot not found at $SNAPSHOT" >&2
    echo "       seed it with: UPDATE=1 $0" >&2
    exit 2
fi

# Stand up a temp file for the build's output. `trap` makes the
# cleanup unconditional even on early `set -e` exits.
GENERATED=$(mktemp -t recor-openapi.XXXXXX.json)
trap 'rm -f "$GENERATED"' EXIT

echo "==> regenerating OpenAPI spec via 'cargo run --bin dump-openapi'"

# `cargo run --quiet` still prints build output to stderr; we capture
# stdout only. The binary itself is pure (no DB, no network), so the
# only failure modes are compile errors and the serde_json::to_string
# fallback inside main().
if ! cargo run \
        --quiet \
        --manifest-path Cargo.toml \
        --package recor-declaration \
        --bin dump-openapi \
        > "$GENERATED"; then
    echo "::error::dump-openapi binary failed; cannot check drift" >&2
    exit 2
fi

# Sanity check the binary actually produced JSON.
if ! python3 -c "import json,sys; json.load(open('$GENERATED'))" 2>/dev/null; then
    echo "::error::dump-openapi produced non-JSON output" >&2
    echo "first 20 lines of output:" >&2
    head -20 "$GENERATED" >&2
    exit 2
fi

if [[ "${UPDATE:-0}" == "1" ]]; then
    cp "$GENERATED" "$SNAPSHOT"
    echo "==> updated $SNAPSHOT"
    exit 0
fi

if diff -u "$SNAPSHOT" "$GENERATED"; then
    echo "==> OpenAPI snapshot matches build output: OK"
    exit 0
else
    echo "" >&2
    echo "::error::OpenAPI spec drift detected" >&2
    echo "        the committed snapshot at $SNAPSHOT is out of date." >&2
    echo "        regenerate locally and commit:" >&2
    echo "" >&2
    echo "            UPDATE=1 $0" >&2
    echo "            git add $SNAPSHOT" >&2
    echo "" >&2
    exit 1
fi
