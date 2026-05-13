# RÉCOR Production-Readiness Audit — Section 02: External Surfaces

**Audit Pass:** A
**Snapshot:** `main` @ `8f0d3ee`
**Companion:** `00-orientation.md`, `01-system-map.md`

For every external interface, this section records:
- Identifier (path / signature / topic)
- Source file:line for the route definition
- Full handler chain (router → middleware → handler → use case → repository)
- Authentication state required
- Exact authentication check (file:line)
- Exact authorization check (file:line)
- Data sources read / write operations
- Expected caller populations
- Sensitivity / classification / feature flag
- A **forbidden-access trace** ("if a caller without authority reaches this, what happens?")

Findings are inline `[FINDING:severity]`.

---

## A. Declaration service — REST surface (`services/declaration/src/api/rest.rs`)

### Common pieces

- **Router builder:** `pub fn router(state: AppState, cfg: &Config) -> Router` at `services/declaration/src/api/rest.rs:73`. Four merged sub-routers (`protected`, `admin`, `internal`, `public`) plus the OpenAPI mount and a sibling `metrics_router`.
- **Auth middleware:** `services/declaration/src/api/auth.rs:63-71` (`auth_middleware`). Resolves `Principal` from one of:
  - dev path: `X-Recor-Dev-Principal` header (only when `state.is_dev == true`) — `auth.rs:77-95`.
  - bearer path: `Authorization: Bearer <jwt>` → `OidcVerifier::verify` (`auth.rs:97-145`). RS256/ES256/EdDSA only; HS* refused by `recor-auth-oidc` at config time.
  - Failure → `ServiceError::AuthenticationRequired` (401). `oidc = None` with a non-empty bearer always 401s (`auth.rs:107-113`).
- **Rate limiting (OPS-1):** `tower_governor` keyed by `Principal::subject`; applied per-route to `submit`, `supersede`, `amend`, `correct` (`rest.rs:88-114`).
- **Idempotency:** `IdempotencyStore` (Postgres). `Idempotency-Key` header — first POST stores, replay returns the stored 201 (`rest.rs:402-432`).
- **CORS:** allowlist from `CORS_ALLOWED_ORIGINS` (CSV); empty disables CORS entirely (`rest.rs:260-285`).
- **TraceLayer + request-id + timeout:** wrapped around the merged router (`rest.rs:226-238`).

---

### A.1 `POST /v1/declarations`  — submit a declaration

| Field                          | Value                                                                                                                                                    |
|--------------------------------|----------------------------------------------------------------------------------------------------------------------------------------------------------|
| Route definition               | `rest.rs:117` (`.route("/v1/declarations", submit_route)`)                                                                                                |
| Handler                        | `submit_declaration` — `rest.rs:375-470`                                                                                                                  |
| Auth state                     | OIDC bearer (`Authorization: Bearer …`) **or** dev header (only when `is_dev`)                                                                            |
| Auth check                     | `auth.rs:63-71` (middleware). Principal resolution at `auth.rs:73-164`.                                                                                  |
| Authorization check            | None at handler — any authenticated principal can submit. The submitted declaration is bound to `principal.subject` (D17 — declarant cannot be spoofed). |
| Rate limit                     | `RATE_LIMIT_PER_MIN` (default 60) + `RATE_LIMIT_BURST` (default 10), keyed by subject. `rate_limit.rs`.                                                  |
| Use case                       | `SubmitDeclarationUseCase::execute` (`services/declaration/src/application/submit_declaration.rs`)                                                       |
| Reads from                     | `idempotency_records` (lookup).                                                                                                                          |
| Writes to                      | `declarations`, `declaration_events`, `outbox`, `idempotency_records`.                                                                                   |
| User population                | Declarants from the portal (D17 — declarant principal sourced from token sub).                                                                            |
| i18n                           | n/a (API surface; the portal handles user-facing i18n).                                                                                                  |
| Classification                 | Request body carries PII (declarant_principal, person_id, etc.). Sensitive-PII bound to person rows.                                                     |
| Feature flag                   | None.                                                                                                                                                     |

**Crypto checks performed by the handler:**
- `canonical_payload_bytes(&req, &principal.subject)` (`rest.rs:381`, defined at `rest.rs:530-561`) — builds the deterministic byte form **using the authenticated principal as the declarant**, not the body field. This is the load-bearing D17 enforcement: even if the request body carries a `declarant_principal` field, the canonical bytes used for attestation verification are computed from the authenticated subject. A spoofed body therefore fails attestation verification.
- `req.attestation.verify_against(&canonical_bytes)` (`rest.rs:382-385`) — Ed25519 signature verification. Failure → `ServiceError::AttestationVerificationFailed` → 401.

**Forbidden-access trace:**
- *No bearer + no dev header* → middleware returns 401 (`auth.rs:104`). Handler never invoked.
- *Bearer with wrong JWKS signature* → `OidcVerifier::verify` returns `TokenInvalid` → 401 (`auth.rs:131-140`).
- *Bearer with HS256 (alg substitution)* → `recor-auth-oidc` config refuses HS* algorithms; verification fails with `UnsupportedAlgorithm` → 401.
- *Valid bearer + spoofed `declarant_principal` body field* → canonicalisation uses `principal.subject`, so the attested bytes won't match the declarant's signature → 401 `AttestationVerificationFailed`.
- *Valid bearer + valid attestation + tampered request body* → attestation verifies against canonical bytes, so any tampered byte breaks the signature → 401.
- *Rate-limit exhausted* → 429 with `Retry-After` (governor middleware).
- *Idempotency replay* → returns the previously stored response (201) without re-executing.
- *Idempotency conflict (same key, different body)* → 409 `IdempotencyConflict`.

`[FINDING:none]` Handler is correctly fail-closed: D14 + D17 + D15 enforced.

---

