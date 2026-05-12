//! COMP-2 — integration tests that prove the audit log immutability
//! guarantee at the SQL layer.
//!
//! Two checks ride together:
//!   1. The migrations apply cleanly against a fresh Postgres 17.
//!   2. A direct attempt to UPDATE / DELETE / TRUNCATE on
//!      `declaration_events` refuses with the SQLSTATE the BEFORE
//!      triggers raise.
//!
//! Why test at the SQL layer and not via the public API? The whole
//! point of this defence is that it does not rely on application code
//! to behave. The integration test acts as a hostile DBA: it bypasses
//! the repository abstractions and points sqlx straight at the table.
//! If the trigger were absent or the REVOKE incomplete, the test would
//! happily UPDATE the row and the assertion would fail.
//!
//! Run with: `cargo test --test audit_immutability -- --ignored`
//! Requires a Docker daemon (testcontainers).

use sqlx::postgres::PgPoolOptions;
use sqlx::Row;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::ImageExt;
use uuid::Uuid;

async fn bring_up_postgres() -> (
    testcontainers::ContainerAsync<Postgres>,
    sqlx::PgPool,
) {
    let pg = Postgres::default()
        .with_tag("17-alpine")
        .start()
        .await
        .expect("postgres container");
    let port = pg
        .get_host_port_ipv4(5432)
        .await
        .expect("postgres port");
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&url)
        .await
        .expect("connect pool");
    // Run the bundled declaration migrations against the fresh DB.
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("declaration migrations apply cleanly");
    (pg, pool)
}

