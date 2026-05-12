#!/usr/bin/env bash
# tools/ci/validate-codeowners.sh
#
# RÉCOR R-001 CODEOWNERS validator.
#
# Checks:
#   1. .github/CODEOWNERS exists and parses as valid CODEOWNERS syntax
#      (path + at least one owner per non-comment, non-blank line)
#   2. Every team mentioned anywhere in CODEOWNERS appears in
#      docs/security/teams.md (no orphan team references — D08)
#   3. Every team listed in docs/security/teams.md appears in CODEOWNERS
#      at least once (no unused team registrations — D08, reverse)
#   4. Every top-level domain directory has at least one explicit rule:
#      services/, applications/, libraries/, contracts/, infrastructure/,
#      policies/, docs/, .claude/, .github/, tools/, tests/
#   5. STRICTER directories carry ≥2 owners (multi-team required):
#         policies/
#         contracts/
#         services/frost-coordinator/
#         services/chaincode/
#         libraries/rust/recor-crypto/
#         libraries/rust/recor-hsm/
#         libraries/rust/recor-frost/
#         libraries/rust/recor-zk/
#         infrastructure/ansible/
#         docs/adversarial-corpus/
#   6. The default catch-all `*` rule exists (no-orphan-file invariant)
#
# Override entry point: set REPO_ROOT to validate a different tree (used by
# the negative-fixture test that exercises rejection).
#
# Exit codes:
#   0  all checks pass
#   1  one or more checks failed (specifics printed to stderr)
#   2  prerequisite tool missing (grep, awk) — environment problem

set -uo pipefail

REPO_ROOT="${REPO_ROOT:-$(git rev-parse --show-toplevel 2>/dev/null || pwd)}"
CODEOWNERS="${REPO_ROOT}/.github/CODEOWNERS"
TEAMS_DOC="${REPO_ROOT}/docs/security/teams.md"

fail_count=0
check_count=0

red()    { printf '\033[31m%s\033[0m' "$*"; }
green()  { printf '\033[32m%s\033[0m' "$*"; }
yellow() { printf '\033[33m%s\033[0m' "$*"; }

ok() {
    check_count=$((check_count + 1))
    printf '  %s  %s\n' "$(green PASS)" "$1"
}

bad() {
    check_count=$((check_count + 1))
    fail_count=$((fail_count + 1))
    printf '  %s  %s\n' "$(red FAIL)" "$1" >&2
    if [ -n "${2:-}" ]; then
        printf '          %s\n' "$2" >&2
    fi
}

section() {
    printf '\n%s %s\n' "$(yellow '──')" "$1"
}

for tool in grep awk sort comm; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        printf 'Required tool missing: %s\n' "$tool" >&2
        exit 2
    fi
done

printf 'RÉCOR CODEOWNERS validator\n'
printf 'Repository root: %s\n' "$REPO_ROOT"
printf 'CODEOWNERS:       %s\n' "$CODEOWNERS"
printf 'Teams doc:        %s\n' "$TEAMS_DOC"

# ─── Check 1: CODEOWNERS exists and parses ──────────────────────────────────
section "Check 1: .github/CODEOWNERS exists and parses"
if [ ! -f "$CODEOWNERS" ]; then
    bad "CODEOWNERS present" "expected $CODEOWNERS"
    printf '\n%s\n' "$(red FAIL)"
    exit 1
fi
ok "CODEOWNERS file present"

# Each non-comment, non-blank line must have:
#   <path-pattern>  <whitespace>  <one-or-more-@owners>
parse_errors=0
while IFS= read -r raw; do
    # Strip inline comments (everything after a '#')
    line="${raw%%#*}"
    # Trim leading/trailing whitespace
    line="$(printf '%s' "$line" | awk '{$1=$1; print}')"
    [ -z "$line" ] && continue
    # Path is the first token; owners are the rest. Owners must be @-prefixed.
    path_tok="${line%% *}"
    owners="${line#* }"
    if [ "$path_tok" = "$line" ]; then
        # Only one token on the line — no owners
        printf '::error::CODEOWNERS line missing owners: %s\n' "$raw" >&2
        parse_errors=$((parse_errors + 1))
        continue
    fi
    # Verify at least one @-prefixed owner
    if ! printf '%s' "$owners" | grep -qE '@[A-Za-z0-9-]+(/[A-Za-z0-9._-]+)?'; then
        printf '::error::CODEOWNERS line has no valid @owner: %s\n' "$raw" >&2
        parse_errors=$((parse_errors + 1))
    fi
done < "$CODEOWNERS"

