# ADR-0002: Dempster-Shafer fusion for the Verification Engine

**Status:** Accepted (since 2026-05-11)
**Decision-makers:** @recor/architect-team, @recor/verification-team
**Date:** 2026-05-11 (commit `327b1a3`)

## Context

The Verification Engine (Architecture V4 P14) is RÉCOR's load-bearing
capability. A declaration submitted to the platform runs through a
nine-stage pipeline — schema validation, identity authentication,
sanctions screening, PEP screening, adverse-media analysis, pattern
detection, cross-source triangulation, fusion, and lane routing. Each
upstream stage produces evidence; Stage 8 fuses that evidence into a
single belief; Stage 9 routes the declaration to a green (auto-accept),
yellow (analyst review), or red (auto-reject) lane based on thresholds.

The structural question for Stage 8: **how do we combine evidence from
seven heterogeneous upstream stages, several of which are stubs today,
into a single decision?** The dominant epistemic situation we face is
not "two stages disagree" — it is "five stages have nothing to say
and two say a weak yes." That is the regime where the choice of
fusion mathematics matters most. The Verification Engine's `CLAUDE.md`
makes this point explicitly: "Architectural integrity — the
Dempster-Shafer fusion is the centrepiece. Any change to its math is
an ADR-required decision, not a casual modification."

The realistic candidates were Bayesian inference, Dempster-Shafer
belief functions, subjective logic, and weighted-sum heuristics. The
choice has direct operational consequences: a fusion that handles
"no signal" badly will either auto-accept declarations on weak
evidence (the green-lane failure mode) or auto-reject them on missing
evidence (the red-lane failure mode). Both are wrong; both have been
the subject of regulatory enforcement actions against other
beneficial-ownership registries.

## Decision

We chose **Dempster-Shafer belief-function theory** over the binary
frame of discernment Θ = {True, False}. Implemented in commit
`327b1a3` at `services/verification-engine/src/domain/fusion.rs`
(~360 lines of pure Rust with 11 unit tests plus a proptest mass
conservation property).

Specifics:

- **Basic Probability Assignment (BPA).** Each upstream stage produces
  a `BasicProbabilityAssignment { m_true, m_false, m_uncertain }`
  with the three masses non-negative and summing to 1 within
  `MASS_EPSILON` (1e-9). The `m_uncertain` term is the mass on the
  universal set {True, False} — the *explicit* representation of
  ignorance that distinguishes Dempster-Shafer from Bayesian
  probability.
- **Vacuous BPA.** Stages that have nothing to say return
  `BasicProbabilityAssignment::vacuous()` = `(0, 0, 1)`. This is the
  identity element for Dempster combination: combining vacuous with
  any other BPA returns that other BPA unchanged. The five stub
  stages (sanctions, PEP, adverse-media, patterns, cross-source) all
  return vacuous today and therefore contribute zero belief to the
  decision.
- **Dempster's rule of combination.** `BPA::combine(other)`
  implements the classical Dempster rule with conflict normalisation
  `1/(1-K)`. Returns `FusionError::TotalConflict` when `K → 1`.
- **Yager's fallback.** When two sources are in total conflict,
  Yager's rule allocates the conflict mass to the universal set
  instead of normalising. This avoids the counter-intuitive
  "more conflict produces sharper conclusions" pathology of strict
  Dempster combination under high disagreement.
- **Lane routing.** Stage 9 reads two derived quantities from the
  fused BPA: `belief(True) = m_true` (lower bound on confidence
  that the declaration is true) and `plausibility(True) = m_true +
  m_uncertain` (upper bound). The gap between belief and plausibility
  is ignorance. Thresholds: belief ≥ 0.85 → green; belief ≥ 0.40 with
  plausibility ≥ 0.70 → yellow; otherwise red.

The yellow-lane behaviour is the key validation. The integration
smoke (Case A) has two beneficial owners both present in seeded
mock BUNEC. Only Stages 1 and 2 produce evidence; the other five
contribute vacuous BPAs. The fused belief is 0.72, plausibility is
1.0, and the case routes to **yellow**. This is the correct outcome:
with only two of seven evidence sources active, the engine is
*correctly under-confident* and routes to analyst review rather than
auto-accepting. A Bayesian fusion would have produced a higher
posterior because the absence of negative evidence would have been
treated as supporting evidence.

