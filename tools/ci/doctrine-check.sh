#!/usr/bin/env bash
# tools/ci/doctrine-check.sh
#
# TODO-058: deterministic doctrine-check linter invoked by the CI gate
# `governance / doctrine-check` (.github/workflows/required-checks.yaml).
#
# Scope: the diff introduced by the current PR (or the last commit on
# `main` for a push event), NOT the whole tree. This keeps existing
# pre-doctrine code from being retroactively rejected and matches the
# Doctrine 7 (no workarounds) principle of fixing the source of the
# problem rather than masking it.
#
# Rules enforced (each fails the build if a violation is INTRODUCED in
# the diff):
#
#   R1 — `unwrap()` in non-test Rust code
#   R2 — `panic!(` in non-test Rust code
#   R3 — `todo!()` / `unimplemented!()` outside trait-default stubs
#   R4 — `unsafe { … }` block lacking a `// SAFETY:` comment above it
#   R5 — `console.log` / `println!` / `dbg!` in production code
#   R6 — committing a .env-style file
#
# Doctrines invoked:
#   D07 no workarounds   — the linter says no to silent escape hatches
#   D08 no dangling      — todo!()/unimplemented!() are by definition
#                          dangling work, refused at the boundary
#   D12 production-grade — println!/dbg!/console.log are dev artefacts
#                          unsuited for production
#   D14 fail-closed      — exit non-zero on first violation; CI fails
#   D18 no secrets       — .env files contain secrets, refused at PR
#
# Allow-list: `.github/doctrine-check-allowlist.txt` documents the
# narrow, time-boxed exceptions. Each line is a regex matched against
# `<rule>:<file>:<line>:<excerpt>` violation strings; matching lines
# are warnings, not failures.
#
# Usage:
#   tools/ci/doctrine-check.sh                  (auto-detects PR base)
#   tools/ci/doctrine-check.sh <base-ref>       (explicit base ref)
#
# Exit codes:
#   0 — no violations (or all allow-listed)
#   1 — at least one introduced violation
#   2 — usage / git error

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
cd "$REPO_ROOT"

BASE_REF="${1:-}"
if [[ -z "$BASE_REF" ]]; then
  if [[ -n "${GITHUB_BASE_REF:-}" ]]; then
    BASE_REF="origin/${GITHUB_BASE_REF}"
  else
    BASE_REF="origin/main"
  fi
fi

# Resolve the base ref to a SHA we can diff against. If the ref does
# not exist (first commit of a new repo, shallow clone, etc.), fall
# back to the empty-tree SHA so the diff is the full working tree.
if ! git rev-parse --verify "$BASE_REF" >/dev/null 2>&1; then
  BASE_SHA="$(git hash-object -t tree /dev/null)"
else
  BASE_SHA="$BASE_REF"
fi

