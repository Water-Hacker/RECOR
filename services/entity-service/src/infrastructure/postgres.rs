//! PostgreSQL adapter implementing `EntityRepository`.
//!
//! All write paths run a single transaction that:
//!   1. appends to `entity_events` (the source of truth)
//!   2. upserts the `entities` projection
//!   3. inserts a row into `outbox`
//!
//! Optimistic concurrency: the INSERT into `entity_events` carries
//! `(entity_id, aggregate_version)` UNIQUE. A version mismatch surfaces
//! as a unique-violation Postgres error and is translated to
//! `RepositoryError::Conflict`.
//!
//! Identity-tuple uniqueness: the projection's UNIQUE
//! `(jurisdiction, registration_number_in_jurisdiction)` enforces that
//! no two RÉCOR entity_ids ever share an external-register handle.
//! A violation surfaces as `RepositoryError::DuplicateIdentityTuple`.
//!
//! Queries use the compile-time-checked `sqlx::query!` /
//! `sqlx::query_as!` macros. Type-checking happens at `cargo build`
//! time against the committed `.sqlx/` offline cache (R-DECL-7 pattern).
//! Production builds set `SQLX_OFFLINE=true` so the build never reaches
//! out to a database (Doctrine 19).

use async_trait::async_trait;
use serde_json::Value as JsonValue;
use sqlx::{PgPool, Postgres, Transaction};
use tracing::instrument;

use crate::application::port::{
    EntityRepository, RepositoryError, SearchCriteria,
};
use crate::application::EntityProjection;
use crate::domain::value_object::{
    CanonicalName, EntityId, EntityType, Jurisdiction, RegistrationNumber,
};
use crate::domain::{
    EntityDissolvedV1, EntityEvent, EntityRegisteredV1, EntityUpdatedV1,
};

pub struct PostgresEntityRepository {
    pool: PgPool,
}

impl PostgresEntityRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Run the bundled migrations against the database. Idempotent;
    /// safe to call on every service startup.
    pub async fn run_migrations(&self) -> Result<(), sqlx::migrate::MigrateError> {
        sqlx::migrate!("./migrations").run(&self.pool).await
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

// TODO(R-VER-1): wire BUNEC as source-of-truth. When the BUNEC adapter
// lands, the registration path below must consult BUNEC for Cameroonian
// entities (jurisdiction == "CM") BEFORE writing the projection: BUNEC
// is authoritative, this service becomes the cache + projection.
// Non-Cameroonian entities continue down the declarant-submitted path
// verified through the verification engine.

#[async_trait]
impl EntityRepository for PostgresEntityRepository {
    #[instrument(skip(self), fields(entity_id = %id))]
    async fn load_events(&self, id: EntityId) -> Result<Vec<EntityEvent>, RepositoryError> {
        let rows = sqlx::query!(
            r#"
            SELECT event_type, event_payload
            FROM entity_events
            WHERE entity_id = $1
            ORDER BY aggregate_version ASC
            "#,
            id.0,
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|row| decode_event(&row.event_type, row.event_payload))
            .collect()
    }

    #[instrument(
        skip(self, event),
        fields(
            entity_id = %event.entity_id(),
            event_type = event.event_type(),
            expected_version = expected_version,
        )
    )]
    async fn save_event(
        &self,
        event: &EntityEvent,
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

    #[instrument(skip(self), fields(entity_id = %id))]
    async fn load_projection(
        &self,
        id: EntityId,
    ) -> Result<Option<EntityProjection>, RepositoryError> {
        let row_opt = sqlx::query!(
            r#"
            SELECT id, canonical_name, entity_type, jurisdiction,
                   registration_number_in_jurisdiction,
                   founded_at, dissolved_at, aggregate_version,
                   created_at, updated_at
            FROM entities
            WHERE id = $1
            "#,
            id.0,
        )
        .fetch_optional(&self.pool)
        .await?;

        let Some(row) = row_opt else { return Ok(None) };
        let entity_type = EntityType::from_storage_string(&row.entity_type)
            .map_err(|e| RepositoryError::InvalidStoredValue(e.to_string()))?;
        let jurisdiction = Jurisdiction::try_from_str(&row.jurisdiction)
            .map_err(|e| RepositoryError::InvalidStoredValue(e.to_string()))?;
        let registration_number =
            RegistrationNumber::try_from_str(&row.registration_number_in_jurisdiction)
                .map_err(|e| RepositoryError::InvalidStoredValue(e.to_string()))?;

        Ok(Some(EntityProjection {
            entity_id: EntityId(row.id),
            canonical_name: row.canonical_name,
            entity_type,
            jurisdiction,
            registration_number_in_jurisdiction: registration_number,
            founded_at: row.founded_at,
            dissolved_at: row.dissolved_at,
            version: u64::try_from(row.aggregate_version).unwrap_or(0),
            created_at: row.created_at,
            updated_at: row.updated_at,
        }))
    }

    #[instrument(skip(self, criteria))]
    async fn find_by_criteria(
        &self,
        criteria: &SearchCriteria,
    ) -> Result<Vec<EntityProjection>, RepositoryError> {
        // Construct a single parameterised statement that fans out the
        // optional filters. Using `COALESCE`-style guards lets us keep
        // sqlx's compile-time-checked macros (no dynamic SQL building)
        // while still supporting "filter is absent → match all" for
        // each optional dimension. The `q` filter is a substring match
        // (ILIKE) wrapped in `%...%`; for v1 cardinality the trigram
        // GIN index in 0001_init keeps this sub-millisecond.
        let limit_clamped = i64::from(criteria.limit.min(200).max(1));
        let q_like = criteria.q.as_deref().map(|s| format!("%{s}%"));
        let rows = sqlx::query!(
            r#"
            SELECT id, canonical_name, entity_type, jurisdiction,
                   registration_number_in_jurisdiction,
                   founded_at, dissolved_at, aggregate_version,
                   created_at, updated_at
            FROM entities
            WHERE ($1::text IS NULL OR canonical_name ILIKE $1)
              AND ($2::text IS NULL OR jurisdiction = $2)
              AND ($3::text IS NULL OR entity_type LIKE $3 || '%')
            ORDER BY founded_at DESC, id ASC
            LIMIT $4
            "#,
            q_like.as_deref(),
            criteria.jurisdiction.as_deref(),
            criteria.entity_type.as_deref(),
            limit_clamped,
        )
        .fetch_all(&self.pool)
        .await?;

        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let entity_type = EntityType::from_storage_string(&row.entity_type)
                .map_err(|e| RepositoryError::InvalidStoredValue(e.to_string()))?;
            let jurisdiction = Jurisdiction::try_from_str(&row.jurisdiction)
                .map_err(|e| RepositoryError::InvalidStoredValue(e.to_string()))?;
            let registration_number = RegistrationNumber::try_from_str(
                &row.registration_number_in_jurisdiction,
            )
            .map_err(|e| RepositoryError::InvalidStoredValue(e.to_string()))?;

            out.push(EntityProjection {
                entity_id: EntityId(row.id),
                canonical_name: row.canonical_name,
                entity_type,
                jurisdiction,
                registration_number_in_jurisdiction: registration_number,
                founded_at: row.founded_at,
                dissolved_at: row.dissolved_at,
                version: u64::try_from(row.aggregate_version).unwrap_or(0),
                created_at: row.created_at,
                updated_at: row.updated_at,
            });
        }
        Ok(out)
    }
}

