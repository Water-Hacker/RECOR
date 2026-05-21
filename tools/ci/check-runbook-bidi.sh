#!/usr/bin/env bash
# tools/ci/check-runbook-bidi.sh — runbook cross-link bidirectionality.
#
# TODO-074 closure. Mirrors the ADR bidi check
# (tools/ci/check-adr-bidi.sh) for the runbook surface.
#
# Algorithm:
#   For every line in any docs/runbooks/*.md that links to another
#   docs/runbooks/<name>.md OR to a docs/adr/NNNN-*.md, assert the
#   linked-to file links back. Failures are listed; the script exits
#   non-zero.
#
# Doctrine:
#   - D05 (docs are part of the feature) — a one-way cross-link rots
#     when the linked-to file mutates without the linked-from being
#     updated. CI enforces bidirectionality.
#   - D14 (fail-closed) — broken cross-links surface as build
#     failures; there is no "this run only" suppression flag. The fix
#     is always "add the back-link" or "drop the forward link".
#
# Why bidi:
#   The runbooks are the on-call's load-bearing surface. A runbook
#   that points to another runbook ("if the BUNEC adapter is also
#   down, see bunec-adapter-outage.md") communicates a hard
#   operational dependency; the linked-to runbook needs the same
#   pointer so the on-call who arrives via the other side does not
#   silently miss the linkage.
#
# Scope:
#   - Forward link target is matched against:
#       docs/runbooks/<basename>.md          (relative to repo root)
#       ./<basename>.md                       (relative to docs/runbooks/)
#       <basename>.md                         (relative to docs/runbooks/)
#       docs/adr/NNNN-*.md / NNNN-*.md       (ADR cross-references)
#   - Self-references are skipped.
#   - Links inside fenced code blocks (``` ... ```) are skipped to
#     avoid example URLs in code samples false-positiving the check.

set -euo pipefail

REPO_ROOT=$(cd "$(dirname "$0")/../.." && pwd)
cd "$REPO_ROOT"

RUNBOOK_DIR="docs/runbooks"
ADR_DIR="docs/adr"

if [[ ! -d "$RUNBOOK_DIR" ]]; then
    echo "::error::runbook directory $RUNBOOK_DIR not found"
    exit 1
fi

shopt -s nullglob
runbook_files=("$RUNBOOK_DIR"/*.md)
shopt -u nullglob

if [[ ${#runbook_files[@]} -eq 0 ]]; then
    echo "no runbooks found under $RUNBOOK_DIR — nothing to check"
    exit 0
fi

# Pre-compute basename → full-path indexes so we can resolve a
# forward-link target quickly without re-walking the filesystem.
declare -A runbook_by_name
for f in "${runbook_files[@]}"; do
    base=$(basename "$f")
    runbook_by_name["$base"]="$f"
done

declare -A adr_by_id
if [[ -d "$ADR_DIR" ]]; then
    shopt -s nullglob
    for f in "$ADR_DIR"/[0-9][0-9][0-9][0-9]-*.md; do
        base=$(basename "$f")
        id=${base%%-*}
        adr_by_id["$id"]="$f"
    done
    shopt -u nullglob
fi

# strip_code_fences: emit STDIN with fenced code blocks (``` … ```)
# replaced by blank lines. Keeps line numbers stable for grep -n
# elsewhere; the cheap awk filter is the right tool for the size of
# input we're processing (a few KB per runbook).
strip_code_fences() {
    awk '
        BEGIN { in_fence = 0 }
        /^```/ { in_fence = !in_fence; print ""; next }
        { if (in_fence) print ""; else print $0 }
    '
}

# A file is considered to "link back" to another file iff the
# linker's basename appears anywhere in the linked-to file's prose
# (outside fenced code). We compare basenames, not paths, because
# Markdown allows several path forms for the same target.
#
# The check is structural only — discipline + PR review verify the
# prose context. Disabling pipefail for this function: a no-match
# from the second `grep` is normal flow control, not a script error.
file_links_to() {
    local container="$1"     # absolute path of the file to scan
    local target_base="$2"   # basename to search for
    local stripped
    stripped=$(strip_code_fences < "$container")
    grep -q -F -- "$target_base" <<<"$stripped"
}

failures=()

for src in "${runbook_files[@]}"; do
    src_base=$(basename "$src")

    # Process the file body with fenced code stripped, so a link in
    # a `bash`-tagged example does not count as a real cross-link.
    body=$(strip_code_fences < "$src")

    # Extract every Markdown link target — the `](…)` shape, with
    # an optional `./` or path prefix, ending in `.md`. This is
    # tighter than "any *.md text" so an inline reference to a
    # rendered runbook NAME (without surrounding link syntax) does
    # NOT cause a false positive. `grep` returning 1 (no match) is
    # NORMAL — a runbook with no cross-links is valid. We swallow
    # that exit code so `pipefail` doesn't abort the loop.
    link_targets=$(grep -oE '\]\([^)]*\.md[^)]*\)' <<<"$body" \
        | sed -E 's/^\]\(//' \
        | sed -E 's/\).*$//' \
        | sed -E 's/#.*$//' \
        | sort -u || true)

    # 1. Runbook → runbook cross-links.
    while IFS= read -r ref; do
        [[ -z "$ref" ]] && continue
        tgt_base=$(basename "$ref")
        if [[ "$tgt_base" == "$src_base" ]]; then
            continue
        fi
        # Only enforce for known runbooks. Unknown basenames are
        # ignored (they could be a renamed file or a doc outside
        # the runbook scope; ADR cross-link handling below covers
        # docs/adr separately).
        if [[ -z "${runbook_by_name[$tgt_base]:-}" ]]; then
            continue
        fi
        tgt_path="${runbook_by_name[$tgt_base]}"
        if ! file_links_to "$tgt_path" "$src_base"; then
            failures+=("$src → $tgt_path (forward link present; reverse link missing)")
        fi
    done <<<"$link_targets"

    # 2. Runbook → ADR cross-links.
    while IFS= read -r ref; do
        [[ -z "$ref" ]] && continue
        adr_base=$(basename "$ref")
        if ! [[ "$adr_base" =~ ^[0-9]{4}- ]]; then
            continue
        fi
        adr_id=$(awk -F'-' '{print $1}' <<<"$adr_base")
        tgt_path="${adr_by_id[$adr_id]:-}"
        if [[ -z "$tgt_path" ]]; then
            failures+=("$src references unknown ADR $adr_id")
            continue
        fi
        if ! file_links_to "$tgt_path" "$src_base"; then
            failures+=("$src → $tgt_path (forward link present; reverse link missing)")
        fi
    done <<<"$link_targets"
done

if [[ ${#failures[@]} -gt 0 ]]; then
    echo "::error::runbook cross-link bidirectionality failures:"
    for f in "${failures[@]}"; do
        echo "  - $f"
    done
    exit 1
fi

echo "runbook cross-link bidirectionality: OK (${#runbook_files[@]} runbooks scanned)"
