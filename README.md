# RÉCOR

**Registre de l'Effective Contrôle et Origine Réelle**
**National Beneficial Ownership Registry of Cameroon**

Sovereign-grade national beneficial-ownership registry operated by a
consortium of ten Cameroonian institutions plus international observers
(BUNEC, ARMP, ANIF, DGI, BEAC, customs, sectoral cadastres, CONAC,
INTERPOL/StAR). RÉCOR captures beneficial-ownership declarations, runs
adversarial reasoning over the ownership chains, and exposes the
verified intelligence to authorised consumers and the public.

Build envelope: 18–24 months · funded budget: USD 18–24 M · operating
budget: USD 6–8 M / year. **Not a prototype.**

---

## What's in the box

This repo is a Rust + TypeScript monorepo. The system is event-sourced
end-to-end with a Dempster-Shafer fusion engine at its core.

### Services (Rust, Cargo workspace, axum + sqlx + tonic)

| Crate | Purpose |
|---|---|
| [`services/declaration`](services/declaration/) | Accepts beneficial-ownership declarations from the Declarant Portal. Ed25519 attestation, event-sourced aggregates, BLAKE3 receipt, REST + gRPC surface, idempotency. |
| [`services/verification-engine`](services/verification-engine/) | 9-stage pipeline running adversarial reasoning over the declared chain. Stages 1–7 wired; Dempster-Shafer fusion is the load-bearing module. |
| [`services/person-service`](services/person-service/) | Canonical natural-person registry. Search, merge, immutable event log. Gates declaration submission. |
| [`services/entity-service`](services/entity-service/) | Canonical legal-entity registry. Mirrors person-service shape. |

### Applications

| Path | What |
|---|---|
| [`applications/declarant-portal`](applications/declarant-portal/) | React 19 + Vite 6 + Tailwind v4 SPA. Browser-side Ed25519 signing, 4-step wizard, offline drafts (Workbox + Dexie), i18n (FR/EN/Pidgin), TanStack Query polling. |
| [`apps/audit-verifier`](apps/audit-verifier/) | Public verifier — reads Hyperledger Fabric audit channel and re-derives BLAKE3 hashes from declaration projections. |
| [`apps/worker-fabric-bridge`](apps/worker-fabric-bridge/) | Async bridge — consumes outbox events and anchors them to the Fabric audit channel. |

### Shared crates

`packages/recor-auth-oidc` (OIDC + JWKS verifier with HMAC-algorithm
refusal), `packages/recor-logging` (PII-redacting tracing layer),
`packages/recor-vault-client` (Vault AppRole client),
`packages/recor-spiffe` (SPIFFE Workload API + rustls),
`packages/recor-inference-gateway` (Anthropic Messages API client,
budget-tracked), `packages/fabric-bridge` (Hyperledger Fabric Gateway
client).

### Infrastructure

| Path | What |
|---|---|
| [`infrastructure/observability-dev`](infrastructure/observability-dev/) | OTel Collector → Prometheus + Tempo + Loki + Grafana stack. 4 dashboards (platform / relay / verification / auth) + alert rules. |
| [`infrastructure/kafka`](infrastructure/kafka/) | Single-broker KRaft dev cluster + topic init. D→V and V→D channels (HTTP-relay → Kafka cutover via `RELAY_TRANSPORT` env). |
| [`infrastructure/spire`](infrastructure/spire/) | SPIRE server + agent for service-to-service mTLS via SPIFFE SVIDs. |
| [`infrastructure/vault`](infrastructure/vault/) | HashiCorp Vault dev stack + AppRole roles for every service. Secret loading bridged at service startup. |
| [`infrastructure/helm`](infrastructure/helm/) · [`infrastructure/argocd`](infrastructure/argocd/) · [`infrastructure/terraform`](infrastructure/terraform/) | Production deployment. |
| [`chaincode/audit-witness`](chaincode/audit-witness/) | Hyperledger Fabric chaincode. Every declaration event is anchored on-chain by `apps/worker-fabric-bridge`. |
| [`contracts/`](contracts/) | Protobuf definitions for the gRPC surface. |

### Engineering surface

- **REST API** under `/v1/declarations` (services/declaration). OpenAPI
  3.1 spec at [`docs/openapi/declaration.json`](docs/openapi/declaration.json);
  Scalar UI at `GET /docs` when running.
- **gRPC** on a separate port — `recor.declaration.v1.DeclarationService`,
  same OIDC verifier via a tonic interceptor.
- **By-principal data-subject access** at `GET /v1/declarations/by-principal`
  (COMP-1).
