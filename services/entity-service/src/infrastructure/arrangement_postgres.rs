//! PostgreSQL adapter implementing `ArrangementRepository`.
//!
//! All write paths run a single transaction that:
//!   1. appends to `arrangement_events` (the source of truth — COMP-2
//!      append-only triggers refuse UPDATE / DELETE / TRUNCATE)
//!   2. upserts the `arrangements` projection (initial INSERT on
//!      Registered; UPDATE on Updated and Dissolved)
//!   3. inserts a row into `outbox`
//!
//! Optimistic concurrency: the UNIQUE (arrangement_id, sequence_no)
//! constraint on `arrangement_events` is the OC anchor — a duplicate
//! version surfaces as Postgres 23505 and is mapped to
//! `ArrangementRepositoryError::Conflict`.
//!
//! Queries use sqlx's compile-time `query!` macros against the
//! committed `.sqlx/` offline cache (R-DECL-7 / IDENTITY-1 pattern).
//! `SQLX_OFFLINE=true` is the production build invariant.

use async_trait::async_trait;
use serde_json::Value as JsonValue;
use sqlx::{PgPool, Postgres, Transaction};
use tracing::instrument;

use crate::application::arrangement_port::{
    ArrangementProjection, ArrangementRepository, ArrangementRepositoryError,
};
use crate::domain::{
    ArrangementDissolvedV1, ArrangementEvent, ArrangementId, ArrangementKind,
    ArrangementRegisteredV1, ArrangementUpdatableFields, ArrangementUpdatedV1,
    ClassBeneficiarySpec, ControlExerciseRef, GoverningLawJurisdiction, NamedBeneficiaryRef,
    ProtectorRef, SettlorRef, TrusteeRef,
};

pub struct PostgresArrangementRepository {
    pool: PgPool,
}

impl PostgresArrangementRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

#[async_trait]
impl ArrangementRepository for PostgresArrangementRepository {
    #[instrument(skip(self), fields(arrangement_id = %id))]
    async fn load_events(
        &self,
        id: ArrangementId,
    ) -> Result<Vec<ArrangementEvent>, ArrangementRepositoryError> {
        let rows = sqlx::query!(
            r#"
            SELECT event_type, payload
            FROM arrangement_events
            WHERE arrangement_id = $1
            ORDER BY sequence_no ASC
            "#,
            id.0,
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|row| decode_event(&row.event_type, row.payload))
            .collect()
    }

    #[instrument(
        skip(self, event),
        fields(
            arrangement_id = %event.arrangement_id(),
            event_type = event.event_type(),
            expected_version = expected_version,
        )
    )]
    async fn save_event(
        &self,
        event: &ArrangementEvent,
        expected_version: u64,
    ) -> Result<(), ArrangementRepositoryError> {
        let mut tx = self.pool.begin().await?;
        let new_version =
            i64::try_from(expected_version.saturating_add(1)).unwrap_or(i64::MAX);

        insert_event(&mut tx, event, new_version).await?;
        upsert_projection(&mut tx, event, new_version).await?;
        write_outbox(&mut tx, event).await?;

        tx.commit().await?;
        Ok(())
    }

    #[instrument(skip(self), fields(arrangement_id = %id))]
    async fn load_projection(
        &self,
        id: ArrangementId,
    ) -> Result<Option<ArrangementProjection>, ArrangementRepositoryError> {
        let row_opt = sqlx::query!(
            r#"
            SELECT arrangement_id,
                   arrangement_kind,
                   governing_law_jurisdiction,
                   constitution_date,
                   dissolution_date,
                   retention_until,
                   settlor_refs,
                   trustee_refs,
                   protector_refs,
                   named_beneficiary_refs,
                   class_beneficiary_specs,
                   control_exercise_refs,
                   aggregate_version,
                   created_at,
                   updated_at
            FROM arrangements
            WHERE arrangement_id = $1
            "#,
            id.0,
        )
        .fetch_optional(&self.pool)
        .await?;

        let Some(row) = row_opt else { return Ok(None) };

        let arrangement_kind = ArrangementKind::try_from_storage_str(&row.arrangement_kind)
            .map_err(|e| ArrangementRepositoryError::InvalidStoredValue(e.to_string()))?;
        let jurisdiction =
            GoverningLawJurisdiction::try_from_str(&row.governing_law_jurisdiction)
                .map_err(|e| ArrangementRepositoryError::InvalidStoredValue(e.to_string()))?;

        let fields = ArrangementUpdatableFields {
            settlor_refs: decode_jsonb_array::<SettlorRef>(&row.settlor_refs, "settlor_refs")?,
            trustee_refs: decode_jsonb_array::<TrusteeRef>(&row.trustee_refs, "trustee_refs")?,
            protector_refs: decode_jsonb_array::<ProtectorRef>(&row.protector_refs, "protector_refs")?,
            named_beneficiary_refs: decode_jsonb_array::<NamedBeneficiaryRef>(
                &row.named_beneficiary_refs,
                "named_beneficiary_refs",
            )?,
            class_beneficiary_specs: decode_jsonb_array::<ClassBeneficiarySpec>(
                &row.class_beneficiary_specs,
                "class_beneficiary_specs",
            )?,
            control_exercise_refs: decode_jsonb_array::<ControlExerciseRef>(
                &row.control_exercise_refs,
                "control_exercise_refs",
            )?,
        };

        Ok(Some(ArrangementProjection {
            arrangement_id: ArrangementId(row.arrangement_id),
            arrangement_kind,
            governing_law_jurisdiction: jurisdiction,
            constitution_date: row.constitution_date,
            dissolution_date: row.dissolution_date,
            retention_until: row.retention_until,
            fields,
            version: u64::try_from(row.aggregate_version).unwrap_or(0),
            created_at: row.created_at,
            updated_at: row.updated_at,
        }))
    }
}

