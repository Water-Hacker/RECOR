#!/usr/bin/env bash
# tests/contract/observability-smoke.test.sh
#
# Contract test for F-007 DoD: "Traces flow from a dev service; dashboards
# render."
#
# Wraps infrastructure/observability-dev/smoke-test.sh with an explicit
# pass/fail signal suitable for CI. Always tears the stack down regardless
# of outcome (no RECOR_OBS_KEEP_RUNNING in contract runs).

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
SMOKE="${REPO_ROOT}/infrastructure/observability-dev/smoke-test.sh"

if [ ! -x "$SMOKE" ]; then
    echo "ERROR: $SMOKE is not executable." >&2
    exit 2
fi

# Ensure tear-down on success and failure alike.
unset RECOR_OBS_KEEP_RUNNING

# Ephemeral admin password — never logs, never persisted.
export RECOR_GRAFANA_ADMIN_PASSWORD="$(openssl rand -base64 24 2>/dev/null || head -c 24 /dev/urandom | base64)"

printf 'RÉCOR observability smoke contract test\n'
printf 'Wrapping %s\n\n' "$SMOKE"

if "$SMOKE"; then
    printf '\nOK — F-007 DoD holds.\n'
    exit 0
else
    rc=$?
    printf '\nFAIL — F-007 DoD broken (smoke exit %d).\n' "$rc" >&2
    exit "$rc"
fi
