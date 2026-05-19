# Ranked findings catalogue — RÉCOR forensic audit

This document aggregates every finding from Passes A, B, and C of
the audit ([`00-orientation.md`](00-orientation.md) through
[`09-stress-test.md`](09-stress-test.md)), assigns a stable
`FIND-NNN` identifier, ranks by severity, and orders within each
severity by `cheap → expensive` so the architect reads the
cheapest critical fixes first.

**Counts:** 6 critical · 14 high · ~52 medium · ~28 low. The
critical / high tier is exhaustively enumerated below. Medium and
low findings appear in a compact table pointing to the source pass
document.

**Calibration.** A finding is **critical** if it permits an
unauthorised actor to read, write, or impersonate at scale, OR if
it leaves the system unable to detect tampering. **High** is a
material risk requiring remediation before any external review or
production deployment. **Medium** is worth fixing in normal course.
**Low** is cosmetic or has a documented compensating control.

---

## CRITICAL findings

### FIND-001 — Audit verifier is unauthenticated and discloses full declaration payloads by UUID

- **Severity:** CRITICAL
- **Location:** `apps/audit-verifier/src/` — the route handler for `GET /v1/audit/verify/{declaration_id}`
- **Source:** Pass A surfaces walkthrough; `08-audit-chain.md` § "Critical observation"
- **Evidence:** Pass A `02-surfaces.md` § A.13 (audit-verifier)
- **Impact:** Anyone on the public internet can enumerate declaration UUIDs and retrieve the full canonical payload — declarant principal, beneficial-owner list, entity_id, ownership_basis_points, attestation. This is RÉCOR's primary PII surface and it is open.
- **Root cause:** The verifier was designed as a public verification surface for the Fabric audit channel. The design assumed the verifier only returned hash-equality booleans; the implementation returns the full re-derived payload so callers can recompute the BLAKE3 hash themselves. The implementation choice creates an enumeration oracle.
- **Remediation (option A, fastest):** OIDC-gate the verifier route exactly as the rest of the declaration service is gated; return only `{declaration_id, on_chain_hash, computed_hash, match: bool}` (drop the payload field).
- **Remediation (option B, more work):** keep the route public but return only `{on_chain_hash}` from chaincode and `{verification_method: "compute from your own copy of the canonical payload"}` documentation; the verifier becomes hash-only. Update audit-verification runbook accordingly.
- **Effort:** cheap (1-2 days for option A; 3-5 days for option B with the docs/UX changes)
- **Cost class:** code-only

### FIND-002 — `POST /v1/verifications` admits arbitrary snapshots from any authenticated declarant

- **Severity:** CRITICAL
- **Location:** `services/verification-engine/src/api/rest.rs:228-257`
- **Source:** Pass B § 7 (`05-permissions.md` PRM-3); `CRITICAL-INTERRUPT.md`
- **Evidence:** the handler extracts `axum::Extension(_principal)` — the underscore is the language signal the value is intentionally unused
- **Impact:** Any registered declarant can submit arbitrary `DeclarationSnapshot` bodies, causing Anthropic API calls (paid) on Stage 5, polluting `verification_cases` with no corresponding real declaration, and potentially spoofing "this declaration was verified Green/Yellow/Red" if downstream code trusts the case record
- **Remediation:** Either gate the endpoint on admin-allowlist (consistent with DLQ admin), OR remove the endpoint entirely and require the D→V loop's HMAC-authenticated path (`/v1/internal/declaration-events`) to be the only verification entry point
- **Effort:** cheap (~1 day)
- **Cost class:** code-only

### FIND-003 — `ENVIRONMENT=dev` + configured OIDC accepts BOTH auth paths simultaneously

