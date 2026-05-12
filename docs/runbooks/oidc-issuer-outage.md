# Runbook — OIDC issuer outage

The OIDC issuer (the platform's identity provider) is unreachable or
returning broken responses. Every authenticated endpoint on declaration
and verification-engine depends on OIDC discovery + JWKS verification.
This runbook walks the fail-open vs fail-closed decision tree.

## Trigger

Any of:

- Spike in 401 responses across the declaration service AND the
  verification-engine simultaneously (the shared dependency is OIDC)
- `kubectl -n recor exec deploy/declaration -- curl -sf
  "${OIDC_ISSUER_URL}/.well-known/openid-configuration"` fails or hangs
- Logs across services show `OIDC verifier error: ... JWKS
  unreachable` or `OIDC discovery failed`
- The IdP team has confirmed an outage on their side
- An alert `RecorOidcDiscoveryFailing` fires

The triage entry point is [oncall-triage-tree](oncall-triage-tree.md)
§ "D. Auth outage."

## The decision tree (read this BEFORE running commands)

```
                  IS OIDC DOWN?
                       │
                       ▼
            Confirm via Step 1 below.
                       │
              ┌────────┴────────┐
              ▼                 ▼
        DOWN < 60s        DOWN ≥ 60s
              │                 │
              ▼                 ▼
       Wait. JWKS         Real outage.
       cache covers       Go to Step 2.
       short blips.
                                │
                       ┌────────┴────────┐
                       ▼                 ▼
                  PRODUCTION         NON-PROD
                       │                 │
                       ▼                 ▼
                  FAIL CLOSED.      Acceptable to fail open
                  Do NOT toggle     by setting
                  the verifier      OIDC_ISSUER_URL="" and
                  off. See "Why     restarting — staging
                  fail-closed in    only. See "Why not
                  prod" below.      production" below.
                       │
                       ▼
              Mitigations:
              - Page IdP team
              - Surface "auth degraded"
                banner on portal
              - Wait for IdP recovery
              - Engage incident
                response
```

## Why fail-closed in production

The platform's auth posture (Architecture V1 P5; D14 fail-closed; D17
zero trust) commits to refusing requests over which the platform
cannot establish provenance. An attacker who can take the IdP offline
should not also be able to bypass the verifier.

The Declaration service's auth layer is the binding code:
`services/declaration/src/api/auth.rs` enforces that bearer-token
requests with no verifier configured are rejected (the `D14
fail-closed` comment in that file). The verifier MUST be configured
in production; the only way to disable it is by setting
`OIDC_ISSUER_URL=""` AND `ENVIRONMENT=dev`, and the config layer
refuses to start production if `OIDC_ISSUER_URL` is unset (see
`services/declaration/CLAUDE.md` § "Active development context" and
`services/declaration/src/config.rs`).

So in production, "fail-open" is not a runtime knob; it would require
re-deploying with intentionally weakened config. **Do not do this.**

## Why not production (for fail-open)

Fail-open in production is equivalent to a temporary unauthenticated
mode. Any actor who can reach the API can submit declarations or
queries with arbitrary `sub` claims (the principal is no longer
verified). The platform's audit chain depends on principal provenance
being meaningful; fail-open invalidates audit for the entire fail-open
window.

Architecture commits this to D14 / D17. The escalation path is "fix
the IdP / fail over to the secondary IdP," not "weaken the verifier."

## Prerequisites

- `kubectl` against the production cluster
- Contact for the IdP operations team (paged via the IdP team's
  on-call surface — see the consortium on-call handbook)
- Read access to the IdP's status page (URL is published in the
  on-call handbook; not committed here per D18)

## Procedure

### Step 1 — Confirm the IdP is actually down

Probe discovery from a service pod (not your laptop — pod network
egress may differ from yours):

```bash
kubectl -n recor exec deploy/declaration -- bash -c '
  echo "OIDC_ISSUER_URL=${OIDC_ISSUER_URL}"
  curl -sSf -m 10 "${OIDC_ISSUER_URL}/.well-known/openid-configuration" \
    | jq -r ".issuer, .jwks_uri, .token_endpoint"
'
```

Expected on a healthy IdP: three URLs printed, exit 0.

Failure modes:

- `curl: (6) Could not resolve host` — DNS broken; check
  `kubectl -n recor exec deploy/declaration -- nslookup <host>`
- `curl: (28) Operation timed out after 10000` — TCP unreachable;
  check the IdP's network status
- `curl: (60) SSL certificate problem` — TLS cert issue; could be
  expired cert on the IdP
- `200 OK but JSON parse fails` — IdP misconfigured; rare

Cross-check with the JWKS endpoint specifically (some IdPs fail JWKS
while discovery succeeds):

```bash
kubectl -n recor exec deploy/declaration -- bash -c '
  JWKS_URI=$(curl -sf -m 10 "${OIDC_ISSUER_URL}/.well-known/openid-configuration" | jq -r .jwks_uri)
  echo "JWKS_URI=${JWKS_URI}"
  curl -sSf -m 10 "${JWKS_URI}" | jq ".keys | length"
'
```