- **Operator admin** under `/v1/internal/outbox-dlq` with allowlist gating.
- **`/metrics`** Prometheus exposition on every service (OBS-1).

---

## Doctrines (binding)

The 24 strict engineering doctrines in `docs/architecture/` § V1 P2
govern every contribution. Doctrine violations block merge.

The load-bearing few:
- **D14 fail-closed** — refuse on any unknown state; never 2xx on
  partial success
- **D15 cryptographic provenance** — Ed25519 attestation + BLAKE3
  receipt + Fabric anchor on every consequential event
- **D17 zero trust** — declarant principal sourced from auth (OIDC),
  never from request body
- **D18 no secrets** — `SecretString` wrappers; PII redaction layer
  (OPS-2); secrets in Vault (OPS-4)
- **D22 Anthropic-primary AI inference** — Inference Gateway crate is
  the single point of egress to Anthropic for Tier A/B reasoning

---

## Architecture decisions (ADRs)

[`docs/adr/`](docs/adr/) carries the MADR-format record of every
load-bearing design decision:

| ADR | Subject |
|---|---|
| [0001](docs/adr/0001-event-sourcing-declaration-aggregate.md) | Event sourcing for the Declaration aggregate |
| [0002](docs/adr/0002-dempster-shafer-fusion.md) | Dempster-Shafer over Bayesian for verification fusion |
| [0003](docs/adr/0003-http-outbox-relay-d-v.md) | HTTP outbox-relay for D↔V (Kafka follow-up) |
| [0004](docs/adr/0004-oidc-jwks-principal-authentication.md) | OIDC + JWKS principal authentication |
| [0005](docs/adr/0005-hmac-channel-rotation.md) | Per-channel HMAC secrets + dual-secret rotation |
| [0006](docs/adr/0006-observability-stack-choice.md) | OTel + Prometheus + Tempo + Loki + Grafana |
| [0007](docs/adr/0007-kafka-transport-cutover.md) | Kafka transport alongside HTTP relay |
| [0008](docs/adr/0008-spiffe-mtls.md) | SPIFFE/mTLS for service-to-service auth |
| [0009](docs/adr/0009-fabric-audit-anchoring.md) | Hyperledger Fabric for receipt anchoring |

---

## Compliance

`docs/compliance/` carries the binding compliance documents.
Every cited legal instrument carries a `[CITATION NEEDED: <slug>]`
marker pending AML/CFT counsel sign-off:

- [`gdpr-procedures.md`](docs/compliance/gdpr-procedures.md) — six GDPR
  data-subject rights mapped to platform endpoints + OHADA AML/CFT
  carve-outs (COMP-1)
- [`data-retention.md`](docs/compliance/data-retention.md) — per-table
  retention policy + the trigger-enforced append-only event log (COMP-2)
- [`data-classification.md`](docs/compliance/data-classification.md) —
  every column classified Public/Internal/Confidential/PII/Sensitive-PII
  (COMP-3)
- [`regulatory-mapping.md`](docs/compliance/regulatory-mapping.md) —
  endpoint + invariant → legal-provision map under Cameroon law,
  OHADA, FATF Rec 24, GDPR (COMP-4)
- [`dr-drill-template.md`](docs/compliance/dr-drill-template.md) —
  quarterly disaster-recovery drill record template (COMP-5)

---

## Security

- [`docs/security/threat-model.md`](docs/security/threat-model.md) —
  STRIDE per component (portal, declaration, V-engine, D↔V loop, auth,
  database, operator surface) with current mitigations and explicit
  accepted-risk rows (DOC-4)
- [`docs/security/pen-test-prep.md`](docs/security/pen-test-prep.md) +
  [`pen-test-rules-of-engagement.md`](docs/security/pen-test-rules-of-engagement.md)
  — engagement package for the vendor-led external pen test (PEN-1)
- [`docs/security/branch-protection.md`](docs/security/branch-protection.md)
  — declarative spec for `main`-branch rules + the apply script (CI-3)
- Vulnerability disclosure: `SECURITY.md` or
  `https://recor.cm/.well-known/security.txt`

---

## Runbooks

[`docs/runbooks/`](docs/runbooks/) covers operator-facing procedures:
on-call triage tree, incident-response template, deploy / rollback,
restore-from-backup, DLQ inundation, HMAC secret rotation, OIDC
issuer outage, BUNEC adapter outage, SPIFFE onboarding, Vault
onboarding, audit verification, soft-launch playbook, observability
dashboards, supply chain, sqlx cache regeneration, image verification.

---

## Authoritative source documents

Three governance documents bind this codebase:

1. **Concept Note** — strategic rationale; funder / political audiences
   → `docs/concept-note/RECOR-Concept-Note.docx`
