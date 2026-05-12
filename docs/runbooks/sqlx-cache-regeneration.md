# Runbook: sqlx Offline Cache Regeneration

**Tracks:** R-DECL-7. Procedure for keeping the committed
`services/<svc>/.sqlx/` offline-cache directories in step with the SQL
queries and migrations in the two Rust services.

## Scope

Both Rust services compile their SQL with `sqlx`'s compile-time-checked
macros (`sqlx::query!`, `sqlx::query_as!`, `sqlx::query_scalar!`). The
macros need a way to type-check each query against the real schema at
`cargo build` time. There are two modes:

| Mode | When | What the build needs |
|---|---|---|
| **Online** (`DATABASE_URL` set, `SQLX_OFFLINE` unset) | local dev with a live DB | a Postgres reachable at `$DATABASE_URL` with current migrations applied |
| **Offline** (`SQLX_OFFLINE=true`) | CI, container builds, reproducible builds, air-gapped builds | the committed `services/<svc>/.sqlx/` JSON cache |

Production image builds set `SQLX_OFFLINE=true` in both Dockerfiles
(see `services/declaration/Dockerfile`,
`services/verification-engine/Dockerfile`) — they must never reach out
to a database at image-build time (Doctrine 19: reproducible everything;
Doctrine 14: fail-closed at the build boundary if the cache is stale).

## When to regenerate

Regenerate the cache **any time** you:

1. Add, remove, or change the SQL of a `sqlx::query!` /
   `sqlx::query_as!` / `sqlx::query_scalar!` call (including renaming
   columns, changing types, or adjusting `SELECT` projections).
2. Add a new migration in `services/<svc>/migrations/` that alters the
   schema seen by any compiled query.
3. Bump the sqlx crate version (the cache schema is versioned).

If you don't regenerate, the CI step `db / sqlx-cache-check` will fail
the PR: it runs `cargo sqlx prepare --check` against a freshly migrated
scratch Postgres and refuses to merge if the committed JSON differs
from what the live schema produces. Branch protection treats this job
as required.

## Procedure

### 1. Stand up scratch Postgres containers

Two databases, one per service, on distinct host ports so they can run
in parallel without colliding with the developer's local stack.

```bash
docker run --rm -d --name recor-decl-pg \
    -p 127.0.0.1:5435:5432 \
    -e POSTGRES_PASSWORD=postgres \
    postgres:17-alpine

docker run --rm -d --name recor-ver-pg \
    -p 127.0.0.1:5436:5432 \
    -e POSTGRES_PASSWORD=postgres \
    postgres:17-alpine

# Wait for both to accept connections.
until docker exec recor-decl-pg pg_isready -U postgres >/dev/null 2>&1; do sleep 0.5; done
until docker exec recor-ver-pg  pg_isready -U postgres >/dev/null 2>&1; do sleep 0.5; done
```

### 2. Apply the migrations

```bash
# Declaration service
for f in services/declaration/migrations/*.sql; do
  docker exec -i recor-decl-pg psql -U postgres -d postgres \
      -v ON_ERROR_STOP=1 < "$f"
done

# Verification engine
for f in services/verification-engine/migrations/*.sql; do
  docker exec -i recor-ver-pg psql -U postgres -d postgres \
      -v ON_ERROR_STOP=1 < "$f"
done
```

### 3. Install `sqlx-cli` (one-time, in the workspace cargo cache)

```bash
docker run --rm \
    -v "$PWD":/work -w /work \
    -e CARGO_HOME=/work/.cargo-cache \
    rust:1.88-bookworm \
    cargo install sqlx-cli --version 0.8.6 \
        --no-default-features --features postgres,rustls \
        --root /work/.cargo-cache --locked
```

The pinned version matches the sqlx crate version in
`Cargo.toml` (`[workspace.dependencies] sqlx = "0.8"`). Bumping the
crate version requires bumping the CLI version here, in the CI
workflow, and in the Dockerfiles.

### 4. Regenerate the cache

Per service:

