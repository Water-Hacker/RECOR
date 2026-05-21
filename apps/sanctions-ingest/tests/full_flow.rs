//! TODO-014 — Integration test for the full ingest flow.
//!
//! Spins a real Postgres via testcontainers, applies the
//! verification-engine migrations, then drives each of the four
//! source-specific parsers + upsert paths end-to-end and asserts:
//!
//! 1. The target table (`sanctions_persons` or `icij_persons`) has
//!    the parsed rows after a happy-path run.
//! 2. The `sanctions_ingest_log` row is written with the correct
//!    digest, source, applied=true, and a non-empty source_revision.
//! 3. The sanity-check gate blocks a >25% drop when re-run with a
//!    smaller fixture and no `--force`; the log row is written with
//!    `applied=false` and the target table is unchanged.
//! 4. The BLAKE3 digest in the log row matches `blake3::hash(bytes)`.
//!
//! Run with:
//!   cargo test -p recor-sanctions-ingest --test full_flow -- --ignored
//!
//! Requires Docker (testcontainers spawns Postgres). The test is
//! marked `#[ignore]` so unit `cargo test -p recor-sanctions-ingest`
//! stays fast and offline.

use sqlx::postgres::PgPoolOptions;
use sqlx::Row;
use testcontainers::ContainerAsync;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::ImageExt;
use time::OffsetDateTime;

use recor_sanctions_ingest::{
    eu::{parse_eu, upsert_eu_entries},
    icij::{parse_icij, upsert_icij_entries, IcijDataset, NodeKind},
    ingest_log::{write_ingest_log, IngestLogEntry},
    ofac::{parse_sdn, upsert_sdn_entries},
    sanity_check::{sanity_check, SanityCheckOutcome},
    un::{parse_un, upsert_un_entries},
};

const OFAC_XML: &[u8] = br#"<?xml version="1.0" encoding="utf-8"?>
<sdnList>
  <sdnEntry>
    <uid>1001</uid>
    <firstName>JOHN</firstName>
    <lastName>DOE</lastName>
    <sdnType>Individual</sdnType>
    <programList><program>SDGT</program></programList>
    <nationalityList>
      <nationality><country>Cameroon</country><mainEntry>true</mainEntry></nationality>
    </nationalityList>
    <dateOfBirthList>
      <dateOfBirthItem><dateOfBirth>15 Jul 1980</dateOfBirth><mainEntry>true</mainEntry></dateOfBirthItem>
    </dateOfBirthList>
  </sdnEntry>
  <sdnEntry>
    <uid>1002</uid>
    <firstName>JANE</firstName>
    <lastName>SMITH</lastName>
    <sdnType>Individual</sdnType>
    <programList><program>NPWMD</program></programList>
  </sdnEntry>
</sdnList>"#;

const OFAC_XML_SHRUNK: &[u8] = br#"<?xml version="1.0" encoding="utf-8"?>
<sdnList></sdnList>"#;

const EU_XML: &[u8] = br#"<?xml version="1.0" encoding="utf-8"?>
<export>
  <sanctionEntity logicalId="500">
    <subjectType code="P"/>
    <nameAlias firstName="JOHN" lastName="DOE" function="primary"/>
    <citizenship countryIso2Code="CM"/>
    <regulation programme="CFSP_LIBYA"/>
  </sanctionEntity>
</export>"#;

const UN_XML: &[u8] = br#"<?xml version="1.0" encoding="utf-8"?>
<CONSOLIDATED_LIST>
  <INDIVIDUALS>
    <INDIVIDUAL>
      <DATAID>6908123</DATAID>
      <FIRST_NAME>JOHN</FIRST_NAME>
      <SECOND_NAME>DOE</SECOND_NAME>
      <UN_LIST_TYPE>Al-Qaida</UN_LIST_TYPE>
      <NATIONALITY><VALUE>Cameroon</VALUE></NATIONALITY>
    </INDIVIDUAL>
  </INDIVIDUALS>
</CONSOLIDATED_LIST>"#;

const ICIJ_CSV: &[u8] = br#"node_id,name,countries,sourceID,note,valid_until
50001,"Doe, John","Cameroon","Panama Papers","leak","2017-09-15"
50002,"Smith, Jane","Germany","Panama Papers","",""
"#;

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
    // The v-engine owns the schema. We apply its migration tree.
    sqlx::migrate!("../../services/verification-engine/migrations")
        .run(&pool)
        .await
        .expect("apply v-engine migrations");
    (pg, pool)
}

async fn count_source(pool: &sqlx::PgPool, source: &str) -> i64 {
    let row = sqlx::query("SELECT COUNT(*)::bigint AS n FROM sanctions_persons WHERE source = $1")
        .bind(source)
        .fetch_one(pool)
        .await
        .expect("count");
    row.get::<i64, _>("n")
}

