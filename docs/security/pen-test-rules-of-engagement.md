# RÉCOR — penetration test Rules of Engagement (RoE)

**Ticket:** PEN-1 (Phase 5 — Pre-launch hardening).
**Companion document:** `docs/security/pen-test-prep.md`.
**Status of this template:** engineering draft. The final RoE the
vendor signs is the version Legal counter-signs alongside the
engagement contract and the NDA. The substantive clauses below are
load-bearing; clause numbering and the legal-recitation prose may be
restructured by counsel to fit the standard contract template.

> This document is a legal-grade contract. Every clause maps to a
> doctrine, a threat-model row, or a regulatory obligation. Vendor and
> RÉCOR sign before any staging access is provisioned. No clause may be
> waived in-engagement; any deviation requires a written amendment
> co-signed by both sides.

## 1. Parties and authority

- **The Platform Owner** (hereinafter "RÉCOR") — the consortium that
  operates the National Beneficial Ownership Registry of Cameroon,
  acting through the engineering and security teams identified in the
  contact graph (§ 9).
- **The Vendor** (hereinafter "the Vendor") — the contracted external
  penetration-testing organisation identified in the engagement
  contract. The Vendor warrants ISO 27001 certification and the
  experience criteria listed in `docs/security/pen-test-prep.md`
  § "Vendor selection criteria".

The signatories on both sides MUST be authorised to bind their
organisation. RÉCOR's signatory is the security-team lead per
`docs/security/teams.md`.

## 2. Engagement window

- **Start date:** YYYY-MM-DD HH:MM (Africa/Douala timezone).
- **End date:** YYYY-MM-DD HH:MM (Africa/Douala timezone).
- **Working hours:** the Vendor MAY exercise the platform 24×7
  during the window; on-call coverage is staffed accordingly.
- **Quiet hours:** none — but the Vendor SHOULD batch noisy probes
  to working hours so on-call is awake when telemetry spikes.
- **Window extensions:** any extension to the end date requires a
  written amendment to this RoE, co-signed by both sides. Drift past
  the end date without an amendment terminates the Vendor's
  authorisation; continued probing post-end-date is treated as an
  unauthorised intrusion and reportable to law enforcement.

The engagement window is **fixed before the contract is signed**.
Slippage on RÉCOR's side (engineering checklist not green in time)
slips the start date; slippage on the Vendor's side compresses the
end date — the budget envelope is unchanged either way.

## 3. NDA and confidentiality

- The Vendor signs RÉCOR's standard mutual NDA before any access.
  The NDA covers: staging credentials, internal architecture
  documents, threat-model gaps, incident-response runbooks, the
  contact graph, the vendor's preliminary findings, and the
  vendor's final findings prior to public disclosure.
- The NDA term is **5 years from engagement end date**, with a
  perpetual carve-out for items that remain non-public (e.g.
  Architecture Document chapters that never publish).
- The Vendor's engagement team is named in the contract; only named
  individuals receive access. Any change to the team mid-engagement
  requires a written amendment to the team roster.
- Vendor sub-contracting is forbidden. The engagement team operates
  in-house at the Vendor or terminates.

## 4. Authorised scope (what the Vendor MAY do)

The Vendor is authorised to perform the test objectives enumerated in
`docs/security/pen-test-prep.md` § "Test objectives" against the
staging-environment surfaces listed in § "Engagement scope" — In
scope. Specifically:

- Network-level probing of the staging public endpoints (port scans,
  TLS-config probes, HTTP request fuzzing).
- Application-level testing against every endpoint enumerated in
  `docs/openapi/declaration.json` and the gRPC reflection-discoverable
  RPC set.
- Authenticated testing with the issued OIDC test accounts (declarant
  and admin).
- DLQ admin-surface testing via the issued admin-allowlisted
  principal.
- HMAC writeback-channel testing using the issued single-slot HMAC
  secrets.
