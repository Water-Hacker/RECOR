# Pass B — Section 7: Permissions

Status: production-readiness audit, Pass B
Reviewer: Claude Code (lead orchestrator), 2026-05-13
Scope: every protected surface mapped to its authorisation gate; role
matrix; public-surface hardening; privilege-escalation enumeration.

## 7.0 Identity sources

RÉCOR has exactly **four** identity sources, defined by which middleware
populated the principal:

| Source | Where set | Trust | Used by |
|---|---|---|---|
| OIDC Bearer | `auth_middleware` after `OidcVerifier.verify` (`services/declaration/src/api/auth.rs:97-145`) | production-grade | declarants, admins, operators |
| `X-Recor-Dev-Principal` | `auth_middleware` when `Config::is_dev()` is true (`services/declaration/src/api/auth.rs:77-95`) | dev-only by design | dev/integration tests |
| HMAC signature | service-to-service inbound (`services/declaration/src/api/internal.rs:122-164`, `services/verification-engine/src/api/internal.rs:107-146`) | shared-secret per channel | declaration ↔ V-engine, worker-fabric-bridge ← relay |
| SPIFFE peer SVID | `recor-spiffe` TLS terminator under `AUTH_TRANSPORT=mtls\|mtls-only` (`services/declaration/src/main.rs`) | workload identity | service-to-service when mTLS active |

The `Principal::subject` field (`services/declaration/src/api/auth.rs:35-39`)
is the **only** identity datum used by downstream handlers. There is no
path supplied, body supplied, or query supplied principal anywhere in
the codebase. This is the load-bearing D17 (zero trust) invariant.

---

## 7.1 Permission model integrity

### Surface inventory — declaration service

| Route | Method | Auth gate | Admin gate | File:line |
|---|---|---|---|---|
| `/healthz` | GET | none | none | `services/declaration/src/api/rest.rs:199, 297-306` |
| `/readyz` | GET | none | none | `services/declaration/src/api/rest.rs:200, 319-339` |
| `/metrics` | GET | none (network-only by deployment) | none | `services/declaration/src/api/rest.rs:217-219` |
| `/openapi.json` | GET | none | none | `services/declaration/src/api/openapi.rs:225` |
| `/docs` | GET | none (Scalar UI) | none | `services/declaration/src/api/openapi.rs:223` |
| `/v1/declarations` | POST | `auth_middleware` | none | `services/declaration/src/api/rest.rs:117` |
| `/v1/declarations/by-principal` | GET | `auth_middleware` | none — principal-scoped | `services/declaration/src/api/rest.rs:124` |
| `/v1/declarations/{id}` | GET | `auth_middleware` + owner-check at handler | none | `services/declaration/src/api/rest.rs:127, 514-518` |
| `/v1/declarations/{id}/supersede` | POST | `auth_middleware` + aggregate owner check | none | `services/declaration/src/api/rest.rs:129-130` |
| `/v1/declarations/{id}/amend` | POST | `auth_middleware` + handler owner check (line 750-754) + aggregate owner check | none | `services/declaration/src/api/rest.rs:133-134` |
| `/v1/declarations/{id}/correct` | POST | `auth_middleware` + handler owner check (line 830-834) + aggregate owner check | none | `services/declaration/src/api/rest.rs:137-138` |
| `/v1/internal/outbox-dlq` | GET | `auth_middleware` | `enforce_admin` (`api/dlq.rs:243-269`) | `services/declaration/src/api/rest.rs:159` |
| `/v1/internal/outbox-dlq/{id}/replay` | POST | `auth_middleware` | `enforce_admin` | `services/declaration/src/api/rest.rs:160-162` |
| `/v1/internal/verification-outcomes` | POST | HMAC (`verify_hmac_with_rotation`) under `hmac` / `mtls`; mTLS peer-ID under `mtls`/`mtls-only` | n/a — service-to-service | `services/declaration/src/api/rest.rs:191-196, api/internal.rs:122-164` |

### Surface inventory — verification-engine

| Route | Method | Auth gate | Admin gate | File:line |
|---|---|---|---|---|
| `/healthz` | GET | none | none | `services/verification-engine/src/api/rest.rs:126, 164-174` |
| `/readyz` | GET | none | none | `services/verification-engine/src/api/rest.rs:127, 176-196` |
| `/metrics` | GET | none | none | `services/verification-engine/src/api/rest.rs:136-138` |
| `/v1/verifications` | POST | `auth_middleware` | none | `services/verification-engine/src/api/rest.rs:63-69` |
| `/v1/verifications/{case_id}` | GET | `auth_middleware` | none | `services/verification-engine/src/api/rest.rs:64-69` |
| `/v1/internal/verification-outbox-dlq` | GET | `auth_middleware` | `enforce_admin` (`api/dlq.rs:185`) | `services/verification-engine/src/api/rest.rs:88-91` |
| `/v1/internal/verification-outbox-dlq/{id}/replay` | POST | `auth_middleware` | `enforce_admin` | `services/verification-engine/src/api/rest.rs:92-95` |
| `/v1/internal/declaration-events` | POST | HMAC + optional mTLS peer-ID | n/a — service-to-service | `services/verification-engine/src/api/rest.rs:118-123` |

