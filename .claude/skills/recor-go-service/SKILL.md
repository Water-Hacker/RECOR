---
name: recor-go-service
description: Go service scaffolding and conventions. Fires when a new Go service is being created or when working in a Go service that requires standard structure.
---

# RÉCOR Go service conventions

Used for: audit service, workflow service (Temporal), notification service,
some Layer 5 integrations.

## Service directory structure

```
services/<service-name>/
├── CLAUDE.md
├── go.mod
├── go.sum
├── README.md
├── justfile
├── migrations/
├── cmd/
│   └── server/
│       └── main.go             -- bootstrap
├── internal/
│   ├── domain/                 -- domain types
│   ├── application/            -- use case orchestration
│   ├── infrastructure/         -- adapters
│   ├── api/                    -- gRPC/REST handlers
│   ├── config/
│   └── observability/
└── tests/
```

## Standard dependencies (justified additions)

- google.golang.org/grpc
- github.com/jackc/pgx/v5 (Postgres)
- github.com/segmentio/kafka-go (or confluent-kafka-go where librdkafka acceptable)
- go.opentelemetry.io/otel and exporters
- go.uber.org/zap
- github.com/stretchr/testify

## Logging pattern

zap.Logger constructed in main.go; passed through context.

## Error pattern

Errors as values; `%w` wrapping; sentinel errors only when callers need to
distinguish.

## Service-template entry point

```go
package main

func main() {
    cfg := config.MustLoad()
    log := observability.MustInit(cfg.Observability)
    defer log.Sync()

    ctx, cancel := signal.NotifyContext(context.Background(),
        syscall.SIGINT, syscall.SIGTERM)
    defer cancel()

    pg := postgres.MustConnect(ctx, cfg.Postgres)
    defer pg.Close()

    kafka := kafka.MustClient(cfg.Kafka)
    defer kafka.Close()

    svc := myservice.New(pg, kafka)

    if err := grpcserver.Serve(ctx, cfg.BindAddr, svc); err != nil {
        log.Fatal("server failed", zap.Error(err))
    }
}
```
