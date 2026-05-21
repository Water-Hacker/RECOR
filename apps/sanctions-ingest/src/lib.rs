//! TODO-014 — Sanctions / PEP / ICIJ ingestion library.
//!
//! Authoritative sources the verification engine screens against:
//!
//! | Source | Cadence | Format | Crate | Sub-binary |
//! |---|---|---|---|---|
//! | OFAC SDN | Daily | XML | `recor_sanctions_ingest::ofac` | `recor-sanctions-ingest-ofac` |
//! | EU CFSP | Weekly | XML | `recor_sanctions_ingest::eu` (planned) | `recor-sanctions-ingest-eu` (planned) |
//! | UN Consolidated | Irregular | XML | `recor_sanctions_ingest::un` (planned) | `recor-sanctions-ingest-un` (planned) |
//! | ICIJ Offshore Leaks | Per-leak | CSV | `recor_sanctions_ingest::icij` (planned) | `recor-sanctions-ingest-icij` (planned) |
//!
//! The `ofac` sub-binary ships as the canonical demonstration. The
//! shape of the other sub-binaries is identical: fetch → parse →
//! upsert into the screening table → write a per-feed audit row
//! recording the source revision + ingested_at + row delta.
//!
//! Doctrines:
//! - **D14 fail-closed** — when a fetched feed shows a > 25% drop in
//!   row count vs the prior revision, the worker refuses the upsert
//!   and writes a `recor_sanctions_ingest_blocked_total{source}`
//!   metric (the operator confirms the source genuinely changed and
//!   re-runs with `--force`).
//! - **D15 cryptographic provenance** — every fetched feed is hashed
//!   (BLAKE3 of the raw bytes) and the digest is recorded alongside
//!   the source revision in `ingest_log`. A future audit can prove
//!   the platform's view of OFAC SDN at any point in time.
//! - **D17 zero trust** — feeds are fetched over TLS; the response
//!   is shape-validated before any database write.
//! - **D18 no secrets in logs** — feed URLs are config; no API keys
//!   appear in tracing output.

pub mod canonical;
pub mod eu;
pub mod icij;
pub mod ingest_log;
pub mod ofac;
pub mod sanity_check;
pub mod un;

pub use canonical::canonicalise_name;
pub use ingest_log::{write_ingest_log, IngestLogEntry};
pub use sanity_check::{sanity_check, SanityCheckOutcome};
