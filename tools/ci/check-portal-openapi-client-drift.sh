#!/usr/bin/env bash
# tools/ci/check-portal-openapi-client-drift.sh
#
# R-PORT-7: assert that the committed generated TypeScript client
# (`applications/declarant-portal/src/generated/openapi.ts`) matches
# what `pnpm openapi:gen` produces from the committed OpenAPI
# snapshot at `docs/openapi/declaration.json`.
#
# Run on every PR (see `.github/workflows/required-checks.yaml`, job
# `portal / openapi-client-drift`).
#
# Exit codes:
#   0 — snapshot matches generator output
#   1 — drift detected (the diff is printed)
#   2 — environment or tooling problem (pnpm missing, generator
#       failed, etc.)
#
# Fail-closed per D14: any failure to regenerate the client is treated
# as drift, never silently passed. There is no `--allow-drift` flag.
# The fix is always "run `pnpm openapi:gen`, commit the result".
#
# Usage:
#   ./tools/ci/check-portal-openapi-client-drift.sh           # check
#   UPDATE=1 ./tools/ci/check-portal-openapi-client-drift.sh  # regen
#                                                              in place
#                                                              (CI never
#                                                              sets
#                                                              UPDATE=1)

set -euo pipefail

REPO_ROOT=$(cd "$(dirname "$0")/../.." && pwd)
cd "$REPO_ROOT"

PORTAL_DIR="applications/declarant-portal"
SPEC="docs/openapi/declaration.json"
SNAPSHOT="$PORTAL_DIR/src/generated/openapi.ts"

if [[ ! -f "$SPEC" ]]; then
    echo "::error::OpenAPI spec not found at $SPEC" >&2
    echo "       run tools/ci/check-openapi-drift.sh first to seed it" >&2
    exit 2
fi

if [[ ! -f "$SNAPSHOT" ]]; then
    echo "::error::generated client not found at $SNAPSHOT" >&2
    echo "       seed it with: UPDATE=1 $0" >&2
    exit 2
fi

if ! command -v pnpm >/dev/null 2>&1; then
    echo "::error::pnpm not on PATH; cannot regenerate client" >&2
    exit 2
fi

# Ensure the portal's node_modules has openapi-typescript available.
# `pnpm install --frozen-lockfile` is idempotent and a no-op when the
# tree is already up to date; in CI the previous step usually handles
# this, but running it here makes the script self-contained for
# developer use.
if [[ ! -d "$PORTAL_DIR/node_modules" ]]; then
    echo "==> installing portal dependencies (first run)"
    (cd "$PORTAL_DIR" && pnpm install --frozen-lockfile --silent)
fi

echo "==> regenerating $SNAPSHOT from $SPEC"

if [[ "${UPDATE:-0}" == "1" ]]; then
    # Regenerate in place. The npm script writes to the committed
    # location directly; no temp file needed.
    if ! (cd "$PORTAL_DIR" && pnpm --silent openapi:gen); then
        echo "::error::openapi-typescript failed; cannot regenerate" >&2
        exit 2
    fi
    echo "==> updated $SNAPSHOT"
    exit 0
fi

# Drift-check mode: generate to a temp file and diff against the
# committed snapshot. We do NOT overwrite the committed file in this
# mode — a CI run must not produce side effects.
GENERATED=$(mktemp -t recor-portal-openapi.XXXXXX.ts)
trap 'rm -f "$GENERATED"' EXIT

if ! (cd "$PORTAL_DIR" && \
      pnpm --silent exec openapi-typescript "../../$SPEC" -o "$GENERATED"); then
    echo "::error::openapi-typescript failed; cannot check drift" >&2
    exit 2
fi

# Sanity check the generator actually wrote a non-empty TypeScript
# file. A zero-byte output would otherwise diff cleanly against a
# nuked snapshot and silently pass.
if [[ ! -s "$GENERATED" ]]; then
    echo "::error::openapi-typescript produced an empty file" >&2
    exit 2
fi

if diff -u "$SNAPSHOT" "$GENERATED"; then
    echo "==> generated client matches the committed snapshot: OK"
    exit 0
else
    echo "" >&2
    echo "::error::portal OpenAPI client drift detected" >&2
    echo "        the committed client at $SNAPSHOT is out of date" >&2
    echo "        with respect to $SPEC." >&2
    echo "        regenerate locally and commit:" >&2
    echo "" >&2
    echo "            UPDATE=1 $0" >&2
    echo "            git add $SNAPSHOT" >&2
    echo "" >&2
    exit 1
fi
