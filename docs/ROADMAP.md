# RÉCOR — Roadmap

This document is the canonical record of work that is **not yet started or
not yet complete**. It replaces the GitHub Issues queue as the source of
truth for "what's deferred" so that the queue stays empty and signal-rich
(open issues = bugs / immediate-todo, not multi-month aspirations).

Each item carries the original ticket number, a one-line statement of
scope, the **external dependency** that blocks autonomous implementation
(if any), and a rough size estimate.

Ranking inside each track is by importance to production-readiness, not
filing order.

---

## Track V — Verification pipeline (the load-bearing capability)

The verification engine ships with Stages 1-2 real, Stages 3-7 honest stubs.
Closing this track makes the pipeline actually adversarial. Without it,
the platform serves authoritative-sounding data that the engine cannot
yet contradict.

| Ticket | Scope | External dep | Size |
|---|---|---|---|
| **R-VER-1** (#16) | Real BUNEC adapter — replace `PostgresMockBunec` with HTTP/SOAP integration against the Cameroon national business register. | BUNEC API access agreement (gov-to-gov), endpoint + auth credentials | ~2 weeks |
| **R-VER-2** (#17) | Stage 3 sanctions screening — OFAC SDN, UN consolidated, EU CFSP feeds. | Feed subscriptions or open-data ingestion pipeline | ~2 weeks |
| **R-VER-3** (#18) | Stage 4 PEP screening — domestic register (sovereign) + commercial backup. | Worldcheck/Refinitiv or equivalent + domestic PEP register agreement | ~3 weeks |
| **R-VER-4** (#19) | Stage 5 adverse media + ICIJ leaked-data — Anthropic-primary retrieval+reasoning per D22. | ICIJ data licence + Anthropic API key + retrieval index | ~4 weeks |
| **R-VER-5** (#20) | Stage 6 pattern detection — 8 signature classes + graph queries. | Neo4j cluster (or alternative graph DB) + signature implementations | ~3 weeks |
| **R-VER-6** (#21) | Stage 7 cross-source triangulation — fuses outputs of Stages 3-6 with declaration self-claim. | All upstream stages must be real for this to be meaningful | ~1 week (after V2-V5) |

**Architectural note**: The Dempster-Shafer fusion math (Stage 8) and lane
router (Stage 9) are **already real and tested**. The track is about
replacing the BPA-emitting stubs with real evidence sources; the fusion
behaviour is correct now and will remain correct as evidence quality
improves.

---

## Track P — Declarant Portal (UX completeness)

The portal ships with a working signed-submission path. The remaining
items make it production-grade for the sovereign-citizen UX commitment.

| Ticket | Scope | External dep | Size |
|---|---|---|---|
| **R-PORT-1** (#26) | i18n — French primary, English secondary, Cameroonian Pidgin tertiary. | Translation review (gov-approved French legal terminology) | ~1 week |
| **R-PORT-2** (#27) | Offline drafts (IndexedDB via Dexie) + Workbox service worker. | None | ~1 week |
| **R-PORT-3** (#28) | Multi-step wizard (entity → owners → review → sign). | None | ~3 days |
| **R-PORT-4** (#29) | Verification status polling — show declaration's verification_state in the portal. | None (the verification_state column landed with D↔V Phase 2) | ~2 days |
| **R-PORT-5** (#30) | Full WCAG 2.1 AA audit + remediation. | a11y audit tooling + screen-reader testing time | ~1 week |
| **R-PORT-6** (#31) | Playwright E2E test suite against the built bundle. | None | ~3 days |
| **R-PORT-7** (#32) | Generated API client from Declaration service's OpenAPI spec. | OpenAPI spec must exist first (currently the API is hand-coded; spec generation is a sub-task) | ~3 days + spec work |

---

## Track D — Declaration service infrastructure

| Ticket | Scope | External dep | Size |
|---|---|---|---|
| **R-DECL-3** (#7) | Amend / Correction / Supersede commands (and matching events + aggregate handlers). | None — pure domain extension | ~1 week |
| **R-DECL-4** (#8) | Person + Entity service integration — beneficial_owners reference real records. | A Person service must exist first; that's its own ticket (filed as part of this) | ~3 weeks (Person service + integration) |
| **R-DECL-6** (#10) | Wire both service crates into a monorepo Cargo workspace at the repo root. Enables shared crates (R-AUTH-1, etc.). | None | ~2 days |
| **R-DECL-7** (#11) | Add `.sqlx` cache for compile-time-checked queries. | CI needs to provision Postgres at build time to regenerate the cache | ~1 day + CI changes |
| **R-DECL-8** (#12) | gRPC API alongside REST. Protobuf definitions, tonic server, parallel handlers. | Protobuf schema design review | ~1 week |
| **R-DECL-9** (#13) | Anchor receipts to Hyperledger Fabric audit channel (per D15). | Fabric cluster + channel config + chaincode | ~3 weeks |

---

## Track L — Cross-service communication (D↔V loop hardening)

Phase 1 (D→V) and Phase 2 (V→D) ship in PRs #38 and #39 over HMAC-signed
HTTP webhooks. The items below replace the transport, harden auth, and
add resilience to that transport.

| Ticket | Scope | External dep | Size |
|---|---|---|---|
| **R-LOOP-2** (#35) | Replace HTTP webhook transport with Kafka. Topics `recor.declaration.events.v1` + `recor.verification.events.v1`. | Kafka cluster (single-node OK for v1) | ~2 weeks |
| **R-LOOP-3** (#36) | Replace HMAC service-to-service auth with SPIFFE+mTLS. | SPIRE deployment + workload API on each service node | ~2 weeks |
| **R-LOOP-4-DLQ** (#37) | Dead-letter queue for outbox rows that exceed `max_attempts`. Today they sit in the table with `last_error`; this moves them to a separate DLQ table + an oncall alert. | None | ~3 days |
| **R-LOOP-4-ROT** (#41) | Per-channel HMAC secret rotation (two distinct envs for D↔V channels; rotation procedure). Closes the "single shared secret" gap that currently exists in dev compose. | Vault or AWS Secrets Manager | ~3 days |

---

## Track A — Authentication consolidation

| Ticket | Scope | External dep | Size |
|---|---|---|---|
| **R-AUTH-1** (#46) | Extract shared `recor-auth-oidc` crate to eliminate the duplicate verifier implementation in both services. | Depends on R-DECL-6 (monorepo workspace) — can't share crates across two independent Cargo packages | ~2 days (after R-DECL-6) |

R-AUTH-2 / 3 / 4 already shipped in PR #51.

---

## Track O — Observability operations

| Ticket | Scope | External dep | Size |
|---|---|---|---|
| **R-OBS-1** (#4) | Promote `observability-smoke` from informational to required check after 10 consecutive green runs against the dev compose. | 10 green runs in CI history | operational |

---

## Sequencing recommendation

If we were prioritising **what next would most increase production
readiness**, the order is:

1. **R-DECL-6 monorepo workspace** (#10) — unblocks R-AUTH-1 and makes
   shared crates possible.
2. **R-AUTH-1 shared crate** (#46) — eliminates the duplicate OIDC
   verifier; both services consume one implementation.
3. **R-DECL-7 sqlx cache** (#11) — moves to compile-time-checked
   queries; catches schema/query drift at build time.
4. **R-LOOP-4-DLQ** (#37) + **R-LOOP-4-ROT** (#41) — small, both make
   the existing D↔V channel more operable.
5. **R-PORT-4 verification status** (#29) — closes the user-facing
   feedback loop; the verification_state column is already there.
6. **R-DECL-3 amend/correction** (#7) — closes the only state-transition
   gap in the declaration lifecycle.
7. **R-VER track in order V1 → V2 → V3 → V4 → V5 → V6** — each requires
   external data agreements; cannot be parallelised against external
   counterparties.
8. **R-LOOP-2 Kafka** (#35) once a Kafka cluster is provisioned.
9. **R-DECL-9 Fabric anchoring** (#13) once a Fabric cluster is up.

Items needing **external partner agreements** (BUNEC, ICIJ, PEP feeds,
gov-approved translations) should start their procurement / partnership
process in parallel with the technical-only tracks; the platform code
is the smaller half of those line items.

---

## How this list updates

When a roadmap item is picked up:

1. Re-open the corresponding GitHub issue (the original numbers above
   remain reserved for traceability).
2. Move the item from this file into the issue body (or link from the
   issue back to this file).
3. Treat the issue as the working ticket; close it on merge of the
   resolving PR; update this file to strike the item.

Items added after this snapshot get a new ticket and an entry here in
the same shape.
