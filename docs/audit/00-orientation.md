# RÉCOR Production-Readiness Audit — Section 00: Orientation

**Audit Pass:** A (Sections 2, 3, 4 of the audit spec)
**Snapshot:** `main` @ `8f0d3ee` (HEAD as of 2026-05-13)
**Auditor:** Forensic; no fixes applied.
**Scope of THIS document:** what is in the repository, how it is built,
how it is tested, what it runs as at runtime, and how documentation maps
to code. Findings (drift, gaps) are flagged inline `[FINDING:severity]`
and aggregated downstream into `10-findings.md`.

---

## 1. Topology — every workspace package / module / app / contract dir / infra dir / docs dir

All paths are relative to repo root (`/home/kali/Music/RECOR`). Every path
below was verified by directory traversal on the audit snapshot.

### 1.1 Cargo workspace members (from `Cargo.toml` lines 18–32)

| Path                                  | Crate name (Cargo.toml)              | Purpose                                                                                                                                                                                                                                                                |
|---------------------------------------|--------------------------------------|------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `services/declaration/`               | `recor-declaration`                  | Beneficial-ownership declaration intake and lifecycle service. REST + gRPC + Kafka outbox. Compiles `contracts/declaration.proto` via `build.rs`. Two binaries: `recor-declaration` (`src/main.rs`) + `dump-openapi` (`src/bin/dump-openapi.rs`). Lib at `src/lib.rs`. |
| `services/verification-engine/`       | `recor-verification-engine`          | 9-stage adversarial pipeline. REST only (no gRPC). Kafka consumer for inbound declaration events; HTTP relay writeback. Single bin `recor-verification-engine`.                                                                                                       |
| `services/person-service/`            | `recor-person-service`               | Canonical natural-person registry. REST only. Two bins (`recor-person-service`, `dump-openapi-person`).                                                                                                                                                               |
| `services/entity-service/`            | `recor-entity-service`               | Canonical legal-entity registry. REST only. Two bins (`recor-entity-service`, `dump-openapi`).                                                                                                                                                                        |
| `apps/audit-verifier/`                | `audit-verifier` (lib + bin)         | Public verifier; reads Hyperledger Fabric audit channel, re-derives BLAKE3 receipts from declaration projection.                                                                                                                                                       |
| `apps/worker-fabric-bridge/`          | `worker-fabric-bridge`               | Outbox → Fabric anchor bridge. HTTP relay receiver (`/v1/relay`) writing to chaincode `audit-witness`.                                                                                                                                                                |
| `packages/recor-auth-oidc/`           | `recor-auth-oidc`                    | OIDC + JWKS verifier (RS256/ES256/EdDSA only; HS* refused). Shared by every Rust service.                                                                                                                                                                              |
| `packages/recor-logging/`             | `recor-logging`                      | PII-redacting tracing layer (OPS-2). BLAKE3-keyed-MAC redacted form for principals, UUIDs, SPIFFE URIs, receipts.                                                                                                                                                     |
| `packages/recor-vault-client/`        | `recor-vault-client`                 | Vault AppRole client + KV-v2 read + Config bridge (OPS-4).                                                                                                                                                                                                            |
| `packages/recor-spiffe/`              | `recor-spiffe`                       | SPIFFE Workload API client + rustls glue + tower middleware for SVID allowlists (R-LOOP-3).                                                                                                                                                                           |
| `packages/recor-inference-gateway/`   | `recor-inference-gateway`            | Anthropic Messages API client, budget-tracked (D22). Includes fixture provider for tests.                                                                                                                                                                              |
| `packages/fabric-bridge/`             | `fabric-bridge`                      | Hyperledger Fabric Gateway client + transport abstractions.                                                                                                                                                                                                            |

### 1.2 Non-Cargo applications

| Path                                  | Manifest                  | Runtime                                                              |
|---------------------------------------|---------------------------|----------------------------------------------------------------------|
| `applications/declarant-portal/`      | `package.json`            | React 19 + Vite 6 + Tailwind v4 SPA. PWA via `vite-plugin-pwa`.      |
| `chaincode/audit-witness/`            | `go.mod`                  | Hyperledger Fabric 2.5 chaincode (Go 1.22).                          |

### 1.3 Contracts

| Path                                  | Contents                                                                                                                |
|---------------------------------------|-------------------------------------------------------------------------------------------------------------------------|
| `contracts/declaration.proto`         | `recor.declaration.v1.DeclarationService` proto. Source for `services/declaration/src/api/grpc.rs` (via `build.rs`).    |
| `contracts/rest/`                     | **Empty.** `[FINDING:medium]` The justfile `_gen-openapi` reads `contracts/rest/declaration.openapi.yaml`; that file does not exist. The OpenAPI surface lives at `docs/openapi/declaration.json` instead (generated from the `dump-openapi` bin). The justfile target is therefore dead. |
| `contracts/grpc/`                     | **Empty.** Proto lives at `contracts/declaration.proto` (parent dir).                                                   |
| `contracts/events/`                   | **Empty.** No Avro schemas committed. `[FINDING:medium]` justfile `_gen-avro` target points at this dir; nothing to generate. ARCHITECTURE.md V4 P15 mentions Avro schemas for D↔V events; codebase serialises events as JSON inside outbox rows. |
| `contracts/bods/`                     | **Empty.** README references BODS export; no schema artefact present. `[FINDING:medium]`                                |
| `contracts/graphql/`                  | **Empty.** ARCHITECTURE.md L4 layer table lists GraphQL among consumer-facing APIs; no schema or implementation exists. `[FINDING:medium]` |

### 1.4 Infrastructure

| Path                                            | Purpose                                                                                                                                  |
|-------------------------------------------------|------------------------------------------------------------------------------------------------------------------------------------------|
| `infrastructure/observability-dev/`             | Local OTel Collector + Prometheus + Tempo + Loki + Grafana via `docker-compose.yaml`. 4 Grafana dashboards + `alert-rules.yaml`.        |
| `infrastructure/kafka/`                         | Single-broker KRaft dev cluster + `topics-init.sh`. Brings up `kafka:9092` for the D↔V cutover.                                          |
| `infrastructure/spire/`                         | SPIRE server + agent dev compose. SVID registration entries under `registration-entries/`. Onboarding scripts in `scripts/`.            |
| `infrastructure/vault/`                         | Vault dev mode compose + AppRole `policies/` + `scripts/` (bootstrap + role rotation).                                                   |
| `infrastructure/helm/observability/`            | Helm chart for prod observability stack (only chart currently committed).                                                                |
| `infrastructure/argocd/`                        | `observability.yaml` Argo CD app definition (only observability is currently Argo-managed).                                              |
| `infrastructure/ansible/`                       | **Empty.** `[FINDING:low]` Architecture references Ansible for host bootstrap; nothing here.                                              |
| `infrastructure/kubernetes/`                    | **Empty.** `[FINDING:high]` No k8s manifests for the services themselves; only the Helm observability chart exists. Roadmap items D-V-K8S-1 etc. are referenced from runbooks but not present. |
| `infrastructure/networks/`                      | **Empty.** `[FINDING:low]` D17 zero-trust network policies are referenced in `services/declaration/CLAUDE.md` for `/metrics` exposure; no NetworkPolicy or equivalent is committed. |
| `infrastructure/terraform/`                     | **Empty.** `[FINDING:high]` No IaC for the production substrate is committed. Doctrine D19 (reproducible everything) is not satisfied for cloud provisioning. |

