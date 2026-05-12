//! PostgreSQL adapter implementing `DeclarationRepository`.
//!
//! All write paths run a single transaction that:
//!   1. appends to `declaration_events` (the source of truth)
//!   2. upserts the `declarations` projection
//!   3. inserts a row into `outbox`
//!
//! Optimistic concurrency: the INSERT into `declaration_events` carries
//! `(declaration_id, aggregate_version)` UNIQUE. A version mismatch
//! surfaces as a unique-violation Postgres error and is translated to
//! `RepositoryError::Conflict`.
//!
//! Note: queries use the runtime-checked `sqlx::query*` family (not the
//! compile-time-checked `sqlx::query!` macros) so the crate builds
//! without a live database at compile time. The migration schema in
//! `/migrations` is the source of truth; query type mismatches surface
//! at first call rather than at compile time. A future ticket may add
//! the `.sqlx/` cache for compile-time checking once the production
//! deployment pipeline is established.

use async_trait::async_trait;
use serde_json::Value as JsonValue;
use sqlx::{PgPool, Postgres, Row, Transaction};
use tracing::instrument;

use crate::application::port::{DeclarationRepository, RepositoryError};
use crate::application::DeclarationProjection;
use crate::domain::attestation::CryptographicAttestation;
use crate::domain::value_object::{
    BeneficialOwnerClaim, DeclarantRole, DeclarationId, DeclarationKind, DeclarationState,
    EntityId, VerificationLane,
};
use crate::domain::{DeclarationEvent, DeclarationSubmittedV1, DeclarationVerifiedV1};

pub struct PostgresDeclarationRepository {
    pool: PgPool,
}

impl PostgresDeclarationRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Run the bundled migrations against the database. Idempotent;
    /// safe to call on every service startup.
    pub async fn run_migrations(&self) -> Result<(), sqlx::migrate::MigrateError> {
        sqlx::migrate!("./migrations").run(&self.pool).await
    }
}

