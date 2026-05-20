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
| `declarant` | OIDC subject claim; not on any admin allowlist | A submitting beneficiary; a registered company representative |
| `admin` | OIDC subject claim AND appears on the per-service `ADMIN_PRINCIPALS` allowlist | Sovereign back-office operator; on-call engineer |
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

| Method + path | unauthenticated | declarant | admin | internal-service | prometheus-scraper |
|---|---|---|---|---|---|
| `GET /healthz`, `/readyz` | Y | Y | Y | Y | Y |
| `GET /openapi.json`, `/docs` | Y | Y | Y | Y | Y |
| `GET /metrics` | n (network-policy 403) | n | n | n | Y |
| `POST /v1/declarations` | n (401) | Y | Y | — | — |
| `GET /v1/declarations/{id}` | n (401) | Y if `declarant_principal == caller` else n (404 — see FIND-004 closure note) | Y | — | — |
| `POST /v1/declarations/{id}/supersede` | n (401) | Y if `declarant_principal == caller` else n (404) | Y | — | — |
| `POST /v1/declarations/{id}/amend` | n (401) | Y if `declarant_principal == caller` else n (404) | Y | — | — |
| `POST /v1/declarations/{id}/correct` | n (401) | n (403) — admin only | Y | — | — |
| `GET /v1/dlq` | n (401) | n (403) | Y | — | — |
| `POST /v1/dlq/{id}/replay` | n (401) | n (403) | Y | — | — |
| `POST /v1/internal/verification-outcomes` | — | — | — | Y (HMAC + `iat` window) | — |

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
