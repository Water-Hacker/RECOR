# Runbook: HMAC Secret Rotation

**Tracks:** R-LOOP-4-ROT. Closes the rotation procedure for the
HMAC-signed cross-service channels (D↔V).

## Scope

RÉCOR has two HMAC-signed cross-service channels:

| Channel | Signer | Verifier | Secret env (production) |
|---|---|---|---|
| **D → V** | Declaration's outbox-relay | V-engine's `POST /v1/internal/declaration-events` | `RECOR_D_TO_V_HMAC` |
| **V → D** | V-engine's outbox-relay | Declaration's `POST /v1/internal/verification-outcomes` | `RECOR_V_TO_D_HMAC` |

The two secrets are **independent**. Rotating one does not require
touching the other. A compromise of one leaks only that direction.

Each verifier accepts a **current** secret and an **optional old**
secret simultaneously (`*_HMAC_SECRET_OLD`). This is the rotation
primitive — during a rotation window both old and new signatures
verify; the operator flips the signer side, drains in-flight requests,
then clears the old secret.

## Procedure (zero-downtime rotation, single channel)

The example below rotates the D→V channel. Substitute env names
appropriately for the V→D channel.

### Step 1 — Generate the new secret

```bash
NEW_SECRET=$(openssl rand -hex 32)
echo "$NEW_SECRET"   # store somewhere safe (vault, sealed-secrets, etc.)
```

### Step 2 — Configure the verifier to accept BOTH old and new

On the **V-engine** (the verifier for D→V):

```bash
# Set the OLD env to whatever RECOR_D_TO_V_HMAC currently is, then
# update the current env to the new value. Both are now accepted.
export INBOUND_HMAC_SECRET_OLD="${INBOUND_HMAC_SECRET}"      # snapshot current
export INBOUND_HMAC_SECRET="${NEW_SECRET}"                   # accept new
# restart / reload V-engine
```

The verifier now accepts envelopes signed with **either** secret.
The integration-smoke pattern is the equivalent — set both envs in
the V-engine container.

### Step 3 — Flip the signer to the new secret

On the **Declaration service** (the signer for D→V):

```bash
export RELAY_HMAC_SECRET="${NEW_SECRET}"
# restart / reload Declaration service
```

New outbox dispatches now sign with the new secret. In-flight requests
already signed with the old secret still verify (Step 2's
`INBOUND_HMAC_SECRET_OLD` covers them).

### Step 4 — Drain in-flight requests

Wait `(relay_poll_interval × max_attempts) + safety_margin`. With
default settings (`poll_interval = 5s`, `max_attempts = 12`) this is
~60s + 30s safety = **90 seconds**. After this window, no envelope
signed with the old secret should be in flight.

Confirm by checking the outbox tables for rows with
`dispatch_attempts > 0` and `dispatched_at IS NULL`:

```bash
docker compose exec postgres-declaration psql -U recor -d declaration -c \
    "SELECT COUNT(*) FROM outbox WHERE dispatched_at IS NULL AND dispatch_attempts > 0"
```

Should be 0 (or matching DLQ already).

### Step 5 — Clear the old secret on the verifier

```bash
unset INBOUND_HMAC_SECRET_OLD   # or export INBOUND_HMAC_SECRET_OLD=""
# restart / reload V-engine
```

The verifier now only accepts the new secret. Rotation complete.

## Safety properties

- **Atomicity at the secret level**: at every moment the verifier
  accepts at least one of (old, new). There is no time window where
  a valid signer's request would be rejected.
- **No replay of stale envelopes**: outbox rows are individually
  marked `dispatched_at` once accepted. A re-fetched envelope with
  a stale signature is still signed correctly with whichever secret
  was current at signing time; once verified it advances the row.
- **DLQ behaviour unchanged**: if a row exhausts `max_attempts`
  during the rotation, it dead-letters as usual (R-LOOP-4-DLQ).

## What this rotation does NOT cover

- **Secret distribution**: the runbook assumes you have a way to
  push new env values to running services (Vault, AWS Secrets
  Manager, K8s ConfigMaps + reload, etc.). Wiring that is out of
  scope.
- **Per-tenant or per-aggregate secrets**: this rotation primitive
  is whole-channel scope. Per-aggregate-key encryption is a future
  ticket (R-CRYPTO-1).
- **Algorithm rotation** (e.g. HMAC-SHA256 → HMAC-SHA512): same
  primitive works, but the migration path is bespoke. File a
  separate ticket.

## On-call quick-reference

| Symptom | Likely cause | Action |
|---|---|---|
| Verifier returns 401 immediately after deploy | Signer-side rotated, verifier didn't get OLD env | Set the verifier's `*_HMAC_SECRET_OLD` to the prior current value, restart |
| Outbox rows hit DLQ during rotation | Skipped step 2 — verifier never accepted the new secret | Replay the DLQ rows after rotation completes; investigate why step 2 was missed |
| Warn-log: "verified against OLD HMAC secret" | Steady state during a rotation window | Expected. Verify rotation is still in progress (step 4); if rotation appears stuck, check signer-side env |

## Verification of the rotation primitive itself

Unit tests in `services/{declaration,verification-engine}/src/api/internal.rs`
exercise the dual-secret verify logic:

- `rotation_off_only_current_secret_works`
- `rotation_active_both_old_and_current_accepted`
- `rotation_third_party_signature_still_rejected`
- `rotation_tampered_payload_still_rejected`

These run on every CI build.

## Related runbooks

- [oncall-triage-tree](oncall-triage-tree.md) — entry point when an
  unexpected 401 spike during rotation is the user-visible symptom
- [dlq-inundation](dlq-inundation.md) — handles the DLQ rows that
  accumulate when a rotation is botched (the common failure mode)
- [oidc-issuer-outage](oidc-issuer-outage.md) — sister runbook for
  the auth secret class (OIDC) versus the inter-service secret class
  (HMAC) documented here
- [deploy-new-version](deploy-new-version.md) — for the rollout step
  that picks up rotated secrets when they are deployment-scoped
- [rollback-deployment](rollback-deployment.md) — if a rotation deploy
  destabilises a service
- [incident-response-template](incident-response-template.md) — for
  the post-mortem if a botched rotation caused an incident
- [observability-prod-stack](observability-prod-stack.md) — for
  finding the rotation-window log lines (`verified against OLD HMAC
  secret`) in production Loki
