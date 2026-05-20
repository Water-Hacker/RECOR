# ADR-0008: SPIFFE-based mTLS for service-to-service authentication

**Status:** Accepted (skeleton landed in PR for R-LOOP-3); operational
cutover staged behind `AUTH_TRANSPORT` env (default `hmac`).
**Decision-makers:** @recor/architect-team, @recor/security-team,
@recor/infrastructure-team
**Date:** 2026-05-12

## Context

The D↔V loop today (ADR-0003 + ADR-0005) authenticates cross-service
requests with HMAC-SHA256 over the raw request body, signed with a
per-channel shared secret that supports dual-secret rotation. That
arrangement works but it carries three structural limitations the
production roadmap is committed to retire:

1. **Symmetric secrets.** Both sides of every channel hold the same
   key material. A compromise on either host yields equal forging
   power; the rotation primitive lessens the operational pain but
   does not change the blast radius.
2. **Transport authentication is the bearer of identity.** "The peer
   who has the HMAC key" is the identity assertion. There is no
   independent cryptographic identity bound to the host or
   workload; the secret could in principle be exfiltrated to any
   process that can reach the receiver.
3. **No layered defence.** Connection-level auth and message-level
   auth are the same single primitive. If the receiver mishandles
   HMAC verification (a coding error, an alg-confusion bug, a
   timing leak), there is nothing below it.

Doctrines D14 (fail-closed at integration boundaries) and D17 (zero
trust at every network boundary) both push toward a transport-layer
primitive that:

- Carries a workload-bound cryptographic identity, not a shared
  secret.
- Is independent of any application-layer signature, so a
  defence-in-depth posture can keep both layers during cutover and
  drop the weaker one once operational confidence is established.
- Has an automated rotation pipeline so the operator does not own
  the rotation cadence as runbook discipline.

`R-LOOP-3` is the ticket that retires the HMAC primitive in favour of
**mutual TLS terminated by per-workload SVIDs issued via SPIFFE**.

## Decision

We deploy **SPIRE** as the SPIFFE control plane and configure both
the Declaration service and the Verification engine to terminate
inbound and outbound traffic with **mTLS using their workload SVID**.

Specifics:

- **Trust domain `recor.cm`.** Matches the national-registry
  domain. Workload SPIFFE IDs are:
  - `spiffe://recor.cm/declaration`
  - `spiffe://recor.cm/verification`
  - `spiffe://recor.cm/portal` (server-side companion only — the
    browser SPA does not participate in SPIFFE)
- **SPIRE server + agent.** One SPIRE server per environment;
  agents on every host. Dev deployment is a single-node
  docker-compose at `infrastructure/spire/`; production is a
  multi-replica Helm chart (deferred follow-up).
- **Workload attestation = docker label selectors (dev) /
  Kubernetes Projected Service Account Tokens (prod).** Both
  schemes bind a container instance to its SPIFFE ID without
  shared secrets.
- **rustls on both ends.** A new shared crate
  `packages/recor-spiffe` exposes:
  - `SpiffeClient` — fetches SVID + trust bundle from the
    Workload API at startup; caches them; refuses to start the
    parent service if the bootstrap fails.
  - `build_server_config` / `build_client_config` — rustls
    ServerConfig and ClientConfig builders that consume the SVID
    and trust bundle. Mutual authentication is the default; the
    `WebPkiClientVerifier` requires + verifies a peer certificate.
  - `peer_spiffe_id_from_cert` + `PeerSpiffeId` request extension
    — extract the URI-SAN SPIFFE ID from a verified peer leaf and
    surface it to handlers.
  - `enforce_peer_id` — the application-layer allowlist gate.
- **Three-state config: `AUTH_TRANSPORT=hmac|mtls|mtls-only`.**
  Both services read this env and gate behaviour:
  - `hmac` (default; v1 production path). No SPIFFE involvement.
    Identical behaviour to the pre-R-LOOP-3 build.
  - `mtls`. Transport-layer mTLS via SPIFFE **AND** HMAC header
    still required on `/v1/internal/*`. This is the defence-in-
    depth cutover window: operators flip mTLS on while keeping
    HMAC as a fallback authenticator. The receiver returns 401 if
    the HMAC header is missing or wrong, even when the TLS handshake
    succeeded.
  - `mtls-only`. mTLS-only steady state. HMAC verification is
    dropped. This is the post-cutover production posture.
- **Fail-closed start-up (D14, D7).** When `AUTH_TRANSPORT=mtls` or
  `mtls-only` and the SPIFFE Workload API is unreachable at
  startup, the service's composition root returns the error from
  `SpiffeClient::bootstrap` rather than silently downgrading.
- **OBS-1 metrics.** Two new counters:
  - `recor_spiffe_svid_fetch_total{result=success|failure|mismatch}` —
    ticks on every bootstrap and any future re-fetch.
  - `recor_spiffe_peer_verify_total{result=success|missing|malformed|denied}` —
    ticks per inbound TLS connection that carries a peer
    certificate.
- **Operational procedures documented separately**:
  - `docs/runbooks/spiffe-onboarding.md` — register a new
    workload, rotate the trust bundle, debug SVID-fetch failures.
  - `infrastructure/spire/README.md` — bring up the dev
    deployment + add a new registration entry.

## Consequences

### Positive

- **Workload-bound cryptographic identity.** The SVID's private key
  never leaves the agent's memory; the workload reads only its
  certificate + key from the Workload API socket on a tmpfs.
