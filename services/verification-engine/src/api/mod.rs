// TODO(R-VER-GRPC): mirror declaration's gRPC surface here. R-DECL-8
// (PR #78) shipped a tonic-based gRPC API alongside REST for
// services/declaration via contracts/declaration.proto + a build.rs
// that invokes tonic-build. The verification engine needs the same
// pattern (a verification.proto under contracts/, a build.rs in this
// crate, and a `grpc` module mirroring `rest`) so service-to-service
// callers can use a typed wire instead of JSON over HTTP. The
// declaration service is the reference implementation.

pub mod auth;
pub mod dlq;
pub mod internal;
pub mod oidc;
pub mod rest;

pub use dlq::{list_dlq, replay_dlq, DlqAdminState};
pub use internal::{handle_declaration_event, InternalAppState};
pub use oidc::{OidcVerifier, OidcVerifierBuilder, VerificationError};
pub use rest::{metrics_only_router, router, AppState};
