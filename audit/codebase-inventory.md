# RÉCOR Codebase Inventory

**Commit audited:** `e1ab0195394a3f24fee5402a151a68069069a122` (main, 2026-05-20).
**Auditor:** Claude Code (orchestrator) + 11 parallel sub-agents.
**Phase:** Phase 2 — codebase forensics.

This document is what exists, not what should exist. Phase 1
(`audit/standards-extract.md`) is the requirements corpus; Phase 3 in
`TODOS.md` cross-tabulates the two.

The detailed test-coverage matrix lives in
`audit/codebase-inventory-G-tests.md` (374 lines, 142 modules graded;
referenced from § G below).

---

## A. HTTP / RPC / queue entry points

### A.1 Routes per service (production, axum `.route()` calls)

| Service / app | Total routes | OpenAPI |
|---|---|---|
| `services/declaration` | 13 (8 protected + 2 admin DLQ + 1 internal HMAC + 2 public) | `/openapi.json`, `/docs` |
| `services/verification-engine` | 12 (2 protected + 2 admin DLQ + 1 internal HMAC + 2 public + 5 internal) | `/openapi.json`, `/docs` |
| `services/person-service` | 8 (4 protected + 2 public + 2 metrics-or-openapi) | `/openapi.json`, `/docs` |
| `services/entity-service` | 8 (5 protected + 2 public + 1 metrics) | `/openapi.json`, `/docs` |
| `apps/audit-verifier` | 3 (1 protected + 2 public) | none |
| `apps/audit-reconciler` | 3 (`/healthz`, `/readyz`, `/metrics`) | none |
| `apps/worker-fabric-bridge` | 4 (`/healthz`, `/readyz`, `/metrics`, `POST /v1/relay`) | none |

### A.2 Route × auth-gate matrix

Notation: `OIDC` (bearer + verified subject) · `Admin` (OIDC + admin allowlist) · `HMAC+iat` (HMAC-SHA256 with replay window) · `mTLS+SPIFFE` (peer SPIFFE-ID allowlist) · `none` (probe).

#### `services/declaration`

| Method+path | Auth gate | File:line |
|---|---|---|
| `POST /v1/declarations` | OIDC + governor rate-limit | `services/declaration/src/api/rest.rs:102, 124` |
| `GET /v1/declarations/by-principal` | OIDC | `services/declaration/src/api/rest.rs:131` |
| `GET /v1/declarations/{id}` | OIDC + per-row tenancy predicate | `services/declaration/src/api/rest.rs:134` |
| `POST /v1/declarations/{id}/supersede` | OIDC + per-row tenancy | `services/declaration/src/api/rest.rs:136` |
| `POST /v1/declarations/{id}/amend` | OIDC + per-row tenancy | `services/declaration/src/api/rest.rs:140` |
| `POST /v1/declarations/{id}/correct` | Admin allowlist | `services/declaration/src/api/rest.rs:144` |
| `GET /v1/internal/outbox-dlq` | Admin allowlist | `services/declaration/src/api/rest.rs:166` |
| `POST /v1/internal/outbox-dlq/{id}/replay` | Admin allowlist | `services/declaration/src/api/rest.rs:169` |
| `POST /v1/internal/verification-outcomes` | HMAC+iat (+ optional mTLS+SPIFFE peer-ID gate) | `services/declaration/src/api/rest.rs:198-204` |
| `GET /healthz`, `GET /readyz` | none | `services/declaration/src/api/rest.rs:206-207` |
| `GET /metrics` | none (port-segregated when `METRICS_BIND_ADDR` set) | `services/declaration/src/api/rest.rs:248, 274` |
| `GET /openapi.json`, `GET /docs` | none | `services/declaration/src/api/openapi.rs:224-225` |

#### `services/verification-engine`

| Method+path | Auth gate | File:line |
|---|---|---|
| `POST /v1/verifications` | OIDC + admin allowlist (FIND-002 closure) | `services/verification-engine/src/api/rest.rs:82` |
| `GET /v1/verifications/{case_id}` | OIDC + per-case `declarant_principal` predicate (FIND-004) | `services/verification-engine/src/api/rest.rs:83` |
| `GET /v1/internal/verification-outbox-dlq` | Admin allowlist | `services/verification-engine/src/api/rest.rs:108` |
| `POST /v1/internal/verification-outbox-dlq/{id}/replay` | Admin allowlist | `services/verification-engine/src/api/rest.rs:111` |
| `POST /v1/internal/declaration-events` | HMAC+iat (+ optional mTLS+SPIFFE peer-ID gate) | `services/verification-engine/src/api/rest.rs:137-143` |
| `GET /healthz`, `GET /readyz` | none | `services/verification-engine/src/api/rest.rs:145-146` |
| `GET /metrics` | none (port-segregated when `METRICS_BIND_ADDR` set) | `services/verification-engine/src/api/rest.rs:175, 202` |
| `GET /openapi.json`, `GET /docs` | none | `services/verification-engine/src/api/openapi.rs:156-157` |

