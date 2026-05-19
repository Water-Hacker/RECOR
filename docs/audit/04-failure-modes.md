# Pass B — Section 6: Failure Modes

Status: production-readiness audit, Pass B
Reviewer: Claude Code (lead orchestrator), 2026-05-13
Scope: catalogue every external dependency and every internal
component for its silent / loud / partial / total failure modes; map
each to detection, mitigation, state-after, operator notification,
and runbook.

## 6.0 How to read this document

For each entry the table columns are:

- **Failure mode** — silent / loud / partial / total + brief description
- **Detection** — metric, alert name, log line, or absent signal
- **Mitigation / recovery** — automatic behaviour + operator steps
- **State** — what is the on-disk / in-flight state after the failure
- **Notification** — who is paged / who learns about it
- **Operator action** — runbook reference
- **Tested?** — unit/integration test coverage citation
- **Severity (current posture)** — LOW / MEDIUM / HIGH / CRITICAL

Where a runbook does not exist, the entry is flagged as a gap with
a `FINDING-FM-*` identifier matching the summary at the end.

---

## 6.1 External dependencies

### 6.1.1 Postgres (per-service)

Each service has its own database (`declaration_db`,
`verification_engine_db`, `person_service_db`, `entity_service_db`,
`worker_fabric_bridge_db`, `audit_verifier_db`). Connection pools
are sized via `DB_POOL_MAX_CONNECTIONS`
(`services/declaration/src/config.rs:18-20`; default 10).

| Failure mode | Detection | Mitigation | State | Notification | Tested? | Severity |
|---|---|---|---|---|---|---|
| **Total outage** (DB down) | `/readyz` 503; `RecorHttp5xxRateHigh` alert | k8s LB drains the pod | no partial writes — every state-changing operation runs in a single tx (`services/declaration/src/infrastructure/postgres.rs:87-148`) | page (k8s probe); operator → `docs/runbooks/restore-database-from-backup.md` | tested in `services/*/tests/` (db-down integration); ✓ | HIGH |
| **Pool exhausted** | sqlx pool-timeout errors → 500; latency p99 climbs | per-request axum timeout (`HTTP_TIMEOUT_SECONDS=10`) caps the wait; tracker threat-model row "pool exhaustion under load" (`docs/security/threat-model.md:160`) | requests time out and 500; pool refills | warn — `RecorHttp5xxRateHigh` if sustained | partial — load test exists for declaration but not chained | MEDIUM |
| **Replica lag** (read-replica behind) | gap between expected and observed projection state | currently NO read-replica is used — all reads go to primary | no inconsistency | n/a | n/a — feature not in v1 | LOW (not yet applicable) |
| **Disk full** | sqlx insert returns `disk full`; rises as 500 | k8s readiness probe should flip when writes fail | last successful tx persisted; subsequent rolled back | page via 5xx alert | runbook gap — `docs/runbooks/restore-database-from-backup.md` does not cover disk-pressure recovery | FINDING-FM-1 | MEDIUM |
| **Connection failure mid-tx** | sqlx error; tx aborted | transaction rolled back; idempotency on retry | safe | log warn | covered by sqlx semantics | LOW |
| **Migration partial-apply** | startup migration crash partway | sqlx-migrate uses single-tx where DDL allows; otherwise the migration is a checkpoint and operator must complete | DB possibly in transitional schema | startup logs; pod restarts; alerts fire on 5xx | runbook gap — no migration-rollback runbook (see FINDING-FM-2) | FINDING-FM-2 | HIGH |

### 6.1.2 Kafka (R-LOOP-2; not yet default transport)

Optional v1, default for v2. Enabled by `RELAY_TRANSPORT=kafka` and
non-empty `KAFKA_BROKERS`
(`services/declaration/src/config.rs:191-216`).