### Surface inventory — person-service

| Route | Method | Auth gate | Admin gate | File:line |
|---|---|---|---|---|
| `/healthz`, `/readyz`, `/metrics`, `/openapi.json`, `/docs` | GET | none | none | `services/person-service/src/api/rest.rs:77-86` |
| `/v1/persons` | POST | `auth_middleware` | none | `services/person-service/src/api/rest.rs:64` |
| `/v1/persons/search` | GET | `auth_middleware` | none | `services/person-service/src/api/rest.rs:65` |
| `/v1/persons/{id}` | GET | `auth_middleware` | none | `services/person-service/src/api/rest.rs:66` |
| `/v1/persons/{id}/merge-into/{target}` | POST | `auth_middleware` | inline admin gate (`api/rest.rs:390-397`) | `services/person-service/src/api/rest.rs:67-70` |

### Surface inventory — entity-service

| Route | Method | Auth gate | Admin gate | File:line |
|---|---|---|---|---|
| `/healthz`, `/readyz`, `/metrics`, `/openapi.json`, `/docs` | GET | none | none | `services/entity-service/src/api/rest.rs:86-95` |
| `/v1/entities` | POST | `auth_middleware` | none | `services/entity-service/src/api/rest.rs:69` |
| `/v1/entities/search` | GET | `auth_middleware` | none | `services/entity-service/src/api/rest.rs:70` |
| `/v1/entities/{id}` | GET | `auth_middleware` | none | `services/entity-service/src/api/rest.rs:71` |
| `/v1/entities/{id}/update` | POST | `auth_middleware` | none (owner check in aggregate) | `services/entity-service/src/api/rest.rs:72-74` |
| `/v1/entities/{id}/dissolve` | POST | `auth_middleware` | inline admin gate (`api/rest.rs:454-462`) | `services/entity-service/src/api/rest.rs:76-79` |

### Surface inventory — worker-fabric-bridge

| Route | Method | Auth gate | Admin gate | File:line |
|---|---|---|---|---|
| `/healthz`, `/readyz`, `/metrics` | GET | none | none | `apps/worker-fabric-bridge/src/handlers.rs:32-34` |
| `/v1/relay` | POST | HMAC (`verify_hmac` — `apps/worker-fabric-bridge/src/handlers.rs:77-80`) | n/a — service-to-service | `apps/worker-fabric-bridge/src/handlers.rs:35` |

### Surface inventory — audit-verifier

| Route | Method | Auth gate | Admin gate | File:line |
|---|---|---|---|---|
| `/healthz`, `/readyz` | GET | none | none | `apps/audit-verifier/src/handlers.rs:27-28` |
| `/v1/audit/verify/{declaration_id}` | GET | **none (public read)** | none | `apps/audit-verifier/src/handlers.rs:29` |

The audit-verifier `verify` endpoint is intentionally public — see the
handler docstring at `apps/audit-verifier/src/handlers.rs:41-66`: a
public auditor can call this to confirm the on-chain hash matches the
projection without holding any credential. The endpoint never returns
the body of the declaration; only the `{ verified, on_chain_tx, ... }`
verdict.

### Integrity checks

#### Every protected surface has an `auth_middleware` or HMAC gate

Verified by walking the routers above. The only gateless paths are:

- public read-only ops surfaces (healthz / readyz / metrics)
- public consumer-facing OpenAPI surface (openapi.json / docs)
- the audit-verifier public read endpoint (deliberate)

#### Every admin gate references a real config field

| Service | Config field | File:line |
|---|---|---|
| declaration | `Config::admin_principals_list()` | `services/declaration/src/config.rs:329-335` |
| verification-engine | `Config::admin_principals_list()` | `services/verification-engine/src/config.rs` (analogous) |
| person-service | `Config::admin_principals_list()` | `services/person-service/src/config.rs` |
| entity-service | `Config::admin_principals_list()` | `services/entity-service/src/config.rs:94-105` |

All four parse `ADMIN_PRINCIPALS` CSV with the same shape: trim, drop
empties, dedupe via HashSet wrap.

#### `Principal::subject` is the only identity source

Confirmed by grep: no handler reads a `principal` field from a request
body or query parameter to make an authorisation decision. The
data-subject-access handler in particular makes this explicit
(`services/declaration/src/api/rest.rs:888-892`):

> "the principal is sourced exclusively from the authenticated session
> (D17) — no path parameter, no body, no query string."

