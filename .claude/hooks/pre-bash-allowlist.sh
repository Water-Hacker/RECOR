#!/usr/bin/env bash
# PreToolUse hook for Bash operations.
# Layered defence on top of the settings.json allowlist.

set -uo pipefail

payload=$(cat)
tool=$(echo "$payload" | jq -r '.tool_name')
[ "$tool" = "Bash" ] || exit 0

cmd=$(echo "$payload" | jq -r '.tool_input.command')
[ -n "$cmd" ] || exit 0

# Hard block on suspicious patterns
hard_blocks=(
  "curl.*\|.*sh"
  "wget.*\|.*sh"
  "\bsudo\b"
  "\brm -rf /\b"
  "\beval\b"
  "\bsource <\("
  "/dev/tcp/"
)
for pattern in "${hard_blocks[@]}"; do
  if echo "$cmd" | grep -E "$pattern" > /dev/null; then
    cat <<EOF
{
  "decision": "block",
  "reason": "The command matches a hard-blocked pattern: '$pattern'. This pattern is forbidden because it has been associated with prompt-injection escapes. If you are intentionally trying to execute this, do so manually."
}
EOF
    exit 0
  fi
done

# Length sanity check
if [ ${#cmd} -gt 4000 ]; then
  cat <<EOF
{
  "decision": "block",
  "reason": "Command longer than 4000 characters; decompose into smaller commands."
}
EOF
  exit 0
fi

# Default: allow (the settings.json allowlist is the primary gate)
echo '{"decision":"allow"}'
exit 0