- Static analysis of source code in the cloned repository (Vendor
  receives read-only access to the same commit hash as is deployed
  in staging).
- Documentation review (threat model, ADRs, runbooks, regulatory
  mapping).

## 5. Forbidden actions (what the Vendor MUST NOT do)

The following actions are **forbidden** and treated as a contract
breach. Some are forbidden because they would damage real or
regulated data; others because they exceed the engagement's
authorised scope.

### 5.1 Data integrity

- The Vendor MUST NOT submit any declaration containing real
  personally-identifiable information. Every test declaration uses
  the seeded synthetic person UUIDs from `tests/e2e/fixtures.ts` or
  Vendor-generated synthetic data that does not resolve to a real
  natural person.
- The Vendor MUST NOT submit declarations against real legal-entity
  identifiers (RCCM numbers belonging to actual Cameroonian
  companies). Synthetic identifiers from the seeded fixture only.
- The Vendor MUST NOT exfiltrate any data from staging beyond what
  is necessary to evidence a finding. Evidence excerpts MUST be
  trimmed to the minimum repro material and redacted of any
  inadvertently-captured real PII.
- The Vendor MUST NOT attempt destructive operations on the
  database (mass DELETE / TRUNCATE / DROP) even where the threat
  model documents that the BEFORE trigger refuses such operations.
  The objective is to confirm the trigger holds; the test is the
  attempt, not the destruction.

### 5.2 Availability

- The Vendor MUST NOT execute volumetric denial-of-service attacks
  (per the out-of-scope clause in pen-test-prep). Rate-limit
  verification is bounded: confirm the limiter exists and is wired,
  do not flood the cluster.
- The Vendor MUST NOT crash the staging cluster as a goal. Probes
  MAY cause incidental restarts (a memory-safety bug in a service is
  a valid finding); the cluster is restored by the engineering team,
  not by the Vendor.
- The Vendor MUST NOT execute long-running fuzzing campaigns that
  consume staging quota for hours; coordinate timing with the
  engineering team when running fuzzers.

### 5.3 Lateral movement

- The Vendor MUST NOT pivot from the staging cluster into any
  adjacent network. The engagement is bounded to the staging
  environment's documented surfaces and to the source repository.
- The Vendor MUST NOT attempt to access operator workstations,
  RÉCOR engineering laptops, the production environment, the CI
  runner infrastructure, the consortium's internal email or chat,
  the OIDC issuer's administrative surface, or any third-party
  service the platform integrates with (BUNEC, sanctions feeds,
  Anthropic, Vault).
- The Vendor MUST NOT attempt to enumerate the RÉCOR organisation's
  cloud-provider accounts or DNS zones.
- The Vendor MUST NOT attempt social engineering against the
  RÉCOR team or its consortium partners. A separate red-team
  engagement covers human-factor testing.

### 5.4 Cryptographic primitives

- The Vendor MUST NOT execute cryptanalytic attacks on Ed25519,
  BLAKE3, HMAC-SHA256, or the underlying primitives used by OIDC
  signatures. Per pen-test-prep, primitive-level cryptanalysis is
  a research programme and out of scope. The Vendor MAY note
  primitive choices and the absence of a post-quantum-agility
  migration plan (Gap G6) in the report.

### 5.5 Production

- The Vendor MUST NOT under any circumstance probe the production
  environment. Production-environment access is not authorised by
  this RoE; any probe is an unauthorised intrusion and reportable
  to law enforcement.
- If the Vendor discovers, during the engagement, evidence that a
  finding may also exist in production, the Vendor stops testing
  that path, pages the primary on-call via the contact graph (§ 9),
  and waits for written instruction. The Vendor does not
  unilaterally "verify" the finding against production.

### 5.6 Disclosure

- The Vendor MUST NOT publicly disclose any finding before the
  coordinated-disclosure window agreed in § 10 expires.
- The Vendor MUST NOT publish the contents of the threat model,
  the regulatory-mapping document, or any other internal RÉCOR
  document.

