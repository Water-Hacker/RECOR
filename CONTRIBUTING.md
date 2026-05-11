# Contributing to RÉCOR

This is a sovereign national infrastructure project. Contributions follow the
documented engineering doctrines and the consortium's review processes. The brief
version is below; the full version is in `docs/architecture/` V1 P2 (doctrines)
and V2 P6 (workflows).

## Before contributing

1. **Complete onboarding.** Read the 24 doctrines (Architecture V1 P2) and the
   OPSEC discipline (Architecture V1 P4). Onboarding completion is recorded by
   the personnel security function.

2. **Understand Claude Code's role.** The majority of code in this repository is
   produced by Claude Code agents on Opus 4.7 under human direction. Read
   Architecture V2 P5 and Companion V2 before initiating agent-assisted work.

3. **Configure your environment.** `just bootstrap` is the entry point.
   Companion V3 P12 has per-language details.

## How to contribute

1. **Plan first.** Open a ticket; produce a substantive plan; have it reviewed by
   the architect-reviewer agent and (where appropriate) by the lead architect.
   Implementation does not begin before the plan is approved.

2. **Implement with the appropriate agents.** Use the specialist agent roster
   documented in Companion V2 P9. Engineers do not freely substitute agents
   without documented rationale.

3. **Test as you implement.** Doctrine 4: tests are part of the feature.

4. **Document as you implement.** Doctrine 5: documentation is part of the feature.

5. **Pass the outcomes rubric.** Every substantive deliverable has a rubric;
   the grading agent evaluates the deliverable against the rubric before human
   review.

6. **Get two reviews.** Doctrine 11: two reviewers, at least one cross-team.
   Reviewers approve only what they have read.

## Pull request expectations

- Linked ticket
- Linked plan (the substantive plan from step 1)
- Linked outcomes rubric and grading agent's output
- Conventional Commits message
- Under 500 lines net change (justify any larger size)
- All CI gates passing

## What we will reject

- PRs without tests (Doctrine 4)
- PRs without documentation (Doctrine 5)
- PRs that introduce workarounds where a real fix exists (Doctrine 7)
- PRs that leave dangling threads (Doctrine 8)
- PRs that violate the doctrine-check agent's automated checks
- PRs that bypass the planning step (Doctrine 23)

## Reviewer accountability

Reviewer approval is not a courtesy. Approving a PR that violates a doctrine is
itself a doctrine violation and is detected through retrospective sampling.

## Getting help

- Engineering questions: `#engineering` on Mattermost
- Architecture questions: ping `@architect-team`
- Security questions or concerns: `#security-private` (request access)
- Doctrine clarifications: lead architect
