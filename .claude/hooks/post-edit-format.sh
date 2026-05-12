#!/usr/bin/env bash
# PostToolUse hook for Edit operations.
# Auto-formats edited files so subsequent reads see canonical formatting.

set -uo pipefail

payload=$(cat)
tool=$(echo "$payload" | jq -r '.tool_name')
[ "$tool" = "Edit" ] || exit 0

file=$(echo "$payload" | jq -r '.tool_input.file_path // empty')
[ -n "$file" ] || exit 0
[ -f "$file" ] || exit 0

case "$file" in
  *.rs)
    rustfmt --edition 2024 "$file" 2>/dev/null
    ;;
  *.go)
    gofmt -w "$file" 2>/dev/null
    ;;
  *.ts | *.tsx | *.js | *.jsx | *.json | *.md | *.yaml | *.yml)
    pnpm prettier --write --log-level=silent "$file" 2>/dev/null
    ;;
  *.tf | *.tfvars)
    terraform fmt "$file" 2>/dev/null
    ;;
  *.rego)
    opa fmt --write "$file" 2>/dev/null
    ;;
  *.proto)
    buf format -w "$file" 2>/dev/null
    ;;
esac

echo '{"decision":"allow"}'
exit 0