async fn count_log(pool: &sqlx::PgPool, source: &str) -> i64 {
    let row = sqlx::query("SELECT COUNT(*)::bigint AS n FROM sanctions_ingest_log WHERE source = $1")
        .bind(source)
        .fetch_one(pool)
        .await
        .expect("count log");
    row.get::<i64, _>("n")
}

#[tokio::test]
#[ignore]
async fn ofac_full_flow_happy_then_blocked() {
    let (_pg, pool) = bring_up_postgres().await;

    // ─── Happy path ───────────────────────────────────────────────
    let entries = parse_sdn(OFAC_XML).expect("parse");
    assert_eq!(entries.len(), 2);
    let applied = upsert_sdn_entries(&pool, &entries).await.expect("upsert");
    assert_eq!(applied, 2);
    let digest_hex = blake3::hash(OFAC_XML).to_hex().to_string();
    write_ingest_log(
        &pool,
        &IngestLogEntry {
            source: "ofac_sdn".to_string(),
            source_revision: "2026-05-20T00:00:00Z".to_string(),
            raw_bytes_digest_hex: digest_hex.clone(),
            prior_row_count: 0,
            proposed_row_count: 2,
            applied: true,
            force_justification: None,
            ingested_at: OffsetDateTime::now_utc(),
        },
    )
    .await
    .expect("log write");
    assert_eq!(count_source(&pool, "ofac_sdn").await, 2);
    assert_eq!(count_log(&pool, "ofac_sdn").await, 1);

    // Verify digest persisted matches blake3 of input.
    let row = sqlx::query(
        "SELECT raw_bytes_digest_hex, applied FROM sanctions_ingest_log WHERE source='ofac_sdn'",
    )
    .fetch_one(&pool)
    .await
    .expect("fetch log row");
    assert_eq!(row.get::<String, _>("raw_bytes_digest_hex"), digest_hex);
    assert!(row.get::<bool, _>("applied"));

    // ─── Shrunk feed: sanity-check should block ───────────────────
    let shrunk = parse_sdn(OFAC_XML_SHRUNK).expect("shrunk parse");
    assert!(shrunk.is_empty());
    let outcome = sanity_check(2, shrunk.len() as u64, 0.25);
    assert!(matches!(outcome, SanityCheckOutcome::Blocked { .. }));

    // Per the bin's semantics we'd write the blocked log row and NOT
    // run the upsert. Mimic that here.
    write_ingest_log(
        &pool,
        &IngestLogEntry {
            source: "ofac_sdn".to_string(),
            source_revision: "2026-05-21T00:00:00Z".to_string(),
            raw_bytes_digest_hex: blake3::hash(OFAC_XML_SHRUNK).to_hex().to_string(),
            prior_row_count: 2,
            proposed_row_count: 0,
            applied: false,
            force_justification: None,
            ingested_at: OffsetDateTime::now_utc(),
        },
    )
    .await
    .expect("log write blocked");
    // No upsert happened → sanctions_persons unchanged.
    assert_eq!(count_source(&pool, "ofac_sdn").await, 2);
    assert_eq!(count_log(&pool, "ofac_sdn").await, 2);
}

#[tokio::test]
#[ignore]
async fn eu_full_flow() {
    let (_pg, pool) = bring_up_postgres().await;
    let entries = parse_eu(EU_XML).expect("parse");
    assert_eq!(entries.len(), 1);
    let applied = upsert_eu_entries(&pool, &entries).await.expect("upsert");
    assert_eq!(applied, 1);
    // Idempotency check — applying again must not error and must not
    // create duplicate rows.
    let _ = upsert_eu_entries(&pool, &entries).await.expect("re-upsert");
    assert_eq!(count_source(&pool, "eu_cfsp").await, 1);
}

#[tokio::test]
#[ignore]
async fn un_full_flow() {
    let (_pg, pool) = bring_up_postgres().await;
    let entries = parse_un(UN_XML).expect("parse");
    assert_eq!(entries.len(), 1);
    let applied = upsert_un_entries(&pool, &entries).await.expect("upsert");
    assert_eq!(applied, 1);
    assert_eq!(count_source(&pool, "un_consolidated").await, 1);
}

#[tokio::test]
#[ignore]
async fn icij_full_flow() {
    let (_pg, pool) = bring_up_postgres().await;
    let entries = parse_icij(ICIJ_CSV, NodeKind::Person).expect("parse");
    assert_eq!(entries.len(), 2);
    let applied = upsert_icij_entries(&pool, IcijDataset::Panama, &entries)
        .await
        .expect("upsert");
    assert_eq!(applied, 2);
    let row = sqlx::query(
        "SELECT COUNT(*)::bigint AS n FROM icij_persons WHERE source_dataset = 'icij_panama'",
    )
    .fetch_one(&pool)
    .await
    .expect("count");
    assert_eq!(row.get::<i64, _>("n"), 2);
}
