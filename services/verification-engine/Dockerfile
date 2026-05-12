# Multi-stage build for recor-verification-engine.

FROM rust:1.88-bookworm AS builder
WORKDIR /build
COPY Cargo.toml rust-toolchain.toml ./
RUN mkdir -p src && echo "fn main() {}" > src/main.rs && \
    echo "" > src/lib.rs && \
    cargo build --release --bin recor-verification-engine --quiet && \
    rm -rf src
COPY src ./src
COPY migrations ./migrations
RUN touch src/main.rs src/lib.rs
RUN cargo build --release --bin recor-verification-engine --quiet \
    && strip target/release/recor-verification-engine

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl tini \
    && rm -rf /var/lib/apt/lists/*
RUN groupadd --system --gid 10001 recor \
    && useradd --system --uid 10001 --gid recor --shell /sbin/nologin recor
WORKDIR /app
COPY --from=builder /build/target/release/recor-verification-engine /app/recor-verification-engine
COPY --from=builder /build/migrations /app/migrations
USER recor:recor
EXPOSE 8081
ENTRYPOINT ["/usr/bin/tini", "--"]
CMD ["/app/recor-verification-engine"]
