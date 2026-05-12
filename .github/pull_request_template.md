<!--
RÉCOR pull request — please fill out every section.
The pr-hygiene workflow (.github/workflows/pr-hygiene.yaml) verifies that
the required sections are present. Empty sections fail the check.
-->

## Ticket

<!-- Ticket ID from the sprint backlog (e.g. F-001, R-002, D-010). -->
**Ticket:** <ID>

<!-- Link to the ticket in the project tracker, if applicable. -->
**Link:** <URL or "n/a">

## Plan link

<!--
Per Doctrine 23, substantive work begins with a plan. Paste the plan that was
reviewed and approved (URL to the comment / discussion, or inline below).

Trivial changes (<50 net lines, single file, no schema, no contract): write
"no plan needed — trivial change" with a one-sentence justification.
-->

## Outcomes rubric

<!--
Per the Outcomes mechanism (Architecture V2 P5), every substantive feature
carries a rubric. Paste it (or link to it) here, plus the grading agent's
findings.

Trivial changes: write "no rubric — trivial change."
-->

## Summary

<!-- 1–3 sentences. What changed, and why. -->

## Doctrine checklist

<!--
Tick the doctrines that apply to this change. Doctrines 15, 17, 18, and 20
are unwaivable; if they apply, they MUST be honoured (no checkbox-skipping).
-->

- [ ] D01 Completeness — implementation + tests + docs + observability shipped together
- [ ] D02 Plan before code — plan linked above
- [ ] D04 Tests are part of the feature — tests in this PR
- [ ] D05 Documentation is part of the feature — docs in this PR
- [ ] D07 No workarounds where the real fix exists
- [ ] D08 No dangling threads — no TODOs without linked tickets; no commented-out code
- [ ] D10 Reviewability — PR under 500 lines net, or large-PR justification below
- [ ] D11 Two reviewers, at least one cross-team — CODEOWNERS will route
- [ ] D12 Production-grade from the first commit — no scaffolds, no "harden later"
- [ ] D13 Idempotency — state-changing operations are replay-safe (if applicable)
- [ ] D14 Fail-closed at integration boundaries (if applicable)
- [ ] D15 Cryptographic provenance — consequential events anchored (if applicable; UNWAIVABLE if applicable)
- [ ] D16 Observability — metrics + traces + logs + alerts (if applicable)
- [ ] D17 Zero trust at every network boundary (UNWAIVABLE if applicable)
- [ ] D18 No secrets in code, tickets, chat, logs (UNWAIVABLE)
- [ ] D20 Supply chain integrity — SLSA Level 4 (UNWAIVABLE for production artefacts)
- [ ] D22 Anthropic-primary AI inference — routing through the Inference Gateway (if applicable)
- [ ] D23 Plan Mode — substantive work planned before execution

## Large-PR justification

<!--
Required when net change exceeds 500 lines (Doctrine 10). Explain why the work
cannot be decomposed without leaving an inconsistent intermediate state.
-->

## Reviewers

<!-- CODEOWNERS will request reviews automatically; list any additional reviewers. -->

## Test plan

<!--
Bulleted checklist of how this PR was tested. Include both happy-path and
failure-mode coverage.
-->

- [ ] Local unit tests pass
- [ ] Local integration tests pass
- [ ] Manual verification — describe scenarios
- [ ] CI required-checks pass

## Rollback plan

<!--
How is this PR reverted if it causes a production issue? Forward-only fix?
Feature flag? Configuration change?
-->

## Linked artefacts

<!-- ADRs, runbooks, dashboards, other commits, related PRs. -->

---

<!--
Do NOT self-merge. Per Doctrine 11, two reviewer approvals are required,
including at least one cross-team. Merging your own PR is a doctrine
violation regardless of approval count.
-->
