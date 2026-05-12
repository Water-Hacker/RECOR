# Service: recor-entity-service (IDENTITY-1)
# Layer: 2 (Architecture V4 § Entity Service)
# Owner: @recor/domain-team
# Doctrines reference: V1 P2

## What this service does

Authoritative cache + projection of legal-entity registry data. Accepts
entity-registration commands from authorised principals, validates the
canonical domain invariants, persists the entity event-sourced in
PostgreSQL with an outbox row for downstream relay, and surfaces a
projection at `/v1/entities/{id}`.

Companion to the (planned) Person service (R-DECL-4): the Declaration
service's beneficial-ownership rows reference both a `person_id` and an
`entity_id`. Today the `entity_id` is whatever UUID the declarant
invents; IDENTITY-1 makes the entity_id resolvable to a real registry
projection.

When the BUNEC adapter lands (R-VER-1), this service becomes the
authoritative cache + projection of BUNEC entries for Cameroonian
entities. Non-Cameroonian entities continue to hold declarant-submitted
data verified through the verification engine. The deferral is marked
with a `// TODO(R-VER-1):` comment in `src/infrastructure/postgres.rs`
near the registration handler and `src/api/rest.rs#register_entity`.

## Classification

Per `docs/compliance/data-classification.md` § entities, every column on
the `entities` table is Public except `created_at` / `updated_at`, which
are Internal. The entity event log and outbox carry no PII because legal
entities are not natural persons.

## Language and toolchain

- Rust 1.88.0 (workspace `rust-toolchain.toml`).
- Cargo workspace member: `services/entity-service`.
- Build via Docker for reproducibility: `docker run -v $PWD:/work -w /work rust:1.88-bookworm cargo ...`
- Test: `cargo test -p recor-entity-service --lib`.
- Integration: `cargo test -p recor-entity-service --test '*' -- --ignored`
  (testcontainers; requires the Docker daemon).
- Lint: `cargo clippy -p recor-entity-service --all-targets -- -D warnings`.

## Architecture

- Persistence: **PostgreSQL 17** via `sqlx 0.8` with the compile-time
  `query!` macro pattern. Build-time type checking against the committed
  `.sqlx/` offline cache (R-DECL-7 pattern). `SQLX_OFFLINE=true` is the
  production build invariant; CI runs `cargo sqlx prepare --check` to
  verify the committed cache is fresh.
- Events: entities are event-sourced. Three event variants:
  `entity.registered.v1`, `entity.updated.v1`, `entity.dissolved.v1`.
- **Audit immutability (COMP-2):** `entity_events` is enforced
  append-only by BEFORE UPDATE/DELETE/TRUNCATE triggers + REVOKE on
  PUBLIC (migration `0001_init.sql`). Identical pattern to
  `declaration_events` 0007.
- Outbox: every event is written to `outbox` in the same transaction;
  the existing outbox-relay pattern (declaration's
  `src/infrastructure/relay.rs`) can be wired in a follow-up ticket
  once consumer integration is defined.
- Identity-tuple uniqueness: the projection's UNIQUE
  `(jurisdiction, registration_number_in_jurisdiction)` enforces that
  no two RÉCOR `entity_id`s ever share an external-register handle. A
  violation surfaces at the API layer as `409 duplicate_identity_tuple`.

## REST surface

| Method | Path | Auth | Notes |
|--------|------|------|-------|
| POST | `/v1/entities` | OIDC | Register; honours `Idempotency-Key` |
| GET | `/v1/entities/{id}` | OIDC | Current projection |
| GET | `/v1/entities/search` | OIDC | `q=` + `jurisdiction=` + `type=` |
| POST | `/v1/entities/{id}/update` | OIDC | In-place update of mutable fields |
| POST | `/v1/entities/{id}/dissolve` | OIDC + admin allowlist | Records dissolution |
| GET | `/healthz`, `/readyz` | none | Probes |
| GET | `/metrics` | none | Prometheus exposition (in-cluster only — D17) |
| GET | `/openapi.json`, `/docs` | none | Public contract |

D14 fail-closed: every error path returns 4xx/5xx. D17 zero-trust: the
dissolve endpoint refuses callers not on the admin allowlist; an empty
allowlist disables the endpoint entirely (refuses every caller — fail
closed). D13 idempotency: `POST /v1/entities` honours `Idempotency-Key`
and replays return the original receipt with the original status code.

## SLOs (aspirational; v1 baselines TBD)

| Operation | p99 latency | Availability |
|-----------|-------------|--------------|
| `POST /v1/entities` | < 500 ms | 99.95% |
| `GET /v1/entities/{id}` | < 50 ms | 99.95% |
| `GET /v1/entities/search` | < 200 ms (per 50 rows) | 99.9% |
| `GET /healthz` | < 10 ms | 100% |
| `GET /readyz` | < 100 ms | 99.95% |

## Active development context

- This is a skeleton in scope of IDENTITY-1. The following are deferred
  to follow-up tickets:
  - **R-VER-1 — BUNEC adapter.** Wire BUNEC as source-of-truth for
    Cameroonian entities in the registration handler. Marker:
    `// TODO(R-VER-1):` in `src/infrastructure/postgres.rs` and in
    `src/api/rest.rs#register_entity`.
  - **Outbox relay.** Same shape as declaration's
    `src/infrastructure/relay.rs`; not wired here because the consumer
    integration is undefined for entity events. Outbox rows accumulate
    until the relay is wired.
  - **Outbox retention worker.** COMP-2-style retention job for the
    outbox (event log is forever-retained per D15; outbox after-
    dispatch retention follows declaration's 30-day pattern).
  - **gRPC surface.** REST only at v1. gRPC arrives when a v-engine or
    other service consumer needs it; reuse declaration's tonic +
    interceptor pattern.
  - **DLQ admin endpoints.** Same shape as declaration's `api/dlq.rs`;
    wired alongside the outbox relay.

## Doctrines that apply with special weight here

- **D01 completeness** — Skeleton covers domain + application +
  infrastructure + API + observability + metrics + tests + OpenAPI +
  Dockerfile in one delivery. Deferred work is explicitly enumerated
  above (not silent gaps).
- **D04 tests** — Unit tests on the aggregate (lifecycle, invariants),
  the value-objects (validation), the register use case (in-memory
  port double); integration smoke against a testcontainers Postgres.
- **D13 idempotency** — `POST /v1/entities` honours `Idempotency-Key`;
  replays return the same response with the same status code.
- **D14 fail-closed** — Every error path returns 4xx/5xx; empty admin
  allowlist refuses the dissolve endpoint entirely; an unknown
  jurisdiction is refused at the value-object boundary; an invalid
  dissolution date (before founded_at, or on an already-dissolved
  entity) is refused at the aggregate.
- **D15 cryptographic provenance** — entity_events is append-only with
  COMP-2 triggers; future Fabric anchoring lands when the audit-channel
  ticket lands (R-DECL-9 analogue).
- **D17 zero trust** — OIDC verification on every authenticated route;
  admin-allowlist gate on `dissolve`; the dev-principal header is
  refused outside `ENVIRONMENT=dev`.
- **D18 no secrets** — Postgres credentials in `DATABASE_URL` env only;
  OPS-2 redaction layer wired through `observability::init`; the
  redaction key is REQUIRED outside dev.

## When in doubt

1. Read this document.
2. Read `docs/PRODUCTION-TODO.md` § IDENTITY-1.
3. Read `docs/compliance/data-classification.md` § entities.
4. Read `services/declaration/CLAUDE.md` — the canonical 4-layer
   pattern this service mirrors.
5. Ask the lead architect — do not improvise.
