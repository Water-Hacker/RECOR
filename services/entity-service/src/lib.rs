//! RÉCOR Entity service library.
//!
//! Service-shaped crate exposing the domain, application, infrastructure,
//! and API layers. `main.rs` wires the composition root.

#![deny(unsafe_code)]
#![warn(clippy::all)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]

pub mod api;
pub mod application;
pub mod config;
pub mod domain;
pub mod error;
pub mod infrastructure;
pub mod metrics;
pub mod observability;
