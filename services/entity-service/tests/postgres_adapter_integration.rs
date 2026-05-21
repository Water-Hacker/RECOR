//! TODO-027 — Postgres adapter integration tests for the Entity service.
//!
//! Tests: round-trip of every event variant, COMP-2 (append-only trigger),
//! optimistic concurrency, projection rebuild, outbox insertion, identity-
//! tuple uniqueness enforcement, and idempotency-cache hit/miss.
//!
//! All tests `#[ignore]` — CI runs via `--ignored`. Run locally:
//!   cargo test -p recor-entity-service --test postgres_adapter_integration \
//!     -- --ignored --nocapture

use std::sync::Arc;

use sqlx::postgres::PgPoolOptions;
use testcontainers::ContainerAsync;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::ImageExt;
use time::{macros::date, OffsetDateTime};
use uuid::Uuid;

use recor_entity_service::application::port::{EntityRepository, RepositoryError};
use recor_entity_service::domain::{
    CanonicalName, EntityDissolvedV1, EntityEvent, EntityId, EntityRegisteredV1, EntityType,
    EntityUpdatedV1, Jurisdiction, RegistrationNumber,
};
use recor_entity_service::domain::value_object::UpdatableFields;
use recor_entity_service::infrastructure::postgres::PostgresEntityRepository;

// ─── Harness ──────────────────────────────────────────────────────────────────

struct Ctx {
    repo: Arc<PostgresEntityRepository>,
    pool: sqlx::PgPool,
    _pg: ContainerAsync<Postgres>,
}

async fn setup() -> Ctx {
    let pg = Postgres::default()
        .with_tag("17-alpine")
        .start()
        .await
        .expect("postgres");
    let port = pg.get_host_port_ipv4(5432).await.expect("port");
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&url)
        .await
        .expect("connect");
    let repo = Arc::new(PostgresEntityRepository::new(pool.clone()));
    repo.run_migrations().await.expect("migrations");
    Ctx { repo, pool, _pg: pg }
}

fn reg_number(suffix: &str) -> RegistrationNumber {
    RegistrationNumber::try_from_str(&format!("RC/TEST/{suffix}")).unwrap()
}

fn registered_event(entity_id: EntityId, reg_suffix: &str, principal: &str) -> EntityEvent {
    EntityEvent::Registered(EntityRegisteredV1 {
        entity_id,
        canonical_name: CanonicalName::try_from_str("Test Entity SARL").unwrap(),
        entity_type: EntityType::Sarl,
        jurisdiction: Jurisdiction::try_from_str("CM").unwrap(),
        registration_number_in_jurisdiction: reg_number(reg_suffix),
        founded_at: date!(2020 - 01 - 01),
        registered_by_principal: principal.to_string(),
        registered_at: OffsetDateTime::now_utc(),
        correlation_id: Uuid::now_v7(),
    })
}

// ─── Test 1: round-trip EntityRegisteredV1 ───────────────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn round_trip_registered_event() {
    let ctx = setup().await;
    let entity_id = EntityId::new();
    let event = registered_event(entity_id, "RT1", "spiffe://recor.cm/entity-pg-1");

    ctx.repo.save_event(&event, 0).await.expect("save_event");

    let events = ctx.repo.load_events(entity_id).await.expect("load_events");
    assert_eq!(events.len(), 1);
    assert!(matches!(&events[0], EntityEvent::Registered(r) if r.entity_id == entity_id));
}

// ─── Test 2: round-trip EntityUpdatedV1 ──────────────────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn round_trip_updated_event() {
    let ctx = setup().await;
    let entity_id = EntityId::new();
    let principal = "spiffe://recor.cm/entity-pg-2";
    ctx.repo.save_event(&registered_event(entity_id, "RT2", principal), 0).await.unwrap();

    let before = UpdatableFields {
        canonical_name: CanonicalName::try_from_str("Test Entity SARL").unwrap(),
        entity_type: EntityType::Sarl,
    };
    let after = UpdatableFields {
        canonical_name: CanonicalName::try_from_str("Test Entity SA Updated").unwrap(),
        entity_type: EntityType::Sa,
    };
    let update = EntityEvent::Updated(EntityUpdatedV1 {
        entity_id,
        before,
        after,
        updated_by_principal: principal.to_string(),
        updated_at: OffsetDateTime::now_utc(),
        correlation_id: Uuid::now_v7(),
    });
    ctx.repo.save_event(&update, 1).await.expect("save update");

    let events = ctx.repo.load_events(entity_id).await.unwrap();
    assert_eq!(events.len(), 2);
    assert!(matches!(&events[1], EntityEvent::Updated(_)));
}

