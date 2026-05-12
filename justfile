# RÉCOR — top-level command interface
# All engineers interact with the build through `just`; Bazel runs underneath.
# Each service has its own justfile with the same command set for that service.

set shell := ["bash", "-c"]
set dotenv-load := true

# Default action: list the commands
default:
    @just --list --unsorted

# Bootstrap a fresh checkout
bootstrap:
    @echo "Installing toolchains via mise..."
    mise install
    @echo "Installing system dependencies..."
    @just _install-system-deps
    @echo "Installing internal CLIs..."
    @just _install-internal-cli
    @echo "Configuring direnv..."
    @just _configure-direnv
    @echo "Bootstrapping pre-commit..."
    pre-commit install --install-hooks
    @echo ""
    @echo "Bootstrap complete. Verify with: cd services/entity && just check"

# Run every check across the monorepo (slow; for CI verification locally)
check:
    @just _check-rust
    @just _check-go
    @just _check-ts
    @just _check-policies
    @just _check-iac

# Format the entire monorepo
fmt:
    cargo fmt --all
    find . -name "*.go" -not -path "*/node_modules/*" -not -path "*/target/*" \
        | xargs gofmt -w
    pnpm prettier --write "**/*.{ts,tsx,js,jsx,json,md,yaml,yml}"
    terraform fmt -recursive infrastructure/

# Run the unit tests
test:
    cargo nextest run --workspace
    go test ./...
    pnpm vitest run

# Build the full monorepo via Bazel
build:
    bazel build //...

# Generate code from contracts (proto, openapi, graphql, avro)
gen:
    buf generate
    @just _gen-openapi
    @just _gen-graphql
    @just _gen-avro

# Run a local kind cluster with the platform deployed
local-up:
    @./tools/cli/local-up.sh

local-down:
    @./tools/cli/local-down.sh

# Bring up the dev observability stack (compose: OTel + Prometheus + Tempo + Loki + Grafana)
observability-up:
    @cd infrastructure/observability-dev && docker compose up -d

# Tear down the dev observability stack and drop its volumes
observability-down:
    @cd infrastructure/observability-dev && docker compose down -v

# Run the F-007 smoke test (emits traces, verifies end-to-end ingestion)
observability-smoke:
    @./tests/contract/observability-smoke.test.sh

# Apply pending migrations against the local development databases
migrate:
    @for svc in services/*/migrations; do \
        svc_name=$(basename $(dirname $svc)); \
        echo "Migrating $svc_name..."; \
        (cd services/$svc_name && just migrate); \
    done

# Validate that the dependency lockfiles are up to date
deps-verify:
    cargo update --dry-run --locked
    pnpm install --frozen-lockfile --dry-run
    go mod verify

# Bring up the local docs server
docs-serve:
    @cd docs && python -m http.server 8080

# Private targets prefixed with _ are not shown by default
_install-system-deps:
    ./tools/cli/install-system-deps.sh

_install-internal-cli:
    cargo install --path tools/cli/recor-cli

_configure-direnv:
    direnv allow .

_check-rust:
    cargo fmt --all -- --check
    cargo clippy --all-targets --all-features -- -D warnings
    cargo nextest run

_check-go:
    test -z "$(gofmt -l .)"
    golangci-lint run

_check-ts:
    pnpm tsc --noEmit
    pnpm eslint --max-warnings 0 .
    pnpm prettier --check .

_check-policies:
    opa fmt --diff policies/
    conftest verify --policy policies/ tests/policy/

_check-iac:
    terraform fmt -check -recursive infrastructure/terraform
    checkov -d infrastructure/

_gen-openapi:
    @echo "Generating OpenAPI types..."
    pnpm exec openapi-typescript contracts/rest/declaration.openapi.yaml \
        -o libraries/ts/recor-api-client/src/declaration.ts

_gen-graphql:
    @echo "Generating GraphQL types..."
    pnpm exec graphql-codegen --config tools/codegen/graphql-codegen.yaml

_gen-avro:
    @echo "Generating Avro bindings..."
    ./tools/codegen/gen-avro.sh contracts/events/
