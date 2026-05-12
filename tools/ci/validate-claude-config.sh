#!/usr/bin/env bash
# tools/ci/validate-claude-config.sh
#
# RÉCOR R-002 smoke validator for the .claude/ configuration surface.
#
# Validates, with structural-defect-on-first-failure semantics:
#   1. .claude/settings.json parses as valid JSON
#   2. Every .claude/agents/*.md has valid YAML frontmatter
#      with required fields (name, description, model)
#   3. The 'name' field of every agent file matches the file's basename
#      (without .md extension)
#   4. Every .claude/skills/*/SKILL.md has valid YAML frontmatter
#      with required fields (name, description)
#   5. Every skill directory contains a SKILL.md (no empty skill dirs)
#   6. The 'name' field of every skill matches its parent directory name
#   7. Every .claude/hooks/*.sh is executable AND bash-syntax-clean (bash -n)
#   8. No .gitkeep files remain anywhere under .claude/
#   9. No file under .claude/ contains an Anthropic API key pattern
#      (sk-ant-...) or generic 'sk_'-prefixed secret pattern
#  10. Doctrine reference sanity: at least one agent and one skill mention
#      'Doctrine' (loose check that the doctrine framework is wired in)
#
# Exit codes:
#   0  all checks pass
#   1  one or more checks failed (specific failures printed to stderr)
#   2  prerequisite tool missing (jq, python3) — environment problem,
#      not a configuration defect
#
# Doctrines exercised by this validator:
#   D04 — this is the test for R-002
#   D08 — no dangling .gitkeep markers under .claude/
#   D18 — no secrets anywhere in .claude/

set -uo pipefail

REPO_ROOT="${REPO_ROOT:-$(git rev-parse --show-toplevel 2>/dev/null || pwd)}"
CLAUDE_DIR="${REPO_ROOT}/.claude"

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

# Prerequisite check
for tool in jq python3; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        printf 'Required tool missing: %s\n' "$tool" >&2
        exit 2
    fi
done

if [ ! -d "$CLAUDE_DIR" ]; then
    printf 'FAIL: %s does not exist\n' "$CLAUDE_DIR" >&2
    exit 1
fi

printf 'RÉCOR Claude Code config validator\n'
printf 'Repository root: %s\n' "$REPO_ROOT"

# --- Check 1: settings.json parses ---
section "Check 1: .claude/settings.json parses as JSON"
settings_file="${CLAUDE_DIR}/settings.json"
if [ ! -f "$settings_file" ]; then
    bad "settings.json present" "expected $settings_file"
elif ! jq empty "$settings_file" 2>/dev/null; then
    bad "settings.json parses" "$(jq empty "$settings_file" 2>&1 | head -1)"
else
    ok "settings.json parses"
    # Additional structural assertions on the settings shape
    for key in permissions hooks subagents skills; do
        if ! jq -e ".\"${key}\"" "$settings_file" >/dev/null 2>&1; then
            bad "settings.json has top-level '$key' key"
        else
            ok "settings.json has '$key' key"
        fi
    done
    for permkey in allow deny ask; do
        if ! jq -e ".permissions.\"${permkey}\" | type == \"array\"" "$settings_file" >/dev/null 2>&1; then
            bad "settings.json: permissions.$permkey is an array"
        else
            ok "settings.json: permissions.$permkey is an array"
        fi
    done
fi

# --- Check 2 + 3: agents have valid frontmatter; name matches filename ---
section "Check 2+3: agent frontmatter valid; agent.name == basename(file)"
agents_dir="${CLAUDE_DIR}/agents"
if [ ! -d "$agents_dir" ]; then
    bad "agents/ directory exists"