| Failure mode | Detection | Mitigation | State | Notification | Tested? | Severity |
|---|---|---|---|---|---|---|
| **Broker down (all)** | producer error in `KafkaOutboxProducer`; consumer-lag gauge climbs | outbox rows stay undispatched (D-side); messages sit in topic (V-side) | safe — at-least-once semantics preserved | metric: `recor_outbox_undispatched`; consumer-lag alert TODO | partial — Kafka path has integration test, no broker-outage chaos test | MEDIUM |
| **Partition reassignment** (broker restart) | brief consumer-rebalance stall | consumer-group resumes from committed offset | safe | none | covered by Kafka semantics | LOW |
| **Consumer lag** (consumer slow) | `kafka_consumer_group_lag` metric | scale consumer replicas | broker retains messages until retention | warn — alert TODO | not tested | LOW |
| **DLQ topic fills** | DLQ topic size growing | replay tooling absent (see DF-4) | data preserved | not metricised | not tested | MEDIUM — FINDING-DF-4 (Section 5) |

### 6.1.3 HMAC channel (HTTP fallback transport)

Two channels: D→V (`/v1/internal/declaration-events`) and V→D
(`/v1/internal/verification-outcomes`). Both rotation-aware
(`services/declaration/src/api/internal.rs:281-297`).

| Failure mode | Detection | Mitigation | State | Notification | Tested? | Severity |
|---|---|---|---|---|---|---|
| **Verifier down (receiver)** | relay logs `transport error`; row `dispatch_attempts++` | relay retries up to `max_attempts=12` (1 min); then DLQ | safe — DLQ is durable | `RecorRelayLatencyHigh` + `RecorDlqOversized` (`infrastructure/observability-dev/alert-rules.yaml:29-65`) | ✓ unit + integration | MEDIUM |
| **Secret-rotation race** (signer-side new secret, verifier still old-only) | 401 from receiver | rotation procedure requires verifier to accept BOTH before signer switches (`docs/runbooks/hmac-secret-rotation.md`) | safe if procedure followed | log warn; `recor_relay_delivery_failures_total{kind=auth}` | ✓ unit-tested rotation accept-both (`services/declaration/src/api/internal.rs:329-355`) | LOW |
| **DLQ accumulates** | `recor_outbox_dlq_size > 100` for 10m → `RecorDlqOversized` (page) | operator replay via `/v1/internal/outbox-dlq/{id}/replay` after fixing root cause | rows safe in DLQ | page; `docs/runbooks/dlq-inundation.md` | ✓ integration | HIGH (page severity by design) |
| **Replay window unbounded** (Gap G2) | no detection — by definition silent | mitigation = secret rotation cadence (30d) | replay window = secret lifetime | none | not testable without iat | HIGH — FINDING-DF-2 (Section 5) |
| **Signer encrypts payload but verifier mis-parses** | 400 from receiver | bug — would surface on every dispatch | rows DLQ after retries | log error; `RecorDlqOversized` | covered by integration smoke | LOW |
| **HMAC secret leak via logs** | OPS-2 RedactingLayer scrubs SecretString; `SecretString` type prevents accidental Display | inspect log corpus | secret type prevents accidental println! | none if redaction holds | static analysis (`SecretString` from `secrecy` crate) | LOW |

### 6.1.4 OIDC issuer + JWKS

`packages/recor-auth-oidc` is the cache + verifier. Issuer URL via
`OIDC_ISSUER_URL`. Discovery + JWKS endpoints both subject to outage.

| Failure mode | Detection | Mitigation | State | Notification | Tested? | Severity |
|---|---|---|---|---|---|---|
| **Issuer total outage** | `recor_oidc_verify_total{result=unavailable}` > 0.5/s for 5m → `RecorOidcVerifierDown` page | JWKS cache covers brief blips; once expired, every protected REST + gRPC fails closed (401) | safe — fail-closed (D14) | page; `docs/runbooks/oidc-issuer-outage.md` | ✓ alert wired | HIGH (page) |
| **Malformed token** (client bug) | `recor_oidc_verify_total{result=invalid}` | 401; bounded counter | safe | warn — alert on sustained spike (TODO) | ✓ unit + integration | LOW |
| **Clock skew** (NBF / EXP boundary) | sporadic 401s with `TokenExpired` / `TokenNotYetValid` in trace | jsonwebtoken's default leeway (60s); operator can tune `OIDC_TOKEN_LEEWAY_SECONDS` (TODO if not present) | safe | log warn | partial — depends on default leeway accepted | LOW |
| **JWKS rotation race** | old kid used after issuer rotates | verifier refetches JWKS on `UnknownKid`; cache invalidated | safe — handled by `packages/recor-auth-oidc` | log info on refetch | covered by unit tests | LOW |
| **Discovery endpoint returns wrong issuer URL** | bug; 401 with `IssuerMismatch` | 401 with metric label `invalid` | safe | warn | covered by unit | LOW |

