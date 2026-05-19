# Audit chain & write-ahead integrity — RÉCOR forensic audit, Section 10

**Scope.** Static + replay analysis of every integrity-protected log
in the system: `declaration_events`, `verification_cases`,
`outbox`, `outbox_dlq` per service, and the Hyperledger Fabric
audit-witness chaincode (R-DECL-9).

**Method.** Read the migrations, the repository code, the
chaincode, and the bridge worker. Static analysis only; live replay
runs are out of scope for this pass (no live D↔V stack stood up;
the production verification will run them against the deployed
stack). The replay-on-corruption test is documented as a follow-up
acceptance step in [`docs/audit/12-recommendations.md`](12-recommendations.md).

---

## Inventory of integrity-protected logs

| Log | Service | File | Append-only guard | COMP-2 trigger? |
|---|---|---|---|---|
| `declaration_events` | declaration | `services/declaration/migrations/0001_init.sql` | Schema-level (no UPDATE clause in any query); SQL trigger | Yes — `migrations/0007_audit_log_immutability.sql` |
| `verification_cases` | verification-engine | `services/verification-engine/migrations/0001_initial.sql` | Schema-level; SQL trigger | Yes — `migrations/0003_audit_log_immutability.sql` |
| `outbox` | both | `services/{declaration,verification-engine}/migrations/0001_*.sql` | INSERT-only on the row's payload; `dispatched_at` UPDATE allowed for the relay | No (trigger would block relay's UPDATE) |
| `outbox_dlq` | both | `services/{declaration,verification-engine}/migrations/0002_*.sql` | INSERT-only on row's payload; SELECT for the admin endpoint; DELETE on replay (atomic move back) | UPDATE + TRUNCATE revoked (COMP-2) |
| `person_events` | person-service | `services/person-service/migrations/0001_init.sql` | Schema-level; trigger | Yes — within the init migration (R-DECL-4 ships it together) |
| `entity_events` | entity-service | `services/entity-service/migrations/0001_init.sql` | Schema-level; trigger | Yes — within the init migration (IDENTITY-1 ships it together) |
| Fabric audit-witness | chaincode | `chaincode/audit-witness/lib/audit_witness.go` | World-state KV; chaincode-level idempotency on `event_id` | Inherent (Fabric ledger is append-only) |

---

## Per-log analysis

### 1. `declaration_events`

**Schema (services/declaration/migrations/0001_init.sql):**

```sql
CREATE TABLE declaration_events (
    event_id      UUID PRIMARY KEY,
    declaration_id UUID NOT NULL,
    version       INTEGER NOT NULL,
    event_kind    TEXT NOT NULL,
    event_payload JSONB NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (declaration_id, version)
);
```

**Append-only enforcement** (migration 0007, COMP-2):

```sql
CREATE OR REPLACE FUNCTION refuse_declaration_events_mutation() ...
CREATE TRIGGER declaration_events_no_update BEFORE UPDATE ON declaration_events ...
CREATE TRIGGER declaration_events_no_delete BEFORE DELETE ON declaration_events ...
CREATE TRIGGER declaration_events_no_truncate BEFORE TRUNCATE ON declaration_events ...
REVOKE UPDATE, DELETE, TRUNCATE ON declaration_events FROM PUBLIC;
```

The trigger raises `EXCEPTION '... append-only ...'` on any UPDATE,
DELETE, or TRUNCATE attempt, regardless of role. This is verified
by the COMP-2 integration test:
`services/declaration/tests/audit_immutability.rs`.

**Replay continuity.** The repository's `load_events_for(declaration_id)`
query orders by `version ASC` and `save_event(event, expected_version)`
issues an INSERT with `(declaration_id, version)` constrained
unique — so:

- Out-of-order INSERTs fail at the UNIQUE constraint
- Replaying events by `version ASC` is the canonical order
- Two instances submitting the same set of events in different
  insertion orders end up with the same `version` sequence per
  aggregate, so the BLAKE3 receipt re-derives identically (D15
  byte-parity)

