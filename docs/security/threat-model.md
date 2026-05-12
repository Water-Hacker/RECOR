# RÉCOR threat model (STRIDE)

This document is the per-component STRIDE catalogue for the RÉCOR
platform as it exists in `main` at the time of writing. It is
maintained alongside the code: any PR that changes a trust boundary,
introduces a new component, or weakens an existing mitigation must
also update this document.

The audience is a security reviewer who has read the architecture
documents but has not built the system. STRIDE rows that read as
"obvious" still belong here — the value of the document is that the
*absence* of a row is an explicit gap, not an oversight.

## Scope

In scope:

1. **Declarant portal** (`applications/declarant-portal/`) — browser
   SPA that generates an Ed25519 keypair, signs a canonical
   declaration, posts to the declaration service, displays the
   receipt.
2. **Declaration service** (`services/declaration/`) — accepts signed
   declarations, persists event-sourced, emits to outbox, returns
   receipts; serves the verification-status read API.
3. **Verification engine** (`services/verification-engine/`) — runs
   the pipeline (today: mock BUNEC + Dempster-Shafer fusion); writes
   verification outcomes back to the declaration service.
4. **D↔V loop** — HMAC-SHA256-signed HTTP today; outbox-relay +
   webhook + DLQ topology. Kafka migration is R-LOOP-2.
5. **Auth** — OIDC discovery + JWKS verification on production, dev
   `X-Recor-Dev-Principal` header in dev. JWT verification refuses
   HMAC algorithms outright.
6. **Database** — Postgres 17, one per service, owned by service
   role; sqlx runtime-checked queries; per-row signatures on the
   `declaration_events` append-only log.
7. **Operator surface** — DLQ admin endpoints under
   `/v1/internal/outbox-dlq` (declaration) and the mirror on
   verification-engine; admin allowlist via env.

Explicitly out of scope (covered by separate documents or future
tickets):

- Identity provider compromise (OIDC issuer). Mitigation is operator
  procedure + JWKS rotation; full IdP threat model is the IdP
  vendor's responsibility.
- Kubernetes / Helm / ArgoCD layer. Will land with OBS-2 / OPS-4.
- The Fabric audit witness chain (R-DECL-9, deferred).
- Physical / human-operator threat model.

## Trust boundaries

| # | Boundary | Enforces |
|---|----------|----------|
| TB1 | Browser ↔ portal nginx | TLS (operator-terminated); CSP per-response (`applications/declarant-portal/security-headers.conf.template`) |
| TB2 | Portal SPA ↔ declaration API | OIDC bearer (prod) or dev header (dev); HMAC body verification at `/v1/internal/*` is server-to-server only |
| TB3 | Declaration service ↔ verification engine | Per-channel HMAC-SHA256 with dual-secret rotation (`services/declaration/src/api/internal.rs:56-72` for the rotation contract) |
| TB4 | Service ↔ Postgres | Service-role-only credential; `DATABASE_URL` is a `SecretString`; no shared role with operator humans |
| TB5 | Operator ↔ DLQ admin | OIDC sub (prod) or dev header (dev) checked against `admin_principals` allowlist (`services/declaration/src/api/dlq.rs::enforce_admin`) |

## Adversary catalogue

- **External attacker (network)** — can speak to any public endpoint
  the portal or service exposes. Cannot read database. Cannot bypass
  TLS (terminator-enforced, out of scope).
- **Malicious declarant** — has valid OIDC credentials, submits
  declarations. Cannot sign on behalf of a different principal.
- **Compromised operator workstation** — has a valid admin OIDC
  identity AND can edit code. Mitigations are detective (audit log)
  not preventive — see DOC-3 incident-response-template.
- **Malicious or compromised dependency** — supply chain. Mitigations
  are SLSA L4 target via cosign (CI-1, shipped) + CI-2 SBOM + Trivy.
- **Compromised verification engine host** — can forge HMAC-signed
  writeback envelopes for any case ID it sees. Mitigation: HMAC
  secret rotation cadence + per-row Ed25519 signature retained on
  every event in the declaration audit log.
- **Nation-state (cryptanalytic)** — capability-class adversary that
  may eventually break Ed25519 / BLAKE3. Mitigation: D21 (post-quantum
  agility) is an explicit doctrinal commitment; today's algorithms
  are classical and the migration plan is deferred to a future ADR.

---

## Per-component STRIDE

### 1. Declarant portal

