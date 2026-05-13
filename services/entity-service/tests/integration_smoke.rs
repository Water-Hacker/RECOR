//! Integration smoke test against a testcontainers-backed Postgres.
//!
//! These tests are `#[ignore]`-gated by default: they require the
//! Docker daemon. Run with `cargo test -p recor-entity-service --test
//! integration_smoke -- --ignored` once a daemon is available.

use std::sync::Arc;
use std::time::Duration;

use sqlx::postgres::PgPoolOptions;
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::postgres::Postgres as PgImage;
use time::macros::date;
use uuid::Uuid;

use recor_entity_service::application::{
    EntityRepository, RegisterEntityUseCase, SearchCriteria, SearchEntitiesUseCase,
};
use recor_entity_service::domain::{
    CanonicalName, EntityId, EntityType, Jurisdiction, RegisterEntity, RegistrationNumber,
};
use recor_entity_service::infrastructure::PostgresEntityRepository;

async fn setup_repo() -> (testcontainers::ContainerAsync<PgImage>, Arc<PostgresEntityRepository>) {
    let node = PgImage::default().start().await.expect("postgres starts");
    let port = node.get_host_port_ipv4(5432).await.expect("port");
    let url = format!(
        "postgres://postgres:postgres@127.0.0.1:{port}/postgres"
    );
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(10))
        .connect(&url)
        .await
        .expect("connect");
    let repo = Arc::new(PostgresEntityRepository::new(pool));
    repo.run_migrations().await.expect("migrations");
    (node, repo)
}

fn make_register_cmd(name: &str) -> RegisterEntity {
    RegisterEntity {
        entity_id: EntityId::new(),
        canonical_name: CanonicalName::try_from_str(name).unwrap(),
        entity_type: EntityType::Sarl,
        jurisdiction: Jurisdiction::try_from_str("CM").unwrap(),
        registration_number_in_jurisdiction: RegistrationNumber::try_from_str(
            &format!("RC/DLA/{}", Uuid::now_v7()),
        )
        .unwrap(),
        founded_at: date!(2020 - 01 - 15),
        registered_by_principal: "spiffe://recor.cm/admin-1".into(),
        registered_at: time::OffsetDateTime::now_utc(),
        correlation_id: Uuid::now_v7(),
    }
}

#[tokio::test]
#[ignore = "requires docker daemon"]
async fn register_then_get_round_trips() {
    let (_node, repo) = setup_repo().await;
    let usecase = RegisterEntityUseCase::new(repo.clone());
    let cmd = make_register_cmd("ACME SARL");
    let id = cmd.entity_id;
    let receipt = usecase.execute(cmd).await.expect("register succeeds");
    assert_eq!(receipt.entity_id, id);

    let projection = repo
        .load_projection(id)
        .await
        .expect("load_projection ok")
        .expect("entity exists");
    assert_eq!(projection.canonical_name, "ACME SARL");
    assert_eq!(projection.jurisdiction.as_str(), "CM");
    assert_eq!(projection.entity_type, EntityType::Sarl);
    assert!(projection.dissolved_at.is_none());
}

#[tokio::test]
#[ignore = "requires docker daemon"]
async fn duplicate_identity_tuple_is_refused_at_db() {
    let (_node, repo) = setup_repo().await;
    let usecase = RegisterEntityUseCase::new(repo.clone());
    let first = make_register_cmd("First");
    usecase.execute(first.clone()).await.expect("first register");

    let mut clash = make_register_cmd("Second");
    clash.jurisdiction = first.jurisdiction.clone();
    clash.registration_number_in_jurisdiction =
        first.registration_number_in_jurisdiction.clone();
    let err = usecase.execute(clash).await.expect_err("second must reject");
    let msg = err.to_string();
    assert!(
        msg.contains("duplicate identity tuple") || msg.contains("DuplicateIdentityTuple"),
        "expected duplicate-identity-tuple error, got: {msg}"
    );
}

#[tokio::test]
#[ignore = "requires docker daemon"]
async fn search_finds_by_substring_and_jurisdiction() {
    let (_node, repo) = setup_repo().await;
    let usecase = RegisterEntityUseCase::new(repo.clone());
    usecase
        .execute(make_register_cmd("ACME Mining SARL"))
        .await
        .expect("first");
    usecase
        .execute(make_register_cmd("Beta Logistics SARL"))
        .await
        .expect("second");

    let search = SearchEntitiesUseCase::new(repo.clone());
    let results = search
        .execute(SearchCriteria {
            q: Some("acme".into()),
            jurisdiction: Some("cm".into()),
            entity_type: None,
            limit: 50,
        })
        .await
        .expect("search ok");
    assert_eq!(results.len(), 1);
    assert!(results[0].canonical_name.contains("ACME"));
}
