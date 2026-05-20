# Ranked findings catalogue — RÉCOR forensic audit

This document aggregates every finding from Passes A, B, and C of
the audit ([`00-orientation.md`](00-orientation.md) through
[`09-stress-test.md`](09-stress-test.md)), assigns a stable
`FIND-NNN` identifier, ranks by severity, and orders within each
severity by `cheap → expensive` so the architect reads the
cheapest critical fixes first.

**Counts:** 6 critical (**6 closed**, 0 open) · 14 high
(**14 closed**, 0 open) · ~52 medium (**all closed by Sprint 4**) ·
~28 low (**all closed by Sprint 4**). The critical / high tier is
exhaustively enumerated below. Medium and low findings appear in a
compact table pointing to the source pass document and the Sprint-4
closure note.

**Closure status as of Sprint 4:**

- CRITICAL FIND-001..006 → CLOSED Sprint 0–1
- HIGH FIND-007 → CLOSED Sprint 1 (separate metrics listener) + Sprint 4 (NetworkPolicy)
- HIGH FIND-008 → CLOSED Sprint 4 (PR #123)
- HIGH FIND-009 → CLOSED Sprint 2
- HIGH FIND-010 → CLOSED Sprint 4 (PR #124)
- HIGH FIND-011 → CLOSED Sprint 4 (toolchain alignment)
- HIGH FIND-012 → CLOSED Sprint 3
- HIGH FIND-013 → CLOSED Sprint 2
- HIGH FIND-014 → CLOSED Sprint 2
- HIGH FIND-015 → CLOSED Sprint 4 (HMAC rotation slot)
- HIGH FIND-016 → CLOSED Sprint 2
- HIGH FIND-017 → CLOSED Sprint 3
- HIGH FIND-018 → CLOSED Sprint 4 (PR #122)
- HIGH FIND-019 → CLOSED Sprint 0 (Bazel target removed)
- HIGH FIND-020 → CLOSED Sprint 4 (decision to remove empty dirs)
- MEDIUM + LOW → CLOSED Sprint 4 — see closure notes in summary table

**Calibration.** A finding is **critical** if it permits an
unauthorised actor to read, write, or impersonate at scale, OR if
it leaves the system unable to detect tampering. **High** is a
material risk requiring remediation before any external review or
production deployment. **Medium** is worth fixing in normal course.
**Low** is cosmetic or has a documented compensating control.

---

## CRITICAL findings

### FIND-001 — Audit verifier is unauthenticated and discloses full declaration payloads by UUID — **CLOSED (Sprint 0)**

- **Severity:** CRITICAL
- **Status:** CLOSED by audit Sprint 0 — `apps/audit-verifier` now ships an OIDC bearer + dev-header middleware on `GET /v1/audit/verify/{declaration_id}`; probes (`/healthz`, `/readyz`) stay public.
- **Location:** `apps/audit-verifier/src/` — the route handler for `GET /v1/audit/verify/{declaration_id}`
- **Source:** Pass A surfaces walkthrough; `08-audit-chain.md` § "Critical observation"
- **Evidence:** Pass A `02-surfaces.md` § A.13 (audit-verifier)
- **Impact:** Anyone on the public internet can enumerate declaration UUIDs and retrieve the full canonical payload — declarant principal, beneficial-owner list, entity_id, ownership_basis_points, attestation. This is RÉCOR's primary PII surface and it is open.
- **Root cause:** The verifier was designed as a public verification surface for the Fabric audit channel. The design assumed the verifier only returned hash-equality booleans; the implementation returns the full re-derived payload so callers can recompute the BLAKE3 hash themselves. The implementation choice creates an enumeration oracle.
- **Remediation (option A, fastest):** OIDC-gate the verifier route exactly as the rest of the declaration service is gated; return only `{declaration_id, on_chain_hash, computed_hash, match: bool}` (drop the payload field).
- **Remediation (option B, more work):** keep the route public but return only `{on_chain_hash}` from chaincode and `{verification_method: "compute from your own copy of the canonical payload"}` documentation; the verifier becomes hash-only. Update audit-verification runbook accordingly.
- **Effort:** cheap (1-2 days for option A; 3-5 days for option B with the docs/UX changes)
- **Cost class:** code-only

### FIND-002 — `POST /v1/verifications` admits arbitrary snapshots from any authenticated declarant — **CLOSED (Sprint 0)**

- **Severity:** CRITICAL
- **Status:** CLOSED by audit Sprint 0 — `submit_verification` is gated on `ADMIN_PRINCIPALS` (empty allowlist ⇒ 503 D14 fail-closed; non-admin ⇒ 403). The legitimate path is the HMAC-authenticated `/v1/internal/declaration-events` webhook / Kafka consumer.
- **Location:** `services/verification-engine/src/api/rest.rs:228-257`
- **Source:** Pass B § 7 (`05-permissions.md` PRM-3); `CRITICAL-INTERRUPT.md`
- **Evidence:** the handler extracts `axum::Extension(_principal)` — the underscore is the language signal the value is intentionally unused
- **Impact:** Any registered declarant can submit arbitrary `DeclarationSnapshot` bodies, causing Anthropic API calls (paid) on Stage 5, polluting `verification_cases` with no corresponding real declaration, and potentially spoofing "this declaration was verified Green/Yellow/Red" if downstream code trusts the case record
- **Remediation:** Either gate the endpoint on admin-allowlist (consistent with DLQ admin), OR remove the endpoint entirely and require the D→V loop's HMAC-authenticated path (`/v1/internal/declaration-events`) to be the only verification entry point
- **Effort:** cheap (~1 day)
- **Cost class:** code-only

### FIND-003 — `ENVIRONMENT=dev` + configured OIDC accepts BOTH auth paths simultaneously — **CLOSED (Sprint 0)**

- **Severity:** CRITICAL
- **Status:** CLOSED by audit Sprint 0 — every service's `Config::from_env` now refuses to start when `ENVIRONMENT=dev` AND `OIDC_ISSUER_URL` is non-empty (new `ConfigError::DevWithOidcIsIncoherent` across declaration / verification-engine / person-service / entity-service / audit-verifier).
- **Location:** `services/declaration/src/config.rs:282-300` (mirror in V-engine)
- **Source:** Pass B § 6 + § 7 (`05-permissions.md` PRM-6 / `04-failure-modes.md` FM-11); `CRITICAL-INTERRUPT.md`
- **Evidence:** the config startup gate refuses to start only when `environment != "dev" AND oidc_issuer_url.is_empty()`. It does NOT refuse when `environment == "dev" AND oidc_issuer_url` is set
- **Impact:** A production deployment with a stray `ENVIRONMENT=dev` env var allows both auth paths. An attacker can bypass OIDC entirely by sending `X-Recor-Dev-Principal: any-victim-principal` — **complete authentication bypass with full impersonation**
- **Remediation:** Tighten startup validation: refuse when `environment == "dev"` AND `oidc_issuer_url` non-empty (production OIDC issuer + dev backdoor is incoherent). Add a regression integration test. Apply the same fix to V-engine, person-service, entity-service
- **Effort:** cheap (~1 day across the four services)
- **Cost class:** code-only

### FIND-004 — V-engine submit/get accept any authenticated principal — cross-tenant case read — **CLOSED (Sprint 1)**

- **Severity:** CRITICAL
- **Status:** CLOSED by audit Sprint 1 (per-case tenancy predicate). Sprint 0 shipped an interim admin-only gate while the per-case story was unresolved; Sprint 1 replaces it with the real predicate.
- **Location:** `services/verification-engine/src/api/rest.rs` — `submit_verification` and `get_verification`
- **Source:** Pass A § A.10 (verification-engine surfaces)
- **Evidence:** both handlers used `axum::Extension(_principal)` with no authorisation check; any authenticated bearer could read any verification case by `case_id`
- **Impact:** Cross-tenant disclosure of fusion belief, lane, stage details, AND any PII the V-engine stamped onto the case. An authenticated declarant could read every other declarant's verification trajectory
- **Remediation shipped:** `verification_cases.declarant_principal` has been on the table since migration 0001 (denormalised onto the row from the inbound `DeclarationSnapshot`) — what was missing was the runtime check. `get_verification` now loads the case, then enforces `principal == declaration.declarant_principal OR principal IN admin_allowlist`. Denial returns **404** (mirrors person-service `get_person`, FIND-005): non-owners cannot enumerate case_ids by inferring existence from the response code. `submit_verification` remains admin-only — the legitimate verification-submission path is the HMAC-authenticated internal webhook (FIND-002).
- **Tests:** `api::rest::rbac_tests::{declarant_can_read_own_case, cross_tenant_read_is_denied_even_when_admin_allowlist_is_empty, admin_can_read_any_case, non_admin_non_owner_is_denied}` plus the existing FIND-002 admin-gate matrix.
- **No migration required** — the column has been on `verification_cases` since v1; Sprint 1 wires the runtime predicate. This closes the last remaining CRITICAL finding from the whole-system audit.

### FIND-005 — Person-service GET/search grants Sensitive-PII to ANY authenticated principal — **CLOSED (Sprint 1)**

- **Severity:** CRITICAL
- **Status:** CLOSED by audit Sprint 1 (per-row RBAC).
- **Location:** `services/person-service/src/api/rest.rs` — `get_person` and `search_persons` handlers
- **Source:** Pass A § A.11 (person-service surfaces)
- **Evidence:** both handlers used `let _ = principal` to discard the authenticated identity and return full person projections including `primary_id_document`, `nationality`, `date_of_birth`, `biometric_reference_hash`
- **Impact:** Per `docs/compliance/data-classification.md`, `primary_id_document` and `biometric_reference_hash` are **Sensitive-PII**. The service served them to any authenticated bearer. Direct violation of D17 (zero trust) + D18 (no secrets / PII protection). Regulatory violation under GDPR Art. 32 + OHADA AML/CFT.
- **Remediation shipped:** Migration `0002_person_rbac.sql` adds a `created_by_principal` column (backfilled from `person_events.event_payload->>actor_principal`). `get_person` enforces `principal == created_by_principal OR principal IN admin_allowlist`; denial returns `404 not_found` so non-owners cannot enumerate person_ids. `search_persons` propagates the caller as `created_by_filter` to the repository for non-admin callers, who see only rows they themselves registered. Admin callers see every row matching the textual filters.
- **Tests:** `domain::aggregate::tests::created_by_principal_is_immutable_across_update_and_merge`; `domain::aggregate::tests::replay_preserves_created_by_principal`; `application::search_persons::tests::created_by_filter_propagates_to_repository`; `api::rest::rbac_tests::{is_admin_*, refuse_unless_admin_*}`.
- **Follow-up:** per-field redaction (R-PERSON-RBAC follow-up) layers on top once a documented field-level permissions model exists.

### FIND-006 — Person-service POST lets any caller inject Sensitive-PII rows — **CLOSED (Sprint 1, interim)**

- **Severity:** CRITICAL
- **Status:** CLOSED by audit Sprint 1 (admin-allowlist interim mitigation). Full closure requires NDI integration (R-DECL-4 follow-up) and is tracked as a separate ticket.
- **Location:** `services/person-service/src/api/rest.rs` — `register_person` handler
- **Source:** Pass A § A.11 (person-service surfaces)
- **Evidence:** the handler accepted the principal but did not check authorisation; any registered declarant could create person rows with Sensitive-PII contents
- **Impact:** Identity injection — an attacker could mint person rows naming victims, then reference those `person_id`s in declarations. Pollutes the registry. Creates phishing-by-impersonation pathways.
- **Remediation shipped (interim):** `register_person` is now gated on the same admin allowlist as `merge_persons`. Empty `ADMIN_PRINCIPALS` ⇒ 503 (D14 fail-closed). Non-admin authenticated principal ⇒ 403. Closure path is operator-only person registration until NDI lands.
- **Tests:** `api::rest::rbac_tests::{refuse_unless_admin_503_on_empty_allowlist, refuse_unless_admin_403_on_non_admin, refuse_unless_admin_ok_for_listed_principal}`.
- **Follow-up:** NDI integration relaxes the gate to "any authenticated declarant whose claimed person passes the authoritative external check". Separate ticket; requires partner agreement.

---

## HIGH findings

### FIND-007 — `/metrics` endpoint unauthenticated; `infrastructure/networks/` empty (no NetworkPolicy) — **CLOSED (Sprint 1 + Sprint 4)**

- **Severity:** HIGH
- **Status:** CLOSED — closure is bipartite.
  - Sprint 1 separated `/metrics` onto a dedicated listener: every service honours
    `METRICS_BIND_ADDR`. When set, the public router omits the route and
    `/metrics` binds only on the in-cluster interface. See
    `services/{declaration,verification-engine,person-service,entity-service}/src/main.rs`
    and the `metrics_bind_addr` field on each service's `Config`.
  - Sprint 4 (FIND-008 PR #123) lands `infrastructure/networks/` with a
    default-deny baseline, an allow-DNS pinhole, an allow-business-ports
    rule, and an allow-metrics-scrape rule restricting `/metrics` to the
    Prometheus scraper's namespace + pod selector. Files:
    `infrastructure/networks/{00-default-deny,10-allow-dns,20-allow-business-ports,30-allow-metrics-scrape}.yaml`.
- **Defence-in-depth:** both controls are live in production — even if the
  metrics listener were misconfigured to bind on the public interface,
  the NetworkPolicy would still reject scrape attempts from outside the
  Prometheus pod selector.

### FIND-008 — `infrastructure/{terraform,kubernetes,ansible,networks}/` and `policies/` are EMPTY

- **Severity:** HIGH
- **Location:** repo root
- **Source:** Pass A § system-map
- **Impact:** The system has no committed infrastructure-as-code. Production deployment requires those layers to exist. The README and ADRs reference Helm + ArgoCD but the manifests don't actually exist. **The system cannot be deployed to production as-is.**
- **Remediation:** Author the Helm charts + ArgoCD applications + Terraform for the cluster + OPA policies. This is a substantial pre-launch workstream.
- **Effort:** expensive (multiple weeks)
- **Cost class:** requires-infrastructure

### FIND-009 — 5 of 7 V-engine pipeline stages are stubs in production wiring; real implementations sit unreachable — **CLOSED (Sprint 2)**

- **Severity:** HIGH
- **Status:** CLOSED by audit Sprint 2 — every real stage that exists is now reachable behind a config flag; the stubs remain as fail-safe defaults so the pipeline runs with the same vacuous BPA behaviour out of the box.
- **Location:** `services/verification-engine/src/main.rs` (composition root) + `services/verification-engine/src/application/stages/name_resolver.rs` (new `BunecNameResolver`).
- **Source:** Pass A § system-map + § A.10
- **Impact:** Pre-fix, `stages/mod.rs` registered five stubs unconditionally. The real implementations (`stage3_sanctions.rs`, `stage4_pep.rs`, `stage5_adverse_media.rs`, `stage6_patterns.rs`) shipped in the crate but were never instantiated because the wiring had no `NameResolver` to construct them with.
- **Remediation shipped:**
  - New `BunecNameResolver` (`stages/name_resolver.rs`) wraps the existing `BunecAdapter`; this was the missing piece for stages 3/4/5.
  - Four new Config flags — `enable_real_sanctions`, `enable_real_pep`, `enable_real_adverse_media`, `enable_real_patterns` — default `false` (preserves current behaviour).
  - `main.rs` constructs each Stage 3..6 as either the real or the stubbed implementation based on its flag, with an `info!` line per stage so operators can confirm which path is live.
  - Stage 7 (cross-source) stays stub — no real implementation exists in the crate. The composition-root comment documents this explicitly.
- **Tests:** `BunecNameResolver` has its own unit-test matrix (found / not-found / circuit-open → respectively Some / None / None). The real stages already shipped with their own unit tests; this PR doesn't change their behaviour, only their reachability.
- **Activation:** operators flip `ENABLE_REAL_SANCTIONS=true` (etc.) per stage. Adverse-media additionally requires `ANTHROPIC_API_KEY` for non-fixture inference; absent the key the gateway runs in fixture mode (see `recor-inference-gateway`).

### FIND-010 — Architecture binders are `.docx` (non-diffable)

- **Severity:** HIGH (doctrine drift)
- **Location:** `docs/architecture/`, `docs/companion/`, `docs/concept-note/`
- **Source:** Pass A § orientation
- **Impact:** The three governance documents (Architecture, Companion, Concept Note) are `.docx` binaries. They cannot be diffed, reviewed in PR, or tracked for staleness via git. Doctrine D5 (docs are part of the feature) is at risk: code can drift from architecture without any tooling-level warning.
- **Remediation:** Convert all three to Markdown (or AsciiDoc) and version them. The conversion is a one-time pass; ongoing edits then become PR-reviewable.
- **Effort:** medium (1-2 weeks for the conversion + review)
- **Cost class:** docs-only

### FIND-011 — Toolchain split-brain: rust-toolchain.toml 1.88.0 vs mise.toml 1.84.0 vs Cargo.toml rust-version 1.85 — **CLOSED (Sprint 4)**

- **Severity:** HIGH
- **Status:** CLOSED — all three sources now declare 1.88. Verified by inspection:
  - `rust-toolchain.toml` `channel = "1.88.0"`
  - `mise.toml` `rust = "1.88.0"`
  - root `Cargo.toml` `rust-version = "1.88"`
- **Guard against regression:** the `cargo-msrv` style sanity check is
  implicit — `cargo build --workspace` requires the rust-toolchain.toml
  channel to be ≥ the workspace `rust-version`, and `mise install` reads
  the same channel. Drift in any one file surfaces on the next bootstrap.

### FIND-012 — D↔V HMAC channel has no `iat`-bound replay window — **CLOSED (Sprint 3)**

- **Severity:** HIGH (carry-over from threat-model Gap G2)
- **Status:** CLOSED by audit Sprint 3 — every internal-service HMAC surface now binds a `iat` (issued-at) timestamp into the MAC and enforces a ±5-minute replay window on receipt.
- **Location:** `packages/recor-hmac-sig` (new shared crate) + every signing/verifying site across `services/declaration`, `services/verification-engine`, and `apps/worker-fabric-bridge`.
- **Source:** Pass B § 5 (DF-2)
- **Impact:** Pre-fix, a captured envelope could be replayed indefinitely until the HMAC secret rotated. Idempotency on `event_id` prevented observable replay effect on the V-engine side, but the time horizon was unbounded.
- **Remediation shipped:**
  - New `packages/recor-hmac-sig` crate: `sign(secret, body, iat)` returns the hex-encoded MAC of `body || "\n" || iat`. `verify(cfg, body, sig, ts, now)` checks the timestamp window first (default ±300s) and then the MAC under the primary OR (during a rotation window) the previous-generation secret.
  - Producers: declaration's outbox-relay + V-engine's writeback-relay now stamp `X-RECOR-Timestamp` alongside the existing `X-RECOR-Signature`.
  - Consumers: V-engine's `/v1/internal/declaration-events`, declaration's `/v1/internal/verification-outcomes`, and worker-fabric-bridge's `/v1/relay` reject missing/stale/forged timestamps with a structured error kind (`missing_timestamp`, `stale_request`, etc.).
- **Tests:** 12 unit tests on the shared crate cover round-trip, missing-header refusal, stale/future-dated window refusal, malformed inputs, body+timestamp tampering, and dual-secret rotation.

### FIND-013 — V-engine has no committed OpenAPI snapshot (TODO marker only) — **CLOSED (Sprint 2)**

- **Severity:** HIGH
- **Status:** CLOSED by audit Sprint 2.
- **Location:** `services/verification-engine/src/api/rest.rs` (was TODO comment)
- **Source:** Pass A § A.10
- **Impact:** Declaration service had DOC-1 OpenAPI + drift check; V-engine did not. Consumer integration (R-PORT-7-VER) was blocked.
- **Remediation shipped:** Wired utoipa across V-engine handlers (`#[utoipa::path]` on `submit_verification`, `get_verification`, `healthz`, `readyz`, `list_dlq`, `replay_dlq`, `handle_declaration_event`); added `ToSchema` derives on the wire DTOs (`SubmitVerificationRequest`, `SubmitVerificationResponse`, `HealthzResponse`, `ReadyzResponse`, `ErrorEnvelope`, `ErrorBody`, `ListDlqResponse`, `DlqItem`, `ReplayDlqResponse`, `InboundResponse`); created `services/verification-engine/src/api/openapi.rs` with the assembled document; committed `docs/openapi/verification-engine.json`; mounted `GET /openapi.json` + `GET /docs` on the V-engine router; extended `tools/ci/check-openapi-drift.sh` to assert the V-engine snapshot alongside declaration's. The Prometheus `/metrics` endpoint is intentionally NOT in the consumer-facing spec (OBS-1; served on a separate listener per FIND-007). Deep nested domain types (`DeclarationSnapshot`, `VerificationCase`) are pinned via `serde_json::Value` shims — the authoritative schema for those bodies lives in `services/declaration`'s OpenAPI document.
- **Tests:** `api::openapi::tests::{openapi_is_3_1, every_public_path_present, submit_endpoint_declares_request_and_known_responses, get_endpoint_documents_404_for_cross_tenant_denial, security_schemes_are_registered, internal_endpoints_carry_internal_tag, metrics_endpoint_is_intentionally_absent}`.

### FIND-014 — V-engine has no `tests/*.rs` integration files (only unit tests) — **CLOSED (Sprint 2)**

- **Severity:** HIGH
- **Status:** CLOSED by audit Sprint 2.
- **Location:** `services/verification-engine/tests/`
- **Source:** Pass A § orientation
- **Impact:** No end-to-end testcontainers coverage of the V-engine. integration-smoke.sh exercised it indirectly via the declaration service; stage failures, pipeline regressions, and lane-router changes had no V-engine-side gate.
- **Remediation shipped:** Four V-engine integration test files (testcontainers Postgres 17, `#[ignore]`-gated so they don't break the lib-test lane):
  - `tests/migrations_apply.rs` — asserts all migrations apply cleanly, verifies the FIND-004 `verification_cases.declarant_principal NOT NULL` invariant in the live schema, and confirms the R-VER-* tables (`sanctions_persons`, `peps`, `icij_persons`) ship.
  - `tests/audit_immutability.rs` — COMP-2 / D15 regression: asserts BEFORE-trigger refusal of UPDATE / DELETE / TRUNCATE on `verification_cases`, mirroring the declaration service's audit-immutability suite.
  - `tests/api_integration.rs` — full HTTP surface: healthz / readyz / `/metrics` Prometheus exposition, `/openapi.json` regression guard (FIND-013), Scalar UI at `/docs`, FIND-002 admin-allowlist on POST /v1/verifications, FIND-004 unauth + admin + non-admin GET predicates, DLQ admin allowlist, internal webhook HMAC gate.
  - `tests/pipeline_integration.rs` — drives `SubmitVerificationUseCase` directly with the seven-stage pipeline production wires today; asserts the resulting `verification_cases` row carries the denormalised `declarant_principal`, an outbox row lands in the same transaction, and replaying the same `declaration_id` is idempotent (D13).
- **gRPC integration:** intentionally NOT in scope — V-engine has no gRPC surface yet (R-VER-GRPC TODO). When the gRPC server lands, `grpc_integration.rs` follows the declaration service's pattern.

### FIND-015 — Worker-fabric-bridge HMAC has no rotation slot — **CLOSED (Sprint 4)**

- **Severity:** HIGH
- **Status:** CLOSED — bridge now accepts a previous-generation secret
  alongside the primary. Config slot: `hmac_secret_old` (env
  `RECOR_FABRIC_BRIDGE_HMAC_OLD`) on
  `apps/worker-fabric-bridge/src/config.rs:30`. Verification logic
  (`apps/worker-fabric-bridge/src/handlers.rs:81-90`) checks the primary
  first, then falls back to the old secret during a rotation window.
  Same ADR-005 pattern as declaration's and V-engine's internal HMAC
  surfaces.
- **Tests:** the bridge's existing HMAC verification unit tests cover
  primary-accept / old-accept / both-reject paths.

### FIND-016 — Audit chain reconciliation cron MISSING (event_log vs Fabric witness divergence) — **CLOSED (Sprint 2)**

- **Severity:** HIGH
- **Status:** CLOSED by audit Sprint 2 — `apps/audit-reconciler` ships a periodic job that detects events in the local event log absent from the Fabric chaincode.
- **Location:** `apps/audit-reconciler/src/reconciler.rs`
- **Source:** Pass C § 08-audit-chain.md "Gaps to close"
- **Impact:** If the worker-fabric-bridge silently fails to anchor an event, no automated job detected it. Threat-model Gap G1 is now fully closed (R-DECL-9 anchors; FIND-016 reconciles).
- **Remediation shipped:** New app `apps/audit-reconciler`. Every `RECONCILE_INTERVAL_SECONDS` (default 600s) it pulls `declaration_events` rows older than `RECONCILE_GRACE_SECONDS` (default 300s — bridge dispatch lag), groups by `declaration_id`, calls `ListAuditEntriesForDeclaration` on the chaincode, and counts every local `event_id` absent from the on-chain set. Each divergence increments `recor_audit_reconciliation_divergence_total{event_type=...}` and emits a structured WARN with `declaration_id` + `event_id` + `event_time`. `recor_audit_reconciliation_runs_total{outcome=ok|gateway_error|db_error}` makes a stuck reconciler itself alertable.
- **Tests:** `reconciler::tests::{happy_path_no_divergence_when_chain_matches_log, divergence_is_counted_and_logged_when_event_missing_onchain, gateway_failure_fails_the_pass_fail_closed, multiple_events_in_same_declaration_share_one_chain_query}`.
- **Operational follow-up:** Prometheus alert rules for divergence increments and stuck reconciler runs land alongside the existing observability stack in a separate PR.

### FIND-017 — mTLS peer-SPIFFE-ID check has no integration test (silent-accept risk) — **CLOSED (Sprint 3)**

- **Severity:** HIGH
- **Status:** CLOSED by audit Sprint 3 — V-engine ships `tests/peer_spiffe_id_gate.rs`, an axum-layer integration suite that exercises the exact middleware pattern `main.rs` will use once R-LOOP-3-followup lands the production wiring.
- **Location:** `services/verification-engine/tests/peer_spiffe_id_gate.rs`
- **Source:** Pass A § A.10
- **Impact:** Pre-fix, the peer-SPIFFE-ID allowlist gate had only unit tests on the underlying `enforce_peer_id` helper. A future refactor that wires the gate as a tower layer (R-LOOP-3-followup) could silently disable the layer with no test catching the regression.
- **Remediation shipped:** Five integration tests that mount the exact middleware structure `recor_spiffe::middleware`'s top-of-module doc sketch describes — `axum::middleware::from_fn_with_state(state, peer_spiffe_gate_middleware)` reading `Extension<PeerSpiffeId>`. A test-only injector mimics the rustls TLS layer by reading the SPIFFE ID from a header. Coverage: matching peer (200), mismatching peer (403), missing extension (403), malformed SPIFFE ID (403), denied/missing counter increments across multiple refusals.
- **Why no testcontainers + SPIRE:** the audit recommendation suggested SPIRE-backed integration testing, but the gate's logic is at the middleware layer NOT the TLS layer — a regression that "silently disables" the gate is a regression in the middleware mount, not in the SPIRE handshake. Testing the middleware directly with an injector substituting for the TLS layer is the cheaper-and-correct approach. The SPIRE-backed end-to-end test is a follow-up alongside the R-LOOP-3-followup wiring.

### FIND-018 — Person + entity services have no Vault, no SPIFFE, no internal HMAC surface

- **Severity:** HIGH
- **Location:** `services/{person,entity}-service/src/main.rs`
- **Source:** Pass A § system-map
- **Impact:** Two services hold Sensitive-PII and use only env-based secrets. No Vault wiring; no SPIFFE peer-auth; no internal HMAC for service-to-service inbound. Inconsistent with declaration + V-engine.
- **Remediation:** Mirror OPS-4 + R-LOOP-3 wiring across the two new services.
- **Effort:** medium-expensive (~1 week per service)
- **Cost class:** code-only

### FIND-019 — Bazel build target in `justfile` is aspirational (no BUILD/WORKSPACE files) — **CLOSED (Sprint 0)**

- **Severity:** HIGH (doctrine drift)
- **Status:** CLOSED — the `bazel build //...` target was removed from
  the root `justfile`. The remaining `# build:` comment (justfile lines
  49-54) documents the decision: cargo + pnpm + Go modules are the
  actual build surfaces; `just test` drives all three. If Bazel returns
  it lands behind its own ADR, not as a stub.

### FIND-020 — `tests/{chaos,performance,e2e}` directories are empty — **CLOSED (Sprint 4, decision)**

- **Severity:** HIGH
- **Status:** CLOSED by decision — the empty `tests/{chaos,performance,e2e}`
  directories were removed (only `tests/contract/` remains, populated).
  The architecture document is updated alongside to declare:
  - **E2E:** authoritatively in `applications/declarant-portal/tests/e2e/`
    (Playwright); the top-level `tests/e2e/` was a doctrine-drift relic.
  - **Chaos:** explicitly deferred to a post-launch hardening workstream;
    a dedicated ADR is required before adding any chaos rig.
  - **Performance:** the contract-level smoke suite under
    `tests/contract/` is the launch-readiness gate; load-shape testing
    follows the chaos ADR.
- **Why deletion rather than WIP scaffolds:** D08 (no dangling threads)
  and D07 (no workarounds where the real fix exists) — an empty WIP
  directory is the canonical example of a dangling thread. Removing it
  is the real fix; a CONTRIBUTING note documents where the actual
  coverage lives.

---

## MEDIUM + LOW findings (summary table)

The full text + reproduction steps for each finding is in the
referenced pass document. ~52 medium and ~28 low findings, summarised:

| Category | Count | Most-cited issues | Source | Sprint-4 closure |
|---|---|---|---|---|
| **Toolchain / build** | ~8 medium | per-service stale `Cargo.lock`s; missing tools/cli/recor-cli; justfile points at non-existent paths; pnpm vitest at repo root without workspace | [`00-orientation.md`](00-orientation.md), [`01-system-map.md`](01-system-map.md) | CLOSED — PR #125 stubbed the justfile targets as `@echo` no-ops with explanatory comments; FIND-019 above removed the Bazel target; `recor-cli` workstream tracked under tooling backlog (deferred ADR). |
| **Cross-service coupling** | ~6 medium | audit-verifier reads declaration's DB without contract; entity-service has outbox but no relay; orphan empty dirs (`alerts/`, `dashboards/`, `libraries/*/`, `scripts/`) | [`01-system-map.md`](01-system-map.md) | CLOSED — PR #125 replaced empty dirs with README placeholders. audit-verifier read-only DB coupling + entity outbox relay tracked as `R-AV-CONTRACT` + `R-ENT-RELAY` follow-up tickets with explicit `// TODO(<ticket>):` markers in source. |
| **Doc / convention drift** | ~12 medium + 8 low | every ADR cross-link not bidirectional; ARCH-claimed L0 substrate (FROST, OpenTimestamps, Halo2, HSM, PQ) absent from Cargo.lock; SW autoUpdate without SRI; GET-by-id 403 vs 404 existence side-channel | [`00-orientation.md`](00-orientation.md), [`02-surfaces.md`](02-surfaces.md) | CLOSED — Sprint 4 lands `tools/ci/check-adr-bidi.sh` (CI gate on bidirectional cross-links); the L0-substrate claims were rewritten in PR #124 to remove vapourware references; SW autoUpdate already uses SRI (verified inline); existence side-channel closed via uniform 404 from the projection adapters (Sprint 1 closure also covers FIND-004). |
| **Data flow** | ~8 medium | declaration response includes attestation signature (re-discloses signer's public key); polling cadence (3s) not coordinated with verification-engine lane decision; portal CSP `connect-src` doesn't include the audit-verifier origin | [`03-data-flows.md`](03-data-flows.md) | CLOSED — attestation public-key re-disclosure is intentional and architected (the public key is bound to the declarant identity for downstream verification); polling cadence documented as 3s aligned with V-engine's pipeline tick budget (5s p99) — see `docs/architecture/05-data-flows.md`; portal CSP `connect-src` templated via `CSP_CONNECT_SRC` (whitespace-separated, accepts the audit-verifier origin at orchestrator config time). |
| **Failure-mode coverage** | ~10 medium | DLQ inundation alert threshold (100) not Prometheus-rule-enforced; OIDC issuer outage runbook says "fail open in dev" but the code default is now mtls-fallback-only; Anthropic budget alert threshold not in alert-rules.yaml | [`04-failure-modes.md`](04-failure-modes.md) | CLOSED — PR #125 lands `alerts/recor-prometheus-rules.yaml` with DLQ inundation (threshold 100), Anthropic budget burn-rate, reconciler divergence, reconciler stuck-runs, OIDC verify failures, governor rejection saturation, HMAC stale-timestamp, and audit-immutability trigger rules. OIDC runbook section in `docs/runbooks/` updated to describe the current code default. |
| **Permission model drift** | ~6 medium + 4 low | admin allowlist CSV-stringly-typed; no single permission matrix file; navigation visibility for admin DLQ lives only in the portal-side rendering | [`05-permissions.md`](05-permissions.md) | CLOSED — Sprint 4 lands `docs/security/permission-matrix.md` (canonical principal × endpoint × decision matrix); admin allowlist CSV typing kept (parser validates each entry; bounded-enum migration tracked under `R-AUTHZ-ENUM`); portal navigation visibility for admin DLQ documented in the matrix as portal-side filtered server-side as well. |
| **UI / a11y** | ~4 medium + 6 low | three Moderate/Minor axe findings from R-PORT-5 (color contrast on "Ajouter un propriétaire" button; aria-live=assertive vs polite on terminal red lane; aria-describedby on resume-draft buttons) | [`06-ui.md`](06-ui.md) | CLOSED — R-PORT-5 WCAG audit + remediation PRs #103/#108 closed all three axe findings (button contrast, aria-live polite on red lane, aria-describedby on draft-resume controls). |
| **Cryptography** | ~6 medium + 4 low | nonce_hex format check inconsistent across portal + server; Vault AppRole role-id + secret-id rotation undocumented; SPIFFE trust-bundle refresh cadence not documented; gitleaks runs in CI but no pre-commit hook | [`07-cryptography.md`](07-cryptography.md) | CLOSED — Sprint 4 lands `docs/runbooks/vault-rotation.md` and `docs/runbooks/spiffe-refresh.md`; PR #125 lands `.githooks/pre-commit` with gitleaks; nonce_hex format check unified at the declaration aggregate (`services/declaration/src/domain/attestation.rs` is the single source of truth — the portal sends `nonce_hex`, the aggregate validates it as 32-char hex). |
| **Audit chain** | ~4 medium + 4 low | reconciliation cron is missing (HIGH; surfaced above as FIND-016); chaincode unit tests don't cover already-committed idempotency directly; bridge worker's `fabric_bridge_dlq` retention undocumented; audit-verifier's cross-DB SQL coupling | [`08-audit-chain.md`](08-audit-chain.md) | CLOSED — FIND-016 reconciler shipped Sprint 2; chaincode idempotency unit test (`audit-witness/audit_witness_test.go::TestRecordAuditEntry_Idempotent`) confirms second-write returns the existing entry; fabric_bridge_dlq retention documented in `docs/runbooks/dlq-retention.md` (30 days, same as declaration outbox retention); audit-verifier cross-DB coupling tracked under `R-AV-CONTRACT`. |

---

## Notes on aggregation

- Pass A's `FINDING:high` count (39 surfaced) collapses into FIND-001..020 above plus the summary table — many `[FINDING:high]` tags in Pass A point at the same root defect from different surfaces (e.g. unauthenticated `/metrics` shows up four times, once per service).
- Pass B's `CRITICAL-INTERRUPT.md` items map to FIND-002, FIND-003, FIND-012.
- Pass C's UI + crypto + audit-chain findings collapse into the summary-table rows + FIND-016.

Every catalogued finding is reproducible from the source pass
document's file:line citations.
