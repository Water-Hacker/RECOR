//! TODO-027 — Postgres adapter integration tests for the Person service.
//!
//! Tests: round-trip serialisation of every event variant, COMP-2 trigger,
//! idempotency-cache hit/miss, projection rebuild from events, outbox
//! insertion + dispatch markers.
//!
//! All tests `#[ignore]` — CI runs via `--ignored`. Run locally:
//!   cargo test -p recor-person-service --test postgres_adapter_integration \
//!     -- --ignored --nocapture

use std::sync::Arc;

use sqlx::postgres::PgPoolOptions;
use testcontainers::ContainerAsync;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::ImageExt;
use time::{macros::date, OffsetDateTime};
use uuid::Uuid;

use recor_person_service::application::port::{PersonRepository, RepositoryError};
use recor_person_service::domain::{
    PersonEvent, PersonId, PersonMergedV1, PersonRegisteredV1, PersonUpdatedV1,
};
use recor_person_service::domain::value_object::{
    CanonicalFullName, IdDocument, IdDocumentType, Nationality, PersonAttributes,
};
use recor_person_service::infrastructure::postgres::{
    IdempotencyStore, PostgresPersonRepository,
};

// ─── Harness ──────────────────────────────────────────────────────────────────

struct Ctx {
    repo: Arc<PostgresPersonRepository>,
    idempotency: Arc<IdempotencyStore>,
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
    let repo = Arc::new(PostgresPersonRepository::new(pool.clone()));
    repo.run_migrations().await.expect("migrations");
    let idempotency = Arc::new(IdempotencyStore::new(pool.clone()));
    Ctx { repo, idempotency, pool, _pg: pg }
}

fn attrs() -> PersonAttributes {
    PersonAttributes {
        canonical_full_name: CanonicalFullName::try_new("Ngono Marie").unwrap(),
        nationality: Nationality::try_new("CM").unwrap(),
        date_of_birth: Some(date!(1985 - 06 - 15)),
        primary_id_document: IdDocument {
            issuer: "CM:DGSN".into(),
            doc_type: IdDocumentType::NationalId,
            number: "100199999".into(),
            expiry: None,
        },
        biometric_reference_hash: None,
    }
}

fn registered_event(person_id: PersonId, principal: &str) -> PersonEvent {
    PersonEvent::Registered(PersonRegisteredV1 {
        person_id,
        attributes: attrs(),
        actor_principal: principal.to_string(),
        registered_at: OffsetDateTime::now_utc(),
        correlation_id: Uuid::now_v7(),
    })
}

// ─── Test 1: round-trip PersonRegisteredV1 ───────────────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn round_trip_registered_event() {
    let ctx = setup().await;
    let person_id = PersonId::new();
    let event = registered_event(person_id, "spiffe://recor.cm/person-pg-1");

    ctx.repo.save_event(&event, 0).await.expect("save_event");

    let events = ctx.repo.load_events(person_id).await.expect("load_events");
    assert_eq!(events.len(), 1);
    assert!(matches!(&events[0], PersonEvent::Registered(r) if r.person_id == person_id));
}

// ─── Test 2: round-trip PersonUpdatedV1 ──────────────────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn round_trip_updated_event() {
    let ctx = setup().await;
    let person_id = PersonId::new();
    let principal = "spiffe://recor.cm/person-pg-2";
    ctx.repo.save_event(&registered_event(person_id, principal), 0).await.unwrap();

    let updated_attrs = PersonAttributes {
        canonical_full_name: CanonicalFullName::try_new("Ngono Marie Updated").unwrap(),
        ..attrs()
    };
    let update = PersonEvent::Updated(PersonUpdatedV1 {
        person_id,
        before: attrs(),
        after: updated_attrs,
        actor_principal: principal.to_string(),
        updated_at: OffsetDateTime::now_utc(),
        correlation_id: Uuid::now_v7(),
    });
    ctx.repo.save_event(&update, 1).await.expect("save update");

    let events = ctx.repo.load_events(person_id).await.unwrap();
    assert_eq!(events.len(), 2);
    assert!(matches!(&events[1], PersonEvent::Updated(_)));
}

