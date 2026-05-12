# RÉCOR — Production TODO

The complete roadmap from "current state on main" to "deployable
sovereign-grade national beneficial-ownership registry." Each ticket
is scoped concretely enough that a specialist agent can pick it up
without further negotiation: scope, acceptance criteria, effort,
dependencies, external blockers, and a ready-to-paste delegation
brief are all present.

## How this document is used

1. The phases below are ordered by **what unblocks what**, not by
   importance. A Phase 0 item can be done today; a Phase 2 item
   typically depends on a Phase 0 or Phase 1 item being merged first.
2. Each ticket names an **agent role** (defined in
   `.claude/agents/`). When ready to implement, paste the brief into
   an `Agent` tool call with that `subagent_type`.
3. Tickets that need **external partner agreements** (BUNEC API,
   sanctions feeds, ICIJ data licence, Anthropic API key, Kafka
   cluster, Vault, etc.) are tagged `EXT:` so procurement can
   parallelise.
4. Every ticket carries explicit **acceptance criteria** aligned to
   the doctrines (D1–D24). A ticket isn't done if any criterion
   fails.

## Status legend

| | |
|---|---|
| ✅ | Done — merged into main |
| 🟡 | In progress |
| ⚪ | Not started, no blocker |
| 🔒 | Blocked on external dependency |

---

## Agent roles

Specialist agents defined in `.claude/agents/`. Pick the role from
the ticket's **Agent** field when delegating.

| Role | Use for |
|---|---|
| `rust-service-engineer` 🆕 | Rust service implementation: domain, use cases, infrastructure adapters, API handlers, migrations |
| `typescript-frontend-engineer` 🆕 | Portal: React/TS components, hooks, vitest tests, build/bundle work |
| `infrastructure-engineer` 🆕 | Docker, Helm, K8s, Vault, observability stack, CI/CD pipelines |
| `security-engineer` 🆕 | TLS, secrets, PII redaction, security headers, threat-model implementation (the existing `security-reviewer` *reviews*; this role *ships code*) |
| `integration-specialist` | External adapters (BUNEC, sanctions, PEP, adverse-media, ICIJ); Anthropic API integration |
| `test-author` | Playwright E2E, contract tests, DR drills, fuzz/property tests beyond what the implementing agent ships |
| `docs-author` | ADRs, operator runbooks, threat-model docs, regulatory mapping |

🆕 = new agent, defined in this PR alongside the existing roles (`integration-specialist`, `docs-author`, `test-author`, `security-reviewer`, `architect-reviewer`, etc.).

---

## Phase 0 — Bounded engineering (no external blocker)

Roughly **30 engineering-days** of work. Can be picked up in any
order; dependencies between tickets are called out per-ticket.

### Operational tooling

#### R-LOOP-DLQ-3 — Mirror DLQ admin endpoints on the V-engine ⚪
- **Why:** PR #59 shipped GET/POST `/v1/internal/outbox-dlq` on the declaration service. The V-engine has the same `verification_outbox_dlq` table with no admin surface.
- **Scope:**
  - Create `services/verification-engine/src/infrastructure/outbox_admin.rs` mirroring the declaration version against `verification_outbox` + `verification_outbox_dlq`
  - Create `services/verification-engine/src/api/dlq.rs` with the same handler shape + admin-principal gate
  - Add `ADMIN_PRINCIPALS` env to V-engine `Config`
  - Mount routes `/v1/internal/verification-outbox-dlq` (GET) + `/{id}/replay` (POST)
  - Extend `dlq-smoke.sh` to also exercise the V-engine endpoint OR write a dedicated `dlq-smoke-vengine.sh`
