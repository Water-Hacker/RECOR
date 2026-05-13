//! Postgres-backed `SanctionsAdapter` implementation. Queries the
//! `sanctions_persons` table using pg_trgm trigram similarity over
//! `full_name_canonical`. Implementation notes:
//!
//!   * pg_trgm is loaded in migration `0005_sanctions.sql`
//!     (renumbered from `0004_sanctions.sql` to avoid the collision
//!     with R-LOOP-2's `0004_add_kafka_consumer_dlq.sql`).
//!   * The GIN index on `full_name_canonical gin_trgm_ops` makes the
//!     `%` operator (default 0.3 threshold) fast for large indexes.
//!   * Application-layer filter raises the floor to 0.5; below that
//!     we treat the candidate as noise.
//!   * The shared `name_match::canonicalise` runs over the input so
//!     the query and stored value compare apples-to-apples.

use async_trait::async_trait;
use sqlx::PgPool;
use tracing::instrument;

use crate::application::port::{AdapterError, PersonQuery, SanctionMatch, SanctionsAdapter};
use crate::infrastructure::name_match::{MatchTier, canonicalise};

pub struct PostgresSanctionsAdapter {
    pool: PgPool,
}

impl PostgresSanctionsAdapter {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// True when the `sanctions_persons` table has at least one row.
    /// Used by the startup-time stage-selector to pick `real` vs `stub`.
    pub async fn has_rows(&self) -> Result<bool, sqlx::Error> {
        let row = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sanctions_persons")
            .fetch_one(&self.pool)
            .await?;
        Ok(row > 0)
    }
}

#[async_trait]
impl SanctionsAdapter for PostgresSanctionsAdapter {
    #[instrument(skip(self), fields(person_id = %query.person_id))]
    async fn screen(
        &self,
        query: &PersonQuery,
        max_candidates: usize,
    ) -> Result<Vec<SanctionMatch>, AdapterError> {
        let canonical = canonicalise(&query.full_name);
        // pg_trgm similarity threshold floor is 0.5; the GIN-backed `%`
        // operator pre-filters, then we re-rank by exact similarity.
        let limit = max_candidates as i64;
        let rows = sqlx::query!(
            r#"
            SELECT
                id                                                  AS "id!: uuid::Uuid",
                source                                              AS "source!",
                full_name_canonical                                 AS "full_name_canonical!",
                sanction_program                                    AS "sanction_program!",
                similarity(full_name_canonical, $1)::float8         AS "similarity!"
            FROM sanctions_persons
            WHERE full_name_canonical % $1
              AND similarity(full_name_canonical, $1) >= 0.5
            ORDER BY similarity(full_name_canonical, $1) DESC
            LIMIT $2
            "#,
            canonical,
            limit,
        )
        .fetch_all(&self.pool)
        .await?;

        let candidates: Vec<SanctionMatch> = rows
            .into_iter()
            .filter_map(|r| {
                MatchTier::from_similarity(r.similarity).map(|tier| SanctionMatch {
                    list_entry_id: r.id,
                    source: r.source,
                    canonical_full_name: r.full_name_canonical,
                    sanction_program: r.sanction_program,
                    similarity: r.similarity,
                    tier: tier.as_str().to_string(),
                })
            })
            .collect();
        Ok(candidates)
    }

    async fn index_rows(&self) -> Result<i64, AdapterError> {
        let n = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sanctions_persons")
            .fetch_one(&self.pool)
            .await?;
        Ok(n)
    }
}