// ─── Test 3: round-trip PersonMergedV1 ───────────────────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn round_trip_merged_event() {
    let ctx = setup().await;
    let source_id = PersonId::new();
    let target_id = PersonId::new();
    let principal = "spiffe://recor.cm/person-pg-3-admin";

    // Register both persons.
    ctx.repo.save_event(&registered_event(source_id, principal), 0).await.unwrap();
    ctx.repo.save_event(&registered_event(target_id, principal), 0).await.unwrap();

    // Merge source into target.
    let merge = PersonEvent::Merged(PersonMergedV1 {
        person_id: source_id,
        into_person_id: target_id,
        actor_principal: principal.to_string(),
        merged_at: OffsetDateTime::now_utc(),
        correlation_id: Uuid::now_v7(),
    });
    ctx.repo.save_merge(&merge, 1).await.expect("save_merge");

    let events = ctx.repo.load_events(source_id).await.unwrap();
    assert_eq!(events.len(), 2);
    assert!(matches!(&events[1], PersonEvent::Merged(_)));
}

// ─── Test 4: COMP-2 trigger — UPDATE on person_events fails ──────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn comp2_update_on_person_events_is_refused() {
    let ctx = setup().await;
    let person_id = PersonId::new();
    ctx.repo
        .save_event(&registered_event(person_id, "spiffe://recor.cm/person-comp2"), 0)
        .await
        .unwrap();

    let err = sqlx::query(
        "UPDATE person_events SET event_type = 'tampered' WHERE person_id = $1",
    )
    .bind(person_id.0)
    .execute(&ctx.pool)
    .await;

    assert!(err.is_err(), "COMP-2 trigger must refuse UPDATE on person_events");
}

// ─── Test 5: COMP-2 trigger — DELETE on person_events fails ──────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn comp2_delete_on_person_events_is_refused() {
    let ctx = setup().await;
    let person_id = PersonId::new();
    ctx.repo
        .save_event(&registered_event(person_id, "spiffe://recor.cm/person-comp2-del"), 0)
        .await
        .unwrap();

    let err = sqlx::query("DELETE FROM person_events WHERE person_id = $1")
        .bind(person_id.0)
        .execute(&ctx.pool)
        .await;

    assert!(err.is_err(), "COMP-2 trigger must refuse DELETE on person_events");
}

// ─── Test 6: optimistic concurrency wrong version → Conflict ─────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn optimistic_concurrency_wrong_version() {
    let ctx = setup().await;
    let person_id = PersonId::new();
    let principal = "spiffe://recor.cm/person-concurrency";

    ctx.repo.save_event(&registered_event(person_id, principal), 0).await.unwrap();

    // Attempt re-register at version 0 (already used).
    let err = ctx
        .repo
        .save_event(&registered_event(person_id, principal), 0)
        .await
        .unwrap_err();
    assert!(
        matches!(err, RepositoryError::Conflict { .. }),
        "wrong version must Conflict, got {err:?}"
    );
}

// ─── Test 7: projection rebuilt from registered event ────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn projection_correct_after_register() {
    let ctx = setup().await;
    let person_id = PersonId::new();
    let principal = "spiffe://recor.cm/person-proj";

    ctx.repo.save_event(&registered_event(person_id, principal), 0).await.unwrap();

    let proj = ctx.repo.load_projection(person_id).await.unwrap().unwrap();
    assert_eq!(proj.person_id, person_id);
    assert_eq!(proj.attributes.canonical_full_name.as_str(), "Ngono Marie");
}

// ─── Test 8: projection absent before any event ───────────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn projection_absent_for_unknown_person() {
    let ctx = setup().await;
    let unknown = PersonId::new();
    let proj = ctx.repo.load_projection(unknown).await.unwrap();
    assert!(proj.is_none());
}

// ─── Test 9: outbox row written on register ───────────────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn outbox_row_written_on_register() {
    let ctx = setup().await;
    let person_id = PersonId::new();
    ctx.repo
        .save_event(&registered_event(person_id, "spiffe://recor.cm/person-outbox"), 0)
        .await
        .unwrap();

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM outbox WHERE aggregate_id = $1")
        .bind(person_id.0)
        .fetch_one(&ctx.pool)
        .await
        .expect("outbox count");
    assert_eq!(count, 1);
}

