# Multi-stage build for the four recor-sanctions-ingest binaries.
# Build context is the REPO ROOT (set in the publish-images workflow) so
# the workspace Cargo.toml and sibling crates are visible.
#
# One Dockerfile, one image per binary. The matrix in
# .github/workflows/publish-images.yaml supplies the `BIN` build-arg —
# the same image source produces:
#   - ghcr.io/water-hacker/recor-sanctions-ingest-ofac:<sha>
#   - ghcr.io/water-hacker/recor-sanctions-ingest-eu:<sha>
#   - ghcr.io/water-hacker/recor-sanctions-ingest-un:<sha>
#   - ghcr.io/water-hacker/recor-sanctions-ingest-icij:<sha>
#
# The runtime layer is gcr.io/distroless/cc-debian12 — the binary is
# dynamically linked against libssl/libcrypto/zlib (via reqwest's rustls
# + librdkafka transitively pulled by the workspace) but the
# distroless/cc image carries the matching libc + libgcc + a CA bundle.
# The result is ~70 MiB compressed per image, no shell, no apt.

FROM rust:1.88-bookworm AS builder

WORKDIR /build

# librdkafka builds from source for any workspace member that pulls
# rdkafka transitively. The sanctions-ingest crate itself does not need
# Kafka, but the workspace graph evaluates declaration's deps even for
# a single-bin build. cmake + libcurl headers satisfy that path.
RUN apt-get update \
    && apt-get install -y --no-install-recommends cmake libcurl4-openssl-dev pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Offline sqlx — the binaries only run runtime-validated queries (none
# are macro-checked), but the workspace's `cargo build` evaluates every
# crate, so we keep SQLX_OFFLINE=true on the image build.
ENV SQLX_OFFLINE=true

COPY . .

# The bin to build is matrix-supplied. Defaulting to ofac keeps
# `docker build` without a build-arg working for local hacks.
ARG BIN=recor-sanctions-ingest-ofac

RUN cargo build --release --bin "${BIN}" --quiet \
    && strip "target/release/${BIN}" \
    && cp "target/release/${BIN}" /build/ingest-bin

FROM gcr.io/distroless/cc-debian12 AS runtime

# Distroless/cc carries CA roots at /etc/ssl/certs/ca-certificates.crt
# and libssl/libcrypto from debian-bookworm-slim — matching the
# builder. No additional package install runs at runtime.

COPY --from=builder /build/ingest-bin /usr/local/bin/ingest

USER 65532:65532

ENTRYPOINT ["/usr/local/bin/ingest"]