### A.2 `GET /v1/declarations/by-principal`

| Field                          | Value                                                                                                                                                   |
|--------------------------------|---------------------------------------------------------------------------------------------------------------------------------------------------------|
| Route definition               | `rest.rs:123-126`. Static-path matching ahead of `/{declaration_id}`.                                                                                    |
| Handler                        | `list_declarations_by_principal` — `rest.rs:884-930`.                                                                                                    |
| Auth state                     | Authenticated principal (bearer or dev header).                                                                                                          |
| Auth check                     | `auth.rs:63-71`.                                                                                                                                         |
| Authorization                  | Implicit: query filter is `WHERE declarant_principal = $principal.subject` — caller can only read their own (D17 + COMP-1 data-subject access).         |
| Use case                       | `ListByPrincipalUseCase::execute`.                                                                                                                       |
| Reads from                     | `declarations` table (projection) filtered by declarant_principal.                                                                                       |
| Writes                         | None.                                                                                                                                                    |
| Audit                          | `info!(event_kind = "data_subject_access", result_count, ...)` (`rest.rs:923-928`). The PII-redaction layer redacts `principal`; the event stays.        |

**Forbidden-access trace:** caller A cannot supply a query parameter that selects caller B's rows — there's no parameter to supply. The only datum bound into the SQL filter is the verified principal. A caller with no token gets 401.

---

### A.3 `GET /v1/declarations/{declaration_id}`

| Field                          | Value                                                                                                                                                              |
|--------------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| Route definition               | `rest.rs:127`.                                                                                                                                                     |
| Handler                        | `get_declaration` — `rest.rs:501-521`.                                                                                                                             |
| Auth state                     | Authenticated principal.                                                                                                                                            |
| Auth check                     | `auth.rs:63-71`.                                                                                                                                                   |
| Authorization                  | Owner-only — `if projection.declarant_principal != principal.subject { return Err(AuthorizationDenied …) }` (`rest.rs:513-517`).                                  |
| Reads                          | `declarations` projection by `declaration_id`.                                                                                                                     |
| Writes                         | None.                                                                                                                                                              |

**Forbidden-access trace:** caller A polls for caller B's declaration_id. The use case fetches the projection unconditionally; the handler then compares `projection.declarant_principal` to the authenticated subject. **The projection is read from the DB before the ownership check, which means caller A can detect *existence* by timing differences (cache hit vs. miss).** `[FINDING:low]` Side-channel-existence leak — not catastrophic but should be flagged. Future hardening: collapse 403 and 404 into a single 404 to avoid existence-disclosure. Doctrine D14 + D17 are honoured; D9 ("holy shit, that's done") would ask for the timing-tightening too.

---

### A.4 `POST /v1/declarations/{declaration_id}/supersede`

| Field                          | Value                                                                                                                                                              |
|--------------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| Route definition               | `rest.rs:128-131` (with rate-limit wrap).                                                                                                                          |
| Handler                        | `supersede_declaration` — `rest.rs:603-631`.                                                                                                                       |
| Auth state                     | Authenticated principal.                                                                                                                                            |
| Auth check                     | `auth.rs:63-71`.                                                                                                                                                   |
| Authorization                  | Enforced **inside the aggregate** (`Declaration::supersede`) — owner check at the domain layer. API layer additionally re-canonicalises with the authenticated principal so a spoofed declarant_principal field cannot succeed. |
| Crypto                         | Re-attestation: a *new* declaration is signed and verified (same path as submit).                                                                                  |
| Reads                          | Previous declaration's projection (to validate ownership + state-machine).                                                                                          |
| Writes                         | New `declarations` row + `declaration_events` + `outbox`. Old declaration is marked superseded (chain link).                                                       |

**Forbidden-access trace:** non-owner attempt to supersede:
1. Authentication: token verified (`auth.rs`).
2. Canonical bytes computed with the **caller's** principal.
3. Attestation verifies against those bytes (signature is from caller — caller has their own keypair).
4. `SupersedeDeclarationUseCase` loads the existing declaration, checks `declarant_principal == caller`. Domain returns `SupersedeNotOwner` → translated to 403 by `ServiceError::Domain::from`.

D17 fully held.

---

### A.5 `POST /v1/declarations/{declaration_id}/amend`

| Field                          | Value                                                                                                                                                              |
|--------------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| Route definition               | `rest.rs:132-135`.                                                                                                                                                 |
| Handler                        | `amend_declaration` — `rest.rs:730-783`.                                                                                                                           |
| Auth state                     | Authenticated principal.                                                                                                                                            |
| Auth check                     | `auth.rs:63-71`.                                                                                                                                                   |
| Authorization                  | Defense in depth — handler does an early 403 check at `rest.rs:751-756` (`projection.declarant_principal != principal.subject`). Aggregate `Declaration::handle_amend` re-checks.                                |
| Crypto                         | Re-attestation over the amendment-canonical bytes (`canonical_amend_bytes`, `rest.rs:638-665`). Key fact: canonical bytes embed `entity_id` resolved from the projection — declarant **cannot rebind to a different entity** via an amendment. |
| Writes                         | `declarations` re-projection + `declaration_events` (`DeclarationAmended` event) + `outbox`.                                                                       |

**Forbidden-access trace:**
- Non-owner amending: handler returns 403 before any state change.
- Amendment with a `kind` other than `amendment`: hard-coded `kind: "amendment"` in canonical bytes (`rest.rs:660`). Caller's signature must have been over `kind: "amendment"`; otherwise verification fails (401).
- Replay of an old amendment with the same nonce: nonce is in the canonical bytes, so the signature is valid; but if the same body lands twice the aggregate refuses based on its state-machine (already-amended-at-version returns 409). Verify in `domain/aggregate.rs::handle_amend`. **(audit candidate)**

---

### A.6 `POST /v1/declarations/{declaration_id}/correct`