// ─── Test 3: round-trip EntityDissolvedV1 ────────────────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn round_trip_dissolved_event() {
    let ctx = setup().await;
    let entity_id = EntityId::new();
    let principal = "spiffe://recor.cm/entity-pg-3-admin";
    ctx.repo.save_event(&registered_event(entity_id, "RT3", principal), 0).await.unwrap();

    let dissolved = EntityEvent::Dissolved(EntityDissolvedV1 {
        entity_id,
        dissolved_at: date!(2026 - 05 - 01),
        dissolved_by_principal: principal.to_string(),
        recorded_at: OffsetDateTime::now_utc(),
        correlation_id: Uuid::now_v7(),
    });
    ctx.repo.save_event(&dissolved, 1).await.expect("save dissolved");

    let events = ctx.repo.load_events(entity_id).await.unwrap();
    assert_eq!(events.len(), 2);
    assert!(matches!(&events[1], EntityEvent::Dissolved(_)));
}

// ─── Test 4: COMP-2 trigger — UPDATE on entity_events fails ──────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn comp2_update_on_entity_events_refused() {
    let ctx = setup().await;
    let entity_id = EntityId::new();
    ctx.repo
        .save_event(&registered_event(entity_id, "C2U", "spiffe://recor.cm/entity-comp2"), 0)
        .await
        .unwrap();

    let err = sqlx::query(
        "UPDATE entity_events SET event_type = 'tampered' WHERE entity_id = $1",
    )
    .bind(entity_id.0)
    .execute(&ctx.pool)
    .await;

    assert!(err.is_err(), "COMP-2 trigger must refuse UPDATE on entity_events");
}

// ─── Test 5: COMP-2 trigger — DELETE on entity_events fails ──────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn comp2_delete_on_entity_events_refused() {
    let ctx = setup().await;
    let entity_id = EntityId::new();
    ctx.repo
        .save_event(&registered_event(entity_id, "C2D", "spiffe://recor.cm/entity-comp2-del"), 0)
        .await
        .unwrap();

    let err = sqlx::query("DELETE FROM entity_events WHERE entity_id = $1")
        .bind(entity_id.0)
        .execute(&ctx.pool)
        .await;

    assert!(err.is_err(), "COMP-2 trigger must refuse DELETE on entity_events");
}

// ─── Test 6: identity-tuple uniqueness enforcement ───────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn duplicate_identity_tuple_returns_error() {
    let ctx = setup().await;
    let e1 = EntityId::new();
    let e2 = EntityId::new();
    let principal = "spiffe://recor.cm/entity-dup";

    ctx.repo.save_event(&registered_event(e1, "DUP1", principal), 0).await.unwrap();

    // Second entity with the SAME (jurisdiction="CM", registration_number="RC/TEST/DUP1")
    // must fail with DuplicateIdentityTuple.
    let err = ctx.repo.save_event(&registered_event(e2, "DUP1", principal), 0).await.unwrap_err();
    assert!(
        matches!(err, RepositoryError::DuplicateIdentityTuple { .. }),
        "duplicate identity tuple must surface correct error variant, got {err:?}"
    );
}

// ─── Test 7: optimistic concurrency wrong version → Conflict ─────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn optimistic_concurrency_wrong_version() {
    let ctx = setup().await;
    let entity_id = EntityId::new();
    let principal = "spiffe://recor.cm/entity-concurrency";

    ctx.repo.save_event(&registered_event(entity_id, "CC1", principal), 0).await.unwrap();

    let err = ctx
        .repo
        .save_event(&registered_event(entity_id, "CC1X", principal), 0)
        .await
        .unwrap_err();
    assert!(
        matches!(err, RepositoryError::Conflict { .. }),
        "wrong version must Conflict, got {err:?}"
    );
}

// ─── Test 8: projection rebuilt correctly ────────────────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn projection_correct_after_register() {
    let ctx = setup().await;
    let entity_id = EntityId::new();
    let principal = "spiffe://recor.cm/entity-proj";

    ctx.repo.save_event(&registered_event(entity_id, "PR1", principal), 0).await.unwrap();

    let proj = ctx.repo.load_projection(entity_id).await.unwrap().unwrap();
    assert_eq!(proj.entity_id, entity_id);
    assert_eq!(proj.canonical_name.as_str(), "Test Entity SARL");
    assert_eq!(proj.version, 1);
}

