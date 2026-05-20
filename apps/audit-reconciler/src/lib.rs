//! `audit-reconciler` — cron job that detects divergence between the
//! local `declaration_events` log and the Fabric audit channel.
//!
//! Closes FIND-016: the audit catalogue flagged that the bridge worker
//! (`apps/worker-fabric-bridge`) can silently fail to anchor an event,
//! and the platform had no automated detector.
//!
//! ## How it works
//!
//! Every `RECONCILE_INTERVAL_SECONDS` (default 600s) the reconciler:
//!
//! 1. Reads `declaration_events` for rows whose `event_time` is older
//!    than `RECONCILE_GRACE_SECONDS` (default 300s — covers normal
//!    bridge dispatch lag) but younger than `RECONCILE_LOOKBACK_SECONDS`
//!    (default 86_400s — one day window). The grace prevents
//!    false-positives on events still in the bridge's retry queue.
//! 2. For each unique `declaration_id` in that batch, calls
//!    `audit-witness`'s `ListAuditEntriesForDeclaration` via the
//!    Fabric Gateway HTTP shim.
//! 3. Diffs the on-chain event_ids against the local set. Every
//!    local event_id NOT on chain is a **divergence**.
//! 4. For each divergence: increments
//!    `recor_audit_reconciliation_divergence_total{event_type=...}`
//!    and emits a structured WARN with the declaration_id +
//!    event_id + event_time.
//! 5. Also publishes
//!    `recor_audit_reconciliation_run_total{outcome=...}` per pass
//!    so the absence of a 0-count run becomes alertable (a stuck
//!    reconciler is itself a finding).
//!
//! ## D-doctrines exercised
//!
//! - **D14 fail-closed.** A Fabric gateway error short-circuits the
//!   run with a `WARN` + `result=error` counter — we do NOT silently
//!   succeed when the on-chain side is unreachable.
//! - **D15 cryptographic provenance.** This module is the read-side
//!   detector that catches the failure mode the bridge worker can
//!   produce; it's load-bearing for the anchoring guarantee.
//! - **D16 observability.** Three counters + a gauge are exported
//!   on `/metrics` for the Prometheus scraper to alert on.
//! - **D18 no secrets.** The Fabric gateway bearer token (if any) is
//!   sourced from env; never logged.

pub mod config;
pub mod handlers;
pub mod metrics;
pub mod reconciler;
pub mod repo;

pub use config::ReconcilerConfig;
pub use metrics::ReconcilerMetrics;
pub use reconciler::{ReconcileError, ReconcileOutcome, ReconcilerLoop};
pub use repo::{EventLogRow, EventLogRepo, PostgresEventLogRepo};