### 1.5 Other top-level dirs

| Path                                  | Purpose                                                                                                                                 |
|---------------------------------------|-----------------------------------------------------------------------------------------------------------------------------------------|
| `tools/ci/`                           | 5 shell scripts: `apply-branch-protection.sh`, `check-openapi-drift.sh`, `check-portal-openapi-client-drift.sh`, `validate-claude-config.sh`, `validate-codeowners.sh`. |
| `tools/cli/`                          | **Empty.** justfile `_install-internal-cli` runs `cargo install --path tools/cli/recor-cli` — that path does not exist. `[FINDING:medium]` |
| `tools/codegen/`                      | **Empty.** justfile `_gen-graphql` reads `tools/codegen/graphql-codegen.yaml`; `_gen-avro` reads `tools/codegen/gen-avro.sh`. Neither present. `[FINDING:medium]` |
| `libraries/go/`                       | **Empty.** `[FINDING:low]` README mentions shared Go libs; none exist.                                                                  |
| `libraries/protos/`                   | **Empty.** Proto source lives in `contracts/declaration.proto` instead.                                                                 |
| `libraries/rust/`                     | **Empty.** Shared Rust crates live under `packages/` not `libraries/`. `[FINDING:low]` Naming inconsistency.                            |
| `libraries/ts/`                       | **Empty.** justfile `_gen-openapi` writes to `libraries/ts/recor-api-client/src/declaration.ts` — that target does not exist. `[FINDING:medium]` |
| `policies/`                           | **Empty.** `[FINDING:high]` Doctrine references and CLAUDE.md ban-list cite OPA Rego policies; `_check-policies` justfile target runs `opa fmt --diff policies/`. No Rego files committed. Authorization defence is currently in-handler only. |
| `alerts/`                             | **Empty.** Alert rules live under `infrastructure/observability-dev/alert-rules.yaml`. `[FINDING:low]` Top-level `alerts/` is a misleading shell. |
| `dashboards/`                         | **Empty.** Dashboards live under `infrastructure/observability-dev/grafana/dashboards/`. `[FINDING:low]`                                |
| `scripts/`                            | One file: `dr-drill.sh`. `[FINDING:low]` low surface; most ops scripts live under `services/*/scripts/`.                                |
| `tests/`                              | Repo-level test harnesses: `contract/` (2 shell tests + fixtures), `e2e/` empty, `chaos/` empty, `performance/` empty. `[FINDING:high]` chaos & performance dirs empty; doctrines D4/D6 imply property + chaos coverage. |
| `chaincode/audit-witness/`            | Go Fabric chaincode. Pinned Fabric 2.5 LTS.                                                                                             |
| `_extracted/`                         | Gitignored. Local `.docx`-extraction working dir. Not part of build.                                                                    |
| `graphify-out/`                       | Gitignored. AST knowledge graph. Not part of build.                                                                                     |
| `target/`, `.cargo-cache/`, `target-precheck/` | Gitignored Cargo build artefacts.                                                                                              |

### 1.6 Documentation directories

| Path                              | Contents                                                                                                                  |
|-----------------------------------|---------------------------------------------------------------------------------------------------------------------------|
| `docs/architecture/`              | One file: `RECOR-Software-Architecture-Document.docx` (binary). `[FINDING:medium]` Authoritative architecture is a binary `.docx`; not diffable. CLAUDE.md routes engineers to `docs/architecture/RECOR-Software-Architecture-Document.docx` for every chapter reference. |
| `docs/companion/`                 | One file: `RECOR-Implementation-Companion.docx` (binary).                                                                 |
| `docs/concept-note/`              | One file: `RECOR-Concept-Note.docx` (binary).                                                                             |
| `docs/build-spec/`                | One file: `RECOR-Sovereign-Build-Specification.docx` (binary).                                                            |
| `docs/onboarding/`                | **Empty.** `[FINDING:medium]` ARCHITECTURE.md refers new engineers to `CONTRIBUTING.md`; no onboarding markdown lives here. |
| `docs/openapi/`                   | One file: `declaration.json` (generated OpenAPI 3.1 from `dump-openapi` bin). Verification-engine OpenAPI deferred (`TODO(R-VER-OPENAPI)` in `services/verification-engine/src/api/rest.rs:3`). |
| `docs/adr/`                       | 9 ADRs + `README.md`. See § 7 below.                                                                                      |
| `docs/runbooks/`                  | 21 runbooks (operator-facing). See § 7 below.                                                                             |
| `docs/security/`                  | 6 markdown files: threat-model, branch-protection, pen-test-prep, pen-test-rules-of-engagement, teams, a11y-audit-2026-Q2, README. |
| `docs/compliance/`                | 6 markdown files: data-classification, data-retention, gdpr-procedures, regulatory-mapping, dr-drill-template, README.   |
| `docs/audit/`                     | This audit output. New in this pass.                                                                                      |
| `docs/PRODUCTION-TODO.md`         | Phased remediation backlog with per-ticket briefs.                                                                        |
| `docs/ROADMAP.md`                 | Coarser roadmap.                                                                                                          |

`[FINDING:high]` The three "binding" reference documents (`Architecture`,
`Companion`, `Concept-Note`) are `.docx` binaries. Doctrine D5 (documentation
is part of the feature) and D19 (reproducible everything) presuppose
text-diffable canonical sources. Any code-vs-architecture drift currently
requires an out-of-band tool (`_extracted/` is the local extraction sandbox)
to compare. The cited section anchors (e.g., "Architecture V4 P14")
cannot be verified mechanically from the audit snapshot.

---

## 2. Build system

### 2.1 Package managers & toolchain pins

| Layer        | Pin                                        | Source                                  |
|--------------|--------------------------------------------|-----------------------------------------|
| Rust         | `1.88.0` workspace channel                 | `rust-toolchain.toml`                   |
| Rust (mise)  | `1.84.0` *(divergent — see finding)*       | `mise.toml` line `rust = "1.84.0"`      |
| Edition      | `2024`                                     | `Cargo.toml` `[workspace.package]`      |
| rust-version | `1.85`                                     | `Cargo.toml` `[workspace.package]`      |
| Go           | `1.26.2` (mise) / `1.22` (chaincode go.mod)| `mise.toml`, `chaincode/audit-witness/go.mod` |
| Node         | `22.11.0`                                  | `mise.toml`                             |
| pnpm         | `9.12.3`                                   | `mise.toml`                             |
| Python       | `3.12.7`                                   | `mise.toml`                             |
| uv           | `0.5.4`                                    | `mise.toml`                             |
| Java         | `21.0.4`                                   | `mise.toml` ("only for the HSM SDK adapters") — no Java sources in tree |
| buf          | `1.47.2`                                   | `mise.toml`                             |
| just         | `1.36.0`                                   | `mise.toml`                             |
| terraform    | `1.10.0`                                   | `mise.toml`                             |
| kubectl      | `1.32.0`                                   | `mise.toml`                             |
| helm         | `3.16.3`                                   | `mise.toml`                             |
| sops, age, yq, jq | latest pins in `mise.toml`           |                                         |

`[FINDING:high]` **Rust toolchain split-brain.** `rust-toolchain.toml` pins
`channel = "1.88.0"` (line 5). `mise.toml` pins `rust = "1.84.0"` (line 6).
Per-service `CLAUDE.md` files (declaration, V-engine, person, entity) all
quote `Rust 1.88.0 (rust-toolchain.toml)`. Engineers running `mise install`
get 1.84.0; engineers running `cargo` get 1.88.0 (rustup resolves
`rust-toolchain.toml`). Doctrine D19 (reproducible everything) prohibits
this mismatch. `Cargo.toml`'s `rust-version = "1.85"` adds a third value.

