# Multi-stage build for audit-reconciler.
# Build context is the REPO ROOT (set in docker-compose) so the
# workspace Cargo.toml and sibling shared crates are visible.

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
COPY apps/audit-reconciler/Cargo.toml ./apps/audit-reconciler/
COPY services/declaration/Cargo.toml services/declaration/build.rs ./services/declaration/
COPY services/verification-engine/Cargo.toml ./services/verification-engine/
COPY contracts ./contracts

RUN mkdir -p packages/recor-auth-oidc/src \
             packages/recor-logging/src \
             packages/fabric-bridge/src \
             apps/worker-fabric-bridge/src \
             apps/worker-fabric-bridge/migrations \
             apps/audit-verifier/src \
             apps/audit-reconciler/src \
             services/declaration/src \
             services/verification-engine/src \
    && echo "" > packages/recor-auth-oidc/src/lib.rs \
    && echo "" > packages/recor-logging/src/lib.rs \
    && echo "" > packages/fabric-bridge/src/lib.rs \
    && echo "fn main() {}" > apps/worker-fabric-bridge/src/main.rs \
    && echo "" > apps/worker-fabric-bridge/src/lib.rs \
    && echo "-- placeholder --" > apps/worker-fabric-bridge/migrations/0001_placeholder.sql \
    && echo "fn main() {}" > apps/audit-verifier/src/main.rs \
    && echo "" > apps/audit-verifier/src/lib.rs \
    && echo "fn main() {}" > apps/audit-reconciler/src/main.rs \
    && echo "" > apps/audit-reconciler/src/lib.rs \
    && echo "fn main() {}" > services/declaration/src/main.rs \
    && echo "" > services/declaration/src/lib.rs \
    && echo "fn main() {}" > services/verification-engine/src/main.rs \
    && echo "" > services/verification-engine/src/lib.rs \
    && cargo build --release --bin audit-reconciler --quiet \
    && rm -rf packages/recor-auth-oidc/src \
              packages/recor-logging/src \
              packages/fabric-bridge/src \
              apps/worker-fabric-bridge/src \
              apps/audit-verifier/src \
              apps/audit-reconciler/src \
              services/declaration/src \
              services/verification-engine/src

COPY packages/recor-auth-oidc ./packages/recor-auth-oidc
COPY packages/recor-logging ./packages/recor-logging
COPY packages/fabric-bridge ./packages/fabric-bridge
COPY apps/audit-verifier ./apps/audit-verifier
COPY apps/audit-reconciler ./apps/audit-reconciler

RUN find packages apps -name "*.rs" -exec touch {} +

RUN cargo build --release --bin audit-reconciler --quiet \
    && strip target/release/audit-reconciler

# ── Stage 2: runtime ───────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

# SECURITY_REFRESH (FIND-007 follow-up): docker layer cache is keyed on
# the command string, so a successful prior build pins this layer to
# whatever debian-security packages existed at build time. The
# publish-images workflow passes the current date as
# `SECURITY_REFRESH=YYYY-MM-DD`, busting the cache once per day so the
# `apt-get upgrade` below re-runs and pulls newly-published Debian
# fixes. First build of each day re-runs apt; same-day rebuilds stay
# cached.
ARG SECURITY_REFRESH=local-build
RUN echo "SECURITY_REFRESH=${SECURITY_REFRESH}" \
    && apt-get update \
    && apt-get -y upgrade \
    && apt-get install -y --no-install-recommends ca-certificates curl tini \
    && rm -rf /var/lib/apt/lists/*

RUN groupadd --system --gid 10001 recor \
    && useradd --system --uid 10001 --gid recor --shell /sbin/nologin recor

WORKDIR /app

COPY --from=builder /build/target/release/audit-reconciler /app/audit-reconciler

USER recor:recor

EXPOSE 8092

ENTRYPOINT ["/usr/bin/tini", "--"]
CMD ["/app/audit-reconciler"]
