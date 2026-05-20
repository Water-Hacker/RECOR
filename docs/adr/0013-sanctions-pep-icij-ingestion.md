# ADR-0013 — Sanctions / PEP / ICIJ ingestion architecture

- **Status:** Accepted (2026-05-20)
- **Deciders:** Verification team, Operations team, Lead architect
- **Closes:** TODO-014 (scaffold + ADR + per-feed audit log)
- **Related:** ADR-0011 (BUNEC pluggability)

## Context

The verification engine screens beneficial-owner candidates against
three classes of authoritative feed:

1. **Sanctions lists** — OFAC SDN, EU CFSP, UN Consolidated. Used by
   Stage 3 of the pipeline.
2. **PEP (Politically Exposed Person) lists** — public-record
   curations of high-risk political figures. Stage 4.
3. **ICIJ leak datasets** — Offshore Leaks, Panama Papers, Paradise
   Papers, Pandora Papers. Stage 5 adverse-media inputs.

The pre-TODO-014 surface had the destination tables
(`sanctions_persons`, `peps`, `icij_persons`) but **no ingestion
code**. Operators seeded the tables by hand via SQL fixtures. The
verification engine claimed to screen "against OFAC" — it screened
against whatever frozen snapshot an operator last loaded.

## Decision: per-source sub-binaries under a single workspace app

New workspace crate `apps/sanctions-ingest`. Each upstream feed gets
its own binary under `src/bin/<source>.rs`. The library
(`src/lib.rs`) hosts the cross-source machinery — sanity check, audit
log, BLAKE3 digest of raw bytes — and exposes per-source parser
modules under `src/<source>.rs`.

| Source | Cadence | Format | Binary |
|---|---|---|---|
| OFAC SDN | Daily | XML | `recor-sanctions-ingest-ofac` (shipped) |
| EU CFSP | Weekly | XML | `recor-sanctions-ingest-eu` (follow-up) |
| UN Consolidated | Irregular | XML | `recor-sanctions-ingest-un` (follow-up) |
| ICIJ Offshore Leaks | Per-leak | CSV | `recor-sanctions-ingest-icij` (follow-up) |
| ICIJ Panama | Per-leak | CSV | Sub-mode of above |
| ICIJ Paradise | Per-leak | CSV | Sub-mode of above |
| ICIJ Pandora | Per-leak | CSV | Sub-mode of above |

The OFAC binary ships as the canonical demonstration. Every other
binary follows the same shape: fetch → BLAKE3 digest → parse →
sanity check → upsert → ingest-log row.

The XML schema model for OFAC is intentionally NOT in this commit;
the surrounding flow (fetch, digest, count-based sanity check,
audit-log write) IS. The schema model lands in TODO-014-OFAC; the
operator can exercise the end-to-end runbook today against a local
fixture, and the upsert step is the only follow-up.

## The audit-log substrate

Migration `services/verification-engine/migrations/0008_sanctions_ingest_log.sql`
creates `sanctions_ingest_log`:

| Column | Purpose |
|---|---|
| `ingest_id` | Stable identifier per run |
| `source` | One of the enumerated source names (CHECK constraint) |
| `source_revision` | Upstream's published revision; UNIQUE per source |
| `raw_bytes_digest_hex` | BLAKE3 of the raw fetched bytes (D15) |
| `prior_row_count` / `proposed_row_count` | Delta |
| `applied` | Whether the upsert ran (false on sanity-check block) |
| `force_justification` | Set when the operator passed `--force` |
| `ingested_at` | UTC timestamp |