```bash
docker run --rm --network host \
    -v "$PWD":/work -w /work \
    -e CARGO_HOME=/work/.cargo-cache \
    -e CARGO_TARGET_DIR=/work/target \
    rust:1.88-bookworm \
    bash -c '
        export PATH=/work/.cargo-cache/bin:$PATH
        cd /work/services/declaration
        DATABASE_URL=postgres://postgres:postgres@127.0.0.1:5435/postgres \
            cargo sqlx prepare -- --lib --bin recor-declaration
        cd /work/services/verification-engine
        DATABASE_URL=postgres://postgres:postgres@127.0.0.1:5436/postgres \
            cargo sqlx prepare -- --lib --bin recor-verification-engine
    '
```

`cargo sqlx prepare` writes one JSON file per distinct query into
`services/<svc>/.sqlx/`. Two crates therefore have two independent
caches — they are never merged into a single workspace cache because
the two services point at different schemas.

### 5. Commit the cache directories

```bash
git add services/declaration/.sqlx services/verification-engine/.sqlx
git commit -m "feat(db): regenerate sqlx offline cache"
```

A new query produces one new JSON; renaming a column in a migration
updates the JSON for every query that references that column. Diffs
are usually small — review them to confirm only the queries you
expected to change actually moved.

### 6. Verify locally

```bash
# Confirm the cache is current vs the live DB.
DATABASE_URL=postgres://postgres:postgres@127.0.0.1:5435/postgres \
    cargo sqlx prepare --check -- --lib --bin recor-declaration \
    --manifest-path services/declaration/Cargo.toml

DATABASE_URL=postgres://postgres:postgres@127.0.0.1:5436/postgres \
    cargo sqlx prepare --check -- --lib --bin recor-verification-engine \
    --manifest-path services/verification-engine/Cargo.toml

# Confirm an offline build still passes — this is the exact mode the
# production Docker image build runs in.
SQLX_OFFLINE=true cargo build --workspace --release
```

`--check` exits non-zero if the cache is stale and prints the queries
whose generated JSON would differ. That same exit code drives the
CI job `db / sqlx-cache-check` in
`.github/workflows/required-checks.yaml`.

### 7. Tear down the scratch containers

```bash
docker rm -f recor-decl-pg recor-ver-pg
```

## CI behaviour

The required-checks workflow performs this exact check on every PR:
spins up two `postgres:17-alpine` service containers, applies each
service's migrations via `psql`, installs `sqlx-cli`, and runs
`cargo sqlx prepare --check` against each service crate. A stale
committed cache fails the job and branch protection blocks the merge.

The workflow also runs a `SQLX_OFFLINE=true cargo build --workspace
--release` step so a missing or malformed `.sqlx/` entry is caught at
PR time, not at the next production image build.

## Exceptions

A small number of query sites cannot be macro-checked (for example,
fully dynamic SQL constructed from runtime data). Today the codebase
has **zero** such sites — every active call site uses a macro. If a
future change genuinely needs runtime-checked SQL, the call site
**must** carry an inline justification:

```rust
// sqlx-runtime-check: dynamic IN-clause over a caller-bounded list
//                     (see ADR-XYZ); the SQL string is constructed
//                     from a const set of column names, never from
//                     untrusted input.
let _ = sqlx::query(&dynamic_sql).bind(...).execute(...).await?;
```

The `// sqlx-runtime-check:` marker is the audit trail (Doctrine 7:
no workarounds without naming the reason). Reviewers grep for it on
every PR.

## Doctrines tracked

- **D04 tests are part of the feature** — the CI cache-check job is
  the regression test for schema drift; the macro itself is a build
  failure if a column is renamed without re-preparing.
- **D07 no workarounds** — runtime-checked queries require an inline
  `// sqlx-runtime-check:` justification; the default is the macro.
- **D14 fail-closed** — `cargo sqlx prepare --check` failing in CI
  blocks the merge. Production image builds set `SQLX_OFFLINE=true`
  and fail fast on a stale cache rather than silently reaching for a
  database connection.
- **D19 reproducible everything** — the `.sqlx/` directory is part of
  the committed source. Any builder, with no network access, produces
  the same binary.

## See also

- `docs/PRODUCTION-TODO.md` — R-DECL-7 ticket (this runbook is paired
  with the ticket as DOC).
- `services/declaration/src/infrastructure/postgres.rs` — module
  doc-comment that points readers here.
- sqlx upstream docs:
  <https://github.com/launchbadge/sqlx/blob/main/sqlx-cli/README.md>
