# Multi-stage build for recor-entity-service (IDENTITY-1).
# Build context is the REPO ROOT (set in docker-compose) so the workspace
# Cargo.toml and sibling shared crates are visible.

# ── Stage 1: build ─────────────────────────────────────────────────────────
FROM rust:1.88-bookworm AS builder

WORKDIR /build

# Production builds use sqlx's offline cache: the committed `.sqlx/`
# directory replaces a live-DB connection at compile time. The image
# therefore builds without network access to Postgres, satisfying
# Doctrine 19 (reproducible everything) and Doctrine 14 (fail-closed at
# the image-build boundary if the cache is stale).
ENV SQLX_OFFLINE=true

# Copy workspace root + every member manifest first, then build a stub
# for each crate so cargo can fetch + compile all transitive deps with
# only the manifests on disk. This caches the dep graph independently
# from source changes.
COPY Cargo.toml rust-toolchain.toml ./
COPY packages/recor-auth-oidc/Cargo.toml ./packages/recor-auth-oidc/
COPY packages/recor-logging/Cargo.toml ./packages/recor-logging/
COPY services/declaration/Cargo.toml services/declaration/build.rs ./services/declaration/
COPY services/entity-service/Cargo.toml ./services/entity-service/
COPY services/verification-engine/Cargo.toml ./services/verification-engine/
COPY contracts ./contracts

RUN mkdir -p packages/recor-auth-oidc/src \
             packages/recor-logging/src \
             services/declaration/src \
             services/entity-service/src \
             services/entity-service/src/bin \
             services/verification-engine/src \
    && echo "" > packages/recor-auth-oidc/src/lib.rs \
    && echo "" > packages/recor-logging/src/lib.rs \
    && echo "fn main() {}" > services/declaration/src/main.rs \
    && echo "" > services/declaration/src/lib.rs \
    && echo "fn main() {}" > services/entity-service/src/main.rs \
    && echo "fn main() {}" > services/entity-service/src/bin/dump-openapi.rs \
    && echo "" > services/entity-service/src/lib.rs \
    && echo "fn main() {}" > services/verification-engine/src/main.rs \
    && echo "" > services/verification-engine/src/lib.rs \
    && cargo build --release --bin recor-entity-service --quiet \
    && rm -rf packages/recor-auth-oidc/src \
              packages/recor-logging/src \
              services/declaration/src \
              services/entity-service/src \
              services/verification-engine/src

# Now bring in the real source for the crates we need to build the
# entity-service binary: the workspace member + its shared deps.
COPY packages/recor-auth-oidc ./packages/recor-auth-oidc
COPY packages/recor-logging ./packages/recor-logging
COPY services/entity-service/src ./services/entity-service/src
COPY services/entity-service/migrations ./services/entity-service/migrations
# Offline sqlx cache (committed alongside the source). SQLX_OFFLINE=true
# tells the macros to consult this directory instead of opening a database.
COPY services/entity-service/.sqlx ./services/entity-service/.sqlx

# Touch so cargo doesn't reuse stub artefacts.
RUN find packages services -name "*.rs" -exec touch {} +

RUN cargo build --release --bin recor-entity-service --quiet \
    && strip target/release/recor-entity-service

# ── Stage 2: runtime ───────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

# SECURITY_REFRESH (audit follow-up to PR #111): docker layer cache is
# keyed on the command string, so a successful prior build pins this
# layer to whatever debian-security packages existed at build time.
# Trivy then flags HIGH/CRITICAL CVEs (libc, libcap, systemd, …) that
# Debian has already published fixes for. The publish-images workflow
# passes the current date as `SECURITY_REFRESH=YYYY-MM-DD`, busting
# the cache once per day so the apt-get upgrade below re-runs and
# pulls newly-published fixes. First build of each day re-runs apt;
# same-day rebuilds stay cached.
ARG SECURITY_REFRESH=local-build
RUN echo "SECURITY_REFRESH=${SECURITY_REFRESH}" \
    && apt-get update \
    && apt-get -y upgrade \
    && apt-get install -y --no-install-recommends ca-certificates curl tini \
    && rm -rf /var/lib/apt/lists/*

RUN groupadd --system --gid 10001 recor \
    && useradd --system --uid 10001 --gid recor --shell /sbin/nologin recor

WORKDIR /app

COPY --from=builder /build/target/release/recor-entity-service /app/recor-entity-service
COPY --from=builder /build/services/entity-service/migrations /app/migrations

USER recor:recor

EXPOSE 8083

ENTRYPOINT ["/usr/bin/tini", "--"]
CMD ["/app/recor-entity-service"]