| STRIDE | Threat | Current mitigation | Code / accepted-risk |
|---|---|---|---|
| S | Attacker impersonates declarant via stolen session | OIDC + short-lived tokens; portal never persists the private key | `applications/declarant-portal/CLAUDE.md` § D18 |
| T | Tampered canonical payload before signature | Browser builds canonical bytes from typed Zod-validated state, never from user-controlled string | `applications/declarant-portal/src/lib/crypto.ts:canonicalPayloadBytes` |
| T | Malicious script injected via XSS replaces signing key | CSP `script-src 'self'` (no `unsafe-inline`, no `unsafe-eval`); `frame-ancestors 'none'` | `applications/declarant-portal/CLAUDE.md` § CSP |
| R | Declarant denies submitting a declaration | Receipt contains Ed25519 signature over canonical bytes; signature retained on `declaration_events` (server-side); declarant printed receipt is reproducible | D15 |
| I | Receipt URL leaked in Referer header to off-site link | `Referrer-Policy: strict-origin-when-cross-origin` | security-headers.conf.template |
| I | Private key persisted to localStorage / IndexedDB by future feature | Memory-only today, enforced by code review per D18 | **Gap G5** — no programmatic enforcement; R-PORT-2 (offline drafts) carries this constraint |
| D | Hostile script consumes CPU via crypto-mining | CSP closes the inline-script path; `script-src 'self'` only allows bundled JS | CSP |
| D | Submit endpoint flooded from a single principal | OPS-1 (shipped) — 60 rpm/principal, 10 burst, on POST routes only | `services/declaration/src/api/rate_limit.rs` |
| E | Browser feature (camera/USB/payment) abused by compromised dependency | `Permissions-Policy` disables every browser feature the portal does not use | security-headers.conf.template |

### 2. Declaration service

| STRIDE | Threat | Current mitigation | Code / accepted-risk |
|---|---|---|---|
| S | Forged declarant identity in request body | Principal comes from `auth_middleware` (OIDC sub or dev header), never from request body — D17 | `services/declaration/src/api/auth.rs:58` |
| S | JWT alg-confusion (sign with HMAC, claim RS256) | HMAC algs (HS256/384/512) refused outright before signature check | `services/declaration/src/api/oidc.rs`, R-DECL-1 closed |
| T | Tampered event log after write | Event log is append-only at the SQL level: BEFORE UPDATE/DELETE/TRUNCATE triggers RAISE EXCEPTION on every mutation attempt regardless of invoking role (COMP-2, migration `services/declaration/migrations/0007_audit_log_immutability.sql`); UPDATE/DELETE/TRUNCATE also REVOKEd from PUBLIC. | migrations + integration test `services/declaration/tests/audit_immutability.rs`; partial gap: no in-DB checksum chain (deferred to R-DECL-9 Fabric anchoring) |
| T | Outbox row mutated between write and relay | Outbox + event + projection in single Postgres transaction (D13); relay is idempotent on `event_id` | `services/declaration/src/infrastructure/outbox.rs` |
| R | Declarant later disputes that they signed | Ed25519 attestation persisted alongside the event; BLAKE3 receipt deterministic from canonical bytes | D15 |
| I | PII leaked via tracing logs | OPS-2 (shipped): `recor-logging::RedactingLayer` masks SPIFFE paths, UUID PII fields, partial receipt hashes | `packages/recor-logging/src/lib.rs` |
| I | Postgres backup theft exposes PII | Filesystem encryption + restricted backup access on the host | **Gap G3** — declaration body PII unencrypted at rest; accepted-risk for v1, tracked in `docs/PRODUCTION-TODO.md` (encryption-at-rest follow-up) |
| D | Submit flood (rate-limited above) | OPS-1 — see portal § D |  |
| D | Slow-loris on `/healthz` blocks the readiness path | Per-route timeout via `TimeoutLayer` | `services/declaration/src/api/rest.rs` |
| E | Idempotency-Key replay grants a fresh receipt | Idempotency record TTL + replay returns the exact previous response, not a new write | D13; `services/declaration/src/application/submit_declaration.rs` |

### 3. Verification engine

