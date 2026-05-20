-- TODO-014 — Sanctions / PEP / ICIJ ingestion audit log.
--
-- Every run of an ingest binary (apps/sanctions-ingest/src/bin/*)
-- writes exactly one row here. The row records the source feed
-- (`source`), the upstream revision string (`source_revision`),
-- the BLAKE3 digest of the raw fetched bytes
-- (`raw_bytes_digest_hex`), the row-count delta, and whether the
-- delta was applied to the target table.
--
-- This lets the operator answer "what did the OFAC SDN list look
-- like on date X" by reading the latest row whose `ingested_at <= X`
-- and inspecting the recorded digest + revision. The actual list
-- contents are reconstructable by re-fetching the upstream feed
-- pinned to that revision (operators are expected to keep raw
-- snapshots in object storage; the `raw_bytes_digest_hex` here is
-- the integrity anchor).
--
-- COMP-2 immutability: the log row is append-only; the upsert path
-- in `recor_sanctions_ingest::ingest_log::write_ingest_log` uses
-- `ON CONFLICT DO NOTHING` so a repeated revision is a no-op rather
-- than an error.

BEGIN;

CREATE TABLE IF NOT EXISTS sanctions_ingest_log (
    ingest_id            UUID PRIMARY KEY,
    source               TEXT NOT NULL CHECK (source IN (
        'ofac_sdn', 'eu_cfsp', 'un_consolidated',
        'icij_offshore_leaks', 'icij_panama', 'icij_paradise',
        'icij_pandora'
    )),
    source_revision      TEXT NOT NULL,
    raw_bytes_digest_hex TEXT NOT NULL,
    prior_row_count      BIGINT NOT NULL,
    proposed_row_count   BIGINT NOT NULL,
    applied              BOOLEAN NOT NULL,
    force_justification  TEXT,
    ingested_at          TIMESTAMPTZ NOT NULL,
    UNIQUE (source, source_revision)
);

CREATE INDEX IF NOT EXISTS idx_sanctions_ingest_log_by_source
    ON sanctions_ingest_log (source, ingested_at DESC);

-- Refuse UPDATE / DELETE on the audit log. Re-use the v-engine's
-- existing audit-mutation refusal trigger pattern from 0003.
-- We define a local function (`sanctions_ingest_log_refuse_mutation`)
-- so the v-engine doesn't depend on a function name owned by the
-- declaration service's migrations (cross-database wiring is
-- separately discouraged by ADR-0003).
CREATE OR REPLACE FUNCTION sanctions_ingest_log_refuse_mutation()
    RETURNS TRIGGER AS $$
BEGIN
    RAISE EXCEPTION 'sanctions_ingest_log is COMP-2 append-only';
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS forbid_sanctions_ingest_log_update ON sanctions_ingest_log;
CREATE TRIGGER forbid_sanctions_ingest_log_update
    BEFORE UPDATE ON sanctions_ingest_log
    FOR EACH ROW EXECUTE FUNCTION sanctions_ingest_log_refuse_mutation();

DROP TRIGGER IF EXISTS forbid_sanctions_ingest_log_delete ON sanctions_ingest_log;
CREATE TRIGGER forbid_sanctions_ingest_log_delete
    BEFORE DELETE ON sanctions_ingest_log
    FOR EACH ROW EXECUTE FUNCTION sanctions_ingest_log_refuse_mutation();

DROP TRIGGER IF EXISTS forbid_sanctions_ingest_log_truncate ON sanctions_ingest_log;
CREATE TRIGGER forbid_sanctions_ingest_log_truncate
    BEFORE TRUNCATE ON sanctions_ingest_log
    FOR EACH STATEMENT EXECUTE FUNCTION sanctions_ingest_log_refuse_mutation();

REVOKE UPDATE, DELETE, TRUNCATE ON sanctions_ingest_log FROM PUBLIC;

COMMIT;
