//! RÉCOR Verification Engine library.

#![deny(unsafe_code)]
#![warn(clippy::all)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::must_use_candidate)]

pub mod api;
pub mod application;
pub mod config;
pub mod domain;
pub mod error;
pub mod infrastructure;
pub mod observability;
