# Service: recor-person-service

Owner: @recor/domain-team
Doctrines reference: V1 P2

## What this service does

Canonical natural-person registry. Anchors every `person_id` referenced
inside the Declaration service's `beneficial_owners` payload so that
those identifiers point at a real, audited record rather than at an
arbitrary UUID the declarant invented.

Closes the structural part of ticket **R-DECL-4**. NDI (Cameroonian
national-ID) integration is intentionally deferred — that path requires
a government agreement that is not yet in place. The TODO marker is in
`src/infrastructure/postgres.rs` near the `primary_id_document` handling.

The service is event-sourced (the `Person` aggregate emits
`PersonRegistered`, `PersonUpdated`, `PersonMerged` events) and the
`persons` table is a derived projection rebuilt by replaying events.
This shape mirrors `services/declaration` so the two services share an
operational surface.

## Language and toolchain

- Rust 1.88.0 (rust-toolchain.toml).
- Cargo workspace member at `services/person-service` — wired into the
  workspace root `Cargo.toml` `members` list.
- Build via Docker: `docker run -v $PWD:/work -w /work rust:1.88-bookworm cargo ...`.
- Test: `cargo test --lib -p recor-person-service` (unit) /
  `cargo test --test '*' -- --ignored` (testcontainers-backed, slower).
- Lint: `cargo clippy --all-targets -- -D warnings`.

## Architecture (four-layer separation)

```
  src/domain          — pure types + invariants, no I/O
    ↓
  src/application     — use-case orchestrators over the repository port
    ↓
  src/infrastructure  — Postgres adapter + outbox
    ↓
  src/api             — axum HTTP surface + OpenAPI annotations
```

- **Persistence:** PostgreSQL 17 via `sqlx 0.8`. v1 uses runtime-checked
  `sqlx::query` (not the compile-time `query!` macro); the follow-up
  ticket **R-PERSON-SQLX-CACHE** flips this crate to the offline
  `.sqlx/` cache for D19 reproducibility, the same way R-DECL-7 did for
  the Declaration service.
- **Events:** `PersonRegisteredV1`, `PersonUpdatedV1`, `PersonMergedV1`.
- **Audit immutability (COMP-2):** `person_events` is enforced
  append-only by BEFORE UPDATE/DELETE/TRUNCATE triggers + REVOKE on
  PUBLIC. See `migrations/0001_init.sql`.
- **Outbox:** every event writes a row to the `outbox` table in the
  same transaction. The outbox shape mirrors the Declaration service
  so the relay/retention workers are reusable across services.
- **Public APIs (REST):**
  - `POST   /v1/persons` — register; OIDC-authenticated; honours
    `Idempotency-Key` (D13).
  - `GET    /v1/persons/{id}` — read projection.
  - `GET    /v1/persons/search?q=&nationality=&limit=` — ILIKE + exact-
    match nationality; capped at 50 results. Trigram-based fuzzy
    matching is the **R-PERSON-FUZZY** follow-up (marker in the
    Postgres adapter).
  - `POST   /v1/persons/{id}/merge-into/{target_id}` — admin allowlist
    gated; emits `PersonMergedV1`.
  - `GET    /healthz`, `/readyz`, `/metrics` — operational.
  - `GET    /openapi.json`, `/docs` — DOC-1 spec + Scalar UI.

## SLOs

| Operation                                              | p99 latency | Availability |
| ------------------------------------------------------ | ----------- | ------------ |
| `POST /v1/persons`                                     | < 500 ms    | 99.95%       |
| `GET /v1/persons/{id}`                                 | < 50 ms     | 99.95%       |
| `GET /v1/persons/search`                               | < 200 ms    | 99.9%        |
| `POST /v1/persons/{id}/merge-into/{target_id}`         | < 500 ms    | 99.9%        |
| `GET /healthz`                                         | < 10 ms     | 100%         |
| `GET /readyz`                                          | < 100 ms    | 99.95%       |

