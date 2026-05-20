# `tests/` — top-level test coverage map

The top-level `tests/` tree carries cross-service contract coverage
only. Per-service unit + integration coverage lives inside each
service's own Cargo crate or Go module.

## What lives where

| Layer | Location | Driver |
|---|---|---|
| **Unit tests** (per-service) | `services/<svc>/src/**/tests` (Rust `#[cfg(test)]`) and `apps/<svc>/...` similarly | `cargo nextest run` / `cargo test --lib` |
| **Integration tests** (per-service, testcontainers Postgres 17) | `services/<svc>/tests/*.rs` (`#[ignore]`-gated) | `cargo test --test '*' -- --ignored` |
| **Property tests** | `services/<svc>/tests/properties_*.rs` and `packages/**/tests/properties_*.rs` | `cargo test --features proptest` |
| **Chaincode tests** | `chaincode/audit-witness/audit_witness_test.go` | `go test ./...` |
| **End-to-end (UI)** | `applications/declarant-portal/tests/e2e/*.spec.ts` (Playwright) | `pnpm --filter @recor/declarant-portal e2e` |
| **Contract tests** | `tests/contract/*.sh` and `tests/contract/*.test.sh` | invoked by `just check` and CI |
| **Load + chaos** | not in repo yet | tracked under post-launch ADR |

## Audit reference — FIND-020

The previous tree contained empty `tests/{chaos,performance,e2e}`
directories. FIND-020 (HIGH) flagged them as doctrine drift (D08 — no
dangling threads). Sprint 4 closure removed the empty directories
rather than commit WIP scaffolds:

- **E2E.** Authoritatively under
  `applications/declarant-portal/tests/e2e/` (Playwright). The
  top-level `tests/e2e/` was a doctrine-drift relic — having a second
  E2E location creates ambiguity about which suite is canonical.
- **Chaos.** Explicitly deferred to a post-launch hardening
  workstream. A dedicated ADR is required before introducing chaos
  rigging — the choice of tool (LitmusChaos, Chaos Mesh, manual
  kill-and-watch) has cluster-wide blast-radius implications.
- **Performance.** The contract-level smoke suite under
  `tests/contract/` is the launch-readiness gate; structured load
  shaping (k6 / vegeta / Locust) follows the chaos ADR.

When chaos or load coverage lands, this README points at it.

## Running tests locally

```bash
# Everything cargo + Go + pnpm + contract suites
just test

# Targeted: one service's unit tests
cargo test -p recor-declaration --lib

# Targeted: one service's integration suite (testcontainers Docker required)
cargo test -p recor-declaration --test '*' -- --ignored
```

CI runs the same `just test` plus the contract suites and the portal
Playwright E2E in `.github/workflows/required-checks.yaml` and
`.github/workflows/portal-e2e.yaml`.
