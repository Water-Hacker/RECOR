//! TODO-014-ICIJ — ICIJ leak-set ingest binary.
//!
//! Usage:
//!   recor-sanctions-ingest-icij \
//!     --dataset {offshore_leaks|panama|paradise|pandora} \
//!     [--node-kind {person|officer|intermediary|entity}] \
//!     --source-file <path-to.csv> \
//!     [--force "<justification>"] \
//!     [--max-drop-ratio 0.25]
//!
//! `--node-kind` defaults to `person`. ICIJ publishes one CSV per
//! node-kind; the binary is invoked once per CSV.
//!
//! Network fetch is supported via `ICIJ_<DATASET>_URL` (e.g.
//! `ICIJ_PANAMA_URL`), but the operator runbook prefers downloading
//! the per-dataset zip from <https://offshoreleaks.icij.org/pages/database>,
//! unpacking it, and feeding the binary local files. The HTTP path is
//! retained for parity with the other three binaries.

use std::path::PathBuf;
use std::process::ExitCode;
use std::str::FromStr;

use anyhow::{Context, Result};
use sqlx::postgres::PgPoolOptions;
use time::OffsetDateTime;
use tracing::{error, info, warn};

use recor_sanctions_ingest::{
    icij::{parse_icij, upsert_icij_entries, IcijDataset, NodeKind},
    ingest_log::{write_ingest_log, IngestLogEntry},
    sanity_check::{sanity_check, SanityCheckOutcome},
};

#[derive(Debug)]
struct Args {
    dataset: IcijDataset,
    node_kind: NodeKind,
    source_file: Option<PathBuf>,
    source_url: Option<String>,
    force_justification: Option<String>,
    max_drop_ratio: f64,
    database_url: String,
}

fn parse_args() -> Result<Args> {
    let mut args = std::env::args().skip(1);
    let mut dataset: Option<IcijDataset> = None;
    let mut node_kind: NodeKind = NodeKind::Person;
    let mut source_file: Option<PathBuf> = None;
    let mut force_justification: Option<String> = None;
    let mut max_drop_ratio: f64 = 0.25;
    while let Some(a) = args.next() {
        match a.as_str() {
            "--dataset" => {
                let v = args.next().context("--dataset requires a value")?;
                dataset = Some(IcijDataset::from_str(&v).map_err(|e| anyhow::anyhow!(e))?);
            }
            "--node-kind" => {
                let v = args.next().context("--node-kind requires a value")?;
                node_kind = NodeKind::from_str(&v).map_err(|e| anyhow::anyhow!(e))?;
            }
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
                    "Usage: recor-sanctions-ingest-icij --dataset {{offshore_leaks|panama|paradise|pandora}} [--node-kind {{person|officer|intermediary|entity}}] --source-file <path> [--force <justification>] [--max-drop-ratio 0.25]"
                );
                std::process::exit(0);
            }
            other => anyhow::bail!("unknown arg `{other}`"),
        }
    }
    let dataset = dataset.context("--dataset is required")?;
    let source_url_env = match dataset {
        IcijDataset::OffshoreLeaks => "ICIJ_OFFSHORE_LEAKS_URL",
        IcijDataset::Panama => "ICIJ_PANAMA_URL",
        IcijDataset::Paradise => "ICIJ_PARADISE_URL",
        IcijDataset::Pandora => "ICIJ_PANDORA_URL",
    };
    Ok(Args {
        dataset,
        node_kind,
        source_file,
        source_url: std::env::var(source_url_env).ok(),
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
            error!(error = ?e, "icij ingest failed");
            ExitCode::from(2)
        }
    }
}

async fn real_main() -> Result<ExitCode> {
    let args = parse_args()?;
    let source = args.dataset.as_source().to_string();

    let bytes: Vec<u8> = if let Some(path) = args.source_file.as_ref() {
        std::fs::read(path).with_context(|| format!("reading {}", path.display()))?
    } else if let Some(url) = args.source_url.as_ref() {
        info!(%url, dataset = %source, "fetching ICIJ feed");
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .context("reqwest client build")?;
        client
            .get(url)
            .send()
            .await
            .context("ICIJ fetch")?
            .error_for_status()
            .context("ICIJ HTTP status")?
            .bytes()
            .await
            .context("ICIJ body read")?
            .to_vec()
    } else {
        anyhow::bail!("either --source-file or ICIJ_<DATASET>_URL env must be set");
    };
    let digest_hex = blake3::hash(&bytes).to_hex().to_string();
    info!(
        bytes_read = bytes.len(),
        digest = %&digest_hex[..16],
        dataset = %source,
        node_kind = %args.node_kind.as_str(),
        "ICIJ bytes ready"
    );

    let entries = parse_icij(&bytes, args.node_kind).context("ICIJ parse")?;
    let proposed: u64 = entries.len() as u64;
    info!(proposed_row_count = proposed, dataset = %source, "ICIJ parsed");

    let pool = PgPoolOptions::new()
        .max_connections(4)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect(&args.database_url)
        .await
        .context("connecting to Postgres")?;
    // Prior count is per (dataset, node_kind) — operators run one
    // binary per kind, and the sanity gate should not trip when the
    // operator only ingests the `officer` CSV while `person` is empty.
    let prior_row: (i64,) = sqlx::query_as(
        r#"SELECT COUNT(*)::bigint FROM icij_persons WHERE source_dataset = $1 AND node_kind = $2"#,
    )
    .bind(&source)
    .bind(args.node_kind.as_str())
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
            source: source.clone(),
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
            dataset = %source,
            "ICIJ ingest BLOCKED by sanity check; re-run with --force '<justification>' to override"
        );
        return Ok(ExitCode::from(6));
    }

    let applied = upsert_icij_entries(&pool, args.dataset, &entries)
        .await
        .context("ICIJ upsert")?;
    info!(applied, dataset = %source, "ICIJ upsert complete");

    let entry = IngestLogEntry {
        source,
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