## 6. Data handling

- All evidence captured during the engagement is treated as
  Confidential per the NDA. Evidence stored on Vendor systems MUST
  be encrypted at rest with a key under the Vendor's control;
  decommissioned on engagement close (90-day retention cap
  post-engagement-end).
- Real-PII captured by accident (e.g. a vendor probe surfaces a
  log entry containing a real declarant's `sub` claim) MUST be
  redacted at the earliest point and reported to RÉCOR's
  security-team lead within **4 hours of discovery**.
- The Vendor's final report is delivered via PGP-encrypted email
  to the security-team lead's published key. Plain-text delivery
  of any deliverable containing real-PII or staging credentials
  is forbidden.

## 7. Test data limits

Per pen-test-prep § "Engagement logistics":

- Maximum **5000 declarations** submitted during the engagement
  window across all test runs.
- Synthetic person UUIDs only (the seeded fixture set OR
  Vendor-generated UUIDs that do not collide with the fixture).
- Synthetic legal-entity identifiers only.
- The 5000-cap is a budget signal, not a denial-of-service
  proof; if the Vendor needs to exceed it for a specific objective,
  request approval from the engineering team before the run.

## 8. Liability boundaries

- The Vendor's professional-indemnity insurance covers the
  engagement at not less than the policy minimum specified in
  the engagement contract. Certificate of insurance is provided
  to RÉCOR before access is granted.
- RÉCOR's liability for incidental disruption caused by
  authorised Vendor activity (e.g. staging cluster needs a
  restart, an engineer's evening is consumed by an incidental
  outage) is bounded to staging time and engineering hours; the
  engagement contract may specify a credit mechanism but no
  damages flow either way for activity inside the authorised
  scope.
- Activity outside the authorised scope (per § 5) carries the
  Vendor's full professional and contractual liability. The
  Vendor's insurance MUST cover unauthorised-action damages up to
  the engagement contract's liability cap.
- Where Vendor activity causes a genuine production-impacting
  incident through misconfiguration on either side, the post-
  mortem (per DOC-3 incident-response-template) applies, and
  blameless review precedes any contractual remedy discussion.

## 9. Contact graph

Communication during the engagement flows through the contacts below.
Out-of-band identities (PGP keys, phone numbers) are exchanged at
engagement kickoff and not committed to this file (D18).

### RÉCOR side

| Role | Responsibility | Hours |
|---|---|---|
| Primary on-call (engineering) | First responder to vendor pages; routes to specialists | 24×7 during engagement |
| Secondary on-call (engineering) | Escalation if primary is unreachable | 24×7 during engagement |
| Security-team lead | Critical / High finding intake; out-of-band channel owner | Working hours; on-call for SEV-1 escalation |
| Engineering-team lead | Code-base questions; staging-cluster authority | Working hours |
| Counsel (legal) | Contract clarifications; PII-handling questions | Working hours |
| Procurement lead | Contract-amendment authority | Working hours |

### Vendor side

| Role | Responsibility | Hours |
|---|---|---|
| Engagement lead | Primary point of contact; final-report author | Working hours |
| Technical lead | Day-to-day testing direction | 24×7 during engagement |
| Vendor counsel / contracts | NDA + amendment authority | Working hours |

### Escalation timeline

- **0-15 min**: Vendor pages primary on-call. Primary acknowledges
  within 15 min. If silent, Vendor pages secondary.
- **15-60 min**: Primary triages per `docs/runbooks/oncall-triage-tree.md`;
  if the finding is Critical, primary engages security-team lead.
- **60+ min**: security-team lead engages engineering-team lead; if
  production may be affected, the engineering-team lead invokes
  LAUNCH-1 § "Rollback triggers" against the soft-launch playbook.

## 10. Disclosure protocol

- **During the engagement**: findings flow only between Vendor and
  RÉCOR via the secure channel.
- **At engagement end**: Vendor delivers the primary report
  (PGP-encrypted, PGP-signed) within **5 business days** of the
  engagement end date.
- **Embargo window**: **90 days from primary-report delivery**
  before Vendor MAY publish anything about the engagement. Within
  the 90 days, RÉCOR ships mitigation PRs for every Critical / High
  finding; the embargo extends by mutual agreement if a Critical
  finding requires a major architectural change.
- **Public summary**: RÉCOR publishes the redacted summary at
  `docs/security/pen-test-report-{date}.md` on or before the
  embargo end. The Vendor MAY also publish a case study (with
  RÉCOR's prior written approval) after the embargo end; case-study
  content is constrained by the perpetual-confidentiality NDA
  carve-outs.

## 11. Re-test and follow-up

- Any Critical or High finding triggers a vendor-cost re-test on
  the engineering team's mitigation PR, bounded to the affected
  objective(s).
- Vendor re-test is delivered within **10 business days of the
  engineering team's request**.
- Medium / Low findings batch into a single follow-up; no automatic
  re-test, but the next quarterly engagement re-checks them as part
  of regular coverage.

## 12. Termination

This RoE may be terminated:

- **By mutual agreement** at any point.
- **By either side with 24-hour notice** before the engagement
  start date.
- **By RÉCOR immediately** if the Vendor breaches any clause in
  § 5 (forbidden actions). Staging access is revoked the same hour.
- **By the Vendor immediately** if RÉCOR fails to maintain the
  staging environment within the agreed availability envelope for
  a sustained period (defined in the engagement contract).
- **Automatically** at the engagement end date unless extended in
  writing.

Termination does not extinguish the NDA, the confidentiality
obligations, or the embargo on disclosure.

## 13. Independence and verification

Per doctrine D17, RÉCOR independently reproduces every Critical and
High finding before treating it as authoritative. The Vendor's claim
alone is not load-bearing; the reproducible repro steps in the
primary report are. The Vendor undertakes to make repro steps
sufficient for an engineering team unfamiliar with the Vendor's
tooling to reproduce the finding from the report alone.

Per doctrine D15, the primary report is PGP-signed by the Vendor
lead. RÉCOR verifies the signature against the key registered at
kickoff before accepting the report. An unsigned report is rejected.

## 14. Doctrines invoked

This RoE is the operational instantiation of:

- **D07 (no workarounds)** — gaps are gaps; the Vendor exercises
  them as ACCEPTED-RISK objectives, not findings.
- **D14 (fail-closed)** — forbidden actions are explicitly listed.
- **D15 (cryptographic provenance)** — Vendor's deliverable is
  signed.
- **D17 (zero trust)** — every Critical / High finding is
  independently reproduced.
- **D18 (no secrets)** — credentials never appear in this file or
  any committed file; out-of-band only.
- **D24 (the standard is non-negotiable)** — clauses do not flex
  in-engagement.

## 15. Signatures

The following signatories bind their organisation to this RoE.

| Role | Name | Signature | Date |
|---|---|---|---|
| RÉCOR security-team lead | _______________ | _______________ | _______________ |
| RÉCOR counsel | _______________ | _______________ | _______________ |
| Vendor engagement lead | _______________ | _______________ | _______________ |
| Vendor counsel | _______________ | _______________ | _______________ |

(Signatures captured on the final-version printed instance held by
RÉCOR procurement; this committed file is the canonical text and is
referenced by the engagement contract.)

## 16. Related documents

- `docs/security/pen-test-prep.md` — the technical engagement brief.
- `docs/security/threat-model.md` — the scope reference.
- `docs/security/README.md` — security documentation index.
- `docs/security/branch-protection.md` — change-control posture.
- `docs/runbooks/oncall-triage-tree.md` — escalation entrypoint.
- `docs/runbooks/incident-response-template.md` — post-mortem
  template (the engagement may produce incidents; this is how
  they get documented).
- `docs/runbooks/soft-launch-playbook.md` — Stage 0 entry gates
  shared with PEN-1.
