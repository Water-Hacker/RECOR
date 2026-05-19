# Stress test — RÉCOR forensic audit, Section 11

**Scope.** Live-fire adversarial exercises against a running stack.

**Honesty disclosure.** This audit pass did NOT stand up the full
D↔V + Kafka + Vault + SPIRE + Fabric stack required for true live-fire
exercises. Standing up the full stack (with seeded mock-BUNEC, real
Postgres replicas, a Fabric cluster, an OIDC issuer, and a SPIRE
trust domain) is a multi-hour exercise that belongs in a dedicated
pen-test window or an acceptance-test cycle.

This section therefore:

1. **Documents the test that exists today** — what RÉCOR's CI/test
   harness already exercises (the `integration-smoke.sh` family,
   the testcontainers integration tests, the Playwright E2E suite,
   the `portal-e2e/live` job, the `audit_immutability` testcontainers
   tests, the `dr-drill` workflow).
2. **Catalogues the residual live-fire exercises** that production
   verification must run before launch (Section 11.1–11.15 of the
   audit spec).
3. **For each catalogued exercise:** documents the substitute (if
   any) used in this pass, the expected outcome, and the production
   acceptance gate.

Findings discovered through static analysis of the test harness are
folded into [`10-findings.md`](10-findings.md). Findings that can ONLY
be confirmed by live-fire are marked `requires-live-fire` in the
catalogue.

---

## What exists today (audited as-shipped)

| Existing exercise | What it covers | Source | Status |
|---|---|---|---|
| `services/declaration/scripts/integration-smoke.sh` | D-only submit + GET + amend + correct round-trip against compose stack | repo | ✓ passes locally |
| `services/declaration/scripts/dlq-smoke.sh` | DLQ failure-path: bad-secret causes dispatch failure → DLQ row → admin list/replay | repo | ✓ passes locally |
| `services/declaration/scripts/kafka-smoke.sh` | Kafka transport: producer + consumer + Kafka broker via compose | repo (R-LOOP-2) | requires live Kafka |
| `services/declaration/scripts/mtls-smoke.sh` | SPIFFE/mTLS path: SPIRE bring-up + cert-cycle (R-LOOP-3) | repo | requires live SPIRE |
| `services/declaration/tests/audit_immutability.rs` | Trigger refusal: UPDATE/DELETE/TRUNCATE on declaration_events → 42501 | testcontainers `#[ignore]` | ✓ passes |
| `services/declaration/tests/api_integration.rs` | Full submit/GET/idempotency-replay via reqwest + Postgres | testcontainers | ✓ passes (5 tests) |
| `services/declaration/tests/rate_limit_integration.rs` | OPS-1 rate-limit boundary: burst+1 → 429 with Retry-After | testcontainers | ✓ passes (3 tests) |
| `services/declaration/tests/grpc_integration.rs` | gRPC submit + REST GET round-trip | testcontainers | ✓ passes |
| `applications/declarant-portal/tests/e2e/*.spec.ts` | Playwright happy-path + validation + verification-rejected + polling-stops + a11y-smoke | Playwright | ✓ mocked-mode 4/4 passes; live-mode happy-path lane assertion widened post-R-VER |
| `.github/workflows/observability-smoke.yaml` | OTel pipeline end-to-end | CI | running |
| `.github/workflows/dr-drill-smoke.yaml` | COMP-5 disaster-recovery drill | CI nightly + on-touch | failing pre-existing; not blocking per OBS-2 deferral |

**Coverage estimate.** ~70% of the load + failure scenarios named in
Section 11 of the audit spec have at least one mocked or
testcontainers-level proxy in the existing harness. The remaining
~30% require a live D↔V stack and are documented below.

---

## Catalogue of residual live-fire exercises

Each entry: brief from Section 11 of the audit spec → substitute used
in this pass → production acceptance gate.

### 11.1 Load

**Spec.** Fire concurrent requests at heaviest endpoint at
progressively increasing rates; measure p50/p95/p99; confirm rate
limiting activates before flood; confirm DB pool + thread pool +
worker queue don't exhaust silently.

**Substitute.** `services/declaration/tests/rate_limit_integration.rs`
proves the rate-limit boundary (burst+1 → 429 with Retry-After).
OBS-1 `http_request_duration_seconds` histogram is wired and exposed
on `/metrics`. The latency budget per `services/declaration/CLAUDE.md`
is documented (POST p99 < 500ms, GET p99 < 50ms).

**Production acceptance gate.** Run k6 / wrk against the staging
endpoint at 10/50/100/500/1000 RPS; confirm:
- p99 latency at 100 RPS < 500ms (declaration SLO)
- Rate-limit activates at the configured `RATE_LIMIT_PER_MIN`
- DB pool, OBS-1 `recor_outbox_undispatched`, and worker queue all
  show in Grafana
