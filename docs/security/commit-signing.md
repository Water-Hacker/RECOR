# Commit signing (TODO-079)

## Policy

Every commit landing on `main` MUST carry a verified signature. The
CI gate `governance / commit-signing` in
`.github/workflows/required-checks.yaml` queries the GitHub API
per-commit `verification.verified` field and fails the build on any
unsigned commit on the PR branch.

Branch protection on `main` references this gate as a required
check; a PR with an unsigned commit cannot merge.

## What "verified" means

GitHub returns `verified: true` for a commit iff one of:

1. The commit was signed with a GPG / SSH key associated with a
   verified GitHub user, AND the signature verifies.
2. The commit was signed via Sigstore `gitsign` and the signature
   verifies against the Sigstore Rekor transparency log.

Both paths are acceptable. RÉCOR recommends Sigstore `gitsign`
for new contributors because:

- It produces a short-lived (10-minute) certificate keyed to a
  one-time OIDC identity, removing long-lived-secret risk.
- The Rekor transparency log is public and append-only; tamper
  detection is independent of GitHub.
- No GPG-keyring lifecycle to manage.

## How to set up

### Option A: Sigstore gitsign (recommended)

```bash
# Install gitsign (macOS / Linux)
brew install sigstore/tap/gitsign        # macOS
go install github.com/sigstore/gitsign@latest   # any platform

# Per-repository configuration
git config --local commit.gpgsign true
git config --local tag.gpgsign true
git config --local gpg.x509.program gitsign
git config --local gpg.format x509

# Verify
git commit --allow-empty -m "test: gitsign verify"
git verify-commit HEAD
```

The first signed commit opens a browser for OIDC authentication
(GitHub / Google / Microsoft).

### Option B: GPG

```bash
# Generate a key
gpg --full-generate-key            # Ed25519, no expiry beyond 1y

# Export the public key + upload to GitHub
gpg --armor --export <key-id> | gh ssh-key add --type signing -

# Per-repository configuration
git config --local user.signingkey <key-id>
git config --local commit.gpgsign true
git config --local tag.gpgsign true
```

Add the same public key to your GitHub account at
`https://github.com/settings/keys` → "New SSH key" → type "Signing".

### Option C: SSH signing

```bash
# Use an existing SSH key (the same one as for git push)
git config --local gpg.format ssh
git config --local user.signingkey ~/.ssh/id_ed25519.pub
git config --local commit.gpgsign true
```

## CI verification

The `governance / commit-signing` job runs on every PR. For each
commit on the head branch, the workflow calls:

```text
GET /repos/{owner}/{repo}/pulls/{number}/commits
```

and asserts `commit.verification.verified == true` for every entry.
A failure looks like:

```text
::error::Unsigned commits on this PR:
  abcdef0123… (unsigned)
```

The remediation is documented inline: the contributor amends the
commit (`git commit --amend -S --no-edit`) or re-signs the range
with `git rebase --exec 'git commit --amend --no-edit -S' main`.

## Branch protection

`tools/ci/apply-branch-protection.sh` (run after merging this
ticket) adds `governance / commit-signing` to the required-status-
checks set on `main`. The branch-protection rule "Require signed
commits" is ALSO enabled at the GitHub repo settings; the CI gate
is the belt, the GitHub-native rule is the suspenders.

## Doctrines

- **D14 fail-closed** — an unsigned commit fails the build; the
  remediation is mechanical, not negotiable.
- **D15 cryptographic provenance** — every consequential event
  (a code merge IS a consequential event) carries verifiable
  cryptographic provenance.
- **D18 no secrets** — Sigstore gitsign removes long-lived
  signing keys from contributor laptops.
- **D20 supply-chain SLSA L4** — every artefact in the build chain
  is traceable to a verifiable signing identity.