| STRIDE | Threat | Current mitigation | Code / accepted-risk |
|---|---|---|---|
| S | Forged "verification outcome" event injected via D→V channel | HMAC-SHA256 body verification at the V-engine inbound endpoint; HMAC key per direction (D→V is distinct from V→D) | `services/verification-engine/src/api/internal.rs` |
| T | Tampered BPA produced by a misbehaving stage | Stages are deterministic and reproducible from input; fusion math is auditable (ADR-002 Dempster-Shafer); event log preserves both inputs and BPAs | ADR-002 |
| R | Disputed verification result | Every result event in V-engine includes the inputs that produced it; deterministic replay re-derives the same fusion outcome | ADR-002 |
| I | Mock BUNEC fixture leaks production PII | Mock data is synthetic only; R-VER-1 wires the real BUNEC API, which terminates trust on BUNEC's side | `services/verification-engine/src/infrastructure/bunec_mock.rs` |
| D | DLQ inundation from a stuck stage | R-LOOP-DLQ-3 (shipped): admin endpoints list/replay; runbook in DOC-3 dlq-inundation | `services/verification-engine/src/api/dlq.rs` |
| E | A stage requests admin-level secret material | Stages execute under the service role with no direct secret access; secrets flow only through `Config::from_env` boundary | D18 |

### 4. D↔V loop

| STRIDE | Threat | Current mitigation | Code / accepted-risk |
|---|---|---|---|
| S | Either side forges an envelope without the shared HMAC | Per-channel HMAC-SHA256; constant-time compare; algorithm not negotiable (no JWT-style alg field on the wire) | `services/declaration/src/api/internal.rs` |
| S | Rotation race: old secret accepted forever | Dual-secret rotation window: operator clears the old slot after migration completes; runbook in `docs/runbooks/hmac-secret-rotation.md` enforces the close-out step | ADR-005 |
| T | Replay of a captured envelope | Receiver idempotency by `event_id` produces no observable effect on replay | **Gap G2** — not bound to envelope timestamp; R-LOOP-2 (Kafka migration) carries the `iat` enforcement |
| R | Either side denies sending | HMAC signatures + persisted outbox + DLQ retain the original envelope bytes | D15 |
| I | HMAC secret leaked in tracing | OPS-2 redacting layer + secrets wrapped in `SecretString`; no `expose_secret()` in any log site | `packages/recor-logging/src/lib.rs` |
| D | DLQ floods consume disk | DLQ admin endpoints (R-LOOP-DLQ-2/3, shipped) let operator drain; alert wiring is OBS-1 (Phase 2) | `services/declaration/src/api/dlq.rs` |
| E | Cross-channel misuse: D→V secret accepted on V→D path | Secrets are separately-named env vars; verifier on each side only reads its own slot | `services/declaration/src/config.rs` |

### 5. Auth (OIDC + dev header)

| STRIDE | Threat | Current mitigation | Code / accepted-risk |
|---|---|---|---|
| S | Token signed with HMAC, asserts RS256 | HMAC algs refused outright before signature check (R-DECL-1) | `services/declaration/src/api/oidc.rs` |
| S | JWKS endpoint MITM | JWKS fetch is HTTPS-only and cached with TTL; HTTP scheme refused at startup | `services/declaration/src/api/oidc.rs` |
| T | JWT claim tampering | Standard JWS verification on every request; tokens with bad signature are 401 before any handler runs | `services/declaration/src/api/auth.rs` |
| R | Token issuer denies issuing a token | OIDC issuer keeps its own audit log; out of scope for this document |  |
| I | Token claim leaked in logs | OPS-2 redaction masks the `sub` claim before logging (UUIDs in `subject` field get keyed-MAC) | `packages/recor-logging/src/lib.rs` |
| D | OIDC issuer down → service refuses all auth | Documented in DOC-3 `oidc-issuer-outage.md` with explicit fail-closed decision tree | runbook |
| E | Dev header used in production | Production refuses to start when `ENVIRONMENT != dev` and `OIDC_ISSUER_URL` is empty; dev header path is conditional on `cfg.is_dev()` | `services/declaration/src/config.rs::Config::from_env` |

### 6. Database

