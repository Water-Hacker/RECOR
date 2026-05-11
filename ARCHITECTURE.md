# RÉCOR — Architecture overview

This file is a navigational signpost, not the authoritative architecture text.
The binding reference is the Software Architecture Document at
`docs/architecture/RECOR-Software-Architecture-Document.docx`, which the
Implementation Companion (`docs/companion/`) materialises.

## What RÉCOR is, in one paragraph

RÉCOR is a sovereign-grade adversarial entity-resolution platform that forces
every legal entity operating in Cameroon to disclose the natural person who
ultimately controls it, verifies that disclosure against concealment, and serves
the verified truth to the eight institutional consumers whose mandates depend on
it. The platform is governed by a ten-organisation consortium with cryptographic
threshold-signed quorum on consequential operations, and is engineered against
the 24 strict engineering doctrines documented in Architecture V1 P2.

## The seven layers

| Layer | Function | Reference |
|-------|----------|-----------|
| L0 | Cryptographic & sovereignty substrate (HSMs, Fabric ledger, FROST, Halo2, OpenTimestamps) | Architecture V4 P11 / Companion V4 P13 |
| L1 | Storage substrate (PostgreSQL, Neo4j, OpenSearch, Kafka, MinIO, Redis) | V4 P12 / C V4 P14 |
| L2 | Domain services — twelve bounded contexts | V4 P13 / C V4 P15–P16 |
| L3 | Verification engine — nine-stage pipeline, eight pattern signatures, Dempster–Shafer fusion | V4 P14 / C V4 P17 |
| L4 | APIs & integration fabric (GraphQL, REST, webhooks, BODS export) | V4 P15 / C V4 P18 |
| L5 | Consumer integrations — eight contractual integrations | V4 P16 / C V4 P19 |
| L6 | Applications — six role-specific surfaces | V4 P17 / C V4 P20 |

## The three inference tiers

| Tier | Volume share | Primary model | Eligible data |
|------|--------------|---------------|---------------|
| A | ~60% | Claude Opus 4.7 via Anthropic API (Sonnet 4.6 fallback) | Public |
| B | ~30% | Claude Opus 4.7 via Bedrock PrivateLink af-south-1 (Sonnet 4.6 fallback) | Pseudonymised PII-derived features |
| C | ~10% | Llama 3.3 70B Instruct on in-country H100s (Mistral Large 2 fallback) | Raw PII |

Routing is enforced at the Inference Gateway by data-classification tag, not by
caller identity. Cross-tier fallback is forbidden. Approximately 90% of all
inference workload routes to Anthropic. See Doctrine 22.

## The 24 engineering doctrines

Architecture V1 P2 documents 24 strict doctrines binding on every contribution.
Four (15, 17, 18, 20) cannot be waived under any circumstance. See
`docs/architecture/` for the full text.

## Where to start reading

- New engineer: `CONTRIBUTING.md` → Architecture V1 P2 (doctrines) →
  Architecture V2 P5 (Claude Code) → the CLAUDE.md for the service you'll touch.
- Reviewer: Architecture V1 P2 + V2 P6 (workflows).
- Operator: Engineering Operations Manual (separate artefact) + Architecture V5.
- Security: Architecture V5 P23 + `docs/security/`.