- **Severity:** CRITICAL
- **Location:** `services/declaration/src/config.rs:282-300` (mirror in V-engine)
- **Source:** Pass B § 6 + § 7 (`05-permissions.md` PRM-6 / `04-failure-modes.md` FM-11); `CRITICAL-INTERRUPT.md`
- **Evidence:** the config startup gate refuses to start only when `environment != "dev" AND oidc_issuer_url.is_empty()`. It does NOT refuse when `environment == "dev" AND oidc_issuer_url` is set
- **Impact:** A production deployment with a stray `ENVIRONMENT=dev` env var allows both auth paths. An attacker can bypass OIDC entirely by sending `X-Recor-Dev-Principal: any-victim-principal` — **complete authentication bypass with full impersonation**
- **Remediation:** Tighten startup validation: refuse when `environment == "dev"` AND `oidc_issuer_url` non-empty (production OIDC issuer + dev backdoor is incoherent). Add a regression integration test. Apply the same fix to V-engine, person-service, entity-service
- **Effort:** cheap (~1 day across the four services)
- **Cost class:** code-only

### FIND-004 — V-engine submit/get accept any authenticated principal — cross-tenant case read

- **Severity:** CRITICAL
- **Location:** `services/verification-engine/src/api/rest.rs` — `submit_verification` (~228-257) and `get_verification` (~260-280)
- **Source:** Pass A § A.10 (verification-engine surfaces)
- **Evidence:** both handlers use `axum::Extension(_principal)` with no authorisation check; any authenticated bearer can read any verification case by `case_id`
- **Impact:** Cross-tenant disclosure of fusion belief, lane, stage details, AND any PII the V-engine stamped onto the case. An authenticated declarant can read every other declarant's verification trajectory
- **Remediation:** Add a per-case `declarant_principal` column (or join through to `declaration_events`) and gate `get_verification` on `principal == case.declarant_principal OR principal IN admin_allowlist`. `submit_verification` is FIND-002 — fix together
- **Effort:** medium (3-5 days — needs a migration to denormalise the declarant_principal onto `verification_cases` for the gate)
- **Cost class:** code + migration

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

### FIND-007 — `/metrics` endpoint unauthenticated; `infrastructure/networks/` empty (no NetworkPolicy)

- **Severity:** HIGH
- **Location:** Every service's `metrics_handler` (`services/{declaration,verification-engine,person-service,entity-service}/src/metrics.rs`). Network protection in `infrastructure/networks/` — but the directory is empty
- **Source:** Pass A § A.9/A.10/A.11/A.12 + § system-map
- **Impact:** Operational fingerprints leak from outside the cluster: DLQ size, OIDC verify counters, governor rejection rates, per-stage latencies, Anthropic budget. Aids attackers in reconnaissance.
- **Remediation:** Land a NetworkPolicy in `infrastructure/networks/` restricting `/metrics` to the Prometheus scraper's pod CIDR. OR move metrics to a separate listener on a different port that isn't exposed via the public ingress.
- **Effort:** medium
- **Cost class:** requires-infrastructure

### FIND-008 — `infrastructure/{terraform,kubernetes,ansible,networks}/` and `policies/` are EMPTY

- **Severity:** HIGH
- **Location:** repo root
- **Source:** Pass A § system-map
- **Impact:** The system has no committed infrastructure-as-code. Production deployment requires those layers to exist. The README and ADRs reference Helm + ArgoCD but the manifests don't actually exist. **The system cannot be deployed to production as-is.**
- **Remediation:** Author the Helm charts + ArgoCD applications + Terraform for the cluster + OPA policies. This is a substantial pre-launch workstream.
- **Effort:** expensive (multiple weeks)
- **Cost class:** requires-infrastructure

### FIND-009 — 5 of 7 V-engine pipeline stages are stubs in production wiring; real implementations sit unreachable