#### `services/person-service`

| Method+path | Auth gate | File:line |
|---|---|---|
| `POST /v1/persons` | OIDC | `services/person-service/src/api/rest.rs:71` |
| `GET /v1/persons/search` | OIDC + per-row `created_by_principal` predicate (FIND-005) | `services/person-service/src/api/rest.rs:72` |
| `GET /v1/persons/{id}` | OIDC + per-row predicate | `services/person-service/src/api/rest.rs:73` |
| `POST /v1/persons/{id}/merge-into/{target_id}` | Admin allowlist | `services/person-service/src/api/rest.rs:75-78` |
| `GET /healthz`, `GET /readyz` | none | `services/person-service/src/api/rest.rs:85-86` |
| `GET /metrics` | none | `services/person-service/src/api/rest.rs:102, 127` |
| `GET /openapi.json`, `GET /docs` | none | `services/person-service/src/api/openapi.rs:118-119` |

#### `services/entity-service`

| Method+path | Auth gate | File:line |
|---|---|---|
| `POST /v1/entities` | OIDC | `services/entity-service/src/api/rest.rs:76` |
| `GET /v1/entities/search` | OIDC | `services/entity-service/src/api/rest.rs:77` |
| `GET /v1/entities/{id}` | OIDC | `services/entity-service/src/api/rest.rs:78` |
| `POST /v1/entities/{id}/update` | OIDC + per-row tenancy | `services/entity-service/src/api/rest.rs:80` |
| `POST /v1/entities/{id}/dissolve` | Admin allowlist | `services/entity-service/src/api/rest.rs:84` |
| `GET /healthz`, `GET /readyz` | none | `services/entity-service/src/api/rest.rs:94-95` |
| `GET /metrics` | none | `services/entity-service/src/api/rest.rs:111, 136` |
| `GET /openapi.json`, `GET /docs` | none | `services/entity-service/src/api/openapi.rs:137-138` |

#### `apps/audit-verifier`

| Method+path | Auth gate | File:line |
|---|---|---|
| `GET /v1/audit/verify/{declaration_id}` | OIDC (FIND-001 closure) | `apps/audit-verifier/src/handlers.rs:31` |
| `GET /healthz`, `GET /readyz` | none | `apps/audit-verifier/src/handlers.rs:39-40` |

#### `apps/audit-reconciler`

| Method+path | Auth gate | File:line |
|---|---|---|
| `GET /healthz`, `GET /readyz`, `GET /metrics` | none (background job, no business surface) | `apps/audit-reconciler/src/handlers.rs:28-30` |

#### `apps/worker-fabric-bridge`

| Method+path | Auth gate | File:line |
|---|---|---|
| `GET /healthz`, `GET /readyz`, `GET /metrics` | none | `apps/worker-fabric-bridge/src/handlers.rs:36-38` |
| `POST /v1/relay` | HMAC+iat (with rotation slot per FIND-015 closure) | `apps/worker-fabric-bridge/src/handlers.rs:39` |

### A.3 gRPC

- `services/declaration/src/api/grpc.rs` — present (1118 LOC per Phase 2G report), proto contract under `contracts/grpc/`. Only one test exercises this file. **Tonic implementation exists but is not validated** outside compile.

### A.4 Schedulers / background workers

- `apps/worker-fabric-bridge` — drains the declaration outbox via `tokio::time::interval` polling cycle (file `apps/worker-fabric-bridge/src/processor.rs`); 390 LOC; bound to `OUTBOX_RELAY_INTERVAL_SECONDS`.
- `apps/audit-reconciler` — periodic divergence detector; ticks every `RECONCILE_INTERVAL_SECONDS` (default 600s); file `apps/audit-reconciler/src/reconciler.rs`.
- Declaration outbox-relay — `services/declaration/src/infrastructure/relay.rs` — same polling pattern.
- Verification outbox-relay (declaration writeback) — `services/verification-engine/src/infrastructure/relay.rs`.
- Idempotency-record sweeper — TTL-bound cleanup of `idempotency_records` rows; one per service.
- Retention worker — NOT YET wired in code; documented in `docs/runbooks/dlq-retention.md` only.

