# RÉCOR — Team Registry

This file is the authoritative registry of every team referenced in
`.github/CODEOWNERS`. `tools/ci/validate-codeowners.sh` enforces a two-way
reference: every team in CODEOWNERS appears here, and every team here is
used in CODEOWNERS at least once.

> **Transitional note (D24).** The repository currently lives on a personal
> GitHub account (`Water-Hacker/RECOR`). The teams listed below are
> `@<org>/<team>`-shaped names referring to the **end-state** consortium GitHub
> organisation. CODEOWNERS rules become enforceable when the repository is
> transferred to that organisation and the teams are created and populated.
> Until that transfer, CODEOWNERS is advisory.

## Cross-cutting platform teams

| Team | Institutional anchor | Scope |
|------|----------------------|-------|
| `@recor/platform-team` | Platform engineering core | Default catch-all owner; shared libraries; cross-service infrastructure |
| `@recor/architect-team` | Architect function (lead architect + Technical Advisory Function delegates) | Architecture document changes; CODEOWNERS; agent/skill ecosystem; ADRs |
| `@recor/security-team` | Security function (security engineering + integrity officers) | Threat models; access policies; cryptographic substrate; D17/D18 enforcement |
| `@recor/sre-team` | Site reliability engineering | Infrastructure-as-code; observability; deploy pipelines; runbooks |
| `@recor/legal-team` | Legal & compliance counsel | LICENSE; international cooperation channels (StAR, INTERPOL); public-tier disclosure |
| `@recor/people-ops` | Personnel security + onboarding | Onboarding documentation; access provisioning |

## Engineering domain teams

| Team | Scope |
|------|-------|
| `@recor/crypto-team` | Layer 0 cryptographic substrate; FROST coordinator; HSM client; Halo2 circuits |
| `@recor/verification-team` | Layer 3 verification engine; pattern signatures; AI inference prompts; adversarial corpus |
| `@recor/domain-team` | Layer 2 domain services (Entity, Person, Declaration, Ownership, Risk, etc.) |
| `@recor/integration-team` | Layer 5 consumer integration services; contract evolution; webhook signing |
| `@recor/frontend-team` | Layer 6 user-facing applications; design system; accessibility |

## Per-integration liaisons

These teams pair with `@recor/integration-team` on the consumer-specific
service. Each liaison is the consortium's single point of contact for that
institution.

| Team | Consumer institution |
|------|----------------------|
| `@recor/armp-liaison` | ARMP — Agence de Régulation des Marchés Publics (public procurement) |
| `@recor/anif-liaison` | ANIF — Agence Nationale d'Investigation Financière (financial intelligence) |
| `@recor/dgi-liaison` | DGI — Direction Générale des Impôts (tax administration) |
| `@recor/beac-liaison` | BEAC — Banque des États de l'Afrique Centrale (central bank) |
| `@recor/customs-liaison` | Customs administration (ASYCUDA integration) |
| `@recor/conac-liaison` | CONAC — Commission Nationale Anti-Corruption (asset declarations) |

## Application sub-teams

| Team | Scope |
|------|-------|
| `@recor/declarant-experience` | Declarant Portal UX; multilingual support; offline behaviour |

## Modification

Adding a team here:
1. Open a PR that adds the row above AND adds at least one CODEOWNERS rule
   that references the new team.
2. The validator enforces this two-way reference; a team listed here without
   a CODEOWNERS reference fails CI.
3. Approval: `@recor/architect-team` + `@recor/security-team` per
   CODEOWNERS rule on this file.

Removing a team here requires the inverse: first delete every CODEOWNERS
reference, then delete the row.

## When the consortium GitHub org exists

When the repository transfers to the consortium GitHub organisation, the
operational steps are:

1. Create each team listed above under the organisation's Teams settings.
2. Populate each team with its initial member list (consortium-side
   personnel records).
3. Re-run `tools/ci/apply-branch-protection.sh` against the transferred
   repo; CODEOWNERS enforcement begins enforcing review at this point.
4. Verify by opening a probe PR touching a stricter-rule path and confirming
   that the two-team review requirement engages.