2. **Software Architecture Document** — what the system is, how it is
   engineered; ~200 pages → `docs/architecture/RECOR-Software-Architecture-Document.docx`
3. **Implementation Companion** — paste-and-go artefacts
   → `docs/companion/RECOR-Implementation-Companion.docx`

When the Architecture and Companion conflict, **the Architecture wins**.

---

## Local development

```bash
# Toolchain
rustup show           # rust 1.88, edition 2024
pnpm --version        # 9.12.3 (for the portal)
docker --version      # for Postgres / Kafka / SPIRE / Vault dev stacks

# Build the workspace
SQLX_OFFLINE=true cargo build --workspace --release

# Run the test suite
SQLX_OFFLINE=true cargo test --workspace --lib
cd applications/declarant-portal && pnpm test

# Bring up the D↔V loop locally
docker compose -f services/declaration/docker-compose.integration.yaml up
bash services/declaration/scripts/integration-smoke.sh

# Generate the OpenAPI spec from the running build
bash tools/ci/check-openapi-drift.sh
```

The committed `services/<svc>/.sqlx/` offline cache means every build
is hermetic. See [`docs/runbooks/sqlx-cache-regeneration.md`](docs/runbooks/sqlx-cache-regeneration.md)
when you add or change a query.

---

## CI / supply chain

| Workflow | Purpose |
|---|---|
| `required-checks.yaml` | Lints (yaml/shell/markdown), secrets scanning (gitleaks + detect-secrets), governance (CODEOWNERS, pr-hygiene, no-dangling), `db / sqlx-cache-check`, `api / openapi-drift`, `portal / openapi-client-drift` |
| `publish-images.yaml` | On every push to `main`, builds + Trivy-scans (HIGH/CRITICAL exit-on-find) + SBOMs (SPDX + CycloneDX) + cosign-signs the three images at `ghcr.io/water-hacker/recor-{declaration,verification-engine,portal}` |
| `observability-smoke.yaml` | OTel pipeline end-to-end smoke against the dev observability compose |
| `portal-e2e.yaml` | Playwright E2E suite (mocked + live D↔V stack modes) |
| `dr-drill-smoke.yaml` | Disaster-recovery drill nightly + on-touch |
| `pr-hygiene.yaml` | PR template completeness, size budget, conventional-commit title, blocked-path / secrets paths |

Branch protection on `main`: linear history, required status checks
match the names in `required-checks.yaml` + `pr-hygiene.yaml`. Force
pushes refused. See `tools/ci/apply-branch-protection.sh` for the
declarative spec.

---

## Claude Code

This repository is built primarily through Claude Code agents on
Opus 4.7. Specialist agents in [`.claude/agents/`](.claude/agents/):

- **rust-service-engineer** · Rust service implementation
- **typescript-frontend-engineer** · Portal + React work
- **infrastructure-engineer** · Docker, K8s, Helm, CI/CD, observability
- **security-engineer** · TLS, secrets, PII redaction, threat-model implementation
- **integration-specialist** · External adapters (BUNEC, sanctions, PEP, ICIJ, Anthropic)
- **verification-engine-specialist** · Pipeline + fusion math
- **migration-specialist** · Database migrations + property tests
- **architect-reviewer** · Architecture compliance (read-only)
- **security-reviewer** · STRIDE / OWASP / CWE reviews (read-only)
- **test-author** · Tests across all layers
- **docs-author** · ADRs, runbooks, threat-model docs
- **refactor-specialist** · Scoped refactors
- **incident-investigator** · Production incident investigation
- **lead-orchestrator** · Top-level coordination

Plan Mode is the default for any substantive change. The 11 skills
in [`.claude/skills/`](.claude/skills/) auto-discover from context.

---

## Contributing

See [`CONTRIBUTING.md`](CONTRIBUTING.md). RÉCOR is sovereign
infrastructure; external contributions are accepted only through the
consortium's documented contribution process.

---

## Production roadmap

[`docs/PRODUCTION-TODO.md`](docs/PRODUCTION-TODO.md) is the canonical
roadmap. Phase 0 (foundational engineering) is complete; the platform
has the SPIFFE/mTLS + Kafka + Fabric + Vault + verification-engine
stages all wired (mostly skeletons gated on partner agreements per the
ticket briefs). Phases 1–5 cover external integrations, infrastructure
migration, portal completeness, compliance sign-offs, and pre-launch
hardening.

---

## Licence

The source code in this repository is the property of the RÉCOR
Consortium. Portions distributed under Apache-2.0 are marked
accordingly; the default is Restricted distribution under consortium
licence terms.