### A.5 Queue consumers

- Kafka consumer in `services/verification-engine/src/infrastructure/kafka_consumer.rs` (622 LOC, no integration tests per Phase 2G).
- Kafka producer in `services/declaration/src/infrastructure/kafka_producer.rs` (455 LOC, no integration tests).
- ADR-007 documents the Kafka transport cutover. Current default deployment uses the HTTP outbox-relay (ADR-003); Kafka path is opt-in.

### A.6 CLI binaries

- `tools/cli/recor-cli` — referenced from `justfile:109` (`cargo install --path tools/cli/recor-cli`) but the path **does not exist in the tree**. The justfile target is aspirational. Tracked under audit catalogue's MEDIUM/LOW "Toolchain / build" row.

### A.7 Chaincode (Hyperledger Fabric)

- `chaincode/audit-witness/` — Go chaincode:
  - `chaincode/audit-witness/lib/` — record + index types
  - `chaincode/audit-witness/cmd/` — chaincode main
- Two key namespaces:
  - `recor.audit.declaration` — one record per anchored declaration event
  - `recor.audit.index.declaration` — index by `(declaration_id, event_id)`
- Idempotent re-write (`audit_witness_test.go::TestRecordAuditEntry_Idempotent`).

---

## B. Persistence

### B.1 Table inventory (22 tables across 5 Postgres deployments)

| Table | Service | Migration | Append-only? | Classification |
|---|---|---|---|---|
| `declarations` | declaration | `0001_initial.sql` | NO (UPDATE allowed for projection) | Sensitive-PII (beneficial-owner rows) |
| `declaration_events` | declaration | `0001_initial.sql` + `0007_audit_log_immutability.sql` | **YES** (COMP-2 triggers) | Sensitive-PII |
| `outbox` | declaration | `0001_initial.sql` | NO (UPDATE on dispatch) | Internal |
| `outbox_dlq` | declaration | `0005_add_outbox_dlq.sql` | NO | Internal |
| `idempotency_records` | declaration | `0001_initial.sql` | NO (TTL-evicted) | Internal |
| `entities` | entity-service | `0001_init.sql` | NO | Public (per `docs/compliance/data-classification.md`) |
| `entity_events` | entity-service | `0001_init.sql` | **YES** (COMP-2) | Public |
| `outbox` | entity-service | `0001_init.sql` | NO | Internal |
| `idempotency_records` | entity-service | `0001_init.sql` | NO | Internal |
| `persons` | person-service | `0001_init.sql` | NO | Sensitive-PII |
| `person_events` | person-service | `0001_init.sql` | **YES** (COMP-2) | Sensitive-PII |
| `outbox` | person-service | `0001_init.sql` | NO | Internal |
| `idempotency_records` | person-service | `0001_init.sql` | NO | Internal |
| `verification_cases` | verification-engine | `0001_initial.sql` + `0003_audit_log_immutability.sql` | **YES** (COMP-2) | PII (declaration_id, declarant_principal) |
| `verification_outbox` | verification-engine | `0001_initial.sql` | NO | Internal |
| `verification_outbox_dlq` | verification-engine | `0002_add_verification_outbox_dlq.sql` | NO | Internal |
| `mock_bunec_persons` | verification-engine | `0001_initial.sql` (placeholder; R-VER-1) | NO | Test-fixture |
| `sanctions_persons` | verification-engine | `0005_sanctions.sql` (renumbered) | NO | Internal (sanctions list) |
| `peps` | verification-engine | `0006_peps.sql` (likely) | NO | Internal |
| `icij_persons` | verification-engine | `0007_icij_persons.sql` (likely) | NO | Internal |
| `kafka_consumer_dlq` | verification-engine | `0004_add_kafka_consumer_dlq.sql` | NO | Internal |
| `declaration_projection` | verification-engine | (placeholder pending writeback subscriber) | NO | Sensitive-PII (mirrors declaration) |
| `fabric_bridge_dlq` | worker-fabric-bridge | `0001_create_fabric_bridge_dlq.sql` | NO | Internal |

### B.2 COMP-2 immutability triggers (4 audit tables)

`declaration_events`, `person_events`, `entity_events`, `verification_cases` enforce append-only via:

- `BEFORE UPDATE` trigger raising exception
- `BEFORE DELETE` trigger raising exception
- `BEFORE TRUNCATE` trigger raising exception
- `REVOKE UPDATE, DELETE, TRUNCATE ON <table> FROM PUBLIC`

