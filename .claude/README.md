# .claude/ — Claude Code project configuration

This directory configures Claude Code for the RÉCOR monorepo.

## Files

- settings.json — permission policy (allow/deny/ask lists), hook bindings
- agents/ — specialist agent definitions
- skills/ — auto-discovered skills
- hooks/ — PreToolUse and PostToolUse hook scripts

## What gets committed; what doesn't

Committed:
- settings.json
- agents/*.md
- skills/**/*  (these define the team's operational discipline)
- hooks/*.sh

Not committed (in .gitignore):
- sessions/ — per-session transcripts
- transcripts/ — saved session transcripts
- cache/ — caches
- local-settings.json — per-engineer overrides

## Engineer setup

After cloning the repository:
1. `mise install` to get the right toolchain versions
2. `just bootstrap` to install everything else
3. Open Claude Code in the repository root
4. The configuration loads automatically; verify with `/agents list`

## Updating the configuration

Changes to anything in .claude/ are reviewed in the standard PR process.
Note that .claude/agents/ and .claude/skills/ have a CODEOWNERS entry
requiring architect-team approval; these are not casually-modified surfaces.

## Where to read more

- /docs/architecture/ V2 P5 (Claude Code Operating Manual)
- /docs/companion/ V2 P6-P11 (the actual artefacts in this directory)
- Anthropic's Claude Code documentation at https://docs.claude.com/