else
    agent_count=0
    while IFS= read -r -d '' agent_file; do
        agent_count=$((agent_count + 1))
        # Skip README.md — it's not an agent definition
        if [ "$(basename "$agent_file")" = "README.md" ]; then
            continue
        fi
        basename_no_ext="$(basename "$agent_file" .md)"

        # Parse frontmatter via python (YAML)
        py_output=$(python3 - "$agent_file" <<'PYEOF' 2>&1
import sys, re
try:
    import yaml
except ImportError:
    print("ERR: pyyaml not installed", file=sys.stderr)
    sys.exit(2)
path = sys.argv[1]
with open(path) as f:
    text = f.read()
m = re.match(r'^---\n(.*?)\n---\n', text, re.DOTALL)
if not m:
    print("ERR: no YAML frontmatter")
    sys.exit(1)
try:
    fm = yaml.safe_load(m.group(1))
except yaml.YAMLError as e:
    print(f"ERR: frontmatter YAML parse: {e}")
    sys.exit(1)
if not isinstance(fm, dict):
    print("ERR: frontmatter is not a dict")
    sys.exit(1)
missing = [k for k in ("name", "description", "model") if k not in fm]
if missing:
    print(f"ERR: missing required field(s): {missing}")
    sys.exit(1)
print(f"OK\t{fm['name']}\t{fm['description'][:80]}")
PYEOF
)
        rc=$?
        if [ $rc -ne 0 ] || [[ "$py_output" == ERR:* ]]; then
            bad "agent frontmatter: $(basename "$agent_file")" "$py_output"
            continue
        fi
        name_field=$(echo "$py_output" | cut -f2)

        if [ "$name_field" != "$basename_no_ext" ]; then
            bad "agent name matches filename: $(basename "$agent_file")" \
                "frontmatter name='$name_field' but filename basename='$basename_no_ext'"
        else
            ok "agent: $name_field"
        fi
    done < <(find "$agents_dir" -maxdepth 1 -name "*.md" -print0)

    if [ "$agent_count" -lt 10 ]; then
        bad "agent count" "expected at least 10 agent files (excluding README); found $agent_count files total"
    else
        ok "agent count: $agent_count files"
    fi
fi

# --- Check 4 + 5 + 6: skills ---
section "Check 4+5+6: each skill has SKILL.md; frontmatter valid; name matches dirname"
skills_dir="${CLAUDE_DIR}/skills"
if [ ! -d "$skills_dir" ]; then
    bad "skills/ directory exists"
else
    skill_count=0
    while IFS= read -r -d '' skill_subdir; do
        # Skip files at the top level (README.md etc.)
        [ -d "$skill_subdir" ] || continue
        # Skip the top-level skills dir itself
        [ "$skill_subdir" = "$skills_dir" ] && continue
        skill_count=$((skill_count + 1))
        dir_name=$(basename "$skill_subdir")
        skill_file="${skill_subdir}/SKILL.md"

        if [ ! -f "$skill_file" ]; then
            bad "skill $dir_name has SKILL.md"
            continue
        fi

        py_output=$(python3 - "$skill_file" <<'PYEOF' 2>&1
import sys, re
try:
    import yaml
except ImportError:
    print("ERR: pyyaml not installed", file=sys.stderr)
    sys.exit(2)
path = sys.argv[1]
with open(path) as f:
    text = f.read()
m = re.match(r'^---\n(.*?)\n---\n', text, re.DOTALL)
if not m:
    print("ERR: no YAML frontmatter")
    sys.exit(1)
try:
    fm = yaml.safe_load(m.group(1))
except yaml.YAMLError as e:
    print(f"ERR: frontmatter YAML parse: {e}")
    sys.exit(1)
if not isinstance(fm, dict):
    print("ERR: frontmatter is not a dict")
    sys.exit(1)
missing = [k for k in ("name", "description") if k not in fm]
if missing:
    print(f"ERR: missing required field(s): {missing}")
    sys.exit(1)
print(f"OK\t{fm['name']}\t{fm['description'][:80]}")
PYEOF
)
        rc=$?
        if [ $rc -ne 0 ] || [[ "$py_output" == ERR:* ]]; then
            bad "skill frontmatter: $dir_name/SKILL.md" "$py_output"
            continue
        fi
        name_field=$(echo "$py_output" | cut -f2)

        if [ "$name_field" != "$dir_name" ]; then
            bad "skill name matches dirname: $dir_name" \
                "frontmatter name='$name_field' but directory name='$dir_name'"
        else
            ok "skill: $name_field"
        fi
    done < <(find "$skills_dir" -mindepth 1 -maxdepth 1 -type d -print0)

    if [ "$skill_count" -lt 11 ]; then
        bad "skill count" "expected at least 11 skill directories; found $skill_count"
    else
        ok "skill count: $skill_count directories"
    fi
