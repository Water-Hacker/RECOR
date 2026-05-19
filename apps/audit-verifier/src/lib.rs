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
//! FIND-001 (audit Sprint 0): the verification report embeds the
//! declaration's projection payload (declarant name, beneficial-
//! ownership graph) and is therefore PII-bearing. The verify endpoint
//! is OIDC-gated behind the platform's identity provider
//! (recor-auth-oidc). The pre-Sprint-0 deployment had the endpoint
//! reachable unauthenticated under the (incorrect) framing that the
//! response was non-identifying; that framing is wrong because the
//! response includes the projection body. A follow-up ticket may add
//! a separate anonymised "public-receipt re-derivation" surface that
//! returns only `{declaration_id, on_chain_hash, derived_hash,
//! match}` for fully-public verification without PII exposure.
//!
//! ## Doctrines
//!
//! - **D14 fail-closed**: when Fabric is unreachable the verifier
//!   returns 503 rather than a "could not verify" success; when a
//!   single entry's hash mismatches, the report status is "tampered"
//!   not "partial".
//! - **D15 cryptographic provenance**: this app is the read-side of
//!   the load-bearing doctrine.

pub mod auth;
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