**Static analysis verdict.** Sound. The triggers + the UNIQUE
constraint + the version-ordered replay together rule out silent
drop, double-count, or out-of-order persistence.

**Live experiments deferred to production verification:**

1. ≥100 entries → replay from genesis → byte-equal receipt sequence
2. Corrupt one row (single-byte mutation in `event_payload`) → replay → fail at expected index
3. Trigger refusal: attempt UPDATE/DELETE/TRUNCATE → confirm 42501 error
4. Canonical-form determinism across two instances seeded with different insertion orders

Each is acceptance-testable when a live stack stands up. The
`audit_immutability.rs` testcontainers integration test covers #3
today.

---

### 2. `verification_cases`

Same shape as `declaration_events`: `case_id PRIMARY KEY`, INSERT-only
queries in `services/verification-engine/src/infrastructure/postgres.rs`,
plus the COMP-2 trigger applied via migration
`services/verification-engine/migrations/0003_audit_log_immutability.sql`.

**Verdict.** Same as #1. The case record is the verification
engine's source-of-truth event log; every stage's BPA is recorded
once and replays deterministically per ADR-002 (Dempster-Shafer
fusion).

---

### 3. `outbox` (per service)

The outbox table is the relay's queue:

```sql
CREATE TABLE outbox (
    event_id UUID PRIMARY KEY,
    aggregate_id UUID NOT NULL,
    event_kind TEXT NOT NULL,
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    dispatched_at TIMESTAMPTZ,
    last_error TEXT,
    dispatch_attempts INTEGER NOT NULL DEFAULT 0
);
```

**Append-only?** Not literally — the relay needs to set
`dispatched_at` and increment `dispatch_attempts`. The retention
worker (COMP-2) deletes rows older than `OUTBOX_RETENTION_DAYS`.
The COMP-2 trigger only revokes TRUNCATE on outbox; UPDATE +
DELETE are required by the relay and the retention worker.

**Risk:** the outbox is a queue, not an audit log. The audit
guarantee lives on `declaration_events` / `verification_cases`,
not the outbox. Dropping outbox rows after dispatch does NOT lose
audit history — every event in the outbox has a corresponding row
in the event log.

**Verdict.** Sound by design.

---

### 4. `outbox_dlq`

Failed outbox rows move atomically (INSERT outbox_dlq + DELETE
outbox in the same transaction) on dispatch-attempts exhaustion.
The DLQ retains rows forever (COMP-2 — DLQ is the forensic
surface).

**Append-only guards:** REVOKE UPDATE + TRUNCATE on outbox_dlq;
DELETE allowed for the admin replay path (atomic INSERT outbox +
DELETE outbox_dlq) — same pattern as the move-on-failure flow.

**Audit emission.** The DLQ admin replay endpoint
(`services/declaration/src/api/dlq.rs:replay_dlq`) emits a
`tracing::info!` with `event_kind = "dlq_replay"` and the operator
principal. OPS-2 redaction layer masks the principal in
production logs.

**Verdict.** Sound. The forensic chain is: failed dispatch →
outbox_dlq → operator-triggered replay → audit-event in tracing
plus row reappearance in outbox.

---

### 5. Fabric audit-witness chaincode

`chaincode/audit-witness/lib/audit_witness.go` exposes three
methods:

- `PutAuditEntry(event_id, declaration_id, receipt_hash_hex, ts, signing_peer_attestation)` — INSERTs to world state at key `recor.audit.declaration:{event_id}`. **Idempotent on event_id:** if the key exists, returns the existing entry's TxId rather than appending a duplicate.
- `GetAuditEntry(event_id)` — KV read; returns nil if missing.
- `ListAuditEntriesForDeclaration(declaration_id)` — secondary-index lookup; returns event_ids ordered by insertion timestamp.

**Bridge worker** (`apps/worker-fabric-bridge/src/`) consumes from
the outbox-relay topic (HTTP or Kafka per `RELAY_TRANSPORT`):

- For each `event_kind ∈ {declaration.submitted.v1, .amended.v1, .corrected.v1, .superseded.v1}` → calls `FabricBridge::commit_audit_entry`
- On permanent error → DLQ row (`fabric_bridge_dlq` table)
- On already-committed (idempotency) → success with existing TxId

