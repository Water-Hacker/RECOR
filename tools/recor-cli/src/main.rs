//! `recor-cli` binary entrypoint.
//!
//! See `lib.rs` for the doctrine notes and the cross-environment
//! configuration contract.

use std::process::ExitCode;
use std::time::Duration;

use anyhow::{Context as _, Result};
use clap::{Parser, Subcommand};
use recor_cli::{command, http_client, CliConfig, Service};

#[derive(Parser, Debug)]
#[command(
    name = "recor-cli",
    version,
    about = "RÉCOR operator CLI — health, verify, sanctions search, DLQ admin",
    long_about = "\
RÉCOR operator CLI. Closes audit-catalogue ticket TODO-056.\n\
\n\
Environment:\n  \
RECOR_API_BASE_URL   Required. Base URL for the platform's public ingress.\n  \
RECOR_TOKEN          Required for admin-gated commands (sanctions / DLQ).\n  \
RECOR_TIMEOUT_SECS   Optional. Per-request timeout (default 30s).\n\
\n\
Examples:\n  \
recor-cli health declaration\n  \
recor-cli verify 018f0000-0000-7000-8000-000000000001\n  \
RECOR_TOKEN=$(get-admin-token) recor-cli admin dlq list verification-engine\n"
)]
struct Cli {
    /// Override `RECOR_API_BASE_URL`. Must be the full origin
    /// (`https://api.example.test`) — the CLI appends the service
    /// prefix itself.
    #[arg(long, env = "RECOR_API_BASE_URL", global = true)]
    base_url: Option<String>,
    /// Override `RECOR_TOKEN`. Bearer token attached to admin-gated
    /// requests. Never logged.
    #[arg(long, env = "RECOR_TOKEN", global = true, hide_env_values = true)]
    token: Option<String>,
    /// Per-request timeout in seconds. Sub-second values are rejected.
    #[arg(long, env = "RECOR_TIMEOUT_SECS", global = true, default_value_t = 30)]
    timeout_secs: u64,
    #[command(subcommand)]
    cmd: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// GET /healthz on the named service.
    Health {
        /// Service name: declaration, verification-engine, person,
        /// entity, audit-verifier.
        service: String,
    },
    /// Call the audit verifier for a declaration and print the report.
    Verify {
        /// Declaration ID (UUID).
        declaration_id: String,
    },
    /// Sanctions adapter on the verification engine.
    Sanctions {
        #[command(subcommand)]
        action: SanctionsAction,
    },
    /// Admin surface — sanctions, DLQ.
    Admin {
        #[command(subcommand)]
        action: AdminAction,
    },
}

#[derive(Subcommand, Debug)]
enum SanctionsAction {
    /// Search the v-engine sanctions adapter for a name. Admin-gated.
    Search {
        /// Free-form name string.
        name: String,
    },
}

#[derive(Subcommand, Debug)]
enum AdminAction {
    /// DLQ admin operations. Admin-gated.
    Dlq {
        #[command(subcommand)]
        action: DlqAction,
    },
}

#[derive(Subcommand, Debug)]
enum DlqAction {
    /// List dead-lettered rows for a service.
    List {
        /// Service name: declaration, verification-engine.
        service: String,
    },
    /// Atomically move a dead-lettered row back onto the outbox.
    Replay {
        /// Service name: declaration, verification-engine.
        service: String,
        /// Row id (UUID).
        id: String,
    },
}

#[tokio::main]
async fn main() -> ExitCode {
    // Tracing for the CLI is opt-in via `RUST_LOG`. The CLI's
    // default-quiet behaviour is correct for operator use; structured
    // logs are emitted when an operator explicitly asks for them
    // (e.g. inside a debugging session).
    if std::env::var_os("RUST_LOG").is_some() {
        tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .with_writer(std::io::stderr)
            .init();
    }
    let cli = Cli::parse();
    match run(cli).await {
        Ok(out) => {
            println!("{out}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            // Print the full anyhow chain so operators see the
            // server's body, the URL, and the wrapping context all
            // at once.
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> Result<String> {
    let cfg = CliConfig::builder()
        .base_url(cli.base_url.clone().unwrap_or_default())
        .token(cli.token.clone())
        .timeout(Duration::from_secs(cli.timeout_secs))
        .build()
        .context("build CLI configuration")?;
    let http = http_client(&cfg)?;

    match cli.cmd {
        Command::Health { service } => {
            let svc = Service::parse(&service)?;
            command::health(&cfg, &http, svc).await
        }
        Command::Verify { declaration_id } => command::verify(&cfg, &http, &declaration_id).await,
        Command::Sanctions {
            action: SanctionsAction::Search { name },
        } => command::sanctions_search(&cfg, &http, &name).await,
        Command::Admin {
            action: AdminAction::Dlq { action },
        } => match action {
            DlqAction::List { service } => {
                let svc = Service::parse(&service)?;
                command::dlq_list(&cfg, &http, svc).await
            }
            DlqAction::Replay { service, id } => {
                let svc = Service::parse(&service)?;
                command::dlq_replay(&cfg, &http, svc, &id).await
            }
        },
    }
}