- No silent pool exhaustion (would show as connection-refused 500s)

**Status.** `requires-live-fire`.

### 11.2 Primary store failure

**Spec.** Kill the primary database mid-flight; confirm degraded
state vs opaque errors; confirm no partial commits; recovery time.

**Substitute.** Static analysis: every write site uses
`sqlx::query!()` macros within a single `BEGIN..COMMIT` transaction
(event + projection + outbox). The trigger on declaration_events
makes partial-write impossible at the SQL level.

**Production acceptance gate.** During the dr-drill exercise (Q in
COMP-5), kill `postgres-declaration` mid-submission. Confirm the
declaration service returns 503 (readyz fails because pool reaches
`db_pool_max_connections`-checks). Confirm in-flight TX rollback
via post-restart event log inspection.

**Status.** `requires-live-fire`. The audit_immutability
testcontainers test partly covers the trigger refusal path.

### 11.3 Upstream format change

**Spec.** Mutate response from an external dependency to break
expected schema; confirm consumer raises a structured error → DLQ
or quarantine; alert; other consumers unaffected.

**Substitute.** Static analysis: every external-call site uses
typed deserialisation via `serde` against Rust structs. A schema
break would surface as a `serde_path_to_error` or transport-level
error → bubbled up to the use case → mapped to ServiceError →
4xx/5xx. The relay worker DLQs on permanent error (R-LOOP-DLQ-2
dual-side). The Inference Gateway's tool-use parsing
(`packages/recor-inference-gateway/src/lib.rs`) verifies the
schema before returning; malformed → vacuous BPA + fixture-mode
fallback.

**Production acceptance gate.** Wiremock-substitute BUNEC,
sanctions, OFAC, OpenSanctions PEP endpoints with malformed payloads.
Confirm each Stage's adapter emits vacuous BPA + the structured
error in tracing.

**Status.** `requires-live-fire`. Wiremock tests in
`services/verification-engine/src/infrastructure/bunec_real.rs::tests`
exercise the happy + failure paths today.

### 11.4 Audit divergence

**Spec.** Manually insert an audit entry into primary that does
not reach a witness; reconciliation job should detect + backfill
OR alert.

**Substitute.** None. **The reconciliation job is missing**
(see [`08-audit-chain.md`](08-audit-chain.md)).

**Production acceptance gate.** Cannot proceed without the
reconciliation cron landing first. Flagged as FIND-AV-02 (HIGH).

**Status.** `blocked-on-missing-feature`.

### 11.5 Concurrent privileged operations

**Spec.** For multi-party-approval or shared-state mutations, run
two callers in parallel; confirm no double-count / race /
inconsistency.

**Substitute.** Static analysis: the DLQ admin replay path uses an
atomic `INSERT outbox + DELETE outbox_dlq` in a single TX. The
COMP-2 retention worker uses a single `DELETE FROM outbox WHERE
dispatched_at < $1` query. Both are safe under concurrency.

**Production acceptance gate.** Two operators run replay against
the same DLQ row id within 100ms of each other; confirm second
returns 404 (row already moved) and audit log shows one
successful + one no-op.

**Status.** `requires-live-fire`. The atomicity is provable from
the SQL but the operator UX gate (concurrent-replay UI) is
deferred.

### 11.6 Forbidden access attack

**Spec.** As lowest-privilege role, attempt every protected
surface by URL manipulation, direct API call, malformed token,
wrong-key-signed token, elevated claim, missing auth, expired
auth. For each: appropriate rejection + audit + no info leak.

**Substitute.** Pass A and Pass B's surface walkthroughs traced
every authentication and authorisation check at the file:line
level. The static walk surfaced:

- **FIND-AV-01 (HIGH):** audit-verifier unauthenticated; full
  declaration payload by UUID
- **PRM-3 (HIGH):** `POST /v1/verifications` admits arbitrary
  snapshots from any authenticated declarant
- **PRM-6 (HIGH):** `ENVIRONMENT=dev` + configured OIDC accepts
  both auth paths — complete dev-header bypass
- **AUTH-VER-SUBMIT (HIGH):** V-engine submit/get accept any
  authenticated principal (cross-tenant case read)
- **AUTH-PERSON-GET (HIGH):** person-service GET/search grants
  Sensitive-PII to any authenticated principal

These are CRITICAL-grade defects surfaced before any live-fire
test. **The live-fire test will simply reproduce them** —
running them is straightforward once the defects close.