// ─── Test 10: idempotency cache miss → None ───────────────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn idempotency_miss() {
    let ctx = setup().await;
    let principal = "spiffe://recor.cm/person-idem-test";
    let result = ctx
        .idempotency
        .check_existing("person-idem-miss", principal)
        .await
        .unwrap();
    assert!(result.is_none());
}

// ─── Test 11: idempotency record → hit ───────────────────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn idempotency_set_then_hit() {
    let ctx = setup().await;
    let k = format!("person-idem-hit-{}", Uuid::now_v7());
    let principal = "spiffe://recor.cm/person-idem-test";
    let v = serde_json::json!({"person_id": Uuid::now_v7()});
    let request_hash = "abcdef01".repeat(8);
    ctx.idempotency
        .record(&k, principal, &request_hash, 201, &v, 3600)
        .await
        .unwrap();
    let hit = ctx.idempotency.check_existing(&k, principal).await.unwrap();
    assert!(hit.is_some(), "key should be a cache hit after record");
    assert_eq!(hit.unwrap().response_status, 201);
}

// ─── Test 12: search returns registered persons ───────────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn search_returns_matching_persons() {
    let ctx = setup().await;
    let person_id = PersonId::new();
    ctx.repo
        .save_event(&registered_event(person_id, "spiffe://recor.cm/person-search"), 0)
        .await
        .unwrap();

    let results = ctx
        .repo
        .search("Ngono", None, None, 10)
        .await
        .expect("search");
    assert!(
        results.iter().any(|r| r.person_id == person_id),
        "search must return the registered person"
    );
}

// ─── Test 13: search with nationality filter ──────────────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn search_with_nationality_filter() {
    let ctx = setup().await;
    let person_id = PersonId::new();
    ctx.repo
        .save_event(&registered_event(person_id, "spiffe://recor.cm/person-search-nat"), 0)
        .await
        .unwrap();

    let cm_results = ctx
        .repo
        .search("Ngono", Some("CM"), None, 10)
        .await
        .unwrap();
    assert!(cm_results.iter().any(|r| r.person_id == person_id));

    let fr_results = ctx
        .repo
        .search("Ngono", Some("FR"), None, 10)
        .await
        .unwrap();
    assert!(
        !fr_results.iter().any(|r| r.person_id == person_id),
        "wrong nationality must not match"
    );
}

// ─── Test 14: load_events unknown person → empty ─────────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn load_events_unknown_person_is_empty() {
    let ctx = setup().await;
    let events = ctx.repo.load_events(PersonId::new()).await.unwrap();
    assert!(events.is_empty());
}

// ─── Test 15: projection version advances on update ──────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn projection_version_advances_on_update() {
    let ctx = setup().await;
    let person_id = PersonId::new();
    let principal = "spiffe://recor.cm/person-version-advance";
    ctx.repo.save_event(&registered_event(person_id, principal), 0).await.unwrap();

    let v1 = ctx.repo.load_projection(person_id).await.unwrap().unwrap().aggregate_version;
    assert_eq!(v1, 1);

    let update = PersonEvent::Updated(PersonUpdatedV1 {
        person_id,
        before: attrs(),
        after: PersonAttributes {
            canonical_full_name: CanonicalFullName::try_new("Ngono Marie V2").unwrap(),
            ..attrs()
        },
        actor_principal: principal.to_string(),
        updated_at: OffsetDateTime::now_utc(),
        correlation_id: Uuid::now_v7(),
    });
    ctx.repo.save_event(&update, 1).await.unwrap();

    let v2 = ctx.repo.load_projection(person_id).await.unwrap().unwrap().aggregate_version;
    assert_eq!(v2, 2);
}

// ─── TODO-040 — outbox relay + DLQ admin integration ───────────────

