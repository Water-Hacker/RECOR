#!/usr/bin/env bash
# PreToolUse hook for Edit operations.
# Performs lightweight doctrine-compatibility checks before the edit is applied.
# Communicates back to Claude Code by writing structured output to stdout
# (see Claude Code hook protocol).

set -uo pipefail

# The hook payload is on stdin as JSON
payload=$(cat)

tool=$(echo "$payload" | jq -r '.tool_name')
[ "$tool" = "Edit" ] || exit 0

# Extract the file path being edited
file=$(echo "$payload" | jq -r '.tool_input.file_path // empty')
[ -n "$file" ] || exit 0

# Convert to repository-relative
rel="${file#/workspace/}"

# Check 1: forbidden surfaces (defence in depth on top of settings.json deny list)
forbidden_globs=(
  "docs/architecture/*"
  "docs/companion/*"
  "policies/access/*"
  "policies/access-encrypted-tier/*"
  "services/frost-coordinator/src/cryptographic/*"
  "services/inference-gateway/src/policy/*"
  "libraries/rust/recor-hsm/*"
  "libraries/rust/recor-frost/*"
  "libraries/rust/recor-zk/*"
  "contracts/grpc/frost.proto"
  "contracts/grpc/inference.proto"
  "infrastructure/terraform/*"
  ".claude/settings.json"
  ".github/workflows/*"
)
for pattern in "${forbidden_globs[@]}"; do
  # Shell glob match
  if [[ "$rel" == $pattern ]]; then
    cat <<EOF
{
  "decision": "block",
  "reason": "The file path '$rel' is in a forbidden-edit zone. The settings.json deny list should have prevented this; if you are seeing this hook fire, the deny list may have a gap. Halt and report to @recor/architect-team."
}
EOF
    exit 0
  fi
done

# Check 2: large file warning (warn-only; allows the edit)
size=$(stat -c%s "$file" 2>/dev/null || echo 0)
if [ "$size" -gt 100000 ]; then
  cat <<EOF
{
  "decision": "allow",
  "reason": "Editing a large file (${size} bytes). Consider whether the edit can be decomposed."
}
EOF
  exit 0
fi

# Default: allow
echo '{"decision":"allow"}'
exit 0