### 6.1.5 Vault (OPS-4)

`packages/recor-vault-client` fetches secrets at startup. `VAULT_ADDR`
empty → env-only fallback with a startup `warn!`.

| Failure mode | Detection | Mitigation | State | Notification | Tested? | Severity |
|---|---|---|---|---|---|---|
| **Vault unreachable at startup** | service refuses to start (D14) | k8s probe fails; pod CrashLoopBackoff | safe — no partial config | page; `docs/runbooks/vault-onboarding.md` | ✓ unit + integration | HIGH |
| **Vault sealed** | login returns 503 sealed | service refuses to start | safe | page; manual unseal procedure | ✓ unit | HIGH |
| **AppRole token expired** | only at startup matters (secrets fetched once) | service refuses to start | safe | page | partial | MEDIUM |
| **Secret rotated mid-flight** | Vault rotation invalidates an old secret value | service holds the value it bootstrapped with; HMAC accepts old+new during rotation window | safe via rotation procedure (`hmac-secret-rotation.md`) | follow rotation runbook | ✓ via HMAC rotation tests | LOW |
| **Vault audit log unreachable** | secondary surface; service continues | log warn | safe; compliance gap if sustained | warn | not tested | LOW |

### 6.1.6 Hyperledger Fabric (R-DECL-9)

The audit anchoring channel. `worker-fabric-bridge` → HTTP Gateway
shim → Fabric peer → ordering service → audit channel.

| Failure mode | Detection | Mitigation | State | Notification | Tested? | Severity |
|---|---|---|---|---|---|---|
| **Peer down** | shim returns gateway error | bridge retries with backoff (max_attempts default 5; `docs/runbooks/fabric-bridge.md`) | row in worker's in-flight queue; if all retries fail → `fabric_bridge_dlq` | warn; alert via `recor_fabric_bridge_dlq_size` if metric exists | partial — chaos test absent | MEDIUM |
| **Write rejection** (chaincode refuses) | application error from gateway | bridge maps to `BridgeError::AppRejected`; row → DLQ immediately (not retried) | safe — DLQ preserves the rejected envelope | warn; manual investigation | covered by unit (`packages/fabric-bridge/src/lib.rs` tests) | MEDIUM |
| **Ordering partition** | TX never commits; shim returns commit-status pending | bridge waits or times out; treated as retryable | safe — eventually retries land | escalate to chain consortium (`docs/runbooks/fabric-bridge.md`) | not chaos-tested | HIGH — listed as R-DECL-9 risk |
| **Chaincode bytecode hash mismatch** | startup-time peer rejects install | catastrophic — chain ops issue, not bridge issue | safe (no commits during) | page chain consortium | not in scope for worker | HIGH (operational, not v1 code surface) |
| **Already-anchored (replay)** | chaincode `AlreadyExists` | bridge maps to `CommitOutcome::AlreadyCommitted` — treated as Ok (D13) | idempotent | none | ✓ unit-tested (`packages/fabric-bridge/src/lib.rs` doctring lines 30-50) | LOW |
| **Audit log divergence** (projection edited out-of-band) | `audit-verifier` reports mismatch | manual investigation; `docs/runbooks/audit-verification.md` | safe — chain is source of truth | manual; no automated alert | partial | MEDIUM — FINDING-FM-3 (no automated audit-divergence alert) |

### 6.1.7 Anthropic API (Inference Gateway)