### Finding PRM-1 (Severity: LOW)

**Title.** Inconsistent error code when admin allowlist is empty:
declaration + V-engine + person-service return **503**;
entity-service returns **400**.

**Evidence.**
- `services/declaration/src/api/dlq.rs:247-256` → 503
- `services/verification-engine/src/api/dlq.rs:187-196` → 503
- `services/person-service/src/api/rest.rs:390-392` → maps to
  `ServiceError::AdminDisabled` → 503 (verify by checking error.rs)
- `services/entity-service/src/api/rest.rs:454-458` → maps to
  `ServiceError::BadRequest` → 400

**Impact.** Operators monitoring 503s as "admin endpoint disabled
because allowlist empty" miss the entity-service case. A penetration
tester probing surface health could pick up the 400 as
"intermittently misbehaving" rather than "fail-closed disabled."

**Remediation.** Align entity-service `dissolve_entity_handler` with
the same `ServiceError::AdminDisabled` → 503 mapping.

---

## 7.2 Role-by-surface walkthrough

Roles in RÉCOR:

- **unauthenticated** — no Authorization header, no dev-header
- **declarant** — any OIDC sub (production) / any dev-header value
  (dev)
- **dev** — `X-Recor-Dev-Principal` with `Config::is_dev() == true`
- **admin** — subject in `ADMIN_PRINCIPALS` CSV
- **service-to-service** — valid HMAC signature on internal endpoints
- **anonymous-audit** — anyone, for `audit-verifier /v1/audit/verify`

Legend: ✓ = should + does; ✗ = should not + does not (matched);
**!** = divergence — should not but does, or should but does not.

### Public surfaces

| Surface | unauth | declarant | dev | admin | s2s | anonymous-audit |
|---|---|---|---|---|---|---|
| `GET /healthz` (all svc) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| `GET /readyz` (all svc) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| `GET /metrics` (all svc) | ✓ (network-only) | ✓ | ✓ | ✓ | ✓ | ✓ |
| `GET /openapi.json` (D/P/E) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| `GET /docs` (D/P/E) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| `GET /v1/audit/verify/{id}` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |

All public reads are deliberate. See PUB-1..PUB-4 in §7.3 for hardening
checks.

### Declaration write surfaces

| Surface | unauth | declarant (self) | declarant (other) | dev | admin |
|---|---|---|---|---|---|
| `POST /v1/declarations` | 401 ✓ | 201 ✓ | n/a (no concept of "other" — declarant submits their own data) | 201 ✓ via dev-header | 201 ✓ (admin can submit too — accepted) |
| `GET /v1/declarations/{id}` (own) | 401 ✓ | 200 ✓ | 403 ✓ (`api/rest.rs:514-518`) | 200 ✓ | 403 ✓ (admin role does **NOT** grant cross-principal read — accepted; admin can use the DBA-direct read for forensics; v1 design) |
| `GET /v1/declarations/by-principal` | 401 ✓ | 200 returns the authenticated principal's rows ✓ | n/a | 200 returns dev-principal's rows ✓ | 200 returns admin's own rows ✓ |
| `POST /v1/declarations/{id}/supersede` | 401 ✓ | 201 if owner ✓ | 403 (aggregate) ✓ | 201 ✓ | 403 if not owner ✓ |
| `POST /v1/declarations/{id}/amend` | 401 ✓ | 200 if owner + state allows ✓ | 403 (handler at line 750-754) ✓ | 200 ✓ | 403 if not owner ✓ |
| `POST /v1/declarations/{id}/correct` | 401 ✓ | 200 if owner + state=submitted ✓ | 403 (handler at line 830-834) ✓ | 200 ✓ | 403 if not owner ✓ |
| `GET /v1/internal/outbox-dlq` | 401 ✓ | 403 ✓ (`enforce_admin`) | 403 ✓ | 403 ✓ unless principal in allowlist | 200 ✓ |
| `POST /v1/internal/outbox-dlq/{id}/replay` | 401 ✓ | 403 ✓ | 403 ✓ | 403 ✓ | 200 ✓ |
| `POST /v1/internal/verification-outcomes` | 401 ✓ | 401 ✓ — user-auth ignored, HMAC required | 401 ✓ | 401 ✓ | 401 ✓ unless HMAC supplied — s2s only |

### Person-service write surfaces

| Surface | unauth | declarant | admin |
|---|---|---|---|
| `POST /v1/persons` | 401 ✓ | 201 ✓ | 201 ✓ |
| `GET /v1/persons/search` | 401 ✓ | 200 ✓ — full DB visible (see PRM-2) | 200 ✓ |
| `GET /v1/persons/{id}` | 401 ✓ | 200 ✓ — any principal can read any person record (see PRM-2) | 200 ✓ |
| `POST /v1/persons/{id}/merge-into/{target}` | 401 ✓ | 403 ✓ (inline admin gate) | 200 ✓ |

