//! PostgreSQL adapter for VerificationCase persistence.

use async_trait::async_trait;
use sqlx::PgPool;
use tracing::instrument;
use uuid::Uuid;

use crate::application::port::{RepositoryError, VerificationRepository};
use crate::domain::{DecisionRationale, VerificationCase, VerificationCaseId};

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

    /// Run the bundled migrations. The production-safe set is always
    /// applied. Dev-only seed migrations (under `./migrations_dev/`) are
    /// applied only when `RECOR_DEV_MIGRATIONS=true` and the
    /// `mock-bunec` cargo feature is enabled at compile time.
    pub async fn run_migrations(&self) -> Result<(), sqlx::migrate::MigrateError> {
        sqlx::migrate!("./migrations").run(&self.pool).await?;
        self.run_dev_migrations_if_enabled().await?;
        Ok(())
    }

    #[cfg(feature = "mock-bunec")]
    async fn run_dev_migrations_if_enabled(&self) -> Result<(), sqlx::migrate::MigrateError> {
        let opt_in = std::env::var("RECOR_DEV_MIGRATIONS")
            .ok()
            .as_deref()
            == Some("true");
        if !opt_in {
            tracing::info!(
                "mock-bunec feature compiled in but RECOR_DEV_MIGRATIONS!=true; skipping dev seed migrations"
            );
            return Ok(());
        }
        tracing::warn!(
            "RECOR_DEV_MIGRATIONS=true — applying dev-only seed migrations (mock_bunec_persons fixtures). \
             Production deploys MUST NOT set this env var."
        );
        sqlx::migrate!("./migrations_dev").run(&self.pool).await
    }

    #[cfg(not(feature = "mock-bunec"))]
    async fn run_dev_migrations_if_enabled(&self) -> Result<(), sqlx::migrate::MigrateError> {
        Ok(())
    }
}

#[async_trait]
impl VerificationRepository for PostgresVerificationRepository {
    #[instrument(skip(self, case, rationale), fields(case_id = %case.case_id, lane = case.lane.as_str()))]
    async fn save_case(
        &self,
        case: &VerificationCase,
        rationale: Option<&DecisionRationale>,
    ) -> Result<(), RepositoryError> {
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

        // TODO-050 — propagate the originating Submit's correlation_id
        // through the writeback envelope. The declaration service's
        // inbound handler refuses any envelope whose correlation_id
        // disagrees with the originating Submitted event's id
        // (D14 fail-closed against forged or mis-routed writebacks).
        let writeback_payload = serde_json::json!({
            "case_id": case.case_id.0,
            "declaration_id": case.declaration.declaration_id,
            "correlation_id": case.declaration.correlation_id,
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

        // TODO-049 — persist the per-case `DecisionRationale` in the
        // same transaction. The COMP-2 immutability triggers
        // (migration 0009) refuse UPDATE/DELETE/TRUNCATE post-write;
        // ON CONFLICT DO NOTHING keeps idempotent replays of the same
        // case_id from breaking the immutability contract.
        //
        // Runtime-checked `sqlx::query()` here so this change does
        // not require regenerating the `.sqlx/` cache. The follow-up
        // `R-VER-SQLX-CACHE` ticket flips this to `sqlx::query!` once
        // the cache regeneration step is wired into CI.
        if let Some(r) = rationale {
            let rationale_json = serde_json::to_value(r)?;
            sqlx::query(
                r#"
                INSERT INTO decision_rationales (
                    case_id, declaration_id, rationale_payload, composed_at
                )
                VALUES ($1, $2, $3, $4)
                ON CONFLICT (case_id) DO NOTHING
                "#,
            )
            .bind(case.case_id.0)
            .bind(case.declaration.declaration_id)
            .bind(rationale_json)
            .bind(r.composed_at)
            .execute(&mut *tx)
            .await?;
        }

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

    #[instrument(skip(self), fields(case_id = %id))]
    async fn load_rationale(
        &self,
        id: VerificationCaseId,
    ) -> Result<Option<DecisionRationale>, RepositoryError> {
        // TODO-049 / R-VER-SQLX-CACHE: runtime-checked for the same
        // reason as the insert above.
        let row_opt: Option<(serde_json::Value,)> = sqlx::query_as(
            r#"SELECT rationale_payload FROM decision_rationales WHERE case_id = $1"#,
        )
        .bind(id.0)
        .fetch_optional(&self.pool)
        .await?;
        match row_opt {
            None => Ok(None),
            Some((payload,)) => Ok(Some(serde_json::from_value(payload)?)),
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

    #[instrument(skip(self, beneficial_owners), fields(declaration_id = %declaration_id))]
    async fn upsert_declaration_projection(
        &self,
        declaration_id: Uuid,
        entity_id: Uuid,
        declarant_principal: &str,
        submitted_at: time::OffsetDateTime,
        effective_from: time::Date,
        beneficial_owners: serde_json::Value,
        entity_jurisdiction: Option<&str>,
    ) -> Result<(), RepositoryError> {
        // TODO-061 closure: writeback subscriber side. Stage 6 pattern
        // queries (src/application/stages/stage6_patterns.rs) read
        // `declaration_projection`; until this method ran the table
        // was empty, so every signature returned vacuous results and
        // the Stage-6 BBA degenerated to the prior.
        //
        // Idempotency: ON CONFLICT DO UPDATE so re-running the use
        // case against the same declaration_id refreshes the row
        // rather than failing.
        sqlx::query(
            r#"
            INSERT INTO declaration_projection (
                declaration_id, entity_id, declarant_principal,
                submitted_at, effective_from, beneficial_owners,
                entity_jurisdiction, has_bunec_activity
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, TRUE)
            ON CONFLICT (declaration_id) DO UPDATE SET
                entity_id           = EXCLUDED.entity_id,
                declarant_principal = EXCLUDED.declarant_principal,
                submitted_at        = EXCLUDED.submitted_at,
                effective_from      = EXCLUDED.effective_from,
                beneficial_owners   = EXCLUDED.beneficial_owners,
                entity_jurisdiction = EXCLUDED.entity_jurisdiction
            "#,
        )
        .bind(declaration_id)
        .bind(entity_id)
        .bind(declarant_principal)
        .bind(submitted_at)
        .bind(effective_from)
        .bind(beneficial_owners)
        .bind(entity_jurisdiction)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
