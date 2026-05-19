//! PostgreSQL adapter implementing `PersonRepository`.
//!
//! Mirrors `services/declaration/src/infrastructure/postgres.rs` in shape.
//! All write paths run a single transaction that:
//!   1. appends to `person_events` (the source of truth)
//!   2. upserts the `persons` projection
//!   3. inserts a row into `outbox`
//!
//! Optimistic concurrency: the INSERT into `person_events` carries
//! `(person_id, aggregate_version)` UNIQUE. A version mismatch surfaces
//! as a unique-violation Postgres error translated to
//! `RepositoryError::Conflict`.
//!
//! Queries use runtime-checked `sqlx::query` / `sqlx::query_as` — this
//! is the same posture `services/declaration` shipped at the start of
//! PI-1 (before R-DECL-7 moved that crate to the compile-time `query!`
//! macros). The follow-up to flip this crate to the offline `.sqlx/`
//! cache is `R-PERSON-SQLX-CACHE`; it's a mechanical refactor once the
//! schema is stable. The runtime path is correct (errors at first query
//! rather than at compile time), but the offline cache pattern is
//! preferred for the production deploy under Doctrine 19 (reproducible
//! everything).

use async_trait::async_trait;
use serde_json::Value as JsonValue;
use sqlx::{PgPool, Postgres, Row, Transaction};
use time::OffsetDateTime;
use tracing::instrument;
use uuid::Uuid;

use crate::application::port::{PersonRepository, RepositoryError};
use crate::application::PersonProjection;
use crate::domain::value_object::{PersonAttributes, PersonId};
use crate::domain::{
    PersonEvent, PersonMergedV1, PersonRegisteredV1, PersonUpdatedV1,
};

pub struct PostgresPersonRepository {
    pool: PgPool,
}

