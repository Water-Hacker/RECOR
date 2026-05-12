//! Build script — generates the Rust gRPC server + client stubs from
//! `contracts/declaration.proto` (R-DECL-8).
//!
//! Why a `build.rs` rather than checking generated code in?
//!   - The .proto is the source of truth (see contracts/declaration.proto).
//!     Regenerating from the .proto on every build keeps generated code
//!     and schema in lockstep; no "did someone forget to regenerate?"
//!     surface area.
//!   - Generated code is excluded from the workspace's offline sqlx
//!     concerns — tonic-build does NOT need DATABASE_URL or any
//!     network, so the R-DECL-7 invariant
//!     (`SQLX_OFFLINE=true cargo build --workspace --release` is clean)
//!     is preserved.
//!
//! The generated module lands in `OUT_DIR` and is included from
//! `src/api/grpc.rs` via `tonic::include_proto!`.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Point tonic-build (→ prost-build) at the vendored static protoc
    // binary so we don't depend on the system having protobuf-compiler
    // installed. CI runners (openapi-drift, sqlx-cache-check) and
    // fresh dev machines compile the .proto without an apt-get step.
    // Respect an operator override if PROTOC is already exported.
    if std::env::var_os("PROTOC").is_none() {
        let protoc = protoc_bin_vendored::protoc_bin_path()
            .expect("protoc-bin-vendored: no protoc binary for this target");
        // SAFETY: build scripts run single-threaded, so `set_var` here
        // doesn't race other threads.
        unsafe {
            std::env::set_var("PROTOC", protoc);
        }
    }

    // The .proto lives at the repo root under contracts/. The path is
    // relative to this build.rs file: services/declaration/build.rs →
    // ../../contracts/declaration.proto.
    let proto = "../../contracts/declaration.proto";

    // Re-run the build script if the .proto or its parent directory
    // changes. Without these the generated code can go stale on a
    // schema-only edit.
    println!("cargo:rerun-if-changed={proto}");
    println!("cargo:rerun-if-changed=../../contracts");

    // Build both the server stub (`DeclarationService`) and the client
    // stub (used by the integration test under
    // services/declaration/tests/grpc_integration.rs).
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        // The default include path is the .proto's parent directory —
        // contracts/. No nested imports today; the explicit include
        // is here to make future cross-file imports trivial.
        .compile_protos(&[proto], &["../../contracts"])?;

    Ok(())
}