Stage 5 calls. `packages/recor-inference-gateway` is the only caller.
Has tiered budgets (Tier A/B/C) and fixture-mode fallback.

| Failure mode | Detection | Mitigation | State | Notification | Tested? | Severity |
|---|---|---|---|---|---|---|
| **Rate limit (429)** | gateway returns `GatewayError::RateLimited` | stage maps to `insufficient_evidence` verdict; vacuous BPA | declaration verification recorded; lane likely Yellow | metric: `recor_inference_gateway_calls_total{result=rate_limited}`; no dedicated alert yet | partial | MEDIUM — FINDING-DF-3 (no runbook) |
| **5xx from Anthropic** | gateway returns `GatewayError::Upstream` | same as above — fail-soft to insufficient_evidence | recorded | metric | partial | MEDIUM |
| **Malformed response** (schema fail) | gateway returns `GatewayError::InvalidResponse` | stage maps to insufficient_evidence | recorded | metric | ✓ unit-tested | LOW |
| **Empty response** (model returned nothing) | gateway returns `GatewayError::EmptyResponse` | same | recorded | metric | ✓ unit | LOW |
| **Hallucination (valid schema, wrong verdict)** | undetectable inline | persisted with `evidence_citations`; post-hoc human review; lane router treats all verdicts cautiously | recorded with full prompt-hash for audit | none inline; audit only | partial | MEDIUM (model-risk; treated as accepted-risk for now) |
| **Budget exhausted** (Tier A monthly cap) | gateway refuses call | stage emits insufficient_evidence | recorded | metric: budget exhaustion | ✓ unit | LOW |
| **API key missing** (config error) | gateway in fixture mode; returns deterministic `insufficient_evidence` for every call | warning at startup; lane always Yellow on adverse-media | safe — no PII leakage; lane-router behaves predictably | startup warn; metric | ✓ unit | MEDIUM — would silently down-grade quality if prod |

### 6.1.8 BUNEC adapter (R-VER-1; mock in v1)

Stage 2 of verification. Currently `PostgresMockBunec`; real adapter
is delivered by R-VER-1 (`docs/runbooks/bunec-adapter-outage.md`
documents the future state).

| Failure mode | Detection | Mitigation | State | Notification | Tested? | Severity |
|---|---|---|---|---|---|---|
| **Real BUNEC down (future)** | circuit breaker opens; `recor_bunec_calls_total{outcome=error}` rate | `BUNEC_FAIL_POLICY=fail_open | fail_closed` lever (interface defined; not wired in v1) | depends on policy | runbook exists for future state | partial — runbook ahead of implementation | MEDIUM-future |
| **Mock BUNEC down** (v1) | stage 2 stage error; lane Red | Postgres-style outage handling | safe | same as Postgres outage | partial | MEDIUM |
| **Mock fixture drift** | mock data does not match real-world expectations | accepted-risk in v1 (mock); will move to fixture-tests after R-VER-1 | safe | none | partial | LOW |

### 6.1.9 SPIFFE Workload API (R-LOOP-3)

`packages/recor-spiffe` fetches SVIDs at startup. Used only when
`AUTH_TRANSPORT` is `mtls` or `mtls-only`.

| Failure mode | Detection | Mitigation | State | Notification | Tested? | Severity |
|---|---|---|---|---|---|---|
| **SVID fetch failure at startup** | service refuses to start (D14) — `services/verification-engine/src/main.rs:163-165` "refusing to start under AUTH_TRANSPORT=mtls (D14 fail-closed)" | pod CrashLoopBackoff | safe | page; `docs/runbooks/spiffe-onboarding.md` | ✓ integration | HIGH |
| **Trust bundle stale** | TLS handshake fails for peers presenting newer SVIDs | metric `recor_spiffe_trust_bundle_age_seconds` | safe — fail-closed at TLS layer | warn — alert TODO (no rule today) | not yet alerted | MEDIUM — FINDING-FM-4 (no alert on stale trust bundle) |
| **SPIRE agent down mid-flight** | new SVIDs cannot be fetched; old SVID is used until expiry | service continues with held SVID until rotation deadline; after rotation deadline, peers' mTLS will fail | safe until rotation | warn → page if rotation deadline crossed | not chaos-tested | MEDIUM |
| **`AUTH_TRANSPORT` set but socket path wrong** | startup error | service refuses to start | safe | startup log | covered by config validation (`services/declaration/src/config.rs:294-300`) | LOW |

