#!/usr/bin/env bash
# PostToolUse hook for Bash operations.
# Captures the command and exit code for the local Claude Code audit log.
# This is in addition to (not a substitute for) Anthropic's API-side audit
# capture.

set -uo pipefail

payload=$(cat)
tool=$(echo "$payload" | jq -r '.tool_name')
[ "$tool" = "Bash" ] || exit 0

cmd=$(echo "$payload" | jq -r '.tool_input.command')
exit_code=$(echo "$payload" | jq -r '.tool_response.exit_code // "unknown"')

# Local audit log; rotates daily
audit_dir="$HOME/.claude/audit/$(date -u +%Y-%m-%d)"
mkdir -p "$audit_dir"
audit_file="$audit_dir/bash.log"

{
  echo "---"
  echo "ts: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "exit: $exit_code"
  echo "cmd: $cmd"
} >> "$audit_file"

echo '{"decision":"allow"}'
exit 0