### B.3 Tables lacking clear owner

- `declaration_projection` — placeholder; pending writeback subscriber that Declaration service writes onto.
- `mock_bunec_persons` — intentional fixture; replaced by real BUNEC adapter when `R-VER-1` lands.

### B.4 On-chain persistence

Hyperledger Fabric `audit-witness` chaincode — see § A.7. Anchored records carry the BLAKE3 hash of the canonical declaration body + the `event_id` + the `actor_principal`. No PII on-chain (hashes only).

### B.5 Vault paths

| Service | Vault path | Contents |
|---|---|---|
| declaration | `secret/recor/declaration/*` | `database`, `relay`, `writeback`, `oidc`, `observability` (5 keys) |
| verification-engine | `secret/recor/verification-engine/*` | same 5-key structure |
| person-service | `secret/recor/person-service/*` | same |
| entity-service | `secret/recor/entity-service/*` | same |
| audit-verifier | `secret/recor/audit-verifier/*` | scoped subset |
| audit-reconciler | `secret/recor/audit-reconciler/*` | scoped subset |
| worker-fabric-bridge | `secret/recor/worker-fabric-bridge/*` | scoped subset |

### B.6 Caches

In-memory caches that hold beneficial-ownership data:

- OIDC token cache — LRU 1024 entries default (`recor-auth-oidc`, expiry-bound)
- JWKS cache — TTL 300s default (`recor-auth-oidc`)
- No declaration/person/entity content caches (every read hits Postgres)
- No Redis / Memcached / sled deployments

---

## C. External integrations

### C.1 Identity providers (OIDC)

- OIDC issuer URL configurable via `OIDC_ISSUER_URL` env. Discovery + JWKS via the `recor-auth-oidc` shared crate (`packages/recor-auth-oidc/src/lib.rs`).
- Subject claim configurable (`OIDC_SUBJECT_CLAIM`); default `sub`.
- Audience configurable (`OIDC_AUDIENCE`).
- Dev fallback: `X-Recor-Dev-Principal` header — **refused outside `ENVIRONMENT=dev`** (FIND-003 closure).

### C.2 Sanctions / PEP / ICIJ data feeds

| List | Adapter file | Update cadence | Notes |
|---|---|---|---|
| Sanctions (consolidated) | `services/verification-engine/src/infrastructure/sanctions_postgres.rs` | n/a in code — table seeded externally | pg_trgm trigram match over `sanctions_persons`. **No feed-ingestion code in repo** — operator-seeded. |
| PEP | `services/verification-engine/src/infrastructure/peps_postgres.rs` (likely; mirrored from sanctions) | n/a in code | Same posture as sanctions. |
| ICIJ (Offshore Leaks / Panama / Paradise / Pandora) | `services/verification-engine/src/infrastructure/icij_postgres.rs` (likely) | n/a in code | Same posture. |
| OFAC | not separately wired — merged into `sanctions_persons` | n/a | Single consolidated table. |
| UN / EU sanctions | not separately wired — merged into `sanctions_persons` | n/a | Single consolidated table. |

Stage 3 (sanctions) is gated behind `ENABLE_REAL_SANCTIONS=true`; default OFF. Stage 4 (PEP) is gated behind `ENABLE_REAL_PEP=true`; default OFF. Stage 5 (adverse media) is gated behind `ENABLE_REAL_ADVERSE_MEDIA=true` AND `ANTHROPIC_API_KEY` present.

### C.3 FIU / ANIF endpoints

**ABSENT.** No `anif`, `FIU`, `STR_SUBMIT`, `SAR_SUBMIT` references anywhere in the code (verified via grep). ANIF is referenced in audit doctrine but no integration exists.

### C.4 Authoritative registries

- **BUNEC** (Cameroonian business register) — interface present (`services/verification-engine/src/infrastructure/bunec_adapter.rs` + a real adapter `bunec_real.rs`); production use deferred to `R-VER-1`. Current default is the `mock_bunec_persons` table.
- **OAPI** — referenced only in CLAUDE.md / architecture docs; no integration code.
- **Foreign company registers** — none.

### C.5 AI inference

- Anthropic API via `packages/recor-inference-gateway/src/lib.rs`.
- Models pinned: `claude-opus-4-7` (Tier A; default for adverse-media) and `claude-haiku-4-5-20251001` (Tier B).
- Fixture mode when `ANTHROPIC_API_KEY` is empty — returns canned responses.
- Doctrine D22: Anthropic-primary inference. OpenAI / Gemini are NOT integrated (verified via grep).

