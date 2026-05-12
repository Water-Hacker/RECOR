#!/usr/bin/env bash
# tests/contract/codeowners.test.sh
#
# Contract test for CODEOWNERS validity.
#
# Asserts two things:
#   (a) The real /.github/CODEOWNERS validates clean (exit 0)
#   (b) The deliberately-broken fixture at
#       tests/contract/fixtures/codeowners-bad/ FAILS validation (exit ≠ 0)
#
# Property (b) is the meta-test: it proves the validator is not vacuously
# permissive. A validator that returns 0 on the bad fixture is itself a
# defect — D14 instantiated at the validator level (fail-closed at the
# governance boundary).

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
VALIDATOR="${REPO_ROOT}/tools/ci/validate-codeowners.sh"
BAD_FIXTURE="${REPO_ROOT}/tests/contract/fixtures/codeowners-bad"

if [ ! -x "$VALIDATOR" ]; then
    echo "ERROR: $VALIDATOR is not executable." >&2
    exit 2
fi

printf 'RÉCOR CODEOWNERS contract test\n'

# (a) Real CODEOWNERS validates clean
printf '\n── (a) Real CODEOWNERS validates clean ──\n'
if "$VALIDATOR" >/dev/null 2>&1; then
    printf '  PASS  real CODEOWNERS exits 0\n'
else
    printf '  FAIL  real CODEOWNERS did not exit 0\n' >&2
    "$VALIDATOR" >&2 || true
    exit 1
fi

# (b) Bad fixture is rejected
printf '\n── (b) Bad fixture is rejected ──\n'
if [ ! -d "$BAD_FIXTURE" ]; then
    printf '  FAIL  fixture directory missing: %s\n' "$BAD_FIXTURE" >&2
    exit 1
fi

set +e
REPO_ROOT="$BAD_FIXTURE" "$VALIDATOR" >/dev/null 2>&1
rc=$?
set -e

if [ "$rc" -eq 0 ]; then
    printf '  FAIL  validator passed the bad fixture (exit 0); validator is too lax\n' >&2
    exit 1
fi
printf '  PASS  validator rejected the bad fixture (exit=%d)\n' "$rc"

printf '\nOK — CODEOWNERS contract holds.\n'
exit 0
