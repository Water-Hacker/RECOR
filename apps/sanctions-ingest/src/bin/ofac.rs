//! TODO-014 — OFAC SDN ingest binary.
//!
//! Usage:
//!   recor-sanctions-ingest-ofac \
//!     --source-file <path-to-sdn.xml> \
//!     [--force "<justification>"] \
//!     [--max-drop-ratio 0.25]
//!
//! When `--source-file` is absent, the binary fetches the canonical
//! URL from `OFAC_SDN_URL` (defaults to the published Treasury
//! endpoint). The dry-run path is the standard local-fixture-based
//! invocation that CI exercises.
//!
//! The binary:
//!   1. Reads/fetches the raw bytes; computes BLAKE3 digest.
//!   2. Counts `<sdnEntry>` tags (the placeholder for parse-count
//!      until the full XML wiring lands in TODO-014-OFAC).
//!   3. Runs `sanity_check` against the prior row count in
//!      `sanctions_persons WHERE source='ofac_sdn'`.
//!   4. On block: writes an `ingest_log` row with `applied=false`,
//!      exits with code 6. With `--force <justification>` the entry
//!      is recorded as forced and the upsert proceeds.
//!   5. On pass: TODO-014-OFAC upsert + `ingest_log` row with
//!      `applied=true`.
//!
//! Today the upsert step is a placeholder logged at INFO; the XML
//! schema model + per-entry upsert are the TODO-014-OFAC follow-up.
//! Everything OUTSIDE the schema model — fetch, digest, sanity
//! check, audit log — is fully wired.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use sqlx::postgres::PgPoolOptions;
use time::OffsetDateTime;
use tracing::{error, info, warn};

use recor_sanctions_ingest::{
    ingest_log::{write_ingest_log, IngestLogEntry},
    ofac::{parse_sdn, upsert_sdn_entries},
    sanity_check::{sanity_check, SanityCheckOutcome},
};

#[derive(Debug)]
struct Args {
    source_file: Option<PathBuf>,
    source_url: Option<String>,
    force_justification: Option<String>,
    max_drop_ratio: f64,
    database_url: String,
}

fn parse_args() -> Result<Args> {
    let mut args = std::env::args().skip(1);
    let mut source_file: Option<PathBuf> = None;
    let mut force_justification: Option<String> = None;
    let mut max_drop_ratio: f64 = 0.25;
    while let Some(a) = args.next() {
        match a.as_str() {
            "--source-file" => {
                source_file = Some(PathBuf::from(
                    args.next().context("--source-file requires a path")?,
                ));
            }
            "--force" => {
                force_justification =
                    Some(args.next().context("--force requires a justification string")?);
            }
            "--max-drop-ratio" => {
                let v = args
                    .next()
                    .context("--max-drop-ratio requires a value")?
                    .parse()
                    .context("--max-drop-ratio must be 0..=1")?;
                if !(0.0..=1.0).contains(&v) {
                    anyhow::bail!("--max-drop-ratio must be 0..=1 (got {v})");
                }
                max_drop_ratio = v;
            }
            "--help" | "-h" => {
                println!(
                    "Usage: recor-sanctions-ingest-ofac --source-file <path> [--force <justification>] [--max-drop-ratio 0.25]"
                );
                std::process::exit(0);
            }
            other => anyhow::bail!("unknown arg `{other}`"),
        }
    }
    Ok(Args {
        source_file,
        source_url: std::env::var("OFAC_SDN_URL").ok(),
        force_justification,
        max_drop_ratio,
        database_url: std::env::var("DATABASE_URL")
            .context("DATABASE_URL is required")?,
    })
}

#[tokio::main]
async fn main() -> ExitCode {
    let _ = dotenvy::dotenv();
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                "info,recor_sanctions_ingest=debug".into()
            }),
        )
        .try_init();

    match real_main().await {
        Ok(code) => code,
        Err(e) => {
            error!(error = ?e, "ofac ingest failed");
            ExitCode::from(2)
        }
    }
}

async fn real_main() -> Result<ExitCode> {
    let args = parse_args()?;

    // 1. Read the raw bytes.
    let bytes: Vec<u8> = if let Some(path) = args.source_file.as_ref() {
        std::fs::read(path)
            .with_context(|| format!("reading {}", path.display()))?
    } else if let Some(url) = args.source_url.as_ref() {
        info!(%url, "fetching OFAC SDN feed");
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .context("reqwest client build")?;
        client
            .get(url)
            .send()
            .await
            .context("OFAC SDN fetch")?
            .error_for_status()
            .context("OFAC SDN HTTP status")?
            .bytes()
            .await
            .context("OFAC SDN body read")?
            .to_vec()
    } else {
        anyhow::bail!(
            "either --source-file or OFAC_SDN_URL env must be set"
        );
    };
    let digest_hex = blake3::hash(&bytes).to_hex().to_string();
    info!(
        bytes_read = bytes.len(),
        digest = %&digest_hex[..16],
        "OFAC SDN bytes ready"
    );

    // 2. Parse the SDN feed.
    let entries = parse_sdn(&bytes).context("OFAC SDN parse")?;
    let proposed: u64 = entries.len() as u64;
    info!(proposed_row_count = proposed, "OFAC SDN parsed");

    // 3. Compare against prior count in the table.
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect(&args.database_url)
        .await
        .context("connecting to Postgres")?;
    let prior_row: (i64,) = sqlx::query_as(
        r#"SELECT COUNT(*)::bigint FROM sanctions_persons WHERE source = 'ofac_sdn'"#,
    )
    .fetch_one(&pool)
    .await
    .context("prior count query")?;
    let prior = prior_row.0.max(0) as u64;

    let outcome = sanity_check(prior, proposed, args.max_drop_ratio);
    let blocked = matches!(outcome, SanityCheckOutcome::Blocked { .. });
    let forced = args.force_justification.is_some();

    let source_revision = OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default();

    let entry = IngestLogEntry {
        source: "ofac_sdn".to_string(),
        source_revision: source_revision.clone(),
        raw_bytes_digest_hex: digest_hex.clone(),
        prior_row_count: prior,
        proposed_row_count: proposed,
        applied: !blocked || forced,
        force_justification: args.force_justification.clone(),
        ingested_at: OffsetDateTime::now_utc(),
    };
    write_ingest_log(&pool, &entry)
        .await
        .context("write ingest_log row")?;

    if blocked && !forced {
        warn!(
            prior,
            proposed,
            max_drop_ratio = args.max_drop_ratio,
            "TODO-014: OFAC ingest BLOCKED by sanity check; re-run with --force '<justification>' to override"
        );
        return Ok(ExitCode::from(6));
    }

    // 4. Per-entry upsert into sanctions_persons in a single tx.
    let applied = upsert_sdn_entries(&pool, &entries)
        .await
        .context("OFAC SDN upsert")?;
    info!(
        upsert_target = "sanctions_persons",
        source = "ofac_sdn",
        applied_rows = applied,
        digest = %&digest_hex[..16],
        "TODO-014-OFAC: OFAC SDN ingest cycle complete"
    );

    Ok(ExitCode::SUCCESS)
}