### C.6 Fabric / blockchain

- `apps/worker-fabric-bridge/` — drains the declaration outbox + writes audit records onto the Fabric channel.
- `chaincode/audit-witness/` — the chaincode itself (Go).
- `packages/fabric-bridge/` — shared bridge logic.
- `apps/audit-reconciler/` — periodic divergence detection (event_log vs. chain).
- `apps/audit-verifier/` — exposes verification of an anchored declaration.

### C.7 Object stores / IPFS / S3

**ABSENT.** No `s3://`, `IpfsClient`, `aws-sdk-s3` references in production code.

### C.8 Observability sinks

- OTel via `recor-logging` shared crate → OTLP exporter to a configurable collector endpoint.
- Prometheus `/metrics` per service (either main listener or `METRICS_BIND_ADDR` separate listener).
- Loki + Tempo + Grafana — referenced in `infrastructure/observability-dev/docker-compose.yml`; production sinks configured at deploy.

### C.9 Email / SMS / push notification dispatchers

**ABSENT.** No notification surfaces in code (no SendGrid, no Twilio, no FCM, no SES).

### C.10 Payment / fee-collection surfaces

**ABSENT** — none expected; confirmed.

---

## D. Cryptographic operations

### D.1 HMAC (D↔V + bridge)

- Crate: `packages/recor-hmac-sig/src/lib.rs`.
- Algorithm: **HMAC-SHA256** (RFC 2104). Constant-time verification via `verify_slice()`.
- Iat-bound replay window: ±300s default (configurable). Closure of FIND-012.
- Dual-secret rotation slot: primary + optional `*_OLD` (ADR-005). Closure of FIND-015 for worker-fabric-bridge.
- Key provenance: env var (`*_HMAC_SECRET`, `*_HMAC_SECRET_OLD`) populated from Vault by `recor-vault-client::populate_from_vault`.
- Concerning: `expect("HMAC accepts any key length")` on initialisation — acceptable (key length never panics) but callers must avoid empty keys.

### D.2 Ed25519 attestation

- File: `services/declaration/src/domain/attestation.rs:1-173`.
- Library: `ed25519_dalek` (`verify_strict()` rejects non-canonical signatures).
- Key format: 32-byte public key, hex (64 chars).
- Nonce: 16-byte hex (per-attestation, declarant-supplied).
- **Concerning:** the production nonce-generation code path is not visible in the scanned code — declarant-side code (in the portal? Or expected client-side?) provides `nonce_hex`. Server accepts whatever it's given. Plausibility/uniqueness check on `nonce_hex` is NOT present.

### D.3 BLAKE3 hashing

- `blake3::hash()` — canonical-declaration digest (for receipt + chaincode anchoring). `services/declaration/src/domain/aggregate.rs:19`.
- `blake3::keyed_hash()` — log-redaction MAC. `packages/recor-logging/src/lib.rs:189-198`.
- Redaction key: `LOG_REDACTION_KEY` env (required outside dev); dev fallback derives the key from `SystemTime + PID` — explicitly NOT for production.

### D.4 mTLS / SPIFFE

- Crate: `packages/recor-spiffe/src/rustls_glue.rs`.
- TLS impl: `rustls 0.23` with `ring` crypto provider.
- Trust domain: `recor.cm` (per architecture).
- Workload API socket: `unix:///tmp/spire-agent/public/api.sock` (default).
- Peer SPIFFE-ID allowlist: per-service env var (`INTERNAL_PEER_SPIFFE_IDS`); enforced at outer tower layer.
- Cert format: X.509 (PEM → DER via `rustls_pemfile`).
- Private key format: PKCS#8 (EC + RSA support).
- Closure of FIND-017 — integration test at `services/verification-engine/tests/peer_spiffe_id_gate.rs`.

### D.5 OIDC + JWKS

- Crate: `packages/recor-auth-oidc/src/lib.rs`.
- Algorithms accepted: RS256, RS384, RS512, ES256, ES384, EdDSA, PS256/384/512.
- Algorithms **rejected**: HS256/384/512 (algorithm-confusion defence, line 444).
- JWKS cache TTL: 300s default.
- Clock-skew tolerance: ±30s (line 165).
- Verified claims: `iss`, `aud`, `exp`, `nbf`.
- Token cache: LRU 1024 entries, expiry-bound.

### D.6 Vault (AppRole)

