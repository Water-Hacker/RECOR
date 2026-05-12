# Runbook — `worker-fabric-bridge`

Operational procedures for the Fabric audit anchoring bridge worker.
The bridge consumes declaration events from the outbox-relay channel
and anchors each to the Hyperledger Fabric audit channel via the
`audit-witness` chaincode. See
`docs/adr/0009-fabric-audit-anchoring.md` for the decision record.

## Service overview

- **Binary**: `worker-fabric-bridge` (`apps/worker-fabric-bridge/`)
- **Default port**: 8090 (configurable via `BIND_ADDR`)
- **Surfaces**:
  - `POST /v1/relay` — HMAC-authenticated receiver for outbox-relay
  - `GET /healthz` — liveness probe
  - `GET /readyz` — readiness probe
  - `GET /metrics` — Prometheus exposition
- **Persistent state**: `fabric_bridge_dlq` table (Postgres)
- **Upstream**: Fabric Gateway HTTP shim (`FABRIC_GATEWAY_URL`)
- **Owner**: SRE + the infra team (Fabric cluster ops)

## Environment

| Variable | Required | Default | Purpose |
|---|---|---|---|
| `DATABASE_URL` | yes | — | Postgres connection for DLQ |
| `FABRIC_GATEWAY_URL` | yes | — | Gateway shim base URL |
| `RECOR_FABRIC_BRIDGE_HMAC` | yes | — | Shared secret with declaration service |
| `BIND_ADDR` | no | `0.0.0.0:8090` | Listener address |
| `FABRIC_CHANNEL` | no | `recor-audit` | Channel name |
| `FABRIC_CHAINCODE` | no | `audit-witness` | Chaincode name |
| `FABRIC_BRIDGE_MAX_ATTEMPTS` | no | `5` | Bridge retry budget |
| `FABRIC_BRIDGE_REQUEST_TIMEOUT_MS` | no | `10000` | Per-attempt timeout |
| `FABRIC_BRIDGE_BACKOFF_BASE_MS` | no | `500` | Initial backoff |
| `FABRIC_GATEWAY_TOKEN` | no | — | Bearer token for gateway shim |
| `FABRIC_BRIDGE_TRANSPORT` | no | `http` | Reserved for R-LOOP-2 Kafka switch |

## Start / stop

The worker is a Kubernetes Deployment under
`infrastructure/kubernetes/worker-fabric-bridge/` (deployed by ArgoCD).
For manual operations:

```bash
# Restart (rolling)
kubectl -n recor rollout restart deployment/worker-fabric-bridge

# Stop (scale to zero — drains in-flight requests via graceful shutdown)
kubectl -n recor scale deployment/worker-fabric-bridge --replicas=0

# Start
kubectl -n recor scale deployment/worker-fabric-bridge --replicas=2
```

Local development:

```bash
cd apps/worker-fabric-bridge
DATABASE_URL=postgres://recor:recor@localhost:5440/recor \
FABRIC_GATEWAY_URL=http://localhost:7050 \
RECOR_FABRIC_BRIDGE_HMAC=dev-secret \
cargo run --release
```

## Health checks

| Probe | Endpoint | Pass criterion |
|---|---|---|
| Liveness | `GET /healthz` | 200 within 1s |
| Readiness | `GET /readyz` | 200 within 5s |
| Functional | `recor_fabric_anchor_total{result="committed"}` increases over 5min |

A worker that is `live` but not `committing` is the failure mode that
matters — it indicates the Fabric upstream is rejecting or hanging,
which manifests as DLQ growth rather than process crashes.

## Metrics

| Metric | Type | Labels | Meaning |
|---|---|---|---|
| `recor_fabric_anchor_total` | counter | `result` ∈ {`committed`, `already_committed`, `retried`, `permanent_failure`} | Anchor attempt outcomes |
| `recor_fabric_anchor_latency_seconds` | histogram | — | Per-attempt bridge → gateway round-trip |
| `recor_fabric_dlq_writes_total` | counter | `cause` ∈ {`permanent`, `non_retryable`, `config`} | DLQ writes by failure type |

### Alerting thresholds (suggested, tune in PI-2 sprint 4)

- `rate(recor_fabric_anchor_total{result="permanent_failure"}[5m]) > 0.1`
  → page (sustained permanent failures = Fabric outage)
- `rate(recor_fabric_dlq_writes_total[1h]) > 0` → warn
  (any DLQ row in an hour deserves operator review)