## Consequences

### Positive

- "No signal" is represented explicitly. The five stub stages
  contribute exactly zero belief — not a flat prior, not a
  pseudo-confidence. When `R-VER-2` through `R-VER-6` add real
  evidence, those stages will *replace* vacuous BPAs with informed
  ones; the math is monotone in the operationally meaningful sense.
- Lane router has a second decision input. The gap between belief
  and plausibility (the ignorance term) is independently visible
  to the threshold logic. "High belief but high ignorance" routes
  to yellow, not green. A Bayesian fusion exposes a single
  posterior; the analyst would not see the ignorance separately.
- Total conflict is detected, not silenced. The `TotalConflict`
  error path lets the lane router default to red on K → 1 rather
  than producing a meaningless "50/50" answer from divide-by-zero.
- Pure, deterministic, property-testable. The fusion module is pure
  Rust, no I/O. The proptest guarantees mass conservation under
  combination over 256 random configurations.

### Negative

- Engineers unfamiliar with belief-function theory need to learn
  it. Bayesian inference is the cultural default. The codebase
  pays the onboarding cost via inline doc comments in
  `fusion.rs` and a dedicated section in the service `CLAUDE.md`.
- Independence assumption is a known weakness. Dempster's rule
  assumes the sources are independent; if Stages 3 and 4 both rely
  on the same upstream sanctions feed, their evidence is correlated
  and the fused belief overstates true confidence. We mitigate by
  documenting source provenance per stage and by treating "two
  stages with the same source" as a single source for fusion
  purposes; the analyst-side UI surfaces source overlap to the
  reviewer.
- Calibration is harder. With a Bayesian posterior we could check
  predictive calibration directly against ground truth (Brier score
  etc.). Belief functions have a calibration analogue but it is
  less standardised. We accept the asymmetric epistemic cost in
  exchange for explicit ignorance.

### Neutral

- The math will evolve. Pignistic transformation (BetP) for
  decision-theoretic comparisons, PCR6 rules for finer conflict
  handling, and per-stage discounting are all reasonable future
  refinements. Per the `CLAUDE.md` directive any such change is
  ADR-required.
- The current code uses the binary frame Θ = {True, False}. A
  future ternary frame ({True, False, NotApplicable}) would be a
  larger structural change — feasible, but ADR-required.

## Alternatives considered

### Bayesian inference

Rejected. Bayesian inference requires a prior on every parameter,
including on the absence of negative evidence. For the dominant
epistemic state ("several stub stages, two real stages saying weak
yes") that prior dominates the posterior, and the prior is
indefensible — there is no principled reason to claim the population
of declarations is 90% honest vs 50% honest vs anything else. A
fusion that produces high posterior confidence on weak evidence
auto-accepts adversarial declarations. The yellow-lane test case
makes this concrete: Bayesian would route Case A to green.

### Subjective logic (Jøsang)

Rejected for v1. Subjective logic is an elegant alternative that
explicitly carries uncertainty in its opinion 4-tuple and has good
calibration properties. It was rejected because the operator
community familiar with this codebase already had Dempster-Shafer
expertise from prior work, the library implementations are sparser
in Rust, and the binary-frame Dempster-Shafer reduction is simpler
to reason about and proof-test. Subjective logic remains a credible
future swap; the swap would be an ADR change.

### Weighted-sum heuristic

Rejected. A weighted sum of per-stage scores has no principled
treatment of missing evidence (does a missing stage contribute 0,
0.5, or its weight times the prior?) and no error handling for
total conflict. It is fast to implement and fast to break.

## References

- Commit `327b1a3` — initial Verification Engine
  (`feat(verification): the architectural heart — 9-stage pipeline + real Dempster-Shafer fusion`)
- `services/verification-engine/src/domain/fusion.rs` (top-of-file
  doc + ~360 lines of math + 11 unit tests + proptest)
- `services/verification-engine/CLAUDE.md` (architectural-integrity
  section)
- `services/verification-engine/src/domain/lane.rs` — threshold logic
- Architecture V4 P14 § Stage 8 (fusion specification)
- Companion V4 P17 § pipeline orchestrator + Dempster-Shafer library
- Follow-up: `R-VER-2` through `R-VER-6` (real evidence in
  Stages 3-7), which exercises the fusion under realistic conditions