fn decode_event(
    event_type: &str,
    payload: JsonValue,
) -> Result<ArrangementEvent, ArrangementRepositoryError> {
    Ok(match event_type {
        "arrangement.registered.v1" => {
            ArrangementEvent::Registered(serde_json::from_value(payload)?)
        }
        "arrangement.updated.v1" => ArrangementEvent::Updated(serde_json::from_value(payload)?),
        "arrangement.dissolved.v1" => {
            ArrangementEvent::Dissolved(serde_json::from_value(payload)?)
        }
        other => {
            return Err(ArrangementRepositoryError::InvalidStoredValue(format!(
                "unknown event_type `{other}` in arrangement_events"
            )))
        }
    })
}

fn decode_jsonb_array<T: serde::de::DeserializeOwned>(
    raw: &JsonValue,
    field: &'static str,
) -> Result<Vec<T>, ArrangementRepositoryError> {
    serde_json::from_value::<Vec<T>>(raw.clone()).map_err(|e| {
        ArrangementRepositoryError::InvalidStoredValue(format!(
            "{field} JSONB malformed: {e}"
        ))
    })
}

async fn insert_event(
    tx: &mut Transaction<'_, Postgres>,
    event: &ArrangementEvent,
    new_version: i64,
) -> Result<(), ArrangementRepositoryError> {
    let (arrangement_id, actor_principal, occurred_at, payload) = match event {
        ArrangementEvent::Registered(p) => (
            p.arrangement_id.0,
            p.registered_by_principal.clone(),
            p.registered_at,
            serde_json::to_value(p)?,
        ),
        ArrangementEvent::Updated(p) => (
            p.arrangement_id.0,
            p.updated_by_principal.clone(),
            p.updated_at,
            serde_json::to_value(p)?,
        ),
        ArrangementEvent::Dissolved(p) => (
            p.arrangement_id.0,
            p.dissolved_by_principal.clone(),
            p.recorded_at,
            serde_json::to_value(p)?,
        ),
    };
    let event_type = event.event_type();
    let event_id = uuid::Uuid::now_v7();

    let result = sqlx::query!(
        r#"
        INSERT INTO arrangement_events
            (event_id, arrangement_id, event_type, payload, actor_principal, occurred_at, sequence_no)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#,
        event_id,
        arrangement_id,
        event_type,
        payload,
        actor_principal,
        occurred_at,
        new_version,
    )
    .execute(&mut **tx)
    .await;

    match result {
        Ok(_) => Ok(()),
        Err(sqlx::Error::Database(db_err)) if db_err.code().as_deref() == Some("23505") => {
            // Either (arrangement_id, sequence_no) UNIQUE — OC anchor —
            // or the event_id PK collided. Treat as concurrency conflict;
            // a higher layer retries with a re-loaded aggregate.
            Err(ArrangementRepositoryError::Conflict {
                expected: u64::try_from(new_version.saturating_sub(1)).unwrap_or(0),
                found: u64::try_from(new_version).unwrap_or(0),
            })
        }
        Err(e) => Err(ArrangementRepositoryError::Backend(e)),
    }
}

async fn upsert_projection(
    tx: &mut Transaction<'_, Postgres>,
    event: &ArrangementEvent,
    new_version: i64,
) -> Result<(), ArrangementRepositoryError> {
    match event {
        ArrangementEvent::Registered(p) => insert_registered(tx, p, new_version).await,
        ArrangementEvent::Updated(p) => update_projection(tx, p, new_version).await,
        ArrangementEvent::Dissolved(p) => dissolve_projection(tx, p, new_version).await,
    }
}

