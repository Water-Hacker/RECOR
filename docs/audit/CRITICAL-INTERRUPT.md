# CRITICAL audit findings — Pass B

Two HIGH findings discovered in Pass B that should block launch
until closed. Both are authentication / authorisation defects.

## PRM-3 — `POST /v1/verifications` admits arbitrary snapshots

**Severity:** HIGH
**Found in:** Pass B, Section 7 (`docs/audit/05-permissions.md`)
**File:** `services/verification-engine/src/api/rest.rs:228-257`

The handler is OIDC-authenticated but does not check that the caller
is authorised to verify a given declaration. Any registered declarant
can submit arbitrary `DeclarationSnapshot` bodies, causing:

1. Anthropic API calls in Stage 5 (paid).
2. `verification_cases` rows with no corresponding real declaration.
3. Potential spoofing of "this declaration was verified Green/Yellow/Red"
   if downstream code trusts the case record.

**Action.** Gate the endpoint on admin or dev-only, OR remove it.

---

## PRM-6 (≡ FM-11) — `ENVIRONMENT=dev` + configured OIDC accepts both auth paths

**Severity:** HIGH
**Found in:** Pass B, Section 6 (`docs/audit/04-failure-modes.md`)
and Section 7 (`docs/audit/05-permissions.md`)
**File:** `services/declaration/src/config.rs:282-300`

The config-startup gate only refuses to start when
`environment != dev AND oidc_issuer_url.is_empty()`. It does NOT
refuse when `environment == dev AND oidc_issuer_url` is set (i.e.,
a production-style deployment with a stray `ENVIRONMENT=dev` env
var). In this state, both auth paths become acceptable, and an
attacker can authenticate via `X-Recor-Dev-Principal: anyone` —
**complete authentication bypass**.

**Action.** Tighten config validation: refuse to start when
`environment != "production"` and any "dev-only" backdoor is
reachable. Add a regression integration test.

---

## Carry-over from Section 5

DF-2 (HIGH) — D↔V HMAC channel has no iat-bound replay window. Open
until R-LOOP-2 (Kafka cutover) lands. Compensating control: 30-day
secret rotation.