if [ "$parse_errors" -eq 0 ]; then
    ok "CODEOWNERS syntax parses ($(grep -cvE '^[[:space:]]*(#|$)' "$CODEOWNERS") rules)"
else
    bad "CODEOWNERS syntax parses" "$parse_errors line(s) malformed"
fi

# Extract all team mentions (anything matching @org/team) — unique, sorted.
extract_teams() {
    grep -oE '@[A-Za-z0-9-]+/[A-Za-z0-9._-]+' "$1" | sort -u
}

codeowners_teams=$(extract_teams "$CODEOWNERS")

# ─── Check 2 + 3: two-way team reference ────────────────────────────────────
section "Check 2+3: two-way reference CODEOWNERS ↔ docs/security/teams.md"
if [ ! -f "$TEAMS_DOC" ]; then
    bad "docs/security/teams.md present" "expected $TEAMS_DOC"
else
    ok "teams.md present"
    teams_doc_teams=$(extract_teams "$TEAMS_DOC")

    # Orphan team references in CODEOWNERS (in CODEOWNERS but not in teams.md)
    orphans=$(comm -23 <(printf '%s\n' "$codeowners_teams") <(printf '%s\n' "$teams_doc_teams"))
    if [ -n "$orphans" ]; then
        bad "every CODEOWNERS team appears in teams.md" \
            "orphans: $(printf '%s' "$orphans" | tr '\n' ' ')"
    else
        ok "every CODEOWNERS team is documented in teams.md"
    fi

    # Unused team registrations in teams.md (in teams.md but not in CODEOWNERS)
    unused=$(comm -13 <(printf '%s\n' "$codeowners_teams") <(printf '%s\n' "$teams_doc_teams"))
    if [ -n "$unused" ]; then
        bad "every teams.md team is referenced in CODEOWNERS" \
            "unused: $(printf '%s' "$unused" | tr '\n' ' ')"
    else
        ok "every teams.md team is used in CODEOWNERS"
    fi
fi

# ─── Check 4: every top-level directory has a rule ─────────────────────────
section "Check 4: every top-level domain directory has a rule"
required_top_dirs=(
    "services/"
    "applications/"
    "libraries/"
    "contracts/"
    "infrastructure/"
    "policies/"
    "docs/"
    ".claude/"
    ".github/"
    "tools/"
    "tests/"
)
for d in "${required_top_dirs[@]}"; do
    if grep -qE "^/$d" "$CODEOWNERS"; then
        ok "rule exists for /$d"
    else
        bad "rule exists for /$d" "no '/$d' rule found in CODEOWNERS"
    fi
done

# ─── Check 5: stricter directories carry ≥2 owners ─────────────────────────
section "Check 5: stricter directories carry ≥2 owners"
declare -a stricter_dirs=(
    "policies/"
    "contracts/"
    "services/frost-coordinator/"
    "services/chaincode/"
    "libraries/rust/recor-crypto/"
    "libraries/rust/recor-hsm/"
    "libraries/rust/recor-frost/"
    "libraries/rust/recor-zk/"
    "infrastructure/ansible/"
    "docs/adversarial-corpus/"
)
for d in "${stricter_dirs[@]}"; do
    # Find the line for /<d>; count owners on it.
    matched_line=$(grep -E "^/${d}[[:space:]]" "$CODEOWNERS" | head -1)
    if [ -z "$matched_line" ]; then
        bad "stricter rule for /$d" "no rule found"
        continue
    fi
    owner_count=$(printf '%s' "$matched_line" \
        | grep -oE '@[A-Za-z0-9-]+/[A-Za-z0-9._-]+' \
        | wc -l)
    if [ "$owner_count" -lt 2 ]; then
        bad "stricter rule for /$d has ≥2 owners" \
            "found $owner_count owner(s) on: $matched_line"
    else
        ok "stricter /$d has $owner_count owners"
    fi
done

# ─── Check 6: default catch-all `*` exists ─────────────────────────────────
section "Check 6: default catch-all '*' rule exists"
if grep -qE '^\*[[:space:]]+@' "$CODEOWNERS"; then
    ok "catch-all '*' rule present"
else
    bad "catch-all '*' rule present" "no '*' line found; some files would be unowned"
fi

# ─── Summary ────────────────────────────────────────────────────────────────
printf '\n──── Summary ────\n'
printf 'Checks run    : %d\n' "$check_count"
printf 'Failures      : %d\n' "$fail_count"

if [ "$fail_count" -gt 0 ]; then
    printf '%s — CODEOWNERS has defects.\n' "$(red FAIL)"
    exit 1
fi

printf '%s — CODEOWNERS validates.\n' "$(green OK)"
exit 0