### Finding PRM-2 (Severity: MEDIUM)

**Title.** `GET /v1/persons/{id}` and `GET /v1/persons/search` are
authenticated but **not** principal-scoped — any authenticated
declarant can fetch any other declarant's person record.

**Evidence.** `services/person-service/src/api/rest.rs:64-66`. No
owner-check at the handler; no admin gate.

**Impact.** A registered declarant can enumerate every person record
in the platform. Person records carry PII (full names, identifier
numbers). This is a deliberate v1 design? — verify by checking the
service CLAUDE.md, but no inline comment justifies it as a v1 trade-off.

**Remediation.** Either (a) admit only the registering principal +
admin, OR (b) defer to a future Access service that translates
"this lawyer is acting on behalf of this declarant" into a query gate.
Today the surface is too wide. Track via a R-PER-* ticket.

### Entity-service write surfaces

| Surface | unauth | declarant | admin |
|---|---|---|---|
| `POST /v1/entities` | 401 ✓ | 201 ✓ | 201 ✓ |
| `GET /v1/entities/{id}` | 401 ✓ | 200 ✓ — entities are intentionally globally readable | 200 ✓ |
| `POST /v1/entities/{id}/update` | 401 ✓ | 200 if owner ✓ (aggregate check) | 200 if admin per aggregate ✓ |
| `POST /v1/entities/{id}/dissolve` | 401 ✓ | 403 ✓ | 200 ✓ |

(Entities are not PII — a company registration is a public-record
concept; the cross-principal read is intentional.)

### V-engine surfaces

| Surface | unauth | declarant | dev | admin | s2s (HMAC) |
|---|---|---|---|---|---|
| `POST /v1/verifications` | 401 ✓ | 201 ✓ — accepted (any authenticated principal can submit a snapshot directly; not gated to a role) | 201 ✓ | 201 ✓ | 201 ✓ if Bearer present |
| `GET /v1/verifications/{case_id}` | 401 ✓ | 200 ✓ — no owner check at handler | 200 ✓ | 200 ✓ | n/a |
| `POST /v1/internal/declaration-events` | 401 ✓ | 401 ✓ — HMAC required | 401 ✓ | 401 ✓ | 201 ✓ |

### Finding PRM-3 (Severity: HIGH)

**Title.** `POST /v1/verifications` accepts a free-form
`DeclarationSnapshot` body from **any** authenticated principal and
runs the full pipeline (including Anthropic API calls in Stage 5)
without authorisation check that the caller "owns" the declaration.

**Evidence.** `services/verification-engine/src/api/rest.rs:228-257`.
Handler is `submit_verification`; principal is extracted but unused
(`_principal` at line 231). The intent appears to be an internal
testing surface — the production path is the HMAC-protected
`/v1/internal/declaration-events`.

**Impact.**
1. A registered declarant can submit arbitrary snapshots to the
   pipeline, including snapshots that name other declarants. This
   causes inference-gateway calls (cost) and writes a
   `verification_cases` row that does not correspond to any real
   declaration.
2. The endpoint is rate-limited via the surrounding stack only if
   OPS-1 wires through (and OPS-1 was set up for the **declaration**
   service — V-engine's `services/verification-engine/src/api/rest.rs`
   does not show a rate-limit layer).

**Remediation.** Either (a) remove this endpoint in production
builds (gate on `cfg.is_dev()`), OR (b) add an admin-allowlist gate
so only registered V-engine operators can use the test surface, OR
(c) document the endpoint as deprecated and removed. Track via
**FINDING-PRM-3** ticket.

### Finding PRM-4 (Severity: MEDIUM)

**Title.** `GET /v1/verifications/{case_id}` is authenticated but not
case-owner-scoped — any authenticated principal can fetch any case.

**Evidence.** `services/verification-engine/src/api/rest.rs:260-267`.
Handler uses `_principal` (unused) and returns the full
`VerificationCase` including the embedded `DeclarationSnapshot`
(with beneficial-owner PII).

**Impact.** Same shape as PRM-2 — a declarant can enumerate every
verification case-id and pull the underlying declaration snapshot.

**Remediation.** Add a handler-level owner check (the case's
`declaration.declarant_principal` must match `principal.subject`)
or restrict to admin allowlist.

---

## 7.3 Public surface hardening

The public (unauthenticated) surfaces are:

PUB-1. `GET /healthz` — every service
PUB-2. `GET /readyz` — every service
PUB-3. `GET /metrics` — every service (in-cluster network only by deployment)
PUB-4. `GET /openapi.json` + `GET /docs` — declaration, person, entity
PUB-5. The declarant-portal SPA (HTML/JS bundle)
PUB-6. `GET /v1/audit/verify/{declaration_id}` — audit-verifier

