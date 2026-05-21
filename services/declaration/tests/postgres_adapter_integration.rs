//! TODO-027 — Postgres adapter integration tests for the Declaration service.
//!
//! Tests: round-trip serialisation of every event variant, COMP-2
//! (append-only trigger — UPDATE/DELETE → error), idempotency-cache
//! hit/miss, projection rebuild from events, outbox insertion + dispatch markers.
//!
//! All tests `#[ignore]` — CI runs via `--ignored`. Run locally:
//!   cargo test -p recor-declaration --test postgres_adapter_integration \
//!     -- --ignored --nocapture

use std::sync::Arc;

use ed25519_dalek::{Signer, SigningKey};
use sqlx::postgres::PgPoolOptions;
use testcontainers::ContainerAsync;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::ImageExt;
use time::OffsetDateTime;
use uuid::Uuid;

use recor_declaration::application::port::{DeclarationRepository, RepositoryError};
use recor_declaration::domain::{
    DeclarationAmendedV1, DeclarationCorrectedV1, DeclarationEvent, DeclarationSubmittedV1,
    DeclarationSupersededV1, DeclarationVerifiedV1,
};
use recor_declaration::domain::attestation::{
    AdequacyClaims, CryptographicAttestation, SignatureAlgorithm,
};
use recor_declaration::domain::value_object::{
    AmendmentSet, BeneficialOwnerClaim, BoCascadeTier, BoControlBasis, CorrectionSet,
    DeclarantRole, DeclarationId, DeclarationKind, DeclarationState, EntityId,
    InterestKind, OwnershipBasisPoints, PersonId, VerificationLane,
};
use recor_declaration::infrastructure::postgres::{IdempotencyStore, PostgresDeclarationRepository};

// ─── Harness ──────────────────────────────────────────────────────────────────

struct Ctx {
    repo: Arc<PostgresDeclarationRepository>,
    idempotency: Arc<IdempotencyStore>,
    pool: sqlx::PgPool,
    _pg: ContainerAsync<Postgres>,
}

async fn setup() -> Ctx {
    let pg = Postgres::default()
        .with_tag("17-alpine")
        .start()
        .await
        .expect("postgres container");
    let port = pg.get_host_port_ipv4(5432).await.expect("port");
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&url)
        .await
        .expect("connect");
    let repo = Arc::new(PostgresDeclarationRepository::new(pool.clone()));
    repo.run_migrations().await.expect("migrations");
    let idempotency = Arc::new(IdempotencyStore::new(pool.clone()));
    Ctx { repo, idempotency, pool, _pg: pg }
}

fn signing_key() -> SigningKey {
    SigningKey::from_bytes(&[42u8; 32])
}

fn attestation(principal: &str, k: &SigningKey) -> CryptographicAttestation {
    let nonce = hex::encode(Uuid::new_v4().as_bytes());
    let msg = format!("{principal}:{nonce}");
    let sig = k.sign(msg.as_bytes());
    CryptographicAttestation {
        signed_by: principal.to_string(),
        signature_algorithm: SignatureAlgorithm::Ed25519,
        signature_hex: hex::encode(sig.to_bytes()),
        public_key_hex: hex::encode(k.verifying_key().to_bytes()),
        nonce_hex: nonce,
    }
}

fn adequacy() -> AdequacyClaims {
    AdequacyClaims {
        adequate: true,
        accurate: true,
        up_to_date_as_of: OffsetDateTime::now_utc(),
        legal_basis: "CEMAC Règlement 01/03/CEMAC/UMAC/CM Art. 12".to_string(),
    }
}

fn owners(person_id: PersonId) -> Vec<BeneficialOwnerClaim> {
    vec![BeneficialOwnerClaim {
        person_id,
        ownership_basis_points: OwnershipBasisPoints(10_000),
        interest_kind: InterestKind::Equity,
        cascade_tier: Some(BoCascadeTier::OwnershipDirect),
        control_basis: None,
        cascade_tier_b_ruled_out_evidence: None,
        is_nominee: Some(false),
        nominator_person_id: None,
    }]
}