#[async_trait]
impl DeclarationRepository for PostgresDeclarationRepository {
    #[instrument(skip(self), fields(declaration_id = %id))]
    async fn load_events(
        &self,
        id: DeclarationId,
    ) -> Result<Vec<DeclarationEvent>, RepositoryError> {
        let rows = sqlx::query(
            r#"
            SELECT event_type, event_payload
            FROM declaration_events
            WHERE declaration_id = $1
            ORDER BY aggregate_version ASC
            "#,
        )
        .bind(id.0)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|row| {
                let event_type: String = row.try_get("event_type")?;
                let payload: JsonValue = row.try_get("event_payload")?;
                decode_event(&event_type, payload)
            })
            .collect()
    }

    #[instrument(skip(self, event), fields(
        declaration_id = %event.declaration_id(),
        event_type = event.event_type(),
        expected_version = expected_version,
    ))]
    async fn save_event(
        &self,
        event: &DeclarationEvent,
        expected_version: u64,
    ) -> Result<(), RepositoryError> {
        let mut tx = self.pool.begin().await?;
        let new_version =
            i64::try_from(expected_version.saturating_add(1)).unwrap_or(i64::MAX);

        insert_event(&mut tx, event, new_version).await?;
        upsert_projection(&mut tx, event, new_version).await?;
        write_outbox(&mut tx, event).await?;

        tx.commit().await?;
        Ok(())
    }

    #[instrument(skip(self), fields(declaration_id = %id))]
    async fn load_projection(
        &self,
        id: DeclarationId,
    ) -> Result<Option<DeclarationProjection>, RepositoryError> {
        let row_opt = sqlx::query(
            r#"
            SELECT declaration_id, entity_id, declarant_principal, declarant_role,
                   declaration_kind, effective_from, beneficial_owners, attestation,
                   state, aggregate_version, submitted_at, receipt_hash_hex,
                   correlation_id, verification_state, verification_lane,
                   verification_case_id, verified_at
            FROM declarations
            WHERE declaration_id = $1
            "#,
        )
        .bind(id.0)
        .fetch_optional(&self.pool)
        .await?;

        let Some(row) = row_opt else {
            return Ok(None);
        };

        let declaration_id: uuid::Uuid = row.try_get("declaration_id")?;
        let entity_id: uuid::Uuid = row.try_get("entity_id")?;
        let declarant_principal: String = row.try_get("declarant_principal")?;
        let declarant_role: String = row.try_get("declarant_role")?;
        let declaration_kind: String = row.try_get("declaration_kind")?;
        let effective_from: time::Date = row.try_get("effective_from")?;
        let beneficial_owners: JsonValue = row.try_get("beneficial_owners")?;
        let attestation: JsonValue = row.try_get("attestation")?;
        let state: String = row.try_get("state")?;
        let aggregate_version: i64 = row.try_get("aggregate_version")?;
        let submitted_at: time::OffsetDateTime = row.try_get("submitted_at")?;
        let receipt_hash_hex: String = row.try_get("receipt_hash_hex")?;
        let correlation_id: uuid::Uuid = row.try_get("correlation_id")?;
        let verification_state: String = row.try_get("verification_state")?;
        let verification_lane: Option<String> = row.try_get("verification_lane")?;
        let verification_case_id: Option<uuid::Uuid> = row.try_get("verification_case_id")?;
        let verified_at: Option<time::OffsetDateTime> = row.try_get("verified_at")?;

        let verification_lane = match verification_lane.as_deref() {
            None => None,
            Some(v) => Some(parse_lane(v)?),
        };

        Ok(Some(DeclarationProjection {
            declaration_id: DeclarationId(declaration_id),
            entity_id: EntityId(entity_id),
            declarant_principal,
            declarant_role: parse_declarant_role(&declarant_role)?,
            kind: parse_kind(&declaration_kind)?,
            effective_from,
            beneficial_owners: serde_json::from_value(beneficial_owners)?,
            attestation: serde_json::from_value::<CryptographicAttestation>(attestation)?,
            state: parse_state(&state)?,
            version: u64::try_from(aggregate_version).unwrap_or(0),
            submitted_at,
            receipt_hash_hex,
            correlation_id,
            verification_state,
            verification_lane,
            verification_case_id,
            verified_at,
        }))
    }
}

fn parse_lane(s: &str) -> Result<VerificationLane, RepositoryError> {
    match s {
        "green" => Ok(VerificationLane::Green),
        "yellow" => Ok(VerificationLane::Yellow),
        "red" => Ok(VerificationLane::Red),
        other => Err(RepositoryError::Backend(sqlx::Error::Decode(
            format!("unknown verification_lane: {other}").into(),
        ))),
    }
}

fn parse_declarant_role(s: &str) -> Result<DeclarantRole, RepositoryError> {
    match s {
        "self" => Ok(DeclarantRole::SelfDeclaration),
        "authorised_agent" => Ok(DeclarantRole::AuthorisedAgent),
        "operator_assisted" => Ok(DeclarantRole::OperatorAssisted),
        other => Err(RepositoryError::Backend(sqlx::Error::Decode(
            format!("unknown declarant_role: {other}").into(),
        ))),
    }
}

fn parse_kind(s: &str) -> Result<DeclarationKind, RepositoryError> {
    match s {
        "incorporation" => Ok(DeclarationKind::Incorporation),
        "annual_renewal" => Ok(DeclarationKind::AnnualRenewal),
        "change_of_control" => Ok(DeclarationKind::ChangeOfControl),
        "correction" => Ok(DeclarationKind::Correction),
        "amendment" => Ok(DeclarationKind::Amendment),
        other => Err(RepositoryError::Backend(sqlx::Error::Decode(
            format!("unknown declaration_kind: {other}").into(),
        ))),
    }
}