### 6.1.10 OTel collector / Prometheus / Grafana

`OTLP_ENDPOINT` empty disables OTLP export and keeps tracing
console-only (`services/declaration/src/config.rs:26-29`).

| Failure mode | Detection | Mitigation | State | Notification | Tested? | Severity |
|---|---|---|---|---|---|---|
| **OTel collector down** | tracing exporter logs "OTLP send failed" | tracing continues to local stdout; spans buffered then dropped on full queue | safe — no app impact | log warn; meta-alert on collector availability | partial | LOW |
| **Prometheus down (scrape side)** | `up{service="recor-declaration"}` goes to 0 | service continues serving `/metrics`; data points lost during outage | safe — `/metrics` is read-only | meta-alert | partial | LOW |
| **Grafana down** | operators can't see dashboards | direct Prometheus query as fallback; `docs/runbooks/observability-dashboards.md` | safe — alerting is via Prometheus, not Grafana | warn | not in scope | LOW |
| **Alert manager down** | alerts not delivered | secondary monitoring required (paging via cluster monitoring) | safe — no app impact | external monitoring | not tested | MEDIUM — FINDING-FM-5 (no documented bypass paging path) |

### 6.1.11 Mock BUNEC / sanctions / PEP ingestion — schema drift

| Failure mode | Detection | Mitigation | State | Notification | Tested? | Severity |
|---|---|---|---|---|---|---|
| **OFAC XML schema drift** (column renamed upstream) | ingestion script error OR — silently — incorrect rows | currently NO loader binary (FINDING-DF-6); DBA-run procedure | safe (no row replacement) but data stale | none if DBA does not check | manual | MEDIUM — FINDING-DF-6 |
| **UN list cadence drift** (list not refreshed) | manual; no freshness check | accepted risk — UN list relatively stable | safe; recall may miss a fresh listee | none | none | MEDIUM |
| **PEP list source-of-truth ambiguity** | manual review | DBA procedure | safe | none | none | LOW |

---

## 6.2 Internal components

### 6.2.1 Declaration service — worker crash / memory leak

| Failure mode | Detection | Mitigation | State | Notification | Tested? | Severity |
|---|---|---|---|---|---|---|
| **Pod OOM** | k8s OOMKilled event | k8s restart policy; outbox + idempotency are durable so retries land safely | safe | metric: pod restart rate | partial | MEDIUM |
| **Tokio runtime panic** | logs `panicked at ...` | tokio swallows panics in spawned tasks; outer task still alive (potentially leaking) | depends — must be tested per task | log error + metric | partial | MEDIUM — FINDING-FM-6 (need panic-handler shim that escalates) |
| **Memory leak** (Arc cycle / connection accumulation) | RSS grows over time | k8s memory-limit + restart on cross | safe via restart | warn → page if leak fast | partial — no continuous canary load | MEDIUM |

### 6.2.2 Verification-engine — single stage failing

The pipeline orchestrator is **fail-safe by design**: a stage that
errors emits `InsufficientEvidence` (vacuous BPA) and the pipeline
continues. A stage that hits a fatal error emits
`ShortCircuitFailClosed`, which forces lane = Red
(`services/verification-engine/src/application/orchestrator.rs:60-76`).

| Stage | Failure mode | Result | Severity |
|---|---|---|---|
| Stage 1 Schema | malformed snapshot | ShortCircuitFailClosed → lane Red | LOW |
| Stage 2 Identity | BUNEC down | InsufficientEvidence; lane likely Yellow | MEDIUM |
| Stage 3 Sanctions | sanctions adapter down | InsufficientEvidence | MEDIUM |
| Stage 4 PEP | PEP adapter down | InsufficientEvidence | MEDIUM |
| Stage 5 Adverse media | Anthropic down | InsufficientEvidence | MEDIUM |
| Stage 6 Patterns | pattern miner down | InsufficientEvidence | LOW |
| Stage 7 Cross-source | cross-checker down | InsufficientEvidence | LOW |
| Stages 8-9 (fusion + lane) | always succeed — pure computation | n/a | n/a |

