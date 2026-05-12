# ADR 0009: Anchor declaration receipts to Hyperledger Fabric

Date: 2026-05-12
Status: Accepted
Authors: Lead architect, infrastructure engineer
Reviewers: Security lead, Technical Advisory Function

## Context

The Declaration service computes a BLAKE3-256 receipt hash over the
canonical declaration body and returns it to the declarant on every
successful submission, amendment, correction, and supersession. Today,
the hash lives in three places:

1. The HTTP response (the declarant keeps it).
2. The `declarations.receipt_hash_hex` column on the projection.
3. The `declaration_events.event_payload` JSONB.

Compendium-2 (COMP-2) closed the in-database tampering risk for (2) and
(3) with append-only triggers and revoked DML grants. That defence
fails against an attacker with raw superuser access, against backup
exfiltration, and against a hostile DBA acting under operational
pressure. Gap **G1** in
`docs/security/threat-model.md` calls this out as a launch-blocking
gap, with this ADR's ticket (R-DECL-9) as the closure path.

Doctrine **D15 (cryptographic provenance on every consequential event)**
requires an external, append-only trust anchor for receipts. The
operational consortium model — ten institutions plus international
observers — already commits the platform to a multi-party governance
posture, so a permissioned ledger jointly operated by those parties is
both the politically and technically appropriate anchor.

This ADR records the decision to anchor every declaration event's
receipt to a **Hyperledger Fabric** channel operated by the consortium,
and explains the alternatives rejected.

## Decision

Adopt Hyperledger Fabric as the audit-receipt anchor.

- **Cluster topology** (out of scope for this implementation; standard
  Fabric ops): 3 orderers (Raft consensus), 4 peers across at least 2
  organisations, deployed on consortium infrastructure. The cluster
  ops are coordinated by the infra team independently of this
  ticket — they are already familiar with Fabric per Architecture V4 P18.
- **Channel**: `recor-audit`, jointly endorsed by ARMP, ANIF, DGI, and
  CONAC (four-of-the-ten-institution write quorum). Read access is open
  to all consortium members and to the public via the audit-verifier
  app.
- **Chaincode**: `chaincode/audit-witness/` — a Go (Fabric's native SDK
  language) contract with three methods:
  - `PutAuditEntry(event_id, declaration_id, receipt_hash_hex, ts, signing_peer_attestation)`
  - `GetAuditEntry(event_id)`
  - `ListAuditEntriesForDeclaration(declaration_id)`
- **Bridge worker** (`apps/worker-fabric-bridge/`): consumes from the
  outbox-relay channel, calls the chaincode through a Fabric Gateway
  HTTP shim, dead-letters permanent failures to `fabric_bridge_dlq`.
- **Verifier app** (`apps/audit-verifier/`): exposes
  `GET /v1/audit/verify/{declaration_id}`; queries the chaincode,
  re-derives the receipt hash from the projection, and returns a
  structured verification report.
- **Idempotency**: enforced at the chaincode layer (duplicate
  `PutAuditEntry` is refused). The bridge worker treats the refusal as
  success — at-least-once delivery is the platform's transport
  guarantee, exactly-once is the chaincode's.
- **Failure semantics**: anchor failures dead-letter to
  `fabric_bridge_dlq` so the declaration submit path is never blocked
  by Fabric. The operator runbook (`docs/runbooks/fabric-bridge.md`)
  prescribes manual re-anchor procedures.

## Considered alternatives

### Alternative A: Bitcoin OP_RETURN

Considered but not chosen. Anchoring receipt hashes to the Bitcoin
mainnet via OP_RETURN gives the strongest public trust anchor available
today — Bitcoin's hash power is the global SHA-256 consensus floor. Cost:

- Each transaction costs ~$0.50-$5 depending on fee market; at the
  platform's projected 100K declarations/year that's ~$50K-$500K/year
  just in fees.
- Confirmation latency is 10 minutes per block; the platform can't wait.
- Bitcoin transactions are pseudo-public; even hashes leak metadata
  (declaration cadence, working hours) about the platform's operations
  to anyone watching the chain. Sovereign infrastructure cannot accept
  uncontrolled metadata leakage.
- The Cameroonian government does not officially recognise Bitcoin;
  operational dependence on it would create political risk.

Acceptable as a Phase-3 "second anchor" for high-stakes declarations
(amendments and supersessions only), batched via a Merkle root posted
once an hour. Not in scope for this ADR.

### Alternative B: On-premise Merkle tree only

Considered but not chosen. A periodic Merkle tree over the event log
posted to a signed timestamping service (e.g., RFC 3161) gives in-house
integrity without inter-organisation coordination overhead. Cost:

- The trust anchor is the platform's own signing key — circular if the
  platform itself is compromised.
- No multi-party governance posture; doesn't match the consortium
  political model.
- Inter-org disputes (e.g., ARMP claiming the platform falsified a
  declaration) have no neutral arbiter.

