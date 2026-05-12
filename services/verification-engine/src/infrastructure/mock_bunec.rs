//! Postgres-backed mock BUNEC adapter.
//!
//! Reads from the `mock_bunec_persons` table seeded by either the
//! integration tests or `scripts/seed-bunec.sh`. In production this
//! adapter is replaced by `RealBunecAdapter` against the actual
//! national identity registry (R-VER-1).

use async_trait::async_trait;
use sqlx::{PgPool, Row};
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
        sqlx::query(
            r#"INSERT INTO mock_bunec_persons (person_id, canonical_full_name, nationality)
               VALUES ($1, $2, $3) ON CONFLICT (person_id) DO NOTHING"#,
        )
        .bind(person_id)
        .bind(full_name)
        .bind(nationality)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

#[async_trait]
impl BunecAdapter for PostgresMockBunec {
    #[instrument(skip(self), fields(person_id = %person_id))]
    async fn lookup(&self, person_id: Uuid) -> Result<BunecLookup, BunecLookupError> {
        let row_opt = sqlx::query(
            r#"SELECT canonical_full_name, nationality
               FROM mock_bunec_persons WHERE person_id = $1"#,
        )
        .bind(person_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| BunecLookupError::Backend(e.to_string()))?;
        match row_opt {
            Some(row) => Ok(BunecLookup::Found {
                person_id,
                canonical_full_name: row
                    .try_get::<String, _>("canonical_full_name")
                    .map_err(|e| BunecLookupError::Backend(e.to_string()))?,
                nationality: row
                    .try_get::<String, _>("nationality")
                    .map_err(|e| BunecLookupError::Backend(e.to_string()))?,
            }),
            None => Ok(BunecLookup::NotFound { person_id }),
        }
    }
}
