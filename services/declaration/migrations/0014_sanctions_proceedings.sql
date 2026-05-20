-- TODO-004 — Sanctions-for-non-compliance proceedings.
--
-- FATF R.24 c.24.13 requires "proportionate, dissuasive, effective"
-- sanctions for failure to comply with BO requirements. This
-- migration adds the workflow's data substrate: a `sanctions_proceedings`
-- projection + a COMP-2-immutable `sanction_events` log. ADR-0012
-- defines the proportionality ladder.
--
-- States (the ladder):
--   submitted     — proceeding opened against a declaration / entity
--   reminder      — administrative reminder issued (first missed
--                   update window)
--   fined         — administrative fine imposed (escalating tiers
--                   captured in `tier`)
--   suspended     — registry status suspended (declaration flagged
--                   non-current; entity flagged)
--   referred      — referral to ANIF / COBAC (regulated-counterparty
--                   path); the referral row is logged here AND a
--                   separate write lands in fiu_disclosure_log when
--                   the referral is to ANIF
--   public_listed — name published on the public non-compliers list
--                   (post-Sovim balancing); requires admin
--                   justification text
--   withdrawn     — proceeding withdrawn (e.g. discrepancy resolved,
--                   declaration corrected); the public list MUST be
--                   updated within 24 hours
--
-- The ladder is forward-only EXCEPT `withdrawn` (which is the only
-- terminal state besides escalating from `public_listed` back).
-- Skipping ladder steps is allowed when warranted (e.g. egregious
-- non-compliance → straight to suspension); the justification text
-- is required on every transition.

BEGIN;

CREATE TABLE IF NOT EXISTS sanctions_proceedings (
    proceeding_id        UUID PRIMARY KEY,
    -- At least one of declaration_id / entity_id MUST be present.
    declaration_id       UUID,
    entity_id            UUID,
    reason_code          TEXT NOT NULL,
    state                TEXT NOT NULL CHECK (state IN (
        'submitted', 'reminder', 'fined', 'suspended',
        'referred', 'public_listed', 'withdrawn'
    )),
    tier                 INT CHECK (tier IS NULL OR tier BETWEEN 1 AND 5),
    initiated_by         TEXT NOT NULL,
    initiated_at         TIMESTAMPTZ NOT NULL,
    last_transition_at   TIMESTAMPTZ NOT NULL,
    last_actor           TEXT NOT NULL,
    last_justification   TEXT NOT NULL,
    -- The `public_listed` substring is on the publicly-visible Sovim-
    -- balancing list. The columns reify what the public list shows
    -- so the public-list endpoint reads from a single row.
    public_listed_at     TIMESTAMPTZ,
    public_listing_name  TEXT,
    public_listing_reason TEXT,
    withdrawn_at         TIMESTAMPTZ,
    aggregate_version    BIGINT NOT NULL DEFAULT 0,
    CONSTRAINT sanctions_target_present
        CHECK (declaration_id IS NOT NULL OR entity_id IS NOT NULL)
);

CREATE INDEX IF NOT EXISTS idx_sanctions_declaration
    ON sanctions_proceedings (declaration_id, state)
    WHERE declaration_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_sanctions_entity
    ON sanctions_proceedings (entity_id, state)
    WHERE entity_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_sanctions_public_listed
    ON sanctions_proceedings (public_listed_at)
    WHERE state = 'public_listed';

CREATE TABLE IF NOT EXISTS sanction_events (
    event_id       UUID PRIMARY KEY,
    proceeding_id  UUID NOT NULL REFERENCES sanctions_proceedings (proceeding_id),
    event_type     TEXT NOT NULL,
    payload        JSONB NOT NULL,
    actor_principal TEXT NOT NULL,
    justification  TEXT NOT NULL,
    occurred_at    TIMESTAMPTZ NOT NULL,
    sequence_no    BIGINT NOT NULL,
    UNIQUE (proceeding_id, sequence_no)
);

CREATE INDEX IF NOT EXISTS idx_sanction_events_proceeding
    ON sanction_events (proceeding_id, sequence_no);

-- COMP-2 immutability — same shape as other audit tables.
DROP TRIGGER IF EXISTS forbid_sanction_events_update ON sanction_events;
CREATE TRIGGER forbid_sanction_events_update
    BEFORE UPDATE ON sanction_events
    FOR EACH ROW EXECUTE FUNCTION declaration_events_refuse_mutation();

DROP TRIGGER IF EXISTS forbid_sanction_events_delete ON sanction_events;
CREATE TRIGGER forbid_sanction_events_delete
    BEFORE DELETE ON sanction_events
    FOR EACH ROW EXECUTE FUNCTION declaration_events_refuse_mutation();

DROP TRIGGER IF EXISTS forbid_sanction_events_truncate ON sanction_events;
CREATE TRIGGER forbid_sanction_events_truncate
    BEFORE TRUNCATE ON sanction_events
    FOR EACH STATEMENT EXECUTE FUNCTION declaration_events_refuse_mutation();

REVOKE UPDATE, DELETE, TRUNCATE ON sanction_events FROM PUBLIC;

COMMIT;
