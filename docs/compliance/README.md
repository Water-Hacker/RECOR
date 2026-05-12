# RÉCOR Compliance Documentation

Strategic index for the `docs/compliance/` directory. The procedures in
this directory translate the project's legal obligations under
Cameroonian law, the OHADA framework, and EU GDPR (for data subjects
located in the EU) into operational artefacts that engineering,
operations, and counsel each rely on.

> **Status disclaimer.** Every document in this directory carries a
> `[CITATION NEEDED: ...]` marker on the legal-basis lines that have
> not yet been signed off by AML/CFT counsel. The placeholders make
> the legal gaps explicit so they are reviewable; they are NOT to be
> treated as confirmed citations until counsel has annotated each one.
> The doctrines (D01 completeness, D05 documentation-is-part-of-the-feature)
> prevent these docs from shipping with the placeholders silently
> resolved by Claude or by an engineer; counsel sign-off is the
> remaining acceptance step.

## Layout

| Document | Purpose | Status |
|---|---|---|
| `gdpr-procedures.md` | Operational procedures for the six GDPR data-subject rights, mapped to the platform's endpoints and the OHADA AML/CFT carve-outs. | Draft, pending counsel sign-off (COMP-1). |
| `data-classification.md` | Per-column inventory: Public / Internal / Confidential / PII / Sensitive-PII. | Draft, pending counsel sign-off (COMP-3). |
| `data-retention.md` | Retention policy for every persisted store (event log, outbox, idempotency, projections, audit chain). | In force from Phase 0 (COMP-2 shipped). |
| `regulatory-mapping.md` | Endpoint → legal-provision map (REST + gRPC) AND invariant → legal-provision map under Cameroon law + OHADA + FATF Rec 24 + GDPR. | Draft, pending counsel sign-off (COMP-4). |
| `dr-drill-template.md` | Quarterly DR-drill record template; on-call copies it each quarter to `dr-drill-YYYY-Qn.md`. | In force (COMP-5). |
| `dr-drill-YYYY-Qn.md` | Per-quarter drill record produced by copying `dr-drill-template.md`. | Created quarterly; one file per quarter, never edited after sign-off. |

## How to use this directory

### When introducing a new endpoint, table, or persisted store

1. **Classify the data.** Decide whether the new fields are Public,
   Internal, Confidential, PII, or Sensitive-PII. Add a row to
   `data-classification.md` (or stub it if the document is not yet
   landed); cite the schema definition.
2. **Decide retention.** Add a row to `data-retention.md` (or stub
   it). Default policy is "indefinite for declaration_events and audit
   anchors; bounded TTL for outbox/idempotency tables".
3. **Map to legal basis.** Add the endpoint or table to
   `regulatory-mapping.md`. Every endpoint MUST cite the law that
   compels its existence — the registry is statutory infrastructure;
   no endpoint is built on consent alone.
4. **Update GDPR procedures.** If the new endpoint changes how a
   data-subject right is exercised (right of access, rectification,
   erasure, portability, restrict, object), update
   `gdpr-procedures.md` so the procedure text matches the
   implementation.
5. **Have counsel review** before the PR merges. Compliance docs are
   binding on the legal team's review board.

### When responding to a data-subject request

`gdpr-procedures.md` is the runbook. It describes the intake form,
the verification step, the response template, and the time bounds.

### When a regulator audits the platform

`regulatory-mapping.md` is the auditor's entry point. Every cited
provision must point to a specific clause; "see decree XYZ" without an
article number is not a citation.

### When running the quarterly DR drill

The DR drill (COMP-5) is a quarterly required exercise. The procedure
and the RTO/RPO commitments live in
`docs/runbooks/restore-from-backup.md`; the record template lives at
`dr-drill-template.md`. On-call:

1. Runs `bash scripts/dr-drill.sh` against a recent build of `main`.
2. Copies `dr-drill-template.md` to `dr-drill-YYYY-Qn.md` and fills in
   every field (including the observed RTO from the script's final
   line).
3. Opens a PR titled `chore(ops): YYYY Qn DR drill record` and asks a
   second on-call to co-sign.
4. Files follow-up tickets for any deviation from the procedure.

A missed quarter is itself a finding — see
`docs/runbooks/restore-from-backup.md` § Quarterly drill cadence.

## Authority hierarchy

When the documents in this directory conflict with each other or with
something else in the repo, this is the resolution order (per the
project doctrines and the Architecture Document V1 P2):

1. The Architecture Document (`/docs/architecture/`) — the platform's
   binding design.
2. The Implementation Companion (`/docs/companion/`) — the binding
   artefacts.
3. Compliance procedures (this directory) — the operational obligations.
4. ADRs (`/docs/adr/`) — the recorded design decisions; an ADR can
   refine but never override an Architecture chapter.
5. Service-level CLAUDE.md files — scoped operational instructions
   for each bounded context.

When a compliance procedure cannot be implemented because the
Architecture forbids it, the procedure is wrong and must be
re-negotiated with counsel; the Architecture wins.

## Out of scope for `docs/compliance/`

- **Information security** — the threat model lives at
  `docs/security/threat-model.md`. The compliance documents reference
  it but do not duplicate its content.
- **Incident response** — operational runbooks live at
  `docs/runbooks/`. Compliance documents reference the relevant
  runbooks (notification timelines, breach-disclosure templates) but
  the runbook is the canonical artefact.
- **ADRs** — Architecture Decision Records live at `docs/adr/`. A
  compliance procedure can cite an ADR; it does not embed one.
