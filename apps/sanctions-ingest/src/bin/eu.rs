//! TODO-014-EU — EU CFSP consolidated sanctions ingest binary.
//!
//! Usage:
//!   recor-sanctions-ingest-eu \
//!     --source-file <path-to-cfsp.xml> \
//!     [--force "<justification>"] \
//!     [--max-drop-ratio 0.25]
//!
//! When `--source-file` is absent, the binary fetches from
//! `EU_CFSP_URL` (defaults to the European Commission's published
//! endpoint).

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use sqlx::postgres::PgPoolOptions;
use time::OffsetDateTime;
use tracing::{error, info, warn};

use recor_sanctions_ingest::{
    eu::{parse_eu, upsert_eu_entries},
    ingest_log::{write_ingest_log, IngestLogEntry},
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
                let v: f64 = args
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
                    "Usage: recor-sanctions-ingest-eu --source-file <path> [--force <justification>] [--max-drop-ratio 0.25]"
                );
                std::process::exit(0);
            }
            other => anyhow::bail!("unknown arg `{other}`"),
        }
    }
    Ok(Args {
        source_file,
        source_url: std::env::var("EU_CFSP_URL").ok(),
        force_justification,
        max_drop_ratio,
        database_url: std::env::var("DATABASE_URL").context("DATABASE_URL is required")?,
    })
}

#[tokio::main]
async fn main() -> ExitCode {
    let _ = dotenvy::dotenv();
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,recor_sanctions_ingest=debug".into()),
        )
        .try_init();

    match real_main().await {
        Ok(code) => code,
        Err(e) => {
            error!(error = ?e, "eu cfsp ingest failed");
            ExitCode::from(2)
        }
    }
}

async fn real_main() -> Result<ExitCode> {
    let args = parse_args()?;

    let bytes: Vec<u8> = if let Some(path) = args.source_file.as_ref() {
        std::fs::read(path).with_context(|| format!("reading {}", path.display()))?
    } else if let Some(url) = args.source_url.as_ref() {
        info!(%url, "fetching EU CFSP feed");
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .context("reqwest client build")?;
        client
            .get(url)
            .send()
            .await
            .context("EU CFSP fetch")?
            .error_for_status()
            .context("EU CFSP HTTP status")?
            .bytes()
            .await
            .context("EU CFSP body read")?
            .to_vec()
    } else {
        anyhow::bail!("either --source-file or EU_CFSP_URL env must be set");
    };
    let digest_hex = blake3::hash(&bytes).to_hex().to_string();
    info!(
        bytes_read = bytes.len(),
        digest = %&digest_hex[..16],
        "EU CFSP bytes ready"
    );

    let entries = parse_eu(&bytes).context("EU CFSP parse")?;
    let proposed: u64 = entries.len() as u64;
    info!(proposed_row_count = proposed, "EU CFSP parsed");

    let pool = PgPoolOptions::new()
        .max_connections(4)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect(&args.database_url)
        .await
        .context("connecting to Postgres")?;
    let prior_row: (i64,) = sqlx::query_as(
        r#"SELECT COUNT(*)::bigint FROM sanctions_persons WHERE source = 'eu_cfsp'"#,
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

    if blocked && !forced {
        let entry = IngestLogEntry {
            source: "eu_cfsp".to_string(),
            source_revision: source_revision.clone(),
            raw_bytes_digest_hex: digest_hex.clone(),
            prior_row_count: prior,
            proposed_row_count: proposed,
            applied: false,
            force_justification: None,
            ingested_at: OffsetDateTime::now_utc(),
        };
        write_ingest_log(&pool, &entry)
            .await
            .context("write ingest_log row (blocked)")?;
        warn!(
            prior,
            proposed,
            max_drop_ratio = args.max_drop_ratio,
            "EU CFSP ingest BLOCKED by sanity check; re-run with --force '<justification>' to override"
        );
        return Ok(ExitCode::from(6));
    }

    let applied = upsert_eu_entries(&pool, &entries)
        .await
        .context("EU CFSP upsert")?;
    info!(applied, "EU CFSP upsert complete");

    let entry = IngestLogEntry {
        source: "eu_cfsp".to_string(),
        source_revision,
        raw_bytes_digest_hex: digest_hex,
        prior_row_count: prior,
        proposed_row_count: proposed,
        applied: true,
        force_justification: args.force_justification.clone(),
        ingested_at: OffsetDateTime::now_utc(),
    };
    write_ingest_log(&pool, &entry)
        .await
        .context("write ingest_log row (applied)")?;

    Ok(ExitCode::SUCCESS)
}