| Field                          | Value                                                                                                                                                              |
|--------------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| Route definition               | `rest.rs:136-139`.                                                                                                                                                 |
| Handler                        | `correct_declaration` — `rest.rs:818-863` (head only seen — full body not opened in this pass; signature consistent with `amend`).                                |
| Auth state                     | Authenticated principal.                                                                                                                                            |
| Crypto                         | Canonical bytes include `declaration_id`, `declarant_principal`, `kind: "correction"`, optional `metadata_notes`, `nonce_hex` (`rest.rs:670-696`). The correction can therefore only change `metadata_notes` — not the declaration body. |
| State machine                  | Corrections are only admitted for declarations in `submitted` state (per OpenAPI 409 description) — verify in aggregate.                                          |

**Forbidden-access trace:** same as amend; non-owner 403, state-machine refuses outside `submitted` state.

---

### A.7 Admin DLQ — `GET /v1/internal/outbox-dlq`, `POST /v1/internal/outbox-dlq/{id}/replay`

| Field                          | Value                                                                                                                                                              |
|--------------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| Route definitions              | `rest.rs:158-167`.                                                                                                                                                 |
| Handlers                       | `dlq::list_dlq` (`api/dlq.rs:148-172`) and `dlq::replay_dlq` (`api/dlq.rs:195-235`).                                                                              |
| Auth state                     | Authenticated principal **AND** subject in `ADMIN_PRINCIPALS` allowlist.                                                                                            |
| Auth check                     | `auth_middleware` (route_layer at `rest.rs:163-166`).                                                                                                              |
| Authorization                  | `enforce_admin(&state.admin_principals, &principal)` — `api/dlq.rs:243-261`. Empty allowlist → 503 (admin disabled). Subject not in allowlist → 403 + `metrics.dlq_admin_denied_total{reason="not_in_allowlist"}`. |
| User population                | Declared-platform operators. Subjects supplied via `ADMIN_PRINCIPALS` env (CSV).                                                                                   |
| Reads                          | `outbox_dlq` table.                                                                                                                                                |
| Writes                         | Atomic move row from `outbox_dlq` back into `outbox` (re-arm). Updates `dispatch_attempts`. Deletes the DLQ row.                                                  |

**Forbidden-access trace:**
- No token → 401 at middleware.
- Authenticated but not admin → 403 with `dlq_admin_denied_total{reason="not_in_allowlist"}` metric increment (allowlist comparison is constant-time? — `HashSet::contains` is not constant-time on the key, but subjects are not secret material here, so the side channel is irrelevant).
- Allowlist empty → 503 with warning log (safe default).

**Drift observation:** `services/verification-engine/src/api/dlq.rs` mirrors this code one-for-one with a different DLQ path (`/v1/internal/verification-outbox-dlq`). The two DLQ admin surfaces are deliberately path-distinct so operators can tell them apart when both services are deployed (confirmed by code comments in both).

---

### A.8 Internal webhook — `POST /v1/internal/verification-outcomes`

| Field                          | Value                                                                                                                                                              |
|--------------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| Route definition               | `rest.rs:191-197`.                                                                                                                                                 |
| Handler                        | `handle_verification_outcome` — `internal.rs:107-…`.                                                                                                              |
| Auth state                     | HMAC-SHA256 over the raw body via `X-RECOR-Signature`, dual-secret rotation (current + old).                                                                       |
| Auth check                     | `internal.rs:140-176`. Constant-time verify (`verify_hmac_with_rotation`). Empty `hmac_secret` with `hmac_required=true` → 503. Missing signature → 401. Bad signature → 401. |
| Authorization                  | None beyond HMAC — the signing peer is the entire authorisation surface.                                                                                            |
| mTLS gate (R-LOOP-3)           | When `AUTH_TRANSPORT=mtls-only`, HMAC is skipped and the TLS peer SPIFFE ID is the sole authenticator. The mTLS termination + peer-SPIFFE-ID check happens **at the tower layer** wired in `main.rs` (per the inline comment in `rest.rs:175-187`). Verify the actual layer wiring in `services/declaration/src/main.rs`. `[FINDING:medium]` the handler's `expected_peer_spiffe_id` field is set in state but is currently only "consumed inside the handler for logging + future per-route enforcement" per the comment at `rest.rs:181-187`. That means the handler does **not** itself re-check the peer SPIFFE ID; the check lives in the outer tower layer. If the layer is mis-wired in any composition root, the handler will silently accept. |
| Bodies accepted                | `event_type == "verification.completed.v1"` only — others 202'd (`internal.rs:194-208`).                                                                            |
| Writes                         | `declaration_events` (`VerificationOutcomeRecorded` event) + `declarations` re-projection (verification_state, verification_lane). Idempotent on `case_id`.        |

**Forbidden-access trace:**
- No `X-RECOR-Signature` header → 401 `missing_signature`.
- Wrong signature → 401 `bad_signature`. Both current and old secrets attempted (rotation slot).
- Body tampered → HMAC fails → 401.
- Event ID replay (same case_id, second submission) → use case sees existing record → 200 with `recorded_new_event: false`.
- Empty `WRITEBACK_HMAC_SECRET` at startup → service starts (default `""`); first inbound request hits the empty-secret branch → 503.

---

### A.9 Healthz / Readyz / Metrics / OpenAPI

