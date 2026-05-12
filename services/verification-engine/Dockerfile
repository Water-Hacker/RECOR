# Multi-stage build for recor-verification-engine.
# Build context is the REPO ROOT (set in docker-compose) so the workspace
# Cargo.toml and sibling shared crates are visible.

FROM rust:1.88-bookworm AS builder

WORKDIR /build

# Production builds use sqlx's offline cache (R-DECL-7): the committed
# `.sqlx/` directory replaces a live-DB connection at compile time. The
# image therefore builds without network access to Postgres, satisfying
# Doctrine 19 (reproducible everything) and Doctrine 14 (fail-closed at
# the image-build boundary if the cache is stale — `cargo build` errors
# out rather than silently falling back to a runtime check).
ENV SQLX_OFFLINE=true

COPY Cargo.toml rust-toolchain.toml ./
COPY packages/recor-auth-oidc/Cargo.toml ./packages/recor-auth-oidc/
COPY services/declaration/Cargo.toml ./services/declaration/
COPY services/verification-engine/Cargo.toml ./services/verification-engine/

RUN mkdir -p packages/recor-auth-oidc/src \
             services/declaration/src \
             services/verification-engine/src \
    && echo "" > packages/recor-auth-oidc/src/lib.rs \
    && echo "fn main() {}" > services/declaration/src/main.rs \
    && echo "" > services/declaration/src/lib.rs \
    && echo "fn main() {}" > services/verification-engine/src/main.rs \
    && echo "" > services/verification-engine/src/lib.rs \
    && cargo build --release --bin recor-verification-engine --quiet \
    && rm -rf packages/recor-auth-oidc/src \
              services/declaration/src \
              services/verification-engine/src

COPY packages/recor-auth-oidc ./packages/recor-auth-oidc
COPY services/verification-engine/src ./services/verification-engine/src
COPY services/verification-engine/migrations ./services/verification-engine/migrations
# Offline sqlx cache (committed alongside the source). SQLX_OFFLINE=true
# above tells the macros to consult this directory instead of opening a
# database connection.
COPY services/verification-engine/.sqlx ./services/verification-engine/.sqlx

RUN find packages services -name "*.rs" -exec touch {} +

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
COPY --from=builder /build/services/verification-engine/migrations /app/migrations

USER recor:recor

EXPOSE 8081

ENTRYPOINT ["/usr/bin/tini", "--"]
CMD ["/app/recor-verification-engine"]
