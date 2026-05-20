# Permission matrix — canonical principal × endpoint × decision

**Doctrine reference:** D17 (zero trust at every network boundary).
**Audit reference:** closes the "Permission model drift" row of the
MEDIUM/LOW summary table in `docs/audit/10-findings.md` — the audit
flagged that the platform had no single canonical place describing
which principals may call which endpoints.

This document is the authoritative permission matrix. Every
state-changing endpoint and every read endpoint MUST appear here. If
an endpoint is not in this matrix, it is considered to refuse all
callers (fail-closed default).

## Principal classes

| Principal class | Source of identity | Examples |
|---|---|---|
| `unauthenticated` | None — request has no valid OIDC token, mTLS peer SPIFFE ID, or HMAC signature | Public ingress to `/healthz`, `/readyz`, `/openapi.json`, `/docs` |
| `declarant` | OIDC subject claim; not on any admin allowlist; OIDC `scope` does NOT include a `recor:obliged-entity` token | A submitting beneficiary; a registered company representative |
| `obliged-entity` (TODO-006) | OIDC subject claim AND OIDC `scope` includes `recor:obliged-entity` (or a sub-scope `recor:obliged-entity:cdd` / `:onboarding` / `:periodic`); IdP provisioning gated by COBAC/CEMAC supervision proof or DNFBP registration | A bank performing customer due diligence under EU 6AMLD Art. 12 + AMLR Chapter IV; a notary performing the post-Sovim legitimate-interest read |
| `admin` | OIDC subject claim AND appears on the per-service `ADMIN_PRINCIPALS` allowlist; the `recor:admin` scope alone is **NOT** sufficient (the allowlist is authoritative) | Sovereign back-office operator; on-call engineer |
| `internal-service` | mTLS peer presents a SPIFFE ID on the per-consumer `INTERNAL_PEER_SPIFFE_IDS` allowlist OR HMAC signature verifies under the configured shared secret with a fresh `iat` | declaration→V-engine; V-engine→declaration; worker-fabric-bridge→V-engine |
| `prometheus-scraper` | NetworkPolicy admits scrape from the Prometheus pod selector; no OIDC required | Cluster-internal Prometheus instance |

## Endpoint matrix