### PUB-1, PUB-2 — health / readiness probes

- `healthz` returns `{"status":"ok"}` only
  (`services/declaration/src/api/rest.rs:297-306`).
- `readyz` performs a `SELECT 1` against the connection pool
  (`services/declaration/src/api/rest.rs:319-339`).
  - 200 if DB reachable; 503 if not.
  - No information leak — does not expose hostnames, DB version,
    connection-pool stats, etc.
- ✓ — no protected route reachable; nothing PII-shaped.

### PUB-3 — `GET /metrics`

- Prometheus text exposition (version 0.0.4). Metric names are
  deliberately not in the OpenAPI spec
  (`services/declaration/src/api/openapi.rs:35-49`).
- Labels are bounded enums (D18); none carry PII or user-supplied
  values. Verified by spot-check of label names: `result`
  (success|invalid|unavailable), `kind` (5-value enum),
  `subscriber` (operator-named, finite set), `lane`
  (green|yellow|red).
- ✓ — no PII; in-cluster network-policy is the deployment expectation
  (`services/declaration/src/api/rest.rs:213-216`).

#### Finding PUB-1 (Severity: LOW)

**Title.** No NetworkPolicy file in `infrastructure/kubernetes/`
that programmatically enforces "metrics accessible only from the
in-cluster Prometheus pod."

**Evidence.** The runbook (`docs/runbooks/observability-dashboards.md`)
documents the **deployment expectation**, but the audit could not
find a `NetworkPolicy` resource enforcing it.

**Remediation.** Ship a `NetworkPolicy` that allows ingress to port
8080 (or wherever `/metrics` is bound) only from the
`prometheus` service-account / pod-selector. Defence-in-depth on
top of the documented expectation.

### PUB-4 — `GET /openapi.json` + `GET /docs`

- Static OpenAPI spec describing the consumer-facing API.
- ✓ The spec **DOES** include `/v1/internal/*` endpoints. This is
  the intended documentation of the HMAC-protected service-to-service
  surface — see the `security: hmacSignature` tag.
- The Scalar UI is read-only HTML+JS hosted alongside the spec.

#### Finding PUB-2 (Severity: LOW)

**Title.** `/openapi.json` documents internal HMAC-only paths as
part of the public spec, increasing attacker reconnaissance surface.

**Evidence.** `services/declaration/src/api/openapi.rs` includes
`/v1/internal/outbox-dlq` and `/v1/internal/verification-outcomes` in
the `paths()` list. The Scalar UI at `/docs` renders these to anyone.

**Impact.** Not a vulnerability — the surface is HMAC-gated and the
spec documents the auth scheme. But it tells an unauthenticated
attacker the **shape** of the admin and webhook bodies, which they
can then attempt to spray. Low impact; defence-in-depth would split
the public spec from a separate internal-only spec served only on
the in-cluster network.

**Remediation.** Optional — author a separate
`/openapi-internal.json` for HMAC-only endpoints, served on an
internal listener. Backlog.

### PUB-5 — declarant-portal SPA

#### Bundle audit

The portal's TypeScript code references no admin endpoints; the
generated OpenAPI types file
(`applications/declarant-portal/src/generated/openapi.ts:39-168`)
does, however, contain the path strings for **every** declaration
surface — including `/v1/internal/outbox-dlq`,
`/v1/internal/outbox-dlq/{id}/replay`, and
`/v1/internal/verification-outcomes`.

#### Finding PUB-3 (Severity: MEDIUM)

**Title.** The declarant-portal's auto-generated OpenAPI types
contain the path strings of internal HMAC-only endpoints (which are
unreachable from the browser without the shared secret, but their
**existence** is visible in the bundle).

**Evidence.**

```
applications/declarant-portal/src/generated/openapi.ts:136:    "/v1/internal/outbox-dlq":
applications/declarant-portal/src/generated/openapi.ts:152:    "/v1/internal/outbox-dlq/{id}/replay":
applications/declarant-portal/src/generated/openapi.ts:168:    "/v1/internal/verification-outcomes":
```

**Impact.** As with PUB-2 — reconnaissance surface only; the
endpoints are HMAC-gated and a portal user cannot forge a signature.
But the bundle ships to every declarant browser, so the path strings
leak there.

**Remediation.** Either (a) filter the generated types to consumer
endpoints only (modify the codegen pipeline at
`tools/codegen/` or wherever `openapi-typescript` is invoked), OR
(b) accept the leak with documentation. Backlog.

#### Env-var audit

`applications/declarant-portal/src/App.tsx:12-13` reads
`VITE_DECLARATION_API_URL` only. This is the intended public-facing
config var. No other `VITE_*` reads in the bundle that would
leak server-only values. ✓.