The risk here is **silent quality degradation**: every stage emitting
`InsufficientEvidence` produces fully vacuous fusion, and the lane
router decides Yellow (the default conservative bucket). Operators
see "Yellow lane" — they do not see "every stage broken."

**FINDING-FM-7 (MEDIUM)** — add an alert on
`recor_verification_stages_total{outcome="insufficient_evidence"}` ratio
exceeding some threshold (e.g., > 80 % over 10m). Today only
per-lane counters are alerted on (cf. alert-rules).

### 6.2.3 Rate limiter false positive

`PrincipalKeyExtractor` returns `UnableToExtractKey` when no Principal
is in extensions (`services/declaration/src/api/rate_limit.rs:55-64`).
Tower-governor maps this to 500. This is fail-closed-by-design but
surfaces as a 500 to the caller.

| Failure mode | Detection | Mitigation | State | Severity |
|---|---|---|---|---|
| **Auth middleware not run before rate limiter** | 500 from rate limiter | routing bug — caught by integration tests | safe | LOW |
| **Legitimate spike (one declarant submits many)** | 429 with Retry-After | adjust `RATE_LIMIT_BURST` / `RATE_LIMIT_PER_MIN` | safe | LOW |
| **Multi-replica skew** | the rate limit is in-process; in multi-replica deployments effective limit is N × per_min | accepted v1 risk; multi-replica integration test pinned to 1 replica | safe | MEDIUM — FINDING-FM-8 (multi-replica rate-limit ↑ effective ceiling) |

### 6.2.4 DLQ filling

Already extensively covered above and in
`docs/runbooks/dlq-inundation.md`. Alert + runbook + admin endpoint
all in place. No new gap.

### 6.2.5 Retention worker — drift

The `OutboxRetentionWorker`
(`services/declaration/src/infrastructure/retention.rs`) runs at
`OUTBOX_RETENTION_INTERVAL_SECONDS` (default 24h) and prunes outbox
rows whose `dispatched_at` is older than `OUTBOX_RETENTION_DAYS`.

| Failure mode | Detection | Mitigation | State | Severity |
|---|---|---|---|---|
| **`OUTBOX_RETENTION_DAYS=0` (default)** | no pruning happens | safe default — operator must explicitly opt in; comment at config:163-168 documents | safe — table grows | LOW |
| **Retention worker too aggressive** (deletes undispatched rows) | code defends — WHERE clause checks `dispatched_at IS NOT NULL` (`services/declaration/src/infrastructure/retention.rs`) | invariant tested | safe | LOW |
| **Retention worker deletes DLQ rows** | accident | code explicitly excludes `outbox_dlq` (config doc:166-168) | safe | LOW |
| **Retention worker fails silently** | metric: `recor_retention_pruned_total` zero for > 24h | warn → operator investigation | safe; rows accumulate but service still works | partial | MEDIUM — FINDING-FM-9 (no alert on retention-worker silence) |

### 6.2.6 AI hallucination / safety chokepoint bypass

The Inference Gateway has **safety-chokepoint** behaviour built in:
the structured-output schema mandates `enum: [adverse, clear,
insufficient_evidence]` (`services/verification-engine/src/application/stages/stage5_adverse_media.rs:79-100`).
Anthropic's tool-use validation enforces the schema server-side; a
malformed response is rejected before it reaches the gateway.

