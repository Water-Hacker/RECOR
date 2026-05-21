# Verification Engine — Per-Stage Decision Rules

**Ticket:** TODO-065
**Owner:** @recor/verification-team
**Last updated:** 2026-05-20
**ADR references:** ADR-0002 (Dempster-Shafer fusion), ADR-0014 (Stage 7),
ADR-0010 (FATF cascade), ADR-0012 (sanctions ladder)

This document is the human-readable specification of every decision rule
the verification engine applies, stage by stage. It supplements the code
in `services/verification-engine/src/application/stages/` and the fusion
math in `services/verification-engine/src/domain/fusion.rs`. When the
code and this document diverge, the code is authoritative; open a PR to
correct the documentation.

Engineers joining the team should read ADR-0002 first (the Dempster-Shafer
background) then this document.

---

## Architecture overview

The pipeline is a linear sequence of nine stages. Each stage receives
the full `DeclarationSnapshot` and produces a `StageOutcome` containing:

- `kind`: one of `Pass | Fail | InsufficientEvidence | ShortCircuitFailClosed`
- `authenticity_bpa`: a `BasicProbabilityAssignment { m_true, m_false, m_uncertain }`
  where the three masses are non-negative and sum to 1.0 (within ε = 1e-9)
- `risk_bpa`: same shape; some stages contribute only to authenticity,
  others contribute to both
- `evidence`: a JSON object carrying the stage's reasoning trace
- `duration_ms`: wall-clock duration of this stage

Stage 8 Dempster-combines all BPAs from Stages 1–7 into a single fused
BPA. Stage 9 applies thresholds to the fused BPA to produce the lane
decision. Stages that have nothing to say return the **vacuous BPA**
`(m_true=0, m_false=0, m_uncertain=1.0)`, which is the identity element
for Dempster combination and contributes exactly zero belief.

### Short-circuit rule

When Stage 1 emits `ShortCircuitFailClosed`, the orchestrator halts
immediately and does not run Stages 2–9. The case is recorded with lane
`red` and belief `certain_false`. No other stage can short-circuit; all
other stages produce a BPA and let the fusion accumulate.

### BPA notation in this document

`BPA(t, f, u)` = `{ m_true=t, m_false=f, m_uncertain=u }` where `t + f + u = 1.0`.

`vacuous()` = `BPA(0, 0, 1)`.
`certain_false()` = `BPA(0, 1, 0)`.
`certain_true()` = `BPA(1, 0, 0)`.

---

## Stage 1 — Schema and format validation

**Source:** `src/application/stages/stage_1_schema_validation.rs`
**I/O:** No external I/O. Deterministic pure function.

### Input signature

```
DeclarationSnapshot {
  declaration_id: Uuid,
  entity_id: Uuid,
  beneficial_owners: Vec<OwnerSnapshot>,  // must be non-empty
  effective_from: Date,
  attestation_signature_hex: String,      // 64-byte hex (128 chars)
  attestation_public_key_hex: String,     // 32-byte hex (64 chars)
  ...
}
```

### Decision rule

Five checks, all required to pass. **Any failure short-circuits the
entire pipeline** (`ShortCircuitFailClosed`):

| Check | Condition | Failure message |
|---|---|---|
| 1. Non-empty owners | `beneficial_owners.len() > 0` | "no beneficial owners declared" |
| 2. Basis-point sum | `sum(ownership_basis_points) == 10_000` | "ownership basis points sum to N, expected 10_000" |
| 3. No duplicate person_id | All `person_id` values distinct within the snapshot | "duplicate beneficial owner person_id within declaration: {uuid}" |
| 4. Effective-from not future | `effective_from <= today_utc` | "effective_from {date} is after today's date {today}" |
| 5. Attestation well-formed | `hex::decode(sig).len() == 64` and `hex::decode(pk).len() == 32` | "attestation signature is not a 64-byte hex string" |

**BPA on pass:** `BPA(0.30, 0.00, 0.70)` — moderate authenticity
support (a well-formed declaration rules out trivial fraud but does
not prove authenticity). Risk BPA: `vacuous()`.

**BPA on any failure:** `certain_false()` authenticity +
`certain_true()` risk. The short-circuit writes these directly to
the case record without running fusion.