| Path             | Auth                          | Handler                                                                                       | Notes                                                                                        |
|------------------|-------------------------------|-----------------------------------------------------------------------------------------------|----------------------------------------------------------------------------------------------|
| `GET /healthz`   | None                          | `rest.rs:297-308`                                                                             | Always 200 if process is alive. Health metric `health_check_duration_seconds{probe="healthz"}` updated. |
| `GET /readyz`    | None                          | `rest.rs:319-355`                                                                             | DB probe via `idempotency.pool() SELECT 1`. 503 if DB unreachable.                            |
| `GET /metrics`   | None                          | `metrics::metrics_handler`                                                                    | Prometheus text exposition. **In-cluster network only is the assumed protection.** `[FINDING:high]` — `infrastructure/networks/` is empty (no NetworkPolicy). The `/metrics` endpoint is exposed on the same listener as the public API. In a misconfigured deployment, scraping the metrics from outside the cluster leaks operational fingerprints (DLQ size gauge, OIDC verify counters, governor rejections, etc.). |
| `GET /openapi.json` | None                       | `api/openapi.rs::openapi_routes`                                                              | Public spec (intentional). DOC-1.                                                            |
| `GET /docs`         | None                       | Scalar UI via `utoipa-scalar`.                                                                | Public.                                                                                       |

---

## B. Declaration service — gRPC surface (`services/declaration/src/api/grpc.rs`)

**Service:** `recor.declaration.v1.DeclarationService` (`contracts/declaration.proto:39`).

**Five RPCs**, all mounted on the same OIDC-intercepted server (`grpc.rs:73-83`):

| RPC                      | Handler (file:line)         | Auth                | Authorization                                                                              | Crypto check                       |
|--------------------------|-----------------------------|---------------------|--------------------------------------------------------------------------------------------|------------------------------------|
| `SubmitDeclaration`      | `grpc.rs:225`               | OIDC bearer (interceptor) | None at handler (declarant bound to principal).                                       | Attestation re-verified at `grpc.rs:256` |
| `GetDeclaration`         | `grpc.rs:309`               | OIDC bearer         | Owner-only check at `grpc.rs:325-329`.                                                     | None (read).                       |
| `SupersedeDeclaration`   | `grpc.rs:334`               | OIDC bearer         | Aggregate-level (handler relies on use case).                                              | Attestation re-verified.            |
| `AmendDeclaration`       | `grpc.rs:406`               | OIDC bearer         | Aggregate-level + handler 403 at `grpc.rs:434`.                                            | Attestation re-verified.            |
| `CorrectDeclaration`     | `grpc.rs:482`               | OIDC bearer         | Aggregate-level.                                                                            | Attestation re-verified.            |

- **Auth interceptor:** `auth_interceptor(GrpcAuthConfig)` at `grpc.rs:115`. Synchronous from tonic's perspective; bridges to async OIDC verify via `block_in_place + Handle::current().block_on`. Logs and returns `Status::unauthenticated` on missing creds (`grpc.rs:173, 178, 202, 212`).
- **Dev path:** also accepted via metadata `x-recor-dev-principal` (mirrors REST), gated by `is_dev`.
- **Mapping to status codes:** authentication → `Status::unauthenticated`; authorisation → `Status::permission_denied`; domain errors mapped via `*_error_to_status` helpers.

`[FINDING:none]` Surface mirror of REST.

`[FINDING:low]` The gRPC service does **not** expose `list_declarations_by_principal` (COMP-1 data-subject access). It is REST-only. Documented as such (`contracts/declaration.proto` has no `ListByPrincipal` RPC). If COMP-1 is consumer-callable, gRPC-only consumers cannot exercise the right.

**Forbidden-access trace (gRPC):**
- No metadata + no dev metadata → interceptor returns `Status::unauthenticated`. Tonic responds with `code = UNAUTHENTICATED`.
- Cross-principal `GetDeclaration` → handler returns `Status::permission_denied`.
- Cross-principal `Amend/Supersede/Correct` → aggregate refuses; status mapped to `permission_denied`.

---

## C. Verification engine — REST surface (`services/verification-engine/src/api/rest.rs`)

### C.1 `POST /v1/verifications`

| Field                          | Value                                                                                                                                                              |
|--------------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| Route definition               | `rest.rs:63`.                                                                                                                                                      |
| Handler                        | `submit_verification` — `rest.rs:229-258`.                                                                                                                         |
| Auth state                     | OIDC bearer (`auth_middleware` route_layer at `rest.rs:65-69`).                                                                                                    |
| Auth check                     | `services/verification-engine/src/api/auth.rs` (mirror of declaration auth).                                                                                       |
| Authorization                  | **None.** Handler extracts the principal via `axum::Extension(_principal)` (`rest.rs:231`) — the underscore prefix is the language signal that the value is intentionally unused. **Any authenticated principal can submit a verification request for any declaration snapshot.** `[FINDING:high]` |
| Reads                          | The declaration snapshot is **in the request body** (`req.declaration`). No authorisation that the caller is the owner.                                            |
| Writes                         | `verification_cases`, `verification_outbox`. Plus stage-driven reads against `sanctions_persons`, `peps`, `icij_persons`, `mock_bunec_persons`.                  |

**Forbidden-access trace:** an authenticated caller can submit an arbitrary declaration snapshot and consume V-engine compute (each request hits stages 1–7 + fusion + lane, with potential outbound calls to Anthropic via the inference gateway). The handler does **not** check that the declaration_id in the snapshot is owned by the caller, nor that the caller has any relationship to the entity. **Cost-DoS surface.**

- `[FINDING:high]` No rate limit on `/v1/verifications`. The declaration service has tower-governor on submit; the V-engine does not. An authenticated caller can drive V-engine load and Anthropic API spend without bound.
- `[FINDING:medium]` Authorization is missing in v1 — but this endpoint should arguably **only be callable by the declaration service via the writeback channel**, not by direct user principals. The path is non-internal and has no HMAC. Consider downgrading to internal-only.
- In practice the V-engine consumes declarations via the **`POST /v1/internal/declaration-events`** HMAC webhook (C.2 below); the public `/v1/verifications` REST surface seems to be a developer/test surface that escaped into production. Verify against ARCHITECTURE.

### C.2 `GET /v1/verifications/{case_id}`