### 2.2 Workspace protocol

- **Cargo workspace** at repo root (`Cargo.toml` line 17–32). Single `Cargo.lock`. Centralised `[workspace.dependencies]` with `dep.workspace = true` per member.
- **pnpm workspace**: not configured at repo root. The only `package.json` is `applications/declarant-portal/package.json` (single project). `[FINDING:low]` justfile's `pnpm prettier`, `pnpm tsc`, `pnpm eslint` calls at repo root will only run inside the portal dir or fail; no root `pnpm-workspace.yaml`.
- **No Bazel BUILD files.** `[FINDING:high]` justfile `build:` target runs `bazel build //...` but there is no `WORKSPACE` / `MODULE.bazel` / `BUILD.bazel` anywhere in the tree. The "build via Bazel" claim in the justfile header is aspirational. Engineers actually build via `cargo build --workspace` and `pnpm build`.

### 2.3 Lockfiles

| File                                       | Present | Notes                                                                                                                      |
|--------------------------------------------|---------|----------------------------------------------------------------------------------------------------------------------------|
| `Cargo.lock`                               | yes     | 125 312 lines. Committed (workspace lockfile).                                                                              |
| `services/declaration/Cargo.lock`          | yes     | `[FINDING:medium]` Stale per-service lockfile — workspace `Cargo.lock` is authoritative. Should be removed (D8 dangling thread). |
| `services/verification-engine/Cargo.lock`  | yes     | `[FINDING:medium]` Same as above.                                                                                          |
| `applications/declarant-portal/pnpm-lock.yaml` | yes  | pnpm-managed.                                                                                                              |
| `chaincode/audit-witness/go.sum`           | *not in listing* | Verify on inspection.                                                                                              |
| Per-package `Cargo.lock` elsewhere         | no      |                                                                                                                            |

### 2.4 Build tools per crate

| Crate                          | Build steps beyond `cargo build`                                                                                                                                |
|--------------------------------|-----------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `services/declaration`         | `build.rs` invokes `tonic-build` to compile `contracts/declaration.proto` into `$OUT_DIR/recor.declaration.v1.rs`; consumed by `src/api/grpc.rs::tonic::include_proto!`. |
| `apps/audit-verifier`          | Dockerfile present. No build.rs.                                                                                                                                |
| `apps/worker-fabric-bridge`    | Dockerfile present. No build.rs.                                                                                                                                |
| `services/verification-engine` | Dockerfile present. No build.rs (no gRPC surface).                                                                                                              |
| `services/person-service`      | Dockerfile present.                                                                                                                                             |
| `services/entity-service`      | Dockerfile present.                                                                                                                                             |
| Portal                         | `vite build` (TypeScript check via `tsc --noEmit` then Vite bundling) → Dockerfile multi-stage `node:22-bookworm` → `nginx:1.27-alpine`. PWA service worker generated by Workbox via `vite-plugin-pwa`. |
| Chaincode                      | `go build` (Fabric chaincode lifecycle handles packaging).                                                                                                       |

### 2.5 Release profile

`Cargo.toml` `[profile.release]`: `opt-level = 3`, `lto = "fat"`,
`codegen-units = 1`, `strip = "debuginfo"`, `panic = "abort"`.
This matches a production-grade profile (slow build, fast binary,
no panic unwinding).

`SOURCE_DATE_EPOCH = 1735689600` (2025-01-01 UTC) is set in `mise.toml [env]`
for reproducible builds (D19). Cargo build does not natively consume this;
deterministic Docker builds in the Dockerfiles would need to honour it
explicitly. **Not verified for the four service Dockerfiles in this pass.**

---

## 3. Test system

### 3.1 Test runners present

| Runner               | Configured?                                                            | Notes                                                                       |
|----------------------|------------------------------------------------------------------------|-----------------------------------------------------------------------------|
| `cargo test` (libtest) | Yes (default).                                                       | Used by `cargo nextest run --workspace` in justfile.                        |
| `cargo nextest`      | Referenced from justfile `_check-rust` and `test`. No `.config/nextest.toml` committed. | `[FINDING:low]` nextest config defaults; no parallelism / retry pinning.    |
| `proptest 1.5`       | Workspace dependency. Used for property tests (e.g., `audit_immutability.rs`). |                                                                       |
| `rstest 0.23`        | Workspace dep.                                                          |                                                                              |
| `testcontainers 0.23` + `testcontainers-modules` (postgres) | Workspace dev-deps.        | Used by integration tests in `services/declaration/tests/`.                  |
| `vitest`             | Configured in portal `vite.config.ts` and dev-deps.                    | `applications/declarant-portal/tests/setup.ts` is the test bootstrap.       |
| `@playwright/test`   | Portal dev-dep. Config at `applications/declarant-portal/playwright.config.ts`. | 5 E2E spec files (`tests/e2e/*.spec.ts`).                            |
| Go `testing`         | `chaincode/audit-witness/` has `testify` 1.9.0 dep.                    | Test files: not enumerated in this pass.                                     |
| `conftest`           | justfile `_check-policies` invokes it on `tests/policy/`.              | `[FINDING:medium]` `tests/policy/` does not exist; target will fail.        |

### 3.2 Test directories + counts (Rust)

Counted via `grep -rn '^#\[\(tokio::\)\?test\]\|^    #\[\(tokio::\)\?test\]'`
across each crate. Counts include both `#[test]` and `#[tokio::test]`,
unit + integration.

| Crate / app                              | Test attribute count | Test dirs                                                                                                |
|------------------------------------------|----------------------|----------------------------------------------------------------------------------------------------------|
| `services/declaration`                   | 162                  | Unit (`src/**/*`); integration (`tests/api_integration.rs`, `tests/audit_immutability.rs`, `tests/grpc_integration.rs`, `tests/kafka_integration.rs`, `tests/log_redaction_integration.rs`, `tests/oidc_integration.rs`, `tests/rate_limit_integration.rs`, `tests/writeback_contract.rs`). Fixtures in `tests/fixtures/` (RSA pem + JWK). |
| `services/verification-engine`           | 86                   | Unit only (`src/**/*`). Integration fixtures in `tests/fixtures/` but no `tests/*.rs` integration files. `[FINDING:medium]` declared 9-stage adversarial pipeline has no integration harness committed. |
| `services/person-service`                | 30                   | Unit only.                                                                                                |
| `services/entity-service`                | 24                   | Unit + `tests/integration_smoke.rs` (one file).                                                           |
| `apps/audit-verifier`                    | 14                   | Unit only.                                                                                                |
| `apps/worker-fabric-bridge`              | 16                   | Unit only.                                                                                                |
| `packages/recor-auth-oidc`               | 13                   | Unit only.                                                                                                |
| `packages/recor-logging`                 | 27                   | Unit only.                                                                                                |
| `packages/recor-spiffe`                  | 30                   | Unit only.                                                                                                |
| `packages/recor-vault-client`            | 8                    | Unit only.                                                                                                |
| `packages/recor-inference-gateway`       | 11                   | Unit only.                                                                                                |
| `packages/fabric-bridge`                 | 15                   | Unit only.                                                                                                |
| **Rust subtotal**                        | **436**              |                                                                                                          |

### 3.3 Test directories + counts (TypeScript / portal)

