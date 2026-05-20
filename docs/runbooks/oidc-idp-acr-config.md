# Runbook — OIDC IdP `acr` claim configuration (TODO-020)

**Doctrine reference:** D14 (fail-closed), D17 (zero trust at every
network boundary). **Standards reference:** NIST SP 800-63A
(Identity Assurance Level), 800-63B (Authentication Assurance Level),
FATF Recommendation 24 c.24.6 IO.5 ("identity verification of the
submitter"), CJEU C-37/20 + C-601/20 (post-Sovim balancing test).

The platform refuses state-changing requests whose verified OIDC
token's `acr` claim resolves below the endpoint minimum. This runbook
covers the operator-side wiring needed to make the issuer advertise an
`acr` claim that maps onto the platform's three-rung IAL ladder.

## The ladder

The verifier (`packages/recor-auth-oidc/src/lib.rs`) resolves the
incoming `acr` claim into one of three discrete levels:

| Level | NIST URI form | Numeric form | Short form | Endpoint policy minimum |
|---|---|---|---|---|
| **IAL1** | `http://idmanagement.gov/ns/assurance/ial/1` | `"0"`, `"1"` | `"IAL1"` | the fail-closed floor; sufficient for `GET` endpoints only |
| **IAL2** | `http://idmanagement.gov/ns/assurance/ial/2` | `"2"` | `"IAL2"` | submission (`POST /v1/declarations`) and amendment |
| **IAL3** | `http://idmanagement.gov/ns/assurance/ial/3` | `"3"` | `"IAL3"` | admin endpoints (`correct`, `supersede`, `dissolve`, `merge-into`, `dlq/replay`) |

Per-endpoint minimums are tabulated in
[permission-matrix.md](../security/permission-matrix.md#todo-020-identity-assurance-level-ialaal-gate).

## Keycloak (the platform's default IdP)

Keycloak's "Authentication flow" + "Required Action" surface drives
which `acr` value lands on the token.

1. **Define the LoA → ACR map.** In `Realm Settings → Tokens →
   Authentication flows`, create a `Level of Authentication` step
   that emits `acr=2` for the standard flow (password + email
   verified) and `acr=3` for the step-up flow (password + WebAuthn /
   in-person enrolment).
2. **Advertise `acr_values_supported`.** In `Realm Settings → Login
   → Login screen customization → Display Auth Levels`, set the
   discovery document to advertise `"acr_values_supported": ["1",
   "2", "3"]`. Verify by curl-ing
   `https://<keycloak>/realms/recor/.well-known/openid-configuration`
   and checking the response body.
3. **Bind step-up to the privileged client.** The declarant-portal's
   OIDC client requests `acr_values=2`; the operator-admin client
   requests `acr_values=3` and refuses tokens whose `acr` is below 3.
4. **Refresh the JWKS.** No action required — the verifier picks up
   the new claim once the IdP signs a token with it.

## Auth0

1. **Add a Rule.** "Rule → Empty rule"; populate the rule body so the
   `acr` claim is mapped from the user's MFA enrolment level:
   ```javascript
   function (user, context, callback) {
     const acr =
       (user.app_metadata && user.app_metadata.acr) ||
       (user.multifactor && user.multifactor.length > 0 ? "2" : "1");
     context.idToken["acr"] = acr;
     context.accessToken["acr"] = acr;
     callback(null, user, context);
   }
   ```
2. **Verify on the discovery document.** Auth0's discovery endpoint
   includes `acr_values_supported` automatically once the rule lands.

## Okta

Okta's "Authentication Policies" tab exposes per-application IAL
settings via the `nist-acr-mapping` Custom Authenticator. Set:
- Default policy → `acr=2`
- Privileged-operator policy → `acr=3`
- `acr_values_supported` is published on the discovery document
  automatically once the mapping is in place.

## Verifying the wiring end-to-end

1. Acquire a token via the operator's normal flow.
2. Decode the JWT payload (any `jwt.io`-style inspector, locally):
   ```bash
   echo "$TOKEN" | cut -d. -f2 | base64 -d 2>/dev/null | jq .acr
   ```
3. Issue a `POST /v1/declarations` request bearing the token. If
   `acr` is `"2"` (or one of its synonyms), the submission succeeds.
   If `acr` is `"1"`, the response is:
   ```json
   {
     "error": {
       "kind": "forbidden",
       "message": "authorization denied: insufficient_assurance"
     }
   }
   ```
   with status `403 Forbidden`.
4. The same token issued to `POST /v1/declarations/{id}/correct`
   returns 403 unless `acr` resolves to IAL3.

## Failure modes and remediation

| Symptom | Cause | Remediation |
|---|---|---|
| Every declarant request returns 403 `insufficient_assurance` | The IdP is not emitting an `acr` claim → verifier defaults to IAL1 | Follow the per-IdP step 2 above; verify `jq .acr` returns a string |
| Admin requests return 403 but submission succeeds | The IdP emits `acr=2` but not `acr=3`; the operator's MFA enrolment is incomplete | Complete the step-up enrolment on the operator's account; verify with `jq .acr` |
| Discovery document `acr_values_supported` is missing | The IdP did not publish the claim in `.well-known/openid-configuration` | Follow the per-IdP step that adjusts the discovery shape; restart the IdP if its config is cache-bound |

## Cross-reference

- Per-endpoint matrix:
  [`docs/security/permission-matrix.md`](../security/permission-matrix.md#todo-020-identity-assurance-level-ialaal-gate)
- ACR parser + tests:
  [`packages/recor-auth-oidc/src/lib.rs`](../../packages/recor-auth-oidc/src/lib.rs)
- Per-handler enforcement:
  [`services/declaration/src/api/rest.rs`](../../services/declaration/src/api/rest.rs)
- TODOS.md entry:
  [`TODOS.md` § TODO-020](../../TODOS.md)
