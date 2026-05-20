# ADR-0014 — Stage 7 cross-source triangulation decision rules

- **Status:** Accepted (2026-05-20)
- **Deciders:** Verification team, Lead architect
- **Closes:** TODO-013 (v1 decision rules; graph traversal +
  prior-declaration drift remain follow-ups)
- **Related:** ADR-0002 (Dempster-Shafer fusion), ADR-0010 (FATF
  cascade + adequacy), ADR-0011 (BUNEC pluggability)

## Context

The pre-TODO-013 surface had Stage 7 as a permanent stub returning
`InsufficientEvidence` with a `R-VER-6` marker. FATF R.24 c.24.6 (the
multi-pronged approach) and IO.5 Core Issue 5.4 require that the
platform NOT consume each verification source in isolation — sources
must be *cross-referenced* so that signals correlated across orthogonal
feeds escalate appropriately.

A bank sanctions hit, considered alone, is meaningful but bounded
(the sanctions adapter might be false-positive). A bank sanctions
hit, combined with the same person also appearing on the PEP list,
combined with an adverse-media bucket match, is a different
signal — three orthogonal feeds agreeing on the same flag is strong
evidence. The verification engine without Stage 7 cannot collapse
those three signals into a single triangulated decision.

## Decision: rule-based triangulation over upstream outcomes

Stage 7's real implementation
(`services/verification-engine/src/application/stages/stage_7_cross_source_real.rs`)
reads the upstream `&[StageOutcome]` slice that the orchestrator
passes to `Stage::run_with_context` and applies the following rules
in order:

### Rule 1 — multi-source convergence (`Fail`)

When ≥2 of `{SanctionsScreening, PoliticallyExposedPersons,
AdverseMedia, PatternDetection}` returned `Fail` on this case,
Stage 7 returns `Fail` with:

- **Authenticity BPA:** `m_true=0.05, m_false=0.60, m_uncertain=0.35`
  — strong support for "this declaration is not what it claims",
  with substantial uncertainty mass so the fusion stage can still
  combine with corroborating evidence.
- **Risk BPA:** `m_true=0.80, m_false=0.05, m_uncertain=0.15` —
  strong support for "this is risky".

### Rule 2 — single-source contradiction with structural support (`Fail`)

When exactly one of the upstream sources `Fail`s AND the declaration
exhibits a structural red flag (`adequacy_claims.adequate == false`
OR a BO with `is_nominee = true` but no `nominator_person_id`),
Stage 7 returns `Fail` with:

- **Authenticity BPA:** `m_true=0.20, m_false=0.40, m_uncertain=0.40`
  — moderate fail.
- **Risk BPA:** `m_true=0.55, m_false=0.15, m_uncertain=0.30`.

### Rule 3 — cascade-tier-B without evidence (`InsufficientEvidence`)

When a BO carries `cascade_tier = "B"` (control-by-other-means) but
the declaration lacks `cascade_tier_b_ruled_out_evidence`, Stage 7
records `InsufficientEvidence` with a structural note. This is **not
a Fail** because the missing evidence is a declarant-side correctable:
the appropriate operator response is to flag the declaration for a
`POST /v1/declarations/{id}/correct` request, not to route it Red.

### Default — `InsufficientEvidence` (vacuous BPA)

Everything else. The orchestrator's fusion accumulator handles
vacuous gracefully — Stage 7 simply contributes no signal.

## Rationale

**Why rule-based and not Bayesian net?** A Bayesian net over Stages
3-6 would be elegant but is hard to defend to a regulator — "the
network was trained on what?" A rule-based stage with documented
mass assignments per rule, traceable in evidence JSON, is auditable
and reproducible without an opaque parameter set.

**Why these specific masses?** Calibrated against the platform's
lane thresholds (Yellow at 0.65 risk belief, Red at 0.85). A
multi-source-convergence outcome alone shouldn't pin the case Red
on its own — that lets the operator escalate consciously. A
0.80 risk_true mass after fusion with vacuous evidence elsewhere
lands the case in Yellow; combined with a Stage 3 `Fail` already
contributing 0.80 risk_true, the fused belief crosses the Red
threshold. This is the multi-pronged principle: NO single source
flips the lane; the *agreement of sources* does.

**Why is `cascade_tier_b_ruled_out_evidence` missing a vacuous
rather than a Fail?** R.24 c.24.6 distinguishes "declarant has not
yet documented this" from "declarant is lying". The former is
correctable through the platform's `correct` endpoint; the latter is
a downstream signal we'd want to see from Stage 3 / 5 / 6, not from
Stage 7's introspection of declaration shape alone.

**Why doesn't the stage write to the declaration's audit log?**
Stages are stateless. The orchestrator collects outcomes; the
persistence layer writes the case record. ADR-0002 reinforces the
discipline.

## Consequences

### Positive

- A real Stage 7 closes IO.5 Core Issue 5.4 ("evidence cross-
  referenced across sources"). The MER reviewer can read the rule
  set + the unit-test fixtures + the per-rule evidence JSON and
  trace any case decision to the source signals.
- The fusion math is preserved: Stage 7 produces a BPA in the same
  shape as Stages 2-6; Dempster combination handles the
  multiplication of evidence.
- The opt-in flag (`ENABLE_REAL_STAGE7=true`) matches the FIND-009
  pattern; the stub stays around for dev / regression baselines.

### Negative

- The rules are bounded — they consume Stage 3-6 outputs + the
  declaration's shape, not the world. Specifically:
  - **Prior-declaration drift** is not yet wired. A declarant who
    amends out a sanctioned BO between submissions is not yet
    flagged by Stage 7. (`TODO-013-graph` follow-up.)
  - **Cross-entity ownership graph** is not yet wired. A `person_id`
    that appears as a BO across two unrelated entities, where both
    entities draw separate sanctions hits, is not yet escalated.
    (`TODO-013-graph` follow-up.)
  - **BUNEC cross-reference** is gated on TODO-015's real adapter.
    Once that lands, Stage 7 will compare declared cascade tiers
    against the registered corporate structure.

### Operator burden

When the operator flips `ENABLE_REAL_STAGE7=true`, they MUST also
ensure Stages 3-6 are running with their real implementations
(`ENABLE_REAL_SANCTIONS`, `_PEP`, `_ADVERSE_MEDIA`, `_PATTERNS`).
Otherwise Stage 7's rules see only stub `InsufficientEvidence`
upstream outcomes and contribute nothing. The runbook
`docs/runbooks/v-engine-stage-flags.md` (planned) covers the
cutover.

## Verification

- Seven unit tests in
  `services/verification-engine/src/application/stages/stage_7_cross_source_real.rs`:
  - No upstream → InsufficientEvidence
  - Multi-source convergence → Fail (rule 1)
  - Single-source + inadequate claim → Fail (rule 2)
  - Single-source + nominee-without-nominator → Fail (rule 2)
  - Single-source + no structural signal → InsufficientEvidence
  - Cascade-tier-B without evidence → InsufficientEvidence (rule 3)
  - All-passes → InsufficientEvidence (default)
- Each test asserts both the outcome kind AND the `evidence.rule`
  field so a refactor that drops one of the rules silently fails CI.

## Linked from

- TODOS.md § TODO-013
- services/verification-engine/src/application/stages/stage_7_cross_source_real.rs
- services/verification-engine/src/config.rs
- docs/architecture/RECOR-Software-Architecture-Document.docx § Stage 7
