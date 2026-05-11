---
name: recor-migration
description: Database migration work. Fires when a database migration is being designed or reviewed. Loads the migration discipline, property-test pattern, and approval requirements.
---

# RÉCOR migration discipline

Database schemas are sovereign data. Migrations are subject to enhanced care.

## Rules

1. **Forward-only**. No automated rollback. Rollback is by a forward migration
   that reverses the change.
2. **Property-tested**. Every migration ships with property tests against a
   representative dataset.
3. **Hot-deployable** wherever possible. Use online schema-change techniques
   (Postgres expand/contract pattern).
4. **Transaction-wrapped** wherever the operation supports transactional DDL.
5. **Idempotent on replay**. `IF NOT EXISTS`, `IF EXISTS`, etc.
6. **Annotated**. Every migration has a header with rationale, sprint, author,
   reviewers.

## Migration tooling

Rust services: sqlx migrate.
Go services: goose (the team's standard Go migration tool).
Locations: /services/<svc>/migrations/

## Header template

```sql
-- Migration: 0042_<imperative-description>
-- Service: declaration
-- Sprint: PI-2 sprint 5
-- Author: <name>
-- Reviewers: <name>, <name>
-- Rationale: <one paragraph>
-- Properties verified post-migration:
--   1. Row count in declaration_events unchanged
--   2. aggregate_version remains monotonic per aggregate_id
--   3. New column declaration_decision is NOT NULL after backfill
```

## Pattern: adding a column

```sql
-- Migration: 0043_add_decision_column.sql
BEGIN;

ALTER TABLE declarations
  ADD COLUMN IF NOT EXISTS decision text;

-- Index later (in a separate migration if hot deploying); see 0044
COMMIT;
```

```sql
-- Migration: 0044_index_decision_column.sql
-- Concurrent index creation cannot be in a transaction.
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_declarations_decision
    ON declarations (decision)
    WHERE decision IS NOT NULL;
```

## Pattern: backfilling

Always background backfill, never blocking the main transaction. Add the new
column nullable; backfill in batches; only then add NOT NULL via a later
migration after backfill is complete.

## Always require human approval

Migrations are never auto-applied. The migration-specialist agent reviews;
the architect-reviewer reviews; production deployment is gated by the
deployment pipeline with operator approval.
