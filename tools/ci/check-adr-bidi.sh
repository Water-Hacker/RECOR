#!/usr/bin/env bash
# tools/ci/check-adr-bidi.sh — verify ADR cross-links are bidirectional.
#
# Audit reference: closes the "every ADR cross-link not bidirectional"
# item from the Doc / convention drift row of the MEDIUM/LOW summary
# table in docs/audit/10-findings.md.
#
# Doctrine: D05 (docs are part of the feature) — a one-way cross-link
# rot-traps as the linked-to ADR mutates without the linked-from being
# updated. CI enforces bidirectionality.
#
# Algorithm:
#   For every line in any docs/decisions/NNNN-*.md that links to another
#   docs/decisions/MMMM-*.md, verify the latter contains a link back to
#   the former. Failures are listed and the script exits non-zero.
#
# The check is structural only — it doesn't verify that the linked
# context is symmetric, only that A↔B both reference each other. Editor
# discipline + PR review verify the prose.

set -euo pipefail

ADR_DIR="docs/adr"
if [[ ! -d "$ADR_DIR" ]]; then
    echo "::error::ADR directory $ADR_DIR not found"
    exit 1
fi

shopt -s nullglob
files=("$ADR_DIR"/[0-9][0-9][0-9][0-9]-*.md)
shopt -u nullglob

if [[ ${#files[@]} -eq 0 ]]; then
    echo "no ADRs found under $ADR_DIR — nothing to check"
    exit 0
fi

# Pre-compute the basename → full-path map for fast lookup.
declare -A by_id
for f in "${files[@]}"; do
    base=$(basename "$f")
    id=${base%%-*}
    by_id["$id"]="$f"
done

failures=()
for f in "${files[@]}"; do
    src_id=$(basename "$f")
    src_id=${src_id%%-*}

    # Extract every ADR-link target referenced inside this file.
    # Pattern: NNNN-anything.md  OR  ./NNNN-anything.md  OR
    # docs/decisions/NNNN-anything.md.
    while IFS= read -r ref; do
        tgt_id=$(basename "$ref" | awk -F'-' '{print $1}')
        if [[ -z "${tgt_id}" || ! "${tgt_id}" =~ ^[0-9]{4}$ ]]; then
            continue
        fi
        if [[ "$tgt_id" == "$src_id" ]]; then
            continue
        fi
        tgt_path="${by_id[$tgt_id]:-}"
        if [[ -z "$tgt_path" ]]; then
            failures+=("$f references missing ADR $tgt_id")
            continue
        fi
        # Verify the target file references this source ADR.
        if ! grep -qE "(^|[^0-9])${src_id}-" "$tgt_path"; then
            failures+=("$f → $tgt_path (forward link present; reverse link missing)")
        fi
    done < <(grep -oE '[0-9]{4}-[a-z0-9-]+\.md' "$f" | sort -u || true)
done

if [[ ${#failures[@]} -gt 0 ]]; then
    echo "::error::ADR cross-link bidirectionality failures:"
    for f in "${failures[@]}"; do
        echo "  - $f"
    done
    exit 1
fi

echo "ADR cross-link bidirectionality: OK (${#files[@]} ADRs scanned)"
