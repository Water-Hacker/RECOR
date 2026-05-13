//! Postgres-backed `PepAdapter` implementation. Mirrors
//! `PostgresSanctionsAdapter` but queries the `peps` table.

use async_trait::async_trait;
use sqlx::PgPool;
use tracing::instrument;

use crate::application::port::{AdapterError, PepAdapter, PepMatch, PersonQuery};
use crate::infrastructure::name_match::{MatchTier, canonicalise};

pub struct PostgresPepAdapter {
    pool: PgPool,
}

impl PostgresPepAdapter {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn has_rows(&self) -> Result<bool, sqlx::Error> {
        let row = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM peps")
            .fetch_one(&self.pool)
            .await?;
        Ok(row > 0)
    }
}

#[async_trait]
impl PepAdapter for PostgresPepAdapter {
    #[instrument(skip(self), fields(person_id = %query.person_id))]
    async fn screen(
        &self,
        query: &PersonQuery,
        max_candidates: usize,
    ) -> Result<Vec<PepMatch>, AdapterError> {
        let canonical = canonicalise(&query.full_name);
        let limit = max_candidates as i64;
        let rows = sqlx::query!(
            r#"
            SELECT
                id                                                  AS "id!: uuid::Uuid",
                source                                              AS "source!",
                full_name_canonical                                 AS "full_name_canonical!",
                position                                            AS "position?",
                country                                             AS "country?",
                is_current                                          AS "is_current!",
                relationship_kind                                   AS "relationship_kind!",
                similarity(full_name_canonical, $1)::float8         AS "similarity!"
            FROM peps
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
        let out = rows
            .into_iter()
            .filter_map(|r| {
                MatchTier::from_similarity(r.similarity).map(|tier| PepMatch {
                    list_entry_id: r.id,
                    source: r.source,
                    canonical_full_name: r.full_name_canonical,
                    position: r.position,
                    country: r.country,
                    is_current: r.is_current,
                    relationship_kind: r.relationship_kind,
                    similarity: r.similarity,
                    tier: tier.as_str().to_string(),
                })
            })
            .collect();
        Ok(out)
    }

    async fn index_rows(&self) -> Result<i64, AdapterError> {
        let n = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM peps")
            .fetch_one(&self.pool)
            .await?;
        Ok(n)
    }
}
