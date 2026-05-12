---
name: recor-test-pyramid
description: Test writing at layer-appropriate ratios. Fires when tests are being written for a change. Loads the pyramid ratios per layer and the testing patterns the project uses.
---

# RÉCOR test pyramid

Doctrine 4: tests are part of the feature. The project's test pyramid varies
by layer because the cost of integration testing and the value of unit testing
differ by layer.

## Pyramid ratios

| Layer | Unit | Integration | E2E |
|-------|------|-------------|-----|
| 0 (crypto) | 80% | 15% | 5% |
| 2 (services) | 70% | 25% | 5% |
| 3 (verification) | 60% | 35% | 5% (adversarial) |
| 4 (APIs) | 50% | 40% | 10% |
| 5 (integrations) | 30% | 60% | 10% |
| 6 (applications) | 30% | 30% | 40% |

## Property-based testing

Use for invariants. Rust: `proptest`. TypeScript: `fast-check`. Go:
`gopter`.

Required for:
- Cryptographic functions (substantial property coverage)
- Database migrations
- Verification engine signature outputs (invariants like monotonicity)
- Idempotent endpoints (replay equivalence)

## Adversarial corpus

For the verification engine, the adversarial corpus at /tests/adversarial/ is
the gold standard. The corpus is governed; new corpus entries require approval
from @recor/verification-team. Engine changes are evaluated against the corpus
before merge.

## Test naming

Rust: `test_<behaviour-being-tested>`
Go: `Test<Behaviour>` or `TestXxx_<scenario>`
TS: `it("<behaviour-being-tested>", ...)` or `describe(...).it(...)`

Tests describe behaviour, not implementation. `test_returns_error_when_input_invalid`
is correct; `test_calls_validate_function` is not.

## Frameworks

- Rust: cargo nextest, proptest, rstest, mockall
- Go: standard library, testify, gomock
- TypeScript: vitest, playwright, fast-check, testing-library

## Common gotchas

- Time-dependent tests: never use system time directly; inject a Clock
- Network-dependent tests: never reach the real network in unit tests
- Database-dependent tests: use the testcontainers pattern for integration tests
- Random-dependent tests: seed the RNG explicitly
