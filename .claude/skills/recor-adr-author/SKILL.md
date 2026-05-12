---
name: recor-adr-author
description: ADR drafting. Fires when the user requests an ADR, when a design decision is being made, or when the architect-reviewer flags that an undocumented design decision is being made. Produces ADRs that follow the project's template.
---

# Author ADRs for RÉCOR

ADRs document substantive design decisions. They are stored in /docs/adr/ and
numbered sequentially.

## When you write an ADR

- A design decision is being made that future engineers would benefit from
  understanding
- The architect-reviewer flagged that an undocumented decision is being made
- A prior decision is being reversed (write a new ADR superseding the old one)
- A trade-off is being made that the team will need to defend later

## When you DON'T write an ADR

- Implementation choices that follow established patterns
- Naming decisions
- Cosmetic refactors
- Code style decisions (those are in the style guide)

## Template

Use /docs/adr/template.md (also in Companion V1 P4). The template requires:

- **Context**: why the decision is being raised now; 2-4 paragraphs
- **Decision**: 1-2 sentences with technical specifics
- **Considered alternatives**: at least 2 alternatives documented
- **Consequences**: easier / harder / new commitments / obsolete commitments
- **Doctrines applied**: which doctrines are relevant and how honoured
- **Document references**: which Architecture sections are affected
- **Implementation**: planned / in progress / implemented

## Quality bar

An ADR that says "we chose X because it's the best option" without alternatives
is not a useful ADR. An ADR that says "we considered Y but didn't choose it"
without saying why is not a useful ADR.

## Numbering

Find the next ADR number with: `ls docs/adr/ | grep -E '^[0-9]+-' | sort | tail -1`

## Naming

Filename: `<NNNN>-<imperative-short-title>.md`
- NNNN: zero-padded four-digit sequential number
- Title: imperative verb phrase, hyphenated, all lowercase

Example: `docs/adr/0027-route-tier-c-inference-through-sovereign-cluster.md`

## After writing

The ADR is committed in the PR that introduces the decision's implementation
(or in the PR that decides to defer the implementation). The ADR is reviewed
in the standard PR process; the architect-reviewer is auto-invoked.
