# Runbook — container image verification

Authoritative operating procedure for verifying the provenance of a
RÉCOR container image before deploying it to any environment.

The publisher is the workflow at `.github/workflows/publish-images.yaml`
(ticket CI-1). It builds three images on every merge to `main` and
keyless-signs each with cosign, binding the signature certificate to
the workflow's own OIDC identity. Verifying that signature is the
on-call gate between "an image at the registry" and "an image we are
willing to run".

## When this runbook fires

- Before promoting an image tag to staging or production
- After a security alert that an image at `ghcr.io/water-hacker/recor-*`
  may have been tampered with
- When a deploy fails with `unable to verify signature` from the
  admission controller (when wired up — `INF-OPA-1`)
- When auditing the supply chain end-to-end (quarterly review)

## Build matrix

| Image | Dockerfile | Build context |
|---|---|---|
| `ghcr.io/water-hacker/recor-declaration` | `services/declaration/Dockerfile` | repo root (`.`) |
| `ghcr.io/water-hacker/recor-verification-engine` | `services/verification-engine/Dockerfile` | repo root (`.`) |
| `ghcr.io/water-hacker/recor-portal` | `applications/declarant-portal/Dockerfile` | `applications/declarant-portal/` |

The two Rust services build from the workspace root because they
depend on shared crates in `packages/`. The portal builds from its
own directory because the Node toolchain expects a single-package
context.

## Tag policy

- `:latest` — moves forward with each merge to `main`. **Never** reference
  `:latest` from a production manifest; it is for human convenience
  and dev/staging-only.
- `:${git_sha}` — the immutable reference. The full 40-character commit
  SHA from `main`. This is the only tag production manifests are
  allowed to pin.

A given digest is signed exactly once. Re-publishing the same SHA
(e.g. via `workflow_dispatch` on the same commit) produces the same
digest from the cache and re-signs it; the signature is appended, not
replaced.

## How on-call verifies an image

Cosign uses keyless verification: the signature certificate was issued
by Sigstore's Fulcio CA against the workflow's GitHub Actions OIDC
token. The `Subject` of the cert is the workflow file's URL plus the
ref it ran on. Pin that with a regex.

```bash
cosign verify \
  --certificate-identity-regexp 'https://github.com/Water-Hacker/RECOR/.github/workflows/publish-images.yaml@.*' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com \
  ghcr.io/water-hacker/recor-declaration:latest
```

Expected output (abbreviated):

```
Verification for ghcr.io/water-hacker/recor-declaration:latest --
The following checks were performed on each of these signatures:
  - The cosign claims were validated
  - Existence of the claims in the transparency log was verified offline
  - The signatures were verified against the specified public key
[{"critical": {...}, "optional": {"Bundle": {...}, "Issuer": "https://token.actions.githubusercontent.com", "Subject": "https://github.com/Water-Hacker/RECOR/.github/workflows/publish-images.yaml@refs/heads/main"}}]
```

The same command form applies to the other two images; substitute
`recor-verification-engine` or `recor-portal` for the image name.

For production-grade verification, pin the tag to the immutable SHA:

```bash
SHA=$(git rev-parse main)
cosign verify \
  --certificate-identity-regexp 'https://github.com/Water-Hacker/RECOR/.github/workflows/publish-images.yaml@.*' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com \
  "ghcr.io/water-hacker/recor-declaration:${SHA}"
```

## What "verified" actually proves

A successful verify means:

1. The image at that digest was signed by the workflow at
   `Water-Hacker/RECOR/.github/workflows/publish-images.yaml`.
2. The OIDC token used to obtain the signing certificate was issued
   by `token.actions.githubusercontent.com` — i.e. the runner was a
   real GitHub-hosted runner, not a forge.
3. The signature is logged in the Sigstore Rekor transparency log
   (cosign verifies the log inclusion proof offline by default).

It does **not** prove the image is free of vulnerabilities or that
the underlying source commit was reviewed. Vulnerability scanning
(Trivy HIGH/CRITICAL gate) and SBOM attestations (SPDX + CycloneDX)
ship in CI-2 — see `docs/runbooks/supply-chain.md` for the audit and
override procedures. Review enforcement arrives in branch protection
(CI-3).

## Failure modes

### `Error: no matching signatures`

The image was not signed by our workflow. Possible causes:

- The image was pushed manually outside the workflow (forbidden in
  production); reject and investigate.
- The workflow ran but the cosign sign step failed silently — check
  the workflow run logs. `cosign sign --yes` exits non-zero on
  failure, which would have failed the job, so this is rare; if seen,
  open an incident.
- The image is from before CI-1 landed. Pre-CI-1 images are
  unsigned and must not be deployed.

### `Error: certificate identity does not match`

The signature exists but the certificate `Subject` does not match the
regex. Either the regex is wrong (check this runbook is current) or
the signature came from a different workflow file — investigate.

### `Error: ... transparency log lookup failed`

Network issue reaching Rekor (`rekor.sigstore.dev`). Retry; if
persistent, check Sigstore status page. Do **not** add `--insecure-ignore-tlog`
to bypass — that is exactly the integrity check we are paying for.

## Manual re-publish

For base-image refreshes (e.g. a new Debian security update lands
upstream) without a code change:

```bash
gh workflow run publish-images.yaml --ref main
```

The workflow rebuilds from cache where it can, re-tags `:latest` and
`:${sha}` against the same commit SHA, and re-signs. Verify with the
command above afterwards.

To re-publish from a non-main ref (e.g. a release branch in the
future), pass `--ref <branch>`. The certificate `Subject` will then
end in `@refs/heads/<branch>`; on-call may need to widen the regex
accordingly when verifying that build.

## Cosign install for on-call laptops

```bash
# macOS
brew install cosign

# Debian / Ubuntu
curl -sLO https://github.com/sigstore/cosign/releases/download/v2.4.1/cosign-linux-amd64
sudo install -m 0755 cosign-linux-amd64 /usr/local/bin/cosign
cosign version
```

The workflow pins `v2.4.1`; on-call should run a major-compatible
client (any 2.x).

## Related

- Workflow: `.github/workflows/publish-images.yaml`
- Ticket: `docs/PRODUCTION-TODO.md` § CI-1
- Companion: `docs/runbooks/supply-chain.md` (CI-2) — SBOM audit,
  Trivy override procedure, drift detection
- Follow-up: CI-3 (branch protection wired to required checks)
- Architecture: V5 P21 § Supply chain integrity (target SLSA Level 4)