fi

# --- Check 7: hooks executable + bash -n clean ---
section "Check 7: hook scripts executable and bash-syntax-clean"
hooks_dir="${CLAUDE_DIR}/hooks"
if [ ! -d "$hooks_dir" ]; then
    bad "hooks/ directory exists"
else
    hook_count=0
    while IFS= read -r -d '' hook_file; do
        hook_count=$((hook_count + 1))
        if [ ! -x "$hook_file" ]; then
            bad "hook executable bit: $(basename "$hook_file")"
            continue
        fi
        if ! bash -n "$hook_file" 2>/dev/null; then
            bad "hook bash syntax: $(basename "$hook_file")" \
                "$(bash -n "$hook_file" 2>&1 | head -1)"
            continue
        fi
        ok "hook: $(basename "$hook_file")"
    done < <(find "$hooks_dir" -maxdepth 1 -name "*.sh" -print0)

    expected_hooks=(pre-edit-doctrine-check.sh pre-bash-allowlist.sh post-edit-format.sh post-bash-audit.sh)
    for h in "${expected_hooks[@]}"; do
        if [ ! -f "${hooks_dir}/${h}" ]; then
            bad "hook present: $h"
        fi
    done
    if [ "$hook_count" -lt 4 ]; then
        bad "hook count" "expected at least 4 hook scripts; found $hook_count"
    fi
fi

# --- Check 8: no .gitkeep remaining under .claude/ ---
section "Check 8: no .gitkeep markers under .claude/"
gitkeep_count=$(find "$CLAUDE_DIR" -name ".gitkeep" -type f 2>/dev/null | wc -l)
if [ "$gitkeep_count" -gt 0 ]; then
    bad ".gitkeep files removed under .claude/" \
        "found $gitkeep_count remaining: $(find "$CLAUDE_DIR" -name ".gitkeep" -type f | tr '\n' ' ')"
else
    ok "no .gitkeep files under .claude/"
fi

# --- Check 9: no API-key patterns anywhere under .claude/ ---
section "Check 9: no Anthropic/secret key patterns under .claude/ (D18)"
# Patterns:
#   sk-ant-...   Anthropic API keys
#   sk_...       generic Stripe / SendGrid / other 'sk_'-prefixed key conventions
# Use grep -r with anchored regex; the validator file itself contains the
# patterns by definition, so exclude self.
self_file="$(realpath "$0")"
secret_hits=$(grep -rE \
    -e 'sk-ant-[A-Za-z0-9_-]{20,}' \
    -e '\bsk_[A-Za-z0-9_-]{20,}' \
    --exclude-dir=.git \
    "$CLAUDE_DIR" 2>/dev/null | \
    grep -v "^$self_file:" || true)

if [ -n "$secret_hits" ]; then
    bad "no secret-key patterns under .claude/" "$secret_hits"
else
    ok "no Anthropic / generic sk_ secret patterns found under .claude/"
fi

# --- Check 10: doctrine framework is wired in ---
section "Check 10: doctrine framework referenced"
doctrine_mentions=$(grep -r -l -E '\bDoctrine\b|\bdoctrine[s ]' \
    --include="*.md" "$CLAUDE_DIR" 2>/dev/null | wc -l)
if [ "$doctrine_mentions" -lt 5 ]; then
    bad "doctrine framework wired in" \
        "expected ≥5 .md files mentioning doctrines; found $doctrine_mentions"
else
    ok "doctrines referenced across $doctrine_mentions .md files"
fi

# --- Summary ---
printf '\n──── Summary ────\n'
printf 'Checks run    : %d\n' "$check_count"
printf 'Failures      : %d\n' "$fail_count"

if [ "$fail_count" -gt 0 ]; then
    printf '%s — Claude Code configuration has structural defects.\n' "$(red FAIL)"
    exit 1
fi

printf '%s — Claude Code configuration validates.\n' "$(green OK)"
exit 0
