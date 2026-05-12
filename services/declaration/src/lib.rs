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
pub mod observability;