Aspirational for v1; baselines establish during the first sprint of
operational traffic.

## Doctrines that apply with special weight here

- **D13 idempotency.** `POST /v1/persons` honours `Idempotency-Key`.
  Same request hash → same response replayed; mismatched body with a
  reused key → 409 `idempotency_conflict`.
- **D14 fail-closed.** Invalid nationality, empty name, oversize names,
  malformed biometric hash, double-merge, self-merge — every domain
  invariant violation maps to a 4xx at the API boundary. There is no
  partial-success path.
- **D15 cryptographic provenance — limited in v1.**
  Person events do **NOT** carry a declarant-supplied Ed25519
  attestation in v1. Unlike declarations, where the declarant signs the
  canonical declaration body, the Person registry is operator-curated:
  the per-event provenance is the authenticated `actor_principal`
  recorded on every event plus the append-only audit chain enforced by
  the COMP-2 triggers. Per-event cryptographic attestation lands in a
  follow-up ticket once the operator-side signing infrastructure is in
  place (likely `R-PERSON-ATTEST`). The follow-up will be additive:
  events keep their current shape and gain an optional `attestation`
  field; replay is forward-compatible.
- **D17 zero trust.** Every state-changing endpoint sources its
  `actor_principal` from the verified OIDC subject (or, in dev, the
  `X-Recor-Dev-Principal` header). Request-body principal fields are
  ignored.
- **D18 no secrets.** Postgres password / OIDC discovery URL / log-
  redaction key all live in `.env` (gitignored). The service refuses to
  start without `DATABASE_URL`. Prometheus labels are bounded enums —
  no UUIDs, principals, or names are ever label values.

## Active development context

- This is the first commit of the Person service. Many "ideal"
  features are deferred to subsequent tickets:
  - **R-PERSON-SQLX-CACHE** — flip to compile-time-checked `query!`
    macros + committed `.sqlx/` cache.
  - **R-PERSON-FUZZY** — pg_trgm trigram similarity on
    `canonical_full_name`.
  - **R-PERSON-RBAC** — per-field ABAC for refining what *which* row a
    caller can read. The per-ROW gate already lands as part of the
    audit Sprint 1 follow-up (FIND-005 + FIND-006, migration 0002 +
    `created_by_principal` column + handler enforcement): admin sees
    every row; non-admin sees only rows they themselves registered.
    The ABAC follow-up tightens this further with per-field
    redaction once a documented permissions model exists.
  - **R-PERSON-ATTEST** — per-event Ed25519 attestation by the actor.
  - **R-ENC-FIELD-LEVEL** — field-level encryption-at-rest on the
    Sensitive-PII columns (`primary_id_document`,
    `biometric_reference_hash`).
  - **NDI-1** — Cameroonian national-ID integration (TODO marker in
    `src/infrastructure/postgres.rs`).
- The integration tests under `tests/` are `#[ignore]`-gated; CI must
  use the testcontainers Docker socket pattern to un-ignore them.

## Cross-service integration

The Declaration service's `SubmitDeclaration` use case validates each
`beneficial_owner.person_id` against the Person registry through the
`PersonRegistryPort` trait (see
`services/declaration/src/application/port.rs`). The HTTP adapter
`PersonRegistryHttpAdapter` is the production wiring. Validation is
gated behind `PERSON_SERVICE_URL`: an empty value skips validation (the
test/dev-default posture) and is acceptable only until the Person
service is generally available. The domain error returned on a
non-resolving id is `BeneficialOwnerNotInPersonRegistry`.

## When in doubt

1. Read this document.
2. Read `services/declaration/CLAUDE.md` — most patterns are mirrored.
3. Read `docs/compliance/data-classification.md` §
   `[PLANNED] services/person-service` for the per-column PII /
   Sensitive-PII rules.
4. Ask the lead architect — do not improvise.