- `histogram_quantile(0.99, rate(recor_fabric_anchor_latency_seconds_bucket[5m])) > 5`
  → warn (p99 latency > 5s suggests gateway-side congestion)

## DLQ inspection

```sql
-- Total DLQ size
SELECT COUNT(*) FROM fabric_bridge_dlq;

-- Recent 24 hours, by cause
SELECT cause, COUNT(*), MAX(dead_lettered_at) AS most_recent
FROM fabric_bridge_dlq
WHERE dead_lettered_at > NOW() - INTERVAL '24 hours'
GROUP BY cause;

-- Find a specific event's DLQ row
SELECT * FROM fabric_bridge_dlq WHERE event_id = '...';

-- Latest 10 dead-letters with the truncated error message
SELECT event_id, event_type, cause, dead_lettered_at, LEFT(last_error, 200) AS err
FROM fabric_bridge_dlq
ORDER BY dead_lettered_at DESC
LIMIT 10;
```

## Common failure modes

### 1. Gateway shim returning 5xx

**Symptom**: `recor_fabric_anchor_total{result="permanent_failure"}` rising,
`fabric_bridge_dlq.cause = 'permanent'`, `last_error` mentions HTTP 5xx.

**Likely cause**: the gateway shim is unhealthy or the Fabric peer it
proxies to is down.

**Action**:
1. `curl -fsSL $FABRIC_GATEWAY_URL/healthz` — verify the shim is alive.
2. Inspect the shim's own logs (`kubectl -n fabric logs deployment/fabric-gateway-shim`).
3. If the shim is alive but the peer is not, escalate to the infra team
   for Fabric cluster recovery.
4. Once the upstream is healthy, manually re-anchor (see below).

### 2. Chaincode rejecting requests

**Symptom**: `fabric_bridge_dlq.cause = 'non_retryable'`, `last_error`
contains "receipt_hash_hex" or "must not be empty".

**Likely cause**: contract violation between declaration service and
chaincode — a payload shape change.

**Action**:
1. Inspect one DLQ row's `payload` field.
2. Compare to the chaincode's expected field set (see
   `chaincode/audit-witness/lib/audit_witness.go`).
3. File a P1 bug — the declaration service has emitted a malformed event.
4. Do NOT re-anchor these rows until the contract is fixed.

### 3. HMAC mismatch from relay

**Symptom**: worker logs "rejected relay request: HMAC mismatch", DLQ
is empty but inbound event count is zero.

**Likely cause**: `RECOR_FABRIC_BRIDGE_HMAC` mismatch between declaration
service and the worker.

**Action**:
1. Verify both services hold the same value from the secrets store.
2. Restart whichever side has the stale value.
3. The relay will retry pending rows automatically; no manual re-anchor
   needed.

## Manual re-anchor

For DLQ rows with `cause = 'permanent'` whose root cause is now fixed:

```bash
# Single row
psql "$DATABASE_URL" -c "
SELECT event_id, payload FROM fabric_bridge_dlq WHERE event_id = '<uuid>';
"

# Issue the chaincode invocation via the gateway shim
curl -fsSL -X POST "$FABRIC_GATEWAY_URL/v1/transactions/recor-audit/audit-witness" \
  -H "Authorization: Bearer $FABRIC_GATEWAY_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{
    "method": "PutAuditEntry",
    "args": ["<event_id>", "<declaration_id>", "<receipt_hash_hex>", "<ts>", "<att_hex>"]
  }'

# On success (200 with tx_id), clear the DLQ row
psql "$DATABASE_URL" -c "
DELETE FROM fabric_bridge_dlq WHERE event_id = '<uuid>';
"
```

The chaincode is idempotent — re-anchor of an already-committed event
returns 409, which is also a success outcome for the operator.

A bulk re-anchor script is a follow-up ticket; today, manual rows.

## Escalation

| Condition | Escalate to |
|---|---|
| DLQ growing > 10 rows/min | SRE on-call + infra team |
| Gateway shim down > 15min | infra team Fabric cluster lead |
| Sustained permanent_failure rate | SRE on-call + lead architect |
| Chaincode rejecting valid requests | declaration service team + lead architect |

## See also

- `docs/runbooks/audit-verification.md` — how an external party verifies
  a declaration via the audit-verifier
- `docs/runbooks/dlq-inundation.md` — outbox-side DLQ (the upstream
  pressure source)
- `docs/adr/0009-fabric-audit-anchoring.md` — design decision
- `chaincode/audit-witness/README.md` — chaincode surface
