//! PostgreSQL adapter for VerificationCase persistence.

use async_trait::async_trait;
use sqlx::{PgPool, Row};
use tracing::instrument;
use uuid::Uuid;

use crate::application::port::{RepositoryError, VerificationRepository};
use crate::domain::{VerificationCase, VerificationCaseId};

pub struct PostgresVerificationRepository {
    pool: PgPool,
}

impl PostgresVerificationRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn run_migrations(&self) -> Result<(), sqlx::migrate::MigrateError> {
        sqlx::migrate!("./migrations").run(&self.pool).await
    }
}

#[async_trait]
impl VerificationRepository for PostgresVerificationRepository {
    #[instrument(skip(self, case), fields(case_id = %case.case_id, lane = case.lane.as_str()))]
    async fn save_case(&self, case: &VerificationCase) -> Result<(), RepositoryError> {
        let mut tx = self.pool.begin().await?;
        let payload = serde_json::to_value(case)?;
        let authenticity_belief = case.fused_authenticity.belief_true();
        let authenticity_plausibility = case.fused_authenticity.plausibility_true();
        let risk_belief = case.fused_risk.belief_true();

        sqlx::query(
            r#"
            INSERT INTO verification_cases (
                case_id, declaration_id, entity_id, declarant_principal,
                lane, authenticity_belief, authenticity_plausibility,
                risk_belief, case_payload, created_at, completed_at,
                total_duration_ms
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            ON CONFLICT (case_id) DO NOTHING
            "#,
        )
        .bind(case.case_id.0)
        .bind(case.declaration.declaration_id)
        .bind(case.declaration.entity_id)
        .bind(&case.declaration.declarant_principal)
        .bind(case.lane.as_str())
        .bind(authenticity_belief)
        .bind(authenticity_plausibility)
        .bind(risk_belief)
        .bind(&payload)
        .bind(case.created_at)
        .bind(case.completed_at)
        .bind(case.total_duration_ms as i64)
        .execute(&mut *tx)
        .await?;

        // Outbox row for future Kafka relay.
        sqlx::query(
            r#"
            INSERT INTO verification_outbox (
                event_id, event_type, event_version, aggregate_id, partition_key, payload
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (event_id) DO NOTHING
            "#,
        )
        .bind(Uuid::now_v7())
        .bind("verification.completed.v1")
        .bind(1_i32)
        .bind(case.case_id.0)
        .bind(case.case_id.0.to_string())
        .bind(&payload)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }

    #[instrument(skip(self), fields(case_id = %id))]
    async fn load_case(
        &self,
        id: VerificationCaseId,
    ) -> Result<Option<VerificationCase>, RepositoryError> {
        let row_opt = sqlx::query(
            r#"SELECT case_payload FROM verification_cases WHERE case_id = $1"#,
        )
        .bind(id.0)
        .fetch_optional(&self.pool)
        .await?;
        match row_opt {
            None => Ok(None),
            Some(row) => {
                let payload: serde_json::Value = row.try_get("case_payload")?;
                Ok(Some(serde_json::from_value(payload)?))
            }
        }
    }

    #[instrument(skip(self), fields(declaration_id = %declaration_id))]
    async fn case_for_declaration(
        &self,
        declaration_id: Uuid,
    ) -> Result<Option<VerificationCaseId>, RepositoryError> {
        let row_opt = sqlx::query(
            r#"SELECT case_id FROM verification_cases WHERE declaration_id = $1"#,
        )
        .bind(declaration_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row_opt.map(|r| {
            let id: Uuid = r.get("case_id");
            VerificationCaseId(id)
        }))
    }
}