Expected: a number ≥ 1 (the IdP serves at least one signing key). 0
keys, or curl failure, → JWKS is broken.

### Step 2 — Determine the duration

Check the alert's start time and your current time. If the outage is
< 60 s, the JWKS cache TTL in the verifier covers it transparently
(see `services/declaration/src/api/oidc.rs` — JWKS fetching is
TTL-cached). Watch and wait; do not act.

If the outage is ≥ 60 s, treat as a real outage. The JWKS cache TTL
is finite; once it expires, the verifier cannot refresh keys and
401-rejects everything.

### Step 3 — Surface the degradation

Post in `#oncall-recor` with the IdP outage facts. Then either:

**Option A — Fail-over to secondary IdP** (preferred if the platform
is configured for it). The Architecture V5 P19 § Auth chapter
documents the secondary-IdP commitment. Today (2026-05-12), secondary
IdP wiring is **TBD — depends on R-AUTH-FAILOVER**. Until that ticket
ships, Option A is unavailable.

**Option B — Wait for IdP recovery** (the default until R-AUTH-FAILOVER
lands). Engage the IdP operations team, monitor their status, post
updates every 15 min.

### Step 4 — Tell users what's happening

The declarant portal should surface a banner: "Authentication system
degraded — submissions paused, your draft is preserved locally." The
banner is owned by R-PORT-2 (`docs/PRODUCTION-TODO.md`); until it
lands, this step is **TBD — depends on R-PORT-2**.

In the absence of an in-app banner, post on the public status page
(URL in on-call handbook).

### Step 5 — Stop accepting writes (only if specifically required)

If the IdP outage is prolonged AND the platform is configured for
prolonged-outage behaviour that involves disabling submissions (the
default architectural position is **NO**; submissions remain
rejected by the auth layer but the API remains up to return clear
401s), then:

```bash
# Scale the writer deployments to zero. This is more drastic than the
# 401-rejection default, and is reserved for scenarios where the IdP
# being down also means downstream consumers cannot process
# submissions reliably anyway.
kubectl -n recor scale deploy/declaration --replicas=0
```

Step 5 is rarely correct. The default behaviour (return 401, let
clients retry when auth comes back) is intentional and is what the
fail-closed doctrine implies. Only execute Step 5 with explicit SRE
lead sign-off.

### Step 6 — Recovery

When the IdP team reports recovery, re-probe (Step 1). Confirm:

- Discovery endpoint returns 200 + valid JSON
- JWKS returns ≥ 1 key
- A test bearer token (issued post-recovery) verifies successfully —
  ask the IdP team for a service-account token or use a real human
  login from a test account

The verifier will pick up the recovered IdP on the next JWKS fetch
(within the cache TTL). No service restart is required. To accelerate,
restart the deployments:

```bash
kubectl -n recor rollout restart deploy/declaration deploy/verification-engine
```

If Step 5 was executed and the writer deployments were scaled to 0,
scale them back to the normal count:

```bash
kubectl -n recor scale deploy/declaration --replicas=3
```

### Step 7 — Post-incident

Per [incident-response-template](incident-response-template.md).

Action items typically include:

- If JWKS cache TTL was the difference between "transparent" and
  "outage," consider extending the TTL (with the security trade-off
  noted in the ADR — longer TTL = slower recovery from key
  compromise)
- If R-AUTH-FAILOVER is not yet scoped, escalate its priority
- If the platform's auth-degraded UX was missing, file or chase
  R-PORT-2

## Verification

OIDC is recovered and the platform is healthy when:

- Step 1's probe succeeds from at least one production service pod
- New requests with fresh bearer tokens verify successfully (sample:
  one canary request from a known-good token, expect 2xx)
- The 401 spike has resolved in Grafana
- DLQ rows that dead-lettered with OIDC-related error strings during
  the outage have been replayed per
  [dlq-inundation](dlq-inundation.md) Phase C (auth failures during
  the window cause D ↔ V replication failures too)
- If a degraded-UX banner was shown, it has been removed
- A post-mortem PR is open

## Rollback

The runbook does not modify configuration in production; nothing to
roll back unless you executed Step 5 (scaled writers to 0).

Step 5's rollback is "scale back up" — Step 6's `kubectl scale
--replicas=3`. The data path is intact; no state was destroyed.

If you ALSO toggled `OIDC_ISSUER_URL=""` in a desperate attempt to
fail-open (against doctrine; do not do this), revert that env via
`kubectl -n recor edit deploy/<service>` and remove the override. The
config layer's production guard will refuse to start the service if
the override is left; that is intentional, fail-closed behaviour. See
`services/declaration/src/config.rs` for the binding code.

## Related runbooks

- [oncall-triage-tree](oncall-triage-tree.md)
- [dlq-inundation](dlq-inundation.md)
- [hmac-secret-rotation](hmac-secret-rotation.md)
- [rollback-deployment](rollback-deployment.md)
- [observability-prod-stack](observability-prod-stack.md)
- [incident-response-template](incident-response-template.md)