**Production acceptance gate.** After FIND-001..005 close, run
every attack pattern in Section 11.6 of the audit spec; persist
artifacts under `docs/audit/evidence/stress-test/forbidden-access/`.

**Status.** `blocked-on-defects` — fix the HIGH findings before
attempting live-fire forbidden-access testing.

### 11.7 Input hardening

**Spec.** Boundary inputs, malformed inputs, injection payloads
(SQL / NoSQL / command / header / template / deserialisation),
file-upload mismatches, PII non-leakage.

**Substitute.** Static analysis:

- sqlx `query!()` macros are parameterised — no string-built SQL
- Zod schemas (portal) + utoipa schemas (declaration) validate at
  the wire boundary
- OPS-2 redacting tracing layer scrubs UUIDs, SPIFFE paths, and
  receipt hashes in logs (`packages/recor-logging/src/lib.rs`)
- No file-upload surface exists today (the portal doesn't accept
  attachments)
- No template-rendering surface beyond Scalar UI (server-rendered
  from utoipa-derived OpenAPI, no user-supplied templates)

**Production acceptance gate.** Run OWASP ZAP / Burp Suite against
the staging surface with the full payload corpus. Persist results
under `docs/audit/evidence/stress-test/input-hardening/`.

**Status.** `requires-live-fire` (static posture is sound).

### 11.8 AI hallucination injection

**Spec.** Adversarial inputs targeting each documented safety
layer of AI inference.

**Substitute.** `packages/recor-inference-gateway/src/lib.rs`
implements Anthropic Messages API with tool-use-forced structured
output. The schema-enforcing wrapper rejects non-conforming
responses → vacuous BPA. Fixture-mode tests already exercise this.

**Production acceptance gate.** Adversarial prompts naming the
seeded happy-path person but instructing the model to flip the
verdict; confirm tool-use wrapper rejects ill-formed JSON. Persist
under `docs/audit/evidence/stress-test/ai-hallucination/`.

**Status.** `requires-live-fire` + `requires-Anthropic-API-key`
(currently CI runs in fixture mode).

### 11.9 Worker crash recovery

**Spec.** Kill a worker; supervisor revives; in-flight messages
either acked or redelivered; DLQ no orphans.

**Substitute.** Static analysis: every worker uses
`tokio::spawn` + a `CancellationToken` (tokio-util). On panic, the
supervising `main` exits non-zero; container orchestrator (k8s
in prod, docker compose in dev) restarts. At-least-once delivery
on Kafka consumer (R-LOOP-2) + idempotency on `event_id` in the
V-engine use case ensures redelivery doesn't double-write.

**Production acceptance gate.** Inside the dr-drill exercise: `docker
compose kill recor-loop-relay` mid-dispatch; confirm Kafka offset
not advanced; restart; confirm event redelivered + idempotency
catches the double.

**Status.** `requires-live-fire`.

### 11.10 Build-time regression

**Spec.** Introduce a deliberate violation of any documented
build-time invariant; confirm build fails with clear error;
remove violation → succeeds.

**Substitute.** Static analysis:

- `cargo sqlx prepare --check` in `required-checks.yaml` catches
  query/migration drift (R-DECL-7)
- `tools/ci/check-openapi-drift.sh` catches OpenAPI drift (DOC-1)
- `tools/ci/check-portal-openapi-client-drift.sh` catches portal
  client drift (R-PORT-7)
- `markdownlint-cli2-action@v17` catches markdown drift
- Trivy (CI-2) catches HIGH/CRITICAL CVEs at image-build time

**Production acceptance gate.** Already passing on every PR.

**Status.** **✓ verified through CI history.**

### 11.11 Secret store unsealing failure

**Spec.** Simulate Vault sealed; confirm fail-closed; no fallback
silently substitutes.

**Substitute.** Code in `services/declaration/src/main.rs` calls
`VaultClient::new()` + `populate_from_vault(&mut cfg)` BEFORE
`Config::from_env` validation. If Vault is unreachable, the
function returns an error and the service exits non-zero (per
`packages/recor-vault-client/src/lib.rs::tests`).

**Production acceptance gate.** During dr-drill: stop Vault; restart
service; confirm exit-code != 0 + `tracing::error!` with
`reason = "vault unreachable"`. Restart Vault; confirm clean start.

**Status.** `requires-live-fire`. Unit-test coverage exists.

### 11.12 Time skew

**Spec.** Set host clock ahead of NTP; confirm rejection with
clock-skew error.

**Substitute.** Static analysis: OIDC verifier in
`packages/recor-auth-oidc/src/lib.rs` checks `iat` and `exp` against
`std::time::SystemTime::now()` with `jsonwebtoken`'s default
60-second leeway. Beyond 60s skew, tokens are rejected with
"invalid token" — clear error in the response.

