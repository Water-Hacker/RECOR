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

- **Required approving review count: 2** — D11 (two reviewers, at least one
  cross-team). Two is the floor; CODEOWNERS multi-team rules effectively
  raise it on stricter paths.
- **Require review from Code Owners: yes** — D11 again. CODEOWNERS routes the
  required reviewers based on path.
- **Dismiss stale approvals on push: yes** — D24 (the standard is
  non-negotiable). An approval before the latest push is an approval of code
  the reviewer did not see.
- **Require last push approval: yes** — closes the loophole where a maintainer
  push after approval re-enters the merge queue without re-review.

> **CODEOWNERS enforcement caveat.** GitHub's "Require review from Code
> Owners" toggle requires the repository to be public OR the account to be on
> GitHub Pro/Team/Enterprise. On a personal-account GitHub Free private repo,
> the rule sets but is not enforced. See `docs/security/teams.md` for the
> transitional posture.

### Required status checks

The protection rule references status checks by their **job name** from
`.github/workflows/required-checks.yaml`. Names must match exactly. The current
required-check set is:

- `lint / yaml`
- `lint / shell`
- `lint / markdown`
- `secrets / gitleaks`
- `secrets / detect-secrets`
- `governance / codeowners-validate`
- `governance / pr-hygiene`
- `governance / no-dangling`
- `claude-config-validate`

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