- **Layered defence during cutover.** `mtls` mode keeps HMAC as a
  fallback authenticator. A bug in either layer fails closed without
  affecting the other.
- **Automated rotation.** SVIDs renew every hour (default TTL); the
  trust bundle rotates on the CA's TTL boundary. The operator does
  not manage rotation pace.
- **Per-peer allowlisting at the application layer.** Even after
  the TLS handshake succeeds, the inbound handler asserts the
  presented SPIFFE ID matches the expected workload — a separate
  D17 zero-trust gate above the transport. A misconfigured SPIRE
  that issues an SVID to the wrong workload still fails the
  application gate.
- **Forward-compatible.** SPIFFE is the standard cloud-native
  identity primitive (CNCF graduated). Kafka migration
  (R-LOOP-2) consumes the same SVIDs for broker auth; Vault
  integration (OPS-4) consumes SVID-bound login.
- **Retires HMAC primitive cleanly.** Once `mtls-only` is the
  production posture, the HMAC rotation runbook
  (`docs/runbooks/hmac-secret-rotation.md`) retires; per-channel
  HMAC secrets retire; ADR-0005's primitive completes its
  retirement path.

### Negative

- **Operational complexity.** SPIRE adds a new control plane to
  deploy, monitor, back up, and rotate. The dev compose is one
  command but production demands a multi-replica server, HA
  storage, KMS-backed key management, and a runbook discipline
  for trust-bundle compromise.
- **Workload attestation correctness is operationally critical.**
  A wrong selector grants an SVID to the wrong workload; we
  mitigate by combining transport-layer mTLS with the application-
  layer allowlist (`enforce_peer_id`), but the SPIRE registration
  entries themselves are a configuration surface that demands
  review.
- **Two transports to maintain during cutover.** `hmac`, `mtls`,
  and `mtls-only` are three code paths. We keep them coherent by
  funnelling every code path through the same
  `InternalAppState::hmac_required` boolean; the
  `services/declaration/src/api/internal.rs` handler is the only
  place that branches on the transport.
- **Browser-side portal does NOT participate.** The SPA cannot
  receive an SVID; portal-to-API authentication remains OIDC
  bearer + dev header. A server-side BFF (`spiffe://recor.cm/portal`)
  is the migration vehicle for "the portal speaks mTLS to the
  API", deferred to a follow-up.

### Neutral

- **HMAC rotation work paid down.** ADR-0005's per-channel +
  dual-secret rotation primitive served its purpose: it bought us
  zero-downtime rotation while we built toward mTLS. The work is
  retired, not wasted.
- **Multiple trust-domain federation is out of scope.** The single
  `recor.cm` trust domain is sufficient for v1. Federation (foreign
  trust-domain bundle imports) is a v2 capability if/when RÉCOR
  needs to share workload identity across organisational
  boundaries.

## Alternatives considered

### Istio service mesh

Rejected. Istio gives us mTLS + identity for free, but it brings
sidecars, an iptables-based traffic-capture layer, and a control
plane (`istiod`) that becomes another node in the operability
budget. For a two-service production loop, the cost is
disproportionate. SPIRE alone gives us the SVID issuance + workload
attestation without the mesh tax. We can adopt a mesh later if the
service topology grows.

### AWS App Mesh / Envoy proxies

Rejected. App Mesh ties us to AWS; the platform is sovereign
infrastructure and provider-agnostic. Self-hosted Envoy as a
sidecar carries the same cost as Istio's sidecars without the
control-plane integration; not the right cost shape.

### Raw certificates managed by hand

Rejected. Static CA + per-service certificates issued out of a
PKI we run is the "make a SPIFFE without the SPIFFE machinery"
option. We would still need an issuer (cfssl, smallstep, Vault
PKI, etc), a rotation pipeline, a workload-to-cert binding
mechanism, and an identity-extraction primitive. SPIRE packages
all of those with the standard.

### Continue with HMAC, no transport-layer change

Rejected (deferred-then-rejected). The R-LOOP-3 ticket has been
on the roadmap since the v1 launch; ADR-0005 explicitly called
out HMAC as an interim primitive that retires when mTLS lands.
The transport-layer identity gap is the most-prominent open
finding from the v1 threat model (TB3 row, `docs/security/threat-
model.md`); we close it now.

## References

- `infrastructure/spire/` — the dev SPIRE deployment.
- `packages/recor-spiffe/` — the shared crate (SVID client +
  rustls glue + middleware + 5+ unit tests).
- `services/declaration/src/main.rs` /
  `services/verification-engine/src/main.rs` — the
  `AUTH_TRANSPORT=mtls` startup paths (D7 / D14 fail-closed).
- `docs/runbooks/spiffe-onboarding.md` — operational procedures.
- ADR-0003 (HTTP outbox relay) — the transport this ADR replaces
  the auth primitive of.
- ADR-0005 (HMAC channel rotation) — the interim primitive this
  ADR retires.
- Doctrines D7 (no workarounds), D14 (fail-closed), D17 (zero
  trust), D20 (supply-chain integrity — SPIRE upstream pinned).

## Linked from

- [ADR-0010 — FATF R.24 BO cascade, bearer-share + nominee disclosure, adequacy claims](0010-fatf-bo-cascade-and-adequacy.md) — references this ADR in its "Related" section.
