# Multi-stage build for recor-verification-engine.
# Build context is the REPO ROOT (set in docker-compose) so the workspace
# Cargo.toml and sibling shared crates are visible.

FROM rust:1.88-bookworm AS builder

WORKDIR /build

# R-LOOP-2: librdkafka is built from source by rdkafka-sys and needs
# cmake + libcurl headers (declaration crate pulls rdkafka in via
# its kafka_producer module; cargo evaluates the workspace dep graph
# even for a single-bin build).
RUN apt-get update \
    && apt-get install -y --no-install-recommends cmake libcurl4-openssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Production builds use sqlx's offline cache (R-DECL-7): the committed
# `.sqlx/` directory replaces a live-DB connection at compile time. The
# image therefore builds without network access to Postgres, satisfying
# Doctrine 19 (reproducible everything) and Doctrine 14 (fail-closed at
# the image-build boundary if the cache is stale — `cargo build` errors
# out rather than silently falling back to a runtime check).
ENV SQLX_OFFLINE=true

# Copy the entire workspace tree. We previously did a manifest-only
# stub-build phase for finer Docker cache hits, but the workspace
# now spans many crates added across multiple PRs and any new
# workspace member silently broke the stub-build's hand-rolled file
# list. The tradeoff is a slower cold build, but `docker buildx`
# layer caching on the published image keeps incremental builds fast.
COPY . .

RUN cargo build --release --bin recor-verification-engine --quiet \
    && strip target/release/recor-verification-engine

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

COPY --from=builder /build/target/release/recor-verification-engine /app/recor-verification-engine
COPY --from=builder /build/services/verification-engine/migrations /app/migrations

USER recor:recor

EXPOSE 8081

ENTRYPOINT ["/usr/bin/tini", "--"]
CMD ["/app/recor-verification-engine"]
