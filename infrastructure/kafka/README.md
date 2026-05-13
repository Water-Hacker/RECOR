# Local Kafka cluster (R-LOOP-2)

Single-broker `bitnami/kafka` in KRaft mode (no Zookeeper) for local
development and the `kafka-smoke.sh` integration test.

This is the dev surface for the D↔V transport. Production deployments
use a multi-broker replicated cluster operated via the Strimzi operator
under `infrastructure/helm/kafka/` (follow-up — not yet shipped).

## What it provides

- One broker on host port `9092` (external) and `kafka:29092`
  (in-cluster, the docker network name).
- KRaft single-node controller+broker (no Zookeeper).
- Two topics created by `topics-init.sh`:
  - `recor.declaration.events.v1` — D → V (3 partitions, retention 7d)
  - `recor.verification.events.v1` — V → D (3 partitions, retention 7d)
- `auto.create.topics.enable=false` — a producer publishing to a typo'd
  topic name fails with `UNKNOWN_TOPIC_OR_PARTITION` instead of
  silently creating a divergent topic.

## Bring it up

```sh
cd infrastructure/kafka
docker compose up -d
./topics-init.sh
```

The `topics-init.sh` script is idempotent — it uses
`kafka-topics.sh --if-not-exists` so re-running after the topics
already exist is a no-op.

## Talk to it from the host

`kcat` (or `kafka-console-consumer.sh` from a checkout of Kafka) reaches
the broker on `localhost:9092`:

```sh
# Tail the declaration events topic from the earliest offset.
kcat -b localhost:9092 -t recor.declaration.events.v1 -C -o beginning

# Tail the verification events topic.
kcat -b localhost:9092 -t recor.verification.events.v1 -C -o beginning
```

If you don't have `kcat` installed, use the broker's own console
consumer:

```sh
docker compose exec kafka /opt/bitnami/kafka/bin/kafka-console-consumer.sh \
    --bootstrap-server 127.0.0.1:9092 \
    --topic recor.declaration.events.v1 \
    --from-beginning
```

## Talk to it from the services

When the declaration / verification services run alongside this
compose stack (either in the same docker network or on the host),
they connect via:

- In-cluster network: `KAFKA_BROKERS=kafka:29092`
- Host network: `KAFKA_BROKERS=127.0.0.1:9092`

The `kafka-smoke.sh` script wires these envs automatically.

## Dual-transport during cutover

While `RELAY_TRANSPORT=kafka` is being rolled out, both transports
run in parallel. Each event lands once via the HTTP relay AND once
via Kafka. The verification engine's consumer is idempotent on
`event_id` (existing invariant from the HTTP path — see
`services/verification-engine/src/application/submit.rs`), so the
duplicate is absorbed without double-application of state.

After verification, flip `RELAY_TRANSPORT=kafka` to the default and
retire the HTTP path in a follow-up (R-LOOP-2 cutover, tracked in
ADR-0007).

## Tear down

```sh
docker compose down -v   # -v drops the named volume; offsets reset.
```

## Production deployment

This compose is **dev-only**. Production deployments target a
3+-broker replicated cluster:

- Operator: Strimzi (a Kubernetes-native Kafka operator)
- Topics: managed declaratively via `KafkaTopic` custom resources
- Security: SASL/SCRAM-SHA-512 for client auth, mTLS for inter-broker,
  and topic ACLs scoped per-service
- Retention: 7 days for v1 envelopes; the schema-registry follow-up
  may raise this when payload migration is needed for forensic replay

See `docs/adr/0007-kafka-transport-cutover.md` for the cutover plan
and the deprecation timeline for the HTTP relay.