| Field                          | Value                                                                                                                                                              |
|--------------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| Route definition               | `rest.rs:64`.                                                                                                                                                      |
| Handler                        | `get_verification` — `rest.rs:260-265`.                                                                                                                            |
| Auth state                     | OIDC bearer.                                                                                                                                                       |
| Authorization                  | **None.** Same underscore-principal pattern (`rest.rs:262`). `[FINDING:high]` Any authenticated principal can read any verification case, including the fusion belief, lane, stage details, and any PII the V-engine stamped onto the case. |
| Reads                          | `verification_cases` by case_id.                                                                                                                                   |

**Forbidden-access trace:** caller A guesses (or enumerates) case_ids; the server returns the full verification case, which includes the declared beneficial owners, the stage decisions, and the fusion belief. This is a **cross-tenant data leak** at the V-engine read path. Severity is high — verification cases carry PII (people in the declaration) and Sensitive-PII derivatives (sanctions matches, PEP matches). The `data-classification.md` doc classifies these projections as PII/Sensitive-PII; this surface ignores that classification.

### C.3 `POST /v1/internal/verification-outbox-dlq` admin

Mirror of A.7. Same shape, different table (`verification_outbox_dlq`), different path. See `services/verification-engine/src/api/dlq.rs:148-235`.

### C.4 `POST /v1/internal/declaration-events`

| Field                          | Value                                                                                                                                                              |
|--------------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| Route definition               | `rest.rs:119-121`.                                                                                                                                                 |
| Handler                        | `handle_declaration_event` — `api/internal.rs:107-…`.                                                                                                              |
| Auth state                     | HMAC-SHA256 with dual-secret rotation (`INBOUND_HMAC_SECRET`, `INBOUND_HMAC_SECRET_OLD`).                                                                          |
| Auth check                     | `api/internal.rs:113-145`. Mirrors A.8 — constant-time HMAC verify; empty-secret + `hmac_required` → 503; missing header → 401; bad sig → 401.                    |
| mTLS                           | Same R-LOOP-3 layering pattern; SPIFFE ID gate at the tower layer (not in-handler).                                                                                |
| Bodies accepted                | `event_type == "declaration_submitted_v1"` (verify against canonical event type names; see `internal.rs:147+`).                                                  |
| Writes                         | `verification_cases` (case kicked off through the pipeline), `verification_outbox` (writeback envelope).                                                          |

**Forbidden-access trace:** identical to A.8 — HMAC failure → 401 fail-closed. The declaration-service signs with `RELAY_HMAC_SECRET`; the V-engine verifies with `INBOUND_HMAC_SECRET`. Same physical secret, two env names, two services. Rotation per `docs/runbooks/hmac-secret-rotation.md`.

### C.5 Healthz / Readyz / Metrics

| Path             | Auth   | Handler                | Notes                                                                                  |
|------------------|--------|------------------------|----------------------------------------------------------------------------------------|
| `GET /healthz`   | None   | `rest.rs:165`          | Always 200.                                                                            |
| `GET /readyz`    | None   | `rest.rs:177-217` (approx) | DB probe.                                                                              |
| `GET /metrics`   | None   | `metrics::metrics_handler` | `[FINDING:high]` same exposure concern as A.9. Per-stage durations, lane counters, Anthropic budget metric leak. |
| (no `/openapi.json`) | n/a | n/a                    | `[FINDING:medium]` `TODO(R-VER-OPENAPI)` at `rest.rs:3` — V-engine has no OpenAPI spec yet. |

---

## D. Person service — REST surface (`services/person-service/src/api/rest.rs`)

### D.1 `POST /v1/persons`

| Field                          | Value                                                                                                                                                              |
|--------------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| Route definition               | `rest.rs:64`.                                                                                                                                                      |
| Handler                        | `register_person` — `rest.rs:194-…`.                                                                                                                              |
| Auth state                     | OIDC bearer.                                                                                                                                                       |
| Authorization                  | None at handler — any authenticated principal can register a person. `[FINDING:high]` Person rows carry Sensitive-PII (national_id, primary_id_document, etc. per migration `0001_init.sql`). Anyone with a token can create person rows, including impersonation rows whose `person_id` is then referenced by a declaration. |
| Idempotency                    | Per-principal idempotency key.                                                                                                                                      |
| Writes                         | `persons`, `person_events`.                                                                                                                                        |

**Forbidden-access trace:** caller A creates a person with a chosen `person_id`; subsequent declarations from any declarant can refer to that `person_id`. The registration is by-creation; there is no operator review path. Coupled with declaration-service's `register_person` in the declaration submit path (the declaration body carries the `person_id`), this is the **identity-injection surface** for the platform. Compliance impact: any caller can pollute the canonical registry with sensitive PII rows.

### D.2 `GET /v1/persons/{id}`

| Field                          | Value                                                                                                                                                              |
|--------------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| Route definition               | `rest.rs:66`.                                                                                                                                                      |
| Handler                        | `get_person` — `rest.rs:296-308`.                                                                                                                                  |
| Auth state                     | OIDC bearer.                                                                                                                                                       |
| Authorization                  | **None.** Explicit comment at `rest.rs:301-304`: *"Reserved for future ABAC; v1 grants any authenticated principal read access. The Person service holds Sensitive-PII so this MUST be tightened before the Sensitive-PII columns are surfaced to non-operator principals. The follow-up ticket is R-PERSON-RBAC."* `let _ = principal` at `rest.rs:305`. |
| Reads                          | `persons` projection.                                                                                                                                              |

`[FINDING:high]` This is documented as a deferred gap, but the deferral itself is a violation of doctrine D14 (fail-closed) and D17 (zero trust at every network boundary). Sensitive-PII columns include national_id, date_of_birth, nationality, primary_id_document. The classification doc explicitly marks these as Sensitive-PII; the endpoint serves them today to any authenticated bearer.

