# Multi-stage build for recor-declaration.
# Build context is the REPO ROOT (set in docker-compose) so the workspace
# Cargo.toml and sibling shared crates are visible.

# ── Stage 1: build ─────────────────────────────────────────────────────────
FROM rust:1.88-bookworm AS builder

WORKDIR /build

# Production builds use sqlx's offline cache (R-DECL-7): the committed
# `.sqlx/` directory replaces a live-DB connection at compile time. The
# image therefore builds without network access to Postgres, satisfying
# Doctrine 19 (reproducible everything) and Doctrine 14 (fail-closed at
# the image-build boundary if the cache is stale — `cargo build` errors
# out rather than silently falling back to a runtime check).
ENV SQLX_OFFLINE=true

# Copy workspace root + every member manifest first, then build a stub
# for each crate so cargo can fetch + compile all transitive deps with
# only the manifests on disk. This caches the dep graph independently
# from source changes.
COPY Cargo.toml rust-toolchain.toml ./
COPY packages/recor-auth-oidc/Cargo.toml ./packages/recor-auth-oidc/
COPY packages/recor-logging/Cargo.toml ./packages/recor-logging/
COPY services/declaration/Cargo.toml services/declaration/build.rs ./services/declaration/
COPY services/verification-engine/Cargo.toml ./services/verification-engine/
COPY contracts ./contracts

RUN mkdir -p packages/recor-auth-oidc/src \
             packages/recor-logging/src \
             services/declaration/src \
             services/verification-engine/src \
    && echo "" > packages/recor-auth-oidc/src/lib.rs \
    && echo "" > packages/recor-logging/src/lib.rs \
    && echo "fn main() {}" > services/declaration/src/main.rs \
    && echo "" > services/declaration/src/lib.rs \
    && echo "fn main() {}" > services/verification-engine/src/main.rs \
    && echo "" > services/verification-engine/src/lib.rs \
    && cargo build --release --bin recor-declaration --quiet \
    && rm -rf packages/recor-auth-oidc/src \
              packages/recor-logging/src \
              services/declaration/src \
              services/verification-engine/src

# Now bring in the real source for the crates we actually need to build
# the declaration binary: the workspace member + its shared deps.
COPY packages/recor-auth-oidc ./packages/recor-auth-oidc
COPY packages/recor-logging ./packages/recor-logging
COPY services/declaration/src ./services/declaration/src
COPY services/declaration/migrations ./services/declaration/migrations
# Offline sqlx cache (committed alongside the source). SQLX_OFFLINE=true
# above tells the macros to consult this directory instead of opening a
# database connection.
COPY services/declaration/.sqlx ./services/declaration/.sqlx

# Touch so cargo doesn't reuse stub artefacts.
RUN find packages services -name "*.rs" -exec touch {} +

RUN cargo build --release --bin recor-declaration --quiet \
    && strip target/release/recor-declaration

# ── Stage 2: runtime ───────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl tini \
    && rm -rf /var/lib/apt/lists/*

RUN groupadd --system --gid 10001 recor \
    && useradd --system --uid 10001 --gid recor --shell /sbin/nologin recor

WORKDIR /app

COPY --from=builder /build/target/release/recor-declaration /app/recor-declaration
COPY --from=builder /build/services/declaration/migrations /app/migrations

USER recor:recor

EXPOSE 8080

ENTRYPOINT ["/usr/bin/tini", "--"]
CMD ["/app/recor-declaration"]