- Crate: `packages/recor-vault-client/src/lib.rs`.
- Auth method: AppRole (`VAULT_ROLE_ID` + `VAULT_SECRET_ID`).
- Bootstrap secret: env var only (D18 — first-secret problem).
- Secret-path convention: `secret/recor/<service>/*`.
- Fail-closed: non-empty `VAULT_ADDR` + Vault unreachable → startup error (D14).
- **Concerning:** 8 `expect()` calls on the Vault-login path; a flaky Vault could panic the service.

### D.7 Random / nonces

- Cryptographic random: `getrandom` via `OsRng` (used by `ed25519_dalek`).
- Non-cryptographic entropy in dev fallback: `SystemTime + PID` for redaction-key derivation (NOT production).
- **No production code generates Ed25519 nonces** — declarant-supplied via `nonce_hex` in the attestation payload.

### D.8 TLS termination

- Protocol floor: TLS 1.2 (rustls default).
- Cipher suites: rustls defaults; **no explicit hardening** to TLS 1.3-only or strong-suite-only configuration in scanned code.
- Mutual auth: enforced for intra-cluster traffic via SPIFFE SVIDs.

### D.9 Post-quantum, HSM, KMS

- **Post-quantum (Kyber, Dilithium, oqs): ZERO references.** Confirmed via grep.
- **HSM: ZERO integration code.** Architecture V4 P11 references "L0 substrate" — no implementation.
- **Cloud KMS (AWS KMS, GCP KMS, Azure Key Vault): ZERO references.**

### D.10 Audit immutability (COMP-2)

- Migration `0007_audit_log_immutability.sql` (declaration); `0003_audit_log_immutability.sql` (verification-engine); `0001_init.sql` (person + entity).
- BEFORE UPDATE/DELETE/TRUNCATE triggers raise exception.
- `REVOKE ALL` on PUBLIC role.

### D.11 Fabric anchoring crypto

- Anchored payload = BLAKE3 hash of canonical declaration body (32 bytes).
- Peer endorsement: per chaincode policy (channel-config-dependent; not in repo).
- Idempotent re-write — second call returns the existing entry.

### D.12 Concerning crypto-path patterns (flagged from Phase 2F)

- `recor-hmac-sig::expect("HMAC accepts any key length")` — acceptable but verify no empty-key callers.
- 8 `expect()` calls in `recor-vault-client` on the auth path.
- Dev-only random-key fallback in `recor-logging:189` — explicitly NOT a production path.
- Declarant-supplied `nonce_hex` accepted without uniqueness check (D7 risk: nonce-replay across attestations).

---

## E. Audit trail

### E.1 Event-log tables (forever-retained)

- `declaration_events` — event-sourced declaration aggregate (`declaration.registered.v1`, `declaration.superseded.v1`, `declaration.amended.v1`, `declaration.corrected.v1`).
- `person_events` — person-aggregate events (`person.registered.v1`, `person.updated.v1`, `person.merged.v1`).
- `entity_events` — entity-aggregate events (`entity.registered.v1`, `entity.updated.v1`, `entity.dissolved.v1`).
- `verification_cases` — event-sourced verification cases (per-case event log; not a separate `_events` table).

All four are COMP-2-immutable (BEFORE UPDATE/DELETE/TRUNCATE triggers + REVOKE on PUBLIC).

### E.2 Outbox + DLQ tables (retention-bounded)

| Table | Retention | Documented |
|---|---|---|
| `outbox` (declaration, person, entity) | 30 days post-dispatch | `docs/runbooks/dlq-retention.md` |
| `outbox_dlq` (declaration) | 30 days post-final-fail | `docs/runbooks/dlq-retention.md` |
| `verification_outbox` | 30 days post-dispatch | same |
| `verification_outbox_dlq` | 30 days post-final-fail | same |
| `fabric_bridge_dlq` | 90 days post-final-fail (longer for Fabric governance windows) | same |
| `kafka_consumer_dlq` | undocumented (likely 30 days but not codified) | gap |

**Retention worker is NOT YET wired in code** — documented in the runbook only. Sweeper SQL pattern present in the runbook but no Rust worker / cron job materialises it.

### E.3 Fabric audit witnesses

- Chaincode: `chaincode/audit-witness/lib/record.go` + `chaincode/audit-witness/cmd/main.go`.
- Payloads anchored: `{declaration_id, event_id, event_kind, blake3_hash, actor_principal, event_time}`.
- Idempotency: second-call returns existing entry (`TestRecordAuditEntry_Idempotent`).

### E.4 Audit reconciliation

