//! FIND-014 integration test — COMP-2 audit-immutability for the
//! verification-engine. Mirrors the declaration service's
//! `audit_immutability.rs`: verifies that direct UPDATE / DELETE /
//! TRUNCATE on `verification_cases` is refused by the BEFORE-trigger
//! defence, independent of application code.
//!
//! Why test at the SQL layer: the whole point of the trigger is that
//! it doesn't depend on the repository to behave. This test bypasses
//! the application and points sqlx at the table directly — if the
//! trigger were absent or the REVOKE incomplete, the assertions
//! would fail.
//!
//! Run with:
//!   cargo test -p recor-verification-engine --test audit_immutability \
//!     -- --ignored
//!
//! Requires a Docker daemon (testcontainers).

use sqlx::postgres::PgPoolOptions;
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
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("verification-engine migrations apply cleanly");
    (pg, pool)
}

/// Insert a single verification_cases row directly so the subsequent
/// UPDATE / DELETE / TRUNCATE assertions have a target. Bypasses the
/// repository — this is the "hostile DBA" test.
async fn insert_case_row(pool: &sqlx::PgPool) {
    let case_id = Uuid::now_v7();
    let declaration_id = Uuid::now_v7();
    let entity_id = Uuid::now_v7();
    let payload = serde_json::json!({"placeholder": "for COMP-2 test"});
    sqlx::query(
        r#"
        INSERT INTO verification_cases
            (case_id, declaration_id, entity_id, declarant_principal,
             lane, authenticity_belief, authenticity_plausibility,
             risk_belief, case_payload, created_at, completed_at,
             total_duration_ms)
        VALUES
            ($1, $2, $3, 'spiffe://recor.cm/test',
             'green', 0.7, 0.9,
             0.1, $4::jsonb, NOW(), NOW(),
             42)
        "#,
    )
    .bind(case_id)
    .bind(declaration_id)
    .bind(entity_id)
    .bind(payload)
    .execute(pool)
    .await
    .expect("seed insert succeeds — the table accepts INSERT");
}

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn update_on_verification_cases_is_refused() {
    let (_pg, pool) = bring_up_postgres().await;
    insert_case_row(&pool).await;

    let result =
        sqlx::query("UPDATE verification_cases SET lane = 'red' WHERE total_duration_ms = 42")
            .execute(&pool)
            .await;

    let err = result.expect_err("UPDATE must be refused by the immutability trigger");
    let message = format!("{err:?}");
    assert!(
        message.contains("append-only")
            || message.contains("COMP-2")
            || message.contains("insufficient_privilege"),
        "expected the trigger's RAISE EXCEPTION; got: {message}"
    );
}

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn delete_on_verification_cases_is_refused() {
    let (_pg, pool) = bring_up_postgres().await;
    insert_case_row(&pool).await;

    let result = sqlx::query("DELETE FROM verification_cases WHERE total_duration_ms = 42")
        .execute(&pool)
        .await;

    let err = result.expect_err("DELETE must be refused by the immutability trigger");
    let message = format!("{err:?}");
    assert!(
        message.contains("append-only")
            || message.contains("COMP-2")
            || message.contains("insufficient_privilege"),
        "expected the trigger's RAISE EXCEPTION; got: {message}"
    );
}

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn truncate_on_verification_cases_is_refused() {
    let (_pg, pool) = bring_up_postgres().await;
    insert_case_row(&pool).await;

    let result = sqlx::query("TRUNCATE TABLE verification_cases").execute(&pool).await;

    let err = result.expect_err("TRUNCATE must be refused by the immutability trigger");
    let message = format!("{err:?}");
    assert!(
        message.contains("append-only")
            || message.contains("COMP-2")
            || message.contains("insufficient_privilege"),
        "expected the trigger's RAISE EXCEPTION; got: {message}"
    );
}