- **Severity:** HIGH
- **Location:** `services/verification-engine/src/application/stages/mod.rs`
- **Source:** Pass A § system-map + § A.10
- **Impact:** The `mod.rs` registers `stage_3_sanctions_stub`, `stage_4_pep_stub`, `stage_5_adverse_media_stub`, `stage_6_pattern_detection_stub`, `stage_7_cross_source_stub` in the pipeline. The "real" implementations (`stage3_sanctions.rs`, `stage4_pep.rs`, etc.) ship the same crate but are NOT registered. The system runs with stubs in production today.
- **Remediation:** Update `stages/mod.rs` to register the real stages behind config switches (per the R-VER-1..6 design). Default behaviour can stay stub-based until ingestion + Anthropic key are in place.
- **Effort:** cheap (~1-2 days) for the registration; the real-data switches require partner data (sanctions feeds, OpenSanctions PEP, ICIJ licence, Anthropic API key — `requires-external-action`)
- **Cost class:** code + requires-external-action for full activation

### FIND-010 — Architecture binders are `.docx` (non-diffable)

- **Severity:** HIGH (doctrine drift)
- **Location:** `docs/architecture/`, `docs/companion/`, `docs/concept-note/`
- **Source:** Pass A § orientation
- **Impact:** The three governance documents (Architecture, Companion, Concept Note) are `.docx` binaries. They cannot be diffed, reviewed in PR, or tracked for staleness via git. Doctrine D5 (docs are part of the feature) is at risk: code can drift from architecture without any tooling-level warning.
- **Remediation:** Convert all three to Markdown (or AsciiDoc) and version them. The conversion is a one-time pass; ongoing edits then become PR-reviewable.
- **Effort:** medium (1-2 weeks for the conversion + review)
- **Cost class:** docs-only

### FIND-011 — Toolchain split-brain: rust-toolchain.toml 1.88.0 vs mise.toml 1.84.0 vs Cargo.toml rust-version 1.85

- **Severity:** HIGH
- **Location:** `rust-toolchain.toml`, `mise.toml`, root `Cargo.toml`
- **Source:** Pass A § orientation
- **Impact:** A developer following the mise workflow gets 1.84; the cargo build wants 1.85 min; the rust-toolchain.toml says 1.88. Three different opinions about what to use. Reproducibility risk; D19 violation.
- **Remediation:** Pick one (1.88.0 since the codebase already uses Edition 2024). Update all three to match.
- **Effort:** cheap (~30 minutes)
- **Cost class:** code-only

### FIND-012 — D↔V HMAC channel has no `iat`-bound replay window

- **Severity:** HIGH (carry-over from threat-model Gap G2)
- **Location:** `services/declaration/src/api/internal.rs::verify_hmac` + `services/verification-engine/src/api/internal.rs::verify_hmac`
- **Source:** Pass B § 5 (DF-2)
- **Impact:** A captured envelope can be replayed indefinitely until the HMAC secret rotates. Idempotency on `event_id` prevents observable replay effect on the V-engine side, but the limiter is unbounded.
- **Remediation:** Bind a `iat` (issued-at) timestamp into the HMAC payload + enforce a 5-minute clock-skew window on receipt. R-LOOP-2 (Kafka) carries this enforcement when transport-cuts-over; for the HTTP fallback transport, add the iat check now.
- **Effort:** medium (~2-3 days)
- **Cost class:** code-only

### FIND-013 — V-engine has no committed OpenAPI snapshot (TODO marker only)

- **Severity:** HIGH
- **Location:** `services/verification-engine/src/api/rest.rs` (TODO comment)
- **Source:** Pass A § A.10
- **Impact:** Declaration service has DOC-1 OpenAPI + drift check. V-engine does not. Consumer integration (R-PORT-7-VER) is blocked.
- **Remediation:** Mirror DOC-1's utoipa setup on V-engine; commit `docs/openapi/verification-engine.json`; wire a drift check.
- **Effort:** medium (~3-5 days)
- **Cost class:** code-only

### FIND-014 — V-engine has no `tests/*.rs` integration files (only unit tests)

