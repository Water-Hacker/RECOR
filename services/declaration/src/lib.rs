//! RÉCOR Declaration service library.
//!
//! Service-shaped crate exposing the domain, application, infrastructure,
//! and API layers. `main.rs` wires the composition root.

#![deny(unsafe_code)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::missing_errors_doc)]

pub mod api;
pub mod application;
pub mod config;
pub mod domain;
pub mod error;
pub mod infrastructure;
// OBS-1: Prometheus metrics registry + middleware. The module is `pub`
// (not `pub(crate)`) so integration tests can build a router and inspect
// the registry directly.
pub mod metrics;
pub mod observability;