### Worked example

**Input.** Declaration with two beneficial owners, both `person_id`
values distinct, `ownership_basis_points` of 6000 + 4000 = 10000,
`effective_from = 2026-01-01`, valid 64-byte hex attestation.

**Outcome.** All 5 checks pass → `Pass` → `BPA(0.30, 0.00, 0.70)`.

**Failure scenario.** Third beneficial owner added with the same
`person_id` as the first → check 3 fails → `ShortCircuitFailClosed`
→ pipeline halts, case recorded as red.

---

## Stage 2 — Identity authentication against BUNEC

**Source:** `src/application/stages/stage_2_identity_authentication.rs`
**I/O:** Async BUNEC adapter lookup per beneficial owner.
**ADR:** See CLAUDE.md § "Mock BUNEC"; real adapter is ticket R-VER-1.

### Input signature

```
DeclarationSnapshot { beneficial_owners: Vec<OwnerSnapshot { person_id, ... }> }
+ BunecAdapter (injected)
```

### Decision rule

For each `person_id` in `beneficial_owners`, call `BunecAdapter::lookup`.
Aggregate the results across all owners:

| Aggregate state | `kind` | `authenticity_bpa` |
|---|---|---|
| Backend error or circuit open | `InsufficientEvidence` | `vacuous()` |
| 0 owners (empty list, post-Stage-1 impossible but guarded) | `InsufficientEvidence` | `vacuous()` |
| All owners found in BUNEC | `Pass` | `BPA(0.60, 0.00, 0.40)` |
| No owners found | `Fail` | `BPA(0.00, 0.85, 0.15)` |
| Mixed (some found, some not) | `Fail` | `BPA(0.00, 0.40, 0.60)` |

Risk BPA: `vacuous()` in all cases. Identity authentication speaks to
authenticity, not risk profile.

### BPA calibration rationale

The "all found" mass assignment `(0.60, 0.00, 0.40)` reflects that
BUNEC presence confirms the person exists and is registered, but a
registered person can still file a false ownership claim. The 0.40
uncertainty term captures that ignorance. The "none found" mass
`(0.00, 0.85, 0.15)` is strong evidence against authenticity: a
beneficial owner who does not appear in the business registry is either
synthetic or the declaration is fraudulent.

**When will these weights change?** The calibration ceremony (operational
concern; not yet scheduled) will tune these against the adversarial
corpus when real BUNEC data is available under R-VER-1. Any change to
these constants requires an ADR (per CLAUDE.md architectural-integrity
section).

### Worked example

**Input.** Two beneficial owners: `person_id = A` (present in mock BUNEC
as "Aïssa Ngo Bidoung", nationality "CM") and `person_id = B` (not in
mock BUNEC).

**Stage-2 outcome.** Mixed → `Fail` → `BPA(0.00, 0.40, 0.60)`.

**Effect on fusion.** When Stages 3–7 all return vacuous (the current
stub state), Stage 8 combines:
`Stage1.BPA(0.30, 0.00, 0.70)` ⊕ `Stage2.BPA(0.00, 0.40, 0.60)` ⊕ 5×`vacuous()`.
Result: belief(true) = 0.18, plausibility(true) = 0.73 → yellow lane.

---

## Stage 3 — Sanctions screening