- **Severity:** HIGH
- **Location:** `services/verification-engine/tests/`
- **Source:** Pass A § orientation
- **Impact:** No end-to-end testcontainers coverage of the V-engine. The integration-smoke.sh exercises it indirectly via the declaration service, but stage failures, pipeline regressions, and lane-router changes have no V-engine-side gate.
- **Remediation:** Author `services/verification-engine/tests/{api_integration,pipeline_integration,grpc_integration}.rs` mirroring the declaration test suite.
- **Effort:** medium-expensive (~5 days)
- **Cost class:** code-only

### FIND-015 — Worker-fabric-bridge HMAC has no rotation slot

- **Severity:** HIGH
- **Location:** `apps/worker-fabric-bridge/src/`
- **Source:** Pass A § A.13
- **Impact:** Every other HMAC-signed channel (D→V, V→D) has dual-secret rotation per ADR-005. The Fabric bridge inherits only the primary secret slot. A secret rotation requires the bridge to restart, breaking the audit anchoring during the rotation window.
- **Remediation:** Add a `FABRIC_BRIDGE_HMAC_SECRET_OLD` config slot following the ADR-005 pattern.
- **Effort:** cheap (~1 day)
- **Cost class:** code-only

### FIND-016 — Audit chain reconciliation cron MISSING (event_log vs Fabric witness divergence)

- **Severity:** HIGH
- **Location:** Does not exist
- **Source:** Pass C § 08-audit-chain.md "Gaps to close"
- **Impact:** If the worker-fabric-bridge silently fails to anchor an event, no automated job detects it. The threat-model marks Gap G1 as partially closed by R-DECL-9; full closure requires this reconciliation.
- **Remediation:** Author a cron-style job that joins `declaration_events` against the chaincode KV by `event_id` and alerts on events present in the event log but missing from chaincode for > N minutes (where N covers normal bridge lag).
- **Effort:** medium (~3 days)
- **Cost class:** code + requires-infrastructure (alert routing)

### FIND-017 — mTLS peer-SPIFFE-ID check has no integration test (silent-accept risk)

- **Severity:** HIGH
- **Location:** `services/declaration/src/main.rs` + `services/verification-engine/src/main.rs` — outer tower layer
- **Source:** Pass A § A.10
- **Impact:** R-LOOP-3 wires SPIFFE/mTLS but the peer-SPIFFE-ID allowlist gate runs in an outer tower layer that isn't covered by an assertion. A future refactor could silently disable the gate.
- **Remediation:** Add an integration test (testcontainers + SPIRE) that submits with a wrong-SPIFFE-ID peer and confirms 403.
- **Effort:** medium (~3 days)
- **Cost class:** code-only

### FIND-018 — Person + entity services have no Vault, no SPIFFE, no internal HMAC surface

- **Severity:** HIGH
- **Location:** `services/{person,entity}-service/src/main.rs`
- **Source:** Pass A § system-map
- **Impact:** Two services hold Sensitive-PII and use only env-based secrets. No Vault wiring; no SPIFFE peer-auth; no internal HMAC for service-to-service inbound. Inconsistent with declaration + V-engine.
- **Remediation:** Mirror OPS-4 + R-LOOP-3 wiring across the two new services.
- **Effort:** medium-expensive (~1 week per service)
- **Cost class:** code-only

### FIND-019 — Bazel build target in `justfile` is aspirational (no BUILD/WORKSPACE files)

- **Severity:** HIGH (doctrine drift)
- **Location:** `justfile`
- **Source:** Pass A § orientation
- **Impact:** `just build-with-bazel` exists but there are no `BUILD.bazel` or `WORKSPACE` files. Hidden expectation; cargo cult.
- **Remediation:** Either remove the Bazel target, OR commit the BUILD/WORKSPACE files. Pick one.
- **Effort:** cheap (~30 min to remove; weeks to commit)
- **Cost class:** code-only

### FIND-020 — `tests/{chaos,performance,e2e}` directories are empty

