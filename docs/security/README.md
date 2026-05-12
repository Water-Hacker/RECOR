# RÉCOR — security documentation

This directory holds the security-policy-grade documents that govern
the RÉCOR platform. Operational procedures live in
`docs/runbooks/`; architecture decisions live in `docs/adr/`. The
files here document *what we enforce and why*, with code references
that map each commitment to the running system.

## Index

- [Threat model (STRIDE)](threat-model.md) — per-component
  adversary catalogue with current mitigations + accepted-risks; the
  source of truth for what the security posture protects against
  and what it does not (DOC-4).
- [Branch protection](branch-protection.md) — declarative spec for
  `main` branch protection rules + the script that applies them
  (CI-3).
- [Teams](teams.md) — `@recor/*` team membership reference used by
  CODEOWNERS and by the branch-protection review-routing rules.

## How to use this directory

- **Reviewing a PR that touches security policy:** read the relevant
  file here first; the PR description should cite the section that is
  being modified.
- **Onboarding to security review for RÉCOR:** read in this order —
  `threat-model.md`, then the ADRs (`docs/adr/`), then the per-
  component CLAUDE.md files for the components flagged as gaps.
- **Filing a new gap or accepted-risk:** open a PR that updates the
  threat model and links the ticket in `docs/PRODUCTION-TODO.md`.

## Maintenance

Each file in this directory is owned by `@recor/security-team` per
CODEOWNERS. Modifications require security-team review even when the
content is documentation-only.
