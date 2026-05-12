# Multi-stage build for recor-declaration.
# Stage 1 builds the release binary; stage 2 produces a minimal distroless
# image with only the binary, the CA bundle, and the migrations.

# ── Stage 1: build ─────────────────────────────────────────────────────────
FROM rust:1.88-bookworm AS builder

WORKDIR /build

# Cache dependencies separately from source for layer-aware rebuilds.
# Copy manifests first, build a stub main, fetch + compile deps, then
# replace the stub with real source.
COPY Cargo.toml rust-toolchain.toml ./
RUN mkdir -p src && echo "fn main() {}" > src/main.rs && \
    echo "" > src/lib.rs && \
    cargo build --release --bin recor-declaration --quiet && \
    rm -rf src

COPY src ./src
COPY migrations ./migrations

# Touch the actual sources so cargo doesn't reuse stub artefacts.
RUN touch src/main.rs src/lib.rs

RUN cargo build --release --bin recor-declaration --quiet \
    && strip target/release/recor-declaration

# ── Stage 2: runtime ───────────────────────────────────────────────────────
# Using debian:bookworm-slim (not distroless) so the image has /bin/sh for
# observability and runbook debugging. Production deployment can swap to
# gcr.io/distroless/cc-debian12 once the runbook flow is established.
FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl tini \
    && rm -rf /var/lib/apt/lists/*

RUN groupadd --system --gid 10001 recor \
    && useradd --system --uid 10001 --gid recor --shell /sbin/nologin recor

WORKDIR /app

COPY --from=builder /build/target/release/recor-declaration /app/recor-declaration
COPY --from=builder /build/migrations /app/migrations

USER recor:recor

EXPOSE 8080

# Honour SIGTERM via tini so graceful-shutdown engages.
ENTRYPOINT ["/usr/bin/tini", "--"]
CMD ["/app/recor-declaration"]
