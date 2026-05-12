# recor-declaration

The first real platform service of RÉCOR — accepts beneficial-ownership
declarations, persists them durably, returns a signed receipt.

## What this service does (today)

- **POST `/v1/declarations`** — accept a declaration, validate it against
  the canonical domain invariants, verify the declarant's Ed25519
  signature over the canonical request bytes, persist as an
  event-sourced aggregate, return a signed receipt with the declaration
  id and the BLAKE3 receipt hash.
- **GET `/v1/declarations/{declaration_id}`** — return the current
  projection of a declaration, scoped to the requesting principal.
- **GET `/healthz`** — liveness, public.
- **GET `/readyz`** — readiness, checks database reachability, public.

Storage is event-sourced PostgreSQL: every state change appends to
`declaration_events`; a `declarations` projection is upserted in the
same transaction; an outbox row is written for downstream Kafka relay
(relay worker is a separate ticket).

## What this service does NOT do (yet)

- **Verification.** The Stages 1–9 verification engine is a separate
  service. Declarations submitted here enter `submitted` state; the
  verification engine consumes the outbox events and runs the pipeline.
- **Kafka outbox relay.** Outbox rows are written but not yet relayed
  to Kafka. Relay is a separate ticket.
- **OIDC JWT verification.** The auth surface exists but the JWT path
  currently peeks at claims without signature verification (dev path).
  Production OIDC ticket replaces this with full JWKS verification.
- **Person / Entity resolution.** Declarations carry person and entity
  identifiers but the Person and Entity services aren't yet
  integrated; the verification engine validates these against the
  canonical records.
- **Graph projection.** The Neo4j ownership-graph projection runs in
  the Ownership service.

## Quick start

```bash
# In services/declaration/
cp .env.example .env
echo "RECOR_DB_PASSWORD=$(openssl rand -base64 24)" >> .env

# Bring up Postgres + service
docker compose up -d --build

# Smoke test (POST + GET end-to-end with a real Ed25519 signature)
./scripts/smoke.sh

# Tear down
docker compose down -v
```

The service listens on `127.0.0.1:8080`. Migrations run automatically
at startup.

## Architecture in one diagram

```
┌─────────────┐                                          ┌─────────────────┐
│ Declarant   │ ── POST /v1/declarations ──────────────► │ recor-declarat. │
│ (anywhere)  │ ◄── 201 + receipt (decl_id, hash) ────── │                 │
└─────────────┘                                          │   ┌────────┐    │
                                                         │   │ aggreg │    │
                                                         │   │  ate   │    │
                                                         │   └───┬────┘    │
                                                         │       │         │
                                                         │   ┌───▼────┐    │
                                                         │   │ pg-tx  │    │
                                                         │   └───┬────┘    │
                                                         └───────┼─────────┘
                                                                 ▼
                                                ┌─────────────────────────────┐
                                                │ declaration_events (event)  │
                                                │ declarations      (proj)    │
                                                │ outbox            (relay)   │
                                                │ idempotency_records         │
                                                └─────────────────────────────┘
```

## Domain model

| Type | Module | Notes |
|------|--------|-------|
| `DeclarationId`, `EntityId`, `PersonId` | `domain::value_object` | UUIDv7 newtypes |
| `OwnershipBasisPoints` | `domain::value_object` | 0..=10_000; basis points not floats |
| `BeneficialOwnerClaim` | `domain::value_object` | person + ownership + interest kind |
| `CryptographicAttestation` | `domain::attestation` | Ed25519 signature + verifier |
| `SubmitDeclaration` | `domain::command` | input command |
| `DeclarationEvent::Submitted` | `domain::event` | versioned (`v1`) |
| `DeclarationAggregate` | `domain::aggregate` | event-sourced; handle + apply |
| `DomainError` | `domain::error` | invariant violations |

Invariants enforced by the aggregate:

- ≥ 1 beneficial owner per declaration
- Sum of ownership basis points = 10_000 (100%) for v1
- No duplicate `person_id` within a single declaration
- `effective_from` is in the past 5 years and not after `submitted_at`
- A `declaration_id` accepts `Submit` only once (use `Amend` next sprint)
- `attestation.signed_by` equals `declarant_principal`

## API contract details

### POST /v1/declarations

Headers:
- `Content-Type: application/json`
- `X-Recor-Dev-Principal: <spiffe-or-oidc-sub>` (dev only)
  OR `Authorization: Bearer <jwt>` (production)
- `Idempotency-Key: <opaque>` (optional; replay returns the original receipt)

Body (see `src/api/dto.rs` for the canonical Serde shape):