fn decode_event(event_type: &str, payload: JsonValue) -> Result<EntityEvent, RepositoryError> {
    Ok(match event_type {
        "entity.registered.v1" => EntityEvent::Registered(serde_json::from_value(payload)?),
        "entity.updated.v1" => EntityEvent::Updated(serde_json::from_value(payload)?),
        "entity.dissolved.v1" => EntityEvent::Dissolved(serde_json::from_value(payload)?),
        other => {
            return Err(RepositoryError::InvalidStoredValue(format!(
                "unknown event_type `{other}` in entity_events"
            )));
        }
    })
}

async fn insert_event(
    tx: &mut Transaction<'_, Postgres>,
    event: &EntityEvent,
    new_version: i64,
) -> Result<(), RepositoryError> {
    let (entity_id, correlation_id, payload) = match event {
        EntityEvent::Registered(p) => (
            p.entity_id.0,
            p.correlation_id,
            serde_json::to_value(p)?,
        ),
        EntityEvent::Updated(p) => (p.entity_id.0, p.correlation_id, serde_json::to_value(p)?),
        EntityEvent::Dissolved(p) => (p.entity_id.0, p.correlation_id, serde_json::to_value(p)?),
    };
    let event_type = event.event_type();
    let result = sqlx::query!(
        r#"
        INSERT INTO entity_events
            (entity_id, aggregate_version, event_type, event_payload, correlation_id)
        VALUES ($1, $2, $3, $4, $5)
        "#,
        entity_id,
        new_version,
        event_type,
        payload,
        correlation_id,
    )
    .execute(&mut **tx)
    .await;

    match result {
        Ok(_) => Ok(()),
        Err(sqlx::Error::Database(db_err)) if db_err.code().as_deref() == Some("23505") => {
            // 23505: unique_violation — the (entity_id, aggregate_version)
            // UNIQUE is the optimistic-concurrency anchor.
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
    event: &EntityEvent,
    new_version: i64,
) -> Result<(), RepositoryError> {
    match event {
        EntityEvent::Registered(p) => upsert_registered(tx, p, new_version).await,
        EntityEvent::Updated(p) => upsert_updated(tx, p, new_version).await,
        EntityEvent::Dissolved(p) => upsert_dissolved(tx, p, new_version).await,
    }
}

async fn upsert_registered(
    tx: &mut Transaction<'_, Postgres>,
    p: &EntityRegisteredV1,
    new_version: i64,
) -> Result<(), RepositoryError> {
    let entity_type_storage = p.entity_type.as_storage_string();
    let jurisdiction = p.jurisdiction.as_str().to_string();
    let registration_number = p.registration_number_in_jurisdiction.as_str().to_string();
    let canonical_name = p.canonical_name.as_str().to_string();
    let result = sqlx::query!(
        r#"
        INSERT INTO entities (
            id, canonical_name, entity_type, jurisdiction,
            registration_number_in_jurisdiction,
            founded_at, dissolved_at, aggregate_version
        )
        VALUES ($1, $2, $3, $4, $5, $6, NULL, $7)
        "#,
        p.entity_id.0,
        canonical_name,
        entity_type_storage,
        jurisdiction,
        registration_number,
        p.founded_at,
        new_version,
    )
    .execute(&mut **tx)
    .await;

    match result {
        Ok(_) => Ok(()),
        Err(sqlx::Error::Database(db_err)) if db_err.code().as_deref() == Some("23505") => {
            // Could be the (id) PK or the (jurisdiction, registration_number) tuple.
            let constraint = db_err.constraint().unwrap_or("").to_string();
            if constraint == "entities_jurisdiction_registration_unique" {
                Err(RepositoryError::DuplicateIdentityTuple {
                    jurisdiction: jurisdiction.clone(),
                    registration_number: registration_number.clone(),
                })
            } else {
                Err(RepositoryError::Backend(sqlx::Error::Database(db_err)))
            }
        }
        Err(e) => Err(RepositoryError::Backend(e)),
    }
}

async fn upsert_updated(
    tx: &mut Transaction<'_, Postgres>,
    p: &EntityUpdatedV1,
    new_version: i64,
) -> Result<(), RepositoryError> {
    let canonical_name = p.after.canonical_name.as_str().to_string();
    let entity_type_storage = p.after.entity_type.as_storage_string();
    sqlx::query!(
        r#"
        UPDATE entities
        SET canonical_name    = $2,
            entity_type       = $3,
            aggregate_version = $4,
            updated_at        = NOW()
        WHERE id = $1
        "#,
        p.entity_id.0,
        canonical_name,
        entity_type_storage,
        new_version,
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn upsert_dissolved(
    tx: &mut Transaction<'_, Postgres>,
    p: &EntityDissolvedV1,
    new_version: i64,
) -> Result<(), RepositoryError> {
    sqlx::query!(
        r#"
        UPDATE entities
        SET dissolved_at      = $2,
            aggregate_version = $3,
            updated_at        = NOW()
        WHERE id = $1
        "#,
        p.entity_id.0,
        p.dissolved_at,
        new_version,
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn write_outbox(
    tx: &mut Transaction<'_, Postgres>,
    event: &EntityEvent,
) -> Result<(), RepositoryError> {
    let (entity_id, payload) = match event {
        EntityEvent::Registered(p) => (p.entity_id.0, serde_json::to_value(p)?),
        EntityEvent::Updated(p) => (p.entity_id.0, serde_json::to_value(p)?),
        EntityEvent::Dissolved(p) => (p.entity_id.0, serde_json::to_value(p)?),
    };
    let event_id = uuid::Uuid::now_v7();
    let event_type = event.event_type();
    let partition_key = entity_id.to_string();
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
        "entity",
        entity_id,
        partition_key,
        payload,
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

// ─── Idempotency store ───────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct IdempotencyRecord {
    pub response_status: i16,
    pub response_body: JsonValue,
    pub request_hash: String,
}

pub struct IdempotencyStore {
    pool: PgPool,
}

impl IdempotencyStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn check_existing(
        &self,
        idempotency_key: &str,
        actor_principal: &str,
    ) -> Result<Option<IdempotencyRecord>, sqlx::Error> {
        let row_opt = sqlx::query!(
            r#"
            SELECT response_status, response_body, request_hash
            FROM idempotency_records
            WHERE idempotency_key = $1
              AND actor_principal = $2
              AND expires_at > NOW()
            "#,
            idempotency_key,
            actor_principal,
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row_opt.map(|r| IdempotencyRecord {
            response_status: r.response_status,
            response_body: r.response_body,
            request_hash: r.request_hash,
        }))
    }

    pub async fn record(
        &self,
        idempotency_key: &str,
        actor_principal: &str,
        request_hash: &str,
        response_status: i16,
        response_body: &JsonValue,
        ttl_seconds: i64,
    ) -> Result<(), sqlx::Error> {
        let ttl = ttl_seconds as f64;
        sqlx::query!(
            r#"
            INSERT INTO idempotency_records
                (idempotency_key, actor_principal, request_hash,
                 response_status, response_body, expires_at)
            VALUES ($1, $2, $3, $4, $5,
                    NOW() + (CAST($6 AS DOUBLE PRECISION) * INTERVAL '1 second'))
            ON CONFLICT (idempotency_key) DO NOTHING
            "#,
            idempotency_key,
            actor_principal,
            request_hash,
            response_status,
            response_body,
            ttl,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

// Force imports during partial-build phase.
#[allow(dead_code)]
fn _force_imports(_c: CanonicalName) {}
