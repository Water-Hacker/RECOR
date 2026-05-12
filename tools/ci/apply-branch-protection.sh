#!/usr/bin/env bash
# tools/ci/apply-branch-protection.sh
#
# Applies the RÉCOR main-branch protection rules to the GitHub remote.
# Wraps `gh api PUT /repos/.../branches/main/protection` with the declarative
# ruleset documented in docs/security/branch-protection.md.
#
# Idempotent: re-running produces the same end state.
#
# Run requirements:
#   - `gh` CLI installed and authenticated (`gh auth status`)
#   - The authenticated user must have admin permission on the target repo
#   - REPO env var: "Owner/RepoName" (default: Water-Hacker/RECOR)
#   - BRANCH env var: branch to protect (default: main)
#
# Some protections (CODEOWNERS enforcement, "restrict pushes" allowlist) require
# either a public repo OR a GitHub Pro/Team/Enterprise plan. On a personal-account
# GitHub Free private repo those settings are silently ignored by GitHub but
# the rest of the protection still applies. See docs/security/branch-protection.md
# for the transitional posture.
#
# ───────────────────────────────────────────────────────────────────────
# Promotion heuristic for new required status checks
# ───────────────────────────────────────────────────────────────────────
# Before adding a new context to required_status_checks.contexts below, the
# workflow it references MUST have demonstrated 10 consecutive green runs
# against main (or against PRs targeting main). The rationale is D14
# (fail-closed): a flaky required check fails-closed on every PR for the
# wrong reason and inverts the doctrine — it punishes contributors for
# infrastructure flakes rather than guarding against real regressions.
#
# Check the run history before promotion:
#
#   gh run list --workflow=<workflow-name>.yaml --limit 20 \
#       --json name,conclusion,createdAt,status
#
# If 10-in-a-row green has not yet accumulated, add the context to the
# "Deferred promotions" table in docs/security/branch-protection.md and
# leave this script's required-contexts list unchanged. See OBS-2 in
# docs/PRODUCTION-TODO.md for the canonical example.

set -euo pipefail

REPO="${REPO:-Water-Hacker/RECOR}"
BRANCH="${BRANCH:-main}"

if ! command -v gh >/dev/null 2>&1; then
    echo "ERROR: gh CLI not installed." >&2
    exit 2
fi

if ! gh auth status >/dev/null 2>&1; then
    echo "ERROR: gh CLI not authenticated. Run 'gh auth login' first." >&2
    exit 2
fi

echo "Applying branch protection to ${REPO}@${BRANCH}..."

# Required-check contexts must match the job names declared in the workflow
# YAML files exactly (job `name:` field, not the key under `jobs:`).
#
# Sources:
#   .github/workflows/required-checks.yaml   — 9 jobs
#   .github/workflows/pr-hygiene.yaml        — 4 jobs
#   .github/workflows/codeowners-validate.yaml — 1 job (validate CODEOWNERS)
#
# observability-smoke is intentionally NOT in this list yet — OBS-2 in
# PRODUCTION-TODO.md adds it once the smoke is reliable.
#
# OBS-2 status (as of 2026-05-12): DEFERRED. The observability-smoke
# workflow has 4 historical runs, all failures (see branch
# ci/obs-2-promote-smoke). Re-evaluate once 10 consecutive green runs
# accumulate. See "Deferred promotions" in
# docs/security/branch-protection.md for the queue + criteria.
#
# Review-count policy: 1 approving review on a single-maintainer personal
# account today, raised to 2 (with CODEOWNERS multi-team enforcement) when
# the repo moves to the consortium GitHub organisation. See
# docs/security/branch-protection.md § "Open issues / acknowledged
# transitional gaps".
#
# `required_signatures` is set via a separate endpoint
# (PUT .../branches/main/protection/required_signatures); GitHub does NOT
# accept it inside the main protection payload. The script applies it
# after the main PUT.
read -r -d '' PROTECTION <<'JSON' || true
{
  "required_status_checks": {
    "strict": true,
    "contexts": [
      "lint / yaml",
      "lint / shell",
      "lint / markdown",
      "secrets / gitleaks",
      "secrets / detect-secrets",
      "governance / codeowners-validate",
      "governance / pr-hygiene",
      "governance / no-dangling",
      "claude-config-validate",
      "pr template completeness",
      "pr size (D10)",
      "conventional commit title",
      "D18 blocked-path / secrets paths",
      "validate CODEOWNERS"
    ]
  },
  "enforce_admins": true,
  "required_pull_request_reviews": {
    "dismiss_stale_reviews": true,
    "require_code_owner_reviews": false,
    "required_approving_review_count": 1,
    "require_last_push_approval": false
  },
  "restrictions": null,
  "required_linear_history": true,
  "allow_force_pushes": false,
  "allow_deletions": false,
  "lock_branch": false,
  "block_creations": false,
  "required_conversation_resolution": true
}
JSON

# `gh api` accepts the JSON body via --input -.
echo "$PROTECTION" | gh api \
    --method PUT \
    --input - \
    "repos/${REPO}/branches/${BRANCH}/protection" >/dev/null

# Signed-commits requirement uses a separate sub-resource.
# Some plans (Free private repos) accept the call without effect; that's
# documented in docs/security/branch-protection.md.
gh api \
    --method POST \
    -H "Accept: application/vnd.github+json" \
    "repos/${REPO}/branches/${BRANCH}/protection/required_signatures" \
    >/dev/null 2>&1 || echo "note: required_signatures not enabled (account plan does not support it on private repos)"

echo "Done. Verify with:"
echo "  gh api repos/${REPO}/branches/${BRANCH}/protection | jq ."