- **Acceptance criteria:**
  - 4 admin-auth unit tests pass (mirror declaration's)
  - Smoke: admin can list + replay V-engine DLQ rows; non-admin gets 403; missing id gets 404
  - No regression on existing smokes
- **Effort:** 3–4 hours
- **Dependencies:** none
- **External:** none
- **Agent:** `rust-service-engineer`
- **Brief:**
  > Closes R-LOOP-DLQ-3 — mirror the DLQ admin endpoints from `services/declaration` onto `services/verification-engine`. The declaration version is at `services/declaration/src/infrastructure/outbox_admin.rs` + `services/declaration/src/api/dlq.rs`, shipped in PR #59. Mirror the exact shape (list with limit/offset, atomic replay, admin-principal gate via new ADMIN_PRINCIPALS env). Tables are `verification_outbox` and `verification_outbox_dlq`; the V-engine outbox schema lacks `aggregate_type` and `headers` (smaller schema), adjust accordingly. Mount routes under `/v1/internal/verification-outbox-dlq`. Add the 4 enforce-admin unit tests. Extend `services/declaration/scripts/dlq-smoke.sh` to also exercise the new V-engine endpoints (the script already runs the full compose so adding curl calls against http://127.0.0.1:8088 is straightforward — actually no, the dlq-smoke uses port 8088 for declaration; the V-engine needs its own compose; write `services/verification-engine/scripts/dlq-smoke.sh` instead). Ship under one PR.

#### OPS-1 — Rate limiting on the public submit endpoint ⚪
- **Why:** Today `POST /v1/declarations` accepts unbounded volume per principal. DoS protection is a v1 requirement.
- **Scope:**
  - Add per-principal token-bucket rate limit (e.g., 60 requests / 60 seconds per `Principal::subject`) using tower-governor or a homebrew middleware
  - Configurable via env: `RATE_LIMIT_PER_MIN` (default 60), `RATE_LIMIT_BURST` (default 10)
  - On exhaustion: 429 with `Retry-After` header
  - Apply to: `POST /v1/declarations`, `POST /v1/declarations/{id}/supersede`. NOT to: GET endpoints (cached anyway), `/healthz`, `/readyz`, internal HMAC endpoints
  - Tests: unit test of the limiter, integration test that hammers the endpoint
- **Acceptance criteria:**
  - 11th request within 1s from same principal gets 429 (with default burst=10)
  - Different principals don't interfere
  - Retry-After header is present and correct
  - Healthz/readyz/internal endpoints unaffected
- **Effort:** 1 day
- **Dependencies:** none
- **External:** none
- **Agent:** `rust-service-engineer`
- **Brief:**
  > Add per-principal rate limiting to `POST /v1/declarations` and `POST /v1/declarations/{id}/supersede` in services/declaration. Use tower-governor (https://crates.io/crates/tower_governor) keyed by the authenticated principal subject. Configurable via RATE_LIMIT_PER_MIN (default 60) and RATE_LIMIT_BURST (default 10) envs. On exhaustion return 429 with Retry-After. Internal HMAC endpoints (/v1/internal/*), health/readyz, and GET endpoints are NOT rate-limited. Include unit tests for the limiter and an integration test that triggers a 429 by submitting burst+1 valid signed declarations from the same principal. Update the integration-smoke.sh to confirm the limiter doesn't trip during normal operation.

#### OPS-2 — PII redaction in tracing logs ⚪
- **Why:** `declarant_principal`, `person_id`, and BLAKE3 receipt hashes currently appear in structured logs via `tracing::instrument(fields(principal = %principal.subject, ...))`. GDPR + the OHADA data protection framework require these be redacted from operational logs.
- **Scope:**
  - Custom `tracing` layer that intercepts span field values and applies a redaction policy
  - Policy: hash principal subjects with BLAKE3-keyed-MAC (operators can correlate without seeing the value); replace person_id UUIDs with `person:<first 8 of BLAKE3>`; pass through public fields (event types, durations, status codes)
  - Configurable via `LOG_REDACTION` env (`enabled` | `disabled-for-dev`). Default `enabled` in non-dev environments
  - Apply to BOTH services + auth-oidc crate
  - Unit tests with the layer wrapping an in-memory subscriber, asserting redaction
- **Acceptance criteria:**
  - In production mode, `grep "spiffe://" <logs>` returns nothing
  - In dev mode (`LOG_REDACTION=disabled-for-dev`), values pass through unchanged so local debugging works
  - Tests assert specific field types redact correctly
- **Effort:** 2 days
- **Dependencies:** none
- **External:** none
- **Agent:** `security-engineer`
- **Brief:**
  > Add a PII-redacting tracing layer to both services + the recor-auth-oidc crate. The layer intercepts span field values during recording and rewrites values matching known PII shapes (SPIFFE URIs, UUIDs in person_id/principal context, hex receipt hashes) into stable redacted forms (BLAKE3-keyed-MAC of the original; key from env LOG_REDACTION_KEY). Configurable via LOG_REDACTION={enabled, disabled-for-dev}. Default to enabled; integration-smoke uses disabled-for-dev so test assertions don't have to deal with redaction. Add unit tests using a custom Subscriber that records intercepted spans and asserts the field values are redacted. The redaction key is generated at startup from LOG_REDACTION_KEY env (REQUIRED in non-dev environments, generated random in dev with a warning).

#### OPS-3 — Security headers on the portal 🟡
- **Why:** Today the portal nginx config serves the SPA without CSP, HSTS, X-Frame-Options, Permissions-Policy.
- **Scope:**
  - Update `applications/declarant-portal/nginx.conf` to set:
    - `Content-Security-Policy: default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; connect-src 'self' <api origin>; frame-ancestors 'none'; base-uri 'self'`
    - `Strict-Transport-Security: max-age=63072000; includeSubDomains; preload`
    - `X-Content-Type-Options: nosniff`
    - `X-Frame-Options: DENY`
    - `Permissions-Policy: geolocation=(), camera=(), microphone=(), payment=()`
    - `Referrer-Policy: strict-origin-when-cross-origin`
  - Document the CSP in `applications/declarant-portal/CLAUDE.md`
  - Add a Playwright check (or curl-based smoke) that asserts all headers are present on the production build's index.html
- **Acceptance criteria:**
  - `curl -I` against the built nginx image shows all required headers
  - CSP doesn't break the bundle (manual + Playwright)
  - Score on securityheaders.com (run locally via the equivalent tool) is A or A+
- **Effort:** 1 day
- **Dependencies:** none
- **External:** none
- **Agent:** `security-engineer`
- **Brief:**
  > Update applications/declarant-portal/nginx.conf to set production-grade security headers: CSP (script-src 'self', style-src 'self' 'unsafe-inline' for Tailwind, connect-src for the API origin, frame-ancestors 'none'), HSTS (max-age 2 years, preload-ready), X-Content-Type-Options, X-Frame-Options, Permissions-Policy disabling geolocation/camera/microphone/payment, Referrer-Policy strict-origin-when-cross-origin. The CONNECT-SRC for the API origin needs to be templated — accept VITE_DECLARATION_API_URL via nginx env interpolation at startup. Document the CSP rationale in applications/declarant-portal/CLAUDE.md. Add a smoke step (extend the existing dlq-smoke.sh or write a separate portal-headers-smoke.sh) that curls -I against the built portal and asserts each header is present.

### Domain completeness

#### R-DECL-3-AMEND — Amend declaration command ⚪
- **Why:** Supersede (PR #55) handles "replace this with a new one". Amend handles "fix a field in place on this same declaration" — for typos and minor corrections that shouldn't require a fresh aggregate.
- **Scope:**
  - New command `AmendDeclaration { declaration_id, amendments: AmendmentSet }`
  - `AmendmentSet` allows updating: beneficial_owners (with the 10_000 invariant), effective_from, declarant_role
  - Does NOT allow updating: entity_id (different entity = supersede), declarant_principal (principal binding), attestation (needs fresh signature on amendment)
  - New event `DeclarationAmendedV1 { declaration_id, before: AmendmentSet, after: AmendmentSet, amended_at, attestation }`
  - Aggregate rule: amend allowed only in state Submitted or InVerification. Accepted = use supersede instead (more transparency). Rejected = re-submit.
  - Each amendment carries a fresh attestation over the new canonical form
  - API: `POST /v1/declarations/{id}/amend` with the new attestation + amendments
  - Projection updates carry the new field values
  - 6+ unit tests + integration smoke section
- **Acceptance criteria:**
  - Aggregate refuses amend on Accepted/Rejected/Superseded states
  - Attestation must be signed over the AMENDED canonical form (not the original)
  - Beneficial-owner sum invariant still 10_000 after amend
  - Audit trail: replaying events reproduces both before and after
  - Integration smoke shows amend + GET reflects new values
- **Effort:** 4–5 days
- **Dependencies:** none (uses existing aggregate + repository pattern)
- **External:** none
- **Agent:** `rust-service-engineer`
- **Brief:**
  > Implement AmendDeclaration command in services/declaration. Same event-sourcing pattern as SupersedeDeclaration (see PR #55: services/declaration/src/application/supersede_declaration.rs and migration 0004). New event DeclarationAmendedV1 carries before+after snapshots of the amendable fields (beneficial_owners, effective_from, declarant_role). Aggregate rule: amend only allowed from Submitted or InVerification states. Each amendment requires a FRESH Ed25519 attestation over the new canonical form (so the API must canonicalise the AMENDED payload and verify against that, not the original). Projection updates the row in place (preserve declaration_id, change the field values). New endpoint POST /v1/declarations/{id}/amend. New migration 0006_add_amendment_columns.sql if any new columns are needed; otherwise project the latest values via the existing columns. 6+ unit tests covering: amend from Submitted OK, amend from Accepted refused (test directs operator to supersede), amend changes beneficial_owners maintaining the 10_000 invariant, amend attestation signed over wrong payload refused, replay-event reproduces both before+after, two amendments in sequence both apply. Integration smoke: extend existing smoke with an amend phase that demonstrates a field change reflected via GET.

#### R-DECL-3-CORRECT — Correct declaration command ⚪
- **Why:** Smaller-scope sibling of Amend. Some corrections (typo in declarant_principal display, person_id swap before verification) need a tighter window. Correct = "fix metadata, no canonical-form change, only allowed pre-verification."
- **Scope:**
  - `CorrectDeclaration { declaration_id, corrections: CorrectionSet }`
  - Allowed: display-only fields (NOT covered today, but hooks for future metadata fields)
  - Initial implementation: just typed plumbing for the command + event + endpoint, with a single supported correction (e.g., a `notes: String` metadata field added to the projection)
  - State rule: only state == Submitted; after the verification engine has touched it (any state other than Submitted), use Amend or Supersede
  - May ship in the same PR as R-DECL-3-AMEND
- **Acceptance criteria:**
  - As Amend but more restrictive
- **Effort:** 2–3 days (combined with Amend ≈ 1 week total)
- **Dependencies:** none, but ship in the same PR as Amend
- **External:** none
- **Agent:** `rust-service-engineer`
- **Brief:**
  > Implement CorrectDeclaration as a sibling of Amend. Same event-sourcing shape. Distinguish by the state machine: Correct only allowed from Submitted (before verification has touched the aggregate); Amend allowed from Submitted OR InVerification. Initial supported correction: a `metadata_notes: Option<String>` field on the projection (add a column via the same migration). Future tickets extend the CorrectionSet shape. Ship in the same PR as R-DECL-3-AMEND so the two commands' state-machine rules and tests reference each other.

#### R-DECL-7 — sqlx compile-time-checked queries ⚪
- **Why:** Today queries are runtime-checked. Schema/query drift catches only at first call (in integration smoke or in production). Compile-time `sqlx::query!` catches drift at `cargo build`.
- **Scope:**
  - Convert all `sqlx::query()` to `sqlx::query!()` or `sqlx::query_as!()` across both services
  - Set up CI to provision Postgres at build time, run migrations, generate `.sqlx/` cache via `cargo sqlx prepare --workspace`
  - Commit `.sqlx/` cache; set `SQLX_OFFLINE=true` for production builds (Dockerfile + CI)
  - Document the regeneration procedure in a runbook
- **Acceptance criteria:**
  - `SQLX_OFFLINE=true cargo build --workspace --release` succeeds without a live Postgres
  - Renaming a column in a migration without updating queries breaks `cargo build`
  - CI provisions Postgres, regenerates cache on every build, fails if cache is stale
- **Effort:** 3 days
- **Dependencies:** none
- **External:** none
- **Agent:** `rust-service-engineer`
- **Brief:**
  > Migrate every sqlx::query() call in services/declaration and services/verification-engine to compile-time-checked sqlx::query! / sqlx::query_as!. There are about 24 call sites in total — inventory them with `grep -rn "sqlx::query" services/`. Stand up a scratch Postgres (docker run postgres:17-alpine) at a known port for the prepare step. Apply both services' migrations to two separate databases (declaration migrations to declaration db, verification migrations to verification db). For each service, set DATABASE_URL and run `cargo sqlx prepare`. Commit the resulting .sqlx/ directories. Update both Dockerfiles to set `ENV SQLX_OFFLINE=true` so production builds don't need a live DB. Update .github/workflows/required-checks.yaml to spin up Postgres and regenerate .sqlx/ as a build step (or alternatively to verify the committed cache is current via `cargo sqlx prepare --check`). Write a brief runbook at docs/runbooks/sqlx-cache-regeneration.md.

#### R-DECL-8 — gRPC API alongside REST ⚪
- **Why:** REST is the right surface for the portal. Future service-to-service calls (between Person service, V-engine, declaration) should be gRPC + protobuf for type safety and performance.
- **Scope:**
  - Add a `contracts/` directory with proto definitions: `declaration.proto`, `verification.proto`, `person.proto` (skeleton)
  - tonic-based gRPC server in `services/declaration` exposing Submit, Get, Supersede, Amend
  - Same auth middleware (OIDC) adapted to gRPC interceptor
  - Reuse the existing use-case layer; gRPC handlers are thin adapters
  - Generated Rust stubs via `tonic-build`; future polyglot consumers can codegen too
  - Listen on a separate port (e.g., 9080) — coexists with REST
  - Integration test that submits via gRPC, verifies via REST GET
- **Acceptance criteria:**
  - `grpcurl` against the service returns a valid response
  - Submitting via gRPC and querying via REST returns the same data
  - Proto files are the source of truth (rust types derive from them in the gRPC layer; REST DTOs remain hand-written for now)
- **Effort:** 1 week
- **Dependencies:** none
- **External:** none
- **Agent:** `rust-service-engineer`
- **Brief:**
  > Add a gRPC surface alongside the existing REST API in services/declaration. Use tonic + tonic-build. Create a contracts/ directory at the repo root with declaration.proto defining: SubmitDeclaration, GetDeclaration, SupersedeDeclaration, AmendDeclaration (the gRPC analogues of the existing REST endpoints) plus shared messages for BeneficialOwner, Attestation, etc. Each gRPC handler is a thin adapter that calls the existing use-case (submit, get, supersede). Auth is the same OIDC verifier wrapped in a tonic interceptor; principal is set on the request extensions. Listen on a separate port from REST (9080 default, configurable via GRPC_BIND_ADDR). Generate Rust stubs via tonic-build in build.rs. Integration test: a small Rust test client uses the generated client to submit, then queries the REST GET endpoint and asserts the same data is returned. Update docker-compose.integration.yaml to expose 9080. Defer gRPC for V-engine to a follow-up.

### CI / supply chain

#### CI-1 — Publish container images on merge to main ⚪
- **Why:** Build artifacts are not published anywhere today; CI builds and discards. Production deploy needs versioned images at a registry.
- **Scope:**
  - GitHub Actions workflow that on push-to-main builds both services + portal images
  - Tag with `:latest` and `:${git_sha}`
  - Push to GitHub Container Registry (`ghcr.io/water-hacker/recor-{declaration,verification-engine,portal}`)
  - Cosign-sign images (keyless, OIDC-bound)
- **Acceptance criteria:**
  - After merging a PR to main, three images exist at ghcr.io with both tags
  - `cosign verify ghcr.io/.../recor-declaration:latest --certificate-identity ...` succeeds
- **Effort:** 1 day
- **Dependencies:** none
- **External:** GitHub Packages enabled (free for public repos)
- **Agent:** `infrastructure-engineer`
- **Brief:**
  > Add .github/workflows/publish-images.yaml triggered on push to main. Builds three images: services/declaration (workspace-root context, existing Dockerfile), services/verification-engine (same), applications/declarant-portal. Tags each :latest and :${{ github.sha }}. Pushes to ghcr.io/water-hacker/recor-{declaration,verification-engine,portal}. Uses docker/login-action with GITHUB_TOKEN (no separate secret needed). Adds keyless cosign signing via sigstore/cosign-installer + a cosign sign --yes step bound to the workflow's OIDC identity. Update a runbook docs/runbooks/image-verification.md explaining how on-call verifies an image's provenance with cosign before deploy.

#### CI-2 — Image vulnerability scanning + SBOM ⚪
- **Why:** Sovereign-grade supply chain requires SBOM + CVE scanning.
- **Scope:**
  - Trivy scan of each published image; fail the workflow if any CVE rated High or Critical with a fix available
  - Syft generates SBOM in SPDX format; attached as a build artifact + uploaded to ghcr.io as an attestation
  - cyclonedx as a second SBOM format (some auditors prefer CycloneDX)
- **Acceptance criteria:**
  - Workflow run produces three SBOM files per image (SPDX-JSON, CycloneDX-JSON, raw trivy report)
  - SBOM is signed via cosign attestation
  - A test commit that pulls in a known-vulnerable dep fails the workflow with a Trivy high-severity finding
- **Effort:** 1 day
- **Dependencies:** CI-1
- **External:** none
- **Agent:** `infrastructure-engineer`
- **Brief:**
  > Extend the image-publish workflow with Trivy scan + SBOM generation. After each image is built, run aquasecurity/trivy-action with severity=HIGH,CRITICAL and exit-code=1 (so a high/critical with fix breaks the build). Run anchore/sbom-action with both spdx-json and cyclonedx-json output formats. Attach SBOMs as workflow artifacts AND as cosign attestations on the registry image. Document the workflow in docs/runbooks/supply-chain.md including how to audit an SBOM and how to override the scan for a documented false positive (via .trivyignore at repo root).

#### CI-3 — Branch protection actually applied ⚪
- **Why:** R-001 documented branch protection rules but they're not active on the repo. The `tools/ci/apply-branch-protection.sh` script exists.
- **Scope:**
  - Run the existing script against main with the documented settings
  - Required status checks: every job in required-checks.yaml + pr-hygiene.yaml + observability-smoke.yaml
  - Require linear history, require PR review, no force-push to main
  - Document the settings in docs/security/branch-protection.md (already exists)
- **Acceptance criteria:**
  - Attempting `git push -f origin main` fails
  - Opening a PR shows the required checks; merging is blocked until all pass
  - Even an admin cannot bypass (set "Do not allow bypassing the above settings")
- **Effort:** 1 hour
- **Dependencies:** none
- **External:** repo admin permission
- **Agent:** `infrastructure-engineer`
- **Brief:**
  > Apply the branch protection rules documented in docs/security/branch-protection.md to the main branch via `tools/ci/apply-branch-protection.sh`. Required status checks: every job name listed in .github/workflows/required-checks.yaml + .github/workflows/pr-hygiene.yaml. Require linear history (no merge commits except the PR merge commit). Require pull-request reviews (1 reviewer minimum). Disallow force pushes. Disallow deletions. Disable allow-bypass-for-admins. After applying, verify by trying `git push -f origin main` from a clone (should fail with "remote: error: GH006") and by opening a draft PR (should show required checks). Document anything that required deviation from the runbook in a comment on the script.

### Documentation

#### DOC-1 — Auto-generated OpenAPI spec ⚪
- **Why:** Today the REST API is hand-coded. No OpenAPI spec means no automated client generation (R-PORT-7 is blocked), no auto-published API docs, no contract tests beyond what we hand-write.
- **Scope:**
  - Add `utoipa` annotations to every handler + DTO in services/declaration
  - Generate OpenAPI 3.1 JSON at build time
  - Expose at `GET /openapi.json` (public, no auth)
  - Add a Swagger UI / Scalar UI at `GET /docs`
  - Commit the generated spec to the repo so consumers can diff
- **Acceptance criteria:**
  - `curl http://localhost:8080/openapi.json | python -c "import json,sys; json.load(sys.stdin)"` succeeds
  - The spec describes every public endpoint, including all error response shapes
  - A test asserts that hand-written DTOs and the generated spec match for known fields
- **Effort:** 3 days
- **Dependencies:** none
- **External:** none
- **Agent:** `rust-service-engineer`
- **Brief:**
  > Generate OpenAPI 3.1 from the existing axum handlers in services/declaration using the `utoipa` crate. Annotate each handler with #[utoipa::path(...)] and each DTO with #[derive(ToSchema)]. Add the generated spec to a new endpoint GET /openapi.json (publicly accessible, no auth). Mount Scalar UI at GET /docs (uses utoipa-scalar; lighter than Swagger UI). Commit the spec to docs/openapi/declaration.json with a CI step that regenerates and fails if drift is detected. Defer V-engine's spec to a follow-up. Include the new endpoints in the integration smoke (just a curl that the spec parses as JSON).

#### DOC-2 — Five foundational ADRs ⚪
- **Why:** Big architectural decisions are buried in commit messages and CLAUDE.md. ADRs make them durable and reviewable.
- **Scope:** One ADR per:
  - ADR-001: Event sourcing for the Declaration aggregate (why event-sourced vs CRUD)
  - ADR-002: Dempster-Shafer over Bayesian for verification fusion
  - ADR-003: HTTP outbox-relay for D↔V (interim before Kafka) — what we accepted, what we deferred
  - ADR-004: OIDC + JWKS (vs in-house auth) for principal authentication
  - ADR-005: Per-channel HMAC secrets + dual-secret rotation
- **Acceptance criteria:**
  - Each ADR follows the MADR template (Context, Decision, Consequences, Alternatives considered)
  - 500–1500 words each
  - Linked from `docs/adr/README.md` (existing) as an index
- **Effort:** 3–5 days (300–600 words per ADR)
- **Dependencies:** none
- **External:** none
- **Agent:** `docs-author`
- **Brief:**
  > Write five ADRs documenting the foundational architectural decisions already made and shipped. Use the MADR template (https://adr.github.io/madr/). For each ADR: Context (the problem at the time of decision; pull from the relevant CLAUDE.md and commit messages), Decision (what we chose), Consequences (positive and negative), Alternatives Considered (with brief rationale for rejection). The five: ADR-001 Event Sourcing for Declarations (pull context from services/declaration/CLAUDE.md and commit cbcc251); ADR-002 Dempster-Shafer Fusion (pull from services/verification-engine/src/domain/fusion.rs doc comments + the V3 P14 architecture); ADR-003 HTTP Outbox-Relay (PR #38 + PR #39 commit messages; explicitly document why we chose this over Kafka in the interim and the migration plan to R-LOOP-2); ADR-004 OIDC + JWKS Auth (PR #45 + PR #51 commits); ADR-005 Per-Channel HMAC Rotation (PR #58 + docs/runbooks/hmac-secret-rotation.md). Index them in docs/adr/README.md.

#### DOC-3 — Ten operator runbooks ⚪
- **Why:** Today we have two runbooks (observability-dev-stack, hmac-secret-rotation). Production deployment needs ~10 more to be operable by oncall who didn't build the system.
- **Scope:** One runbook per:
  - Deploy a new version (CI/CD walkthrough)
  - Roll back a deployment
  - Restore a database from backup
  - Handle DLQ inundation (>100 rows accumulate in a short window)
  - Recover from OIDC issuer outage (fail-open vs fail-closed decision tree)
  - BUNEC adapter outage handling
  - HMAC secret rotation (already exists; cross-link)
  - sqlx cache regeneration (DOC paired with R-DECL-7)
  - On-call triage tree (start here)
  - Incident response template (post-mortem format)
- **Acceptance criteria:**
  - Each runbook is testable: an engineer who has never seen the system can follow it
  - Procedures use exact commands (no "consult the deployment guide" hand-waves)
  - "Verification" section at end of each: how to confirm the procedure succeeded
- **Effort:** 1 week
- **Dependencies:** DOC-2 (some runbooks reference ADRs)
- **External:** none
- **Agent:** `docs-author`
- **Brief:**
  > Write ten operator runbooks in docs/runbooks/. Each follows the structure: Trigger (when to run this), Prerequisites (what access/tools needed), Procedure (numbered steps with exact commands), Verification (how to confirm success), Rollback (if the procedure failed midway). Topics: deploy-new-version, rollback-deployment, restore-database-from-backup, dlq-inundation, oidc-issuer-outage, bunec-adapter-outage, oncall-triage-tree, incident-response-template, plus updates to two existing runbooks (hmac-secret-rotation - cross-link the others; observability-dev-stack - add a "production" companion). Procedures must use exact commands (gh cli, docker compose, psql, kubectl when relevant). For procedures that touch production state, include a "dry-run first" step and a verification command. Cross-link related runbooks at the bottom of each.

#### DOC-4 — Threat model ⚪
- **Why:** Sovereign-grade requires a documented adversary catalogue + mitigations.
- **Scope:** STRIDE per component:
  - Declarant portal (browser-side crypto)
  - Declaration service (event log + projections + outbox)
  - Verification engine (pipeline + mock BUNEC)
  - D↔V loop (HMAC-signed HTTP today; Kafka future)
  - Auth (OIDC + dev header)
  - Database (Postgres with sensitive data)
  - Operator surface (DLQ admin endpoints)
- **For each component:** spoofing / tampering / repudiation / information-disclosure / denial-of-service / elevation-of-privilege, with current mitigations and gaps
- **Acceptance criteria:**
  - Document covers all components above
  - Every threat has either an existing mitigation OR an explicit accepted-risk with rationale
  - Output is a markdown doc at `docs/security/threat-model.md` linked from the security README
- **Effort:** 1 week
- **Dependencies:** DOC-2 (references several ADRs)
- **External:** ideally a security reviewer who didn't write the code
- **Agent:** `security-engineer`
- **Brief:**
  > Write a STRIDE threat model for RÉCOR at docs/security/threat-model.md. Cover seven components: declarant portal, declaration service, verification engine, D↔V loop, auth, database, operator surface (DLQ admin endpoints + future operator UIs). For each component, walk through Spoofing/Tampering/Repudiation/InformationDisclosure/DenialOfService/ElevationOfPrivilege threats. For each threat, document current mitigation (with code references where applicable) OR explicitly mark accepted-risk with rationale. Pull mitigations from the relevant CLAUDE.md docs, runbooks, and code. Include a "gaps" section listing threats with no current mitigation that block production. Link from docs/security/README.md (existing or create). 1500–3000 words total.

---

## Phase 1 — External-dependency work (in parallel with procurement)

These tickets need external partner agreements to fully ship, but
the **interface/adapter skeleton** can land now so the integration
is plug-and-play once the partner agreement closes.

### Verification stages

#### R-VER-1 — Real BUNEC adapter 🔒
- **Why:** Today `PostgresMockBunec` is the identity-authentication backend. Real Cameroon business register integration is the V1 architectural commitment.
- **Scope:**
  - `BunecAdapter` trait already exists in code; concrete `RealBunecAdapter` impl that talks to BUNEC's HTTP API (SOAP if that's what they use)
  - Connection config: BUNEC_API_BASE_URL, BUNEC_API_KEY (or mTLS cert), timeout
  - Retry + circuit-breaker via existing tower middleware
  - Telemetry: histogram on call latency, counter on errors
  - Fallback: if BUNEC is down for > N seconds, the stage emits an "insufficient evidence" BPA rather than rejecting (configurable; gov may require fail-closed)
  - Integration test against a recorded BUNEC response fixture (using wiremock or similar)
- **Acceptance criteria:**
  - With BUNEC reachable: declaration with a known person → Stage 2 contributes meaningful BPA → fusion lands accurate result
  - With BUNEC down: stage emits insufficient-evidence; case_id still records the failure with stage_outcome.evidence
  - p99 BUNEC call latency: < 1s (configurable circuit breaker tied to this)
- **Effort:** 2 weeks (1 week interface + retry/CB, 1 week tuning against real API)
- **Dependencies:** none in code; **EXT: BUNEC API access agreement + sandbox credentials**
- **External:** gov-to-gov data agreement; sandbox API for testing
- **Agent:** `integration-specialist`
- **Brief:**
  > Implement a RealBunecAdapter alongside the existing PostgresMockBunec. The BunecAdapter trait already exists in services/verification-engine/src/application/port.rs. Add HTTP/SOAP client config (BUNEC_API_BASE_URL, BUNEC_API_KEY or mTLS — confirm with the partner spec). Wrap calls in reqwest-retry (3 attempts, exponential backoff) and a tower CircuitBreaker (open after 5 consecutive failures, half-open after 30s). Emit tracing spans + Prometheus counters (recor_bunec_calls_total, recor_bunec_call_latency_seconds). Fallback policy: on circuit-open, emit a BasicProbabilityAssignment::vacuous() with stage_outcome.evidence_summary = "bunec circuit open at <timestamp>" so the case still resolves (configurable via BUNEC_FAIL_POLICY=fail_closed | fail_open; default fail_open in dev, fail_closed in prod). Integration test: use wiremock to record a representative BUNEC response, replay it, assert the BPA. Until the real API access lands, leave the trait wired so the actual switch is a config change (BUNEC_BACKEND=mock | real).

#### R-VER-2 — Sanctions screening (Stage 3) 🔒
- **Why:** Stage 3 is a stub returning vacuous BPA. Real sanctions screening compares declarant + beneficial owners against OFAC SDN, UN consolidated, and EU CFSP lists.
- **Scope:**
  - Ingestion pipeline that fetches the three feeds nightly (OFAC publishes daily delta, UN publishes weekly, EU publishes weekly)
  - Storage: a sanctions index in Postgres (or Elasticsearch for fuzzy matching)
  - `SanctionsAdapter` trait + impl: given (full_name, nationality, optional DOB), returns match candidates with confidence scores
  - Fuzzy matching: phonetic + Levenshtein for transliterated names (Arabic/French/English variants)
  - The stage's BPA computation: certain match → BPA(0.05, 0.85, 0.10); high-confidence near-match → BPA(0.4, 0.4, 0.2); no match → BPA(0.0, 0.0, 1.0) (no evidence; vacuous)
  - Operator UI to review near-matches (deferred to a separate ticket)
- **Acceptance criteria:**
  - Submission with a known SDN-listed name (use a test fixture) → Stage 3 emits high-mass-on-False BPA → lane = red
  - Submission with a random name → Stage 3 emits vacuous BPA → lane unchanged from other stages
  - Daily ingestion job updates the index; CI test asserts the job parses the published feed format correctly
- **Effort:** 3 weeks
- **Dependencies:** R-VER-1 not strictly required but parallel
- **External:** OFAC SDN is free public; UN consolidated free; EU CFSP free. **No partner cost**, just engineering time + a server.
- **Agent:** `integration-specialist`
- **Brief:**
  > Implement Stage 3 sanctions screening. Build an ingestion pipeline (separate Rust binary or cronjob) that nightly fetches OFAC SDN (https://www.treasury.gov/ofac/downloads/sdn.xml), UN consolidated (https://scsanctions.un.org/resources/xml/en/consolidated.xml), and EU CFSP (https://webgate.ec.europa.eu/fsd/fsf/public/files/xmlFullSanctionsList_1_1/content?token=...). Parse the XML; normalise into a sanctions_persons table with columns (id, source, full_name_canonical, full_name_aliases jsonb, nationality, date_of_birth, sanction_program, list_entry_date). Add phonetic + Levenshtein indexes (pg_trgm + soundex; or use Elasticsearch if available). Implement a SanctionsAdapter trait in services/verification-engine with two methods: nightly_refresh and screen(person). The stage adapter consults all three lists and returns a BPA per the scoring rules in the ticket. Unit tests with fixture SDN entries: certain match → high-False-mass; random name → vacuous. Integration test: seed the test DB with one SDN entry, submit a declaration naming that person, assert lane=red.

#### R-VER-3 — PEP screening (Stage 4) 🔒
- **Why:** Stage 4 is a stub. PEP (Politically Exposed Person) screening compares against domestic + international PEP databases.
- **Scope:** Same shape as R-VER-2 but for PEP data
  - Domestic register: needs Cameroon-specific source (sovereign agreement); fallback: open-source PEP datasets (e.g., OpenSanctions PEP list)
  - Commercial backup: Refinitiv / Worldcheck / Sayari (any of the three)
  - Same fuzzy-matching infrastructure as Stage 3
  - Distinguish "PEP" (domestic political exposure) from "associate of PEP" — emits different BPAs
- **Effort:** 3 weeks (much can leverage R-VER-2's matching infrastructure)
- **Dependencies:** R-VER-2's matching/index infrastructure
- **External:** Cameroon PEP register agreement; OR a commercial PEP feed subscription; OR a defensible open-source policy
- **Agent:** `integration-specialist`
- **Brief:**
  > Implement Stage 4 PEP screening as a sibling of Stage 3 (R-VER-2). Reuse the fuzzy-matching infrastructure (pg_trgm + soundex; or Elasticsearch). For data sources: phase 1 uses OpenSanctions' PEP dataset (open data, MIT licence — https://www.opensanctions.org/datasets/peps/); phase 2 (commercial backup) integrates Refinitiv/Worldcheck via their API once a subscription is in place. Schema: peps table (id, source, full_name, position, country, start_date, end_date, is_current). Stage's BPA: confirmed PEP → BPA(0.2, 0.5, 0.3); associate of PEP → BPA(0.3, 0.3, 0.4); no match → vacuous. Coordinate with R-VER-2 on the shared matching code — the adapter for both stages should consult a single `name_match(person)` helper that returns scored candidates from BOTH the sanctions_persons and peps tables.

#### R-VER-4 — Adverse media + ICIJ (Stage 5) 🔒
- **Why:** Stage 5 is a stub. D22 commits to Anthropic-primary inference for adverse-media reasoning.
- **Scope:**
  - Recor Inference Gateway: a service abstraction over Anthropic's API (Tier A reasoning) + a fallback model
  - Adverse-media retrieval: index of ICIJ Offshore Leaks Database + (optionally) a news index
  - For each declarant + beneficial owner: prompt the Inference Gateway with the person + entity context + retrieved snippets, ask for an adverse-media verdict with reasoning
  - Stage's BPA: weighted by the model's expressed confidence + retrieval recall
- **Effort:** 4 weeks
- **Dependencies:** R-VER-2/3 matching infrastructure
- **External:** Anthropic API key + budget; ICIJ data licence (free for research/journalism; needs gov-research framing)
- **Agent:** `integration-specialist`
- **Brief:**
  > Implement Stage 5 adverse-media screening with Anthropic Claude as the reasoning agent. Build packages/recor-inference-gateway (new shared crate at the workspace root) abstracting Anthropic API calls: typed request/response, retry, budget tracking (token usage per case), prompt templating, model-version pinning. Ingest the ICIJ Offshore Leaks Database (https://offshoreleaks.icij.org/pages/database) into a Postgres table. For each beneficial owner in a declaration: retrieve top-5 leak hits via name match; pass owner name + entity context + retrieved hits to Claude as a structured prompt asking for adverse-media verdict + confidence + cited evidence. Parse the structured response (use Anthropic's tool-use feature to force schema). Map confidence to a BPA. Test fixtures: fixture with a person matching a known ICIJ entry → high-False-mass BPA; fixture with a random person → vacuous. Anthropic key is read from ANTHROPIC_API_KEY env (required outside dev; in dev, falls back to a fixture-based mock so tests don't need network).

#### R-VER-5 — Pattern detection (Stage 6) 🔒
- **Why:** Stage 6 is a stub. Real pattern detection runs eight signature classes over the entity-ownership graph (shell companies, circular ownership, beneficial-owner-of-many-entities, etc.).
- **Scope:**
  - A graph store (Neo4j OR pgrouting depending on infra constraints)
  - Eight signature implementations (the architecture spec lists them — circular ownership, common-owner pattern, BO-of-shell, etc.)
  - Each signature returns a per-aggregate BPA contribution
- **Effort:** 3 weeks
- **Dependencies:** R-VER-1 (entity data must be real)
- **External:** Neo4j cluster OR commitment to pgrouting
- **Agent:** `integration-specialist`
- **Brief:**
  > Implement Stage 6 pattern detection. Stand up Neo4j (or pgrouting if you want to avoid a new datastore) populated from the declaration projections. Implement 8 signature classes (the architecture v4 p14 doc spells them out — pull from docs/architecture/): circular ownership, common-owner-pattern, BO-of-shell-company, layered-ownership-deep-stack, BO-with-no-prior-history, sudden-ownership-change, opaque-jurisdiction-route, sanctions-adjacent-cluster. Each runs as a graph query returning matched aggregates + a confidence score; the score maps to a per-aggregate BPA contribution. Stage's combined BPA: Dempster-Shafer combination of the 8 individual contributions. Test fixtures for each signature.

#### R-VER-6 — Cross-source triangulation (Stage 7) 🔒
- **Why:** Stage 7 fuses outputs of Stages 3-6 with the declaration self-claim to detect inconsistencies (e.g., self-claim says A owns X, but pattern detection found A is BO of 50 entities).
- **Scope:**
  - Pure inference (no external data — operates on outputs of upstream stages)
  - Logic: pairwise consistency checks between (self-claim, identity, sanctions, PEP, adverse-media, pattern)
  - Emits a BPA reflecting cross-source agreement / disagreement
- **Effort:** 1 week (after V2-V5 are real)
- **Dependencies:** R-VER-2 + R-VER-3 + R-VER-4 + R-VER-5 (otherwise it has nothing to triangulate)
- **External:** none
- **Agent:** `integration-specialist`
- **Brief:**
  > Implement Stage 7 cross-source triangulation. This stage takes no external data; it consumes the outputs of stages 1-6 (which are already passed to the orchestrator) and emits a BPA reflecting cross-source agreement. Logic: for each pair of upstream stages, check whether their conclusions are consistent (e.g., Stage 2 says "person A exists in BUNEC" + Stage 5 says "person A is in ICIJ leaks" → consistent on existence; Stage 4 says "person A is PEP" + self-claim says "person A is a private investor" → inconsistent on role). Each pairwise check contributes to the cross-source BPA via Dempster's rule. Test fixtures: build cases where every pair agrees (high True-mass); where one pair sharply disagrees (high False-mass); where everything is vacuous (no signal → vacuous).

### Identity foundations

#### R-DECL-4 — Person service 🔒
- **Why:** Beneficial owners reference `person_id` UUIDs that aren't anchored to anything. A Person service is the canonical natural-person registry.
- **Scope:**
  - New service `services/person-service` with its own DB
  - Schema: persons table (id, canonical_full_name, nationality, date_of_birth, ID document references, biometric reference hash)
  - REST endpoints: create, get, search, link-to-NDI (Cameroonian national ID system)
  - Declaration service's submit handler now validates that person_ids reference Person service entries
  - Multi-service deployment (now three Rust services + one TS app)
- **Effort:** 4 weeks
- **Dependencies:** none architecturally; everything else is cleaner once this exists
- **External:** NDI integration (Cameroonian national ID) requires gov agreement
- **Agent:** `rust-service-engineer`
- **Brief:**
  > Build services/person-service as a new workspace member. Postgres-backed, event-sourced, axum REST, same patterns as services/declaration. Schema: persons table (id UUID PK, canonical_full_name TEXT, nationality TEXT CHAR(2), date_of_birth DATE, primary_id_document JSONB {issuer, type, number, expiry}, biometric_reference_hash BYTEA optional, created_at TIMESTAMPTZ). Events: PersonRegistered, PersonUpdated, PersonMerged (when duplicates are reconciled). REST endpoints: POST /v1/persons (create), GET /v1/persons/{id}, GET /v1/persons/search?q= (fuzzy by name + nationality), POST /v1/persons/{id}/merge-into/{target_id} (admin only, audited). Auth: OIDC (uses shared recor-auth-oidc). Outbox: emits PersonRegisteredV1 events for downstream consumers (declaration service will eventually subscribe to invalidate cached projections). Declaration service's SubmitDeclaration command in its own crate gains a step: validate that each beneficial_owner.person_id resolves to a real Person via PERSON_SERVICE_URL config. Failure surfaces as a domain error BeneficialOwnerNotInPersonRegistry. Defer NDI integration to a separate ticket once gov agreement closes.

#### IDENTITY-1 — Entity service 🔒
- **Why:** Same as R-DECL-4 but for legal entities. Today `entity_id` is whatever UUID the declarant invents.
- **Scope:** Same shape as R-DECL-4 but for entities (companies, partnerships, trusts)
- **Effort:** 4 weeks
- **Dependencies:** ideally after R-DECL-4 so we reuse the patterns
- **External:** ties to BUNEC eventually (entities ARE the BUNEC business register entries)
- **Agent:** `rust-service-engineer`
- **Brief:**
  > Build services/entity-service mirroring R-DECL-4's Person service. Entities have a different shape: (id, canonical_name, entity_type {sa, sarl, partnership, etc.}, jurisdiction, registration_number_in_jurisdiction, founded_at, dissolved_at optional). Once BUNEC adapter exists (R-VER-1), this service becomes the authoritative cache + projection of BUNEC entries for Cameroon entities; for non-Cameroonian entities it holds declarant-submitted data verified through the verification engine.

---

## Phase 2 — Infrastructure migration

### Transport + auth

#### R-LOOP-2 — Kafka transport 🔒
- **Why:** HTTP outbox-relay works but doesn't scale to thousands of declarations/hour. Kafka is the target architecture for v1.
- **Scope:**
  - Single-broker Kafka cluster in `infrastructure/kafka/` (single-node OK for v1; replicate for prod)
  - Replace declaration's HTTP relay with a Kafka producer
  - Replace V-engine's HTTP webhook handler with a Kafka consumer
  - Two topics: `recor.declaration.events.v1`, `recor.verification.events.v1`
  - Schema versioning: payload still JSON; topic suffix is the schema version
  - Keep the HTTP webhooks behind a feature flag for one release cycle (gradual cutover)
- **Acceptance criteria:**
  - Same integration smoke passes against Kafka backend
  - Throughput: 1000 declarations/second sustained without DLQ accumulation
  - Failure: kill the broker → relay backs off; restore broker → resumes
- **Effort:** 3 weeks
- **Dependencies:** R-LOOP-4-DLQ shipped (done — PRs #57+#59)
- **External:** Kafka cluster (single-broker is fine for v1; needs infra team)
- **Agent:** `infrastructure-engineer`
- **Brief:**
  > Add Kafka transport alongside the existing HTTP relay. infrastructure/kafka/ gets a docker-compose for local dev (single-broker bitnami/kafka image). Add KAFKA_BROKERS env to both services. New relay implementations: services/declaration/src/infrastructure/kafka_producer.rs (publishes outbox rows to recor.declaration.events.v1 keyed by aggregate_id), services/verification-engine/src/infrastructure/kafka_consumer.rs (consumes the topic, hits the same SubmitVerificationUseCase that the HTTP webhook used). The HTTP webhook handler stays — gated by env RELAY_TRANSPORT=http|kafka (default http for one release). After verification, flip the default to kafka in a follow-up. Use rdkafka crate. Schema: payload stays JSON for now (same shape as outbox.payload); future schema-registry follow-up. Add a kafka-smoke.sh that brings up Kafka + both services with RELAY_TRANSPORT=kafka and runs the standard integration-smoke assertions.

#### R-LOOP-3 — SPIFFE+mTLS service-to-service auth 🔒
- **Why:** HMAC-signed HTTP is fine for v1 but not the target. Real service-to-service auth is mTLS via SPIFFE identities.
- **Scope:**
  - SPIRE server + agent deployed in the K8s cluster
  - Each service gets a SVID via SPIRE workload API
  - mTLS termination at the service level (rustls)
  - Replace HMAC verify with SVID verify on inbound endpoints
  - HTTP HMAC stays behind a flag for one release
- **Effort:** 3 weeks
- **Dependencies:** R-LOOP-2 (mTLS over Kafka is a different shape than mTLS over HTTP)
- **External:** SPIRE deployment; cluster CA trust
- **Agent:** `infrastructure-engineer`
- **Brief:**
  > Deploy SPIRE for service-to-service mTLS. Set up SPIRE server + agent in infrastructure/spire/. Each service registers a workload entry (spiffe://recor.cm/declaration, spiffe://recor.cm/verification, etc.). Add spiffe-rs crate to both services to fetch SVID + trust bundle at startup, configure rustls with both. Inbound endpoints use a tower middleware that extracts the peer SVID from the TLS connection and asserts it matches an allowlist (configurable per endpoint). The HMAC path stays as a fallback for one release (flag AUTH_TRANSPORT=hmac|mtls; default hmac). Update integration smoke to optionally run in mtls mode.

### Audit immutability

#### R-DECL-9 — Anchor receipts to Hyperledger Fabric 🔒
- **Why:** D15 cryptographic-provenance commitment. Today the receipt hash is in the response but not anchored anywhere — a DB admin could rewrite history.
- **Scope:**
  - Hyperledger Fabric cluster (3 orderers + 4 peers minimum for production)
  - Audit chaincode (the `chaincode/audit-witness/` skeleton already exists; needs real implementation)
  - Each `declaration.submitted.v1` event → a Fabric transaction recording { event_id, declaration_id, receipt_hash_hex, timestamp, signing-peer-attestation }
  - Async; doesn't block the declaration submit path (commits to Fabric in a background worker)
  - Operator can verify any historical declaration's receipt against the Fabric audit channel
- **Effort:** 6 weeks (Fabric is complex; chaincode + ops)
- **Dependencies:** none in code
- **External:** Fabric cluster operators; chaincode review by infosec
- **Agent:** `infrastructure-engineer`
- **Brief:**
  > Implement audit anchoring of declaration receipts to a Hyperledger Fabric channel. The Fabric cluster ops are out of scope for the implementing agent (assume someone else stands up the cluster); focus on (1) finishing chaincode/audit-witness/ with real PutState + GetState methods for recor.audit.declaration entries keyed by event_id, (2) adding apps/worker-fabric-bridge that consumes from outbox-relay topic and writes to Fabric, (3) adding apps/audit-verifier that takes a declaration_id and produces a proof from Fabric. Defer the cluster provisioning to the infra team (they're already familiar with Fabric per Architecture v4 p18).

### Observability + secrets

#### OPS-4 — Vault for secret management 🔒
- **Why:** Today secrets live in env vars. Production needs Vault or AWS Secrets Manager.
- **Effort:** 1 week
- **Dependencies:** none
- **External:** Vault cluster
- **Agent:** `infrastructure-engineer`
- **Brief:**
  > Stand up Vault in infrastructure/vault/ (dev mode docker-compose for local; production deployment is separate). Both services + portal pull their secrets at startup via Vault's KV-v2 + AppRole auth. Replace the env-based DATABASE_URL, RECOR_*_HMAC, LOG_REDACTION_KEY etc. with Vault paths. The bootstrap secret (the AppRole role-id + secret-id) IS still env-based — there's always one secret you can't pull from Vault — but it's tightly scoped. Document the Vault setup + rotation in docs/runbooks/vault-onboarding.md.

#### OBS-1 — Prometheus metrics + Grafana dashboards ⚪ (was R-OBS-1)
- **Why:** OTel traces flow today but there's no Prometheus scraping; no operator dashboards.
- **Scope:**
  - Add `/metrics` endpoint to both services exposing Prometheus-compatible counters/histograms
  - Define core metrics: `recor_declarations_submitted_total`, `recor_verification_cases_total{lane}`, `recor_outbox_undispatched`, `recor_outbox_dlq_size`, `recor_relay_delivery_latency_seconds`, `recor_oidc_jwks_fetch_latency_seconds`, etc.
  - Grafana dashboards (4 minimum): platform health (request rates, error rates, p99 latencies), relay health (queue depths, delivery latencies, DLQ accumulation), verification health (lane distribution, fusion belief distributions), auth health (JWKS fetch latency, OIDC failure rates)
  - Alert rules: DLQ > 100 rows for > 10min, outbox delivery latency p99 > 30s, OIDC verifier down
- **Effort:** 1 week
- **Dependencies:** F-007 observability stack (already shipped)
- **External:** none (uses the existing dev observability stack)
- **Agent:** `infrastructure-engineer`
- **Brief:**
  > Add Prometheus metrics to both services. Use the prometheus crate; expose at /metrics (no auth — this is internal). Define core metrics: per-endpoint request counter, per-endpoint latency histogram, recor_declarations_submitted_total, recor_verification_cases_total{lane}, recor_outbox_undispatched (gauge), recor_outbox_dlq_size (gauge), recor_outbox_dlq_replays_total{success}, recor_relay_delivery_latency_seconds histogram, recor_oidc_jwks_fetch_latency_seconds histogram, recor_oidc_verify_total{result}. Update the existing dev Prometheus config (infrastructure/observability-dev/prometheus.yml) to scrape /metrics on both services. Build 4 Grafana dashboards as JSON in infrastructure/observability-dev/grafana/dashboards/: platform-health.json, relay-health.json, verification-health.json, auth-health.json. Add alert rules in a new infrastructure/observability-dev/alert-rules.yaml.

#### OBS-2 — Promote observability-smoke to required check 🟡
- **Why:** R-OBS-1 in the original roadmap. The observability-smoke workflow runs but isn't required for merge.
- **Scope:**
  - After 10 consecutive green runs (operational confidence) add it to required-checks
  - Update docs/security/branch-protection.md
- **Effort:** 1 hour
- **Dependencies:** observability-smoke must have 10 green runs in CI history
- **External:** none
- **Agent:** `infrastructure-engineer`
- **Brief:**
  > Promote the observability-smoke job to a required status check on main. Verify 10 consecutive green runs in the action history first; if not yet, defer. Update docs/security/branch-protection.md and re-run tools/ci/apply-branch-protection.sh.

---

## Phase 3 — Portal completeness

These can be picked up in any order; no external blockers.

### Portal tickets

#### R-PORT-1 — i18n (FR primary, EN secondary, Pidgin tertiary) ⚪
- **Why:** Cameroon is officially bilingual French/English, with Pidgin widely spoken. English-only is unacceptable.
- **Scope:**
  - i18next + react-i18next setup
  - Three locale files: fr.json (primary), en.json (secondary), pidgin.json (tertiary; community-translated)
  - Locale selector in the header
  - Browser locale auto-detection + persistent preference (localStorage)
  - Legal-French terminology review (lawyer needs to sign off on French strings — this isn't an engineering decision)
  - Build splits per locale (or single-bundle with dynamic import — measure bundle size both ways)
- **Acceptance criteria:**
  - Toggling locale changes every visible string; no untranslated fragments
  - Translation review: FR by a Cameroonian lawyer (legal terms), Pidgin by a community linguist
  - Bundle size delta: < 30 KB gzipped per added locale (stay within the 250 KB total budget)
- **Effort:** 1 week + translation review
- **Dependencies:** none
- **External:** translation review
- **Agent:** `typescript-frontend-engineer`
- **Brief:**
  > Add i18n to applications/declarant-portal using react-i18next. Three locale files at src/locales/{fr,en,pidgin}.json. Wrap every visible string in t('key.path'). Add a locale selector in App.tsx header. Auto-detect via navigator.language, persist via localStorage. For initial PR: ship English + French (the legal language); leave Pidgin as a stub file with TODO comments inviting community translation. Bundle: use dynamic import to keep non-active locales out of the main chunk. Update vitest tests to mock the i18n provider. Update CLAUDE.md with the translation review workflow.

#### R-PORT-2 — Offline drafts (Workbox + IndexedDB) ⚪
- **Why:** Cameroonian connectivity is intermittent. A declarant filling out a form for 15 min shouldn't lose their work when their connection drops.
- **Scope:**
  - Workbox service worker for asset caching + offline shell
  - IndexedDB (via Dexie) for in-progress drafts
  - Auto-save the form state every 5s while editing
  - On reconnect: prompt the declarant to resume the saved draft
  - Drafts auto-expire after 24h
- **Effort:** 1 week
- **Dependencies:** none
- **External:** none
- **Agent:** `typescript-frontend-engineer`
- **Brief:**
  > Add offline-draft support to the portal. Workbox service worker (use vite-plugin-pwa) caches the SPA shell + static assets so the portal loads offline. Dexie wrapper around IndexedDB; schema { drafts: { id auto, form_state json, created_at, last_modified_at } }. In DeclarationForm.tsx, watch react-hook-form state via watch() and save to Dexie every 5 seconds while dirty. On portal load, query Dexie for un-submitted drafts; if any < 24h old exist, show a "Resume your saved draft?" banner above the form. After successful submit, clear the corresponding draft. Expire drafts > 24h via a cron-style cleanup on app boot. Tests: assert auto-save fires; assert resume restores form state.

#### R-PORT-3 — Multi-step wizard ⚪
- **Why:** Current single-page form is overwhelming. A wizard splits into entity → owners → review → sign.
- **Scope:**
  - Four steps: Entity (entity_id, kind, effective_from), Owners (beneficial_owners array with sub-form per owner), Review (read-only summary), Sign (cryptographic signature step + submit)
  - Forward + back navigation; can't proceed if current-step validation fails
  - Progress indicator
- **Effort:** 3-4 days
- **Dependencies:** none; pairs well with R-PORT-1 (translate the step labels)
- **External:** none
- **Agent:** `typescript-frontend-engineer`
- **Brief:**
  > Refactor applications/declarant-portal/src/features/declaration/DeclarationForm.tsx into a 4-step wizard. Step 1: Entity (entity_id, kind, effective_from). Step 2: Owners (the beneficial_owners array with the existing useFieldArray sub-form, can add/remove). Step 3: Review (read-only summary of all fields, plus the live canonical-payload bytes preview). Step 4: Sign + Submit (the Ed25519 signing + POST). Forward button on each step disabled until the step's fields validate (use react-hook-form's trigger() to validate just the step's fields). Back button always enabled. Visible progress indicator (1/4, 2/4, ...). Maintain state across steps via the same useForm() instance. Tests with @testing-library/react: each step's content renders correctly; can't proceed past step 2 with invalid owner data; back navigation preserves typed-in values.

#### R-PORT-5 — WCAG 2.1 AA audit + remediation ⚪
- **Effort:** 1 week + remediation cycles
- **Dependencies:** R-PORT-3 (wizard) if doing both
- **External:** a11y audit tooling (axe-core, NVDA/JAWS for manual)
- **Agent:** `typescript-frontend-engineer`
- **Brief:**
  > Run axe-core (eslint-plugin-jsx-a11y for static; @axe-core/playwright for runtime) against the portal. Address every Critical and Serious finding. Manual screen-reader pass (NVDA on Windows or VoiceOver on macOS). Add an a11y-smoke.test.ts that runs axe against each major view (form, wizard steps if R-PORT-3 shipped, verification-status, error states). Document the audit + findings + remediations in docs/security/a11y-audit-2026-q2.md (or whatever the current quarter is).

#### R-PORT-6 — Playwright E2E ⚪
- **Effort:** 3-4 days
- **Dependencies:** R-PORT-5 (overlaps; both need browser automation harness)
- **External:** none
- **Agent:** `test-author`
- **Brief:**
  > Stand up Playwright E2E test suite at applications/declarant-portal/tests/e2e/. Configure to test against the built bundle (pnpm preview + Playwright config webServer block). Critical-path scenarios: (1) Happy path — fill form, sign, submit, see receipt + verification status poll → accepted; (2) Validation — invalid entity_id surfaces error; (3) Verification rejected — fill form for a person not in mock BUNEC, see red-lane status; (4) Polling stops on terminal state. Add to CI: spin up the full D↔V compose stack as a service, run Playwright against http://localhost:8082 (the portal nginx). Use mock BUNEC seeding so tests are deterministic.

#### R-PORT-7 — Generated API client from OpenAPI spec ⚪
- **Effort:** 3 days
- **Dependencies:** DOC-1 (OpenAPI spec must exist first)
- **External:** none
- **Agent:** `typescript-frontend-engineer`
- **Brief:**
  > Generate a TypeScript API client from the OpenAPI spec produced by DOC-1. Use openapi-typescript (https://openapi-ts.dev/) — generates a minimal types-only client. Replace applications/declarant-portal/src/lib/api.ts hand-written types with imports from the generated client. Keep the higher-level wrappers (submitDeclaration, getDeclaration) but their inner types now come from the spec. Add a CI step that regenerates the client and fails on drift. Update the portal's Vite config to include the generated file as a dev dependency (or commit it).

---

## Phase 4 — Compliance & legal

These cannot be coded by an engineering agent alone — they need
lawyer + compliance review. The engineering agent can prep the
artifacts; legal signs off.

### Compliance tickets

#### COMP-1 — GDPR / OHADA data-subject rights ⚪
- **Scope:** Documented procedures for: right-to-access (declarant can download all data RÉCOR holds on them), right-to-rectification (covered by R-DECL-3 Amend/Correct), right-to-erasure (with audit-log preservation — hard problem; usually requires legal carve-out for AML/CFT registries)
- **Effort:** 2 weeks engineering + 2 weeks legal review
- **Dependencies:** R-DECL-3-AMEND + R-DECL-3-CORRECT
- **External:** privacy lawyer
- **Agent:** `docs-author` (drafts) + `rust-service-engineer` (endpoints)
- **Brief:**
  > Draft docs/compliance/gdpr-procedures.md covering: right-to-access (build POST /v1/declarations/by-principal endpoint that returns everything for the authenticated principal), right-to-rectification (we have Amend/Correct), right-to-erasure (special case: AML/CFT registries are typically exempt from full erasure under OHADA framework — document the legal basis citation and our partial-erasure procedure: redact PII but retain the audit hash chain). Engineering surface: implement the by-principal endpoint, ensure audit logs do NOT log PII (paired with OPS-2). Coordinate with privacy counsel — DO NOT ship without legal sign-off.

#### COMP-2 — Audit log immutability + retention policy ⚪
- **Scope:** Database-level grants that prevent UPDATE/DELETE on `declaration_events`; documented retention for outbox/idempotency tables
- **Effort:** 2 days engineering + review
- **Dependencies:** R-DECL-9 (Fabric anchoring as the higher-layer immutability)
- **External:** none
- **Agent:** `security-engineer`
- **Brief:**
  > Lock down the event log. Migration: REVOKE UPDATE, DELETE ON declaration_events FROM recor; only INSERT and SELECT remain. Same for verification_cases payload column once schema stabilises. Update the recor role's grants accordingly. Document the retention policy: outbox rows pruned 30d after dispatched_at; outbox_dlq retained forever; idempotency_records expire at expires_at (already set); declaration_events retained forever. Write a cleanup job (separate cron-like binary or a function called from main on a tokio interval) that prunes outbox rows. Document the entire data-retention policy in docs/compliance/data-retention.md.

#### COMP-3 — Data classification ⚪
- **Scope:** Document which fields are PII, public-record, confidential. Every field in every table classified.
- **Effort:** 3 days
- **Dependencies:** none
- **External:** legal review
- **Agent:** `docs-author`
- **Brief:**
  > Inventory every column in every table in services/declaration, services/verification-engine, and the future services/{person,entity}-service. Classify each as: Public, Internal, Confidential, PII, Sensitive-PII. Document at docs/compliance/data-classification.md as a table. Each row links to the schema definition. Document the corresponding handling rules: PII fields must be redacted in logs (paired with OPS-2); Sensitive-PII (biometrics) requires field-level encryption (FUTURE ticket); Public can appear in cached responses.

#### COMP-4 — Regulatory mapping ⚪
- **Scope:** Cite which provisions of Cameroon's beneficial-ownership law each endpoint enforces; do the same for OHADA AML/CFT framework
- **Effort:** 1 week
- **External:** AML/CFT counsel
- **Agent:** `docs-author`
- **Brief:**
  > Cite the legal basis for each endpoint and each invariant. docs/compliance/regulatory-mapping.md: table mapping endpoint → legal provision (Cameroon law N° 2014/007 du 23 avril 2014 portant fixation des incriminations et des sanctions pénales pour la transparence du secteur extractif; OHADA Acte uniforme révisé du 30 janvier 2014; FATF Recommendation 24). Same for invariants (e.g., "ownership_basis_points sum = 10_000" cites Cameroon decree on beneficial-ownership thresholds). Get the citations from AML/CFT counsel — don't invent them.

#### COMP-5 — DR drill + backup tooling ⚪
- **Scope:** Documented disaster recovery procedure + a quarterly drill
- **Effort:** 2 weeks engineering + ongoing
- **Dependencies:** OPS-4 (Vault) + OBS-1 (metrics)
- **External:** none for engineering, scheduled time for the actual drill
- **Agent:** `infrastructure-engineer`
- **Brief:**
  > Build the disaster-recovery toolkit: (1) docs/runbooks/restore-from-backup.md from DOC-3; (2) scripts/dr-drill.sh that takes a current dev environment to "full data loss" state, restores from the most recent backup, and asserts a freshly-recovered platform can serve traffic; (3) schedule the drill as a quarterly required exercise — add a calendar reminder + a ticket template. Document the RTO (Recovery Time Objective) and RPO (Recovery Point Objective) commitments.

---

## Phase 5 — Pre-launch hardening

### Pre-launch tickets

#### PEN-1 — External penetration test 🔒
- **Effort:** 2-3 weeks (vendor-led)
- **Dependencies:** all of Phase 0 + most of Phase 2 (need a complete system to pentest)
- **External:** pentest vendor; budget
- **Agent:** N/A — vendor-led
- **Brief:** N/A

#### LAUNCH-1 — Soft launch playbook ⚪
- **Effort:** 1 week
- **Agent:** `docs-author`
- **Brief:** docs/runbooks/soft-launch-playbook.md outlining the 100-user → 1000-user → public ramp-up with rollback triggers.

---

## Summary table

| Phase | Track | Items | Bounded total |
|---|---|---|---|
| **Phase 0** | Operational, domain, CI/CD, docs | 14 tickets | ~30 days |
| **Phase 1** | Verification stages + identity (external-blocked but skeletons buildable) | 8 tickets | ~16 weeks code, parallel with months of procurement |
| **Phase 2** | Infrastructure migration | 5 tickets | ~12 weeks, blocked on cluster ops |
| **Phase 3** | Portal completeness | 6 tickets | ~3 weeks |
| **Phase 4** | Compliance | 5 tickets | ~5 weeks code, paced by legal review |
| **Phase 5** | Pre-launch | 2 tickets | ~3 weeks |

## How we'll execute

1. Phase 0 can start immediately — every item has a ready brief and no external blocker.
2. In parallel, procurement starts the external-dependency conversations (BUNEC API, sanctions feeds, ICIJ licence, Anthropic budget, Kafka cluster, Vault, Fabric cluster).
3. As Phase 0 items land, the agent definitions in `.claude/agents/` get refined (the briefs here are starting points — each finished ticket teaches the next).
4. Phase 2 + 3 unblock as procurement closes.
5. Phase 4 runs in lockstep with legal review.
6. Phase 5 fires when the system is feature-complete enough to be a meaningful pentest target.
