# Specialist agents for RÉCOR

This directory contains the definitions of the specialist sub-agents the
lead orchestrator delegates to.

Each agent file is markdown with YAML frontmatter. The frontmatter specifies:
- name: how the agent is invoked
- description: what the agent does (matched against user intent)
- model: which model the agent runs on
- tools: which tools the agent can call

The body of the file is the agent's system prompt.

## Agents

| Agent | Model | Scope |
|-------|-------|-------|
| **Implementation roles (ship code)** | | |
| rust-service-engineer | Opus 4.7 | Rust services: domain, use cases, infrastructure, API, migrations |
| typescript-frontend-engineer | Opus 4.7 | Declarant portal: React/TS/Vite/Tailwind |
| infrastructure-engineer | Opus 4.7 | Docker, K8s, Helm, observability, CI/CD, Vault |
| security-engineer | Opus 4.7 | TLS, secrets, PII redaction, security headers, threat-model implementation |
| integration-specialist | Opus 4.7 | External adapters (BUNEC, sanctions, PEP, ICIJ, Anthropic) |
| verification-engine-specialist | Opus 4.7 | Verification engine pipeline + fusion math |
| migration-specialist | Opus 4.7 | Database migrations + data backfills |
| **Review / advisory roles (read-only)** | | |
| architect-reviewer | Opus 4.7 | Architecture compliance reviews |
| security-reviewer | Opus 4.7 | STRIDE / OWASP / CWE reviews |
| **Cross-cutting** | | |
| test-author | Sonnet 4.6 | Tests across all layers (Playwright E2E, contract, fuzz) |
| docs-author | Sonnet 4.6 | ADRs, runbooks, threat-model docs, regulatory mapping |
| refactor-specialist | Opus 4.7 | Scoped refactors |
| incident-investigator | Opus 4.7 | Production incident investigation |
| lead-orchestrator | Opus 4.7 | Top-level coordination (default) |

See [`docs/PRODUCTION-TODO.md`](../../docs/PRODUCTION-TODO.md) for the
roadmap that assigns each remaining ticket to one of these roles.

## Modification

Agent definitions are reviewed by @recor/architect-team and @recor/security-team
per CODEOWNERS. Modifications require ADR documenting the rationale.