```json
{
  "declaration_id": "018f0000-0000-7000-0000-000000000001",
  "entity_id":      "018f0000-0000-7000-0000-000000000002",
  "declarant_role": "self",
  "kind":           "incorporation",
  "effective_from": "2026-01-01",
  "beneficial_owners": [
    {
      "person_id": "018f0000-0000-7000-0000-000000000003",
      "ownership_basis_points": 10000,
      "interest_kind": "equity"
    }
  ],
  "attestation": {
    "signed_by": "spiffe://recor.cm/declarant-001",
    "signature_algorithm": "ed25519",
    "signature_hex": "<hex>",
    "public_key_hex": "<hex>",
    "nonce_hex": "<hex>"
  }
}
```

Response: `201 Created`

```json
{
  "declaration_id": "...",
  "state": "submitted",
  "receipt_hash_hex": "<64 hex chars; BLAKE3>",
  "submitted_at": "2026-05-11T18:30:00Z",
  "receipt_url": "http://localhost:8080/v1/declarations/..."
}
```

Errors:

| Status | Kind | When |
|--------|------|------|
| 400 | `bad_request` | Schema / canonical-form invalid |
| 401 | `authentication_required` | No principal in header |
| 401 | `bad_attestation` | Signature didn't verify against the canonical bytes |
| 403 | `forbidden` | Attestation principal doesn't match the auth principal |
| 409 | `conflict` | declaration_id already submitted |
| 409 | `idempotency_conflict` | Same idem key, different body |
| 422-ish (400) | `bad_request` | Domain invariant violation (owner sum, etc.) |

### GET /v1/declarations/{declaration_id}

Returns the current projection. Authorisation: the requesting principal
must equal the declaration's `declarant_principal` (v1 — future Access
service overrides this).

## Configuration (env)

| Var | Default | Notes |
|-----|---------|-------|
| `BIND_ADDR` | `0.0.0.0:8080` | |
| `DATABASE_URL` | _required_ | Postgres connection string |
| `DB_POOL_MAX_CONNECTIONS` | `10` | |
| `IDEMPOTENCY_TTL_SECONDS` | `86400` | 24h |
| `OTLP_ENDPOINT` | `` | Empty disables OTLP export |
| `LOG_FILTER` | `info,recor_declaration=debug,sqlx=warn` | RUST_LOG syntax |
| `SERVICE_NAME` | `recor-declaration` | OTel resource attribute |
| `ENVIRONMENT` | `dev` | `dev`/`staging`/`prod` |
| `OIDC_ISSUER_URL` | `` | Required when `ENVIRONMENT != dev` |
| `HTTP_TIMEOUT_SECONDS` | `10` | |

## Testing

```bash
# Unit tests (no Docker needed)
cargo test --lib

# Integration tests against testcontainers Postgres
# (requires Docker daemon)
cargo test --test api_integration -- --ignored

# Full local end-to-end smoke
./scripts/smoke.sh
```

## Observability

The service emits OpenTelemetry traces when `OTLP_ENDPOINT` is set
(e.g. to the F-007 OTel Collector at `http://otel-collector:4317`).
Without it, structured JSON logs go to stdout.

Span attributes on the hot paths:
- `declaration_id`, `entity_id`, `declarant_principal`
- `correlation_id` (per request)
- `event_type`, `expected_version` on `save_event`

## Doctrines applied

- D01 — completeness: real auth surface, real attestation, real
  persistence, real idempotency, real observability
- D04 — tests: aggregate unit tests, attestation unit tests,
  integration tests against testcontainers Postgres
- D05 — documentation: this README, inline rustdoc on every public
  function
- D08 — no dangling threads: every TODO has a clear next-ticket scope
- D12 — production-grade from first commit: no scaffolds; auth /
  signatures / idempotency / observability all real
- D13 — idempotency: `Idempotency-Key` header + transactional record
- D14 — fail-closed: malformed request, bad attestation, conflict all
  return 4xx; never silently 2xx
- D15 — cryptographic provenance: Ed25519 attestation; BLAKE3 receipt
  hash; the receipt is itself a verifiable claim about what was
  submitted
- D16 — observability: tracing crate, OTLP exporter, structured logs
- D17 — zero trust: every authenticated request carries a verified
  principal in extensions; handlers fail to compile without it
- D18 — no secrets: `.env` is gitignored; smoke generates ephemeral
  passwords

## What's next (follow-up tickets)

- `R-DECL-1` — OIDC JWT verification with JWKS rotation
- `R-DECL-2` — Outbox relay worker (publishes to Kafka)
- `R-DECL-3` — Amend / Correction / Supersede commands
- `R-DECL-4` — Person / Entity service integration (validate the ids)
- `R-DECL-5` — Verification engine integration (consume outbox, run
  Stages 1–9, write back `accepted`/`rejected` state)
- `R-LANG-1` — Bump V3 P12 toolchain pin to 1.88.0, migrate Cargo.toml
  to edition 2024