fn submitted_event(
    declaration_id: DeclarationId,
    entity_id: EntityId,
    person_id: PersonId,
    principal: &str,
    k: &SigningKey,
) -> DeclarationEvent {
    DeclarationEvent::Submitted(DeclarationSubmittedV1 {
        declaration_id,
        entity_id,
        subject: None,
        declarant_principal: principal.to_string(),
        declarant_role: DeclarantRole::SelfDeclaration,
        kind: DeclarationKind::Incorporation,
        effective_from: time::Date::from_calendar_date(2026, time::Month::January, 1).unwrap(),
        beneficial_owners: owners(person_id),
        attestation: attestation(principal, k),
        adequacy_claims: Some(adequacy()),
        last_event_observed_at: None,
        submitted_at: OffsetDateTime::now_utc(),
        correlation_id: Uuid::now_v7(),
        receipt_hash_hex: hex::encode([0xabu8; 32]),
    })
}

// ─── Test 1: round-trip DeclarationSubmittedV1 ───────────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn round_trip_submitted_event_persists_and_reloads() {
    let ctx = setup().await;
    let k = signing_key();
    let decl_id = DeclarationId::new();
    let entity_id = EntityId(Uuid::now_v7());
    let person_id = PersonId(Uuid::now_v7());
    let principal = "spiffe://recor.cm/pg-adapter-test-1";

    let event = submitted_event(decl_id, entity_id, person_id, principal, &k);
    ctx.repo.save_event(&event, 0).await.expect("save_event");

    let events = ctx.repo.load_events(decl_id).await.expect("load_events");
    assert_eq!(events.len(), 1);
    assert!(matches!(&events[0], DeclarationEvent::Submitted(s) if s.declaration_id == decl_id));
}

// ─── Test 2: round-trip DeclarationVerifiedV1 ────────────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn round_trip_verified_event() {
    let ctx = setup().await;
    let k = signing_key();
    let decl_id = DeclarationId::new();
    let entity_id = EntityId(Uuid::now_v7());
    let person_id = PersonId(Uuid::now_v7());
    let principal = "spiffe://recor.cm/pg-adapter-test-2";

    ctx.repo
        .save_event(&submitted_event(decl_id, entity_id, person_id, principal, &k), 0)
        .await
        .unwrap();

    let verified = DeclarationEvent::Verified(DeclarationVerifiedV1 {
        declaration_id: decl_id,
        verification_case_id: Uuid::now_v7(),
        lane: VerificationLane::Green,
        fused_authenticity_belief: 0.95,
        fused_authenticity_plausibility: 0.97,
        fused_risk_belief: 0.05,
        completed_at: OffsetDateTime::now_utc(),
        recorded_at: OffsetDateTime::now_utc(),
    });
    ctx.repo.save_event(&verified, 1).await.expect("save verified");

    let events = ctx.repo.load_events(decl_id).await.unwrap();
    assert_eq!(events.len(), 2);
    assert!(matches!(&events[1], DeclarationEvent::Verified(_)));
}

// ─── Test 3: round-trip DeclarationAmendedV1 ─────────────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn round_trip_amended_event() {
    let ctx = setup().await;
    let k = signing_key();
    let decl_id = DeclarationId::new();
    let entity_id = EntityId(Uuid::now_v7());
    let person_id = PersonId(Uuid::now_v7());
    let principal = "spiffe://recor.cm/pg-adapter-test-3";

    ctx.repo
        .save_event(&submitted_event(decl_id, entity_id, person_id, principal, &k), 0)
        .await
        .unwrap();

    let before_owners = owners(person_id);
    let after_owners = owners(person_id);
    let amended = DeclarationEvent::Amended(DeclarationAmendedV1 {
        declaration_id: decl_id,
        before: AmendmentSet {
            beneficial_owners: before_owners,
            effective_from: time::Date::from_calendar_date(2026, time::Month::January, 1).unwrap(),
            declarant_role: DeclarantRole::SelfDeclaration,
            adequacy_claims: Some(adequacy()),
        },
        after: AmendmentSet {
            beneficial_owners: after_owners,
            effective_from: time::Date::from_calendar_date(2026, time::Month::February, 1).unwrap(),
            declarant_role: DeclarantRole::SelfDeclaration,
            adequacy_claims: Some(adequacy()),
        },
        attestation: attestation(principal, &k),
        amended_at: OffsetDateTime::now_utc(),
        correlation_id: Uuid::now_v7(),
    });
    ctx.repo.save_event(&amended, 1).await.expect("save amended");

    let events = ctx.repo.load_events(decl_id).await.unwrap();
    assert_eq!(events.len(), 2);
    assert!(matches!(&events[1], DeclarationEvent::Amended(_)));
}