impl PostgresPersonRepository {
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
impl PersonRepository for PostgresPersonRepository {
    #[instrument(skip(self), fields(person_id = %id))]
    async fn load_events(
        &self,
        id: PersonId,
    ) -> Result<Vec<PersonEvent>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT event_type, event_payload \
             FROM person_events \
             WHERE person_id = $1 \
             ORDER BY aggregate_version ASC",
        )
        .bind(id.0)
        .fetch_all(&self.pool)
        .await?;

        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let event_type: String = row.try_get("event_type")?;
            let payload: JsonValue = row.try_get("event_payload")?;
            out.push(decode_event(&event_type, payload)?);
        }
        Ok(out)
    }

    #[instrument(skip(self, event), fields(
        person_id = %event.person_id(),
        event_type = event.event_type(),
        expected_version,
    ))]
    async fn save_event(
        &self,
        event: &PersonEvent,
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

    #[instrument(skip(self, event), fields(
        person_id = %event.person_id(),
        event_type = event.event_type(),
    ))]
    async fn save_merge(
        &self,
        event: &PersonEvent,
        expected_version: u64,
    ) -> Result<(), RepositoryError> {
        // Same transactional shape as save_event for the v1 skeleton.
        // A future ticket may add a cross-aggregate referential update
        // here (e.g. propagating the merge pointer into the surviving
        // record's audit history) but v1 keeps the pointer one-way.
        self.save_event(event, expected_version).await
    }

    #[instrument(skip(self), fields(person_id = %id))]
    async fn load_projection(
        &self,
        id: PersonId,
    ) -> Result<Option<PersonProjection>, RepositoryError> {
        let row_opt = sqlx::query(
            "SELECT person_id, canonical_full_name, nationality, date_of_birth, \
                    primary_id_document, biometric_reference_hash, \
                    aggregate_version, created_at, updated_at, merged_into, \
                    created_by_principal \
             FROM persons \
             WHERE person_id = $1",
        )
        .bind(id.0)
        .fetch_optional(&self.pool)
        .await?;

        let Some(row) = row_opt else {
            return Ok(None);
        };

        let canonical_full_name: String = row.try_get("canonical_full_name")?;
        let nationality: String = row.try_get("nationality")?;
        let date_of_birth: Option<time::Date> = row.try_get("date_of_birth")?;
        let primary_id_document: JsonValue = row.try_get("primary_id_document")?;
        let biometric_reference_hash: Option<String> =
            row.try_get("biometric_reference_hash")?;
        let aggregate_version: i64 = row.try_get("aggregate_version")?;
        let created_at: OffsetDateTime = row.try_get("created_at")?;
        let updated_at: OffsetDateTime = row.try_get("updated_at")?;
        let merged_into: Option<Uuid> = row.try_get("merged_into")?;
        let created_by_principal: String = row.try_get("created_by_principal")?;
        // TODO(NDI-1): Cameroonian national ID integration; requires
        // gov agreement. Once the NDI API ships, this is the row-shape
        // we cross-reference against the issuer's authoritative
        // record. For v1 the column carries declarant-supplied data.

        let person_id: Uuid = row.try_get("person_id")?;
        let attributes = decode_attributes(
            canonical_full_name,
            nationality,
            date_of_birth,
            primary_id_document,
            biometric_reference_hash,
        )?;

        Ok(Some(PersonProjection {
            person_id: PersonId(person_id),
            attributes,
            aggregate_version: u64::try_from(aggregate_version).unwrap_or(0),
            created_at,
            updated_at,
            merged_into: merged_into.map(PersonId),
            created_by_principal,
        }))
    }

    #[instrument(skip(self, query), fields(
        query_len = query.len(),
        nationality = ?nationality_filter,
        created_by_scope = if created_by_filter.is_some() { "self" } else { "admin" },
    ))]
    async fn search(
        &self,
        query: &str,
        nationality_filter: Option<&str>,
        created_by_filter: Option<&str>,
        limit: i64,
    ) -> Result<Vec<PersonProjection>, RepositoryError> {
        // v1: ILIKE on canonical_full_name + optional exact-match nationality.
        // TODO(R-PERSON-FUZZY): upgrade to pg_trgm trigram similarity
        // (CREATE EXTENSION pg_trgm + a GIN index) so "Ngono" matches
        // "N'gono" with a soundex-style fall-back. Once we ship that,
        // the WHERE clause becomes `canonical_full_name % $1 OR
        // canonical_full_name ILIKE $2` with the similarity score
        // returned for ORDER BY.
        //
        // FIND-005 RBAC scope: `created_by_filter` adds an extra
        // `AND created_by_principal = $N` predicate when the caller
        // is non-admin. Admin callers pass `None` and see every row
        // matching the textual filters. The combinations are
        // (nationality, created_by) ∈ {(Some, Some), (Some, None),
        // (None, Some), (None, None)} — four parameter-shape variants
        // so sqlx binds the right number of $N placeholders.
        let like_pattern = format!("%{}%", query);
        let rows = match (nationality_filter, created_by_filter) {
            (Some(nat), Some(actor)) => sqlx::query(
                "SELECT person_id, canonical_full_name, nationality, date_of_birth, \
                        primary_id_document, biometric_reference_hash, \
                        aggregate_version, created_at, updated_at, merged_into, \
                        created_by_principal \
                 FROM persons \
                 WHERE canonical_full_name ILIKE $1 AND nationality = $2 \
                   AND created_by_principal = $3 \
                   AND merged_into IS NULL \
                 ORDER BY canonical_full_name ASC \
                 LIMIT $4",
            )
            .bind(&like_pattern)
            .bind(nat)
            .bind(actor)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?,
            (Some(nat), None) => sqlx::query(
                "SELECT person_id, canonical_full_name, nationality, date_of_birth, \
                        primary_id_document, biometric_reference_hash, \
                        aggregate_version, created_at, updated_at, merged_into, \
                        created_by_principal \
                 FROM persons \
                 WHERE canonical_full_name ILIKE $1 AND nationality = $2 \
                   AND merged_into IS NULL \
                 ORDER BY canonical_full_name ASC \
                 LIMIT $3",
            )
            .bind(&like_pattern)
            .bind(nat)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?,
            (None, Some(actor)) => sqlx::query(
                "SELECT person_id, canonical_full_name, nationality, date_of_birth, \
                        primary_id_document, biometric_reference_hash, \
                        aggregate_version, created_at, updated_at, merged_into, \
                        created_by_principal \
                 FROM persons \
                 WHERE canonical_full_name ILIKE $1 \
                   AND created_by_principal = $2 \
                   AND merged_into IS NULL \
                 ORDER BY canonical_full_name ASC \
                 LIMIT $3",
            )
            .bind(&like_pattern)
            .bind(actor)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?,
            (None, None) => sqlx::query(
                "SELECT person_id, canonical_full_name, nationality, date_of_birth, \
                        primary_id_document, biometric_reference_hash, \
                        aggregate_version, created_at, updated_at, merged_into, \
                        created_by_principal \
                 FROM persons \
                 WHERE canonical_full_name ILIKE $1 \
                   AND merged_into IS NULL \
                 ORDER BY canonical_full_name ASC \
                 LIMIT $2",
            )
            .bind(&like_pattern)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?,
        };

        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let canonical_full_name: String = row.try_get("canonical_full_name")?;
            let nationality: String = row.try_get("nationality")?;
            let date_of_birth: Option<time::Date> = row.try_get("date_of_birth")?;
            let primary_id_document: JsonValue = row.try_get("primary_id_document")?;
            let biometric_reference_hash: Option<String> =
                row.try_get("biometric_reference_hash")?;
            let aggregate_version: i64 = row.try_get("aggregate_version")?;
            let created_at: OffsetDateTime = row.try_get("created_at")?;
            let updated_at: OffsetDateTime = row.try_get("updated_at")?;
            let merged_into: Option<Uuid> = row.try_get("merged_into")?;
            let person_id: Uuid = row.try_get("person_id")?;
            let created_by_principal: String = row.try_get("created_by_principal")?;
            let attributes = decode_attributes(
                canonical_full_name,
                nationality,
                date_of_birth,
                primary_id_document,
                biometric_reference_hash,
            )?;
            out.push(PersonProjection {
                person_id: PersonId(person_id),
                attributes,
                aggregate_version: u64::try_from(aggregate_version).unwrap_or(0),
                created_at,
                updated_at,
                merged_into: merged_into.map(PersonId),
                created_by_principal,
            });
        }
        Ok(out)
    }
}

