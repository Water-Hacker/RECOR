//! Postgres-backed mock BUNEC adapter.
//!
//! Reads from the `mock_bunec_persons` table seeded by either the
//! integration tests or `scripts/seed-bunec.sh`. In production this
//! adapter is replaced by `RealBunecAdapter` against the actual
//! national identity registry (R-VER-1).

use async_trait::async_trait;
use sqlx::PgPool;
use tracing::instrument;
use uuid::Uuid;

use crate::application::port::{BunecAdapter, BunecLookup, BunecLookupError};

pub struct PostgresMockBunec {
    pool: PgPool,
}

impl PostgresMockBunec {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Seed a record. Used by tests + the smoke script.
    pub async fn seed(
        &self,
        person_id: Uuid,
        full_name: &str,
        nationality: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"INSERT INTO mock_bunec_persons (person_id, canonical_full_name, nationality)
               VALUES ($1, $2, $3) ON CONFLICT (person_id) DO NOTHING"#,
            person_id,
            full_name,
            nationality,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

#[async_trait]
impl BunecAdapter for PostgresMockBunec {
    #[instrument(skip(self), fields(person_id = %person_id))]
    async fn lookup(&self, person_id: Uuid) -> Result<BunecLookup, BunecLookupError> {
        let row_opt = sqlx::query!(
            r#"SELECT canonical_full_name, nationality
               FROM mock_bunec_persons WHERE person_id = $1"#,
            person_id,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| BunecLookupError::Backend(e.to_string()))?;
        match row_opt {
            Some(row) => Ok(BunecLookup::Found {
                person_id,
                canonical_full_name: row.canonical_full_name,
                nationality: row.nationality,
            }),
            None => Ok(BunecLookup::NotFound { person_id }),
        }
    }
}
