//! HTTP API.

pub mod auth;
pub mod dto;
pub mod rest;

pub use rest::{router, AppState};