Acceptable for internal-integrity-only systems; the wrong fit for a
multi-institutional registry that explicitly needs external arbitration.

### Alternative C: Sigstore Rekor

Considered but not chosen. Rekor (the Sigstore transparency log) is
operationally simple, free, and well-engineered. Cost:

- Hosted by the Linux Foundation; not under consortium control.
- Sovereignty: the audit log of a Cameroonian sovereign registry should
  not depend on a service hosted outside Cameroon's jurisdiction.
- No multi-party endorsement model; whoever has Rekor write access
  can append anything.

A close call for non-sovereign projects. For RÉCOR, the sovereignty
argument is binding.

### Alternative D: Ethereum L2 (Polygon / Arbitrum)

Considered but not chosen. An EVM L2 gives the lowest per-transaction
cost outside the consortium-private ledger family and inherits some
Ethereum L1 finality. Cost:

- Same sovereignty issue as Rekor: the trust anchor is operated by
  parties outside the consortium.
- Gas market volatility makes operational budget planning unreliable.
- Smart-contract risk: a bug in the anchoring contract is unfixable
  without an upgrade process the consortium does not control.

Reject for sovereignty + risk reasons; reconsider only if the
consortium can run its own validator on the chosen L2 (an enormous
undertaking).

### Alternative E: Hyperledger Indy

Considered but not chosen. Indy is purpose-built for decentralised
identity, not for general audit anchoring. Cost:

- Schema designed around DIDs and verifiable credentials; we'd be
  forcing a peg into a square hole.
- Smaller operational community than Fabric.

Fabric was the natural choice; Indy is over-specialised.

## Consequences

### Easier

- Doctrine D15 obligation is mechanically discharged for every event.
- Gap G1 in the threat model closes — the launch is unblocked on this
  axis.
- The verifier app gives operators and the public a way to independently
  validate any historical declaration's receipt.
- Multi-party governance is naturally expressed at the channel-policy
  layer (endorsement quorum maps directly to consortium-member
  signatures).
- The bridge is asynchronous; declaration submission latency is
  unaffected.

### Harder

- Operating a Hyperledger Fabric cluster requires Fabric-specific
  expertise (orderers, peer chaincode lifecycles, channel
  configuration, MSP issuance). The infra team is provisioning the
  cluster separately.
- The Fabric Gateway HTTP shim is a custom operator-side component
  (Go binary) — one more thing to deploy and monitor. Documented in
  `docs/runbooks/fabric-bridge.md`.
- The Rust ecosystem does not have a mature Fabric Gateway SDK; we
  bridge through HTTP today. A future ticket can migrate to native
  gRPC once the SDK matures.
- Recovery from a chaincode-state divergence (split-brain across
  orderers) is a documented but rare operational procedure.

### New commitments

- 4 peers + 3 orderers operational SLO (handled by the infra team's
  Fabric runbook, separate from this PR).
- Operator on-call for `fabric_bridge_dlq` review (target: zero rows
  after a 24h grace period; non-zero rows = page).
- Public commitment to verifier endpoint availability: 99.95% (matches
  declaration service's read SLO).

### Old commitments now obsolete

None — this is the closure of a previously-acknowledged gap (G1), not a
displacement.

## Doctrines applied

- **D01** — completeness: chaincode + bridge + verifier + ADR + 2 runbooks
  + threat-model update ship together.
- **D04** — tests: chaincode unit tests, bridge unit tests, processor
  unit tests, verifier handler tests all in this PR.
- **D13** — idempotency: enforced at the chaincode boundary; bridge
  treats refusal as success.
- **D14** — fail-closed: bridge DLQs permanent failures; verifier
  returns 503 when Fabric is unreachable rather than degrading to
  projection-only verification.
- **D15** — cryptographic provenance: this ticket IS the doctrine.
- **D16** — observability: OBS-1 metrics on every anchor attempt and
  every DLQ write.
- **D17** — zero trust: gateway shim authenticates via mTLS-derived
  identity OR bearer token; the worker authenticates the relay POST
  via HMAC.

## Document references

- Architecture V4 P18 — Audit anchoring topology
- `docs/security/threat-model.md` § Gap G1
- `docs/PRODUCTION-TODO.md` § R-DECL-9
- `services/declaration/CLAUDE.md` § D15 follow-ups

## Implementation

- Status: Implemented (this PR)
- Sprint: PI-2 Sprint 3
- Linked tickets: R-DECL-9 (this); G1 closure on the threat model
- Follow-up tickets:
  - Migrate the bridge transport from HTTP-via-shim to native gRPC once
    a production-ready Rust Fabric Gateway SDK exists
  - Phase-3 Bitcoin OP_RETURN second-anchor for high-stakes events
    (separate ADR when filed)
  - Periodic chaincode-state reconciliation report (cron-style job
    that compares the projection event count to the on-chain count
    and alerts on drift)
