# Specialist agents for RÉCOR

This directory contains the definitions of the ten specialist sub-agents the
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
| architect-reviewer | Opus 4.7 | Architecture compliance reviews |
| security-reviewer | Opus 4.7 | STRIDE / OWASP / CWE reviews |
| test-author | Sonnet 4.6 | Test writing |
| docs-author | Sonnet 4.6 | Documentation writing |
| refactor-specialist | Opus 4.7 | Scoped refactors |
| migration-specialist | Opus 4.7 | Database migrations |
| integration-specialist | Opus 4.7 | Consumer integrations |
| incident-investigator | Opus 4.7 | Production incident investigation |
| verification-engine-specialist | Opus 4.7 | Verification engine work |
| lead-orchestrator | Opus 4.7 | Top-level coordination (default) |

## Modification

Agent definitions are reviewed by @recor/architect-team and @recor/security-team
per CODEOWNERS. Modifications require ADR documenting the rationale.