- **Severity:** HIGH
- **Location:** repo root
- **Source:** Pass A § orientation
- **Impact:** Three empty test categories the architecture promises. The Playwright suite under `applications/declarant-portal/tests/e2e/` is the actual E2E coverage; the top-level `tests/e2e/` is empty. Chaos + performance coverage is wholly missing.
- **Remediation:** Either delete the empty dirs OR commit the test scaffolds with WIP markers. Defer real coverage to a dedicated workstream.
- **Effort:** cheap (remove) + expensive (real coverage)
- **Cost class:** code-only

---

## MEDIUM + LOW findings (summary table)

The full text + reproduction steps for each finding is in the
referenced pass document. ~52 medium and ~28 low findings, summarised:

| Category | Count | Most-cited issues | Source |
|---|---|---|---|
| **Toolchain / build** | ~8 medium | per-service stale `Cargo.lock`s; missing tools/cli/recor-cli; justfile points at non-existent paths; pnpm vitest at repo root without workspace | [`00-orientation.md`](00-orientation.md), [`01-system-map.md`](01-system-map.md) |
| **Cross-service coupling** | ~6 medium | audit-verifier reads declaration's DB without contract; entity-service has outbox but no relay; orphan empty dirs (`alerts/`, `dashboards/`, `libraries/*/`, `scripts/`) | [`01-system-map.md`](01-system-map.md) |
| **Doc / convention drift** | ~12 medium + 8 low | every ADR cross-link not bidirectional; ARCH-claimed L0 substrate (FROST, OpenTimestamps, Halo2, HSM, PQ) absent from Cargo.lock; SW autoUpdate without SRI; GET-by-id 403 vs 404 existence side-channel | [`00-orientation.md`](00-orientation.md), [`02-surfaces.md`](02-surfaces.md) |
| **Data flow** | ~8 medium | declaration response includes attestation signature (re-discloses signer's public key); polling cadence (3s) not coordinated with verification-engine lane decision; portal CSP `connect-src` doesn't include the audit-verifier origin | [`03-data-flows.md`](03-data-flows.md) |
| **Failure-mode coverage** | ~10 medium | DLQ inundation alert threshold (100) not Prometheus-rule-enforced; OIDC issuer outage runbook says "fail open in dev" but the code default is now mtls-fallback-only; Anthropic budget alert threshold not in alert-rules.yaml | [`04-failure-modes.md`](04-failure-modes.md) |
| **Permission model drift** | ~6 medium + 4 low | admin allowlist CSV-stringly-typed; no single permission matrix file; navigation visibility for admin DLQ lives only in the portal-side rendering | [`05-permissions.md`](05-permissions.md) |
| **UI / a11y** | ~4 medium + 6 low | three Moderate/Minor axe findings from R-PORT-5 (color contrast on "Ajouter un propriétaire" button; aria-live=assertive vs polite on terminal red lane; aria-describedby on resume-draft buttons) | [`06-ui.md`](06-ui.md) |
| **Cryptography** | ~6 medium + 4 low | nonce_hex format check inconsistent across portal + server; Vault AppRole role-id + secret-id rotation undocumented; SPIFFE trust-bundle refresh cadence not documented; gitleaks runs in CI but no pre-commit hook | [`07-cryptography.md`](07-cryptography.md) |
| **Audit chain** | ~4 medium + 4 low | reconciliation cron is missing (HIGH; surfaced above as FIND-016); chaincode unit tests don't cover already-committed idempotency directly; bridge worker's `fabric_bridge_dlq` retention undocumented; audit-verifier's cross-DB SQL coupling | [`08-audit-chain.md`](08-audit-chain.md) |

---

## Notes on aggregation

- Pass A's `FINDING:high` count (39 surfaced) collapses into FIND-001..020 above plus the summary table — many `[FINDING:high]` tags in Pass A point at the same root defect from different surfaces (e.g. unauthenticated `/metrics` shows up four times, once per service).
- Pass B's `CRITICAL-INTERRUPT.md` items map to FIND-002, FIND-003, FIND-012.
- Pass C's UI + crypto + audit-chain findings collapse into the summary-table rows + FIND-016.

Every catalogued finding is reproducible from the source pass
document's file:line citations.
