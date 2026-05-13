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

# protoc-bin-vendored provides the protoc binary the declaration crate's
# build.rs needs for tonic-build (R-DECL-8). No system protoc install
# required.

# Copy the entire workspace tree. We previously did a manifest-only
# stub-build phase for finer Docker cache hits, but the workspace
# now spans many crates added across multiple PRs (recor-auth-oidc,
# recor-logging, recor-vault-client, etc.) and any new workspace
# member silently broke the stub-build's hand-rolled file list. The
# tradeoff is a slower cold build, but `docker buildx` layer caching
# on the published image keeps incremental builds fast.
COPY . .

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
