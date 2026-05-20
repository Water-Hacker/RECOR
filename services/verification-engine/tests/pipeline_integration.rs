//! FIND-014 integration test — V-engine pipeline orchestrator
//! end-to-end against a real Postgres.
//!
//! Drives `SubmitVerificationUseCase` directly (no HTTP) with the
//! same seven-stage pipeline production uses, persists the resulting
//! `VerificationCase` via the Postgres repository, and asserts both
//! the case row AND the outbox row land in the database.
//!
//! Companion to `api_integration.rs`. That file gates the HTTP
//! surface; this file gates the pipeline + repository contract.
//!
//! Run with:
//!   cargo test -p recor-verification-engine --test pipeline_integration \
//!     -- --ignored --nocapture
//!
//! Requires Docker (testcontainers spawns Postgres).

use std::sync::Arc;

use sqlx::postgres::PgPoolOptions;
use sqlx::Row;
use testcontainers::ContainerAsync;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::ImageExt;
use time::macros::{date, datetime};
use uuid::Uuid;

use recor_verification_engine::application::stages::{
    AdverseMediaStub, CrossSourceStub, IdentityAuthenticationStage, PatternDetectionStub,
    PepStub, SanctionsStub, SchemaValidationStage,
};
use recor_verification_engine::application::{
    PipelineOrchestrator, SubmitVerificationUseCase,
};
use recor_verification_engine::domain::{
    DeclarationSnapshot, LaneThresholds, OwnerSnapshot, Stage,
};
use recor_verification_engine::infrastructure::{
    PostgresMockBunec, PostgresVerificationRepository,
};

async fn bring_up_postgres() -> (ContainerAsync<Postgres>, sqlx::PgPool) {
    let pg = Postgres::default()
        .with_tag("17-alpine")
        .start()
        .await
        .expect("postgres container");
    let port = pg.get_host_port_ipv4(5432).await.expect("pg port");
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&url)
        .await
        .expect("connect pool");
    (pg, pool)
}

fn build_snapshot() -> DeclarationSnapshot {
    DeclarationSnapshot {
        declaration_id: Uuid::now_v7(),
        entity_id: Uuid::now_v7(),
        declarant_principal: "spiffe://recor.cm/test-declarant".to_string(),
        declarant_role: "self".to_string(),
        kind: "incorporation".to_string(),
        effective_from: date!(2026 - 05 - 01),
        beneficial_owners: vec![OwnerSnapshot {
            person_id: Uuid::now_v7(),
            ownership_basis_points: 10_000,
            interest_kind: "equity".to_string(),
        }],
        attestation_signed_by: "spiffe://recor.cm/test-declarant".to_string(),
        attestation_signature_hex: "00".repeat(64),
        attestation_public_key_hex: "11".repeat(32),
        receipt_hash_hex: "22".repeat(32),
        correlation_id: Uuid::now_v7(),
        submitted_at: datetime!(2026-05-01 12:00:00 UTC),
    }
}

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn pipeline_runs_and_persists_case_plus_outbox_row() {
    let (_pg, pool) = bring_up_postgres().await;

    let repository = Arc::new(PostgresVerificationRepository::new(pool.clone()));
    repository.run_migrations().await.expect("migrations");

    let bunec = Arc::new(PostgresMockBunec::new(pool.clone()));
    let stages: Vec<Arc<dyn Stage>> = vec![
        Arc::new(SchemaValidationStage::new()),
        Arc::new(IdentityAuthenticationStage::new(bunec.clone())),
        Arc::new(SanctionsStub::new()),
        Arc::new(PepStub::new()),
        Arc::new(AdverseMediaStub::new()),
        Arc::new(PatternDetectionStub::new()),
        Arc::new(CrossSourceStub::new()),
    ];
    let orchestrator =
        Arc::new(PipelineOrchestrator::new(stages, LaneThresholds::default()));
    let submit = SubmitVerificationUseCase::new(orchestrator, repository.clone());

    let snapshot = build_snapshot();
    let declaration_id = snapshot.declaration_id;
    let case = submit
        .execute(snapshot)
        .await
        .expect("submit verification use case succeeds");

    // Sanity: the case carries a lane decision and the source snapshot.
    assert_eq!(case.declaration.declaration_id, declaration_id);
    assert!(
        case.total_duration_ms > 0,
        "pipeline should record a non-zero total_duration_ms"
    );

    // Case row landed in `verification_cases` with the declarant
    // principal denormalised onto it (FIND-004 column).
    let row = sqlx::query(
        "SELECT declarant_principal, lane FROM verification_cases WHERE declaration_id = $1",
    )
    .bind(declaration_id)
    .fetch_one(&pool)
    .await
    .expect("verification_cases row inserted");
    let declarant: String = row.try_get("declarant_principal").expect("col");
    let lane: String = row.try_get("lane").expect("col");
    assert_eq!(declarant, "spiffe://recor.cm/test-declarant");
    assert!(
        ["green", "yellow", "red"].contains(&lane.as_str()),
        "lane must be one of the three D14 enum values; got '{lane}'"
    );

    // Outbox row landed in the same transaction. The relay drains
    // these later — at this point dispatched_at is NULL.
    let outbox_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM verification_outbox WHERE aggregate_id = $1",
    )
    .bind(case.case_id.0)
    .fetch_one(&pool)
    .await
    .expect("outbox count query");
    assert_eq!(
        outbox_count, 1,
        "exactly one outbox row should land per verification case"
    );
}

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn pipeline_is_idempotent_on_declaration_id() {
    let (_pg, pool) = bring_up_postgres().await;

    let repository = Arc::new(PostgresVerificationRepository::new(pool.clone()));
    repository.run_migrations().await.expect("migrations");

    let bunec = Arc::new(PostgresMockBunec::new(pool.clone()));
    let stages: Vec<Arc<dyn Stage>> = vec![
        Arc::new(SchemaValidationStage::new()),
        Arc::new(IdentityAuthenticationStage::new(bunec.clone())),
        Arc::new(SanctionsStub::new()),
        Arc::new(PepStub::new()),
        Arc::new(AdverseMediaStub::new()),
        Arc::new(PatternDetectionStub::new()),
        Arc::new(CrossSourceStub::new()),
    ];
    let orchestrator =
        Arc::new(PipelineOrchestrator::new(stages, LaneThresholds::default()));
    let submit = SubmitVerificationUseCase::new(orchestrator, repository.clone());

    let snapshot = build_snapshot();
    let first = submit
        .execute(snapshot.clone())
        .await
        .expect("first submit");
    let second = submit
        .execute(snapshot)
        .await
        .expect("second submit (idempotent replay)");

    assert_eq!(
        first.case_id, second.case_id,
        "idempotent replay must surface the original case_id (D13)"
    );

    // Only one row should be in verification_cases for the declaration.
    let case_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM verification_cases WHERE declaration_id = $1",
    )
    .bind(first.declaration.declaration_id)
    .fetch_one(&pool)
    .await
    .expect("case count query");
    assert_eq!(case_count, 1, "idempotency must not duplicate rows");
}