`applications/declarant-portal/`:

| Type   | File count | Files                                                                                                                                                                                       |
|--------|------------|---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| Vitest | 7          | `src/App.test.tsx`, `src/lib/api.test.ts`, `src/lib/crypto.test.ts`, `src/lib/drafts/__tests__/drafts.test.ts`, `src/features/declaration/VerificationStatus.test.tsx`, `src/features/declaration/wizard/__tests__/useDraftAutosave.test.tsx`, `src/features/declaration/wizard/__tests__/DeclarationWizard.test.tsx`. |
| Playwright | 5 specs | `tests/e2e/{happy-path,verification-rejected,polling-stops,validation,a11y-smoke}.spec.ts`. Helper: `tests/e2e/fixtures.ts`.                                                              |

### 3.4 Repository-level test dirs

| Path                             | Contents                                                                                                                                       |
|----------------------------------|------------------------------------------------------------------------------------------------------------------------------------------------|
| `tests/contract/`                | `codeowners.test.sh`, `observability-smoke.test.sh`. Fixtures in `tests/contract/fixtures/codeowners-bad/`.                                    |
| `tests/e2e/`                     | **Empty.** `[FINDING:medium]` D11 ('two reviewers, at least one cross-team') and D4 (tests part of feature) imply cross-service e2e coverage. The portal's Playwright is the only E2E harness. |
| `tests/chaos/`                   | **Empty.** `[FINDING:high]` Doctrine D14 fail-closed and D9 ('holy shit, that's done') suggest chaos coverage for sovereign infrastructure.    |
| `tests/performance/`             | **Empty.** `[FINDING:high]` No load tests committed. README claims 4 Grafana dashboards exist; nothing exercises latency SLOs.                 |
| `tests/policy/`                  | **Missing.** justfile references it; absent.                                                                                                   |

### 3.5 Run-all and run-single commands

**All tests** (justfile `test:`):

```
cargo nextest run --workspace
go test ./...
pnpm vitest run
```

`[FINDING:low]` `pnpm vitest run` at the repo root won't resolve — `pnpm`
has no workspace. Engineers actually run it from `applications/declarant-portal/`.

**Single test (Rust)**:

```
cargo nextest run --workspace --no-fail-fast -E 'test(<name>)'
cargo test -p recor-declaration --test api_integration -- --ignored <name>
```

Integration tests under `services/declaration/tests/` are gated on
`--ignored` in some files (testcontainers).

**Single test (Vitest)**:

```
cd applications/declarant-portal && pnpm vitest run path/to/test.test.ts
```

**Single Playwright**:

```
cd applications/declarant-portal && pnpm exec playwright test tests/e2e/happy-path.spec.ts
```

**Go (chaincode)**:

```
cd chaincode/audit-witness && go test ./...
```

---

## 4. Runtime topology — every long-running process the system expects

| Process                          | Bin / image                                          | Port(s)                                       | Talks to                                                                            | Composition root                                |
|----------------------------------|------------------------------------------------------|-----------------------------------------------|-------------------------------------------------------------------------------------|-------------------------------------------------|
| Declaration service              | `recor-declaration` (axum + tonic)                   | REST `BIND_ADDR` (compose: 8080), gRPC separate (`GRPC_BIND_ADDR`) | Postgres, Kafka (optional, via `RELAY_TRANSPORT=kafka`), V-engine (writeback), Vault (boot), OIDC issuer (JWKS) | `services/declaration/src/main.rs`              |
| Verification engine              | `recor-verification-engine` (axum)                   | REST `BIND_ADDR`                              | Postgres, Kafka (consumer), Declaration service (HTTP writeback), Vault, OIDC issuer | `services/verification-engine/src/main.rs`      |
| Person service                   | `recor-person-service` (axum)                        | REST                                          | Postgres, OIDC issuer                                                               | `services/person-service/src/main.rs`           |
| Entity service                   | `recor-entity-service` (axum)                        | REST                                          | Postgres, OIDC issuer                                                               | `services/entity-service/src/main.rs`           |
| Worker — Fabric bridge           | `worker-fabric-bridge` (axum + Fabric Gateway)       | REST (`/v1/relay`, healthz, readyz, metrics)  | Hyperledger Fabric peer (Gateway gRPC), Postgres (DLQ), upstream HTTP relayer       | `apps/worker-fabric-bridge/src/main.rs`         |
| Audit verifier                   | `audit-verifier` (axum)                              | REST                                          | Hyperledger Fabric (gateway HTTP), Postgres (projection)                            | `apps/audit-verifier/src/main.rs`               |
| Declarant portal                 | nginx serving the static SPA                         | 80 / 443 (nginx)                              | Declaration REST (POST submit, GET status), browser → no other backend              | `applications/declarant-portal/nginx.conf.template` |
| Postgres                         | Container (per-service compose; production unspecified) | 5432                                       | All services                                                                        | `services/*/docker-compose.yaml`                |
| Kafka (KRaft)                    | Container (`infrastructure/kafka/`)                  | 9092                                          | Declaration (producer/consumer), V-engine (consumer)                                | `infrastructure/kafka/docker-compose.yaml`      |
| OTel Collector                   | Container (`infrastructure/observability-dev/`)      | 4317 (OTLP gRPC), 4318 (OTLP HTTP)            | Tempo, Loki, Prometheus                                                             | `infrastructure/observability-dev/docker-compose.yaml` |
| Prometheus                       | Container                                            | 9090                                          | Scrapes `/metrics` on every service                                                 | as above                                        |
| Tempo                            | Container                                            | 3200                                          | Receives traces from OTel Collector                                                 | as above                                        |
| Loki                             | Container                                            | 3100                                          | Receives logs                                                                       | as above                                        |
| Grafana                          | Container                                            | 3000                                          | Tempo + Loki + Prometheus                                                           | as above                                        |
| SPIRE server                     | Container                                            | (per `infrastructure/spire/server.conf`)      | SPIRE agents → all Rust services for SVID                                           | `infrastructure/spire/docker-compose.yaml`      |
| SPIRE agent                      | Container                                            |                                               | SPIRE server, Workload API socket consumed by services                              | `infrastructure/spire/agent.conf`               |
| Vault                            | Container (dev mode in compose)                      | 8200                                          | All Rust services (`packages/recor-vault-client`)                                  | `infrastructure/vault/docker-compose.yaml`      |
| Hyperledger Fabric peer + orderer | Not bundled in repo; runbook implies external/managed test net | gRPC                                | `apps/worker-fabric-bridge`, `apps/audit-verifier`                                  | not committed `[FINDING:high]`                  |
| OIDC IdP (Keycloak or similar)   | Not bundled in repo                                  | configurable via `OIDC_ISSUER_URL`            | Every Rust service (JWKS fetch + token verify)                                      | not committed `[FINDING:medium]` — dev path uses `is_dev = true` to bypass OIDC entirely |

**Process-count for a "full local stack" boot**: 4 services + 2 apps + portal nginx + Postgres + Kafka + Vault + SPIRE (2) + 5 obs containers + external IdP + external Fabric ≈ **18+ containers**. No top-level compose file orchestrates them all; the local-up flow (`tools/cli/local-up.sh`) is referenced in the justfile but **`tools/cli/` is empty** (`[FINDING:high]`).

---

## 5. External infrastructure dependencies (production-named)

These are services the platform expects to consume in production. Each is
referenced by name + protocol from code or runbook citations.

| External vendor / system                   | Protocol / surface           | Code / config reference                                                                                  | Production status                              |
|--------------------------------------------|------------------------------|----------------------------------------------------------------------------------------------------------|------------------------------------------------|
| **PostgreSQL** (host TBD)                  | Postgres wire 5432, TLS      | `DATABASE_URL` env on every service; sqlx pool in each `main.rs`.                                       | Production cluster not specified; backups documented in `docs/runbooks/restore-database-from-backup.md`. |
| **Apache Kafka** (cluster TBD)             | Kafka 2.x+ wire              | `KAFKA_BROKERS`; `rdkafka 0.36`. Adapter: `services/declaration/src/infrastructure/kafka_producer.rs`, `services/verification-engine/src/infrastructure/kafka_consumer.rs`. | Production: `[FINDING:high]` no committed prod K8s/Helm for Kafka; ADR-0007 documents the cutover. |
| **Anthropic API (Claude Opus 4.7)**        | HTTPS Messages API           | `packages/recor-inference-gateway/`; D22 (Anthropic-primary). Used by V-engine stages.                  | Live SaaS. API key in Vault per OPS-4.        |
| **Bedrock PrivateLink af-south-1 (Tier B)**| HTTPS Bedrock Messages API   | Referenced in `ARCHITECTURE.md` § Three inference tiers; no client code present. `[FINDING:high]`        | Not implemented.                               |
| **Llama 3.3 70B / Mistral Large 2 (Tier C)** | On-prem H100 inference     | Referenced in ARCHITECTURE.md; no client code present. `[FINDING:high]`                                  | Not implemented.                               |
| **Hyperledger Fabric (audit channel)**     | Fabric Gateway gRPC          | `packages/fabric-bridge/`; chaincode in `chaincode/audit-witness/`. Worker: `apps/worker-fabric-bridge`. | Peer + orderer external; no managed-cluster manifests committed. |
| **HashiCorp Vault** (HA cluster)           | HTTP API + AppRole           | `packages/recor-vault-client/`; ADR uses `OPS-4`. Bootstrap script: `infrastructure/vault/scripts/`.    | Dev-only compose committed; production deployment not in repo. |
| **SPIRE** (workload identity)              | Workload API (Unix socket) + SPIFFE SVID | `packages/recor-spiffe/`; ADR-0008.                                                            | Dev-only compose committed.                    |
| **OIDC Identity Provider** (Cameroon NDI / Keycloak shim) | OIDC discovery + JWKS | `packages/recor-auth-oidc/`; per-service `OIDC_ISSUER_URL`. ADR-0004.                            | External; no provider deployed in repo.        |
| **BUNEC adapter** (commercial registry source) | HTTPS (TBD)             | `services/verification-engine/src/infrastructure/bunec_real.rs` (real adapter, stub today); `mock_bunec.rs` (testing). Runbook: `docs/runbooks/bunec-adapter-outage.md`. | Adapter wired; partnership agreement pending per PRODUCTION-TODO §R-VER-1. |
| **Sanctions feeds** (OFAC, UN, EU)         | HTTPS / S3 (TBD)             | `services/verification-engine/src/infrastructure/sanctions_postgres.rs` — Postgres cache only. Ingestion path not present. `[FINDING:medium]` |                                                |
| **PEP data source**                        | HTTPS / S3 (TBD)             | `pep_postgres.rs` — Postgres cache only. Ingestion path not present.                                    |                                                |
| **ICIJ Offshore Leaks**                    | bulk download                | `icij_postgres.rs` — Postgres cache only. Ingestion not present.                                        |                                                |
| **Adverse-media providers**                | HTTPS (TBD)                  | Stage 5 of V-engine; current implementation is `stage_5_adverse_media_stub.rs`.                          |                                                |
| **OpenTimestamps**                         | HTTPS + Bitcoin              | Referenced in ARCHITECTURE.md L0 stack; no integration code present. `[FINDING:medium]`                  | Not implemented.                               |
| **Object store (MinIO / S3)**              | S3 wire                      | ARCHITECTURE.md L1 lists MinIO; no S3 client code present in services. `[FINDING:medium]`                | Not implemented.                               |
| **Neo4j**                                  | Bolt 7687                    | ARCHITECTURE.md L1; `0006_graph_views.sql` is a Postgres-side projection, not Neo4j. `[FINDING:medium]`  | Not implemented.                               |
| **OpenSearch**                             | OpenSearch wire              | ARCHITECTURE.md L1; no client code present. `[FINDING:medium]`                                            | Not implemented.                               |
| **Redis**                                  | RESP                         | ARCHITECTURE.md L1; no client code present. `[FINDING:medium]`                                            | Not implemented.                               |
| **HSMs**                                   | PKCS#11 / native             | ARCHITECTURE.md L0; mise installs Java for HSM SDK adapters; no Java source. `[FINDING:medium]`           | Not implemented.                               |
| **FROST threshold-signature substrate**    | (lib TBD)                    | ARCHITECTURE.md L0 "FROST"; no library or threshold-sign code. `[FINDING:high]`                          | Not implemented — claimed in ARCHITECTURE for consortium governance. |
| **Halo2 zero-knowledge substrate**         | Rust crate (Halo2)           | ARCHITECTURE.md L0; no Halo2 crate dependency in `Cargo.lock`. `[FINDING:high]`                          | Not implemented.                               |

The "claimed in architecture / not present in code" set is the dominant
production-readiness gap. Pass C of this audit (Section 8 of the audit
spec) will weight these against doctrine D9 ("holy shit, that's done").

---

## 6. Cryptographic dependencies

All versions from `Cargo.lock`.

| Crate                  | Version | Use site                                                                  | Audit status / last release          | Notes                                                                                                                                                |
|------------------------|---------|---------------------------------------------------------------------------|--------------------------------------|------------------------------------------------------------------------------------------------------------------------------------------------------|
| `ed25519-dalek`        | 2.2.0   | Declaration attestation (browser-side + server-side verify); test fixtures. | Maintained; 2.x line audited via downstream review. Last release ≈ 2025. | Used in `services/declaration/src/domain/attestation.rs`, `services/verification-engine/src/api/internal.rs`, `apps/audit-verifier`.                  |
| `ed25519`              | 2.2.3   | Transitive (signature/key types).                                         |                                      |                                                                                                                                                      |
| `blake3`               | 1.8.5   | Receipt derivation (D15); PII redaction MAC; outbox content hashes.        | Reference impl maintained by BLAKE3 team. | `services/declaration/src/api/rest.rs:13` `use blake3::Hasher`; `packages/recor-logging`.                                                            |
| `hmac`                 | 0.12.1  | D↔V channel auth (`/v1/internal/*`); RustCrypto.                          | RustCrypto. Mature.                  | `services/declaration/src/api/internal.rs`.                                                                                                          |
| `sha2`                 | 0.10.9  | HMAC-SHA-256 backing; OIDC.                                               | RustCrypto.                          |                                                                                                                                                      |
| `ring`                 | 0.17.14 | rustls crypto provider.                                                   | Audited by `cure53`/openssl-team-equivalent reviewers; 0.17 stable. |                                                                                                                                            |
| `rustls`               | 0.23.40 | TLS for sqlx, reqwest, tonic, axum, SPIFFE.                               | Maintained. 0.23 LTS-equivalent.     | Workspace-wide.                                                                                                                                       |
| `rustls-webpki`        | 0.103.13 | Cert validation.                                                         |                                      |                                                                                                                                                      |
| `rustls-pki-types`     | 1.14.1  |                                                                           |                                      |                                                                                                                                                      |
| `rustls-pemfile`       | 2.2.0   | SPIFFE bundle parsing.                                                    |                                      |                                                                                                                                                      |
| `tokio-rustls`         | 0.26.4  | TLS adapter.                                                              |                                      |                                                                                                                                                      |
| `rsa`                  | 0.9.10  | OIDC RS256 verification (transitive via `jsonwebtoken`).                  | Maintained; RustCrypto.              | `[FINDING:low]` `rsa` crate has a known weakness window for non-constant-time decrypt (RUSTSEC-2023-0071 era); 0.9.x removed the affected path.       |
| `jsonwebtoken`         | 9.3.1   | OIDC token verify; HS* algorithms refused at config time (`packages/recor-auth-oidc`). | Maintained; widely used.    |                                                                                                                                                       |
| `secrecy`              | 0.10.3  | SecretString wrappers around DB URL, HMAC keys, OIDC client secret, Vault token. | RustCrypto-adjacent. Mature.   | D18 (no secrets in logs).                                                                                                                            |
| `uuid` v7              | 1.11    | All event/aggregate IDs.                                                  | Mature.                              |                                                                                                                                                       |
| `tonic` (gRPC)         | 0.13    | Pulls in `tonic-tls` via rustls when in mTLS mode.                        |                                      |                                                                                                                                                       |

**Crypto coverage observations:**

- `[FINDING:high]` Doctrine D21 ('post-quantum agility') is referenced in ARCHITECTURE.md and CLAUDE.md but no PQ-capable signature or KEM dependency is present (no `pqcrypto`, `ml-kem`, `kyber`, `falcon`, etc.). The current attestation is Ed25519 only.
- `[FINDING:high]` Doctrine D15 ('cryptographic provenance on every consequential event') is implemented via Ed25519 + BLAKE3 + Fabric anchoring; a) FROST threshold signing (claimed in ARCH L0) is not present; b) OpenTimestamps anchoring (claimed) is not present.
- `[FINDING:medium]` Browser-side Ed25519 in the portal uses **Web Crypto API natively, no third-party crypto library** (per `applications/declarant-portal/CLAUDE.md`). Web Crypto's Ed25519 support is recent across browsers (Chrome 113+, Safari 17+, Firefox 130+). The portal's i18n claims FR/EN/Pidgin support; older mobile browsers on lower-end Android may not have Ed25519 in `crypto.subtle`. There is no fallback library — a declarant on an older browser sees a generic crypto failure. The `crypto.test.ts` parity test does not cover the polyfill question.
- HMAC secret rotation: per-channel dual-secret scheme (current + old) is wired (`services/*/src/api/internal.rs`); rotation runbook at `docs/runbooks/hmac-secret-rotation.md`. Cryptographically correct (D15 + D17).