fn parse_state(s: &str) -> Result<DeclarationState, RepositoryError> {
    match s {
        "draft" => Ok(DeclarationState::Draft),
        "submitted" => Ok(DeclarationState::Submitted),
        "in_verification" => Ok(DeclarationState::InVerification),
        "accepted" => Ok(DeclarationState::Accepted),
        "rejected" => Ok(DeclarationState::Rejected),
        "superseded" => Ok(DeclarationState::Superseded),
        other => Err(RepositoryError::Backend(sqlx::Error::Decode(
            format!("unknown declaration state: {other}").into(),
        ))),
    }
}

fn decode_event(
    event_type: &str,
    payload: JsonValue,
) -> Result<DeclarationEvent, RepositoryError> {
    match event_type {
        "declaration.submitted.v1" => {
            let v1: DeclarationSubmittedV1 = serde_json::from_value(payload)?;
            Ok(DeclarationEvent::Submitted(v1))
        }
        "declaration.verified.v1" => {
            let v1: DeclarationVerifiedV1 = serde_json::from_value(payload)?;
            Ok(DeclarationEvent::Verified(v1))
        }
        other => Err(RepositoryError::Backend(sqlx::Error::Decode(
            format!("unknown event_type: {other}").into(),
        ))),
    }
}

async fn insert_event(
    tx: &mut Transaction<'_, Postgres>,
    event: &DeclarationEvent,
    new_version: i64,
) -> Result<(), RepositoryError> {
    let (declaration_id, correlation_id, payload) = match event {
        DeclarationEvent::Submitted(p) => {
            (p.declaration_id.0, p.correlation_id, serde_json::to_value(p)?)
        }
        DeclarationEvent::Verified(p) => {
            // No declarant-supplied correlation_id on a verified event;
            // tracing context still flows via the request span. Stamp
            // the case_id so the event row carries its provenance.
            (p.declaration_id.0, p.verification_case_id, serde_json::to_value(p)?)
        }
    };
    let result = sqlx::query(
        r#"
        INSERT INTO declaration_events
            (declaration_id, aggregate_version, event_type, event_payload, correlation_id, causation_id)
        VALUES ($1, $2, $3, $4, $5, NULL)
        "#,
    )
    .bind(declaration_id)
    .bind(new_version)
    .bind(event.event_type())
    .bind(payload)
    .bind(correlation_id)
    .execute(&mut **tx)
    .await;

    match result {
        Ok(_) => Ok(()),
        Err(sqlx::Error::Database(db_err)) if db_err.is_unique_violation() => {
            Err(RepositoryError::Conflict {
                expected: u64::try_from(new_version - 1).unwrap_or(0),
                found: u64::try_from(new_version).unwrap_or(0),
            })
        }
        Err(e) => Err(RepositoryError::Backend(e)),
    }
}

