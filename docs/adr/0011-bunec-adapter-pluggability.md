# ADR-0011 — BUNEC adapter pluggability (mock / real switch)

- **Status:** Accepted (2026-05-20)
- **Deciders:** Verification team, Domain team, Lead architect
- **Closes:** TODO-015
- **Related:** ADR-0002 (Dempster-Shafer fusion), ADR-0008 (SPIFFE mTLS)

## Context

The verification engine's Stage 2 (identity) needs to look up natural
persons in the Cameroonian company registry (BUNEC). FATF R.24 c.24.6
explicitly names "the registry of incorporated entities" as a required
component of the multi-pronged approach to beneficial ownership
information. Without a real BUNEC adapter, the verification engine is
querying a `mock_bunec_persons` table seeded by integration tests —
which is acceptable in dev and CI, but is a structural finding when
inspected by an external auditor.

The cross-organisational data-sharing agreement with BUNEC is **not
yet in place** at the time of this ADR. The agreement requires:

1. mTLS handshake configuration on both sides (SPIFFE workload IDs
   in the platform; certificate trust chain on BUNEC's side).
2. A documented JSON wire contract for the lookup endpoint.
3. A documented refresh cadence so the platform can set its cache
   TTL coherently.
4. A documented escalation channel for outages and a published SLO.

Until those four items land, the platform cannot point production at a
real BUNEC endpoint. But shipping `BUNEC_ADAPTER_KIND=real` as a code-
gated capability **today** means the day the agreement lands the
change is a config flip in the production manifest, not a code change
plus review cycle plus redeploy.

## Decision

Add a `BUNEC_ADAPTER_KIND` config knob to the verification engine.
Values:

- `mock` (default) — wires `PostgresMockBunec` against
  `mock_bunec_persons`. Suitable for dev, CI, and the integration-
  smoke test rig.
- `real` — wires `RealBunecAdapter` against `BUNEC_BASE_URL` with the
  Bearer-token API key from `BUNEC_API_KEY`. Pre-existing retry +
  circuit-breaker + fail-policy machinery applies.

When `BUNEC_ADAPTER_KIND=real` but `BUNEC_BASE_URL` or `BUNEC_API_KEY`
is empty, the service **refuses to start** (D14 fail-closed). The
alternative — silently fall back to the mock — would let an operator
ship production thinking they have BUNEC integration when they don't.

The readiness probe surfaces which kind is in use so an operator
can confirm at a glance:

```
GET /readyz
{"bunec_adapter": "mock"}      # dev / CI
{"bunec_adapter": "real"}      # production after the agreement lands
```

## Rationale

**Why not just default to `real` and let it fail at runtime?**
Production verification submissions would error 5xx until the
operator notices. Failing at boot surfaces the misconfiguration
before any traffic is served (D14).

**Why a string enum and not a feature flag (`#[cfg(feature = "real-bunec")]`)?**
Feature flags ship different binaries; the production image and the
dev image would diverge, defeating reproducibility (D19) and SLSA L3
(D20 — every release MUST have a single verifiable provenance). A
single binary that branches on config is observably equivalent across
environments.

**Why fail-closed when `real` is set without a URL?**
A registry that says "I asked BUNEC and the entity is unknown" when
in reality it asked a Postgres table is committing a soundness error
that no log line later can undo. The blast radius of a misconfigured
start-up is bounded: the operator fixes the env and retries. The
blast radius of a months-of-misconfigured-production is a regulatory
finding the platform cannot survive.

**Why not require mTLS in code today?**
The agreement defines the certificate trust chain. Hard-coding mTLS
assumptions now would either be wrong (the agreement may specify a
different shape) or right-by-accident. The `RealBunecAdapter` uses
Bearer + TLS via reqwest's default client; once the agreement lands,
the adapter can be augmented with a SPIFFE-issued client cert via a
follow-up commit that is a localised change to the constructor — not
a re-architecture.

## Consequences

### Positive

- The day the BUNEC agreement lands, the production switch is `kubectl
  set env deployment/recor-verification-engine BUNEC_ADAPTER_KIND=real
  BUNEC_BASE_URL=... BUNEC_API_KEY_FROM=...`. No code change.
- The readiness probe makes the state of integration visible — an
  on-call cannot accidentally claim BUNEC is wired when it is not.
- Dev / CI / smoke-tests are unaffected; the default is `mock`.
- The breaker + fail-policy machinery already in `bunec_real.rs` is
  exercised by the same code path the operator will set live.

### Negative

- The platform's claim "we query BUNEC" is **still aspirational**
  until the agreement lands. This ADR does not change that fact —
  it only makes the flip a non-event when the moment comes.
- Production manifests must be updated to set
  `BUNEC_ADAPTER_KIND=mock` explicitly until the agreement lands,
  so an operator reading the manifest sees the choice rather than
  inheriting a default that silently picks mock.

### Operator burden

When the agreement lands:

1. Provision the BUNEC API key via Vault path
   `recor/v-engine/bunec` (the Vault loader in `main.rs` picks it up).
2. Set `BUNEC_BASE_URL` to the agreed endpoint.
3. Update the production manifest:
   ```yaml
   env:
     - { name: BUNEC_ADAPTER_KIND, value: real }
     - { name: BUNEC_BASE_URL, value: https://api.bunec.cm/v1 }
     - { name: BUNEC_FAIL_POLICY, value: fail-closed }
   ```
4. Follow the cutover procedure in
   `docs/runbooks/bunec-onboarding.md`.

## Alternatives considered

1. **Hardcode `real`, refuse to start without BUNEC config.** Rejected:
   would break dev + CI for the entire team until the agreement lands.
2. **Hardcode `real`, silently fall back to mock when config is
   empty.** Rejected: see "Why fail-closed when `real` is set without
   a URL" above.
3. **Ship two binaries (`-mock` and `-real`).** Rejected: violates
   D19 (reproducible everything) and D20 (single SLSA provenance per
   release). One binary, config-driven, is the platform-wide pattern.

## Verification

- Unit tests in `services/verification-engine/src/infrastructure/
  bunec_real.rs` exercise the retry + breaker + fail-policy paths.
- Boot-time refusal when `BUNEC_ADAPTER_KIND=real` is set without
  the URL/key is exercised by the operator manually as part of the
  cutover dry-run; an integration test is the cutover follow-up.
- The readiness probe surfaces the active adapter kind; the on-call
  dashboard reads this label.

## Linked from

- TODOS.md § TODO-015
- docs/runbooks/bunec-onboarding.md
- docs/runbooks/bunec-adapter-outage.md
- services/verification-engine/CLAUDE.md
