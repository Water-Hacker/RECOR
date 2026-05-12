# recor-verification-engine

The architectural heart of RÉCOR. Takes a declaration snapshot,
processes it through nine stages, fuses the evidence via Dempster-Shafer,
routes to a lane (green / yellow / red).

## What works (v1, end-to-end smoke verified)

**Real implementations:**

| Stage | Role | v1 implementation |
|---|---|---|
| 1 — Schema validation | Deterministic | ✅ 5 invariant checks; short-circuits the pipeline on fail |
| 2 — Identity authentication | BUNEC lookup | ✅ Postgres-backed mock BUNEC; weighted authenticity BPA from per-owner outcomes |
| **3-7 — Sanctions, PEP, adverse-media, pattern, cross-source** | Evidence sources | ⏸ honest stubs that return `InsufficientEvidence` (vacuous BPA); follow-up tickets `R-VER-2` through `R-VER-6` wire each to a real data source + Tier A/B AI reasoning |
| 8 — Dempster-Shafer fusion | Math | ✅ real basic-probability-assignment combination via Dempster's rule of combination, with Yager fallback on total conflict; ~200 lines of pure Rust + 11 unit tests + property tests |
| 9 — Lane routing | Threshold logic | ✅ green/yellow/red decision over fused authenticity + risk BPAs |

The pipeline is end-to-end functional today. Stages 3-7 contribute vacuous mass (i.e. they don't update the fused belief either way), which the Dempster-Shafer math handles correctly — ignorance is a first-class citizen in the formalism.

## What's verified

- **30 unit tests pass** (`cargo test --lib`)
- **End-to-end smoke** runs the pipeline against two distinct cases:
  - Case A: both owners present in BUNEC → lane green/yellow
  - Case B: ghost owner not in BUNEC → lane red
- Pipeline persistence: `verification_cases` table holds every case; outbox row written for future Kafka relay

## Quick start

```bash
cd services/verification-engine
echo "RECOR_DB_PASSWORD=$(openssl rand -base64 24)" > .env
docker compose up -d --build
./scripts/smoke.sh
```

Service listens on `127.0.0.1:8081`. Postgres on `127.0.0.1:5433` (different port from declaration service's 5432 so both can run simultaneously).

## API

### POST /v1/verifications

Request:
```json
{
  "declaration": {
    "declaration_id": "...",
    "entity_id": "...",
    "declarant_principal": "spiffe://recor.cm/...",
    "declarant_role": "self",
    "kind": "incorporation",
    "effective_from": "2026-01-01",
    "beneficial_owners": [
      {"person_id": "...", "ownership_basis_points": 10000, "interest_kind": "equity"}
    ],
    "attestation_signed_by": "...",
    "attestation_signature_hex": "<128 hex chars>",
    "attestation_public_key_hex": "<64 hex chars>",
    "receipt_hash_hex": "<64 hex chars>",
    "correlation_id": "...",
    "submitted_at": "2026-05-11T22:00:00Z"
  }
}
```

Response (201 Created):
```json
{
  "case_id": "...",
  "lane": "yellow",
  "authenticity_belief": 0.3,
  "authenticity_plausibility": 0.94,
  "risk_belief": 0.0,
  "total_duration_ms": 15,
  "case_url": "http://localhost:8081/v1/verifications/..."
}
```

### GET /v1/verifications/{case_id}

Returns the full case record: every stage outcome with structured evidence, fused BPAs, lane decision, timestamps.

### GET /healthz, /readyz

Public liveness / readiness probes.

## Dempster-Shafer in one paragraph

The Architecture chose Dempster-Shafer over Bayesian probability for one reason: Bayesian forces you to pick a prior on the absence of negative evidence. In adversarial verification the dominant epistemic state is "several weak positive signals, no negative signals" — the prior you pick dominates the outcome and you can't defend the choice. Dempster-Shafer lets us put mass on the universal set (Θ = {True, False}) explicitly: that mass is *ignorance*, and the math propagates it through fusion correctly. The lane router then uses the *gap between belief and plausibility* (= ignorance) as a separate input to the decision; "high authenticity belief but high ignorance" routes to yellow (analyst review), not green.

## Doctrines

- **D01** completeness — pipeline + persistence + auth + observability + tests + Docker + smoke
- **D04** tests — 30 unit tests; property tests for the fusion math
- **D05** docs — README + CLAUDE.md + inline rustdoc on every public type
- **D08** no dangling threads — every stub stage names its follow-up ticket
- **D14** fail-closed — pipeline short-circuits on Stage 1 fail; lane defaults to Red on total conflict
- **D15** cryptographic provenance — receipt hash + signature carried through from declaration; outbox row writes verification-completed event
- **D16** observability — tracing crate, OTLP exporter (compatible with F-007)
- **D17** zero trust — every protected handler requires verified `Principal`
- **D18** no secrets — `.env` gitignored, fail-closed on missing password
- **D22** Anthropic-primary inference — Stages 5+7 stubs will integrate via R-VER-4/R-VER-6 follow-ups; no direct Anthropic SDK use in this commit because Stages 3-7 are not yet implemented

## Follow-up tickets

- **R-VER-1** — Real BUNEC adapter (replaces PostgresMockBunec)
- **R-VER-2** — Stage 3: sanctions feed ingestion + Tier A reasoning
- **R-VER-3** — Stage 4: PEP feeds (commercial + sovereign domestic register)
- **R-VER-4** — Stage 5: adverse-media + ICIJ corpus + Tier A RAG reasoning
- **R-VER-5** — Stage 6: 8 pattern-detection signatures + Neo4j ownership-graph
- **R-VER-6** — Stage 7: cross-source triangulation (ARMP/DGI/concession cadastres)
- **R-VER-7** — Full OIDC JWT verification (currently stub like declaration service)
- **R-VER-8** — Kafka consumer of declaration outbox (today: receives via direct HTTP POST)
- **R-VER-9** — Verification outcome relay to declaration service (closes the loop so declarations transition from "submitted" to "accepted"/"rejected"/"in_verification")

## Architecture reference

V4 P14 § Verification engine: stage order, BPA per stage, fusion math, lane thresholds. This service implements the structural skeleton; Stages 3-7 are the AI-driven evidence sources that future tickets fill in.