| STRIDE | Threat | Current mitigation | Code / accepted-risk |
|---|---|---|---|
| S | Service-role credential used by an unauthorised client | DB credential is a `SecretString` in env; no shared role; refuses to start without `DATABASE_URL` | D18 |
| T | Direct row mutation on event log bypasses domain | UPDATE/DELETE/TRUNCATE refused by BEFORE trigger that fires regardless of invoking role (COMP-2); REVOKE strips PUBLIC; tested by `services/declaration/tests/audit_immutability.rs`. Same mirror on `verification_cases`. | migrations 0007 (declaration) + 0003 (verification-engine) |
| T | sqlx query injection | sqlx runtime-checked queries with parameterised binds; no string-built SQL | code-review-enforced |
| R | DBA later denies running a destructive statement | Production DBA access is procedural (DOC-3 incident-response-template) | **Gap G4** — no in-database audit of DBA-role statements; OBS-1 (Phase 2) ships programmatic audit |
| I | Backup theft (see Declaration § I) | Filesystem encryption + access restrictions on backup hosts | accepted-risk for v1 |
| D | Connection pool exhaustion under load | `db_pool_max_connections` (configurable); per-request timeout in axum | `services/declaration/src/config.rs` |
| E | Privilege escalation via Postgres extension | No extensions installed beyond `pgcrypto` for `gen_random_uuid()`; testcontainers pinned to `postgres:17-alpine` matching production | migrations |

### 7. Operator surface (DLQ admin)

| STRIDE | Threat | Current mitigation | Code / accepted-risk |
|---|---|---|---|
| S | Unauthorised principal calls admin endpoint | `enforce_admin` checks principal against `admin_principals` allowlist; empty allowlist → 503 (fail-closed) | `services/declaration/src/api/dlq.rs::enforce_admin` |
| T | Replay a DLQ row maliciously | Replay is idempotent (same event_id); writes nothing new | `services/declaration/src/api/dlq.rs` |
| R | Operator denies running a replay | `tracing` span records principal + DLQ row id; redaction allows operator identity to be recovered from the keyed MAC via the operations team | OPS-2 |
| I | DLQ row content (declaration body) leaks to operator | Operator already has admin role; treated as authorised-disclosure. Detection rather than prevention | accepted-risk |
| D | Operator floods the replay endpoint | OPS-1 rate limit does NOT cover internal endpoints by design; admin allowlist is the gate; operator-flood treated as compromised-operator scenario | accepted-risk |
| E | DLQ admin grants escalation to other endpoints | Admin allowlist is per-endpoint-pair only; no shared privilege escalation path | `enforce_admin` |

---

## Gaps blocking production

| # | Gap | Closing ticket |
|---|---|---|
| G1 | No in-DB audit chain on the event log (today: append-only via triggers + grants — COMP-2 shipped — but no cryptographic chaining between rows) | R-DECL-9 (Fabric anchoring, Phase 2) |
| G2 | D↔V replay window not bound to envelope timestamp | TBD — Phase 2 follow-up; tracked in `docs/PRODUCTION-TODO.md` R-LOOP-2 (Kafka migration carries the iat enforcement) |
| G3 | Declaration body PII unencrypted at rest in the projection table | TBD — encryption-at-rest ticket to file; not in Phase 0 |
| G4 | DBA-role statement audit | OBS-1 (Phase 2 — production observability) |
| G5 | Portal Ed25519 key isn't programmatically restricted from persistence (memory-only by code-review) | R-PORT-2 (offline drafts; carries this constraint) |
| G6 | Post-quantum agility plan not yet drafted | TBD — D21 ADR in Phase 2 |
| G7 | Threat model independence (this doc is self-authored; no external security review) | PEN-1 (Phase 5 pre-launch penetration test + threat-model peer review) |

Closing G1, G3, and G4 is a precondition for launch. G2, G5, G6, G7 are
acknowledged accepted-risks for the v1 launch envelope and have
named tickets owning the resolution path.

---

## Maintenance

This document is reviewed:

- On every PR that touches a file referenced in a STRIDE row (CI does
  not yet enforce this — manual reviewer check today).
- On every PR that introduces a new trust boundary (architect-reviewer
  agent should flag).
- Quarterly by the security-reviewer agent during the DOC-4 refresh
  cycle.

Updates to the doctrines (V1 P2) or to a component CLAUDE.md may
invalidate STRIDE rows; the maintainer of the changed file is
responsible for updating this document in the same PR.

## Related documents

- `docs/security/README.md` — index of security documentation
- `docs/security/branch-protection.md` — main-branch enforcement
- `docs/adr/0001-event-sourcing-declaration-aggregate.md`
- `docs/adr/0003-http-outbox-relay-d-v.md`
- `docs/adr/0004-oidc-jwks-principal-authentication.md`
- `docs/adr/0005-hmac-channel-rotation.md`
- `docs/runbooks/hmac-secret-rotation.md`
- `docs/runbooks/oidc-issuer-outage.md` (DOC-3)
- `docs/runbooks/incident-response-template.md` (DOC-3)
- `docs/PRODUCTION-TODO.md` — open tickets including the gaps above