---

## 7. Documentation index — every markdown in `docs/` with freshness

`Freshness key:`
- **Fresh** = last commit within 14 days (since 2026-04-29).
- **Recent** = within 30 days.
- **Stale** = older than 30 days.

### 7.1 ADRs (`docs/adr/`)

| File                                                  | Last commit ISO        | Freshness | One-line purpose                                                            |
|-------------------------------------------------------|------------------------|-----------|-----------------------------------------------------------------------------|
| `0001-event-sourcing-declaration-aggregate.md`        | 2026-05-12             | Fresh     | Event sourcing for the Declaration aggregate.                               |
| `0002-dempster-shafer-fusion.md`                      | 2026-05-12             | Fresh     | Dempster–Shafer fusion over Bayesian for verification.                      |
| `0003-http-outbox-relay-d-v.md`                       | 2026-05-12             | Fresh     | HTTP outbox-relay between D and V (Kafka follow-up).                        |
| `0004-oidc-jwks-principal-authentication.md`          | 2026-05-12             | Fresh     | OIDC + JWKS principal authentication.                                       |
| `0005-hmac-channel-rotation.md`                       | 2026-05-12             | Fresh     | Per-channel HMAC secrets + dual-secret rotation.                            |
| `0006-observability-stack-choice.md`                  | 2026-05-12             | Fresh     | OTel + Prometheus + Tempo + Loki + Grafana.                                 |
| `0007-kafka-transport-cutover.md`                     | 2026-05-13             | Fresh     | Kafka transport alongside HTTP relay.                                       |
| `0008-spiffe-mtls.md`                                 | 2026-05-13             | Fresh     | SPIFFE/mTLS for service-to-service auth.                                    |
| `0009-fabric-audit-anchoring.md`                      | 2026-05-12             | Fresh     | Hyperledger Fabric for receipt anchoring.                                   |
| `README.md`                                           | 2026-05-13             | Fresh     | ADR index + MADR template guide.                                            |

