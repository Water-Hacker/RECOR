#!/usr/bin/env bash
# R-LOOP-2 — create the v1 topics on the local dev Kafka.
#
# Topics:
#   recor.declaration.events.v1   — D → V transport (replaces the
#       declaration service's HTTP outbox relay during the cutover)
#   recor.verification.events.v1  — V → D transport (replaces the
#       verification engine's HTTP writeback relay)
#
# Both topics: 3 partitions (so a future cluster scale-out spreads
# keys evenly), retention 7 days (v1 — schema-registry follow-up may
# raise this), keyed by aggregate_id at produce time so all events
# for one declaration land on one partition (preserves order).
#
# Invocation (from this directory, after `docker compose up -d`):
#   ./topics-init.sh
#
# Idempotent: re-running is a no-op once the topics exist.
#
# D14 fail-closed: AUTO_CREATE_TOPICS is off on the broker. The
# declaration producer publishing to an unknown topic name will
# get UNKNOWN_TOPIC_OR_PARTITION instead of silently creating a
# typo'd topic.

set -euo pipefail

BROKER="${KAFKA_BROKER:-127.0.0.1:9092}"
COMPOSE_FILE="$(dirname "$0")/docker-compose.yaml"

# Wait for the broker to accept API requests. The healthcheck already
# does this, but compose's `up -d` returns before healthy.
echo "── waiting for Kafka at ${BROKER} ──"
for i in {1..30}; do
    if docker compose -f "$COMPOSE_FILE" exec -T kafka \
        /opt/bitnami/kafka/bin/kafka-broker-api-versions.sh \
        --bootstrap-server "${BROKER}" >/dev/null 2>&1; then
        echo "  Kafka ready after ${i}s"
        break
    fi
    sleep 1
    if [ "$i" = "30" ]; then
        echo "FAIL: Kafka did not become ready in 30s"
        docker compose -f "$COMPOSE_FILE" logs kafka | tail -30
        exit 1
    fi
done

create_topic() {
    local topic="$1"
    local partitions="$2"
    local retention_ms="$3"
    echo "── ensuring topic ${topic} (partitions=${partitions}, retention=${retention_ms}ms) ──"
    docker compose -f "$COMPOSE_FILE" exec -T kafka \
        /opt/bitnami/kafka/bin/kafka-topics.sh \
        --bootstrap-server "${BROKER}" \
        --create \
        --if-not-exists \
        --topic "${topic}" \
        --partitions "${partitions}" \
        --replication-factor 1 \
        --config "retention.ms=${retention_ms}" \
        --config "cleanup.policy=delete"
}

# 7 days in milliseconds.
RETENTION_7D_MS=604800000

create_topic "recor.declaration.events.v1" 3 "${RETENTION_7D_MS}"
create_topic "recor.verification.events.v1" 3 "${RETENTION_7D_MS}"

echo ""
echo "── topics now on the broker ──"
docker compose -f "$COMPOSE_FILE" exec -T kafka \
    /opt/bitnami/kafka/bin/kafka-topics.sh \
    --bootstrap-server "${BROKER}" \
    --list

echo ""
echo "── topic descriptions ──"
docker compose -f "$COMPOSE_FILE" exec -T kafka \
    /opt/bitnami/kafka/bin/kafka-topics.sh \
    --bootstrap-server "${BROKER}" \
    --describe \
    --topic "recor.declaration.events.v1,recor.verification.events.v1" || true

echo ""
echo "OK"