| Failure mode | Detection | Mitigation | State | Severity |
|---|---|---|---|---|
| **Verdict valid but wrong** | not detectable inline | persisted `evidence_citations`; post-hoc human review | recorded with prompt-hash | MEDIUM (model risk; accepted) |
| **Prompt injection** (input contains "ignore previous instructions") | hard to detect | input fields are name-shaped (no free-form text inputs); schema-enforced output | accepted v1 risk | MEDIUM — FINDING-FM-10 (no prompt-injection corpus test) |
| **Schema bypass** (model returns extra fields) | gateway rejects via JSON-schema validation | fail-soft to insufficient_evidence | safe | LOW |
| **PII leak via prompt** | input includes full name + entity name + ICIJ snippet | prompt is hashed and span-stamped only; not logged in clear in prod | safe under OPS-2 redaction | LOW |
| **Inference Gateway logs prompt verbatim** | code review | redact-by-default in lib.rs (verify in 05-permissions.md) | depends on impl | LOW |

### 6.2.7 Migration failing partway

Sqlx-migrate runs each migration in its own transaction where DDL
permits. Some Postgres DDL (`CREATE INDEX CONCURRENTLY`) cannot be
inside a transaction.

| Failure mode | Detection | Mitigation | State | Severity |
|---|---|---|---|---|
| **Migration crash mid-transaction** | sqlx-migrate aborts; pod CrashLoopBackoff | next pod starts; migration rolled back (in-tx case) | safe | LOW |
| **Migration crash AFTER `CREATE INDEX CONCURRENTLY`** | partial schema state | NO automatic recovery; DBA must clean up the `INVALID` index | DB schema in transitional state | manual | HIGH — FINDING-FM-2 (no runbook for partial migration cleanup) |
| **Migration version collision** | startup migrate refuses | service refuses to start | safe — fail-closed | runbook gap | MEDIUM |
| **Schema cache stale** (`.sqlx/` not regenerated) | build error in CI | `docs/runbooks/sqlx-cache-regeneration.md` exists | safe | LOW |

### 6.2.8 Chaincode bytecode hash mismatch

Already covered in 6.1.6.

### 6.2.9 HMAC secret-rotation race

Already covered in 6.1.3 with the dual-secret pattern at
`services/declaration/src/api/internal.rs:281-297`. Runbook at
`docs/runbooks/hmac-secret-rotation.md`.

### 6.2.10 Env-var misconfig selecting dev substitute in prod

| Failure mode | Detection | Mitigation | State | Severity |
|---|---|---|---|---|
| **`ENVIRONMENT=dev` set in prod accidentally** | dev-header `X-Recor-Dev-Principal` becomes accepted; OIDC bypass | `Config::from_env` refuses to start when `oidc_issuer_url` is empty and environment != dev (`services/declaration/src/config.rs:282-284`); does NOT refuse when `environment=dev` AND `oidc_issuer_url` is set | depends — if both env=dev AND OIDC configured, both auth paths accepted; this is a HIGH risk | none | HIGH — FINDING-FM-11 (no startup refusal when environment=dev BUT a prod-style OIDC issuer is configured) |
| **`PERSON_SERVICE_URL` empty in prod** | submission silently skips Person-registry cross-check (R-DECL-4) | the `SubmitDeclarationUseCase` builder makes `person_registry` optional (`services/declaration/src/application/submit_declaration.rs:60-64`); empty URL → `None` adapter → no check | submission accepts unknown person ids | warn at startup? (verify) | partial | HIGH — FINDING-FM-12 (verify and gate) |
| **`AUTH_TRANSPORT=hmac` in prod when intent was mtls-only** | service starts with HMAC only — mTLS layer absent | log info on chosen path (`services/verification-engine/src/main.rs:147-176`) | safe in isolation; misconfig | startup log only; should be alerted | partial | MEDIUM |

### 6.2.11 Clock skew

| Failure mode | Detection | Mitigation | State | Severity |
|---|---|---|---|---|
| **Pod clock behind** | NBF claim rejects valid tokens | NTP via host; k8s pods inherit host clock | safe — fail-closed (401) | metric label `invalid` | LOW |
| **Pod clock ahead** | EXP claim treats valid tokens as expired | NTP | safe — fail-closed | metric | LOW |
| **Cross-service skew** (D and V at different times) | event ordering anomalies in `submitted_at` | not currently bounded — relies on host NTP | safe at v1 traffic | none | MEDIUM — FINDING-FM-13 (no clock-skew probe between pods) |

