# Multi-stage build for recor-person-service.
# Build context is the REPO ROOT (set in docker-compose) so the workspace
# Cargo.toml and sibling shared crates are visible.

# ── Stage 1: build ─────────────────────────────────────────────────────────
FROM rust:1.88-bookworm AS builder

WORKDIR /build

# Production builds use sqlx's offline cache once R-PERSON-SQLX-CACHE
# ships. For the v1 skeleton the postgres adapter uses runtime-checked
# queries, so SQLX_OFFLINE has no effect on compilation — but we set it
# anyway to keep the build hermetic the moment the cache lands.
ENV SQLX_OFFLINE=true

# Copy workspace root + every member manifest first, then build a stub
# for each crate so cargo can fetch + compile all transitive deps with
# only the manifests on disk. Caches the dep graph independently from
# source changes.
COPY Cargo.toml rust-toolchain.toml ./
COPY packages/recor-auth-oidc/Cargo.toml ./packages/recor-auth-oidc/
COPY packages/recor-logging/Cargo.toml ./packages/recor-logging/
COPY services/declaration/Cargo.toml services/declaration/build.rs ./services/declaration/
COPY services/person-service/Cargo.toml ./services/person-service/
COPY services/verification-engine/Cargo.toml ./services/verification-engine/
COPY contracts ./contracts

RUN mkdir -p packages/recor-auth-oidc/src \
             packages/recor-logging/src \
             services/declaration/src \
             services/person-service/src \
             services/verification-engine/src \
    && echo "" > packages/recor-auth-oidc/src/lib.rs \
    && echo "" > packages/recor-logging/src/lib.rs \
    && echo "fn main() {}" > services/declaration/src/main.rs \
    && echo "" > services/declaration/src/lib.rs \
    && echo "fn main() {}" > services/person-service/src/main.rs \
    && echo "" > services/person-service/src/lib.rs \
    && echo "fn main() {}" > services/verification-engine/src/main.rs \
    && echo "" > services/verification-engine/src/lib.rs \
    && cargo build --release --bin recor-person-service --quiet \
    && rm -rf packages/recor-auth-oidc/src \
              packages/recor-logging/src \
              services/declaration/src \
              services/person-service/src \
              services/verification-engine/src

# Now bring in the real source for the crates we actually need to build
# the person-service binary: the workspace member + its shared deps.
# packages/recor-logging is part of this set per the #86 fix that the
# Dockerfile must include it in the stub-build phase.
COPY packages/recor-auth-oidc ./packages/recor-auth-oidc
COPY packages/recor-logging ./packages/recor-logging
COPY services/person-service/src ./services/person-service/src
COPY services/person-service/migrations ./services/person-service/migrations

# Touch so cargo doesn't reuse stub artefacts.
RUN find packages services -name "*.rs" -exec touch {} +

RUN cargo build --release --bin recor-person-service --quiet \
    && strip target/release/recor-person-service

# ── Stage 2: runtime ───────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl tini \
    && rm -rf /var/lib/apt/lists/*

RUN groupadd --system --gid 10001 recor \
    && useradd --system --uid 10001 --gid recor --shell /sbin/nologin recor

WORKDIR /app

COPY --from=builder /build/target/release/recor-person-service /app/recor-person-service
COPY --from=builder /build/services/person-service/migrations /app/migrations

USER recor:recor

EXPOSE 8082

ENTRYPOINT ["/usr/bin/tini", "--"]
CMD ["/app/recor-person-service"]
