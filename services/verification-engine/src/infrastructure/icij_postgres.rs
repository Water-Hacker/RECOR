//! Postgres-backed `IcijAdapter` implementation. Queries the
//! `icij_persons` table populated by `bin/icij_ingest.rs`.

use async_trait::async_trait;
use sqlx::PgPool;
use tracing::instrument;

use crate::application::port::{AdapterError, IcijAdapter, IcijCandidate, PersonQuery};
use crate::infrastructure::name_match::{MatchTier, canonicalise};

pub struct PostgresIcijRepository {
    pool: PgPool,
}

impl PostgresIcijRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn has_rows(&self) -> Result<bool, sqlx::Error> {
        let n = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM icij_persons")
            .fetch_one(&self.pool)
            .await?;
        Ok(n > 0)
    }
}

#[async_trait]
impl IcijAdapter for PostgresIcijRepository {
    #[instrument(skip(self), fields(person_id = %query.person_id))]
    async fn retrieve(
        &self,
        query: &PersonQuery,
        max_candidates: usize,
    ) -> Result<Vec<IcijCandidate>, AdapterError> {
        let canonical = canonicalise(&query.full_name);
        let limit = max_candidates as i64;
        let rows = sqlx::query!(
            r#"
            SELECT
                id                                                  AS "id!: uuid::Uuid",
                node_kind                                           AS "node_kind!",
                source_dataset                                      AS "source_dataset!",
                full_name_canonical                                 AS "full_name_canonical!",
                country_raw                                         AS "country_raw?",
                snippet                                             AS "snippet?",
                similarity(full_name_canonical, $1)::float8         AS "similarity!"
            FROM icij_persons
            WHERE full_name_canonical % $1
              AND similarity(full_name_canonical, $1) >= 0.5
              AND node_kind IN ('person', 'officer')
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
                MatchTier::from_similarity(r.similarity).map(|tier| IcijCandidate {
                    id: r.id,
                    node_kind: r.node_kind,
                    source_dataset: r.source_dataset,
                    canonical_full_name: r.full_name_canonical,
                    country_raw: r.country_raw,
                    snippet: r.snippet,
                    similarity: r.similarity,
                    tier: tier.as_str().to_string(),
                })
            })
            .collect();
        Ok(out)
    }
}