The Ed25519 signing key is **in-memory only** by code review
(threat-model Gap G5 — `docs/security/threat-model.md:184`), not by
programmatic enforcement.

#### Branding + identity audit

- The portal is branded "RÉCOR" with French as the primary locale
  (`applications/declarant-portal/src/i18n.ts`); English is secondary.
  Appropriate for the Cameroonian audience.
- No "test" / "staging" / "dev" strings visible in the production
  bundle by spot-check (verify in CI on a fresh build).

### PUB-6 — `GET /v1/audit/verify/{declaration_id}`

- Public read-only. By design — anyone can independently verify the
  audit chain (`apps/audit-verifier/src/handlers.rs:42-94`).
- The response shape is `{ result: authentic|tampered|missing, ... }`
  plus the on-chain TxId for each entry. It does **NOT** return the
  declaration body, beneficial-owner PII, or the projection contents.
  Only the integrity verdict + on-chain receipt-hash + tx ids.
- ✓ no PII; deliberate public surface; consistent with D15 ("the
  audit chain is the source of truth, verifiable by any reader").

### Public-surface summary

| ID | Surface | Hardening verdict |
|---|---|---|
| PUB-1 | `/healthz`, `/readyz` | ✓ tight |
| PUB-2 | `/metrics` | ✓ tight content; deployment NetworkPolicy missing → FINDING-PUB-1 |
| PUB-3 | `/openapi.json`, `/docs` | content includes internal paths → FINDING-PUB-2 |
| PUB-4 | declarant-portal SPA | env-vars clean; internal paths in OpenAPI types → FINDING-PUB-3 |
| PUB-5 | audit-verifier `verify` | ✓ by design |

---

## 7.4 Privilege-escalation paths

### 7.4.1 JWT alg-confusion

**Status.** CLOSED (R-DECL-1).

**Evidence.** The verifier crate `recor-auth-oidc` rejects tokens
where `header.alg` does not match the JWK's `kty`/`alg` advertisement,
and refuses `none` algorithm tokens. Tests at
`packages/recor-auth-oidc/src/` enforce this; threat-model row
(`docs/security/threat-model.md:131`) cites R-DECL-1 closed.

### 7.4.2 Token replay (HMAC-channel)

**Status.** OPEN (Gap G2 — see DF-2 in Section 5).

**Evidence.** `services/declaration/src/api/internal.rs:122-164` does
not enforce an iat/nbf bound on the inbound envelope. Receiver-side
idempotency on `event_id` neutralises the *effect* of a replay (no
new event written), but a captured envelope is replayable
indefinitely until the HMAC secret is rotated.

**Compensating control.** Idempotency at `event_id` / `case_id`
(`services/declaration/src/application/record_verification_outcome.rs:79-99`)
plus aggressive 30-day secret rotation
(`docs/runbooks/hmac-secret-rotation.md`).

**Resolution path.** R-LOOP-2 (Kafka) carries iat enforcement +
exactly-once semantics. Tracked.

### 7.4.3 Confused deputies

A confused-deputy escalation would require:
- The service-to-service HMAC endpoint accepting a body that names a
  victim principal, OR
- The user-authenticated endpoint inferring a principal from a body
  field rather than the auth-middleware-set Principal.

Walk-through:

- **`POST /v1/internal/verification-outcomes`** accepts an envelope
  with `case_id, declaration_id, lane, ...`. It does NOT accept a
  field naming the writeback target — the target is the
  `declaration_id` carried in the payload, which is by definition
  the declaration the verifier is reporting on. No principal-spoofing
  surface.
- **`POST /v1/internal/declaration-events`** (V-engine inbound) carries
  a `DeclarationSubmittedV1Wire` whose `declarant_principal` is the
  *original signer*, used only for snapshot construction (not for
  authorisation). The pipeline does not grant any privilege based on
  this field.
- **`POST /v1/relay`** (worker-fabric-bridge) carries the same
  envelope as the D-side outbox; commit is identified by
  `event_id, declaration_id, receipt_hash, ts`. No principal-derived
  authorisation.
- **`POST /v1/persons/{id}/merge-into/{target}`** — admin-gated; the
  body has no principal field; `actor_principal` is taken from
  `principal.subject`
  (`services/person-service/src/api/rest.rs:399-402`). ✓ no confused
  deputy.

**Conclusion.** No confused-deputy path discoverable in the surface
inventory. (Caveat: gRPC surface at `services/declaration/src/api/grpc.rs`
was not fully walked — see PRM-5 below.)

### Finding PRM-5 (Severity: LOW)

**Title.** gRPC surface at `services/declaration/src/api/grpc.rs` was
not fully audited in this pass — confirm it does not accept a
principal field in any RPC body.

**Action.** Spot-check `auth_interceptor` and the
`SubmitDeclarationRequest` proto message in
`contracts/declaration.proto`; this audit pass was scoped to REST and
s2s. Move to Pass C or follow-up.

### 7.4.4 Race conditions

#### Idempotency-key race (TOCTOU on the submit replay path)

**Walk-through.** `services/declaration/src/api/rest.rs:406-454`:

1. `check_existing(key, principal)` — read
2. (concurrent request with same key fires step 1, gets `None` too)
3. `submit_usecase.execute(cmd)` — write event
4. `idempotency.record(...)` — write replay-record

Two concurrent requests with the same Idempotency-Key + same
principal + same body could both pass step 1 → step 3 → step 4. The
event-stream is protected by the `(declaration_id, aggregate_version)`
UNIQUE — so one of them wins and the loser gets
`RepositoryError::Conflict` (409).

✓ **No actual race** — the event-stream UNIQUE constraint is the
serialisation point. The idempotency-store TOCTOU is a benign duplicate
write (last-writer-wins on the replay record).

#### Admin allowlist race

`enforce_admin` reads the snapshotted `HashSet<String>` built at
router construction
(`services/declaration/src/api/rest.rs:151-157`). A mid-flight
`ADMIN_PRINCIPALS` env change does not take effect until the pod
restarts. ✓ — config change is atomic from the running service's
perspective.

#### HMAC rotation race

Tested explicitly:
`services/declaration/src/api/internal.rs:329-355`. Both old and
current secrets accepted during the window. ✓.

### 7.4.5 Default-permissive fallbacks

| Permissive fallback | Where | Gating |
|---|---|---|
| `OIDC_ISSUER_URL` empty → no JWT verification | `services/declaration/src/api/auth.rs:107-113` | Config refuses to start outside dev (`config.rs:282-284`) ✓ |
| `ADMIN_PRINCIPALS` empty → 503 disabled | `services/declaration/src/api/dlq.rs:247-256` | Fail-closed by design ✓ |
| `WRITEBACK_HMAC_SECRET` empty → 503 disabled | `services/declaration/src/api/internal.rs:132-140` | Fail-closed by design ✓ |
| `PERSON_SERVICE_URL` empty → cross-service check skipped | `services/declaration/src/application/submit_declaration.rs:60-64` | **NOT** gated in production — see FM-12 in Section 6 |
| `RATE_LIMIT_PER_MIN=0` → rate limiting disabled | `services/declaration/src/api/rate_limit.rs:79-81` | safe default; operator must opt in |
| `OTLP_ENDPOINT` empty → console-only tracing | `config.rs:26-29` | acceptable for dev; should be set in prod |
| `LOG_REDACTION=disabled` → loud `warn!` at startup but pass-through | `config.rs:140-148` | warn signal only; could miss |
| `CORS_ALLOWED_ORIGINS` empty → CORS disabled | `services/declaration/src/api/rest.rs:268-269` | safe — no cross-origin admitted |
| `KAFKA_BROKERS` empty → Kafka path disabled | `config.rs:194-199` | safe — falls back to HTTP relay |
| `AUTH_TRANSPORT=hmac` default | `config.rs:416-420` | safe v1 default; operator opts into mTLS |
| `BUNEC_FAIL_POLICY` not wired in v1 | port trait exists, real adapter pending R-VER-1 | accepted-risk gap |

### Finding PRM-6 (Severity: HIGH) — re-statement of FM-11

**Title.** `ENVIRONMENT=dev` combined with a configured OIDC issuer
does **NOT** raise a startup refusal — both auth paths (dev-header
**and** OIDC) become acceptable simultaneously.

**Evidence.** `services/declaration/src/config.rs:282-300` only
enforces "`environment != dev` AND `oidc_issuer_url.is_empty()`" as
a startup-refuse condition. The complementary case
"`environment == dev` AND `oidc_issuer_url` is set" is not refused.

**Impact.** In a misconfigured production deployment that sets both,
an attacker with knowledge of the dev-header convention can submit
declarations as any principal via `X-Recor-Dev-Principal`. This is
a complete authentication bypass.

**Remediation.** Tighten the gate: when `environment != dev`, also
refuse if `is_dev` would resolve true (defensive). Add an integration
test that sets `ENVIRONMENT=staging` and a real OIDC URL, then
verifies `X-Recor-Dev-Principal` returns 401.

### 7.4.6 Dev backdoors in prod

- `X-Recor-Dev-Principal` header path: `services/declaration/src/api/auth.rs:78`
  gates on `state.is_dev`. `is_dev` is sourced from `Config::is_dev()`
  which is sourced from `ENVIRONMENT == "dev"`. The header path is
  hard-gated; PRM-6 is the only escape.

- Dev-mode permissive logs (`LOG_REDACTION=disabled-for-dev` —
  `config.rs:140-148`): emits a startup `warn!` but pass-throughs
  PII. Acceptable in dev; loud-by-design in prod (the warn fires).

- Static JWT key fallback: NOT present in code (per `api/auth.rs:7-10`
  doctring: "an HS256-equivalent static key shortcut is NOT used; we
  accept a special `X-Recor-Dev-Principal` header that asserts the
  principal name"). ✓.

- Local-dev signing key in fixtures: lives in test fixtures only;
  not present in production builds.

### 7.4.7 Cross-service trust chain

| Step | Trust source | Risk |
|---|---|---|
| Portal → declaration | OIDC Bearer | declarant identity authentic |
| declaration → V-engine | HMAC (or mTLS) | secret-bearer authenticated |
| V-engine → Anthropic | API key | platform identity, no per-call PII gating |
| V-engine → BUNEC adapter | DB credentials (Postgres) or future mTLS | data-feed adapter |
| V-engine → declaration | HMAC writeback | secret-bearer |
| declaration outbox → worker-fabric-bridge | HMAC | secret-bearer |
| worker-fabric-bridge → Fabric peer (via shim) | gateway token + Fabric TLS cert | chain consortium identity |
| audit-verifier → Fabric peer | gateway token + Fabric TLS cert | read-only chain consortium identity |

Every cross-service trust hop is **strong-authenticated** (HMAC,
OIDC, mTLS, or DB cred). The platform has no anonymous internal API.
Note: the V-engine → Anthropic hop is per-platform-identity, not
per-declarant — there is no Anthropic-side audit of which declarant
caused a given prompt. This is the expected design.

---

## 7.5 Findings summary (Section 7)

| ID | Severity | Title |
|---|---|---|
| PRM-1 | LOW | Inconsistent admin-disabled status (entity-service 400 vs others 503) |
| PRM-2 | MEDIUM | `GET /v1/persons/{id}` and `/search` not principal-scoped — declarants can read any person record |
| PRM-3 | HIGH | `POST /v1/verifications` allows arbitrary snapshot submission by any authenticated principal — wastes Anthropic API + can spoof verification cases |
| PRM-4 | MEDIUM | `GET /v1/verifications/{case_id}` not owner-scoped — declarants can read any case |
| PRM-5 | LOW | gRPC surface in `services/declaration/src/api/grpc.rs` not fully walked this pass |
| PRM-6 (≡ FM-11) | HIGH | `ENVIRONMENT=dev` + configured OIDC does not refuse startup — auth bypass via dev-header |
| PUB-1 | LOW | No NetworkPolicy enforcing in-cluster-only `/metrics` access |
| PUB-2 | LOW | `/openapi.json` includes internal HMAC paths in the public spec |
| PUB-3 | MEDIUM | Declarant-portal bundle contains internal paths in generated OpenAPI types |

Carried over: FM-11 (≡ PRM-6), DF-2 (token replay window).

### Pre-launch must-fix (HIGH)

- PRM-3 — restrict `POST /v1/verifications` to admin or dev-only
- PRM-6 / FM-11 — tighten `ENVIRONMENT=dev` startup gate

### Pre-launch should-fix (MEDIUM)

- PRM-2 — principal-scope person reads
- PRM-4 — owner-scope verification-case reads
- PUB-3 — strip internal paths from portal bundle types

### Backlog (LOW)

- PRM-1, PRM-5, PUB-1, PUB-2

---

## 7.6 Cross-cutting observations

### Strengths

1. **D17 zero-trust is consistently honoured.** Every audited handler
   sources principal from `req.extensions::<Principal>()`, set
   exclusively by the auth-middleware. No handler I encountered
   reads a body-supplied principal.

2. **Admin allowlists are uniform.** The four services that have
   admin endpoints all read CSV via `Config::admin_principals_list()`
   and gate via `enforce_admin` or an inline equivalent. The empty
   allowlist is fail-closed (503) — *except* entity-service which
   returns 400 (PRM-1).

3. **HMAC rotation is operationally proven.** Dual-secret accept-both
   is unit-tested and runbook-documented.

4. **Cryptographic provenance carries through.** Every
   state-changing endpoint requires a fresh attestation over the
   canonical bytes, even for amend/correct.

### Areas requiring follow-up

1. **Person + verification-case reads are not principal-scoped**
   (PRM-2, PRM-4). The platform's audit logs will show "everyone
   read everyone else's records" — this is fine for a v1 pilot but
   not for a public registry.

2. **Test-shaped surfaces leaked into production** (PRM-3 — `POST
   /v1/verifications`). The HMAC-protected
   `/v1/internal/declaration-events` is the intended pipeline entry;
   the Bearer-protected variant should not exist in prod.

3. **Dev-header bypass when env=dev** (PRM-6). The most serious
   single-config-mistake risk in the codebase. Tighten config
   validation.