async fn insert_registered(
    tx: &mut Transaction<'_, Postgres>,
    p: &ArrangementRegisteredV1,
    new_version: i64,
) -> Result<(), ArrangementRepositoryError> {
    let kind = p.arrangement_kind.as_storage_str().to_string();
    let jurisdiction = p.governing_law_jurisdiction.as_str().to_string();
    let settlor = serde_json::to_value(&p.fields.settlor_refs)?;
    let trustee = serde_json::to_value(&p.fields.trustee_refs)?;
    let protector = serde_json::to_value(&p.fields.protector_refs)?;
    let named_beneficiary = serde_json::to_value(&p.fields.named_beneficiary_refs)?;
    let class_beneficiary = serde_json::to_value(&p.fields.class_beneficiary_specs)?;
    let control_exercise = serde_json::to_value(&p.fields.control_exercise_refs)?;

    sqlx::query!(
        r#"
        INSERT INTO arrangements (
            arrangement_id, arrangement_kind, governing_law_jurisdiction,
            constitution_date, dissolution_date, retention_until,
            settlor_refs, trustee_refs, protector_refs,
            named_beneficiary_refs, class_beneficiary_specs, control_exercise_refs,
            created_by_principal, aggregate_version
        )
        VALUES ($1, $2, $3, $4, NULL, NULL,
                $5, $6, $7,
                $8, $9, $10,
                $11, $12)
        "#,
        p.arrangement_id.0,
        kind,
        jurisdiction,
        p.constitution_date,
        settlor,
        trustee,
        protector,
        named_beneficiary,
        class_beneficiary,
        control_exercise,
        p.registered_by_principal,
        new_version,
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn update_projection(
    tx: &mut Transaction<'_, Postgres>,
    p: &ArrangementUpdatedV1,
    new_version: i64,
) -> Result<(), ArrangementRepositoryError> {
    let settlor = serde_json::to_value(&p.after.settlor_refs)?;
    let trustee = serde_json::to_value(&p.after.trustee_refs)?;
    let protector = serde_json::to_value(&p.after.protector_refs)?;
    let named_beneficiary = serde_json::to_value(&p.after.named_beneficiary_refs)?;
    let class_beneficiary = serde_json::to_value(&p.after.class_beneficiary_specs)?;
    let control_exercise = serde_json::to_value(&p.after.control_exercise_refs)?;
    sqlx::query!(
        r#"
        UPDATE arrangements
        SET settlor_refs             = $2,
            trustee_refs             = $3,
            protector_refs           = $4,
            named_beneficiary_refs   = $5,
            class_beneficiary_specs  = $6,
            control_exercise_refs    = $7,
            aggregate_version        = $8,
            updated_at               = NOW()
        WHERE arrangement_id = $1
        "#,
        p.arrangement_id.0,
        settlor,
        trustee,
        protector,
        named_beneficiary,
        class_beneficiary,
        control_exercise,
        new_version,
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn dissolve_projection(
    tx: &mut Transaction<'_, Postgres>,
    p: &ArrangementDissolvedV1,
    new_version: i64,
) -> Result<(), ArrangementRepositoryError> {
    sqlx::query!(
        r#"
        UPDATE arrangements
        SET dissolution_date    = $2,
            retention_until     = $3,
            aggregate_version   = $4,
            updated_at          = NOW()
        WHERE arrangement_id = $1
        "#,
        p.arrangement_id.0,
        p.dissolution_date,
        p.retention_until,
        new_version,
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn write_outbox(
    tx: &mut Transaction<'_, Postgres>,
    event: &ArrangementEvent,
) -> Result<(), ArrangementRepositoryError> {
    let (arrangement_id, payload) = match event {
        ArrangementEvent::Registered(p) => (p.arrangement_id.0, serde_json::to_value(p)?),
        ArrangementEvent::Updated(p) => (p.arrangement_id.0, serde_json::to_value(p)?),
        ArrangementEvent::Dissolved(p) => (p.arrangement_id.0, serde_json::to_value(p)?),
    };
    let event_id = uuid::Uuid::now_v7();
    let event_type = event.event_type();
    let partition_key = arrangement_id.to_string();
    sqlx::query!(
        r#"
        INSERT INTO outbox (
            event_id, event_type, event_version, aggregate_type, aggregate_id,
            partition_key, payload
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#,
        event_id,
        event_type,
        1_i32,
        "arrangement",
        arrangement_id,
        partition_key,
        payload,
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}
