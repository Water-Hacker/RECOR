# ADR-0005: Per-channel HMAC secrets with dual-secret rotation

**Status:** Accepted (since 2026-05-12); supersedes the single-shared-
secret arrangement that existed between PR #38 and PR #58
**Decision-makers:** @recor/architect-team, @recor/security-team
**Date:** 2026-05-12 (commit `fe70ddd`, PR #58)

## Context

The D↔V loop (see ADR-0003) authenticates cross-service requests with
HMAC-SHA256 over the raw request body. Each side (the signer's relay
task and the verifier's HTTP endpoint) holds a shared secret; on each
request the signer signs the body, the verifier re-computes the HMAC
and compares constant-time. This is the v1 transport authentication
primitive — Kafka + SPIFFE/mTLS (`R-LOOP-2`, `R-LOOP-3`) replaces it
later.

The arrangement that shipped in PRs #38 and #39 used **one** shared
secret across both channels and both directions. The four config
slots — D's relay-side, V's inbound-side, V's relay-side, D's
writeback-side — were all populated from a single
`RECOR_HMAC_SECRET` env in `docker-compose.integration.yaml`. The
production handover runbook called out two problems with this:

1. **Blast radius of a compromise.** A leak of the shared secret
   broke integrity for both directions simultaneously. An attacker
   who learned the secret could forge D→V envelopes and V→D
   envelopes equally.
2. **Rotation requires downtime.** Rotating the single secret meant
   that, for the window between "signer flips" and "verifier
   reloads", the verifier saw signatures from the new secret with
   only the old secret loaded — every in-flight envelope was
   rejected. The relay would retry; eventually verifier and signer
   would converge; the dispatch_attempts counters would have noise.
   The semantics were correct (eventually consistent) but the
   operational signature looked like an incident.

Doctrine D17 ("zero trust at every network boundary") and Doctrine
D14 ("fail-closed at integration boundaries") both push toward
minimising the blast radius of a single credential. The compromise
case is the one we have to design for.

The realistic options at the time of decision:

- **Per-channel secrets + a dual-secret rotation window.** Two
  independent secrets (one per channel direction), each with the
  verifier accepting "current + optional old" for the duration of
  a rotation.
- **Stay with the single shared secret.** Cheaper to operate;
  worse blast radius and worse rotation UX.
- **Switch to mTLS now (skip the HMAC rotation work).** Bigger
  change; depends on SPIRE deployment per `R-LOOP-3`; deferred.
- **JWT for service-to-service.** Token-based; requires an issuer
  service; heavier than the problem warrants.

## Decision

We chose **per-channel HMAC secrets with a dual-secret rotation
window**. Implemented in PR #58 (commit `fe70ddd`, "R-LOOP-4-ROT").

Specifics:

- **Two distinct secrets**, one per channel direction:
  - `RECOR_D_TO_V_HMAC` — signs D→V envelopes (D's relay) and
    verifies them (V's `POST /v1/internal/declaration-events`).
  - `RECOR_V_TO_D_HMAC` — signs V→D envelopes (V's relay) and
    verifies them (D's `POST /v1/internal/verification-outcomes`).
  A compromise of either secret leaks only that direction.
- **Optional `*_OLD` envs.** Each verifier accepts an additional
  "still-valid old" secret simultaneously:
  - V-engine: `INBOUND_HMAC_SECRET_OLD`
  - Declaration: `WRITEBACK_HMAC_SECRET_OLD`
  Empty (default) means rotation is not active; only the current
  secret is accepted.
- **`verify_hmac_with_rotation()` primitive.** Both services'
  `src/api/internal.rs` carry the same helper: try the current
  secret first; if `old_secret` is non-empty and the current
  didn't match, also try the old. On a successful match against
  the old secret a `warn!` tracing event fires so operators see
  "rotation in progress" in logs and can confirm the rotation
  is progressing.
- **Rotation procedure** (zero downtime). Documented step-by-step
  in `docs/runbooks/hmac-secret-rotation.md`:
  1. Generate the new secret (`openssl rand -hex 32`).
  2. On the **verifier** side, snapshot the current secret into
     `*_HMAC_SECRET_OLD` and set the current env to the new value.
     Both old and new now verify.
  3. On the **signer** side, flip to the new secret. In-flight
     requests signed with the old secret still verify against
     `*_HMAC_SECRET_OLD`.
  4. Wait the drain window: `(poll_interval × max_attempts) +
     safety_margin`. With default `poll_interval = 5s`,
     `max_attempts = 12`, that is ~60s + 30s = **90 seconds**.
  5. Clear the `*_OLD` env on the verifier. Rotation complete.
- **Unit-test coverage.** Each service adds four rotation-specific
  tests:
  - rotation off: only current secret accepted
  - rotation on: both current AND old accepted
  - third-party signature still rejected during rotation
  - tampered payload still rejected during rotation
- **Default is fail-closed.** Empty `*_OLD` env means rotation is
  not active and only the current secret is accepted. Operators
  must explicitly opt in to the "two secrets valid" window; there
  is no accidental acceptance of a stale secret.

## Consequences

### Positive

- Blast radius halved. A leak of `RECOR_D_TO_V_HMAC` does not
  enable forgery of V→D envelopes, and vice versa.
- Rotation is zero-downtime. The verifier's "both old and new
  accepted" window covers the gap between the operator setting
  the new secret on the verifier and the signer.
- Operational confirmation is built in. The `warn!` log when the
  old secret matched gives the operator a tangible signal that
  the rotation window is still consuming in-flight requests.
  Once the warns stop, the drain has completed.
- Symmetric across both services. The same `verify_hmac_with_rotation`
  helper, the same four unit tests, the same env-name convention
  (`INBOUND_*` and `WRITEBACK_*`) on each side.
- Compatible with the existing DLQ. Rows that exhaust
  `max_attempts` move to the DLQ regardless of secret state; the
  rotation work did not touch the DLQ path.

### Negative

- Two secrets to manage in operator tooling. Vault paths,
  Kubernetes ExternalSecrets, and the integration-smoke `.env`
  generation all have to know about both. Today the
  integration-smoke scripts handle this via two
  `openssl rand -hex 32` calls; production tooling
  (Vault / Sealed Secrets) is a deployment-time concern.
- Operators must remember to clear `*_OLD` after the drain
  window. Leaving the old secret in place keeps the previous
  credential valid indefinitely, defeating the rotation. The
  runbook calls this out as Step 5; the warn-log signal helps
  but it is operator discipline that closes the loop.
- A misconfigured `*_OLD` (e.g. left pointing at a current
  secret used elsewhere) silently widens the verifier's
  accepted-secret set. The constraint is operational, not
  enforced in code.

### Neutral

- HMAC is an interim primitive. The whole HMAC arrangement
  retires when SPIFFE+mTLS lands (`R-LOOP-3`). The
  per-channel/rotation work is paid down at that point; what
  carries forward is the operational habit of rotating secrets
  on a schedule.
- The drain-window calculation depends on `poll_interval` and
  `max_attempts`. Operators who change those values must update
  the runbook's 90-second figure.

## Alternatives considered

### Single shared secret (the prior state)

Rejected, retroactively. The single-secret arrangement made a
compromise affect both channels and forced a downtime window for
rotation. Per-channel + dual-secret retires both problems for ~3
days of work.

### Full mTLS from day 1

Rejected, deferred to `R-LOOP-3`. mTLS solves the same blast-radius
problem and more, but it requires SPIRE workload identity per
service node, certificate rotation tooling, and a CA hierarchy.
That is 2 weeks of focused infrastructure work; HMAC rotation is
3 days. We pay the rotation work now and retire it later.

### JWT for service-to-service

Rejected. Service-to-service JWTs need an issuer (either OIDC or a
purpose-built signer), expiry handling, and a verifier dependency
on the issuer's JWKS — the same shape as the human OIDC path
(ADR-0004) but for a different surface. Heavier than the
HMAC-on-body primitive, and for the same expected retirement
horizon (`R-LOOP-3`). Not the right cost shape.

### Rotate the single secret with downtime acceptance

Rejected. A "rotate during a maintenance window" pattern is
operator-hostile and incompatible with the platform's 99.95%
availability target. The dual-secret window is the zero-downtime
primitive precisely because it has no maintenance window.

## References

- Commit `fe70ddd` — R-LOOP-4-ROT (PR #58)
- `services/declaration/src/api/internal.rs` — `verify_hmac_with_rotation`
- `services/verification-engine/src/api/internal.rs` —
  `verify_hmac_with_rotation` (mirror)
- `docs/runbooks/hmac-secret-rotation.md` — full procedure with
  step-by-step commands, drain-window calculation, and on-call
  quick-reference table
- `services/declaration/docker-compose.integration.yaml` — split
  envs `RECOR_D_TO_V_HMAC` + `RECOR_V_TO_D_HMAC`
- `docs/ROADMAP.md` Track L — `R-LOOP-3` (SPIFFE+mTLS, ~2 weeks,
  retires this ADR's primitive)
- Doctrines D14 (fail-closed), D17 (zero trust), D18 (no secrets
  in code/logs)
