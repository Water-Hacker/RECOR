# Service: recor-declaration
# Layer: 2 (Architecture V4 P13 § Declaration Service)
# Owner: @recor/domain-team
# Doctrines reference: V1 P2

## What this service does

Accepts beneficial-ownership declarations from the Declarant Portal (and
other authorised clients), validates the canonical domain invariants,
verifies the declarant's Ed25519 cryptographic attestation, persists the
declaration event-sourced in PostgreSQL with an outbox row for
downstream relay, returns a signed receipt.

The service is the entry point of the platform's declared-data flow.
What it captures, the verification engine processes; what the
verification engine accepts, the consumers consume. Failures here
fail-close at the boundary.

## Language and toolchain

- Rust 1.88.0 (rust-toolchain.toml) — bumped from V3 P12's 1.84.0 because
  the modern dep graph requires edition 2024 stable. See R-LANG-1.
- Cargo workspace at the service root (not yet wired to the monorepo
  workspace — `R-DECL-6` follow-up).
- Build via Docker: `docker run -v $PWD:/work -w /work rust:1.88-bookworm cargo ...`
- Test: `cargo test --lib` (unit, fast) or `cargo test --test api_integration -- --ignored` (testcontainers, slower, needs docker daemon).
- Lint: `cargo clippy --all-targets -- -D warnings`.

## Architecture

- Persistence: **PostgreSQL 17** via `sqlx 0.8` (runtime-checked queries,
  not the compile-time `query!` macro — see migration files and
  `R-DECL-7` follow-up for moving to `.sqlx/` cache).
- Events: declarations are event-sourced. Today: `declaration.submitted.v1`.
- **Audit immutability (COMP-2):** `declaration_events` is enforced
  append-only by BEFORE UPDATE/DELETE/TRUNCATE triggers (migration
  `0007_audit_log_immutability.sql`) + REVOKE on PUBLIC. The outbox
  retention worker (`infrastructure/retention.rs`) prunes
  `outbox` rows 30 days after `dispatched_at`; the event log and DLQ
  are NEVER touched. See `docs/compliance/data-retention.md`.
- Outbox: every event is written to `outbox` in the same transaction; a
  future outbox-relay worker publishes to Kafka.
- Kafka transport (R-LOOP-2): `infrastructure::kafka_producer` publishes
  the same outbox rows to `recor.declaration.events.v1` keyed by
  `aggregate_id`. Gated by `RELAY_TRANSPORT=http|kafka` (default `http`);
  set `KAFKA_BROKERS` + `RELAY_TRANSPORT=kafka` to spawn the producer
  alongside the HTTP relay during the cutover (see ADR-0007).
- gRPC contracts: `contracts/declaration.proto` (R-DECL-8 / #78).
  tonic-based server bound on `GRPC_BIND_ADDR` (default empty →
  disabled; production uses `0.0.0.0:9080`). Same OIDC verifier as
  REST via a tonic interceptor, and the canonical-payload bytes are
  byte-parity with REST so a single signature is valid under either
  transport (D15). The gRPC surface is intentionally NOT in the
  OpenAPI spec — different transport, different shape; the `.proto`
  is the source of truth for gRPC types while REST DTOs remain
  hand-written under utoipa. V-engine's gRPC surface is deferred to
  `R-VER-GRPC` (TODO marker in `services/verification-engine/src/api/mod.rs`).
- Public APIs: REST under `/v1/declarations` (see `src/api/rest.rs` and
  the dto module) + gRPC `recor.declaration.v1.DeclarationService`
  (see `src/api/grpc.rs`).
- **Audit anchoring (R-DECL-9):** every declaration event is
  asynchronously anchored to the Hyperledger Fabric `recor-audit`
  channel via the `worker-fabric-bridge` app
  (`apps/worker-fabric-bridge/`). The bridge consumes from the same
  outbox-relay channel as the verification engine, calls the
  `audit-witness` chaincode (`chaincode/audit-witness/`) through the
  Fabric Gateway HTTP shim, and dead-letters permanent failures to
  `fabric_bridge_dlq`. The verifier app (`apps/audit-verifier/`)
  re-derives the receipt hash from the projection and compares to
  the on-chain entry, exposing
  `GET /v1/audit/verify/{declaration_id}` for operator and public
  verification. Closes Gap G1 in `docs/security/threat-model.md`.
  See `docs/adr/0009-fabric-audit-anchoring.md`,
  `docs/runbooks/fabric-bridge.md`, and
  `docs/runbooks/audit-verification.md`.
- GDPR data-subject access: `GET /v1/declarations/by-principal`
  returns every declaration submitted by the authenticated principal
  (COMP-1). Principal sourced from auth, never from request; see
  `docs/compliance/gdpr-procedures.md` for the full right-of-access /
  rectification / erasure / portability procedures.

## SLOs

| Operation | p99 latency | Availability |
|-----------|-------------|--------------|
| `POST /v1/declarations` | < 500 ms | 99.95% |
| `GET /v1/declarations/{id}` | < 50 ms | 99.95% |
| `GET /healthz` | < 10 ms | 100% |
| `GET /readyz` | < 100 ms | 99.95% |

These are aspirational for v1; measured baselines establish during the
first sprint of operational traffic.

The legal basis for each endpoint above (and for the load-bearing
domain invariants the SLOs depend on) is mapped in
`docs/compliance/regulatory-mapping.md` (COMP-4).

## Active development context

- This is the first commit of platform code. Many "ideal" features are
  deferred to subsequent tickets — see the follow-ups list at the bottom
  of the service README. Do not silently add to scope without an ADR.
- OIDC JWT verification is real: `src/api/oidc.rs` does discovery,
  JWKS fetching with TTL caching, signature + `iss` + `aud` + `exp` +
  `nbf` verification. HMAC algorithms (HS256/384/512) are refused
  outright — algorithm-confusion attacks have nothing to land on.
  Production refuses to start when `ENVIRONMENT != dev` and
  `OIDC_ISSUER_URL` is empty; `OIDC_AUDIENCE` is required whenever
  `OIDC_ISSUER_URL` is set. R-DECL-1 is CLOSED (was: peeking at claims
  unverified).
- The integration tests are `#[ignore]`-gated; CI must use the
  testcontainers Docker socket pattern to un-ignore them. Today: run
  manually with `cargo test -- --ignored`.

## Doctrines that apply with special weight here

- **D13 idempotency** — `POST /v1/declarations` honours the
  `Idempotency-Key` header; replay returns the same receipt. Adding a
  new state-changing endpoint here without idempotency is a doctrine
  violation. Talk to the architect-reviewer if you think a new endpoint
  doesn't need it.
- **D15 cryptographic provenance** — Every declaration carries an
  Ed25519 attestation. Receipt hash is BLAKE3 over the canonical form.
  Receipts are anchored to the Hyperledger Fabric audit channel via
  the bridge worker (R-DECL-9, shipped). Tampering on the projection
  is detectable via the audit-verifier app.
- **D18 no secrets** — The Postgres password lives in `.env`
  (gitignored). The service refuses to start without `DATABASE_URL`.
- **D14 fail-closed** — Any malformed request, bad attestation,
  conflict, or downstream failure returns 4xx/5xx. Never 2xx on a
  partial success.

## When in doubt

1. Read this document.
2. Read `docs/architecture/` V4 P13 § Declaration Service.
3. Read `docs/architecture/` V4 P14 § Canonical Data Model.
4. Check the ADR record in `/docs/adr/`.
5. Ask the lead architect — do not improvise.
