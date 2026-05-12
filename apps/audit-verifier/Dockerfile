# Multi-stage build for audit-verifier.
# Build context is the REPO ROOT (set in docker-compose) so the workspace
# Cargo.toml and sibling shared crates are visible.

# ── Stage 1: build ─────────────────────────────────────────────────────────
FROM rust:1.88-bookworm AS builder

WORKDIR /build

ENV SQLX_OFFLINE=true

COPY Cargo.toml rust-toolchain.toml ./
COPY packages/recor-auth-oidc/Cargo.toml ./packages/recor-auth-oidc/
COPY packages/recor-logging/Cargo.toml ./packages/recor-logging/
COPY packages/fabric-bridge/Cargo.toml ./packages/fabric-bridge/
COPY apps/worker-fabric-bridge/Cargo.toml ./apps/worker-fabric-bridge/
COPY apps/audit-verifier/Cargo.toml ./apps/audit-verifier/
COPY services/declaration/Cargo.toml services/declaration/build.rs ./services/declaration/
COPY services/verification-engine/Cargo.toml ./services/verification-engine/
COPY contracts ./contracts

RUN mkdir -p packages/recor-auth-oidc/src \
             packages/recor-logging/src \
             packages/fabric-bridge/src \
             apps/worker-fabric-bridge/src \
             apps/worker-fabric-bridge/migrations \
             apps/audit-verifier/src \
             services/declaration/src \
             services/verification-engine/src \
    && echo "" > packages/recor-auth-oidc/src/lib.rs \
    && echo "" > packages/recor-logging/src/lib.rs \
    && echo "" > packages/fabric-bridge/src/lib.rs \
    && echo "fn main() {}" > apps/worker-fabric-bridge/src/main.rs \
    && echo "" > apps/worker-fabric-bridge/src/lib.rs \
    && echo "-- placeholder --" > apps/worker-fabric-bridge/migrations/0001_create_fabric_bridge_dlq.sql \
    && echo "fn main() {}" > apps/audit-verifier/src/main.rs \
    && echo "" > apps/audit-verifier/src/lib.rs \
    && echo "fn main() {}" > services/declaration/src/main.rs \
    && echo "" > services/declaration/src/lib.rs \
    && echo "fn main() {}" > services/verification-engine/src/main.rs \
    && echo "" > services/verification-engine/src/lib.rs \
    && cargo build --release --bin audit-verifier --quiet \
    && rm -rf packages/recor-auth-oidc/src \
              packages/recor-logging/src \
              packages/fabric-bridge/src \
              apps/worker-fabric-bridge/src \
              apps/audit-verifier/src \
              services/declaration/src \
              services/verification-engine/src

COPY packages/recor-auth-oidc ./packages/recor-auth-oidc
COPY packages/recor-logging ./packages/recor-logging
COPY packages/fabric-bridge ./packages/fabric-bridge
COPY apps/audit-verifier ./apps/audit-verifier

RUN find packages apps -name "*.rs" -exec touch {} +

RUN cargo build --release --bin audit-verifier --quiet \
    && strip target/release/audit-verifier

# ── Stage 2: runtime ───────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl tini \
    && rm -rf /var/lib/apt/lists/*

RUN groupadd --system --gid 10001 recor \
    && useradd --system --uid 10001 --gid recor --shell /sbin/nologin recor

WORKDIR /app

COPY --from=builder /build/target/release/audit-verifier /app/audit-verifier

USER recor:recor

EXPOSE 8091

ENTRYPOINT ["/usr/bin/tini", "--"]
CMD ["/app/audit-verifier"]