**Forbidden-access trace:** caller A passes a UUID, gets back the full person record including any Sensitive-PII fields the projection serializes. There is no per-row authorisation; there is no admin gate. Anyone who can issue an OIDC token (which in dev defaults to *any HTTP caller with the dev header*) reads any person.

### D.3 `GET /v1/persons/search`

| Field                          | Value                                                                                                                                                              |
|--------------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| Route definition               | `rest.rs:65`.                                                                                                                                                      |
| Handler                        | `search_persons` — `rest.rs:337-353`.                                                                                                                              |
| Authorization                  | **None.** Same `let _ = principal` pattern (`rest.rs:339-340`). Returns trigram-matched person rows with full projection. `[FINDING:high]` |
| Reads                          | `persons` projection filtered on canonical_full_name + optional nationality.                                                                                       |

**Forbidden-access trace:** caller A submits a substring; the platform returns matched people. This is a **directory-enumeration surface** for Sensitive-PII. Even ignoring full-row leakage in D.2, search lets a caller find the canonical UUID of any person by name.

### D.4 `POST /v1/persons/{id}/merge-into/{target_id}`

| Field                          | Value                                                                                                                                                              |
|--------------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| Route definition               | `rest.rs:67-…`.                                                                                                                                                    |
| Handler                        | `merge_persons` — `rest.rs:385-…`.                                                                                                                                |
| Authorization                  | Admin-principal allowlist check at handler (`rest.rs:393-402`). Empty allowlist → 503. Non-admin → 403. **Correct admin gate** — mirror of declaration's DLQ pattern. |

### D.5 Healthz / Readyz / Metrics

Same shape as A.9; same `[FINDING:high]` concern on unauthenticated `/metrics` exposure outside the cluster.

---

## E. Entity service — REST surface (`services/entity-service/src/api/rest.rs`)

Per `docs/compliance/data-classification.md`, entity data is **Public** with `created_at`/`updated_at` Internal. The authorisation posture below is more defensible than person-service's because the underlying data is public.

### E.1 `POST /v1/entities`

| Field                          | Value                                                                                                                                                              |
|--------------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| Route definition               | `rest.rs:69`.                                                                                                                                                      |
| Handler                        | `register_entity` — `rest.rs:205-303`.                                                                                                                             |
| Auth                           | OIDC bearer.                                                                                                                                                       |
| Authorization                  | None at handler — anyone authenticated can register an entity. Marked with `TODO(R-VER-1)` (`rest.rs:259-261`): "wire BUNEC as source-of-truth for jurisdiction == CM". |
| Idempotency                    | Yes, hashed over body + principal subject.                                                                                                                          |
| Writes                         | `entities`, `entity_events`, `outbox`.                                                                                                                             |
| Outbox drain                   | **Not wired.** `[FINDING:medium]` `entity-service` has an `outbox` table but no relay implementation in `src/infrastructure/` (only `postgres.rs`, `mod.rs`). Events accumulate without being delivered. |

### E.2 `GET /v1/entities/{id}`

| Field                          | Value                                                                                                                                                              |
|--------------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| Handler                        | `get_entity` — `rest.rs:319-329`.                                                                                                                                  |
| Authorization                  | Comment `// any authenticated caller may read the public projection` at `rest.rs:325`. Consistent with the entity-public classification.                          |

### E.3 `GET /v1/entities/search`

Same shape — public read, any authenticated principal.

### E.4 `POST /v1/entities/{id}` (update) and `POST /v1/entities/{id}/dissolve`

| Field                          | Value                                                                                                                                                              |
|--------------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| Route definition               | `rest.rs:72-79` (full snippet not opened — handlers at `rest.rs:401` and `rest.rs:448`).                                                                          |
| Auth                           | OIDC bearer.                                                                                                                                                       |
| Authorization                  | `update_entity_handler` and `dissolve_entity_handler` — admin gating or owner gating to be verified. `[FINDING:medium]` This pass did not enumerate the bodies; entry in 10-findings to verify gate. Update/dissolve are state-changing operations that, on a public-classified projection, still need to be operator-only. |

---

## F. Audit verifier — REST surface (`apps/audit-verifier/src/handlers.rs`)

### F.1 `GET /v1/audit/verify/{declaration_id}`

| Field                          | Value                                                                                                                                                              |
|--------------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| Route definition               | `handlers.rs:29`.                                                                                                                                                  |
| Handler                        | `verify` — `handlers.rs:42-128` (approx).                                                                                                                          |
| Auth state                     | **NONE.** Router has no `axum::middleware::from_fn_with_state(..., auth_middleware)` call. Verified in `handlers.rs:26-30`. The crate's `Cargo.toml` imports `recor-auth-oidc` but the router does not consume it. |
| Authorization                  | None.                                                                                                                                                              |
| Reads                          | `outbox` + `declaration_events` from the **Declaration service's** Postgres DB (`apps/audit-verifier/src/projection.rs:68-83`). Plus Fabric audit channel for on-chain records (`fabric_client.rs`). |
| Writes                         | None.                                                                                                                                                              |

**Forbidden-access trace:** anonymous caller hits `GET /v1/audit/verify/{declaration_id}`:
1. Router accepts the request (no middleware).
2. Handler validates the UUID format.
3. Handler queries Fabric for the on-chain entries (`state.fabric.list_for_declaration`).
4. Handler queries Postgres `outbox JOIN declaration_events` for each event_id from Fabric — full event_payload returned (`apps/audit-verifier/src/projection.rs:68-83`).
5. Handler builds a verification report containing on-chain hashes + projection hashes + tampering verdict.

