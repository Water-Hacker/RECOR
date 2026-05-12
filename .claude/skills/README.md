# RÉCOR Skills Catalogue

This directory contains the eleven skills that auto-discover based on user
intent. Each skill is a folder containing SKILL.md (the definition) plus any
supporting templates or scripts.

## How skills work

Claude Code reads all SKILL.md files at session start. The `description` field
in each file's YAML frontmatter is matched against the user's request. When a
match is found, the skill's content is loaded into the conversation.

A skill that doesn't load is usually a description problem: the words in the
description don't match the words the engineer (or you, the agent) typically
use to describe the work.

## Skills

- recor-doctrine-check — first line of doctrine enforcement
- recor-adr-author — ADR drafting
- recor-test-pyramid — test writing at appropriate ratios
- recor-rust-service — Rust service scaffolding
- recor-go-service — Go service scaffolding
- recor-react-app — React application scaffolding
- recor-migration — database migration work
- recor-integration-contract — consumer integration work
- recor-incident-investigation — production incident investigation
- recor-security-review — security review
- recor-doc-author — documentation writing

## Modification

Skills are reviewed by @recor/architect-team per CODEOWNERS. The descriptions
are particularly important — they determine when the skill fires. Description
changes are reviewed for retrieval accuracy.

## Skill testing

Each skill has a tests/ subdirectory with scenarios that exercise the skill.
The grading agent runs the scenarios after any skill change; regressions
in retrieval or output block the merge.