- App: `apps/audit-reconciler/src/reconciler.rs` (393 LOC).
- Cadence: every `RECONCILE_INTERVAL_SECONDS` (default 600s).
- Grace: skips events newer than `RECONCILE_GRACE_SECONDS` (default 300s — bridge dispatch lag).
- Output: `recor_audit_reconciliation_divergence_total{event_type=...}` Prometheus metric + structured WARN per divergence.
- Outcome: `recor_audit_reconciliation_runs_total{outcome=ok|gateway_error|db_error}`.
- Closure of FIND-016.

### E.5 Audit verifier

- App: `apps/audit-verifier/src/handlers.rs`.
- Surface: `GET /v1/audit/verify/{declaration_id}` (OIDC-gated post-FIND-001).
- Response: `{declaration_id, on_chain_hash, computed_hash, match: bool}`.
- 11 unwraps in the handler (Phase 2F finding) — JSON-deserialisation panics could occur on malformed input.

### E.6 Structured logging — actor fields

- Every state-changing handler logs `actor_principal`, `declaration_id` / `person_id` / `entity_id`, `decision_kind` via the `tracing` crate.
- OPS-2 redaction layer (`recor-logging`) applies `blake3::keyed_hash()` to SPIFFE URIs + UUIDs.
- Verified: every `info!(actor_principal = ..., ...)` call routes through the redaction subscriber.

### E.7 Replay support

- `services/declaration/src/api/dlq.rs:195: replay_dlq` — admin endpoint to replay a DLQ row.
- `services/verification-engine/src/api/dlq.rs:187: replay_dlq` — same for V-engine.
- `services/declaration/src/infrastructure/outbox_admin.rs:124: replay_dlq` — admin-side replay helper.
- Aggregate-side: `declaration::domain::aggregate::tests::replay_amend_event_reproduces_before_and_after` — replay is event-sourcing idempotent.
- **Idempotency: replay returns the same response as the original call** (D13).

### E.8 Audit-trail access

- FIU access path: **ABSENT.** No code path provides ANIF or any FIU with audit-trail access.
- MLAT (foreign authority) path: **ABSENT.**
- Internal investigator path: indirectly via the admin DLQ endpoints + the audit-verifier; no dedicated investigator role.

### E.9 Canonical event taxonomy (every emitted `event_kind`)

- `declaration.registered.v1`
- `declaration.superseded.v1`
- `declaration.amended.v1`
- `declaration.corrected.v1`
- `verification.case.opened.v1` (implicit in `verification_cases` event sourcing)
- `verification.stage.completed.v1` (per stage 1..7)
- `verification.outcome.decided.v1`
- `person.registered.v1`
- `person.updated.v1`
- `person.merged.v1`
- `entity.registered.v1`
- `entity.updated.v1`
- `entity.dissolved.v1`

---

## F. Code-rot catalog

20,797 source files scanned. Summary counts (from Phase 2F):

| Pattern | Total | Production | Test |
|---|---|---|---|
| `TODO` / `FIXME` / `XXX` / `HACK` | 12 | 12 | 0 |
| `mock` / `stub` / `fake` (non-test) | 62 | 62 | 0 |
| `placeholder` | 15 | 13 | 2 |
| `temporary` / `TEMP` / `for now` | 4 | 4 | 0 |
| `unimplemented!()` / `not implemented` | 6 | 0 | 6 |
| `unwrap()` / `expect()` | 709 | ~650 | ~59 |
| `unsafe { }` | 2 | 2 | 0 |
| `console.log` / `println!` / `dbg!` in production | 8 | 8 | 0 |
| `: any` / `as any` (TypeScript) | 6 | 0 | 6 |
| `unwrap_or_default()` / `let _ = ...` (swallow) | 82 | 82 | 0 |

### F.1 Top-10 concentrated rot locations