`[FINDING:high]` The verification report leaks every declaration event by UUID. The receipt-hash design is publicly verifiable by intent, but the verifier returns the *full canonical event payload* (the declarant's identity, the beneficial-owner list, etc.) to compute the hash on-the-fly. An unauthenticated caller can therefore retrieve any declaration's full body by UUID enumeration.

The architecturally correct shape is: the verifier accepts a *caller-supplied canonical payload* and verifies it against the on-chain hash. The committed code returns the projection payload, which defeats the privacy property of receipt-based verification.

Additionally:
- No rate limiting on this endpoint.
- No CORS, no security headers in the router. Verify `apps/audit-verifier/src/handlers.rs` does not add `SetResponseHeader`.
- `/healthz`, `/readyz` also unauthenticated (handlers return static "ok"/"ready"). No metrics endpoint at all.

Severity is **high**: a national beneficial-ownership registry with an unauthenticated full-disclosure endpoint by UUID exceeds the threat-model in `docs/security/threat-model.md`.

---

## G. Worker — fabric-bridge HTTP surface (`apps/worker-fabric-bridge/src/handlers.rs`)

### G.1 `POST /v1/relay`

| Field                          | Value                                                                                                                                                              |
|--------------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| Route definition               | `handlers.rs:35`.                                                                                                                                                  |
| Handler                        | `receive` — `handlers.rs:67-110`.                                                                                                                                  |
| Auth state                     | HMAC-SHA256 over body, header `X-RECOR-Signature` (`handlers.rs:71-79`).                                                                                            |
| Auth check                     | `verify_hmac(secret, body, sig_hex)` at `handlers.rs:111-118`. Constant-time via `mac.verify_slice` (verified).                                                    |
| Rotation slot                  | **None.** `WorkerConfig` exposes only `RECOR_FABRIC_BRIDGE_HMAC` (single secret). `[FINDING:medium]` Outbox→Fabric channel cannot be rotated without downtime.    |
| Authorization                  | None beyond HMAC.                                                                                                                                                  |
| Writes                         | Fabric world state (via chaincode); `fabric_bridge_dlq` on permanent failure.                                                                                       |
| Returns                        | 200 with `committed:<tx_id>` / `already_committed:<tx_id>` / `dead_lettered:<cause>` / `skipped`; 503 transient (relay retries).                                  |

**Forbidden-access trace:**
- No signature → 401 `invalid signature`.
- Wrong signature → 401.
- Body tampered → HMAC fails → 401.
- Caller floods `/v1/relay` with valid signatures (compromised secret) → no rate limit at the handler; the only ceiling is the Fabric gateway. `[FINDING:medium]` rate-limit absent.

### G.2 Healthz / Readyz / Metrics

| Path             | Auth   | Notes                                                                                  |
|------------------|--------|----------------------------------------------------------------------------------------|
| `GET /healthz`   | None   | Always 200.                                                                            |
| `GET /readyz`    | None   | Returns 200 + side-effect: touches `metrics.anchor_total{committed}` (counter `with_label_values` only — no `.inc()` — so this is a label registration, not an increment). `handlers.rs:43-49`. |
| `GET /metrics`   | None   | Standard Prometheus exposition. Same in-cluster-trust concern. `[FINDING:high]`        |

---

## H. Declarant portal — Pages and discoverability

The portal is a single-page React app served by nginx (`applications/declarant-portal/nginx.conf.template`). There is **one logical surface**: the declaration wizard.

### H.1 `/` — declaration wizard (single route)

- **Entry:** `applications/declarant-portal/src/App.tsx`. The app mounts `<DeclarationForm apiBaseUrl={API_BASE_URL} />` directly (`App.tsx:53`). No client-side router (`react-router` is not in the dep tree).
- **Steps (R-PORT-3):** 4-step wizard in `applications/declarant-portal/src/features/declaration/wizard/`:
  1. `EntityStep.tsx` — entity_id, declarant_principal, declarant_role, kind, effective_from.
  2. `OwnersStep.tsx` — beneficial owners (`useFieldArray`).
  3. `ReviewStep.tsx` — read-only summary + canonical-byte preview.
  4. `SignStep.tsx` — public-key confirmation + Sign-and-Submit CTA.
- **Verification status:** `VerificationStatus.tsx` polls `GET /v1/declarations/{id}` every ~3s after submission. Uses TanStack Query.
- **Auth state expected by API:** OIDC bearer. **The portal does not implement OIDC login flows itself.** It assumes `API_BASE_URL` is fronted by an IdP-aware gateway, or that local dev uses the `X-Recor-Dev-Principal` header. `[FINDING:medium]` In production this means the portal nginx layer (or an upstream OIDC proxy) is the OIDC client; the portal's React code is OIDC-agnostic. No code in the portal handles tokens. There is no logout button, no token refresh path, no error path for token expiry. This is a defendable design (proxy-mediated OIDC) but it should be explicit in the architecture; nothing in `applications/declarant-portal/CLAUDE.md` covers the proxy layer's OIDC role.
- **Browser crypto:** Web Crypto API. `applications/declarant-portal/src/lib/crypto.ts`. No third-party crypto lib. Parity test `crypto.test.ts`.
- **Offline drafts:** Dexie via `src/lib/drafts/index.ts`. Service worker via `vite-plugin-pwa`. API endpoints are excluded from cache via `navigateFallbackDenylist`.
- **i18n coverage:** FR (primary), EN (full), Pidgin (stub). Locale selector at `App.tsx:65-100`.
- **Discoverability gating:** none — the portal is a single page with one form. No alternative routes are gated.

`[FINDING:none]` The portal is structurally minimal and the audit findings here are inherited from the API layer (the API is the only thing the portal can call).

### H.2 Static security headers

`applications/declarant-portal/security-headers.conf.template` defines CSP, X-Frame-Options, Referrer-Policy, etc. The `headers-smoke.sh` script validates them post-build. OPS-3.

### H.3 Service worker

Vite-plugin-pwa generates `sw.js`. `registerType: 'autoUpdate'` silently updates installed clients. `injectRegister: 'auto'` injects the registration code. The SW caches the SPA shell + hashed JS/CSS chunks only.

`[FINDING:low]` `autoUpdate` with no user confirmation is acceptable here because the portal has no user-modifiable local state outside drafts (drafts live in a separate Dexie DB, not invalidated by SW updates). But: an attacker who can serve a modified `sw.js` (via a CDN compromise or DNS hijack) gets persistent code execution on all installed clients. SRI / cosign-on-static-assets is not configured in `nginx.conf.template`. Out of scope for Pass A but flagged for downstream review.

---

## I. Surface-level summary table

| Service / app                  | Auth required                          | Authorization model                                                          | Audit findings (severity)                                                                                          |
|--------------------------------|----------------------------------------|------------------------------------------------------------------------------|--------------------------------------------------------------------------------------------------------------------|
| declaration REST submit + COMP-1 + by-id | bearer OR dev header             | Owner-only (declaration), principal-bound submit                            | none structural; LOW timing-existence leak on GET-by-id                                                            |
| declaration REST admin DLQ     | bearer + ADMIN_PRINCIPALS              | Allowlist                                                                    | none                                                                                                                |
| declaration REST internal      | HMAC + (optional mTLS gate at outer layer) | HMAC peer is sole authoriser                                            | MEDIUM mTLS gate lives in outer layer; if mis-wired, peer SPIFFE check is skipped silently                          |
| declaration gRPC               | bearer (interceptor)                   | Mirror of REST                                                               | LOW gRPC missing COMP-1 surface                                                                                     |
| declaration `/metrics`         | None                                   | n/a                                                                          | HIGH no NetworkPolicy committed; metrics expose ops state                                                          |
| V-engine REST submit + get     | bearer                                 | **None** (`let _ = principal` / `_principal`)                                | HIGH cross-tenant read of verification cases; HIGH unbounded V-engine compute / Anthropic spend                    |
| V-engine REST admin DLQ        | bearer + admin allowlist               | Allowlist                                                                    | none                                                                                                                |
| V-engine REST internal         | HMAC                                   | HMAC peer                                                                    | MEDIUM mTLS gate as above                                                                                            |
| V-engine `/metrics`            | None                                   | n/a                                                                          | HIGH                                                                                                                |
| V-engine OpenAPI               | n/a                                    | n/a                                                                          | MEDIUM `TODO(R-VER-OPENAPI)` — spec missing                                                                         |
| person POST                    | bearer                                 | None (any caller can create persons)                                          | HIGH identity-injection surface for Sensitive-PII                                                                  |
| person GET-by-id, search       | bearer                                 | **None** (deferred to R-PERSON-RBAC)                                          | HIGH Sensitive-PII leak to any authenticated caller                                                                |
| person merge                   | bearer + admin allowlist               | Allowlist                                                                    | none                                                                                                                |
| entity POST / update / dissolve | bearer                                | None at handler level (Update/Dissolve handlers not deeply audited)         | MEDIUM update/dissolve gating to verify; MEDIUM entity outbox not drained                                          |
| entity GET / search            | bearer                                 | Public projection                                                            | none (entity data is Public per classification)                                                                    |
| audit-verifier `/v1/audit/verify/{id}` | **None**                       | n/a                                                                          | HIGH unauthenticated full-disclosure surface; serves declaration payload by UUID                                  |
| audit-verifier health/ready    | None                                   | n/a                                                                          | LOW (no info disclosure beyond liveness)                                                                            |
| worker-fabric-bridge `/v1/relay` | HMAC                                | HMAC peer (no rotation slot)                                                 | MEDIUM no dual-secret rotation; MEDIUM no rate limit                                                               |
| worker `/metrics`              | None                                   | n/a                                                                          | HIGH                                                                                                                |
| portal `/`                     | (proxy-mediated)                       | n/a (single page)                                                            | MEDIUM portal-OIDC story is implicit; LOW SW supply-chain                                                          |

---

## J. Findings recap (severity-ranked)

**HIGH**
1. V-engine REST `submit_verification` and `get_verification` have no authorisation — any authenticated principal can read or trigger any case.
2. V-engine REST `/v1/verifications` has no rate limit; Anthropic-spend DoS via unbounded calls.
3. person-service `GET /v1/persons/{id}`, `GET /v1/persons/search` — any authenticated principal reads Sensitive-PII. Deferred to R-PERSON-RBAC.
4. person-service `POST /v1/persons` — any authenticated principal can create person rows with PII fields.
5. audit-verifier `GET /v1/audit/verify/{declaration_id}` — **unauthenticated**; returns full declaration payload by UUID.
6. `/metrics` endpoints unauthenticated + no NetworkPolicy committed (`infrastructure/networks/` empty). Applies to all 4 services + 1 worker.

**MEDIUM**
7. R-LOOP-3 mTLS peer-SPIFFE-ID check lives in the outer tower layer; if mis-wired in any composition root the handlers silently accept. No unit test asserts the layer is present.
8. entity-service has an `outbox` table but no relay drain.
9. worker-fabric-bridge HMAC has no rotation slot (single secret only).
10. worker-fabric-bridge `/v1/relay` has no rate limit.
11. entity-service update/dissolve handlers not deeply audited in this pass — gating to verify downstream.
12. V-engine has no committed OpenAPI snapshot (`TODO(R-VER-OPENAPI)`).
13. Portal proxy-mediated OIDC is implicit — no committed proxy config or token-refresh story.

**LOW**
14. `GET /v1/declarations/{id}` returns 403 for non-owner (vs 404) — minor existence side-channel.
15. gRPC surface lacks COMP-1 by-principal RPC; gRPC consumers cannot exercise the data-subject access right.
16. Portal `vite-plugin-pwa` `autoUpdate` SW without SRI / cosign on static assets.

End of Section 02 (Surfaces).