fn decode_attributes(
    canonical_full_name: String,
    nationality: String,
    date_of_birth: Option<time::Date>,
    primary_id_document: JsonValue,
    biometric_reference_hash: Option<String>,
) -> Result<PersonAttributes, RepositoryError> {
    use crate::domain::value_object::{CanonicalFullName, IdDocument, Nationality};
    let canonical_full_name = CanonicalFullName::try_new(canonical_full_name).map_err(
        |e| RepositoryError::Backend(sqlx::Error::Decode(e.to_string().into())),
    )?;
    let nationality = Nationality::try_new(nationality).map_err(|e| {
        RepositoryError::Backend(sqlx::Error::Decode(e.to_string().into()))
    })?;
    let primary_id_document: IdDocument = serde_json::from_value(primary_id_document)?;
    Ok(PersonAttributes {
        canonical_full_name,
        nationality,
        date_of_birth,
        primary_id_document,
        biometric_reference_hash,
    })
}

fn decode_event(
    event_type: &str,
    payload: JsonValue,
) -> Result<PersonEvent, RepositoryError> {
    match event_type {
        "person.registered.v1" => {
            let v1: PersonRegisteredV1 = serde_json::from_value(payload)?;
            Ok(PersonEvent::Registered(v1))
        }
        "person.updated.v1" => {
            let v1: PersonUpdatedV1 = serde_json::from_value(payload)?;
            Ok(PersonEvent::Updated(v1))
        }
        "person.merged.v1" => {
            let v1: PersonMergedV1 = serde_json::from_value(payload)?;
            Ok(PersonEvent::Merged(v1))
        }
        other => Err(RepositoryError::Backend(sqlx::Error::Decode(
            format!("unknown event_type: {other}").into(),
        ))),
    }
}