/// Insert a single declaration_events row directly so the subsequent
/// UPDATE / DELETE assertions have a target. The row's columns are
/// chosen to satisfy every CHECK constraint without exercising the
/// domain layer (the test is about the table, not the aggregate).
async fn insert_event_row(pool: &sqlx::PgPool) {
    let id = Uuid::now_v7();
    let payload = serde_json::json!({"placeholder": "for COMP-2 test"});
    sqlx::query(
        r#"
        INSERT INTO declaration_events
            (declaration_id, aggregate_version, event_type, event_payload, correlation_id, causation_id)
        VALUES ($1, 1, 'declaration.submitted.v1', $2::jsonb, $3, NULL)
        "#,
    )
    .bind(id)
    .bind(payload)
    .bind(id)
    .execute(pool)
    .await
    .expect("seed insert succeeds — the table accepts INSERT");
}

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn migrations_apply_cleanly() {
    // Just bring the DB up — `bring_up_postgres` panics if any migration
    // in the chain fails. The fact that this test reaches an `assert!`
    // means 0001..=0007 all applied.
    let (_pg, pool) = bring_up_postgres().await;
    // Sanity: the triggers exist.
    let row = sqlx::query("SELECT COUNT(*) AS n FROM pg_trigger WHERE tgname LIKE 'trg_declaration_events_no_%'")
        .fetch_one(&pool)
        .await
        .expect("query pg_trigger");
    let n: i64 = row.try_get("n").expect("count column");
    assert_eq!(
        n, 3,
        "expected three refusal triggers (UPDATE, DELETE, TRUNCATE); got {n}"
    );
}

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn update_on_declaration_events_is_refused() {
    let (_pg, pool) = bring_up_postgres().await;
    insert_event_row(&pool).await;

    let result = sqlx::query(
        "UPDATE declaration_events SET event_type = 'tampered' WHERE aggregate_version = 1",
    )
    .execute(&pool)
    .await;

    let err = result.expect_err("UPDATE must be refused by the immutability trigger");
    let message = format!("{err:?}");
    assert!(
        message.contains("append-only") || message.contains("COMP-2") || message.contains("insufficient_privilege"),
        "expected the trigger's RAISE EXCEPTION; got: {message}"
    );
}

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn delete_on_declaration_events_is_refused() {
    let (_pg, pool) = bring_up_postgres().await;
    insert_event_row(&pool).await;

    let result = sqlx::query(
        "DELETE FROM declaration_events WHERE aggregate_version = 1",
    )
    .execute(&pool)
    .await;

    let err = result.expect_err("DELETE must be refused by the immutability trigger");
    let message = format!("{err:?}");
    assert!(
        message.contains("append-only") || message.contains("COMP-2") || message.contains("insufficient_privilege"),
        "expected the trigger's RAISE EXCEPTION; got: {message}"
    );
}

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn truncate_on_declaration_events_is_refused() {
    let (_pg, pool) = bring_up_postgres().await;
    insert_event_row(&pool).await;

    let result = sqlx::query("TRUNCATE declaration_events").execute(&pool).await;

    let err = result.expect_err("TRUNCATE must be refused by the immutability trigger");
    let message = format!("{err:?}");
    assert!(
        message.contains("append-only") || message.contains("COMP-2") || message.contains("insufficient_privilege"),
        "expected the trigger's RAISE EXCEPTION; got: {message}"
    );
}

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn outbox_update_and_delete_still_work() {
    // The relay legitimately UPDATEs `dispatched_at` and the DLQ-move
    // path DELETEs. Make sure COMP-2 didn't break either by mistake.
    let (_pg, pool) = bring_up_postgres().await;

    let event_id = Uuid::now_v7();
    let agg_id = Uuid::now_v7();
    sqlx::query(
        r#"
        INSERT INTO outbox (event_id, event_type, event_version, aggregate_type, aggregate_id, partition_key, payload)
        VALUES ($1, 'test.event.v1', 1, 'declaration', $2, $3, '{}'::jsonb)
        "#,
    )
    .bind(event_id)
    .bind(agg_id)
    .bind(agg_id.to_string())
    .execute(&pool)
    .await
    .expect("outbox INSERT");

    sqlx::query("UPDATE outbox SET dispatched_at = NOW() WHERE event_id = $1")
        .bind(event_id)
        .execute(&pool)
        .await
        .expect("outbox UPDATE on dispatched_at must succeed");

    sqlx::query("DELETE FROM outbox WHERE event_id = $1")
        .bind(event_id)
        .execute(&pool)
        .await
        .expect("outbox DELETE must succeed (retention worker + DLQ-move path)");
}

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn outbox_truncate_is_refused() {
    // TRUNCATE on the outbox would silently drop un-relayed events.
    // The migration revokes TRUNCATE from PUBLIC; on the testcontainer
    // the `postgres` superuser bypasses REVOKE. The assertion here is
    // weaker than for `declaration_events`: we only assert the GRANT
    // was actually stripped, observable via pg_class privileges.
    let (_pg, pool) = bring_up_postgres().await;
    let row = sqlx::query(
        r#"
        SELECT has_table_privilege('public', 'outbox', 'TRUNCATE') AS public_can_truncate
        "#,
    )
    .fetch_one(&pool)
    .await
    .expect("query has_table_privilege");
    let public_can_truncate: bool = row.try_get("public_can_truncate").unwrap_or(false);
    assert!(
        !public_can_truncate,
        "PUBLIC must not retain TRUNCATE on outbox after COMP-2 migration"
    );
}

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn outbox_dlq_update_is_refused_at_public_level() {
    // outbox_dlq UPDATE is not used by the application; the migration
    // revokes it from PUBLIC. Same observability caveat as above for
    // the superuser test path.
    let (_pg, pool) = bring_up_postgres().await;
    let row = sqlx::query(
        r#"
        SELECT has_table_privilege('public', 'outbox_dlq', 'UPDATE') AS public_can_update,
               has_table_privilege('public', 'outbox_dlq', 'TRUNCATE') AS public_can_truncate
        "#,
    )
    .fetch_one(&pool)
    .await
    .expect("query has_table_privilege");
    let public_can_update: bool = row.try_get("public_can_update").unwrap_or(false);
    let public_can_truncate: bool = row.try_get("public_can_truncate").unwrap_or(false);
    assert!(
        !public_can_update,
        "PUBLIC must not retain UPDATE on outbox_dlq after COMP-2 migration"
    );
    assert!(
        !public_can_truncate,
        "PUBLIC must not retain TRUNCATE on outbox_dlq after COMP-2 migration"
    );
}

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn retention_worker_with_days_zero_does_not_prune() {
    // COMP-2 — the safe default for tests is OUTBOX_RETENTION_DAYS=0,
    // which makes the retention worker a no-op even when the table has
    // rows older than the (would-be) cutoff.
    use recor_declaration::infrastructure::OutboxRetention;

    let (_pg, pool) = bring_up_postgres().await;

    // Insert an outbox row, mark it dispatched at a time far in the
    // past. Without retention disabled, it would be eligible to prune;
    // with retention disabled, it must remain.
    let event_id = Uuid::now_v7();
    let agg_id = Uuid::now_v7();
    sqlx::query(
        r#"
        INSERT INTO outbox (event_id, event_type, event_version, aggregate_type, aggregate_id, partition_key, payload, dispatched_at)
        VALUES ($1, 'test.event.v1', 1, 'declaration', $2, $3, '{}'::jsonb, NOW() - INTERVAL '365 days')
        "#,
    )
    .bind(event_id)
    .bind(agg_id)
    .bind(agg_id.to_string())
    .execute(&pool)
    .await
    .expect("seed dispatched row");

    let retention = OutboxRetention::new(pool.clone()); // defaults to retention_days = 0
    let outcome = retention
        .prune_once()
        .await
        .expect("prune_once succeeds (no-op)");
    assert_eq!(
        outcome.pruned, 0,
        "with retention_days=0, prune_once MUST be a no-op"
    );

    // Verify the row is still there.
    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*)::bigint FROM outbox WHERE event_id = $1",
    )
    .bind(event_id)
    .fetch_one(&pool)
    .await
    .expect("count");
    assert_eq!(count.0, 1, "outbox row must remain when retention is disabled");
}

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn retention_worker_prunes_dispatched_rows_older_than_window() {
    use recor_declaration::infrastructure::OutboxRetention;

    let (_pg, pool) = bring_up_postgres().await;

    // Three rows:
    //   - old & dispatched      → must be pruned
    //   - recent & dispatched   → must remain
    //   - old & NOT dispatched  → must remain (un-delivered)
    let mk = |dispatched_at: Option<&str>| {
        let event_id = Uuid::now_v7();
        let agg = Uuid::now_v7();
        (event_id, agg, dispatched_at.map(str::to_string))
    };
    let (old_disp_id, old_disp_agg, _) = mk(Some("NOW() - INTERVAL '60 days'"));
    let (recent_disp_id, recent_disp_agg, _) = mk(Some("NOW() - INTERVAL '1 day'"));
    let (old_undisp_id, old_undisp_agg, _) = mk(None);

    // Use raw SQL so the test row carries the explicit dispatched_at
    // expression (binding NOW() - INTERVAL via sqlx is awkward).
    sqlx::query(
        r#"
        INSERT INTO outbox (event_id, event_type, event_version, aggregate_type, aggregate_id, partition_key, payload, dispatched_at)
        VALUES ($1, 'test.event.v1', 1, 'declaration', $2, $3, '{}'::jsonb, NOW() - INTERVAL '60 days')
        "#,
    ).bind(old_disp_id).bind(old_disp_agg).bind(old_disp_agg.to_string())
        .execute(&pool).await.expect("seed old-dispatched");
    sqlx::query(
        r#"
        INSERT INTO outbox (event_id, event_type, event_version, aggregate_type, aggregate_id, partition_key, payload, dispatched_at)
        VALUES ($1, 'test.event.v1', 1, 'declaration', $2, $3, '{}'::jsonb, NOW() - INTERVAL '1 day')
        "#,
    ).bind(recent_disp_id).bind(recent_disp_agg).bind(recent_disp_agg.to_string())
        .execute(&pool).await.expect("seed recent-dispatched");
    sqlx::query(
        r#"
        INSERT INTO outbox (event_id, event_type, event_version, aggregate_type, aggregate_id, partition_key, payload, dispatched_at)
        VALUES ($1, 'test.event.v1', 1, 'declaration', $2, $3, '{}'::jsonb, NULL)
        "#,
    ).bind(old_undisp_id).bind(old_undisp_agg).bind(old_undisp_agg.to_string())
        .execute(&pool).await.expect("seed undispatched");

    let retention = OutboxRetention::new(pool.clone()).with_retention_days(30);
    let outcome = retention.prune_once().await.expect("prune_once");
    assert_eq!(outcome.pruned, 1, "exactly one row (old & dispatched) should prune");

    // Verify the survivors.
    let n_remaining: (i64,) = sqlx::query_as("SELECT COUNT(*)::bigint FROM outbox")
        .fetch_one(&pool)
        .await
        .expect("count remaining");
    assert_eq!(n_remaining.0, 2, "two rows must remain (recent + undispatched)");
}
