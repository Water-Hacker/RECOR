# Service: recor-verification-engine
# Layer: 3 (Architecture V4 P14)
# Owner: @recor/verification-team
# Doctrines reference: V1 P2

## What this service does

Receives declaration snapshots, processes them through nine stages
(schema validation → identity → sanctions → PEP → adverse media →
patterns → cross-source → fusion → lane), returns a verification case
with authenticity belief, risk belief, and a lane decision.

The verification engine is RÉCOR's load-bearing capability. If
verification fails to catch the dominant adversarial failure modes,
the rest of the platform serves authoritative-sounding data that is
in fact wrong, which is a worse outcome than no platform.

## Language and toolchain

- Rust 1.88.0 (rust-toolchain.toml)
- Cargo workspace at the service root
- Build via Docker: `docker run rust:1.88-bookworm cargo ...`
- Test: `cargo test --lib`

## Architecture

- Persistence: PostgreSQL via sqlx (runtime-checked queries)
- **Audit immutability (COMP-2):** `verification_cases` is enforced
  append-only by BEFORE UPDATE/DELETE/TRUNCATE triggers (migration
  `0003_audit_log_immutability.sql`) + REVOKE on PUBLIC. The
  retention worker (`infrastructure/retention.rs`) prunes
  `verification_outbox` rows 30 days after `dispatched_at`; the case
  log and DLQ are NEVER touched. See `docs/compliance/data-retention.md`.
- Stages: traits implemented in `src/application/stages/`. Stages 1-2
  ship real implementations; 3-7 are honest stubs.
- Fusion: pure Dempster-Shafer math in `src/domain/fusion.rs`; no I/O,
  property-tested.
- Lane routing: pure threshold logic in `src/domain/lane.rs`.
- Mock BUNEC: Postgres-backed in `src/infrastructure/mock_bunec.rs`;
  real BUNEC integration is `R-VER-1`.

## SLOs

| Operation | p99 latency | Availability |
|-----------|-------------|--------------|
| `POST /v1/verifications` (all stubs vacuous) | < 200 ms | 99.95% |
| `POST /v1/verifications` (real Stages 3-7 wired) | < 30 s | 99.9% |
| `GET /v1/verifications/{id}` | < 50 ms | 99.95% |

The 30s target reflects the Architecture's commitment: green-lane
declarations complete pipeline in 30s p99 (V4 P14 § Operational
characteristics).

## Doctrines that apply with special weight here

- **D14 fail-closed** — pipeline short-circuits on Stage 1 fail. Lane
  router defaults to Red on total Dempster conflict.
- **D15 cryptographic provenance** — declaration's receipt hash +
  attestation hex carry through into the case record. Future ticket
  anchors case records to Fabric audit channel.
- **D22 Anthropic-primary inference** — Stages 5+7 stubs name the
  follow-up tickets that will integrate via the Inference Gateway.
  No direct Anthropic SDK use in this commit; Tier A/B reasoning
  arrives in `R-VER-2` through `R-VER-6`.
- **Architectural integrity** — the Dempster-Shafer fusion is the
  centrepiece. Any change to its math (e.g. swapping Yager for PCR6)
  is an ADR-required decision, not a casual modification.

## When in doubt

1. Read this document.
2. Architecture V4 P14 § Verification Engine (the full stage spec).
3. Companion V4 P17 § pipeline orchestrator + Dempster-Shafer library.
4. Architecture V5 P18 § AI inference (for the Stages 3-7 follow-ups).
5. Ask the verification-team-specialist agent (per `.claude/agents/`).