### 7.2 Runbooks (`docs/runbooks/`)

| File                                  | Last commit  | Freshness | One-line purpose                                              |
|---------------------------------------|--------------|-----------|---------------------------------------------------------------|
| `audit-verification.md`               | 2026-05-12   | Fresh     | Operator path for forensic audit verification via `apps/audit-verifier`. |
| `bunec-adapter-outage.md`             | 2026-05-12   | Fresh     | Mitigations when BUNEC upstream is down.                       |
| `deploy-new-version.md`               | 2026-05-12   | Fresh     | Cut-a-release path.                                           |
| `dlq-inundation.md`                   | 2026-05-12   | Fresh     | DLQ replay procedure.                                          |
| `fabric-bridge.md`                    | 2026-05-12   | Fresh     | Worker-fabric-bridge operations.                              |
| `hmac-secret-rotation.md`             | 2026-05-12   | Fresh     | Per-channel HMAC rotation runbook.                            |
| `image-verification.md`               | 2026-05-12   | Fresh     | Cosign / SLSA provenance verification at deploy.              |
| `incident-response-template.md`       | 2026-05-12   | Fresh     | Incident response template.                                   |
| `observability-dashboards.md`         | 2026-05-12   | Fresh     | Dashboard inventory.                                          |
| `observability-dev-stack.md`          | 2026-05-11   | Fresh     | Local OTel stack bring-up.                                    |
| `observability-prod-stack.md`         | 2026-05-12   | Fresh     | Production stack composition.                                  |
| `oidc-issuer-outage.md`               | 2026-05-12   | Fresh     | OIDC issuer outage drill.                                     |
| `oncall-triage-tree.md`               | 2026-05-12   | Fresh     | First-responder decision tree.                                |
| `restore-database-from-backup.md`     | 2026-05-12   | Fresh     | Postgres PITR.                                                |
| `restore-from-backup.md`              | 2026-05-12   | Fresh     | Generic restore steps. `[FINDING:low]` overlaps with `restore-database-from-backup.md`; near-duplicate. |
| `rollback-deployment.md`              | 2026-05-12   | Fresh     | Rollback path.                                                |
| `soft-launch-playbook.md`             | 2026-05-12   | Fresh     | Soft-launch flight plan.                                      |
| `spiffe-onboarding.md`                | 2026-05-13   | Fresh     | New-service SPIFFE entry registration.                        |
| `sqlx-cache-regeneration.md`          | 2026-05-12   | Fresh     | `cargo sqlx prepare` flow.                                    |
| `supply-chain.md`                     | 2026-05-12   | Fresh     | SLSA + cosign supply-chain operations.                        |
| `vault-onboarding.md`                 | 2026-05-12   | Fresh     | New-service Vault AppRole + secret bootstrap.                 |