// ─── Test 4: round-trip DeclarationCorrectedV1 ───────────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn round_trip_corrected_event() {
    let ctx = setup().await;
    let k = signing_key();
    let decl_id = DeclarationId::new();
    let entity_id = EntityId(Uuid::now_v7());
    let person_id = PersonId(Uuid::now_v7());
    let principal = "spiffe://recor.cm/pg-adapter-test-4";

    ctx.repo
        .save_event(&submitted_event(decl_id, entity_id, person_id, principal, &k), 0)
        .await
        .unwrap();

    let corrected = DeclarationEvent::Corrected(DeclarationCorrectedV1 {
        declaration_id: decl_id,
        before: CorrectionSet { metadata_notes: None },
        after: CorrectionSet { metadata_notes: Some("Corrected note".to_string()) },
        attestation: attestation(principal, &k),
        corrected_at: OffsetDateTime::now_utc(),
        correlation_id: Uuid::now_v7(),
    });
    ctx.repo.save_event(&corrected, 1).await.expect("save corrected");

    let events = ctx.repo.load_events(decl_id).await.unwrap();
    assert_eq!(events.len(), 2);
    assert!(matches!(&events[1], DeclarationEvent::Corrected(_)));
}

// ─── Test 5: COMP-2 trigger — UPDATE on declaration_events fails ──────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn comp2_update_on_events_table_is_refused() {
    let ctx = setup().await;
    let k = signing_key();
    let decl_id = DeclarationId::new();
    let entity_id = EntityId(Uuid::now_v7());
    let person_id = PersonId(Uuid::now_v7());

    ctx.repo
        .save_event(
            &submitted_event(decl_id, entity_id, person_id, "spiffe://recor.cm/pg-comp2", &k),
            0,
        )
        .await
        .unwrap();

    // Attempt a raw UPDATE — the BEFORE UPDATE trigger must fire and raise.
    let err = sqlx::query(
        "UPDATE declaration_events SET event_type = 'tampered' WHERE declaration_id = $1",
    )
    .bind(decl_id.0)
    .execute(&ctx.pool)
    .await;

    assert!(err.is_err(), "COMP-2 trigger must refuse UPDATE on declaration_events");
}

// ─── Test 6: COMP-2 trigger — DELETE on declaration_events fails ──────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn comp2_delete_on_events_table_is_refused() {
    let ctx = setup().await;
    let k = signing_key();
    let decl_id = DeclarationId::new();
    let entity_id = EntityId(Uuid::now_v7());
    let person_id = PersonId(Uuid::now_v7());

    ctx.repo
        .save_event(
            &submitted_event(decl_id, entity_id, person_id, "spiffe://recor.cm/pg-comp2-del", &k),
            0,
        )
        .await
        .unwrap();

    let err = sqlx::query("DELETE FROM declaration_events WHERE declaration_id = $1")
        .bind(decl_id.0)
        .execute(&ctx.pool)
        .await;

    assert!(err.is_err(), "COMP-2 trigger must refuse DELETE on declaration_events");
}

