-- Migration 0002: Person-service per-row RBAC (closes FIND-005 + FIND-006).
--
-- Pre-Sprint-1, the persons projection had no column tracking which
-- principal registered the row. `register_person`, `get_person`, and
-- `search_persons` accepted any authenticated bearer and returned the
-- full Sensitive-PII payload. That is the FIND-005 / FIND-006 attack
-- surface called out by the whole-system audit (docs/audit/10-findings.md).
--
-- This migration adds the `created_by_principal` column, backfills it
-- from the per-event `actor_principal` already recorded in
-- `person_events.event_payload`, and tightens the column to NOT NULL +
-- adds a partial index over the live (non-merged-out) rows for the
-- search-filter query the handler runs on every non-admin search.
--
-- D15 / COMP-2 invariant: the `person_events` log is the canonical
-- source of `actor_principal`. We populate `persons.created_by_principal`
-- from that log in a single transaction; the projection becomes a fast
-- read path for the RBAC predicate without competing with the audit log
-- on read latency.
--
-- The backfill uses the event with the LOWEST aggregate_version per
-- person — the registered event (version 1). If a person somehow lacks
-- any event (impossible in normal flow but defensive), the column
-- defaults to '__legacy_unknown__' so the NOT NULL constraint at the
-- end succeeds; that sentinel matches nothing in the admin allowlist
-- and nothing a real OIDC principal would carry, so denied-by-default
-- (D14 fail-closed).

BEGIN;

-- ── Phase 1: nullable column so existing rows are valid mid-migration ──
ALTER TABLE persons
    ADD COLUMN IF NOT EXISTS created_by_principal TEXT NULL;

-- ── Phase 2: backfill from person_events (oldest event per aggregate) ──
-- The first event for any person is `person.registered.v1`; its payload
-- carries the `actor_principal` that minted the row.
WITH first_events AS (
    SELECT DISTINCT ON (person_id)
           person_id,
           event_payload ->> 'actor_principal' AS actor_principal
      FROM person_events
     ORDER BY person_id, aggregate_version
)
UPDATE persons p
   SET created_by_principal = COALESCE(fe.actor_principal, '__legacy_unknown__')
  FROM first_events fe
 WHERE p.person_id = fe.person_id
   AND p.created_by_principal IS NULL;

-- ── Phase 3: catch any orphan row (no event at all — defensive) ────────
UPDATE persons
   SET created_by_principal = '__legacy_unknown__'
 WHERE created_by_principal IS NULL;

-- ── Phase 4: enforce NOT NULL going forward ────────────────────────────
ALTER TABLE persons
    ALTER COLUMN created_by_principal SET NOT NULL;

-- ── Phase 5: a non-empty principal string is the contract ──────────────
ALTER TABLE persons
    ADD CONSTRAINT persons_created_by_principal_nonempty
        CHECK (char_length(created_by_principal) BETWEEN 1 AND 1024);

COMMENT ON COLUMN persons.created_by_principal IS
    'Authenticated principal subject (OIDC `sub` or admin id) that registered this row. Drives the FIND-005/006 RBAC predicate on get_person + search_persons. Immutable after INSERT. Backfilled from person_events.event_payload->>actor_principal in migration 0002.';

-- ── Phase 6: index for non-admin search filter ─────────────────────────
-- `WHERE merged_into IS NULL` mirrors the existing projection indexes;
-- merged-out shells never need to surface to a declarant via search.
CREATE INDEX IF NOT EXISTS idx_persons_created_by_principal
    ON persons (created_by_principal)
    WHERE merged_into IS NULL;

COMMIT;