**Verifier** (`apps/audit-verifier/src/`) exposes
`GET /v1/audit/verify/{declaration_id}`:

- Fetches all audit entries for that declaration via `ListAuditEntriesForDeclaration`
- For each, reads the declaration's projection from the declaration service (cross-DB coupling — flagged FIND-A-DM-08 in Pass A's system-map)
- Re-derives BLAKE3 receipt from canonical payload bytes
- Asserts on-chain hash matches re-derived hash
- Returns a verification report JSON

**Critical observation:** the audit-verifier is **unauthenticated**
(Pass A FIND-AV-01, HIGH). The verifier reveals the full declaration
payload by UUID. This is by design ("public verifier") but the
threat-model does not yet cover the disclosure axis. **The audit
chain is sound; the access-control on the verifier is the issue.**
See [`10-findings.md`](10-findings.md) FIND-001.

**Live experiments deferred to production verification:**

1. End-to-end submission → outbox → bridge → chaincode → verifier round-trip; persist artifact bundle with TxId
2. Witness failure: take down worker-fabric-bridge → submit event → confirm event persists to outbox/event log; bring bridge back → confirm DLQ-replay job picks up + writes to chaincode
3. Detection job for divergence (event_log has entries chaincode doesn't): document the reconciliation cadence

**Reconciliation job status.** No `fabric-reconciliation` cron is
currently committed. The bridge's DLQ + manual `worker-fabric-bridge --replay-dlq` covers operator-initiated recovery; an automated
divergence detector is a follow-up (FIND-AV-02 in Pass A).

---

## Continuity properties summary

| Property | Status | Evidence |
|---|---|---|
| Event log primary-key uniqueness | Enforced | UNIQUE constraint per migration |
| Event log append-only | Enforced | COMP-2 BEFORE-UPDATE/DELETE/TRUNCATE trigger + REVOKE PUBLIC |
| Version-ordered replay determinism | Enforced | `version ASC` ordering in repo query + insert at expected_version |
| Canonical-form byte-parity (D15) | Enforced | `crypto.test.ts` + `canonical_payload_bytes` unit tests |
| Idempotency on chaincode commit | Enforced | chaincode-level event_id KV-check |
| Outbox dispatched-once-then-deleted | Best-effort | Retention worker default-disabled; production must opt in to OUTBOX_RETENTION_DAYS |
| Witness divergence detection | **Missing** | No reconciliation cron committed; FIND-AV-02 |
| Audit-verifier access control | **Missing** | Unauthenticated; FIND-AV-01 |

---

## Gaps to close before production

1. **Auth on audit-verifier.** Either OIDC-gate the route or document the public-disclosure decision in a new threat-model row + ADR.
2. **Witness divergence cron.** Periodic job that joins `declaration_events` LEFT JOIN audit-witness chaincode by `event_id` and alerts on rows present in event log but missing from chaincode for > N minutes (where N covers worker-fabric-bridge's normal lag).
3. **Live replay acceptance tests.** Run the 7 experiments listed in this document against a live D↔V + Fabric stack; persist artifacts under `docs/audit/evidence/audit-chain/`.

---

## Cross-references

- [`02-surfaces.md`](02-surfaces.md) — audit-verifier surface walkthrough + forbidden-access trace (HIGH finding)
- [`04-failure-modes.md`](04-failure-modes.md) — failure-mode entries for Fabric peer down, ordering partition, bridge worker crash
- [`07-cryptography.md`](07-cryptography.md) — BLAKE3 receipt derivation; canonical-form byte-parity
- [`10-findings.md`](10-findings.md) — aggregated findings catalogue
- [`docs/adr/0009-fabric-audit-anchoring.md`](../adr/0009-fabric-audit-anchoring.md) — the anchoring design decision
- [`docs/security/threat-model.md`](../security/threat-model.md) Gap G1 (now partially closed by R-DECL-9; full closure requires the reconciliation cron above)