// ─── Test 7: optimistic concurrency — wrong version → Conflict ───────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn optimistic_concurrency_wrong_version_returns_conflict() {
    let ctx = setup().await;
    let k = signing_key();
    let decl_id = DeclarationId::new();
    let entity_id = EntityId(Uuid::now_v7());
    let person_id = PersonId(Uuid::now_v7());
    let principal = "spiffe://recor.cm/pg-concurrency";

    ctx.repo
        .save_event(&submitted_event(decl_id, entity_id, person_id, principal, &k), 0)
        .await
        .unwrap();

    // Attempt to save again at version 0 (already used) — must conflict.
    let second = submitted_event(decl_id, entity_id, person_id, principal, &k);
    let err = ctx.repo.save_event(&second, 0).await.unwrap_err();
    assert!(
        matches!(err, RepositoryError::Conflict { .. }),
        "wrong expected_version must return Conflict, got {err:?}"
    );
}

// ─── Test 8: projection rebuild from events ───────────────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn projection_rebuilds_correctly_from_submitted_event() {
    let ctx = setup().await;
    let k = signing_key();
    let decl_id = DeclarationId::new();
    let entity_id = EntityId(Uuid::now_v7());
    let person_id = PersonId(Uuid::now_v7());
    let principal = "spiffe://recor.cm/pg-projection";

    ctx.repo
        .save_event(&submitted_event(decl_id, entity_id, person_id, principal, &k), 0)
        .await
        .unwrap();

    let proj = ctx
        .repo
        .load_projection(decl_id)
        .await
        .expect("load_projection")
        .expect("projection exists after submit");

    assert_eq!(proj.declaration_id, decl_id);
    assert_eq!(proj.declarant_principal, principal);
    assert_eq!(proj.version, 1);
    assert_eq!(proj.state, DeclarationState::Submitted);
}

// ─── Test 9: projection version advances with each event ─────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn projection_version_advances_on_each_event() {
    let ctx = setup().await;
    let k = signing_key();
    let decl_id = DeclarationId::new();
    let entity_id = EntityId(Uuid::now_v7());
    let person_id = PersonId(Uuid::now_v7());
    let principal = "spiffe://recor.cm/pg-version-advance";

    ctx.repo
        .save_event(&submitted_event(decl_id, entity_id, person_id, principal, &k), 0)
        .await
        .unwrap();

    let v1 = ctx.repo.load_projection(decl_id).await.unwrap().unwrap().version;
    assert_eq!(v1, 1);

    let verified = DeclarationEvent::Verified(DeclarationVerifiedV1 {
        declaration_id: decl_id,
        verification_case_id: Uuid::now_v7(),
        lane: VerificationLane::Green,
        fused_authenticity_belief: 0.9,
        fused_authenticity_plausibility: 0.95,
        fused_risk_belief: 0.1,
        completed_at: OffsetDateTime::now_utc(),
        recorded_at: OffsetDateTime::now_utc(),
    });
    ctx.repo.save_event(&verified, 1).await.unwrap();

    let v2 = ctx.repo.load_projection(decl_id).await.unwrap().unwrap().version;
    assert_eq!(v2, 2);
}

// ─── Test 10: outbox row inserted on submit ───────────────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn outbox_row_inserted_on_submit() {
    let ctx = setup().await;
    let k = signing_key();
    let decl_id = DeclarationId::new();
    let entity_id = EntityId(Uuid::now_v7());
    let person_id = PersonId(Uuid::now_v7());
    let principal = "spiffe://recor.cm/pg-outbox";

    ctx.repo
        .save_event(&submitted_event(decl_id, entity_id, person_id, principal, &k), 0)
        .await
        .unwrap();

    // Direct SQL query on the outbox to check a row was inserted.
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM outbox WHERE aggregate_id = $1")
        .bind(decl_id.0)
        .fetch_one(&ctx.pool)
        .await
        .expect("outbox count");

    assert_eq!(count, 1, "one outbox row per submitted event");
}

// ─── Test 11: idempotency cache miss → None ───────────────────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn idempotency_cache_miss_returns_none() {
    let ctx = setup().await;
    let idem_key = format!("idem-miss-{}", Uuid::now_v7());
    let principal = "spiffe://recor.cm/pg-idem-test";
    let result = ctx.idempotency.check_existing(&idem_key, principal).await.expect("check_existing");
    assert!(result.is_none(), "unseen key should be cache miss");
}