Notation:
- `Y` — class is permitted
- `n` — class is refused with a fail-closed status code
- `—` — class is structurally unable to reach this endpoint (e.g. an
  internal-service endpoint behind mTLS won't see an OIDC token)
- For each refusal, the parenthesised status code is the canonical one
  the service returns

### `services/declaration`

| Method + path | unauthenticated | declarant | obliged-entity (TODO-006) | admin | internal-service | prometheus-scraper | IAL minimum (TODO-020) |
|---|---|---|---|---|---|---|---|
| `GET /healthz`, `/readyz` | Y | Y | Y | Y | Y | Y | — |
| `GET /openapi.json`, `/docs` | Y | Y | Y | Y | Y | Y | — |
| `GET /metrics` | n (network-policy 403) | n | n | n | n | Y | — |
| `POST /v1/declarations` | n (401) | Y | n (403) | Y | — | — | **IAL2** |
| `GET /v1/declarations/{id}` | n (401) | Y if `declarant_principal == caller` else n (404 — see FIND-004 closure note) | Y; **redacted** (no declarant_principal, no correlation_id, no verification_case_id, no BO `cascade_tier_b_ruled_out_evidence`, no `nominator_person_id`) | Y; full payload | — | — | IAL1 |
| `POST /v1/declarations/{id}/supersede` | n (401) | Y if `declarant_principal == caller` else n (404) | n (403) | Y | — | — | **IAL3** |
| `POST /v1/declarations/{id}/amend` | n (401) | Y if `declarant_principal == caller` else n (404) | n (403) | Y | — | — | **IAL2** |
| `POST /v1/declarations/{id}/correct` | n (401) | n (403) — admin only | n (403) | Y | — | — | **IAL3** |
| `GET /v1/dlq` | n (401) | n (403) | n (403) | Y | — | — | IAL1 |
| `POST /v1/dlq/{id}/replay` | n (401) | n (403) | n (403) | Y | — | — | IAL1 |
| `POST /v1/internal/verification-outcomes` | — | — | — | — | Y (HMAC + `iat` window) | — | — |
| `POST /v1/discrepancies` (TODO-003) | n (401) | n (403) | Y | n (403) — admins use the back-office workflow, not the FI intake | — | — | **IAL2** |
| `GET /v1/discrepancies/by-obliged-entity` (TODO-003) | n (401) | n (403) | Y; sees their own submissions | n (403) | — | — | IAL1 |
| `POST /v1/fiu/search` (TODO-008) | n (401) | n (403) | n (403) | n (403) — admins use the projection surface, not the FIU log | — | — | **IAL3** |
| `GET /v1/fiu/disclosure/{id}` (TODO-008) | n (401) | n (403) | n (403) | n (403) | — | — | IAL2 |
| `POST /v1/public-feedback` (TODO-009) | Y (CAPTCHA + per-IP throttle) | Y | Y | Y | — | — | — |
| `POST /v1/sanctions/initiate` (TODO-004) | n (401) | n (403) | n (403) | Y; allowlist + justification required | — | — | **IAL3** |
| `POST /v1/sanctions/{id}/escalate` (TODO-004) | n (401) | n (403) | n (403) | Y; allowlist + justification required | — | — | **IAL3** |
| `POST /v1/sanctions/{id}/withdraw` (TODO-004) | n (401) | n (403) | n (403) | Y; allowlist + justification required | — | — | **IAL3** |
| `GET /v1/sanctions/public` (TODO-004) | Y (cached 24h) | Y | Y | Y | — | — | — |

#### TODO-008 — FIU (ANIF + R.40 / Egmont MLAT) disclosure

`POST /v1/fiu/search` is admitted ONLY for the `recor:fiu-anif`
OIDC scope. Production deployments MUST additionally enforce:

1. **mTLS peer-ID allowlist** — via the SPIFFE bootstrap in
   `services/declaration/src/main.rs`, restricting the calling pod
   identity to ANIF-owned workloads (or the Egmont gateway pod
   identity for MLAT-routed foreign-FIU requests).
2. **Source-IP allowlist** — at the cluster ingress (network policy
   or service mesh), gating ingress to known ANIF egress IPs.

Every disclosure writes a row to
[`fiu_disclosure_log`](../../services/declaration/migrations/0012_fiu_disclosure_log.sql)
with the requesting principal, the ANIF case reference, the free-text
justification, the field-level audit (which projection columns were
disclosed), the resolved declaration_id (when the search hit), and
the MLAT identifiers when the request was routed through Egmont. The
table is COMP-2-immutable; retention is indefinite (D15).

Operator handover: see
[`docs/runbooks/anif-onboarding.md`](../runbooks/anif-onboarding.md).

#### TODO-009 — Public-feedback intake

`POST /v1/public-feedback` is the only **unauthenticated** state-
changing endpoint on the platform. Access is gated by:

1. A CAPTCHA token issued by the configured provider (hCaptcha or
   reCAPTCHA). The platform stores BLAKE3(token) in the audit row,
   never the raw token.
2. A per-IP throttle keyed on BLAKE3(`X-Forwarded-For` first hop).
   The window + limit are config-driven (`PUBLIC_FEEDBACK_PER_IP_*`).
   Excess submissions surface the
   `recor_public_feedback_rate_limited_total{result=throttled}`
   metric and refuse with `429`-equivalent.
3. Mass-flag detection: when more than `PUBLIC_FEEDBACK_MASS_FLAG_THRESHOLD`
   reports name the same target within the configured window, the
   row's `triage_priority` is set to `low` so the back-office can
   batch-dismiss anonymous mass-flags.

Every row is event-sourced into
[`public_feedback_log`](../../services/declaration/migrations/0013_public_feedback_log.sql),
which is COMP-2-immutable. Triage / resolve endpoints are
back-office and admit only admins.

#### TODO-006 — Obliged-entity tier

The obliged-entity tier is gated on the verified OIDC `scope` claim
containing `recor:obliged-entity` or any sub-scope
(`recor:obliged-entity:cdd`, `:onboarding`, `:periodic`). The
platform-side wiring is in
[`services/declaration/src/api/auth.rs`](../../services/declaration/src/api/auth.rs)
([`PrincipalClass`](../../services/declaration/src/api/auth.rs) /
[`PrincipalClass::from_scope_claim`](../../services/declaration/src/api/auth.rs)).
The IdP-side onboarding workflow (supervised-entity proof,
revocation-on-lapse, per-supervision-class rate limits, per-disclosure
audit log) is the operator's responsibility — see the runbook
`docs/runbooks/obliged-entity-onboarding.md` (planned).

The reduced `GET /v1/declarations/{id}` payload for the obliged-entity
tier is implemented by
[`GetDeclarationResponse::redact_for_obliged_entity`](../../services/declaration/src/api/dto.rs)
and exercised by the unit test
`api::auth::dto_redaction_tests::obliged_entity_redactor_strips_sensitive_fields`.

#### TODO-020 — Identity-assurance-level (IAL/AAL) gate

Per NIST 800-63A and FATF c.24.6 IO.5 ("identity verification of the
submitter"), each state-changing endpoint enforces a minimum IAL on
the verified OIDC token's `acr` claim. The mapping is computed in
`packages/recor-auth-oidc/src/lib.rs` via
[`AssuranceLevel::from_acr_claim`](../../packages/recor-auth-oidc/src/lib.rs); the
ladder is:

- **IAL1** — self-asserted identity; the fail-closed floor when the
  IdP does not advertise an `acr` claim.
- **IAL2** — verified evidence + verified address. Required for
  `POST /v1/declarations` (submission) and `POST /v1/declarations/{id}/amend`.
- **IAL3** — in-person or supervised remote verification. Required for
  the administrative endpoints `correct`, `supersede`,
  `dissolve`, `merge-into`, and `dlq/replay` (per service).

Tokens whose `acr` resolves below the endpoint minimum are refused
with `403 Forbidden` and the body
`{ "error": { "kind": "forbidden", "message": "authorization denied: insufficient_assurance" } }`.

Operators wiring a new IdP must read
`docs/runbooks/oidc-idp-acr-config.md` for the per-IdP configuration
that maps the issuer's policy onto this ladder.

### `services/verification-engine`

| Method + path | unauthenticated | declarant | admin | internal-service | prometheus-scraper |
|---|---|---|---|---|---|
| `GET /healthz`, `/readyz` | Y | Y | Y | Y | Y |
| `GET /openapi.json`, `/docs` | Y | Y | Y | Y | Y |
| `GET /metrics` | n (network-policy 403) | n | n | n | Y |
| `POST /v1/verifications` | n (401) | n (403 — admin only post-FIND-002) | Y | — | — |
| `GET /v1/verifications/{declaration_id}` | n (401) | Y if `declarant_principal == caller` else n (404 — see FIND-004) | Y | — | — |
| `GET /v1/dlq` | n (401) | n (403) | Y | — | — |
| `POST /v1/dlq/{id}/replay` | n (401) | n (403) | Y | — | — |
| `POST /v1/internal/declaration-events` | — | — | — | Y (HMAC + `iat` window) | — |

### `services/person-service`

| Method + path | unauthenticated | declarant | admin | internal-service | prometheus-scraper |
|---|---|---|---|---|---|
| `GET /healthz`, `/readyz`, `/openapi.json`, `/docs` | Y | Y | Y | Y | Y |
| `GET /metrics` | n | n | n | n | Y |
| `POST /v1/persons` | n (401) | Y | Y | — | — |
| `GET /v1/persons/{id}` | n (401) | Y if `created_by_principal == caller` else n (404 — FIND-005/FIND-006 closure) | Y | — | — |
| `GET /v1/persons/search` | n (401) | Y; results filtered to rows the caller registered | Y; sees all rows | — | — |
| `POST /v1/persons/{id}/merge-into/{target_id}` | n (401) | n (403) | Y | — | — |

### `services/entity-service` — arrangements (TODO-002 / R.25)

When the `TODO-002-domain` follow-up lands, the entity-service will
also expose `/v1/arrangements`. The current state: migration 0003
has shipped (data substrate + COMP-2 + retention column), but the
REST surface is not yet wired. Per ADR-0015 the discriminated
section lives in the same service binary; the permission gates will
mirror the entity surface (declarant + admin) with one addition:
**trustee identity proof** — a trustee referenced in
`trustee_refs` MUST be either a verified person_id, an
entity_id, or a fiduciary_registration_id on the per-jurisdiction
allowlist (the constraint is enforced at the domain layer in the
follow-up).

### `services/entity-service`

| Method + path | unauthenticated | declarant | admin | internal-service | prometheus-scraper |
|---|---|---|---|---|---|
| `GET /healthz`, `/readyz`, `/openapi.json`, `/docs` | Y | Y | Y | Y | Y |
| `GET /metrics` | n | n | n | n | Y |
| `POST /v1/entities` | n (401) | Y | Y | — | — |
| `GET /v1/entities/{id}` | n (401) | Y | Y | — | — |
| `GET /v1/entities/search` | n (401) | Y | Y | — | — |
| `POST /v1/entities/{id}/update` | n (401) | Y if creator else n | Y | — | — |
| `POST /v1/entities/{id}/dissolve` | n (401) | n (403) | Y | — | — |

### `apps/audit-verifier`

| Method + path | unauthenticated | declarant | admin | internal-service | prometheus-scraper |
|---|---|---|---|---|---|
| `GET /healthz`, `/readyz` | Y | Y | Y | Y | Y |
| `GET /metrics` | n | n | n | n | Y |
| `POST /v1/audit/verify` | n (401) — closure of FIND-001 | Y; receipt redacted to public fields per data-classification | Y; full receipt | — | — |

#### Sovim-tiered response shape (TODO-007 / TODO-023 closure)

The audit-verifier's `GET /v1/audit/verify/{id}` response is tiered
per CJEU C-37/20 + C-601/20 (WM/Sovim). The tier is resolved from the
verified OIDC `scope` claim (production) or the `X-Recor-Dev-Scope`
header (dev). Unknown / missing → **PublicLegitimateInterest**
(D14 fail-closed).

| Scope | OIDC `scope` claim | Per-entry payload retained |
|---|---|---|
| Admin | `recor:admin` | `event_id`, `status`, `tx_id`, `on_chain_receipt_hash_hex`, `derived_receipt_hash_hex`, `on_chain_ts`, `event_type` |
| ObligedEntity | `recor:obliged-entity` (R.24 c.24.6(c); REQ-amld-iv-005) | Same as Admin **today**; redactor hook is in place — any future PII field added to `EntryReport` MUST be stripped at this tier |
| PublicLegitimateInterest | any other / missing | `event_id`, `status` only; **tx_id, hashes, timestamps, event_type stripped** |
| Unauthenticated | no token | refused — 401 |

The Sovim-protected identifiers — `national_id_document`,
`national_id_number`, `residential_address`,
`biometric_reference_hash`, `signer_public_key`, `public_key_hex`,
`primary_id_document` — are NEVER serialised into any tier's response,
even when the upstream projection event_payload includes them.

The integration test
`apps/audit-verifier/tests/payload_scoping.rs` enforces every row of
this table on every CI run; a future refactor that re-exports a
prohibited field fails CI immediately.

### `apps/audit-reconciler` (no HTTP surface)

Reconciler is a background job; its only externally visible surface is
`GET /metrics` on a dedicated listener (Prometheus only).

### `applications/declarant-portal`

Browser-side; identity flows through the declaration service. The
portal renders the admin DLQ navigation chrome conditionally on
`admin_principals` membership AND the underlying server-side check
refuses any request from a non-admin (defence in depth).

## Refusal status codes

The audit also flagged a 403-vs-404 existence side-channel risk on
`GET /v1/declarations/{id}` and equivalent endpoints. Closure pattern:

- **404** (not 403) — when a declarant requests a row another
  principal owns. The projection adapter returns "not found" rather
  than "forbidden" so an unauthorised caller cannot enumerate which
  IDs exist.
- **403** — only when the endpoint is structurally admin-only (e.g.
  `correct`, `dissolve`, `merge-into`, `dlq/replay`). The caller's
  inability to access the endpoint is not declaration-specific — they
  cannot access it for any input — so 403 leaks no row-level
  information.
- **401** — only when the request has no valid identity at all.

## Maintenance

When a new endpoint is added, the PR MUST update this matrix in the
same commit (D05 — documentation is part of the feature). The
`tools/ci/check-adr-bidi.sh` companion script (Sprint 4) verifies that
every service-side route handler with the `#[utoipa::path]` attribute
has at least one matching row in this file; absence fails CI.
