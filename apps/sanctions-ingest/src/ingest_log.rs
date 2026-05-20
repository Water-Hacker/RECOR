//! TODO-014 — Per-feed ingest audit log.
//!
//! Every ingestion run — successful or blocked — writes one row to
//! `sanctions_ingest_log` (migration shipped alongside as
//! `0008_sanctions_ingest_log.sql` on the v-engine side; the ingest
//! worker is the only writer). A row records the source name, source
//! revision (the upstream feed's version string when published), the
//! BLAKE3 digest of the raw fetched bytes, the row count before /
//! after, the outcome, and — when applicable — the operator's
//! `--force` justification.
//!
//! Operators can answer "what did the OFAC SDN list look like on
//! 2026-04-12" by querying this table for the latest row that
//! preceded that date and inspecting the recorded digest + revision.

use serde::Serialize;
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct IngestLogEntry {
    pub source: String,
    pub source_revision: String,
    pub raw_bytes_digest_hex: String,
    pub prior_row_count: u64,
    pub proposed_row_count: u64,
    pub applied: bool,
    pub force_justification: Option<String>,
    pub ingested_at: OffsetDateTime,
}

/// Write a single ingest_log row. Idempotent on `(source, source_revision)`
/// via the unique index in the migration.
pub async fn write_ingest_log(
    pool: &PgPool,
    entry: &IngestLogEntry,
) -> Result<Uuid, sqlx::Error> {
    let id = Uuid::now_v7();
    sqlx::query(
        r#"
        INSERT INTO sanctions_ingest_log (
            ingest_id, source, source_revision, raw_bytes_digest_hex,
            prior_row_count, proposed_row_count, applied,
            force_justification, ingested_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        ON CONFLICT (source, source_revision) DO NOTHING
        "#,
    )
    .bind(id)
    .bind(&entry.source)
    .bind(&entry.source_revision)
    .bind(&entry.raw_bytes_digest_hex)
    .bind(entry.prior_row_count as i64)
    .bind(entry.proposed_row_count as i64)
    .bind(entry.applied)
    .bind(entry.force_justification.as_deref())
    .bind(entry.ingested_at)
    .execute(pool)
    .await?;
    Ok(id)
}