The table is COMP-2-immutable (BEFORE-UPDATE/DELETE/TRUNCATE refused
by trigger; mirrors migration 0003's pattern). The unique
`(source, source_revision)` index makes the upsert idempotent — the
same OFAC SDN revision applied twice in a single day writes one row.

## The sanity-check gate

`recor_sanctions_ingest::sanity_check::sanity_check(prior, proposed, max_drop_ratio)`
returns `Pass` or `Blocked` based on a one-sided percentage drop
threshold. The default `max_drop_ratio = 0.25` blocks when the new
feed has more than 25% fewer rows than the prior revision — the
classic "OFAC released an empty file by accident" failure mode.

When blocked, the binary writes the `ingest_log` row with
`applied=false` and exits with code `6`. Operator override is
`--force "<justification>"`; the justification is recorded in the
log row so a later audit can see why the override was applied.

`prior == 0` (first ingestion of a source) always passes. Growth
never blocks.

## D14 fail-closed properties

| Failure | Behaviour |
|---|---|
| Network fetch fails | Binary exits non-zero; no log row, no upsert |
| Source bytes empty | Parser refuses; no log row, no upsert |
| Sanity check blocks (no force) | Log row with `applied=false`; exit 6; no upsert |
| Database unreachable | Binary exits non-zero; no log row landed (D14 — better not to upsert without the audit row) |

## D15 cryptographic provenance properties

Every ingest_log row carries BLAKE3 of the raw bytes. An auditor can,
months later, ask "what was OFAC SDN's content on 2026-04-12":

1. Query `sanctions_ingest_log` for the latest row with
   `source='ofac_sdn' AND ingested_at <= '2026-04-12'`.
2. Read `source_revision` and `raw_bytes_digest_hex`.
3. Pull the raw snapshot from the operator's object storage
   (the operator runbook prescribes keeping snapshots; the
   `ingest_log` row is the cryptographic anchor).
4. Verify BLAKE3 matches.

## Cron / scheduling

The ingest binaries are deliberately stateless single-shot processes.
Scheduling is the operator's responsibility — Kubernetes CronJob is
the default pattern:

```yaml
apiVersion: batch/v1
kind: CronJob
metadata:
  name: recor-sanctions-ingest-ofac
spec:
  # Daily at 02:00 UTC — OFAC publishes overnight US time.
  schedule: "0 2 * * *"
  jobTemplate:
    spec:
      template:
        spec:
          containers:
            - name: ingest
              image: ghcr.io/water-hacker/recor-sanctions-ingest-ofac:latest
              env:
                - { name: DATABASE_URL, valueFrom: { secretKeyRef: { name: ve-db, key: url } } }
                - { name: OFAC_SDN_URL, value: "https://www.treasury.gov/ofac/downloads/sdn.xml" }
          restartPolicy: OnFailure
```

The operator runbook documents per-source cadences and the
appropriate cron entries (`docs/runbooks/sanctions-ingest.md`,
planned).

## Rationale

**Why one workspace app + per-source binaries, not one big worker?**
Sources fail independently. A broken OFAC parser must not block the
EU CFSP ingestion. Per-source binaries means per-source CronJobs,
per-source images, per-source alerts.

**Why sanity-check inside the binary instead of in the DB constraint?**
The CHECK constraint at insertion time only sees one row; the count-
delta check needs the prior row count. Putting it in the binary is
the natural placement — it's where the operator's `--force` lever
also lives.

**Why don't we just stream the raw XML into a JSONB column and
parse later?** The verification engine queries the screening tables
synchronously during pipeline runs. Parsing-on-read at every
verification would blow the SLO. Parse-on-ingest is the right call.

## Consequences

### Positive

- Operators can answer "is the platform's OFAC SDN current" by
  querying `sanctions_ingest_log` and confirming the latest row is
  within 24h.
- The forensic story is complete: digest + revision + delta + force
  justification are all in the log.
- New sources are additive: a new sub-binary + a new CHECK constraint
  value + a new cron entry.

### Negative

- The full XML schema model for each source is the remaining work.
  The TODO-014-OFAC / -EU / -UN / -ICIJ follow-ups will land them.
  The surrounding flow is verified independently — adding the model
  is a localised change.
- Operators must provision object storage for raw snapshots
  separately. The ingest_log row is the integrity anchor; the
  storage is the body.

## Linked from

- TODOS.md § TODO-014
- apps/sanctions-ingest/src/lib.rs
- services/verification-engine/migrations/0008_sanctions_ingest_log.sql
- docs/runbooks/sanctions-ingest.md (planned)
