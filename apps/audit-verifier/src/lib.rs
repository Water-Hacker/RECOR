//! `audit-verifier` — operator / public verification surface.
//!
//! Exposes `GET /v1/audit/verify/{declaration_id}` which:
//!
//! 1. Queries the Fabric audit channel via the Gateway shim's
//!    `ListAuditEntriesForDeclaration` chaincode method.
//! 2. For each entry, fetches the corresponding event from the
//!    Declaration service's projection (via the read-only DB or — when
//!    a `DECLARATION_API_URL` is set — the REST GET endpoint).
//! 3. Re-derives the BLAKE3 receipt hash from the canonical payload
//!    bytes.
//! 4. Asserts the on-chain hash matches.
//! 5. Returns a structured verification report.
//!
//! ## Trust model
//!
//! - The Fabric channel is the trust anchor for the receipt hash.
//! - The Declaration service's projection is the source of the
//!   canonical payload bytes.
//! - A mismatch means EITHER the projection was tampered with OR the
//!   Fabric entry was misanchored — the report calls out the discrepancy
//!   without taking a side. Operators decide.
//!
//! ## Auth
//!
//! Public read. The endpoint is OIDC-gated behind the platform's
//! identity provider (recor-auth-oidc), but the verification result
//! itself is not personally identifying (the on-chain entry contains
//! only IDs + the hash). A future ticket may add anonymisation /
//! rate-limit-by-IP for fully-public verification.
//!
//! ## Doctrines
//!
//! - **D14 fail-closed**: when Fabric is unreachable the verifier
//!   returns 503 rather than a "could not verify" success; when a
//!   single entry's hash mismatches, the report status is "tampered"
//!   not "partial".
//! - **D15 cryptographic provenance**: this app is the read-side of
//!   the load-bearing doctrine.

pub mod config;
pub mod fabric_client;
pub mod handlers;
pub mod hashing;
pub mod projection;
pub mod report;

pub use config::VerifierConfig;
pub use fabric_client::{FabricClient, FabricClientError, OnChainEntry};
pub use hashing::derive_receipt_hash;
pub use projection::{ProjectionRepo, ProjectionRow};
pub use report::{EntryStatus, VerificationReport, VerificationResult};
