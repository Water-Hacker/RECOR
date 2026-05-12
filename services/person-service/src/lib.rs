//! RÉCOR Person service library — canonical natural-person registry.
//!
//! Closes the structural part of R-DECL-4. Anchors every `person_id`
//! referenced inside the Declaration service's `beneficial_owners`
//! payload. Same four-layer separation as `services/declaration`:
//!
//! ```text
//!   domain         — pure types + invariants, no I/O
//!     ↓
//!   application    — use-case orchestrators over the repository port
//!     ↓
//!   infrastructure — concrete Postgres adapter + outbox
//!     ↓
//!   api            — axum HTTP surface + OpenAPI annotations
//! ```
//!
//! Event-sourced: every state-changing operation appends to
//! `person_events`; the `persons` table is a derived current-state
//! projection. The event log is append-only by trigger (COMP-2 mirror
//! of `declaration_events`).
//!
//! Doctrine notes:
//!   - **D15 cryptographic provenance — limited in v1:** unlike
//!     declarations, person events do NOT carry a declarant-supplied
//!     Ed25519 attestation in v1. The registry is operator-curated,
//!     not declarant-signed; the per-event provenance comes from the
//!     authenticated principal recorded on the event and the
//!     append-only audit chain. Per-event signatures are deferred to
//!     a future ticket (see service `CLAUDE.md`).
//!   - **D17 zero trust:** every state-changing endpoint sources its
//!     principal from auth, never from the request body.
//!   - **D13 idempotency:** `POST /v1/persons` honours `Idempotency-Key`.

#![deny(unsafe_code)]
#![warn(clippy::all)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::missing_errors_doc)]

pub mod api;
pub mod application;
pub mod config;
pub mod domain;
pub mod error;
pub mod infrastructure;
pub mod metrics;
pub mod observability;