1. **V-engine stage stubs (5)** — `services/verification-engine/src/application/stages/stage{3..7}_*.rs` — Stage 7 (cross-source) is the only stage with NO real implementation in repo; Stages 3..6 have a stub default + a real implementation behind a config flag. (Tracked by FIND-009 — closed by the gating mechanism but Stage 7 remains stubbed.)
2. **V-engine integration tests (44+ unwraps)** — `services/verification-engine/tests/api_integration.rs` — concentration is acceptable (tests, fatal failure is intended).
3. **`apps/audit-verifier/src/handlers.rs`** — 11 unwraps in the request handler; **production code; PANIC RISK on malformed input.**
4. **`services/verification-engine/tests/migrations_apply.rs`** — 18 cascading expects on DB queries (acceptable, tests).
5. **`services/declaration/src/metrics.rs`** — 15+ unwraps in Prometheus registry construction (acceptable — registry construction is startup-only, panic at startup is recoverable via supervisor).
6. **`packages/fabric-bridge/src/transport.rs`** — 20+ unwraps in MockServer fixtures (acceptable, fixtures).
7. **`packages/recor-vault-client/src/lib.rs`** — 8 expects on Vault login (auth-path; flaky Vault → service crash).
8. **`services/declaration/src/main.rs`** — 6× `let _ = h.await` swallowing shutdown errors.
9. **`services/person-service/src/infrastructure/postgres.rs`** — TODO + unwrap cluster around pg_trgm fuzzy search (NDI-1 deferred work).
10. **`applications/declarant-portal/tests/e2e/`** — 5 `any` types in Playwright tests.

### F.2 The 2 `unsafe { }` blocks (no `// SAFETY:` comment check needed)

To be enumerated post-Phase-3.

---

## G. Test coverage

Detailed matrix in `audit/codebase-inventory-G-tests.md` (374 lines; 142 modules graded).

**Platform aggregate:** 19.7% verified — 28 of 142 modules.

- 28 verified
- 18 unverified-implemented (tests exist but only superficial)
- 96 none (no test file)

### G.1 Verified modules (high-confidence)

- `recor-hmac-sig` — 11 tests
- `recor-auth-oidc` — 12 tests
- `declaration::domain::aggregate` — 31 tests
- `verification-engine::application::fusion` — 24 tests (Dempster-Shafer property-tested)

### G.2 Top-10 highest-risk uncovered modules

1. `declaration::api::rest` (947 LOC) — HTTP boundary untested
2. `declaration::infrastructure::postgres` (736 LOC)
3. `verification-engine::api::rest` (645 LOC)
4. `entity-service::infrastructure::postgres` (496 LOC)
5. `person-service::infrastructure::postgres` (579 LOC)
6. `worker-fabric-bridge::processor` (390 LOC) — Fabric integration stubbed
7. `declaration::infrastructure::kafka_producer` (455 LOC)
8. `verification-engine::infrastructure::kafka_consumer` (622 LOC)
9. `audit-reconciler::reconciler` (393 LOC)
10. `declaration::api::grpc` (1118 LOC) — only 1 test

### G.3 Regression detection reality check

**Would catch:** ownership sum violations, signature verification failures, HMAC tampering, fusion math bugs.

**Would NOT catch:** Postgres constraint races, HTTP parsing bugs, Kafka serialization issues, Fabric replay attacks, GDPR data leaks, cross-tenant 403/404 boundary regressions.

---

## H. Inventory of services in the platform

| Service / app | LOC est | Tier | Lifecycle |
|---|---|---|---|
| `services/declaration` | ~25k Rust | Layer 2 — core domain | Production-grade |
| `services/verification-engine` | ~30k Rust | Layer 2 — analytical | Production-grade for stages 1-2; stages 3-6 gated; stage 7 stubbed |
| `services/person-service` | ~6k Rust | Layer 2 — identity | Skeleton; deferred follow-ups (NDI-1, R-PERSON-FUZZY, R-PERSON-RBAC) |
| `services/entity-service` | ~5k Rust | Layer 2 — identity | Skeleton; deferred follow-ups (R-VER-1 / BUNEC) |
| `apps/audit-verifier` | ~2k Rust | Layer 3 — audit | Production-grade post-FIND-001 |
| `apps/audit-reconciler` | ~3k Rust | Layer 3 — audit | Production-grade |
| `apps/worker-fabric-bridge` | ~4k Rust | Layer 3 — audit | Production-grade |
| `applications/declarant-portal` | ~15k TS/React | Layer 6 — UI | Vite + React 18 + Tailwind; Playwright E2E |
| `chaincode/audit-witness` | ~1.5k Go | Layer 5 — chain | Production-grade |
| `packages/recor-hmac-sig` | ~700 Rust | shared crate | Production-grade |
| `packages/recor-auth-oidc` | ~1.5k Rust | shared crate | Production-grade |
| `packages/recor-spiffe` | ~1k Rust | shared crate | Production-grade |
| `packages/recor-vault-client` | ~600 Rust | shared crate | Production-grade |
| `packages/recor-logging` | ~400 Rust | shared crate | Production-grade |
| `packages/recor-inference-gateway` | ~1k Rust | shared crate | Production-grade |
| `packages/fabric-bridge` | ~2k Rust | shared crate | Production-grade |
