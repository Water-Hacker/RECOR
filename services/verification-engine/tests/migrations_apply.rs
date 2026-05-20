//! FIND-014 integration test: V-engine migrations apply cleanly against
//! a fresh Postgres 17. Catches schema-drift regressions (migration
//! ordering errors, missing dependencies, syntax errors that the
//! per-file `sqlx::query!` macros can't surface alone).
//!
//! Run with:
//!   cargo test -p recor-verification-engine --test migrations_apply \
//!     -- --ignored
//!
//! Requires a Docker daemon (testcontainers).

use sqlx::postgres::PgPoolOptions;
use sqlx::Row;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::ImageExt;

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
    // Run every V-engine migration against the fresh DB. A failure
    // anywhere in 0001..=0007 will panic here.
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("verification-engine migrations apply cleanly");
    (pg, pool)
}

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn every_migration_applies() {
    let (_pg, pool) = bring_up_postgres().await;

    // verification_cases has been on the schema since 0001 and is the
    // foundation the per-case tenancy gate (FIND-004) reads. The
    // `declarant_principal NOT NULL` constraint is what makes the
    // gate possible — assert it exists in the live schema.
    let row = sqlx::query(
        "SELECT is_nullable \
         FROM information_schema.columns \
         WHERE table_name = 'verification_cases' \
           AND column_name = 'declarant_principal'",
    )
    .fetch_one(&pool)
    .await
    .expect("declarant_principal column present");
    let is_nullable: String = row.try_get("is_nullable").expect("is_nullable column");
    assert_eq!(
        is_nullable, "NO",
        "FIND-004: declarant_principal must be NOT NULL on verification_cases"
    );

    // outbox-DLQ + Kafka consumer DLQ tables ride together with the
    // base relay stack and must both apply.
    for table in ["verification_outbox", "verification_outbox_dlq", "kafka_consumer_dlq"] {
        let row = sqlx::query(
            "SELECT COUNT(*) AS n FROM information_schema.tables WHERE table_name = $1",
        )
        .bind(table)
        .fetch_one(&pool)
        .await
        .expect("table-lookup query");
        let n: i64 = row.try_get("n").expect("count column");
        assert_eq!(n, 1, "expected table {table} to exist post-migration");
    }
}

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn comp2_triggers_are_present() {
    let (_pg, pool) = bring_up_postgres().await;

    // Three append-only refusal triggers on verification_cases —
    // identical pattern to declaration_events (D15 cryptographic
    // provenance).
    let row = sqlx::query(
        "SELECT COUNT(*) AS n FROM pg_trigger \
         WHERE tgname LIKE 'trg_verification_cases_no_%'",
    )
    .fetch_one(&pool)
    .await
    .expect("query pg_trigger");
    let n: i64 = row.try_get("n").expect("count column");
    assert_eq!(
        n, 3,
        "expected three refusal triggers (UPDATE, DELETE, TRUNCATE) on \
         verification_cases — COMP-2 immutability guarantee. Got {n}."
    );
}

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn r_ver_sanctions_pep_icij_tables_apply() {
    let (_pg, pool) = bring_up_postgres().await;

    // R-VER-2..6 (the real stage tables) ship in 0005..=0007. The
    // pipeline-stage wiring (FIND-009 follow-up) depends on these
    // tables existing.
    for table in ["sanctions_persons", "peps", "icij_persons"] {
        let row = sqlx::query(
            "SELECT COUNT(*) AS n FROM information_schema.tables WHERE table_name = $1",
        )
        .bind(table)
        .fetch_one(&pool)
        .await
        .expect("table-lookup query");
        let n: i64 = row.try_get("n").expect("count column");
        assert_eq!(
            n, 1,
            "expected table {table} (R-VER-*) to exist post-migration"
        );
    }
}
