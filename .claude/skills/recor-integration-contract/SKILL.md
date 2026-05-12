---
name: recor-integration-contract
description: Consumer integration contract work. Fires when consumer integration changes are being designed or modified. Loads the contract discipline, mTLS requirements, fail-closed boundary.
---

# RÉCOR consumer integration contracts

Every consumer integration has a contract that lives in /contracts/.

## Contract surface

- gRPC: /contracts/grpc/<consumer>.proto
- REST: /contracts/openapi/<consumer>.openapi.yaml
- Webhooks: /contracts/webhooks/<consumer>.md (signature format documented)
- Event schemas (if consumer publishes events to us): /contracts/avro/<consumer>/

## Contract evolution

Changes to a consumer-facing contract are coordinated with the @<consumer>-liaison:
- Backward-compatible additions: minor version bump
- Behaviour changes: major version bump; coordinated rollout
- Removal: deprecation period documented in contract, then removal

## Operational requirements

- mTLS at the mesh boundary
- SPIFFE workload identities
- HMAC-Ed25519 webhook signatures
- Fail-closed at the consumer side

## Per-consumer specifics

Each consumer has its own service in /services/integrations/<consumer>/ with
its own CLAUDE.md and runbook. Read those before working on the contract.