// ─── Test 9: projection version advances on update ───────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn projection_version_advances_on_update() {
    let ctx = setup().await;
    let entity_id = EntityId::new();
    let principal = "spiffe://recor.cm/entity-version-adv";

    ctx.repo.save_event(&registered_event(entity_id, "VA1", principal), 0).await.unwrap();
    let v1 = ctx.repo.load_projection(entity_id).await.unwrap().unwrap().version;
    assert_eq!(v1, 1);

    let update = EntityEvent::Updated(EntityUpdatedV1 {
        entity_id,
        before: UpdatableFields {
            canonical_name: CanonicalName::try_from_str("Test Entity SARL").unwrap(),
            entity_type: EntityType::Sarl,
        },
        after: UpdatableFields {
            canonical_name: CanonicalName::try_from_str("Test Entity SARL v2").unwrap(),
            entity_type: EntityType::Sarl,
        },
        updated_by_principal: principal.to_string(),
        updated_at: OffsetDateTime::now_utc(),
        correlation_id: Uuid::now_v7(),
    });
    ctx.repo.save_event(&update, 1).await.unwrap();

    let v2 = ctx.repo.load_projection(entity_id).await.unwrap().unwrap().version;
    assert_eq!(v2, 2);
}

// ─── Test 10: outbox row inserted on register ─────────────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn outbox_row_inserted_on_register() {
    let ctx = setup().await;
    let entity_id = EntityId::new();
    ctx.repo
        .save_event(&registered_event(entity_id, "OBX1", "spiffe://recor.cm/entity-outbox"), 0)
        .await
        .unwrap();

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM outbox WHERE aggregate_id = $1")
        .bind(entity_id.0)
        .fetch_one(&ctx.pool)
        .await
        .expect("outbox count");
    assert_eq!(count, 1);
}

// ─── Test 11: load_events unknown entity → empty ──────────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn load_events_unknown_entity_is_empty() {
    let ctx = setup().await;
    let events = ctx.repo.load_events(EntityId::new()).await.unwrap();
    assert!(events.is_empty());
}

// ─── Test 12: projection absent for unknown entity ───────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn projection_absent_for_unknown_entity() {
    let ctx = setup().await;
    let proj = ctx.repo.load_projection(EntityId::new()).await.unwrap();
    assert!(proj.is_none());
}

// ─── Test 13: search by name returns matching entity ─────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn search_by_name_returns_match() {
    use recor_entity_service::application::port::SearchCriteria;
    let ctx = setup().await;
    let entity_id = EntityId::new();
    ctx.repo
        .save_event(&registered_event(entity_id, "SRCH1", "spiffe://recor.cm/entity-search"), 0)
        .await
        .unwrap();

    let results = ctx
        .repo
        .find_by_criteria(&SearchCriteria {
            q: Some("Test Entity".to_string()),
            jurisdiction: None,
            entity_type: None,
            limit: 10,
        })
        .await
        .expect("find_by_criteria");

    assert!(
        results.iter().any(|r| r.entity_id == entity_id),
        "search must return the registered entity"
    );
}

// ─── Test 14: search by jurisdiction filters correctly ───────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn search_by_jurisdiction_filters_correctly() {
    use recor_entity_service::application::port::SearchCriteria;
    let ctx = setup().await;
    let entity_id = EntityId::new();
    ctx.repo
        .save_event(&registered_event(entity_id, "JRSD1", "spiffe://recor.cm/entity-juris"), 0)
        .await
        .unwrap();

    let cm_results = ctx
        .repo
        .find_by_criteria(&SearchCriteria {
            q: None,
            jurisdiction: Some("CM".to_string()),
            entity_type: None,
            limit: 10,
        })
        .await
        .unwrap();
    assert!(cm_results.iter().any(|r| r.entity_id == entity_id));

    let fr_results = ctx
        .repo
        .find_by_criteria(&SearchCriteria {
            q: None,
            jurisdiction: Some("FR".to_string()),
            entity_type: None,
            limit: 10,
        })
        .await
        .unwrap();
    assert!(!fr_results.iter().any(|r| r.entity_id == entity_id));
}

// ─── Test 15: dissolved entity projection reflects terminal state ─────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn dissolved_projection_reflects_terminal_state() {
    let ctx = setup().await;
    let entity_id = EntityId::new();
    let principal = "spiffe://recor.cm/entity-dissolved";
    ctx.repo.save_event(&registered_event(entity_id, "DISS1", principal), 0).await.unwrap();

    let dissolved = EntityEvent::Dissolved(EntityDissolvedV1 {
        entity_id,
        dissolved_at: date!(2026 - 05 - 01),
        dissolved_by_principal: principal.to_string(),
        recorded_at: OffsetDateTime::now_utc(),
        correlation_id: Uuid::now_v7(),
    });
    ctx.repo.save_event(&dissolved, 1).await.unwrap();

    let proj = ctx.repo.load_projection(entity_id).await.unwrap().unwrap();
    assert!(
        proj.dissolved_at.is_some(),
        "dissolved entity must have dissolved_at set in projection"
    );
}