### 7.3 Security (`docs/security/`)

| File                                  | Last commit  | Freshness | Purpose                                                       |
|---------------------------------------|--------------|-----------|---------------------------------------------------------------|
| `README.md`                           | 2026-05-13   | Fresh     | Security docs index.                                          |
| `a11y-audit-2026-Q2.md`               | 2026-05-13   | Fresh     | Accessibility audit record (WCAG 2.1 AA, R-PORT-5).           |
| `branch-protection.md`                | 2026-05-12   | Fresh     | `main` branch protection spec + apply script (CI-3).          |
| `pen-test-prep.md`                    | 2026-05-12   | Fresh     | Pen-test scoping + engagement package.                        |
| `pen-test-rules-of-engagement.md`     | 2026-05-12   | Fresh     | Pen-test ROE.                                                 |
| `teams.md`                            | 2026-05-11   | Fresh     | Security team roster + roles.                                 |
| `threat-model.md`                     | 2026-05-13   | Fresh     | STRIDE threat model per component (DOC-4).                    |

### 7.4 Compliance (`docs/compliance/`)

| File                                  | Last commit  | Freshness | Purpose                                                       |
|---------------------------------------|--------------|-----------|---------------------------------------------------------------|
| `README.md`                           | 2026-05-12   | Fresh     | Compliance index.                                             |
| `data-classification.md`              | 2026-05-12   | Fresh     | Per-column classification: Public / Internal / Confidential / PII / Sensitive-PII (COMP-3). |
| `data-retention.md`                   | 2026-05-12   | Fresh     | Per-table retention + append-only event log (COMP-2).         |
| `dr-drill-template.md`                | 2026-05-12   | Fresh     | DR drill template (COMP-5).                                   |
| `gdpr-procedures.md`                  | 2026-05-12   | Fresh     | Six GDPR data-subject rights mapped to endpoints (COMP-1).    |
| `regulatory-mapping.md`               | 2026-05-12   | Fresh     | Endpoint → legal provision map (Cameroon law, OHADA, FATF, GDPR). `[FINDING:medium]` per README — every cited legal instrument carries a `[CITATION NEEDED]` marker pending counsel sign-off. |

### 7.5 Top-level

| File                                  | Last commit  | Freshness | Purpose                                                       |
|---------------------------------------|--------------|-----------|---------------------------------------------------------------|
| `PRODUCTION-TODO.md`                  | 2026-05-12   | Fresh     | Phased remediation backlog with per-ticket briefs.            |
| `ROADMAP.md`                          | 2026-05-11   | Fresh     | Quarterly roadmap.                                            |
| `architecture/RECOR-Software-Architecture-Document.docx` | binary | n/a   | Authoritative architecture text (V1–V5). `[FINDING:high]` non-diffable. |
| `companion/RECOR-Implementation-Companion.docx`         | binary | n/a   | Implementation companion. `[FINDING:high]` non-diffable.    |
| `concept-note/RECOR-Concept-Note.docx`                   | binary | n/a   | Concept note. `[FINDING:medium]` non-diffable.              |
| `build-spec/RECOR-Sovereign-Build-Specification.docx`    | binary | n/a   | Sovereign build spec. `[FINDING:high]` non-diffable.        |
| `openapi/declaration.json`            | generated    | n/a       | OpenAPI 3.1 snapshot from `dump-openapi` bin.                 |

**Observation:** every markdown doc in scope is "Fresh" by the 14-day
standard — meaning the docs are actively maintained. The risk axis is
not doc rot; it is doc-vs-code drift, which downstream audit sections
enumerate.

---

## 8. Conventions detected

What the code *actually* does, not what the architecture aspires to. All
citations are file paths in the worktree.

### 8.1 Layering / hexagonal structure

Every Rust service follows an identical 5-folder hexagonal layout:

```
src/
├── api/           # axum routers + handlers (the "left" adapter)
├── application/   # use cases (`*_usecase.rs`) + port traits (`port.rs`)
├── domain/        # aggregates, events, value objects (zero outward deps)
├── infrastructure/# postgres + kafka + relay (the "right" adapters)
├── config.rs      # env-config loader
├── error.rs       # one ServiceError enum + IntoResponse impl
├── metrics.rs     # Prometheus registry + middleware
├── observability.rs # tracing/OTel init
├── lib.rs / main.rs
```