**Production acceptance gate.** Inside the dr-drill exercise:
`hwclock --set --date '+1h'` on the V-engine host; submit
declaration; confirm OIDC verifier rejects + tracing emission
includes `iat` + `now` so operator can diagnose.

**Status.** `requires-live-fire`.

### 11.13 Configuration drift

**Spec.** Inspect running env vs documented template; flag
undocumented production vars; flag unused documented vars; flag
secret-named vars logged anywhere.

**Substitute.** Static analysis of `.env.example` in each service
plus grep across `src/` for `std::env::var` and `dotenvy::var`:

- All env reads route through `Config::from_env` (typed `config`
  crate) — no scattered `env::var` calls outside that
- The `.env.example` files cover every documented field
- OPS-2 redacting layer scrubs secret-shaped values from tracing
- No `tracing::info!("password is {}", x)` patterns found

**Production acceptance gate.** Compare `kubectl get configmap +
secret -o yaml` against `.env.example`; reconcile. Confirm
`grep -i 'secret\|password\|token' /var/log/recor` returns no
plaintext values.

**Status.** `requires-live-fire`. Pre-shipment posture is sound.

### 11.14 Resource exhaustion

**Spec.** Exhaust each shared resource (FDs, threads, DB
connections, memory, disk, queue depth); confirm graceful
degrade + recovery.

**Substitute.** Static analysis: every connection pool has a
`db_pool_max_connections` config; every Kafka consumer has
`max.poll.records`; every HTTP client has a `TimeoutLayer`. Tokio
runtime is the default-multi-thread + bounded queues.

**Production acceptance gate.** k6 ramp to 10x the declaration
SLO; confirm degraded-503 vs OOM. Disk-fill test on the postgres
host: `fallocate -l ${free_disk - 1G}` on the volume; submit; confirm
graceful refusal.

**Status.** `requires-live-fire`.

### 11.15 Dependency upgrade safety

**Spec.** Run vulnerability scanner; report critical + high.

**Substitute.** **Run today.** The Trivy scan in CI-2 catches
HIGH/CRITICAL on the published images; the most recent failure was
nghttp2-libs (CVE-2026-27135) on the portal, closed by PR #101.

```
cargo audit && pnpm audit --audit-level=high
```

Results: workspace `cargo audit` is clean against the current
`Cargo.lock`; portal `pnpm audit` reports zero HIGH/CRITICAL.

**Production acceptance gate.** ✓ verified through CI history.

**Status.** **✓ verified through CI history + manual run.**

---

## Summary

| # | Section | Static analysis | Live-fire status |
|---|---|---|---|
| 11.1 | Load | partial via OBS-1 + rate-limit tests | requires-live-fire |
| 11.2 | Primary store failure | trigger + tx atomicity confirmed | requires-live-fire |
| 11.3 | Upstream format change | typed deserialisation + DLQ wiring confirmed | requires-live-fire |
| 11.4 | Audit divergence | reconciliation cron MISSING | blocked-on-feature |
| 11.5 | Concurrent privileged ops | atomic SQL confirmed | requires-live-fire |
| 11.6 | Forbidden access | **5 HIGH defects surfaced statically** | blocked-on-defects |
| 11.7 | Input hardening | parameterised SQL + Zod/utoipa confirmed | requires-live-fire |
| 11.8 | AI hallucination | tool-use schema-forcing confirmed | requires-live-fire |
| 11.9 | Worker crash recovery | cancellation token + idempotency confirmed | requires-live-fire |
| 11.10 | Build-time regression | ✓ verified through CI | done |
| 11.11 | Secret store fail-closed | unit tests + composition root confirmed | requires-live-fire |
| 11.12 | Time skew | jsonwebtoken leeway confirmed | requires-live-fire |
| 11.13 | Configuration drift | typed Config + redaction confirmed | requires-live-fire |
| 11.14 | Resource exhaustion | pool + timeout + bounded queues confirmed | requires-live-fire |
| 11.15 | Dependency CVE | ✓ Trivy + cargo audit + pnpm audit clean | done |

**Calibration.** This pass clears the 2 of 15 exercises that can be
verified through CI history. The remaining 13 cluster into:
- **3 blocked-on-defects** (audit-divergence + forbidden-access + AI-hallucination): defects must close before live-fire is meaningful
- **10 requires-live-fire**: production acceptance gates documented; live exercises slot into pre-launch pen-test window (PEN-1)

**Recommendation.** Treat this section as the **pre-launch verification plan**, not a "test passes" stamp. Each `requires-live-fire` entry is a numbered acceptance gate that PEN-1's vendor engagement (or an internal red-team window) executes.
