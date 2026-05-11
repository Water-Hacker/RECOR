---
name: recor-rust-service
description: Rust service scaffolding and conventions. Fires when a new Rust service is being created or when working in a Rust service that requires standard structure. Loads the service template, the composition root pattern, and the project's Rust conventions.
---

# RÉCOR Rust service conventions

Most Layer 2 and Layer 0 services are Rust 2024 edition.

## Service directory structure

```
services/<service-name>/
├── CLAUDE.md                  -- service orientation (see Companion V2 P7)
├── Cargo.toml                 -- crate definition
├── README.md                  -- engineer-facing readme
├── justfile                   -- service commands (mirrors top-level)
├── migrations/                -- sqlx-managed migrations
│   ├── 0001_initial.sql
│   └── ...
├── src/
│   ├── main.rs                -- bootstrap; reads config, sets up tracing,
│   │                             constructs composition root, serves
│   ├── lib.rs                 -- public crate root
│   ├── domain/                -- domain types; pure
│   ├── application/           -- use case orchestration
│   ├── infrastructure/        -- adapters (postgres, neo4j, etc.)
│   ├── api/                   -- gRPC/REST/GraphQL implementations
│   ├── config.rs              -- typed configuration
│   ├── error.rs               -- service-scoped error type
│   └── observability.rs       -- tracing/metrics setup
└── tests/                     -- integration tests
```

## Composition root pattern

`main.rs` constructs the dependency graph explicitly. No DI framework, no
service locator. Composition root reads configuration, instantiates infrastructure
adapters, wires them into application services, mounts API handlers.

## Standard dependencies (justified additions)

```toml
tokio = { version = "1.43", features = ["full"] }
tonic = "0.13"                # gRPC
axum = "0.8"                  # REST (where applicable)
sqlx = { version = "0.8", features = ["runtime-tokio-rustls", "postgres", "macros", "uuid", "time"] }
tracing = "0.1"
tracing-subscriber = "0.3"
opentelemetry = "0.27"
opentelemetry-otlp = "0.27"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
anyhow = "1"
uuid = { version = "1.11", features = ["v7"] }
time = "0.3"
```

Per-service additions go into the service's Cargo.toml; shared dependencies live
in the workspace Cargo.toml at /Cargo.toml.

## Error pattern

Service-scoped error type with `thiserror`. Errors at API boundaries are
mapped to status codes; internal errors are not exposed.

## Observability pattern

Tracing initialised in main.rs. Every public function gets a `#[instrument]`
attribute or its equivalent. OTLP exporter is the production configuration.

## Composition root template

```rust
#[tokio::main]
async fn main() -> Result<()> {
    let config = recor_config::load::<Config>()?;
    recor_observability::init(&config.observability)?;

    let postgres = recor_postgres::connect(&config.postgres).await?;
    let kafka = recor_kafka::client(&config.kafka)?;
    let access_client = recor_access_client::new(&config.access).await?;

    let repository = PostgresRepository::new(postgres);
    let publisher = KafkaPublisher::new(kafka);
    let authorizer = AccessAuthorizer::new(access_client);

    let service = MyService::new(repository, publisher, authorizer);

    recor_grpc::serve(
        config.bind_addr,
        MyServiceGrpcAdapter::new(service),
    )
    .await?;

    Ok(())
}
```

## When you need help

- Service template: copy from /services/_template/
- Composition root example: /services/entity/src/main.rs
- Convention examples: /libraries/rust/recor-platform/