---

## 6.3 Test coverage matrix

| Component | Unit | Integration | Chaos / load | Notes |
|---|---|---|---|---|
| Declaration submit | ✓ | ✓ (`tests/`) | partial (load-test for submit) | well-covered |
| HMAC verify (rotation) | ✓ | ✓ | n/a | well-covered |
| Outbox relay | ✓ | ✓ | n/a | well-covered |
| Outbox DLQ replay | ✓ | ✓ | n/a | well-covered |
| Fabric bridge retries | ✓ | partial | partial | bridge tested; Fabric outage not chaos-tested |
| Anthropic gateway | ✓ | partial (fixture-mode) | n/a | hallucination not tested; covered by structured-output enforcement |
| OIDC verify | ✓ | ✓ | n/a | well-covered |
| Vault bootstrap | ✓ | ✓ | n/a | well-covered |
| SPIFFE bootstrap | ✓ | ✓ | n/a | well-covered |
| Postgres outage | partial | ✓ | n/a | well-covered |
| Verification pipeline | ✓ | ✓ | partial | each stage unit-tested |

---

## 6.4 Findings summary (Section 6)

| ID | Severity | Title | Suggested ticket scope |
|---|---|---|---|
| FM-1 | MEDIUM | No disk-pressure section in `restore-database-from-backup.md` | one-page runbook addition |
| FM-2 | HIGH | No partial-migration cleanup runbook (especially `CREATE INDEX CONCURRENTLY` failure) | runbook + DBA procedure |
| FM-3 | MEDIUM | No automated audit-divergence alert | metric + alert |
| FM-4 | MEDIUM | No alert on stale SPIFFE trust bundle | one alert rule |
| FM-5 | MEDIUM | No documented bypass-paging path when Alert Manager is down | runbook |
| FM-6 | MEDIUM | No tokio-panic escalation shim — panics in spawned tasks are silent | shim crate + integration test |
| FM-7 | MEDIUM | No alert on `recor_verification_stages_total{outcome=insufficient_evidence}` ratio | alert rule |
| FM-8 | MEDIUM | Multi-replica rate-limit skew — effective ceiling = N × per_min | doc + redis-backed limiter (deferred) |
| FM-9 | MEDIUM | No alert on retention-worker silence | metric + alert |
| FM-10 | MEDIUM | No prompt-injection corpus test for Stage 5 | adversarial corpus + CI |
| FM-11 | HIGH | `environment=dev` with prod-style OIDC configured does NOT trigger startup refusal — dev-header `X-Recor-Dev-Principal` accepted alongside real OIDC | config-validation tightening |
| FM-12 | HIGH | `PERSON_SERVICE_URL` empty silently disables R-DECL-4 cross-service check | startup warn → startup refusal in prod |
| FM-13 | MEDIUM | No clock-skew probe between pods | meta-alert |

Carried over from Section 5: DF-1, DF-2, DF-3, DF-4, DF-5, DF-6, DF-7, DF-8.

---

## 6.5 Cross-cutting recommendations

### Pre-launch must-fix (HIGH)

- FM-2 — partial migration cleanup runbook
- FM-11 — environment=dev + OIDC gate
- FM-12 — PERSON_SERVICE_URL prod-gate
- DF-2 — ship R-LOOP-2 Kafka cutover (with iat) before public launch

### Pre-launch should-fix (MEDIUM)

- FM-1, FM-3, FM-4, FM-5, FM-6, FM-7, FM-9, FM-13
- DF-1, DF-3, DF-4, DF-6

### Backlog (LOW)

- DF-5, DF-7, DF-8
- FM-8 (multi-replica rate-limit) — accepted v1 risk; deferred until
  horizontal scaling matters
- FM-10 (prompt-injection corpus) — accepted v1 risk; backlog for
  v1.1

### Non-actionable (covered or out-of-scope)

- Various "well-covered" entries above