async fn upsert_projection(
    tx: &mut Transaction<'_, Postgres>,
    event: &DeclarationEvent,
    new_version: i64,
) -> Result<(), RepositoryError> {
    match event {
        DeclarationEvent::Submitted(p) => {
            let owners = serde_json::to_value(&p.beneficial_owners)?;
            let attestation = serde_json::to_value(&p.attestation)?;
            sqlx::query(
                r#"
                INSERT INTO declarations (
                    declaration_id, entity_id, declarant_principal, declarant_role,
                    declaration_kind, effective_from, beneficial_owners, attestation,
                    state, aggregate_version, submitted_at, receipt_hash_hex, correlation_id,
                    verification_state
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
                ON CONFLICT (declaration_id) DO UPDATE SET
                    state             = EXCLUDED.state,
                    aggregate_version = EXCLUDED.aggregate_version,
                    updated_at        = NOW()
                "#,
            )
            .bind(p.declaration_id.0)
            .bind(p.entity_id.0)
            .bind(&p.declarant_principal)
            .bind(p.declarant_role.as_str())
            .bind(p.kind.as_str())
            .bind(p.effective_from)
            .bind(owners)
            .bind(attestation)
            .bind("submitted")
            .bind(new_version)
            .bind(p.submitted_at)
            .bind(&p.receipt_hash_hex)
            .bind(p.correlation_id)
            .bind("pending")
            .execute(&mut **tx)
            .await?;
        }
        DeclarationEvent::Verified(p) => {
            // Projection-only update; the declarations row already exists
            // from the prior Submitted event. The aggregate's `state`
            // column AND the `verification_state` mirror are written in
            // the same statement so reads see a consistent snapshot.
            let state_str = p.lane.to_declaration_state().as_str();
            let verification_state_str = p.lane.as_verification_state_str();
            sqlx::query(
                r#"
                UPDATE declarations
                SET state               = $2,
                    aggregate_version   = $3,
                    verification_state  = $4,
                    verification_lane   = $5,
                    verification_case_id = $6,
                    verified_at         = $7,
                    updated_at          = NOW()
                WHERE declaration_id = $1
                "#,
            )
            .bind(p.declaration_id.0)
            .bind(state_str)
            .bind(new_version)
            .bind(verification_state_str)
            .bind(p.lane.as_str())
            .bind(p.verification_case_id)
            .bind(p.completed_at)
            .execute(&mut **tx)
            .await?;
        }
    }
    Ok(())
}

async fn write_outbox(
    tx: &mut Transaction<'_, Postgres>,
    event: &DeclarationEvent,
) -> Result<(), RepositoryError> {
    let (declaration_id, payload) = match event {
        DeclarationEvent::Submitted(p) => (p.declaration_id.0, serde_json::to_value(p)?),
        DeclarationEvent::Verified(p) => (p.declaration_id.0, serde_json::to_value(p)?),
    };
    let event_id = uuid::Uuid::now_v7();
    sqlx::query(
        r#"
        INSERT INTO outbox (
            event_id, event_type, event_version, aggregate_type, aggregate_id,
            partition_key, payload
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#,
    )
    .bind(event_id)
    .bind(event.event_type())
    .bind(1_i32)
    .bind("declaration")
    .bind(declaration_id)
    .bind(declaration_id.to_string())
    .bind(payload)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

// Helper for the API layer / idempotency middleware.
pub struct IdempotencyStore {
    pool: PgPool,
}

#[derive(Debug, Clone)]
pub struct IdempotencyRecord {
    pub response_status: i16,
    pub response_body: JsonValue,
    pub request_hash: String,
}

impl IdempotencyStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Expose the underlying pool to the readiness probe.
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Look up an existing idempotency record without mutating state.
    pub async fn check_existing(
        &self,
        idempotency_key: &str,
        declarant_principal: &str,
    ) -> Result<Option<IdempotencyRecord>, sqlx::Error> {
        let row_opt = sqlx::query(
            r#"
            SELECT response_status, response_body, request_hash
            FROM idempotency_records
            WHERE idempotency_key = $1
              AND declarant_principal = $2
              AND expires_at > NOW()
            "#,
        )
        .bind(idempotency_key)
        .bind(declarant_principal)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row_opt.map(|row| IdempotencyRecord {
            response_status: row.get("response_status"),
            response_body: row.get("response_body"),
            request_hash: row.get("request_hash"),
        }))
    }

    /// Record a fresh idempotency entry.
    pub async fn record(
        &self,
        idempotency_key: &str,
        declarant_principal: &str,
        request_hash: &str,
        response_status: i16,
        response_body: &JsonValue,
        ttl_seconds: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO idempotency_records
                (idempotency_key, declarant_principal, request_hash,
                 response_status, response_body, expires_at)
            VALUES ($1, $2, $3, $4, $5,
                    NOW() + (CAST($6 AS DOUBLE PRECISION) * INTERVAL '1 second'))
            ON CONFLICT (idempotency_key) DO NOTHING
            "#,
        )
        .bind(idempotency_key)
        .bind(declarant_principal)
        .bind(request_hash)
        .bind(response_status)
        .bind(response_body)
        .bind(ttl_seconds as f64)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    #[error("invalid stored data: {0}")]
    InvalidData(String),
}

// Suppress unused-import warning during the partial-build phase.
#[allow(dead_code)]
fn _force_imports(_o: BeneficialOwnerClaim) {}
