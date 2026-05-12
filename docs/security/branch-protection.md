# RÉCOR — `main` branch protection specification

This file is the declarative source-of-truth for the branch protection rules
applied to `main`. The actual rules live on GitHub (a remote-side setting,
not a committable artefact); this file documents what they are, why they
exist, and how to apply them.

`tools/ci/apply-branch-protection.sh` translates this spec into a
`gh api PUT /repos/.../branches/main/protection` call. Running the script is
idempotent — re-running produces the same end state.

## Why these rules

Every rule maps to a specific engineering doctrine in Architecture V1 P2 or
to an operational constraint identified in V1 P3 (SDLC) / V1 P5 (security
operations). The intent is that protection is not "best practice" rule-of-thumb
but the technical instantiation of doctrines that already bind the team.

## The rules

### Required reviews

- **Required approving review count: 1** *(transitional; target: 2)* — D11
  calls for two reviewers with at least one cross-team. On the current
  single-maintainer personal account, two is unreachable because no second
  reviewer exists; setting the value to 2 would block every PR and break
  CI-3 (the rules must actually allow merging). The count steps to 2 in the
  same transition that introduces the `@recor/*` teams (see § "Open
  issues" below).
- **Require review from Code Owners: no** *(transitional; target: yes)* —
  GitHub's "Require review from Code Owners" toggle requires the repository
  to be public OR the account to be on GitHub Pro/Team/Enterprise. The
  toggle is left off today; CODEOWNERS routing is advisory and enforced by
  the `governance / codeowners-validate` status check.
- **Dismiss stale approvals on push: yes** — D24 (the standard is
  non-negotiable). An approval before the latest push is an approval of code
  the reviewer did not see.
- **Require last push approval: no** *(transitional; target: yes)* — closes
  the loophole where a maintainer push after approval re-enters the merge
  queue without re-review. Enabled in the same transition as the review
  count step.

### Required status checks

The protection rule references status checks by their **job name** from the
workflow YAML (the `name:` field on each job, not the key under `jobs:`).
Names must match exactly. The current required-check set spans three
workflows:

From `.github/workflows/required-checks.yaml`:

- `lint / yaml`
- `lint / shell`
- `lint / markdown`
- `secrets / gitleaks`
- `secrets / detect-secrets`
- `governance / codeowners-validate`
- `governance / pr-hygiene`
- `governance / no-dangling`
- `claude-config-validate`

From `.github/workflows/pr-hygiene.yaml`:

- `pr template completeness`
- `pr size (D10)`
- `conventional commit title`
- `D18 blocked-path / secrets paths`

From `.github/workflows/codeowners-validate.yaml`:

- `validate CODEOWNERS`

`observability-smoke` is intentionally **not** in this list yet — OBS-2 in
`docs/PRODUCTION-TODO.md` adds it once the smoke is reliable. Adding a
flaky check fails-closed on every PR for the wrong reason (D14 inverted).

Language-specific gates (rust, go, ts) are **not** in this list yet because no
production code exists. They are added in the tickets that introduce the
code (D-001, D-002, …). Adding them now would fail-closed on every PR
(D14 inverted) for paths that have nothing to fail-close against.

`strict: true` — branches must be up-to-date with `main` before merging.
This prevents the merge-but-broken-on-main race.

### Linear history

- **Require linear history: yes** — bisect-friendly history; rebase or
  squash-merge only. No "merge commits from main into feature branch"
  noise. D19 (reproducible everything) is materially easier with linear
  history because every commit is a complete snapshot.

### Force-push and deletion

- **Allow force pushes: no** — force-push to `main` rewrites history;
  retroactive history rewriting on a default branch is a doctrine 7
  workaround that the protection rule makes impossible.
- **Allow deletions: no** — the `main` branch cannot be deleted.

### Signed commits

- **Require signed commits: yes** — D15 (cryptographic provenance on every
  consequential event). Every commit to `main` carries a verified GPG or
  SSH signature.

> **Bootstrap exception.** Commit `009c95e` (`chore: monorepo skeleton per
> Companion V1 P02`) is unsigned because it was authored before signing
> infrastructure was in place on the bootstrap machine. This is a one-time
> exception documented here per D24 (the standard is non-negotiable; the
> *path* to meet it is negotiable). All commits from R-001 onward MUST be
> signed. The bootstrap exception is not extended by precedent.

### Restrict who can push to `main`

- **Restrictions: null (open)** on GitHub Free for personal accounts; on
  Pro/Team/Enterprise, this becomes "Restrict pushes that create matching
  branches" with an empty user/team list (PR-only).
- Operationally, no engineer pushes directly to `main`. The merge button in
  the GitHub UI is the only path. The "Restrict who can push" setting is
  belt-and-braces against accidental local push.

### Conversation resolution

- **Required conversation resolution: yes** — every review-comment thread
  must be marked resolved before merge. D08 (no dangling threads) extended
  to PR discussions.

### Enforce admins

- **Enforce admins: yes** — even repository admins are bound by these
  rules. D24 (the standard is non-negotiable) operationalised at the
  GitHub-permissions layer.

## How to apply

```bash
# From the repo root
gh auth status   # confirm authenticated as a repo admin
tools/ci/apply-branch-protection.sh
```

Override the target via env vars if applying to a fork or staging mirror:

```bash
REPO=acme/recor-mirror BRANCH=main tools/ci/apply-branch-protection.sh
```

## How to verify

```bash
gh api repos/Water-Hacker/RECOR/branches/main/protection | jq '
  {
    reviews: .required_pull_request_reviews,
    checks:  .required_status_checks.contexts,
    linear:  .required_linear_history,
    signed:  .required_signatures,
    admins:  .enforce_admins
  }
'
```

## When rules change

Every change to this spec is a substantive governance change:

1. Edit this file and `tools/ci/apply-branch-protection.sh` in the same PR.
2. The PR routes to `@recor/security-team` and `@recor/architect-team` per
   CODEOWNERS on `docs/security/`.
3. After merge to `main`, an admin runs the apply script against the live
   protection ruleset.
4. The change is announced in `#engineering` (or its successor channel) so
   active contributors see the new merge rules before they hit them.

## Open issues / acknowledged transitional gaps

- **CODEOWNERS enforcement is advisory** until the repo transfers to a paid
  GitHub plan or to an org with the required visibility. Reviewers still
  apply judgement; the routing rules document the intent.
- **`@recor/*` teams do not yet exist** on the current account. The bootstrap
  exception for unsigned commits, the CODEOWNERS enforcement gap, and the
  team-non-existence gap will all close in the same transition event:
  repository move to the consortium GitHub organisation.
- **Anthropic API key signing** is out of scope for this branch-protection
  spec; that is covered by the inference gateway and Vault (V5 P18, V5 P21).

## Applied state as of 2026-05-12

CI-3 (`docs/PRODUCTION-TODO.md`) applied the protection ruleset to
`Water-Hacker/RECOR@main` via `tools/ci/apply-branch-protection.sh`.

### Required status checks (14, all required)

`strict: true`. Names match each workflow's job `name:` field exactly.

1. `lint / yaml`
2. `lint / shell`
3. `lint / markdown`
4. `secrets / gitleaks`
5. `secrets / detect-secrets`
6. `governance / codeowners-validate`
7. `governance / pr-hygiene`
8. `governance / no-dangling`
9. `claude-config-validate`
10. `pr template completeness`
11. `pr size (D10)`
12. `conventional commit title`
13. `D18 blocked-path / secrets paths`
14. `validate CODEOWNERS`

### Settings applied

| Setting                                    | Value                          |
|--------------------------------------------|--------------------------------|
| `enforce_admins`                           | `true` (no admin bypass)       |
| `required_pull_request_reviews.required_approving_review_count` | `1` (transitional; target 2) |
| `required_pull_request_reviews.dismiss_stale_reviews`           | `true`                       |
| `required_pull_request_reviews.require_code_owner_reviews`      | `false` (transitional)       |
| `required_pull_request_reviews.require_last_push_approval`      | `false` (transitional)       |
| `required_linear_history`                  | `true`                         |
| `allow_force_pushes`                       | `false`                        |
| `allow_deletions`                          | `false`                        |
| `block_creations`                          | `false`                        |
| `required_conversation_resolution`         | `true`                         |
| `required_signatures`                      | `true`                         |
| `lock_branch`                              | `false`                        |

### Applied by

- Repo admin: `Water-Hacker` (sole administrator of the personal-account
  repository on 2026-05-12).
- Commit / PR: CI-3 PR in `chore/apply-branch-protection`.
- Mechanism: `tools/ci/apply-branch-protection.sh` (idempotent re-run
  produces the same end state).

### Deviations from prior R-001 spec

The script's review-count and CODEOWNERS-enforcement settings were
relaxed from the R-001 doctrine target (count=2, CODEOWNERS=enforced,
last-push-approval=on) because the repo is a personal-account private
repo with a single maintainer; the R-001 values would block every PR
from merging. The doctrine targets are restored in the same transition
event that creates the `@recor/*` teams. The remaining R-001 settings
(linear history, force-push off, deletions off, conversation resolution,
signed commits, enforce-admins) are unchanged from doctrine.

### How to verify

Read the full ruleset:

```bash
gh api repos/Water-Hacker/RECOR/branches/main/protection | jq .
```

Expected output snippet (truncated):

```json
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
  "required_pull_request_reviews": {
    "dismiss_stale_reviews": true,
    "require_code_owner_reviews": false,
    "require_last_push_approval": false,
    "required_approving_review_count": 1
  },
  "required_signatures": { "enabled": true },
  "enforce_admins":      { "enabled": true },
  "required_linear_history":          { "enabled": true },
  "allow_force_pushes":               { "enabled": false },
  "allow_deletions":                  { "enabled": false },
  "block_creations":                  { "enabled": false },
  "required_conversation_resolution": { "enabled": true },
  "lock_branch":                      { "enabled": false }
}
```

Confirm force-push is rejected:

```bash
# In a fresh clone:
git reset --hard HEAD~1
git push --force origin main
# Expected:
#   remote: error: GH006: Protected branch update failed for refs/heads/main.
#   remote: - Cannot force-push to this branch
#    ! [remote rejected] main -> main (protected branch hook declined)
```