#[tokio::test]
#[ignore = "requires docker daemon"]
async fn relay_exhausts_attempts_then_moves_to_dlq() {
    // TODO-040 integration: register a person, then run the relay
    // against a webhook URL that ALWAYS returns 500. The relay must
    // retry `max_attempts` times, then atomically move the row into
    // outbox_dlq. The original outbox row must be gone.
    use recor_person_service::infrastructure::relay::{OutboxRelay, RelaySubscriber};
    use std::net::SocketAddr;
    use std::time::Duration;

    let ctx = setup().await;

    // Seed one event so the outbox has a row.
    let person_id = PersonId(Uuid::now_v7());
    let event = registered_event(person_id, "spiffe://recor.cm/admin-1");
    ctx.repo.save_event(&event, 0).await.expect("save_event");
    let outbox_rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM outbox WHERE dispatched_at IS NULL",
    )
    .fetch_one(&ctx.pool)
    .await
    .expect("count");
    assert_eq!(outbox_rows, 1, "exactly one outbox row before relay starts");

    let app =
        axum::Router::new().route(
            "/sink",
            axum::routing::post(|| async {
                (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "nope")
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let subscriber = RelaySubscriber {
        name: "test-sink".into(),
        webhook_url: format!("http://{addr}/sink"),
        hmac_secret: "test-secret".into(),
    };
    let relay = OutboxRelay::new(ctx.pool.clone(), subscriber)
        .with_poll_interval(Duration::from_millis(100))
        .with_max_attempts(3)
        .with_batch_size(10);

    let cancel = tokio_util::sync::CancellationToken::new();
    let cancel_c = cancel.clone();
    let h = tokio::spawn(async move { relay.run(cancel_c).await });
    tokio::time::sleep(Duration::from_millis(800)).await;
    cancel.cancel();
    h.await.unwrap();
    server.abort();

    let undispatched: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM outbox WHERE dispatched_at IS NULL",
    )
    .fetch_one(&ctx.pool)
    .await
    .expect("count");
    assert_eq!(undispatched, 0, "outbox row should have moved to DLQ");
    let dlq_rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM outbox_dlq")
        .fetch_one(&ctx.pool)
        .await
        .expect("count");
    assert_eq!(dlq_rows, 1, "exactly one row in DLQ after exhaustion");
    let dlq_attempts: i32 =
        sqlx::query_scalar("SELECT dispatch_attempts FROM outbox_dlq LIMIT 1")
            .fetch_one(&ctx.pool)
            .await
            .expect("attempts");
    assert!(
        dlq_attempts >= 3,
        "DLQ row should record at least 3 attempts; got {dlq_attempts}"
    );
}

#[tokio::test]
#[ignore = "requires docker daemon"]
async fn dlq_replay_moves_row_back_to_outbox() {
    // TODO-040 integration: seed a DLQ row directly, then call
    // OutboxAdminStore::replay_dlq and assert the row migrated back to
    // outbox with dispatch_attempts reset to 0. A second replay of the
    // same id is a NotFound (idempotency).
    use recor_person_service::infrastructure::outbox_admin::{
        OutboxAdminError, OutboxAdminStore,
    };
    let ctx = setup().await;

    let id = Uuid::now_v7();
    let event_id = Uuid::now_v7();
    let aggregate_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO outbox_dlq ( \
             id, event_id, event_type, event_version, aggregate_type, \
             aggregate_id, partition_key, payload, headers, \
             created_at, dead_lettered_at, dispatch_attempts, last_error \
         ) VALUES ($1,$2,'person.registered.v1',1,'person',$3,$4,'{}'::jsonb,'{}'::jsonb, \
                  NOW(), NOW(), 12, 'transport refused')",
    )
    .bind(id)
    .bind(event_id)
    .bind(aggregate_id)
    .bind(aggregate_id.to_string())
    .execute(&ctx.pool)
    .await
    .expect("seed dlq row");

    let store = OutboxAdminStore::new(ctx.pool.clone());
    let total = store.count_dlq().await.expect("count_dlq ok");
    assert_eq!(total, 1);

    store.replay_dlq(id).await.expect("replay ok");

    let dlq_left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM outbox_dlq")
        .fetch_one(&ctx.pool)
        .await
        .unwrap();
    assert_eq!(dlq_left, 0, "DLQ row should be gone after replay");
    let back_in_outbox: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM outbox WHERE id = $1 AND dispatched_at IS NULL AND dispatch_attempts = 0",
    )
    .bind(id)
    .fetch_one(&ctx.pool)
    .await
    .unwrap();
    assert_eq!(
        back_in_outbox, 1,
        "row must be back in outbox with attempts reset to 0"
    );

    // Idempotency: a second replay of the same id is NotFound.
    let err = store.replay_dlq(id).await.expect_err("second replay refused");
    match err {
        OutboxAdminError::NotFound(missing) => assert_eq!(missing, id),
        other => panic!("expected NotFound, got {other:?}"),
    }
}
