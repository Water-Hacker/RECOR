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

# Required-check contexts must match the job names declared in
# .github/workflows/required-checks.yaml exactly.
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
      "claude-config-validate"
    ]
  },
  "enforce_admins": true,
  "required_pull_request_reviews": {
    "dismiss_stale_reviews": true,
    "require_code_owner_reviews": true,
    "required_approving_review_count": 2,
    "require_last_push_approval": true
  },
  "restrictions": null,
  "required_linear_history": true,
  "allow_force_pushes": false,
  "allow_deletions": false,
  "required_signatures": true,
  "lock_branch": false,
  "block_creations": false,
  "required_conversation_resolution": true
}
JSON

# `gh api` accepts the JSON body via --input -.
echo "$PROTECTION" | gh api \
    --method PUT \
    --input - \
    "repos/${REPO}/branches/${BRANCH}/protection"

echo "Done. Verify with:"
echo "  gh api repos/${REPO}/branches/${BRANCH}/protection | jq ."