**Source:** `src/application/stages/stage3_sanctions.rs`
**I/O:** `SanctionsAdapter::screen` per beneficial owner; requires names
from the `NameResolver` (populated from Stage 2's BUNEC results).
**Ticket:** R-VER-2.

### Input signature

```
DeclarationSnapshot { beneficial_owners: Vec<OwnerSnapshot> }
+ SanctionsAdapter (injected)
+ NameResolver (injected; populated from Stage 2 canonical names)
```

### Decision rule

For each owner: resolve the canonical name via `NameResolver`; call
`SanctionsAdapter::screen(query, max_candidates=5)`. Aggregate across
owners using a worst-case tier accumulator (Certain > Near > Weak > None).

| Best-tier across all owners | `kind` | `authenticity_bpa` | `risk_bpa` |
|---|---|---|---|
| Adapter error | `InsufficientEvidence` | `vacuous()` | `vacuous()` |
| `Certain` match | `Fail` | `BPA(0.05, 0.85, 0.10)` | `BPA(0.85, 0.05, 0.10)` |
| `Near` match | `Fail` | `BPA(0.20, 0.40, 0.40)` | `BPA(0.40, 0.20, 0.40)` |
| `Weak` match only | `InsufficientEvidence` | `BPA(0.30, 0.10, 0.60)` | `BPA(0.10, 0.30, 0.60)` |
| No match | `Pass` | `vacuous()` | `vacuous()` |
| Name not resolved (Stage 2 backend failed) | `Pass` (vacuous; unscreeneable) | `vacuous()` | `vacuous()` |

### BPA calibration rationale

A `Certain` sanctions hit (similarity ≥ 0.95 against OFAC/UN/EU list)
is strong evidence that the declaration is not what it claims AND that
the entity poses a high risk. The 0.85 false-mass and 0.85 risk-true-mass
reflect near-certainty but preserve 0.10 uncertainty to allow Dempster
combination with corroborating evidence to push the fused belief
further. A `Near` match (similarity 0.70–0.95) warrants analyst review
but not auto-rejection; the 0.40 false-mass and 0.40 risk-true-mass
land a yellow lane when combined with vacuous evidence elsewhere.

### Short-circuit condition

None. Stage 3 never short-circuits. An adapter error produces vacuous,
which is conservative but not catastrophic: the verification pipeline
continues, and Stage 7's cross-source rule applies if other stages
also see failures.

### Worked example

**Input.** Single owner "Listed Person" appears in OFAC SDN list with
similarity 0.95 → `Certain` tier.

**Outcome.** `Fail`, `BPA(0.05, 0.85, 0.10)` authenticity, `BPA(0.85, 0.05, 0.10)` risk.

**Fusion effect (with Stage 1 passing, Stage 2 mixed, Stages 4–7 vacuous).**
Stage 8 combines Stage 1 `BPA(0.30, 0.00, 0.70)` ⊕ Stage 2 `BPA(0.00, 0.40, 0.60)` ⊕
Stage 3 `BPA(0.05, 0.85, 0.10)`. After normalisation, the fused
belief(false) exceeds the red-lane risk threshold → case routes Red.

---

## Stage 4 — PEP screening

**Source:** `src/application/stages/stage4_pep.rs`
**I/O:** `PepAdapter::screen` per beneficial owner. Shares `NameResolver`
with Stage 3.
**Ticket:** R-VER-3.

### Input signature

Same as Stage 3 with `PepAdapter` substituted for `SanctionsAdapter`.

### Decision rule

PEP exposure is a risk signal, not an authenticity signal. A senior
official can legitimately own assets; therefore `authenticity_bpa` carries
predominantly uncertainty mass, while `risk_bpa` carries the meaningful signal.

| Best result across all owners | `kind` | `authenticity_bpa` | `risk_bpa` |
|---|---|---|---|
| Backend error | `InsufficientEvidence` | `vacuous()` | `vacuous()` |
| Confirmed PEP, Certain or Near tier | `Fail` | `BPA(0.20, 0.50, 0.30)` | `BPA(0.50, 0.20, 0.30)` |
| Associate of confirmed PEP | `Fail` | `BPA(0.30, 0.30, 0.40)` | `BPA(0.30, 0.30, 0.40)` |
| Weak match only | `InsufficientEvidence` | `BPA(0.30, 0.10, 0.60)` | `BPA(0.10, 0.30, 0.60)` |
| No match | `Pass` | `vacuous()` | `vacuous()` |

### Worked example

**Input.** Owner is a confirmed senior government official (Certain PEP
tier in OpenSanctions).

**Outcome.** `Fail`, `BPA(0.20, 0.50, 0.30)` authenticity,
`BPA(0.50, 0.20, 0.30)` risk.

A Certain PEP + Certain sanctions hit (Stage 3) together will Dempster-combine
to risk belief > 0.85 and trigger Stage 7 Rule 1 (multi-source convergence),
pushing the case Red.

---

## Stage 5 — Adverse media screening

**Source:** `src/application/stages/stage5_adverse_media.rs`
**I/O:** `IcijAdapter::lookup` + Anthropic Inference Gateway structured-output call.
**Ticket:** R-VER-4. **D22:** Anthropic-primary inference.

### Input signature

```
DeclarationSnapshot { beneficial_owners: Vec<OwnerSnapshot> }
+ NameResolver (same as Stages 3–4)
+ IcijAdapter (ICIJ Offshore Leaks lookup)
+ InferenceGateway (Anthropic Claude; structured-output verdict schema)
```

### Decision rule

For each owner:
1. Resolve name via `NameResolver`.
2. Retrieve top-5 ICIJ Offshore Leaks candidates via `IcijAdapter`.
3. Send owner name + entity context + ICIJ snippets to Inference Gateway
   with a structured-output schema requesting `{ verdict, confidence, citations }`.
4. Map verdict + confidence to a BPA:

| Verdict | Confidence | `kind` | `authenticity_bpa` | `risk_bpa` |
|---|---|---|---|---|
| `adverse` | ≥ 0.70 | `Fail` | `BPA(0.05, 0.80, 0.15)` | `BPA(0.80, 0.05, 0.15)` |
| `adverse` | 0.40–0.70 | `Fail` | `BPA(0.15, 0.50, 0.35)` | `BPA(0.50, 0.15, 0.35)` |
| `clear` | any | `Pass` | `BPA(0.40, 0.05, 0.55)` | `vacuous()` |
| `insufficient_evidence` | — | `InsufficientEvidence` | `vacuous()` | `vacuous()` |
| Inference Gateway unavailable | — | `InsufficientEvidence` | `vacuous()` | `vacuous()` |

**Fixture mode.** When the Inference Gateway is in fixture mode (no
API key, `ANTHROPIC_FIXTURE_MODE=true`), it returns
`insufficient_evidence` deterministically. Stage 5 emits vacuous BPA.
This preserves offline test reproducibility (D14 fail-closed at the
integration boundary).

### Worked example

**Input.** Owner "Jean-Pierre Kamga" appears in ICIJ Panama Papers with
three relevant snippets. The Inference Gateway returns
`{ verdict: "adverse", confidence: 0.83, citations: ["Panama Papers - Mossack Fonseca – Kamga J.P."] }`.

**Outcome.** `Fail`, `BPA(0.05, 0.80, 0.15)` authenticity,
`BPA(0.80, 0.05, 0.15)` risk.

---

## Stage 6 — Pattern detection

**Source:** `src/application/stages/stage6_patterns.rs`
**I/O:** Eight SQL signature queries via `PatternDetector` against the
entity-ownership graph projected by the writeback subscriber.
**Ticket:** R-VER-5.

### Input signature

```
DeclarationSnapshot { entity_id, beneficial_owners }
+ PatternDetector (database-backed; parameterised queries only — D17)
```

### Decision rule

Eight structural-pattern signatures are evaluated in order. Each
signature returns a `(confidence, BPA_contribution)` pair. The stage
Dempster-combines the per-signature BPAs into one stage BPA.

| Signature | Pattern detected | BPA contribution on trigger |
|---|---|---|
| 1. Circular ownership | A owns B, B (transitively) owns A | `BPA(0.05, 0.70, 0.25)` risk |
| 2. Common-owner pattern | One person owns > threshold entities | `BPA(0.15, 0.50, 0.35)` risk |
| 3. Shell-company BO | Entity has no declared BUNEC activity | `BPA(0.10, 0.60, 0.30)` risk |
| 4. Layered ownership > N | Ownership chain depth > MAX_DEPTH | `BPA(0.10, 0.55, 0.35)` risk |
| 5. BO with no prior history | Declarant first seen < 24h before declaration | `BPA(0.20, 0.40, 0.40)` authenticity |
| 6. Sudden ownership change | > 50pp shift within 30 days | `BPA(0.20, 0.50, 0.30)` risk |
| 7. Opaque-jurisdiction route | Entity jurisdiction on FATF grey/black list | `BPA(0.10, 0.65, 0.25)` risk |
| 8. Sanctions-adjacent cluster | Owner shares graph neighbour with a sanctions hit | `BPA(0.15, 0.55, 0.30)` risk |

Signatures that fail to query (DB unavailable, timeout) are recorded
in evidence but do not abort the stage; the stage emits the Dempster
combination of whatever signatures succeeded. An empty combination
(all signatures failed) produces vacuous.

### Worked example

**Input.** Declaration for an entity in a FATF grey-list jurisdiction
(signature 7 triggers). Declarant was first registered 18 hours before
submission (signature 5 triggers). All other signatures clear.

**Outcome.** Stage 6 Dempster-combines sig-5 `BPA(0.20, 0.40, 0.40)` ⊕
sig-7 `BPA(0.10, 0.65, 0.25)`. Conflict K is moderate; fused belief
favours `false`. The stage emits a moderate-fail BPA that, combined with
other vacuous stages, routes the case Yellow.

---

## Stage 7 — Cross-source triangulation

**Source:** `src/application/stages/stage_7_cross_source_real.rs`
**I/O:** Reads the upstream `&[StageOutcome]` slice; no external I/O.
**ADR:** ADR-0014 — the full decision-rule text is there.
**Feature flag:** `ENABLE_REAL_STAGE7=true`.

### Input signature

```
&[StageOutcome]   // outcomes from Stages 1–6 passed by the orchestrator
+ DeclarationSnapshot   // for structural checks (adequacy_claims, is_nominee)
```

### Decision rules (from ADR-0014)

**Rule 1 — Multi-source convergence (Fail).**
When ≥ 2 of `{ SanctionsScreening, PoliticallyExposedPersons, AdverseMedia, PatternDetection }`
returned `Fail` on this case:

- Authenticity BPA: `BPA(0.05, 0.60, 0.35)`
- Risk BPA: `BPA(0.80, 0.05, 0.15)`

Rationale: three independent orthogonal feeds agreeing on a flag is a
qualitatively different signal from any single feed (FATF c.24.6
multi-pronged approach). No single feed flips the lane; agreement does.

**Rule 2 — Single-source contradiction with structural support (Fail).**
When exactly one upstream source `Fail`s AND (`adequacy_claims.adequate == false`
OR a BO with `is_nominee = true` and no `nominator_person_id`):

- Authenticity BPA: `BPA(0.20, 0.40, 0.40)`
- Risk BPA: `BPA(0.55, 0.15, 0.30)`

**Rule 3 — Cascade-tier-B without evidence (InsufficientEvidence).**
When a BO carries `cascade_tier = "B"` (Control-by-other-means) but the
declaration lacks `cascade_tier_b_ruled_out_evidence`. This is NOT a
`Fail`: the operator response is a correction request, not a rejection.

**Default — InsufficientEvidence (vacuous BPA).**
All other cases. Stage 7 contributes zero to the fusion accumulator.

### Worked example

**Input.** Stage 3 (sanctions) returned `Fail`. Stage 4 (PEP) returned
`Fail`. Stage 5 (adverse media) returned `Pass`. Stages 6 vacuous.
Declaration has `adequacy_claims.adequate = true` and all nominee fields
correct.

**Stage 7 evaluation.** Two sources from the target set failed →
Rule 1 triggers.

**Stage 7 outcome.** `Fail`, `BPA(0.05, 0.60, 0.35)` authenticity,
`BPA(0.80, 0.05, 0.15)` risk.

**Fusion (Stage 8).** Stage 1 `BPA(0.30, 0.00, 0.70)` ⊕ Stage 2
(assume all found, `BPA(0.60, 0.00, 0.40)`) ⊕ Stage 3
`BPA(0.05, 0.85, 0.10)` ⊕ Stage 4 `BPA(0.20, 0.50, 0.30)` ⊕ Stage 5
`vacuous()` ⊕ Stage 6 `vacuous()` ⊕ Stage 7 `BPA(0.05, 0.60, 0.35)`.
Fused risk belief well above 0.85 → Red lane.

---

## Stage 8 — Dempster-Shafer fusion

**Source:** `src/domain/fusion.rs`
**I/O:** Pure math; no I/O.
**ADR:** ADR-0002 (the full decision record).

### Input signature

```
Vec<BasicProbabilityAssignment>   // one per stage; vacuous stages contributed
                                  // identity element, so they're harmless
```

### Decision rule

Iterative application of **Dempster's rule of combination**:

```
m12(A) = (1/(1-K)) * Σ_{B∩C=A} m1(B) * m2(C)
K = Σ_{B∩C=∅} m1(B) * m2(C)   (conflict measure)
```

When `K → 1` (total conflict between two sources), the platform applies
**Yager's fallback**: conflict mass is allocated to the universal set
`{True, False}` (i.e., added to `m_uncertain`) rather than normalising.
This avoids the counter-intuitive "more conflict produces sharper
conclusions" pathology of strict Dempster combination under high disagreement.

**Total-conflict error.** If the conflict `K` reaches 1.0 exactly,
`FusionError::TotalConflict` is returned. The orchestrator converts this
to a `red` lane (fail-closed, D14).

### Mass conservation property

The fusion module property-tests mass conservation: for any combination of
two valid BPAs, the output BPA's masses sum to 1.0 within ε. This is
tested with 256 random configurations via proptest.

### Worked example

See the worked examples in Stages 3 and 7 above for end-to-end fusion
traces.

---

## Stage 9 — Lane routing

**Source:** `src/domain/lane.rs`
**I/O:** Pure threshold logic; no I/O.

### Input signature

```
FusedBPA { m_true, m_false, m_uncertain }
```

### Decision rule

Two derived quantities:

```
belief_true = m_true
plausibility_true = m_true + m_uncertain
```

Lane assignment:

| Condition | Lane |
|---|---|
| `belief_true ≥ 0.85` | **green** (auto-accept) |
| `belief_true ≥ 0.40` AND `plausibility_true ≥ 0.70` | **yellow** (analyst review) |
| All other | **red** (auto-reject) |
| `FusionError::TotalConflict` | **red** (fail-closed, D14) |

### Why the two-quantity design

The gap between `belief_true` and `plausibility_true` is the ignorance
mass (`m_uncertain`). A Bayesian fusion exposes a single posterior; the
analyst would not see the ignorance separately. With Dempster-Shafer, a
"high belief but high ignorance" case routes yellow rather than green —
the engine is correctly under-confident when evidence is sparse (ADR-0002
§ Decision, yellow-lane behaviour).

### Calibration of thresholds

Current thresholds (belief ≥ 0.85 → green; ≥ 0.40 / ≥ 0.70 → yellow)
are set so that a declaration with only Stages 1 and 2 active (both
finding all owners in BUNEC) produces a yellow outcome. This is the
correct outcome for the current stub-stage deployment: the engine is
explicitly under-confident. When R-VER-2 through R-VER-6 land, the
calibration ceremony will tune the thresholds against real data under
an ADR.

### Worked example

**Scenario A (current deployment, all BUNEC found).** Stage 1
`BPA(0.30, 0.00, 0.70)` ⊕ Stage 2 `BPA(0.60, 0.00, 0.40)` ⊕ 5×`vacuous()`.
Fused: `m_true ≈ 0.72`, `m_false = 0.00`, `m_uncertain ≈ 0.28`.
belief_true = 0.72 (< 0.85), plausibility_true = 1.0 (≥ 0.70),
belief_true ≥ 0.40 → **yellow**.

**Scenario B (full pipeline, no issues).** Stages 1–7 all pass / vacuous;
no Fail. Fused belief_true ≥ 0.85 → **green**.

**Scenario C (sanctions hit + PEP + Stage 7 Rule 1).** As worked in
Stage 7 above → fused risk belief > 0.85 → **red**.

---

## Cross-reference

- `services/verification-engine/CLAUDE.md` — service orientation,
  SLOs, and architectural-integrity rules
- `docs/adr/0002-dempster-shafer-fusion.md` — the ADR for Stage 8
- `docs/adr/0014-stage7-cross-source-decision-rules.md` — the ADR for Stage 7
- `docs/adr/0010-fatf-bo-cascade-and-adequacy.md` — the FATF cascade
  fields that Stages 6 and 7 inspect
- `docs/adr/0012-sanctions-proportionality-ladder.md` — how stage
  outcomes feed the sanctions workflow
- Architecture V4 P14 § Verification Engine — the canonical stage spec
- Companion V4 P17 § pipeline orchestrator + Dempster-Shafer library
