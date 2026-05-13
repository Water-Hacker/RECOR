# Architecture Decision Records

This directory holds RÉCOR's Architecture Decision Records (ADRs).
Each ADR captures a single, significant architectural choice along
with its context, the alternatives considered, and the consequences
the team accepted.

## Format

We use the [MADR](https://adr.github.io/madr/) template. Each ADR
has:

- **Status** — Proposed, Accepted, Superseded
- **Decision-makers** — the team(s) that own the decision
- **Date** — the commit / merge date of the implementing change
- **Context** — the problem at the time of decision
- **Decision** — what was chosen, with code references
- **Consequences** — positive, negative, and neutral outcomes
- **Alternatives considered** — what was rejected, and why
- **References** — PRs, commits, files, related docs

ADRs are immutable once Accepted. A change of direction produces a
new ADR that supersedes the old one; the old ADR is annotated
"Superseded by ADR-NNNN" and stays in the directory as historical
record.

## Index

| ADR | Title | Status | Date |
|---|---|---|---|
| [ADR-0001](0001-event-sourcing-declaration-aggregate.md) | Event sourcing for the Declaration aggregate | Accepted | 2026-05-11 |
| [ADR-0002](0002-dempster-shafer-fusion.md) | Dempster-Shafer fusion for the Verification Engine | Accepted | 2026-05-11 |
| [ADR-0003](0003-http-outbox-relay-d-v.md) | HTTP outbox-relay for the D↔V loop (interim before Kafka) | Accepted (interim) | 2026-05-11 |
| [ADR-0004](0004-oidc-jwks-principal-authentication.md) | OIDC + JWKS for principal authentication | Accepted | 2026-05-11 |
| [ADR-0005](0005-hmac-channel-rotation.md) | Per-channel HMAC secrets with dual-secret rotation | Accepted | 2026-05-12 |
| [ADR-0006](0006-observability-stack-choice.md) | Observability stack — dev compose + production Helm | Accepted | 2026-05-11 |
| [ADR-0007](0007-kafka-transport-cutover.md) | Kafka transport cutover plan for the D↔V loop | Proposed | 2026-05-12 |

## How to add a new ADR

1. Pick the next available number (4-digit, zero-padded).
2. Copy the structure of an existing ADR (ADR-0001 is a good
   starting template).
3. Keep the file in the 500-1500-word range. Long enough to
   document the why; short enough that the next maintainer reads
   it.
4. Reference specific files, commits, and PRs. ADRs that say
   "we use OIDC for auth" without naming the implementing file or
   the hardening PR are unfalsifiable.
5. Be honest about tradeoffs. The Negative-Consequences section
   is where ADRs earn their keep — every decision has tradeoffs
   and the next maintainer needs to know them.
6. Link related ADRs and roadmap items. ADRs that supersede or
   feed into each other should cite the connection explicitly.

## How to supersede an existing ADR

1. Write the new ADR with status `Accepted (supersedes ADR-NNNN)`.
2. Edit the old ADR's status to `Superseded by ADR-MMMM (date)`.
3. Do not delete the old ADR. Future maintainers may need to
   understand the path the codebase took.

## Related documents

- `docs/architecture/` — the architecture commits ADRs implement
- `docs/ROADMAP.md` — open follow-up tickets, including ones that
  will eventually retire current ADRs (e.g. ADR-0003 retires when
  `R-LOOP-2` lands, ADR-0005 retires when `R-LOOP-3` lands)
- `docs/runbooks/` — operational procedures for the systems ADRs
  describe
- `.claude/agents/docs-author.md` — agent role definition for
  documentation work
