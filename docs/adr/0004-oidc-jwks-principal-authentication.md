# ADR-0004: OIDC + JWKS for principal authentication

**Status:** Accepted (since 2026-05-11)
**Decision-makers:** @recor/architect-team, @recor/security-team
**Date:** 2026-05-11 (commit `dd8c018`, PR #45); 2026-05-11
(commit `6649034`, PR #51 — hardening); 2026-05-11 (commit
`f40594b`, PR #54 — extraction to shared crate)

## Context

The Declaration service and the Verification Engine both expose
HTTP endpoints that require an authenticated principal. The
Declarant Portal submits beneficial-ownership declarations on
behalf of a human (typically a corporate officer or compliance
officer). Internal services and admin tooling also speak to both
APIs. Per Doctrine D17 ("zero trust at every network boundary")
every request that reaches a protected handler must arrive with a
verified principal.

At the time of decision (PRs #45, #51), each service carried an
`src/api/oidc.rs` placeholder that *peeked* at JWT claims without
verifying the signature. This was filed as the open R-DECL-1
ticket; it was the first real "production-grade from the first
commit" debt to repay (Doctrine D12).

The realistic options for principal authentication:

- **OIDC Bearer tokens with JWKS-based RS/ES/EdDSA verification.**
  The industry-standard answer; libraries are mature; multiple
  identity providers (Auth0, Keycloak, Okta, sovereign IdPs the
  consortium may stand up) plug in without code change.
- **Custom HS256 with a server-managed secret.** Simple but
  hands every service a shared secret it must protect, and is
  the target of the well-known JWT algorithm-confusion attack.
- **API keys.** Long-lived bearer-equivalents with no audience or
  expiry. Adequate for service-to-service but not for human
  principals.
- **mTLS only.** Works for service-to-service; does not work for
  human declarants speaking through a browser.

A key constraint specific to RÉCOR: the consortium is multi-tenant
in the sense that different jurisdictions (and the consortium's own
internal directory) may host different IdPs. Whatever we picked had
to support multiple issuers eventually and let each service be
configured for its own audience.

## Decision

We chose **OIDC Bearer tokens with JWKS-based signature verification**.
The implementation lives in `packages/recor-auth-oidc/src/lib.rs`
(the shared crate, extracted under R-AUTH-1, PR #54); both
`services/declaration` and `services/verification-engine` consume it
via `src/api/oidc.rs` re-exports.

Specifics:

- **Discovery.** The verifier discovers the issuer's JWKS endpoint
  via `${issuer}/.well-known/openid-configuration` at startup.
- **JWKS caching.** The JWKS is fetched on first discover() call
  and re-fetched at most every 300s (configurable). The fetch is
  shared via an `RwLock<JwksCache>`.
- **JWKS pre-warm.** `discover()` calls `refresh_jwks()` at the
  end so the cache is hot before the first request (R-AUTH-3). A
  dead issuer at startup fails the service rather than letting it
  serve 500s.
- **Signature + claim validation.** Every request decodes the JWT
  header, looks up the `kid` in the JWKS, verifies the signature
  with `jsonwebtoken::decode`, and validates the `iss`, `aud`,
  `exp`, and `nbf` claims with 30-second leeway for clock skew.
- **HMAC algorithms refused.** RS256, RS384, RS512, ES256, ES384,
  EdDSA are accepted. HS256/HS384/HS512 are refused at the
  algorithm filter. This neutralises the well-known
  algorithm-confusion attack (swapping `alg: RS256` for `alg:
  HS256` and signing with the RSA public key as the HMAC secret).
- **Configurable subject claim** (R-AUTH-2, PR #51). The principal
  subject is read from a configurable claim — `OIDC_SUBJECT_CLAIM`,
  default `sub`. Production deployments may want
  `preferred_username`, `email`, or a custom claim per IdP.
  Missing claim returns 401 `SubjectClaimAbsent`.
- **Verified-token LRU cache** (R-AUTH-4, PR #51). Successful
  verifications memoise by raw token string. Cache entries expire
  at the token's own `exp`. Bounded 1024 entries. A token
  presented in a tight loop verifies once, then hits the cache.
  Revocation follows token-expiry semantics — there is no
  separate revocation signal.
- **Config-side fail-closed.** Both services refuse to start when
  `ENVIRONMENT != dev` and `OIDC_ISSUER_URL` is empty.
  `OIDC_AUDIENCE` is required whenever `OIDC_ISSUER_URL` is set.
  A production deployment cannot land in the "no verifier" state.
- **Dev override.** Outside production, an `X-Recor-Dev-Principal`
  header asserts the principal's subject. Gated by `Config::is_dev()`
  and refused otherwise. Smoke scripts and local testing use this
  path.

The auth middleware (`src/api/auth.rs` in each service) injects
the resolved `Principal` into request extensions; protected
handlers extract it via a `RequirePrincipal` extractor.

## Consequences

### Positive

- Standard wire protocol: every modern IdP speaks OIDC. The
  consortium can swap IdPs (Keycloak today, sovereign IdP
  tomorrow) without touching service code.
- Algorithm-confusion is structurally impossible. We never accept
  an HMAC algorithm; the attack has nothing to land on.
- JWKS rotation is automatic. When the issuer rotates its signing
  key, the cache refresh picks up the new key within 300 seconds.
- Pre-warm + LRU make the steady-state per-request cost ~0. Cold
  start pays the JWKS discovery once; warm requests with a cached
  token hit a HashMap.
- The shared crate (R-AUTH-1) means one implementation, one set of
  hardening tests, one place to fix a bug. Both services consume
  the same Rust code path.
- The auth middleware is testable in isolation. We unit-test the
  verifier with synthetic JWKS + JWTs; we integration-test the
  middleware with a fake issuer.

### Negative

- Issuer availability is a dependency. If the configured OIDC
  issuer is down, JWKS refresh fails and *new* `kid`s cannot be
  verified. Cached `kid`s continue to verify until cache TTL
  expires. The "OIDC issuer outage" runbook is on the DOC-3
  backlog; the operating posture is fail-closed.
- Every service must be configured with a matching `OIDC_AUDIENCE`.
  Misconfiguration is a startup-time refuse-to-start, which is
  the correct fail-closed posture but adds an environment value
  to manage.
- The verified-token LRU memoises the JWT string, including the
  signature. The cache must not leak via process inspection (it
  does not; it lives in `Mutex<LruCache<String, _>>`), but
  operators should know the cache exists.

### Neutral

- We do not implement a token-revocation feed. Token expiry is
  the only revocation signal. For short-lived tokens (e.g. 1h)
  this is adequate; for long-lived tokens it is not. Operating
  posture is to issue short-lived tokens.
- Multi-issuer support is a configuration matter, not a code
  matter. Today each service is configured with one issuer +
  audience. A second issuer would require a verifier-list and
  a header-based or claim-based routing rule.

## Alternatives considered

### Custom HS256 with a server-managed shared secret

Rejected. HS256 means every service that verifies the token has
the secret used to mint it, which is also the secret an attacker
could use to mint a token. Algorithm confusion (swapping `RS256`
to `HS256` against an `alg`-permissive verifier) is the canonical
JWT attack. Removing HMAC from the accepted-algorithm list
eliminates the entire attack class.

### API keys

Rejected for human principals. API keys have no audience binding,
no expiry, and no signature-rotation primitive. They are
long-lived bearer secrets; a leaked key cannot be revoked without
operator action. API keys remain reasonable for *specific*
service-to-service scenarios where mTLS is overkill; they are not
the right primitive for the human-principal surface of the
platform.

### mTLS-only

Rejected. mTLS works between services and is on the roadmap as
`R-LOOP-3` (SPIFFE+mTLS via SPIRE) for the D↔V channel. It does
not work for human declarants speaking through a browser: client
certificates have a famously bad UX, are non-portable across
devices, and have no good story for "I lost my laptop, give me a
new credential." OIDC handles the human case; mTLS will handle
the service-to-service case.

### Roll-our-own JWT verifier

Rejected. Production-grade JWT verification is well-trodden
ground with multiple library implementations (`jsonwebtoken` for
Rust, `node-jsonwebtoken` for Node, etc.). Writing our own
signature verification would replicate work the library does well,
with worse review surface for security defects.

## References

- Commit `dd8c018` — R-DECL-1 initial OIDC verification (PR #45)
- Commit `6649034` — R-AUTH-2/3/4 hardening (PR #51)
- Commit `f40594b` — R-AUTH-1 shared `recor-auth-oidc` crate
  (PR #54)
- `packages/recor-auth-oidc/src/lib.rs` (top-of-file doc, ~500
  lines of implementation)
- `services/declaration/src/api/auth.rs` (middleware)
- `services/declaration/src/api/oidc.rs` (re-export of the shared
  crate)
- Architecture V4 P13 § auth surface
- Doctrines D14 (fail-closed), D17 (zero trust), D18 (no secrets)
- Follow-ups: `R-LOOP-3` SPIFFE+mTLS for service-to-service (the
  human path stays on OIDC); future ticket for multi-issuer
  support if/when the second IdP onboards