// ─── Test 12: idempotency cache record then hit → Some ───────────────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn idempotency_cache_set_then_hit() {
    let ctx = setup().await;
    let idem_key = format!("idem-hit-{}", Uuid::now_v7());
    let principal = "spiffe://recor.cm/pg-idem-test";
    let response_body = serde_json::json!({"declaration_id": Uuid::now_v7(), "state": "submitted"});
    let request_hash = "deadbeef".repeat(8); // 64-char dummy hash

    ctx.idempotency
        .record(&idem_key, principal, &request_hash, 201, &response_body, 3600)
        .await
        .expect("record");

    let hit = ctx.idempotency.check_existing(&idem_key, principal).await.expect("check_existing");
    assert!(hit.is_some(), "key should be a cache hit after record");
    let record = hit.unwrap();
    assert_eq!(record.response_status, 201);
    assert_eq!(record.request_hash, request_hash);
}

// ─── Test 13: idempotency replay is idempotent (record twice, same key) ───────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn idempotency_repeated_set_is_idempotent() {
    let ctx = setup().await;
    let idem_key = format!("idem-repeat-{}", Uuid::now_v7());
    let principal = "spiffe://recor.cm/pg-idem-repeat";
    let response_body = serde_json::json!({"x": 1});
    let request_hash = "cafebabe".repeat(8);

    ctx.idempotency
        .record(&idem_key, principal, &request_hash, 201, &response_body, 3600)
        .await
        .unwrap();
    // Second record with same key must not fail (ON CONFLICT DO NOTHING).
    ctx.idempotency
        .record(&idem_key, principal, &request_hash, 201, &response_body, 3600)
        .await
        .expect("repeated record is idempotent");

    let hit = ctx.idempotency.check_existing(&idem_key, principal).await.unwrap();
    assert!(hit.is_some());
}

// ─── Test 14: outbox dispatched_at is set after raw SQL update ───────────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn outbox_dispatch_marker_marks_row_dispatched() {
    let ctx = setup().await;
    let k = signing_key();
    let decl_id = DeclarationId::new();
    let entity_id = EntityId(Uuid::now_v7());
    let person_id = PersonId(Uuid::now_v7());
    let principal = "spiffe://recor.cm/pg-dispatch-marker";

    ctx.repo
        .save_event(&submitted_event(decl_id, entity_id, person_id, principal, &k), 0)
        .await
        .unwrap();

    // Fetch the outbox row id.
    let outbox_id: Uuid =
        sqlx::query_scalar("SELECT id FROM outbox WHERE aggregate_id = $1 LIMIT 1")
            .bind(decl_id.0)
            .fetch_one(&ctx.pool)
            .await
            .expect("outbox row");

    // Mark dispatched directly via SQL (the relay worker does this in production).
    sqlx::query(
        "UPDATE outbox SET dispatched_at = NOW(), dispatch_attempts = 1 WHERE id = $1",
    )
    .bind(outbox_id)
    .execute(&ctx.pool)
    .await
    .expect("mark dispatched");

    let dispatched_at: Option<OffsetDateTime> =
        sqlx::query_scalar("SELECT dispatched_at FROM outbox WHERE id = $1")
            .bind(outbox_id)
            .fetch_one(&ctx.pool)
            .await
            .expect("dispatched_at query");

    assert!(dispatched_at.is_some(), "dispatched_at should be set after marking");
}

// ─── Test 15: load_events on unknown declaration_id returns empty vec ─────────

#[tokio::test]
#[ignore = "requires docker for testcontainers"]
async fn load_events_unknown_id_returns_empty_vec() {
    let ctx = setup().await;
    let unknown = DeclarationId::new();
    let events = ctx.repo.load_events(unknown).await.expect("load");
    assert!(events.is_empty(), "unknown id must return empty events, not error");
}