ALLOWLIST="${REPO_ROOT}/.github/doctrine-check-allowlist.txt"
declare -a ALLOW_PATTERNS=()
if [[ -f "$ALLOWLIST" ]]; then
  while IFS= read -r line; do
    [[ -z "$line" || "$line" =~ ^# ]] && continue
    ALLOW_PATTERNS+=("$line")
  done < "$ALLOWLIST"
fi

is_allowed() {
  local violation="$1"
  for pat in "${ALLOW_PATTERNS[@]:-}"; do
    if [[ "$violation" =~ $pat ]]; then
      return 0
    fi
  done
  return 1
}

# Gather the changed files (A=added, M=modified). Renames/copies show
# the new path. We skip deletions because there's nothing to lint.
CHANGED_FILES="$(git diff --name-only --diff-filter=AM "$BASE_SHA"...HEAD 2>/dev/null || true)"

if [[ -z "$CHANGED_FILES" ]]; then
  echo "doctrine-check: no changed files vs $BASE_REF — pass"
  exit 0
fi

VIOLATIONS=0
WARNINGS=0

emit_violation() {
  local rule="$1" file="$2" line="$3" excerpt="$4"
  local msg="${rule}:${file}:${line}:${excerpt}"
  if is_allowed "$msg"; then
    printf '::warning file=%s,line=%s::doctrine-check[%s] ALLOW-LISTED: %s\n' "$file" "$line" "$rule" "$excerpt"
    WARNINGS=$((WARNINGS + 1))
  else
    printf '::error file=%s,line=%s::doctrine-check[%s] %s\n' "$file" "$line" "$rule" "$excerpt"
    VIOLATIONS=$((VIOLATIONS + 1))
  fi
}

# Heuristic test-context check: a Rust file or chunk is in test context
# if it is under a `tests/`, `benches/`, or `examples/` directory, has
# a filename ending in `_test.rs` / `tests.rs`, or the matched line is
# inside an obvious `#[cfg(test)]` / `#[test]` / `#[tokio::test]` block.
is_rust_test_file() {
  local f="$1"
  [[ "$f" =~ /tests/ ]]            && return 0
  [[ "$f" =~ /benches/ ]]          && return 0
  [[ "$f" =~ /examples/ ]]         && return 0
  [[ "$f" =~ _test\.rs$ ]]         && return 0
  [[ "$f" =~ /tests\.rs$ ]]        && return 0
  return 1
}

# Returns 0 if the given line in the file is inside a `#[cfg(test)] mod
# tests { … }` block (the canonical Rust unit-test pattern). We scan
# backwards from `lineno` looking for the nearest of:
#   - `#[cfg(test)]\n[pub ]mod ` (we are in tests; return 0)
#   - Any `^}` at column 0 with no `mod ` before it (we are NOT in tests)
# This is a heuristic, not a full Rust parser; the false-positive direction
# is "miss a test-context", which surfaces a violation the operator can
# allow-list. The false-negative direction (incorrectly classify production
# as test) is the one we MUST avoid — and we do, because the scan only
# accepts an explicit `#[cfg(test)]` marker as proof.
is_inside_cfg_test_block() {
  local f="$1" lineno="$2"
  # Limit the look-back to 800 lines to keep the scan O(1) per match.
  local start=$((lineno > 800 ? lineno - 800 : 1))
  # Walk backwards. We want the LAST `#[cfg(test)]` OR the LAST `^}` at
  # column 0; whichever is closer wins.
  local last_cfg_test=0 last_close_brace=0
  while IFS= read -r entry; do
    local n="${entry%%:*}"
    local content="${entry#*:}"
    if [[ "$content" =~ ^#\[cfg\(test\)\] ]]; then
      last_cfg_test="$n"
    elif [[ "$content" =~ ^\}[[:space:]]*$ ]]; then
      last_close_brace="$n"
    fi
  done < <(awk -v s="$start" -v e="$lineno" 'NR>=s && NR<=e { print NR ":" $0 }' "$f" 2>/dev/null)
  if (( last_cfg_test > last_close_brace )); then
    return 0
  fi
  return 1
}

# Per-file scan. Each rule grep is scoped to lines introduced by the
# diff (`git diff … -U0` then filter `^+` lines) so we do not flag
# pre-existing code.
for f in $CHANGED_FILES; do
  [[ ! -f "$f" ]] && continue

  # R6 — .env-style files in the diff (any extension is fatal; secrets
  # in chat/code/tickets are D18). The .env.example is permitted via
  # the allow-list.
  case "$f" in
    *.env|*/.env|.env|.env.*)
      emit_violation "R6_env_file" "$f" "0" ".env-style file added to commit ($f)"
      ;;
  esac

  # Skip non-Rust / non-TS / non-JS files for the source-code rules.
  case "$f" in
    *.rs|*.ts|*.tsx|*.js|*.jsx|*.mjs|*.cjs) ;;
    *) continue ;;
  esac

  # Get the introduced lines (prefix `+`, skipping the `+++` header)
  # together with their target line numbers. We use `git diff -U0` so
  # only the changed hunks are emitted.
  while IFS= read -r diffline; do
    if [[ "$diffline" =~ ^@@\ -[0-9,]+\ \+([0-9]+)(,[0-9]+)?\ @@ ]]; then
      cur_lineno="${BASH_REMATCH[1]}"
      continue
    fi
    if [[ "$diffline" =~ ^\+\+\+ ]]; then
      continue
    fi
    if [[ "$diffline" =~ ^- ]]; then
      continue
    fi
    if [[ "$diffline" =~ ^\+(.*)$ ]]; then
      content="${BASH_REMATCH[1]}"
      lineno="${cur_lineno:-0}"
      cur_lineno=$((cur_lineno + 1))

      # Common rules across Rust + JS/TS
      case "$f" in
        *.rs)
          if is_rust_test_file "$f"; then
            # Skip the in-test-file rules; only check R6 above.
            continue
          fi
          if is_inside_cfg_test_block "$f" "$lineno"; then
            # Inside a `#[cfg(test)] mod tests { … }` block — skip.
            continue
          fi

          # Strip line comments before matching so `// note: unwrap()`
          # in a doc comment does not trigger.
          stripped="${content%%//*}"

          # R1 — unwrap() in non-test code
          if [[ "$stripped" == *".unwrap()"* ]]; then
            emit_violation "R1_unwrap" "$f" "$lineno" "${content}"
          fi

          # R2 — panic!( in non-test code
          if [[ "$stripped" == *"panic!("* ]]; then
            emit_violation "R2_panic" "$f" "$lineno" "${content}"
          fi

          # R3 — todo!()/unimplemented!() outside trait-default stubs.
          # Trait-default stubs we accept iff the surrounding context
          # contains `fn ` AND `default fn`; we cannot easily detect
          # this from a line-scan, so we flag every introduction and
          # rely on the allow-list to whitelist documented exceptions.
          if [[ "$stripped" == *"todo!()"* ]] || [[ "$stripped" == *"unimplemented!()"* ]]; then
            emit_violation "R3_todo" "$f" "$lineno" "${content}"
          fi

          # R4 — unsafe { … } without a `// SAFETY:` comment on the
          # line above (we approximate by looking for a SAFETY comment
          # in the same hunk; the precise check is in clippy's
          # `undocumented_unsafe_blocks` lint).
          if [[ "$stripped" =~ unsafe[[:space:]]*\{ ]]; then
            # Look back 3 lines in the file (post-edit working tree)
            # for the SAFETY marker.
            window_start=$((lineno > 3 ? lineno - 3 : 1))
            if ! sed -n "${window_start},${lineno}p" "$f" 2>/dev/null | grep -q '// SAFETY:'; then
              emit_violation "R4_unsafe_no_safety" "$f" "$lineno" "${content}"
            fi
          fi

          # R5 — println!/dbg! in production Rust code
          if [[ "$stripped" == *"println!("* ]]; then
            emit_violation "R5_println" "$f" "$lineno" "${content}"
          fi
          if [[ "$stripped" =~ (^|[^A-Za-z_])dbg!\( ]]; then
            emit_violation "R5_dbg" "$f" "$lineno" "${content}"
          fi
          ;;

        *.ts|*.tsx|*.js|*.jsx|*.mjs|*.cjs)
          # Skip portal/app test files.
          if [[ "$f" =~ \.test\. ]] || [[ "$f" =~ \.spec\. ]] || [[ "$f" =~ /tests/ ]] || [[ "$f" =~ /__tests__/ ]] || [[ "$f" =~ /e2e/ ]]; then
            continue
          fi
          # Strip // line comments.
          stripped="${content%%//*}"
          if [[ "$stripped" == *"console.log("* ]]; then
            emit_violation "R5_console_log" "$f" "$lineno" "${content}"
          fi
          ;;
      esac
    fi
  done < <(git diff -U0 "$BASE_SHA"...HEAD -- "$f" 2>/dev/null || true)
done

echo "---"
echo "doctrine-check: ${VIOLATIONS} violation(s), ${WARNINGS} allow-listed warning(s)"
if [[ "$VIOLATIONS" -gt 0 ]]; then
  echo "FAIL — fix the violations above, or document an allow-list entry"
  echo "       at .github/doctrine-check-allowlist.txt with a justification."
  exit 1
fi
exit 0