Verified across `services/{declaration,verification-engine,person-service,entity-service}/`.
This is doctrine D6-and-D12 compliant ("Production-grade from the first
commit"; "Plan before writing code"). `[FINDING:low]` `apps/audit-verifier`
and `apps/worker-fabric-bridge` use a flat module layout (no `api/`,
no `application/`); minor convention break, defensible given they're
thin shells.

### 8.2 Naming

- Crates: `recor-<service>` (Rust convention `kebab-case`).
- Lib paths: `recor_<service>` (Rust `snake_case` for `name = ...`).
- Use cases: `<verb>_<noun>.rs` per file, struct named `<Verb><Noun>UseCase` (e.g., `SubmitDeclarationUseCase`).
- Migrations: `NNNN_description.sql`, zero-padded 4-digit prefix.
- Tests: `*_integration.rs` for `tests/` integration files; `tests` mod inside the file under test for unit tests.

### 8.3 Error handling

- Per service: one `error::ServiceError` enum with `IntoResponse` for axum.
- Repository / use case layer: returns `anyhow::Error` is **avoided** in domain; `anyhow` reserved for `main.rs` composition root.
- `thiserror` 2.x is the workspace default for typed errors (`Cargo.toml` line 72).
- D14 (fail-closed): handler defaults to 500 with no body on uncategorised error. Verified in `services/declaration/src/error.rs`.

### 8.4 Logging / observability

- `tracing` + `tracing-subscriber` JSON formatter (workspace-pinned: `tracing-subscriber 0.3`).
- `tracing-opentelemetry 0.28` → OTLP gRPC export. Endpoint via `OTEL_EXPORTER_OTLP_ENDPOINT` env.
- PII redaction is enforced by a custom `tracing` layer (`packages/recor-logging`) that BLAKE3-MACs principals/UUIDs/SPIFFE URIs/receipts at span field record time. Toggled via `LOG_REDACTION` env (`enabled` | `disabled-for-dev` | `disabled`) + `LOG_REDACTION_KEY` (32-byte hex MAC key). D18 enforced.
- Prometheus metrics: per-service `metrics.rs`, single Registry per service. Bounded-enum label values only (per workspace-deps comment on `prometheus`).

### 8.5 Config loading

- All services use the `config 0.15` crate (no defaults — `default-features = false, features = ["toml"]`). The runtime path is env-only via `Config::from_env()`.
- Sensitive values wrapped in `secrecy::SecretString`.
- Vault bridge (`packages/recor-vault-client`): when `VAULT_ADDR` is set, secrets fetched from Vault paths *before* `Config::from_env()` runs and injected into process env. When `VAULT_ADDR` is empty, env-only mode with a `warn!` at startup. D14: non-empty `VAULT_ADDR` with unreachable Vault hard-fails.

### 8.6 HTTP server stack

- `axum 0.8` + `tower 0.5` + `tower-http 0.6` (trace, request-id, timeout, compression-gzip).
- Each service uses a 4-router merge pattern: `protected` (OIDC bearer auth) + `admin` (auth + admin-principal gate) + `internal` (HMAC) + `public` (`healthz`, `readyz`). Plus a sibling `metrics_router` for `/metrics`. Plus the OpenAPI/`/docs` sibling (declaration only).
- Authentication: bearer token → `recor-auth-oidc::OidcVerifier` (JWKS-cached, HS* refused). Service-to-service: HMAC over canonical payload (`/v1/internal/*`) with dual-secret rotation; mTLS via SPIFFE optional (`AUTH_TRANSPORT=hmac|mtls|mtls-only`).
- Rate limiting (OPS-1): `tower_governor 0.8`, keyed by `Principal::subject`, applied only to state-changing POSTs on declaration service.
- CORS allowed for the portal origin (`services/declaration/src/api/rest.rs` middleware section).

### 8.7 Database

- `sqlx 0.8` `runtime-tokio-rustls`, Postgres, `macros` (compile-time-checked queries). Per-service migrations under `services/*/migrations/NNNN_*.sql`.
- Event sourcing pattern: aggregate state derived from `*_events` table; `*` (projection) table is rebuilt from replay. Verified in declaration, person, entity, V-engine.
- Immutability invariant: `BEFORE UPDATE/DELETE/TRUNCATE` triggers + `REVOKE ALL ON ... FROM PUBLIC` on event-log tables. Migration `0007_audit_log_immutability.sql` (declaration) and `0003_audit_log_immutability.sql` (V-engine).
- Outbox pattern: `<aggregate>_outbox` table for downstream relay; DLQ companion table (`*_outbox_dlq`) + admin API.

### 8.8 gRPC

- Only `services/declaration` exposes gRPC (`recor.declaration.v1.DeclarationService`). Proto at `contracts/declaration.proto`; compiled via `build.rs` (tonic-build). OIDC verification mirrored via tonic interceptor in `src/api/grpc.rs`.
- `[FINDING:low]` no `tonic-reflection` is enabled — clients must have the proto out-of-band. Production-acceptable; documented as such in `services/declaration/CLAUDE.md`.

### 8.9 Frontend conventions (portal)

- React 19 functional components; no class components.
- State: TanStack Query for server state; `react-hook-form` for form state.
- Validation: Zod schemas in `src/features/declaration/schema.ts`. Schemas are the canonical source — server DTOs are mirrored manually (no codegen from the same source).
- Crypto: Web Crypto API only (Ed25519, BLAKE3 via WASM); no `noble-curves`, no `tweetnacl`, no third-party crypto lib. Parity test in `src/lib/crypto.test.ts` verifies byte-for-byte equivalence with the server's canonicaliser.
- i18n: `i18next 23.x` + `react-i18next 15.x`. Locales: FR (primary), EN, Pidgin (stub: `"_translation_status": "stub"` marker).
- PWA: `vite-plugin-pwa` with Workbox `autoUpdate`. API endpoints (`/v1/declarations*`) deliberately excluded from cache (`navigateFallbackDenylist`) so offline = real failure, not stale 200.
- Tests: vitest unit + Playwright E2E + axe-core a11y.

### 8.10 Branch protection / repo hygiene

- `.gitleaks.toml` with allowlist for `test_rsa_*.pem` fixtures.
- `.secrets.baseline` (detect-secrets) — 103 KB; large allowlist (audit candidate).
- `.gitattributes` — line-ending normalisation + LFS not configured.
- `.gitignore` — comprehensive, with explicit retention of `services/*/tests/fixtures/test_rsa_*.pem`.
- `.pre-commit-config.yaml` configured (`pre-commit install --install-hooks` in bootstrap).
- `renovate.json` configured.
- `.markdownlint-cli2.jsonc` configured.
- `.trivyignore` — present (Trivy in CI).
- Commit signing: not configured in repo metadata; gitleaks does not check signatures. `[FINDING:low]` D15 cryptographic provenance is anchored at the event level; commit-level provenance via signed commits is a separate axis.

### 8.11 Doctrine alignment summary (orientation only — full doctrine audit deferred)

| Doctrine | Visible enforcement                                                                              | Visible gap                                                                                |
|----------|--------------------------------------------------------------------------------------------------|--------------------------------------------------------------------------------------------|
| D4 (tests part of feature) | Per-crate test counts above; every PR title in PRODUCTION-TODO ties tests to ticket. | `tests/chaos/`, `tests/performance/`, `tests/e2e/` are empty.                              |
| D13 (idempotency)          | Per-service `IdempotencyStore` (declaration, person, entity).                                  |                                                                                            |
| D14 (fail-closed)          | Repeated in handler error.rs paths; bearer-auth middleware refuses no-token with 401.          |                                                                                            |
| D15 (crypto provenance)    | Ed25519 + BLAKE3 + Fabric chaincode + outbox.                                                  | FROST, OpenTimestamps, Halo2 (ARCH-claimed) absent.                                        |
| D16 (observability)        | OTel + Prom + structured tracing; alert-rules.yaml; 4 dashboards.                              | No SLO recording rules surfaced; no synthetic prober.                                      |
| D17 (zero trust)           | OIDC for inbound; HMAC + SPIFFE mTLS for service-to-service.                                   | `[FINDING:high]` no NetworkPolicy / no k8s manifests; `/metrics` exposure model is "trust the cluster". |
| D18 (no secrets in logs)   | `recor-logging` redaction layer; `SecretString` wrappers.                                      |                                                                                            |
| D19 (reproducibility)      | `Cargo.lock` workspace, pinned toolchains in mise.toml, `SOURCE_DATE_EPOCH` env.               | Rust toolchain split-brain (1.84.0 vs 1.88.0); no IaC (terraform empty).                   |
| D20 (SLSA L4)              | `docs/runbooks/supply-chain.md` + `image-verification.md` reference cosign; no `.github/workflows/` listing checked in this pass. |                                                  |
| D21 (post-quantum agility) | n/a                                                                                            | No PQ crypto deps; no agility scaffolding.                                                 |
| D22 (Anthropic-primary)    | `packages/recor-inference-gateway`; D22 cited in workspace deps comments.                      | Tier B (Bedrock) + Tier C (local) not implemented.                                         |

---

## 9. Files referenced in this document (audit evidence trail)

- `Cargo.toml` (workspace root) lines 17–32 (members), 47–139 (deps), 132–139 (release profile).
- `Cargo.lock` (crypto crate inventory).
- `mise.toml` (toolchain pins).
- `rust-toolchain.toml` (alternative rust pin — divergent).
- `justfile` (build / test / gen targets).
- `.env.example` (root); per-service `.env.example` for declaration, verification-engine, portal, observability-dev.
- `.gitignore`, `.gitleaks.toml`, `.secrets.baseline`, `.pre-commit-config.yaml`.
- `services/declaration/Cargo.toml`, `services/declaration/src/main.rs`, `services/declaration/src/api/rest.rs` (route enumeration).
- `services/verification-engine/src/api/rest.rs:1-10` (TODO marker for OpenAPI).
- `services/{verification-engine,person-service,entity-service}/Cargo.toml`.
- `apps/{audit-verifier,worker-fabric-bridge}/Cargo.toml` + `src/main.rs`.
- `packages/*/Cargo.toml`.
- `applications/declarant-portal/package.json`, `vite.config.ts`, `CLAUDE.md`.
- `chaincode/audit-witness/go.mod`.
- `docs/PRODUCTION-TODO.md`, `docs/ROADMAP.md`, `ARCHITECTURE.md`, `README.md`, `CLAUDE.md`.
- `git log -1 --format=%cI` per markdown for freshness.

End of Section 00 (Orientation).
