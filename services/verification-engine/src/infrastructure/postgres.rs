//! PostgreSQL adapter for VerificationCase persistence.

use async_trait::async_trait;
use sqlx::PgPool;
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

        let lane = case.lane.as_str();
        let total_duration_ms = case.total_duration_ms as i64;
        sqlx::query!(
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
            case.case_id.0,
            case.declaration.declaration_id,
            case.declaration.entity_id,
            case.declaration.declarant_principal,
            lane,
            authenticity_belief,
            authenticity_plausibility,
            risk_belief,
            payload,
            case.created_at,
            case.completed_at,
            total_duration_ms,
        )
        .execute(&mut *tx)
        .await?;

        // Outbox row — slim writeback contract consumed by the Declaration
        // service's POST /v1/internal/verification-outcomes endpoint.
        // The full case stays in `verification_cases.case_payload`; the
        // outbox event carries only what the Declaration aggregate needs
        // to transition `verification_state`. Keeping the payload tight
        // makes the cross-service contract explicit and stable.
        let writeback_payload = serde_json::json!({
            "case_id": case.case_id.0,
            "declaration_id": case.declaration.declaration_id,
            "lane": case.lane.as_str(),
            "fused_authenticity_belief": authenticity_belief,
            "fused_authenticity_plausibility": authenticity_plausibility,
            "fused_risk_belief": risk_belief,
            "completed_at": case
                .completed_at
                .format(&time::format_description::well_known::Rfc3339)
                .expect("OffsetDateTime formats to RFC3339"),
        });

        let event_id = Uuid::now_v7();
        let partition_key = case.declaration.declaration_id.to_string();
        sqlx::query!(
            r#"
            INSERT INTO verification_outbox (
                event_id, event_type, event_version, aggregate_id, partition_key, payload
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (event_id) DO NOTHING
            "#,
            event_id,
            "verification.completed.v1",
            1_i32,
            case.declaration.declaration_id,
            partition_key,
            writeback_payload,
        )
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
        let row_opt = sqlx::query!(
            r#"SELECT case_payload FROM verification_cases WHERE case_id = $1"#,
            id.0,
        )
        .fetch_optional(&self.pool)
        .await?;
        match row_opt {
            None => Ok(None),
            Some(row) => Ok(Some(serde_json::from_value(row.case_payload)?)),
        }
    }

    #[instrument(skip(self), fields(declaration_id = %declaration_id))]
    async fn case_for_declaration(
        &self,
        declaration_id: Uuid,
    ) -> Result<Option<VerificationCaseId>, RepositoryError> {
        let row_opt = sqlx::query!(
            r#"SELECT case_id FROM verification_cases WHERE declaration_id = $1"#,
            declaration_id,
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row_opt.map(|r| VerificationCaseId(r.case_id)))
    }
}