async fn insert_event(
    tx: &mut Transaction<'_, Postgres>,
    event: &PersonEvent,
    new_version: i64,
) -> Result<(), RepositoryError> {
    let (person_id, correlation_id, payload) = match event {
        PersonEvent::Registered(p) => {
            (p.person_id.0, p.correlation_id, serde_json::to_value(p)?)
        }
        PersonEvent::Updated(p) => {
            (p.person_id.0, p.correlation_id, serde_json::to_value(p)?)
        }
        PersonEvent::Merged(p) => {
            (p.person_id.0, p.correlation_id, serde_json::to_value(p)?)
        }
    };
    let event_type = event.event_type();
    let result = sqlx::query(
        "INSERT INTO person_events \
             (person_id, aggregate_version, event_type, event_payload, correlation_id, causation_id) \
         VALUES ($1, $2, $3, $4, $5, NULL)",
    )
    .bind(person_id)
    .bind(new_version)
    .bind(event_type)
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
    event: &PersonEvent,
    new_version: i64,
) -> Result<(), RepositoryError> {
    match event {
        PersonEvent::Registered(p) => {
            let primary_id_doc = serde_json::to_value(&p.attributes.primary_id_document)?;
            // FIND-005/006: persist the creating principal on INSERT.
            // The column is immutable on subsequent UPDATE paths — the
            // Updated/Merged branches below do NOT touch
            // created_by_principal, mirroring the aggregate's invariant
            // that creation attribution survives every state transition.
            sqlx::query(
                "INSERT INTO persons ( \
                     person_id, canonical_full_name, nationality, date_of_birth, \
                     primary_id_document, biometric_reference_hash, \
                     created_by_principal, aggregate_version \
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
                 ON CONFLICT (person_id) DO UPDATE SET \
                     canonical_full_name      = EXCLUDED.canonical_full_name, \
                     nationality              = EXCLUDED.nationality, \
                     date_of_birth            = EXCLUDED.date_of_birth, \
                     primary_id_document      = EXCLUDED.primary_id_document, \
                     biometric_reference_hash = EXCLUDED.biometric_reference_hash, \
                     aggregate_version        = EXCLUDED.aggregate_version, \
                     updated_at               = NOW()",
            )
            .bind(p.person_id.0)
            .bind(p.attributes.canonical_full_name.as_str())
            .bind(p.attributes.nationality.as_str())
            .bind(p.attributes.date_of_birth)
            .bind(primary_id_doc)
            .bind(p.attributes.biometric_reference_hash.as_deref())
            .bind(p.actor_principal.as_str())
            .bind(new_version)
            .execute(&mut **tx)
            .await?;
        }
        PersonEvent::Updated(p) => {
            let primary_id_doc = serde_json::to_value(&p.after.primary_id_document)?;
            sqlx::query(
                "UPDATE persons SET \
                     canonical_full_name      = $2, \
                     nationality              = $3, \
                     date_of_birth            = $4, \
                     primary_id_document      = $5, \
                     biometric_reference_hash = $6, \
                     aggregate_version        = $7, \
                     updated_at               = NOW() \
                 WHERE person_id = $1",
            )
            .bind(p.person_id.0)
            .bind(p.after.canonical_full_name.as_str())
            .bind(p.after.nationality.as_str())
            .bind(p.after.date_of_birth)
            .bind(primary_id_doc)
            .bind(p.after.biometric_reference_hash.as_deref())
            .bind(new_version)
            .execute(&mut **tx)
            .await?;
        }
        PersonEvent::Merged(p) => {
            sqlx::query(
                "UPDATE persons SET \
                     merged_into       = $2, \
                     aggregate_version = $3, \
                     updated_at        = NOW() \
                 WHERE person_id = $1",
            )
            .bind(p.person_id.0)
            .bind(p.into_person_id.0)
            .bind(new_version)
            .execute(&mut **tx)
            .await?;
        }
    }
    Ok(())
}

async fn write_outbox(
    tx: &mut Transaction<'_, Postgres>,
    event: &PersonEvent,
) -> Result<(), RepositoryError> {
    let (person_id, payload) = match event {
        PersonEvent::Registered(p) => (p.person_id.0, serde_json::to_value(p)?),
        PersonEvent::Updated(p) => (p.person_id.0, serde_json::to_value(p)?),
        PersonEvent::Merged(p) => (p.person_id.0, serde_json::to_value(p)?),
    };
    let event_id = Uuid::now_v7();
    let event_type = event.event_type();
    let partition_key = person_id.to_string();
    sqlx::query(
        "INSERT INTO outbox ( \
             event_id, event_type, event_version, aggregate_type, aggregate_id, \
             partition_key, payload \
         ) VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(event_id)
    .bind(event_type)
    .bind(1_i32)
    .bind("person")
    .bind(person_id)
    .bind(partition_key)
    .bind(payload)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

// ─── Idempotency store ─────────────────────────────────────────────────

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
        let row_opt = sqlx::query(
            "SELECT response_status, response_body, request_hash \
             FROM idempotency_records \
             WHERE idempotency_key = $1 \
               AND actor_principal = $2 \
               AND expires_at > NOW()",
        )
        .bind(idempotency_key)
        .bind(actor_principal)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row_opt.map(|row| IdempotencyRecord {
            response_status: row.get::<i16, _>("response_status"),
            response_body: row.get::<JsonValue, _>("response_body"),
            request_hash: row.get::<String, _>("request_hash"),
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
        let ttl_seconds_f64 = ttl_seconds as f64;
        sqlx::query(
            "INSERT INTO idempotency_records \
                 (idempotency_key, actor_principal, request_hash, \
                  response_status, response_body, expires_at) \
             VALUES ($1, $2, $3, $4, $5, \
                     NOW() + (CAST($6 AS DOUBLE PRECISION) * INTERVAL '1 second')) \
             ON CONFLICT (idempotency_key) DO NOTHING",
        )
        .bind(idempotency_key)
        .bind(actor_principal)
        .bind(request_hash)
        .bind(response_status)
        .bind(response_body)
        .bind(ttl_seconds_f64)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
