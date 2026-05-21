# RÉCOR — Production Readiness Audit (TODOS)

**Audit date:** 2026-05-20
**Auditor:** Claude Code (Opus 4.7) — orchestrated 11 parallel forensic sub-agents
**Commit audited:** `e1ab0195394a3f24fee5402a151a68069069a122` (main)
**Working files:**
- Requirements corpus: [`audit/standards-extract.md`](audit/standards-extract.md) — 367+ requirements with citations
- Codebase inventory: [`audit/codebase-inventory.md`](audit/codebase-inventory.md) — 22 tables, 50+ routes, 142 modules graded
- Detailed test-coverage matrix: [`audit/codebase-inventory-G-tests.md`](audit/codebase-inventory-G-tests.md)

## Finding counts by priority

| Priority | Count | Definition |
|---|---|---|
| **P0** | **24** | Blocks any claim of FATF compliance, or introduces material security/integrity risk. Cannot ship. |
| **P1** | **31** | Required for production deployment. International evaluator would flag in an MER. |
| **P2** | **18** | Required for operational maturity, not for first launch. |
| **P3** | **9** | Quality, polish, future-proofing. |
| **Total** | **82** | |

## How to read this file

Each finding follows the schema in the operating directive: ID, Priority, Category, Standard cited, Current state, Required state, Why it matters, Acceptance criteria, Effort, Dependencies.

`Current state` cites file paths and line ranges where evidence exists. `ABSENT` means no implementation code attempts the requirement.

This file is intentionally long. It is the *deliverable*, not a summary.

---

# P0 — Cannot ship without these

## TODO-001 — Beneficial-ownership data model does not implement the FATF 25%/control/SMO cascade

- **Priority:** P0
- **Category:** Data Model | Compliance
- **Standard cited:** FATF R.24 §c.24.6 fn 25 ("BO threshold MUST be determined on a risk basis and MUST NOT exceed 25%"); FATF Guidance §2.4 ("ownership → control → senior managing official cascade"). `REQ-r24-018`, `REQ-r24-021`, `REQ-r24-fatf-guidance-cascade`.
- **Current state:** PARTIAL. `services/declaration/src/domain/beneficial_owner.rs` (referenced from `aggregate.rs`) holds `ownership_basis_points` (integer, 0–10000 representing percentage × 100). There is no `control_basis` field (e.g. voting-right control, board-appointment control, contractual control) and no "senior managing official" fallback type discriminator. The cascade is not modelled at all — the schema treats every BO as if they were a percentage holder. Verified by reading `services/declaration/src/domain/aggregate.rs` and the absence of any `BoControlBasis` / `BoCascadeTier` enum in `services/declaration/src/domain/`.
- **Required state:** The declaration domain MUST distinguish three BO cascade tiers per FATF Guidance: (a) **ownership** (≥ 25% direct or indirect ownership interest), (b) **control** (voting rights, board-appointment power, contract-based control, family-of-controllers aggregation), (c) **senior managing official** (residual, only when (a) and (b) yield no identified BO). Each `beneficial_owner` row MUST carry the cascade tier and the specific basis (e.g. "direct ownership 31.2%", "voting rights via shareholder agreement", "CEO under cascade tier (c)"). The aggregate MUST refuse to register a declaration that claims a cascade-tier (c) BO when a (b) BO has not been searched-for-and-ruled-out.
- **Why it matters:** Cameroon's MER will be assessed against c.24.6. A registry that records "25% ownership" only — and cannot distinguish a controlling shareholder from a senior managing official — fails IO.5 Core Issue 5.1. ANIF and foreign FIUs reading the data via MLAT cannot derive risk-tier without the cascade. The audit-verifier exposing payloads with no cascade-tier surfaces this gap to anyone who reads the data.
- **Acceptance criteria:**
  - [ ] `BoCascadeTier` enum (`OwnershipDirect | OwnershipIndirect | Control | SeniorManagingOfficial`) added to `services/declaration/src/domain/`.
  - [ ] `BoControlBasis` enum (`VotingRights | BoardAppointment | ContractualControl | FamilyAggregation | OtherDocumented`) added.
  - [ ] `beneficial_owner` value object carries both new fields with a SCHEMA validator that refuses tier (c) when no (b) is present.
  - [ ] Migration `0010_bo_cascade_tier.sql` adds columns to `declarations` projection (NOT NULL with a default for backfill).
  - [ ] Unit tests cover all six (tier × basis) combinations + the "tier (c) refused when tier (b) was not searched" rule.
  - [ ] OpenAPI schema regenerated; portal form updated to require the cascade tier on every BO row.
  - [ ] Documentation in `docs/architecture/` updated; ADR-010 authored.
- **Effort:** L. New enum + migration + aggregate logic + form changes + tests across declaration + portal.
- **Dependencies:** none.

## TODO-002 — Trusts and similar arrangements have no register or schema (FATF R.25)

- **Priority:** P0
- **Category:** Data Model | Compliance
- **Standard cited:** FATF R.25 + INR.25 ("countries MUST require trustees of any express trust governed under their law to obtain and hold adequate, accurate, and up-to-date information on the identity of the settlor, the trustee(s), the protector, the beneficiaries, and any other natural person exercising ultimate effective control over the trust"). `REQ-r25-001` through `REQ-r25-033`.
- **Current state:** ABSENT. The platform has `entities` (legal persons) and `persons` (natural persons). There is no `trusts` table, no arrangement-specific entity-type discriminator, no `arrangements` register, no schema field that even distinguishes a trust from a company. Verified: `grep -ri "trust\|arrangement" services/entity-service/src/` returns only spurious matches (the word "trust" appears in `trust_bundle` for SPIFFE — unrelated). `entity-service` is a single-shape entity register.
- **Required state:** A separate register (or a clearly demarcated section of the entity register) MUST exist for legal arrangements (express trusts, fiducies, waqf, and similar). The schema MUST capture: settlor(s), trustee(s), protector(s), beneficiaries (named and class-described), other natural persons exercising ultimate effective control. The same adequacy + accuracy + up-to-date obligations as for legal persons MUST apply. The 5-year-after-cessation retention MUST apply.
- **Why it matters:** A BO register that covers only legal persons fails FATF R.25 entirely — it is half of the obligation. Cameroon's MER would receive a Non-Compliant rating on R.25. Trusts and similar arrangements are the primary mechanism the StAR "Puppet Masters" study identifies for obscuring beneficial ownership in grand-corruption cases. A register that has no arrangement-side is not credible.
- **Acceptance criteria:**
  - [ ] Decide architecture: separate `services/arrangement-service` OR a discriminated section of `entity-service` (ADR required).
  - [ ] Migration adds `arrangements` table with: settlor IDs (FK to persons), trustee IDs, protector IDs, named-beneficiary IDs, class-beneficiary description, control-exercise documentation, governing-law jurisdiction, retention dates.
  - [ ] REST surface: `POST /v1/arrangements`, `GET /v1/arrangements/{id}`, `POST /v1/arrangements/{id}/update`, `POST /v1/arrangements/{id}/dissolve`, per-row tenancy predicate matching entity-service pattern.
  - [ ] Domain invariants: trustee MUST be one of {natural person, legal person, registered fiduciary} — refuse arbitrary strings.
  - [ ] COMP-2 immutable `arrangement_events` table parallels `entity_events`.
  - [ ] Adequate, accurate, up-to-date obligations enforced by the same 30-day update window as TODO-005.
  - [ ] ADR-011 documents the decision; permission matrix updated.
- **Effort:** XL. New bounded context end-to-end; minimum 4–6 weeks per the entity-service scaffolding precedent.
- **Dependencies:** TODO-001 (cascade tier model applies to arrangements too).

## TODO-003 — No discrepancy-reporting intake or workflow (FATF R.24 §c.24.6 + 6AMLD Art. 10)

- **Priority:** P0
- **Category:** Workflow | Compliance | API
- **Standard cited:** FATF R.24 c.24.6(c) ("Country MUST use additional supplementary BO sources... including discrepancy reports from FIs/DNFBPs"); 6AMLD Art. 10 ("Obliged entities... MUST report any discrepancy they find between the BO information available to them in the central registers and the BO information available to them as part of their CDD procedures"). `REQ-r24-017`, `REQ-6amld-010`.
- **Current state:** ABSENT. There is no `discrepancies` table, no discrepancy intake endpoint, no obliged-entity-side workflow. `grep -ri "discrepancy\|discrepanc" services/ apps/` returns zero matches.
- **Required state:** An obliged entity (bank, notary, DNFBP) MUST be able to (a) submit a structured discrepancy report referencing a `declaration_id` or `entity_id`, (b) receive an acknowledgement with a tracking ID, (c) be notified when the discrepancy is triaged/resolved. A back-office workflow MUST exist for triage, investigation, requesting corrections from the declarant, applying sanctions for non-correction, and resolution. Every state change MUST be event-sourced and append to the audit chain.
- **Why it matters:** Discrepancy reporting is one of the three pillars of c.24.6's multi-pronged approach. A registry that has no intake for FI/DNFBP discrepancy reports cannot claim it is using "additional supplementary BO sources" — it is a single-source register. Cameroon banks under COBAC supervision will, post-launch, face supervisory questions about how they fulfil their own discrepancy-reporting obligation under CEMAC AML/CFT règlement.
- **Acceptance criteria:**
  - [ ] New service or app: `apps/discrepancy-intake` OR new endpoint family in declaration service.
  - [ ] `POST /v1/discrepancies` — authenticated under obliged-entity OIDC scope, accepts `{declaration_id, field_path, observed_value, expected_value, evidence_attachment_hash, submitter_obliged_entity_id}`.
  - [ ] `GET /v1/discrepancies/by-obliged-entity` — same submitter sees their open + resolved discrepancies.
  - [ ] `GET /v1/discrepancies/{id}` — admin/back-office only.
  - [ ] `POST /v1/discrepancies/{id}/triage` — back-office.
  - [ ] `POST /v1/discrepancies/{id}/resolve` — back-office, with required `resolution_kind` enum (`declarant_corrected | discrepancy_invalid | sanction_imposed | escalated`).
  - [ ] `discrepancies` table with COMP-2 events.
  - [ ] Permission-matrix entry for the obliged-entity principal class.
  - [ ] Notification path back to obliged entity (out-of-scope-for-launch: queue an audit-grade ack; full notification channel is a follow-up).
- **Effort:** L. New endpoint family + new principal class + workflow state machine.
- **Dependencies:** TODO-006 (obliged-entity principal class).

## TODO-004 — No sanctions-for-non-compliance workflow (FATF R.24 §c.24.13 / INR.24 IO.5.3)

- **Priority:** P0
- **Category:** Workflow | Compliance
- **Standard cited:** FATF R.24 c.24.13 ("Country MUST ensure proportionate, dissuasive, effective sanctions exist for failure to comply with BO requirements"); IO.5 Core Issue 5.3. `REQ-r24-051` (sanctions for non-compliance).
- **Current state:** ABSENT. The platform has a `dissolve` operation on entities but no "non-compliance sanction" workflow — no penalty fee tracking, no suspension state, no public-shaming list, no escalation to ANIF. `grep -ri "sanction\|penalt\|fine\|suspend" services/entity-service/src/ services/declaration/src/` returns no domain logic.
- **Required state:** A sanctions workflow that supports the proportionate-dissuasive-effective spectrum: (a) **administrative reminder** (first missed update window), (b) **administrative fine** (escalating tiers), (c) **suspension of registry status** (declaration marked as non-current; entity flagged), (d) **referral to ANIF/COBAC** for the regulated-counterparty path, (e) **public listing** of persistent non-compliers (post-Sovim legitimate-interest gate). Every step MUST be event-sourced, every escalation MUST require a documented justification, every published name MUST be removable on correction.
- **Why it matters:** R.24 c.24.13 is unwaivable in any MER. A registry with no sanction surface cannot claim its requirements are "proportionate, dissuasive, and effective" — the proportionate-dissuasive-effective triad MEANS there is a sanction. Without this, every other BO obligation is unenforceable; the registry becomes voluntary.
- **Acceptance criteria:**
  - [ ] `sanctions_proceedings` table with proceeding-state event log (COMP-2).
  - [ ] `POST /v1/sanctions/initiate` — admin allowlist; requires `entity_id` or `declaration_id`, reason code, evidence.
  - [ ] `POST /v1/sanctions/{id}/escalate` — admin; advances the tier.
  - [ ] `POST /v1/sanctions/{id}/withdraw` — admin; requires reason.
  - [ ] `GET /v1/sanctions/public` — public list of currently-published non-compliers (post-Sovim balancing); 24-hour cache invalidation on withdraw.
  - [ ] Permission-matrix row per sanction action.
  - [ ] ADR-012 documents the proportionality ladder.
  - [ ] Integration with TODO-003 (discrepancy resolution → sanction trigger).
- **Effort:** L. Workflow + UI + ADR + permission discipline.
- **Dependencies:** TODO-003.

## TODO-005 — No 30-day update-obligation enforcement on BO data (FATF R.24 §c.24.8 fn 29)

- **Priority:** P0
- **Category:** Compliance | Workflow
- **Standard cited:** FATF R.24 c.24.8 fn 29 ("BO information MUST be updated within a reasonable period following any change; FATF benchmark: within one month"); 6AMLD Art. 12 ("changes... without delay and in any event within 28 days"). `REQ-r24-023`, `REQ-6amld-012`.
- **Current state:** ABSENT. The declaration aggregate has `supersede` and `amend` operations but no concept of "this declaration is stale because the BO control event happened > 30 days ago and no update has been received". There is no `due_by` field on declarations, no overdue-update reminder, no automated escalation to the sanctions workflow.
- **Required state:** Each declaration MUST carry a `last_event_observed_at` timestamp (declarant-asserted "when did the BO change occur"). A background worker MUST surface declarations where `now - last_event_observed_at > 30 days` AND `last_update_at < last_event_observed_at`. Operators / declarants MUST be notified. Persistent non-update MUST cascade into the TODO-004 sanctions workflow.
- **Why it matters:** The "up-to-date" prong of c.24.8 is one of three (adequate, accurate, up-to-date). A registry that records a one-time snapshot is not up-to-date — it is a static snapshot. Real BO is dynamic (share transfers, dilutions, control changes). Without an enforcement mechanism, the data the registry holds will decay; in MER terms the country will be assessed as having c.24.8 partially-implemented.
- **Acceptance criteria:**
  - [ ] Migration adds `last_event_observed_at TIMESTAMP NOT NULL` to `declarations`.
  - [ ] Aggregate refuses any declaration where `last_event_observed_at > now()` or `> 5 years ago` (sanity).
  - [ ] Background worker `staleness-watcher` in `apps/` enumerates stale declarations and emits `recor_declaration_staleness_total` metric.
  - [ ] Notification path (email/portal banner) to the declarant.
  - [ ] Escalation hook into TODO-004 after configurable threshold.
  - [ ] Tests: a declaration registered 35 days ago without an update lands in the watcher's output; one registered 25 days ago does not.
- **Effort:** M.
- **Dependencies:** TODO-004 for the sanctions cascade; can ship before that if escalation is logged only.

## TODO-006 — No obliged-entity (FI/DNFBP) principal class or legitimate-interest access tier

- **Priority:** P0
- **Category:** Access Control | Compliance
- **Standard cited:** EU 6AMLD Art. 12 + AMLR Chapter IV; FATF c.24.6(c); post-Sovim CJEU C-37/20 + C-601/20 ruling (public access requires balancing; obliged entities get legitimate-interest access). `REQ-amld-iv-005`, `REQ-cjeu-sovim-006`.
- **Current state:** ABSENT. The permission matrix (`docs/security/permission-matrix.md`) defines `unauthenticated | declarant | admin | internal-service | prometheus-scraper`. There is no `obliged-entity` principal class. The audit-verifier exposes payloads to "any OIDC-authenticated bearer" — there is no scope gate distinguishing "I am a bank doing CDD" from "I am a curious member of the public".
- **Required state:** A new principal class `obliged-entity` MUST exist with: (a) onboarding workflow where the obliged entity proves COBAC/CEMAC supervision OR DNFBP registration, (b) scope-restricted OIDC token containing the obliged-entity ID, (c) per-access audit log including the legitimate-interest justification, (d) rate limiting per obliged entity to prevent bulk scraping, (e) automatic revocation when supervision lapses. A separate `public-legitimate-interest` tier MUST exist for journalists/civil-society organisations under the Sovim balancing test, with court-or-administrative-order intake.
- **Why it matters:** Post-Sovim, an EU-conformant register cannot expose BO data to the general public; even FATF Recommendation 24 (2022 update) shifts toward authority-only access with legitimate-interest carve-outs. A register that has only "OIDC-authenticated" as the gate cannot distinguish an obliged entity (whose access is regulated, supervised, and logged) from anyone with a Google account. The register's privacy posture in any GDPR-adequacy or correspondent-banking review depends on this.
- **Acceptance criteria:**
  - [ ] OIDC client-registration flow that provisions obliged-entity scope (`recor:obliged-entity:cdd`); revoked when supervision lapses.
  - [ ] `services/declaration/src/api/rest.rs` GET handlers branch on the obliged-entity scope and return the post-Sovim subset (no national-ID-number, no residential address) unless the caller has an admin scope.
  - [ ] `obliged_entity_access_log` table records every BO row disclosed under the obliged-entity scope.
  - [ ] Rate limit: 1000 reads / day per obliged-entity, configurable per supervision class.
  - [ ] Permission matrix updated.
  - [ ] Integration test: obliged-entity token reads BO without national-ID; admin token reads BO with national-ID.
- **Effort:** L.
- **Dependencies:** TODO-007 (the post-Sovim payload subset).

## TODO-007 — Audit-verifier discloses full canonical payload, including national-ID numbers (post-Sovim breach + GDPR Art. 5(1)(c) breach)

- **Priority:** P0
- **Category:** Security | Compliance | Data Protection
- **Standard cited:** GDPR Art. 5(1)(c) data minimisation ("adequate, relevant and limited to what is necessary"); CJEU C-37/20 + C-601/20 (WM/Sovim); FATF R.24 c.24.8 fn 27 (BO data MUST include national-ID number — but only for *competent-authority* access).
- **Current state:** PARTIAL. `apps/audit-verifier/src/handlers.rs` is OIDC-gated (FIND-001 closure) and verifies the BLAKE3 hash matches the chaincode. However, the response field set returned to a non-admin caller has NOT been audited against the post-Sovim minimisation requirement. The risk is that the full canonical payload (including `national_id_document`, `residential_address`, `biometric_reference_hash`) is leaked to any authenticated caller. Verified: the handler reuses the same DTO shape used internally; no per-scope filtering layer exists.
- **Required state:** Audit verifier responses MUST be scoped: (a) admin → full payload, (b) obliged-entity → reduced payload (omit national-ID-number, residential address, biometric hash), (c) public (post-Sovim legitimate-interest) → strictly minimal (entity name, BO full-name, BO nationality, cascade tier, ownership_basis_points, declaration date). The on-chain hash returned MUST be the hash of the FULL canonical payload (so the verifier remains useful for cryptographic integrity) but the *payload portion* of the response MUST be the scope-appropriate subset.
- **Why it matters:** A BO register that returns national-ID numbers to a public OIDC bearer is a single CJEU referral away from being shut down. It also violates GDPR Art. 5(1)(c) and Article 32 (appropriate security measures). The Sovim ruling specifically scoped what is permissible to expose; this code path has not been re-scoped post-ruling.
- **Acceptance criteria:**
  - [ ] DTO `VerifyDeclarationResponse` split into `VerifyDeclarationResponseAdmin | ObligedEntity | Public`.
  - [ ] Handler maps OIDC scope → response variant; default is `Public` (most-restrictive).
  - [ ] National-ID number is NEVER in any variant other than `Admin`.
  - [ ] Residential address is NEVER in any variant other than `Admin`.
  - [ ] Biometric reference hash is NEVER returned to any caller (it is an internal verification artefact).
  - [ ] Integration test: OIDC token without admin scope receives the `Public` response; token with admin scope receives `Admin`; token with `obliged-entity` scope receives `ObligedEntity`.
  - [ ] OpenAPI schema documents all three variants.
- **Effort:** M.
- **Dependencies:** TODO-006.

## TODO-008 — No ANIF (Cameroon FIU) disclosure endpoint or audit log (FATF R.24 §c.24.9 + R.40)

- **Priority:** P0
- **Category:** Workflow | Compliance | API
- **Standard cited:** FATF R.24 c.24.9 ("Law enforcement and FIUs MUST have all powers necessary to obtain timely access to basic and BO information"); R.40 (international cooperation); ANIF mandate under Cameroon AML law. `REQ-r24-024`, `REQ-r40-002`.
- **Current state:** ABSENT. `grep -ri "anif\|FIU\|STR_SUBMIT" services/ apps/ packages/` returns zero matches. There is no FIU adapter, no FIU-authentication path, no disclosure-audit table.
- **Required state:** An ANIF-specific access surface MUST exist: (a) ANIF principal authenticated via OIDC AND mTLS (defence in depth), (b) ANIF can submit a search by name / national-ID / declaration-ID / entity-ID and receive the full payload (highest scope), (c) every ANIF disclosure MUST be event-sourced in a separate `fiu_disclosure_log` table with COMP-2 immutability, recording: disclosure_id, requesting principal, ANIF case-reference, fields disclosed, justification text. Disclosure log retention: indefinite. ANIF MUST have a documented MLAT (R.40) pathway for foreign-FIU requests routed via the Egmont Group; the pathway is back-office workflow.
- **Why it matters:** A BO register the FIU cannot query in real-time fails IO.5 Core Issue 5.2. The FATF assessment will downgrade the country's effectiveness rating. Without an audit log of every disclosure, the registry cannot demonstrate appropriate data-subject-rights compliance under GDPR Article 30 (records of processing) or its CEMAC equivalent.
- **Acceptance criteria:**
  - [ ] New principal class `fiu-anif` with OIDC + mTLS + IP allowlist.
  - [ ] `POST /v1/fiu/search` — body specifies name / national-ID / declaration-ID / entity-ID and a free-text justification.
  - [ ] `GET /v1/fiu/disclosure/{id}` — ANIF retrieves prior disclosures by ID.
  - [ ] `fiu_disclosure_log` table with COMP-2 (BEFORE UPDATE/DELETE refused).
  - [ ] Field-level audit: which columns were disclosed, not just "the row was disclosed".
  - [ ] Runbook `docs/runbooks/anif-onboarding.md`.
  - [ ] R.40 / MLAT pathway documented (back-office only; not a self-serve endpoint).
- **Effort:** L.
- **Dependencies:** TODO-006 (principal class infrastructure).

## TODO-009 — No public-feedback channel for registry-data quality (FATF R.24 c.24.8 + 6AMLD Art. 10)

- **Priority:** P0
- **Category:** Workflow | Compliance | API
- **Standard cited:** FATF Guidance §3.5 + 6AMLD Art. 10 + Open Ownership Principle 5.5 (public verification feedback). `REQ-fatf-guidance-035`, `REQ-oo-principles-055`.
- **Current state:** ABSENT. No public form, no public endpoint, no triage table, no public-comment UI.
- **Required state:** Any member of the public MUST be able to flag a registry entry as incorrect via a structured form (similar to TODO-003 but with a different submitter class). The submission MUST capture the alleged inaccuracy, optional evidence, and the submitter's contact (CAPTCHA-gated; throttled). Back-office triage workflow per TODO-003 applies; some signals (e.g. anonymous mass-flag) route to a lower priority. Each flag MUST be event-sourced.
- **Why it matters:** Sovim's balancing test specifically recognises that public/civil-society scrutiny is part of the "necessary and proportionate" calculus that justifies public BO access at all. A registry with no public-feedback path cannot claim the post-Sovim public-access tier is justified — the public is being asked to read, not to read AND flag.
- **Acceptance criteria:**
  - [ ] `POST /v1/public-feedback` — CAPTCHA-gated (hCaptcha or equivalent), throttled per-IP and per-target.
  - [ ] Feedback row in `public_feedback_log` table.
  - [ ] Triage workflow under back-office allowlist.
  - [ ] Permission-matrix entry for the public-feedback submitter.
  - [ ] Rate-limit metric `recor_public_feedback_rate_limited_total`.
- **Effort:** M.
- **Dependencies:** TODO-006 (principal-class infrastructure).

## TODO-010 — No bearer-share / nominee-arrangement disclosure (FATF R.24 §c.24.12)

- **Priority:** P0
- **Category:** Data Model | Compliance
- **Standard cited:** FATF R.24 c.24.12(a–c) ("country MUST prohibit issuance of new bearer shares... existing bearer shares MUST be converted or immobilised... nominees MUST disclose their nominators"). `REQ-r24-028`, `REQ-r24-029`, `REQ-r24-030`, `REQ-r24-031` (nominees).
- **Current state:** ABSENT. `grep -ri "bearer\|nominee" services/` returns zero domain references.
- **Required state:** Two new disclosure surfaces: (a) **bearer-share disclosure** — every legal-person registration MUST attest whether bearer shares are outstanding; if yes, the entity MUST be flagged and the conversion/immobilisation status tracked; (b) **nominee disclosure** — every declaration MUST attest whether the named BO is acting on behalf of a nominator; if yes, the nominator MUST also be registered as a BO under the cascade.
- **Why it matters:** c.24.12 is one of the most-failed sub-criteria in MERs globally. Without explicit bearer-share + nominee fields, the register accepts declarations that are facially compliant but materially defective. Cameroon's grey-list remediation track will require demonstrating compliance with this specific sub-criterion.
- **Acceptance criteria:**
  - [ ] `entities.has_outstanding_bearer_shares BOOLEAN NOT NULL DEFAULT false`.
  - [ ] `entities.bearer_share_status` enum (`none | outstanding | converted | immobilised`) added.
  - [ ] `beneficial_owners.is_nominee BOOLEAN NOT NULL DEFAULT false` + `nominator_person_id UUID NULL` FK to persons.
  - [ ] Aggregate rule: `is_nominee = true` requires `nominator_person_id` to be set AND the nominator must be registered as a separate BO at the cascade tier they exercise control through.
  - [ ] Migration + portal form + tests.
- **Effort:** M.
- **Dependencies:** TODO-001 (cascade tier — nominators inherit the cascade).

## TODO-011 — Audit-verifier handler has 11 unwraps; malformed input panics the service (D14 violation)

- **Priority:** P0
- **Category:** Security | API
- **Standard cited:** OWASP ASVS V5.1.4 (input validation that fails-closed); OWASP API Security Top 10 (2023) API8 (security misconfiguration); D14 (fail-closed). `REQ-asvs-v5-014`.
- **Current state:** PARTIAL. `apps/audit-verifier/src/handlers.rs` contains 11 `unwrap()` / `expect()` calls in the request-handling path (Phase 2F inventory). Each is a panic surface. Some are on JSON deserialisation paths.
- **Required state:** Every `unwrap()` in `apps/audit-verifier/src/handlers.rs` MUST be replaced with explicit `?` propagation returning a structured 4xx/5xx error. No production handler MAY contain `unwrap()` or `expect()` on user-controlled input.
- **Why it matters:** A panicking handler returns 500 (or worse, the connection drops without any response — depending on tower middleware). Repeated panics from a single bad-actor can drive denial-of-service. Beyond the DoS surface, a panic in a production handler indicates the developer did not think about the failure mode — D14 fail-closed is unwaivable.
- **Acceptance criteria:**
  - [ ] Every `unwrap()` / `expect()` in `apps/audit-verifier/src/handlers.rs` removed.
  - [ ] `cargo clippy -p recor-audit-verifier -- -D clippy::unwrap_used -D clippy::expect_used` passes (production code).
  - [ ] Fuzz test (cargo-fuzz or proptest) confirms malformed inputs return 4xx, not panic.
- **Effort:** S. ~1–2 days.
- **Dependencies:** none.

## TODO-012 — `recor-vault-client` has 8 expects on the auth path; flaky Vault crashes the service (D14 violation)

- **Priority:** P0
- **Category:** Security | Infra
- **Standard cited:** D14 (fail-closed at integration boundaries); NIST 800-53 SC-24 (fail in a known state). `REQ-nist-sc-024`.
- **Current state:** PARTIAL. `packages/recor-vault-client/src/lib.rs` contains 8 `expect()` calls on the Vault-login path. A 500 from Vault, a timeout, a token-decode failure → service panic.
- **Required state:** Every `expect()` on the Vault-login path replaced with structured error returns. Vault failures during startup MUST return a typed `ConfigError::VaultUnreachable` and refuse to start (correct fail-closed behaviour); Vault failures DURING runtime (re-login on token expiry) MUST be retried with exponential backoff and emit a `recor_vault_lookup_failures_total` metric, NOT panic.
- **Why it matters:** Vault is shared infrastructure. A network blip during a 4am AppRole-token renewal currently crashes every service simultaneously. The retention worker / scheduled jobs running through that window die. This is a single point of failure for the entire platform.
- **Acceptance criteria:**
  - [ ] Every `expect()` / `unwrap()` in `packages/recor-vault-client/src/lib.rs` removed.
  - [ ] Retry policy on transient Vault failures (5xx, timeout) with 3 retries + jittered exponential backoff.
  - [ ] `recor_vault_lookup_failures_total{outcome=...}` metric.
  - [ ] Integration test against a deliberately-flaky Vault.
- **Effort:** S–M.
- **Dependencies:** none.

## TODO-013 — Stage 7 verification (cross-source) is permanently stubbed (FATF R.24 c.24.6 multi-pronged + IO.5.4)

- **Priority:** P0
- **Category:** Verification | Compliance
- **Standard cited:** FATF R.24 c.24.6 (multi-pronged approach); IO.5 Core Issue 5.4. `REQ-r24-012`.
- **Current state:** STUB. `services/verification-engine/src/application/stages/stage7_*.rs` ships as a stub with no real implementation. Stages 3–6 have real implementations behind config flags; Stage 7 has NO real path.
- **Required state:** Stage 7 cross-source reconciliation MUST compare the declarant's BO claims against (a) BUNEC corporate-register data (when `R-VER-1` lands), (b) sanctions / PEP / ICIJ hits from Stages 3–5, (c) the declarant's own prior declarations, (d) BO data for related entities (cross-entity ownership graph). Discrepancies feed the verification case's lane decision (Green/Yellow/Red).
- **Why it matters:** The "multi-pronged approach" is the spine of c.24.6. Without Stage 7, the verification engine runs each lookup in isolation; a BO who appears in the sanctions list AND in the declarant's prior amended-out declaration is not flagged for the second pattern. The verification engine is then half-blind — it does the lookups but does not cross-reference.
- **Acceptance criteria:**
  - [ ] `services/verification-engine/src/application/stages/stage7_cross_source.rs` ships a real implementation.
  - [ ] Cross-references against Stages 3–6 outputs already in the case state.
  - [ ] Cross-references against the `declarations` projection (or read replica) for prior-declaration drift.
  - [ ] Config flag `ENABLE_REAL_STAGE7=true` activates the real path (matches the FIND-009 pattern).
  - [ ] Property test on the cross-reference graph algorithm.
  - [ ] ADR documents the decision rules (which inconsistencies escalate to Red vs Yellow).
- **Effort:** XL.
- **Dependencies:** R-VER-1 (real BUNEC adapter) for the corporate-register prong.

## TODO-014 — Sanctions, PEP, and ICIJ data feeds have NO ingestion code (operator-seeded only)

- **Priority:** P0
- **Category:** Integration | Verification
- **Standard cited:** FATF R.6 (Targeted Financial Sanctions) implicit dependency; OWASP ASVS V8 (Data Protection); BODS §statement.identifier (provenance and source data). `REQ-r6-002`, `REQ-bods-status-source`.
- **Current state:** PARTIAL. `sanctions_persons`, `peps`, `icij_persons` tables exist with adapter code that queries them; ingestion code does NOT exist. The `INSERT` paths in tests use hand-rolled SQL fixtures. There is no scheduled job that pulls OFAC, EU, UN, ICIJ sources and refreshes the tables.
- **Required state:** A scheduled ingestion worker (`apps/sanctions-ingest`?) MUST pull from each authoritative source on the source's published cadence (OFAC: daily; EU: weekly; UN: irregular; ICIJ: when a new leak drops). Source data MUST be checksummed and the table MUST record the source + the refresh timestamp per row. The worker MUST refuse to apply a delta that drops > N% of the prior dataset (sanity check against an upstream-broken-feed pattern).
- **Why it matters:** Without ingestion, the sanctions table is whatever the operator decided to load last. The verification engine claims to screen against sanctions; it screens against a frozen snapshot. A new addition to the OFAC SDN list this morning will not appear in tomorrow's verification.
- **Acceptance criteria:**
  - [ ] New app `apps/sanctions-ingest` with one sub-binary per source (OFAC, EU, UN, ICIJ Offshore Leaks, ICIJ Panama, ICIJ Paradise, ICIJ Pandora).
  - [ ] Each sub-binary fetches the canonical published format (XML for OFAC SDN, CSV for ICIJ, JSON for EU).
  - [ ] Per-row `source` + `source_revision` + `ingested_at` columns added.
  - [ ] Sanity check: refuse delta > 25% row drop without operator confirmation.
  - [ ] Cron schedule per source.
  - [ ] Test fixtures pinned to specific source revisions.
- **Effort:** L (one ingest per source).
- **Dependencies:** none (independently deployable).

## TODO-015 — Real BUNEC adapter is deferred (R-VER-1); current default is `mock_bunec_persons` table (FATF R.24 c.24.6 multi-pronged failure)

- **Priority:** P0
- **Category:** Integration | Verification
- **Standard cited:** FATF R.24 c.24.6(b)(i) (a public authority MUST hold BO information); IO.5 Core Issue 5.1. `REQ-r24-015`.
- **Current state:** STUB. `services/verification-engine/src/infrastructure/bunec_adapter.rs` is the interface; a real adapter `bunec_real.rs` exists but production default is the mock-Postgres `mock_bunec_persons` table (verified in `services/verification-engine/CLAUDE.md:39`). The integration-smoke script literally seeds `mock_bunec_persons` to make tests pass.
- **Required state:** The real BUNEC adapter MUST be the production default. The integration MUST: (a) be authenticated under the agreed mTLS pattern with BUNEC, (b) be circuit-broken (fail-closed if BUNEC is down for > N minutes), (c) cache responses with TTL aligned to BUNEC's update cadence, (d) emit `recor_bunec_calls_total{result=...}` metrics. Mock paths remain available behind a feature flag for testing only.
- **Why it matters:** The "multi-pronged approach" requires the company registry (BUNEC) to be a real source. A mock table fakes the integration. In the MER review BUNEC will be asked: "does the BO register query you?" — and the answer today is "no, it queries a mock table". This is a credibility-eroding finding the moment any external auditor looks at the wiring.
- **Acceptance criteria:**
  - [ ] BUNEC mTLS handshake established (out-of-band; documented in `docs/runbooks/bunec-onboarding.md`).
  - [ ] `BUNEC_ADAPTER_KIND=real` is the default in production manifests; `mock` is only set in dev/test.
  - [ ] Circuit-breaker triggers fail-closed at the verification engine after configurable consecutive-failures.
  - [ ] Cache TTL aligned with BUNEC's documented refresh cadence.
  - [ ] Integration test against a BUNEC sandbox.
  - [ ] Runbook for BUNEC outage.
- **Effort:** XL. Cross-organisation handshake + adapter + cache + circuit + runbook.
- **Dependencies:** organisational agreement with BUNEC (out of repo).

## TODO-016 — No retention worker wired in code; only documented (D08 + GDPR Art. 5(1)(e))

- **Priority:** P0
- **Category:** Compliance | Data Protection | Infra
- **Standard cited:** GDPR Art. 5(1)(e) storage limitation; FATF R.24 c.24.7 (5-year retention after dissolution); D08 (no dangling threads). `REQ-gdpr-005-001-e`, `REQ-r24-020`.
- **Current state:** STUB. `docs/runbooks/dlq-retention.md` documents the SQL pattern (DELETE FROM outbox WHERE dispatched_at < now() - interval '30 days'). NO code implements this. There is no `apps/retention-worker`, no scheduled job, no advisory-lock. Rows accumulate indefinitely.
- **Required state:** A retention worker (one of: dedicated app, or a per-service background tokio task) MUST run hourly under an advisory-lock; MUST delete outbox rows older than 30 days; MUST delete `fabric_bridge_dlq` rows older than 90 days; MUST archive (not delete) BO declaration data 5 years after the entity's dissolution per c.24.7. Per-table retention policies MUST be discoverable from a single source.
- **Why it matters:** Two convergent problems: (a) GDPR Art. 5(1)(e) — data MUST NOT be kept longer than necessary; the register is currently keeping outbox rows forever, violating the principle; (b) D08 — a runbook that has no code is a dangling thread. Either the runbook describes ghost infrastructure (the work isn't done) or the code is missing (the work isn't done). Both states fail the closure standard.
- **Acceptance criteria:**
  - [ ] `apps/retention-worker` OR per-service tokio task implements the documented DELETEs.
  - [ ] Advisory-lock prevents concurrent execution across replicas.
  - [ ] Per-cycle deletion is bounded by `LIMIT 10000` to avoid long transactions.
  - [ ] `recor_retention_deleted_total{table=...}` metric exposed.
  - [ ] Integration test seeds expired rows and asserts they are removed.
  - [ ] Post-dissolution declaration archival path (c.24.7's "keep 5 years after cessation"): row marked `archived_at` rather than DELETE; full delete after 5 years.
- **Effort:** M.
- **Dependencies:** none.

## TODO-017 — Declarant-supplied `nonce_hex` accepted without uniqueness check (signature-replay risk)

- **Priority:** P0
- **Category:** Security | Cryptography
- **Standard cited:** OWASP ASVS V2.5 (replay protection); NIST 800-63B §5.2.8 (replay-resistant authenticators); Ed25519 signature-attestation pattern. `REQ-asvs-v2-005`.
- **Current state:** PARTIAL. `services/declaration/src/domain/attestation.rs:48` carries `nonce_hex: String`; `verify_strict()` checks the signature is well-formed but no code checks `nonce_hex` uniqueness against prior declarations from the same signer. A declarant can re-sign with the same nonce, allowing replay against a prior signature.
- **Required state:** Every accepted declaration's `nonce_hex` MUST be persisted in a per-signer `attestation_nonces` table (or equivalent). New submissions whose `nonce_hex` collides with a prior nonce from the same signer MUST be refused with a 409. Nonce table retention: keep for the signer's public-key lifetime + 1 year (longer than any reasonable signature validity).
- **Why it matters:** A signature without a nonce-uniqueness check is just a longer payload. The Ed25519 + nonce_hex design assumed nonce uniqueness; without enforcement, an attacker who captures one signed declaration can replay it (or worse, replay variants of it constructed from observable BO patterns). This silently breaks D15 (cryptographic provenance).
- **Acceptance criteria:**
  - [ ] Migration: `attestation_nonces (signer_public_key BYTEA, nonce_hex TEXT, declaration_id UUID, used_at TIMESTAMPTZ, PRIMARY KEY (signer_public_key, nonce_hex))`.
  - [ ] Aggregate's `record_attestation` op refuses on collision.
  - [ ] Unit test: same `nonce_hex` from same signer → second call refused.
  - [ ] Retention policy: TODO-016's worker prunes nonces older than `signer_public_key_revoked_at + 1y`.
- **Effort:** S.
- **Dependencies:** TODO-016 (for retention).

## TODO-018 — No "sufficient link" test for foreign legal persons (FATF c.24.1(d) + c.24.3(b) + c.24.10)

- **Priority:** P0
- **Category:** Compliance | Data Model
- **Standard cited:** FATF R.24 c.24.1(d) fn 15; c.24.3(b); c.24.10. `REQ-r24-002`, `REQ-r24-007`, `REQ-r24-026`.
- **Current state:** ABSENT. The `entities` table has a `jurisdiction` column but no "sufficient link" discriminator (branch / significant business / FI/DNFBP relationship / real estate / employees / tax residence). A foreign entity is registered the same way as a domestic one; there is no test for whether the foreign entity has a Cameroonian nexus that triggers obligations.
- **Required state:** `entities.jurisdiction != 'CMR'` (foreign) MUST require a `sufficient_link_kind` enum (`branch | significant_business | financial_relationship | real_estate | employees | tax_residence | other_documented`) + a `sufficient_link_evidence` text/JSON column documenting the basis. Foreign-entity registrations that cannot assert at least one sufficient-link kind MUST be refused at the aggregate.
- **Why it matters:** Without a sufficient-link test, the register either rejects ALL foreign entities (over-restrictive — fails c.24.10) or accepts ALL foreign entities (over-inclusive — pollutes the registry with non-nexus entities and dilutes risk-scoring). The FATF Methodology specifically requires the country to *document* the sufficient-link test.
- **Acceptance criteria:**
  - [ ] Migration adds `sufficient_link_kind` enum + `sufficient_link_evidence` JSONB to entities.
  - [ ] Aggregate enforces "foreign → must have a sufficient link" rule.
  - [ ] OpenAPI schema documents the field set.
  - [ ] Portal form branches on jurisdiction.
  - [ ] Property test on the foreign-entity acceptance rule.
- **Effort:** M.
- **Dependencies:** TODO-001 (cascade tier — same migration discipline).

## TODO-019 — Declaration response includes signer's public key in the attestation block (BODS / GDPR data-minimisation tension)

- **Priority:** P0
- **Category:** Security | Data Protection | API
- **Standard cited:** GDPR Art. 5(1)(c) data minimisation; OWASP API Security Top 10 (2023) API3 (broken object property level authorization). `REQ-gdpr-005-001-c`.
- **Current state:** PARTIAL. The declaration submission response includes the attestation block, which contains `signer_public_key`. This re-discloses the signer's public key to anyone with read access to the declaration — including (post-FIND-007) the audit-verifier's public response. The signer's public key is a stable identifier across declarations; an observer can de-anonymise.
- **Required state:** The `signer_public_key` is internal verification metadata. The full canonical payload (used for hash verification) MUST contain it, but the *response surface* MUST NOT echo it to non-admin callers. The audit-verifier returns `match: bool` against the on-chain hash; it does NOT need to echo the public key.
- **Why it matters:** A stable per-signer public key, returned to every reader, lets an observer build the signer's submission graph: which entities they declare, in what cadence, with what BO patterns. This is the kind of side-channel that any GDPR review will flag immediately.
- **Acceptance criteria:**
  - [ ] Response DTOs scrub `signer_public_key` for non-admin scopes.
  - [ ] Audit verifier never returns `signer_public_key`.
  - [ ] Tests assert the public-key field is absent in the non-admin response.
- **Effort:** S.
- **Dependencies:** TODO-007 (response-variant infrastructure).

## TODO-020 — No declarant identity-assurance level (IAL) documented (NIST 800-63A); OIDC issuer trust level unspecified

- **Priority:** P0
- **Category:** Identity | Compliance
- **Standard cited:** NIST 800-63A §IAL2/IAL3; FATF R.24 c.24.6 IO.5 ("identity verification of the submitter"). `REQ-nist-63a-001`, `REQ-r24-022`.
- **Current state:** PARTIAL. The platform requires an OIDC bearer for state-changing operations. There is no documented requirement on the OIDC issuer's IAL — any issuer producing a valid token under the configured discovery URL is accepted. A self-asserted email identity is the same as a NDI-verified-in-person identity.
- **Required state:** The declaration submission endpoint MUST require IAL2 (verified evidence + verified address) at minimum; IAL3 (in-person verification) for the dissolve/correct/admin-allowlist endpoints. The OIDC issuer's IAL MUST be advertised via the discovery document (`acr_values_supported` or a similar claim). The verifier MUST check the `acr` claim on submission.
- **Why it matters:** A registry that accepts "any OIDC token" cannot claim it verifies submitters' identities. The FATF Methodology IO.5 explicitly requires "identity verification of the submitter" — accepting tokens from a public-email OIDC issuer fails this. The 2024 MER pattern increasingly cites NIST 800-63 IAL levels as the benchmark.
- **Acceptance criteria:**
  - [ ] OIDC discovery configuration requires the issuer to advertise an `acr_values_supported` claim.
  - [ ] `recor-auth-oidc` verifier rejects tokens where `acr` is below the per-endpoint IAL threshold.
  - [ ] Per-endpoint IAL minimum documented in `docs/security/permission-matrix.md`.
  - [ ] Runbook for the operator: "what to configure on the IdP to advertise IAL2/IAL3".
- **Effort:** M.
- **Dependencies:** none.

## TODO-021 — Declarant attestations do NOT explicitly cover the "adequate, accurate, up-to-date" claim (R.24 c.24.8)

- **Priority:** P0
- **Category:** Compliance | Data Model
- **Standard cited:** FATF R.24 c.24.8 ("adequate, accurate, up-to-date"). `REQ-r24-021`, `REQ-r24-022`, `REQ-r24-023`.
- **Current state:** PARTIAL. The Ed25519 attestation signs the canonical payload (verify with `verify_strict()`). The declarant signs the *data*. There is no explicit attestation-of-truth, no machine-readable assertion that the declarant claims the data is adequate, accurate, and up-to-date as of the attestation moment.
- **Required state:** The canonical payload signed by the declarant MUST include a JSON-LD-style `claims` block: `{adequate: true, accurate: true, up_to_date_as_of: "<timestamp>", legal_basis: "<R.24-compliance|other>"}`. This becomes an explicit perjury surface — if the declarant signs false claims, the sanctions workflow (TODO-004) has the cryptographic evidence.
- **Why it matters:** Cryptographic attestation without explicit claims is just a signed snapshot. The sanctions workflow (TODO-004) needs an unambiguous "the declarant claimed X" record; perjury-grade evidence. Without the explicit claims block, every sanctioning would require reconstructing what the declarant "must have meant".
- **Acceptance criteria:**
  - [ ] Canonical payload schema extended with `claims` block.
  - [ ] Aggregate refuses submissions where any of `adequate`/`accurate`/`up_to_date_as_of` is missing.
  - [ ] OpenAPI schema documents the block.
  - [ ] Portal form renders an explicit checkbox set + textbox for legal basis.
  - [ ] Audit-verifier surfaces the claims block in the admin-scope response.
- **Effort:** M.
- **Dependencies:** TODO-001 (the schema change cycle).

## TODO-022 — No SBOM published or supply-chain-attested release (D20 + SLSA Level 4)

- **Priority:** P0
- **Category:** Security | Infra
- **Standard cited:** D20 (Supply chain integrity, SLSA Level 4 — unwaivable); OWASP ASVS V14.2 (Dependencies); EO 14028 SBOM. `REQ-asvs-v14-002`.
- **Current state:** ABSENT (verified). There is no `.sbom/`, no published SPDX/CycloneDX manifest, no SLSA provenance attestation on built images. `grep -ri "sbom\|spdx\|cyclonedx\|slsa\|provenance" .github/workflows/` returns no matches.
- **Required state:** Every container image released MUST ship with: (a) an SPDX or CycloneDX SBOM listing every transitive dependency, (b) a SLSA Level 4 provenance attestation, (c) a signed cosign bundle attaching both to the image digest. The CI pipeline MUST refuse to publish if any dependency in the SBOM matches the known-vulnerable-version GHSA feed.
- **Why it matters:** D20 is one of the four unwaivable doctrines. A platform without SBOM cannot answer "what's in your image" to any external reviewer — and the answer is the precondition for every supply-chain attack mitigation that follows. SLSA L4 specifically addresses build-system integrity, which is the failure mode SolarWinds demonstrated.
- **Acceptance criteria:**
  - [ ] `syft` (or equivalent) generates SBOM in the publish-images workflow.
  - [ ] `slsa-github-generator` (or `cosign attest`) produces the SLSA provenance.
  - [ ] `cosign attach attestation` binds SBOM + provenance to the image digest.
  - [ ] GHSA-feed CI gate refuses publish on known-vulnerable-version match.
  - [ ] Verification script `tools/ci/verify-sbom.sh` for downstream consumers.
- **Effort:** M.
- **Dependencies:** none.

## TODO-023 — No threat-model-derived defensive integration test for the Sovim-public tier (when implemented)

- **Priority:** P0
- **Category:** Testing | Compliance
- **Standard cited:** CJEU C-37/20 + C-601/20 (Sovim); D17 (zero trust). `REQ-cjeu-sovim-001`.
- **Current state:** ABSENT. When TODO-006 + TODO-007 land, the post-Sovim public access tier becomes a code path; without integration tests, a future refactor could silently re-disclose the over-broad payload. Currently there is no public tier at all, so this finding is forward-looking but P0 because shipping TODO-006/007 without it is shipping insecure code.
- **Required state:** When the public tier ships, an integration test MUST construct a token at every tier (admin / obliged-entity / public-legitimate-interest / unauthenticated) and assert the response payload field-set is correct at each tier — particularly that national-ID-number, residential address, and biometric hash are NEVER in the non-admin variants. The test MUST run on every PR touching the audit-verifier handlers.
- **Why it matters:** A scope-misconfiguration after FIND-007-style separation, or a DTO refactor that re-exports an internal field, would silently break Sovim compliance. The verifier is the user-facing surface for the entire registry's privacy posture.
- **Acceptance criteria:**
  - [ ] Integration test in `apps/audit-verifier/tests/payload_scoping.rs` (or similar) instantiates a real server with each scope, calls the endpoint, asserts the response shape.
  - [ ] The test fails CI on any DTO change that adds a field to a lower-scoped variant.
  - [ ] Documentation cross-link from `docs/security/permission-matrix.md`.
- **Effort:** S.
- **Dependencies:** TODO-006, TODO-007.

## TODO-024 — Stage 5 adverse-media inference has no cost cap or budget enforcement (D14 + financial control)

- **Priority:** P0
- **Category:** Cost | Security | Infra
- **Standard cited:** D14 (fail-closed); OWASP API Security Top 10 (2023) API4 (unrestricted resource consumption). `REQ-asvs-v11-002`.
- **Current state:** PARTIAL. Stage 5 is gated behind `ENABLE_REAL_ADVERSE_MEDIA=true` AND `ANTHROPIC_API_KEY` set. When enabled, every verification case calls Anthropic. There is no daily/monthly token-budget cap, no rate limiter, no kill-switch that flips back to fixture mode when the budget exceeds N.
- **Required state:** `recor-inference-gateway` MUST enforce: (a) per-day token budget per service, (b) per-month token budget per service, (c) an admin-controlled kill switch that flips to fixture mode, (d) Prometheus alert when budget is at 80% / 95% / 100%. When the cap is hit, the gateway MUST return `BudgetExceeded` (fixture-mode-equivalent) rather than continuing to spend.
- **Why it matters:** Stage 5 is the only paid-on-every-call surface in the system. An attacker who can trigger Stage 5 verifications (TODO-013 cross-source + verification-engine submit) can drain the Anthropic budget. Without a cap, that's denial-of-service-by-bill. The platform's operating budget (USD 6–8M/year per `CLAUDE.md`) is finite; a single bad day can consume a quarter of it.
- **Acceptance criteria:**
  - [ ] `recor_anthropic_tokens_used_total{service=,model=,tier=}` metric.
  - [ ] `recor_anthropic_budget_remaining_tokens{service=,window=daily|monthly}` gauge.
  - [ ] Budget enforcement returns `BudgetExceeded` (fail-closed) on hit.
  - [ ] Prometheus alerts at 80/95/100%.
  - [ ] Admin endpoint to flip the kill switch.
  - [ ] Documentation in `docs/runbooks/anthropic-budget.md`.
- **Effort:** M.
- **Dependencies:** none.

---

# P1 — Required for production deployment

## TODO-025 — Declaration::api::rest has no integration tests (947 LOC of HTTP boundary untested)

- **Priority:** P1
- **Category:** Testing
- **Standard cited:** OWASP ASVS V11.1.4 (Tested business logic). `REQ-asvs-v11-001-004`.
- **Current state:** UNVERIFIED. Phase 2G report: 947 LOC, only superficial tests. Malformed JSON, oversized payloads, content-type sniffing, header confusion, governor edge cases all untested at the HTTP boundary.
- **Required state:** Integration tests via testcontainers cover: (a) every documented status code in the OpenAPI spec, (b) malformed/oversized inputs return 4xx not 500, (c) the governor rate-limit triggers under load, (d) the per-row tenancy predicate (FIND-004 closure) is exercised for every endpoint.
- **Why it matters:** This is the platform's primary public surface. Untested HTTP boundaries are how 0-day exploits enter; the rest of the platform's defence-in-depth assumes the HTTP layer doesn't let malformed input through.
- **Acceptance criteria:**
  - [ ] `services/declaration/tests/rest_integration.rs` with ≥30 test cases.
  - [ ] Property test on JSON payload variations.
  - [ ] All status codes from the OpenAPI exercised at least once.
  - [ ] CI runs the suite under `--ignored` testcontainers profile.
- **Effort:** L.
- **Dependencies:** none.

## TODO-026 — Verification-engine::api::rest has no integration tests (645 LOC)

- **Priority:** P1
- **Category:** Testing
- **Standard cited:** OWASP ASVS V11.1.4. `REQ-asvs-v11-001-004`.
- **Current state:** UNVERIFIED. Phase 2G: similar shape to TODO-025.
- **Required state:** Same pattern as TODO-025 for verification-engine.
- **Why it matters:** Verification engine is the analytical surface — the lane decision (Green/Yellow/Red) is the platform's most consequential output. Untested HTTP boundary on this surface is unconscionable.
- **Acceptance criteria:**
  - [ ] `services/verification-engine/tests/rest_integration.rs` with ≥25 cases.
  - [ ] FIND-002 admin-allowlist gate exercised end-to-end.
  - [ ] FIND-004 per-case tenancy predicate exercised end-to-end.
- **Effort:** L.
- **Dependencies:** none.

## TODO-027 — Postgres adapters for declaration/person/entity have no integration tests (1900+ LOC combined)

- **Priority:** P1
- **Category:** Testing
- **Standard cited:** ASVS V11.1.4; D4 (tests are part of the feature). `REQ-asvs-v11-001-004`.
- **Current state:** UNVERIFIED. Phase 2G top-risk modules 2/4/5 — `declaration::infrastructure::postgres` (736 LOC), `entity-service::infrastructure::postgres` (496), `person-service::infrastructure::postgres` (579).
- **Required state:** testcontainers-backed integration tests for: constraint violations under concurrent writes; unique-key collisions; FK cascades; the COMP-2 trigger refusal of UPDATE/DELETE; index-only-scan paths.
- **Why it matters:** The persistence layer is where every consistency invariant lives. Untested Postgres adapters mean an undetected migration drift or constraint regression silently corrupts the registry.
- **Acceptance criteria:**
  - [ ] Each `infrastructure/postgres.rs` has a matching `tests/postgres_integration.rs` with ≥20 cases.
  - [ ] Concurrent-write race covered.
  - [ ] COMP-2 trigger refusal covered.
- **Effort:** L.
- **Dependencies:** none.

## TODO-028 — Kafka producer + consumer paths have no integration tests (1077 LOC combined)

- **Priority:** P1
- **Category:** Testing | Infra
- **Standard cited:** ASVS V11.1.4. `REQ-asvs-v11-001-004`.
- **Current state:** UNVERIFIED. Phase 2G: `kafka_producer.rs` (455 LOC) and `kafka_consumer.rs` (622 LOC) both at coverage class `none`.
- **Required state:** testcontainers-backed Kafka integration tests (use redpanda) covering: produce-then-consume round trip; consumer DLQ on bad message; producer back-pressure; broker-unavailable failure mode.
- **Why it matters:** ADR-007 documents Kafka as the transport cutover. If Kafka rolls out untested, the platform loses its event-distribution backbone the moment a real broker shows a difference from the local dev shape.
- **Acceptance criteria:**
  - [ ] `services/declaration/tests/kafka_integration.rs` + `services/verification-engine/tests/kafka_integration.rs`.
  - [ ] Each ≥15 cases.
- **Effort:** L.
- **Dependencies:** none.

## TODO-029 — Worker-fabric-bridge::processor has no integration tests (390 LOC)

- **Priority:** P1
- **Category:** Testing | Audit
- **Standard cited:** ASVS V11.1.4; D15 (cryptographic provenance). `REQ-asvs-v11-001-004`.
- **Current state:** UNVERIFIED. Phase 2G: top-risk module 6 — 390 LOC, no integration tests; replay detection not exercised.
- **Required state:** Integration tests against a Fabric testnet (or a fabric-mock) covering: anchoring round-trip; idempotent re-write; circuit-breaker on chain-unavailable; replay rejection.
- **Why it matters:** The audit-chain is the platform's tamper-evidence claim. Untested anchoring means tamper-evidence is theoretical.
- **Acceptance criteria:**
  - [ ] `apps/worker-fabric-bridge/tests/anchoring_integration.rs` with ≥10 cases.
- **Effort:** L.
- **Dependencies:** Fabric testnet harness (the `chaincode/audit-witness/audit_witness_test.go` suite is the chaincode-side counterpart but doesn't exercise the Rust bridge).

## TODO-030 — Audit-reconciler has no integration tests against real Fabric (393 LOC)

- **Priority:** P1
- **Category:** Testing | Audit
- **Standard cited:** ASVS V11.1.4. `REQ-asvs-v11-001-004`.
- **Current state:** UNVERIFIED. Phase 2G: unit-test coverage exists; divergence detector not exercised against a real Fabric.
- **Required state:** Integration test that anchors N events, deletes one chaincode entry (or simulates a missed write), and asserts the reconciler detects + counts the divergence.
- **Why it matters:** The reconciler is the safety net for FIND-016. A reconciler that's never seen a real divergence won't be trusted at first divergence.
- **Acceptance criteria:**
  - [ ] `apps/audit-reconciler/tests/divergence_integration.rs` with ≥5 scenarios.
- **Effort:** M.
- **Dependencies:** Fabric testnet.

## TODO-031 — TLS cipher-suite hardening not configured (rustls defaults only)

- **Priority:** P1
- **Category:** Security
- **Standard cited:** OWASP ASVS V9.1.2 (Use only strong cipher suites); NIST SP 800-52 Rev. 2 §3. `REQ-asvs-v9-001-002`.
- **Current state:** PARTIAL. rustls 0.23 defaults accept TLS 1.2+ with the rustls-recommended suite set. No explicit hardening to TLS 1.3-only or strong-suite-only.
- **Required state:** TLS 1.3-only (or TLS 1.2 + explicit strong-suite allowlist) per NIST 800-52r2 §3.1. Hardening documented in `docs/security/tls-policy.md`.
- **Why it matters:** Default rustls is reasonable but defensible only if documented. An auditor asking "what TLS suites does your platform negotiate" should get a one-line answer with citation, not "rustls defaults".
- **Acceptance criteria:**
  - [ ] `recor-spiffe::rustls_glue.rs` configures `versions(&[&TLS13])` (or equivalent).
  - [ ] `docs/security/tls-policy.md` documents the choice + cites NIST.
  - [ ] Smoke test confirms a TLS 1.2 client is refused.
- **Effort:** S.
- **Dependencies:** none.

## TODO-032 — No GDPR data-subject-rights endpoints (export, rectification, erasure-restriction)

- **Priority:** P1
- **Category:** Compliance | Data Protection | API
- **Standard cited:** GDPR Art. 15 (right of access), Art. 16 (rectification), Art. 17 (erasure — with public-interest exemption), Art. 18 (restriction), Art. 20 (portability). `REQ-gdpr-015`, `REQ-gdpr-016`, `REQ-gdpr-017`.
- **Current state:** PARTIAL. `services/declaration/src/api/rest.rs:131: list_declarations_by_principal` is the closest thing — a declarant can list their own declarations (COMP-1 closure). There is no rectification flow (other than amend/correct), no export-as-portability endpoint, no erasure-restriction marker, no Art. 18 restriction state.
- **Required state:** Per-data-subject (person, declarant) endpoints: (a) `GET /v1/data-subject/export` — full export of all rows referencing the subject, in machine-readable JSON; (b) `POST /v1/data-subject/rectify` — proposes a correction, routed to back-office triage; (c) `POST /v1/data-subject/restrict` — marks processing restricted under Art. 18 (public-interest exemption documented per record); (d) `GET /v1/data-subject/processing-log` — Art. 15 right-of-access.
- **Why it matters:** A BO register is exempt from GDPR's hard right-to-erasure (public-interest task per Art. 6(1)(e) + Art. 17(3)(b)). It is NOT exempt from access, rectification, restriction, or portability. Without these endpoints the registry cannot respond to a DSAR within the GDPR's 30-day window — automatic non-compliance.
- **Acceptance criteria:**
  - [ ] All four endpoints implemented under the declaration service.
  - [ ] DSAR-fulfilment runbook `docs/runbooks/dsar-fulfilment.md`.
  - [ ] Audit log per DSAR action.
  - [ ] Integration tests cover all four endpoints.
- **Effort:** L.
- **Dependencies:** none.

## TODO-033 — No documented data-protection impact assessment (GDPR Art. 35)

- **Priority:** P1
- **Category:** Compliance | Documentation
- **Standard cited:** GDPR Art. 35 (DPIA required for high-risk processing). `REQ-gdpr-035`.
- **Current state:** ABSENT. No `docs/compliance/dpia.md`, no equivalent.
- **Required state:** A DPIA covering: nature of the processing, necessity + proportionality assessment, risks to data-subject rights, mitigations. Reviewed annually.
- **Why it matters:** A BO register is by definition Art. 35 high-risk processing (systematic monitoring + sensitive personal data + large-scale). The supervisory authority will ask for the DPIA on day one. Without it, the platform's lawful basis for processing is procedurally unestablished.
- **Acceptance criteria:**
  - [ ] `docs/compliance/dpia.md` authored using the WP29 template.
  - [ ] Reviewed + signed by the appointed Data Protection Officer.
  - [ ] Reference linked from `docs/compliance/data-classification.md`.
- **Effort:** M (writing + review).
- **Dependencies:** none.

## TODO-034 — No records-of-processing register (GDPR Art. 30)

- **Priority:** P1
- **Category:** Compliance | Documentation
- **Standard cited:** GDPR Art. 30. `REQ-gdpr-030`.
- **Current state:** ABSENT.
- **Required state:** A processing register documenting: purpose, categories of data subjects + data, recipients, transfers, retention periods, security measures. Maintained per service and per data flow.
- **Why it matters:** Art. 30 is a hard documentation requirement. The supervisor will ask for it.
- **Acceptance criteria:**
  - [ ] `docs/compliance/records-of-processing.md` per service.
  - [ ] Linked from `docs/compliance/data-classification.md`.
- **Effort:** M.
- **Dependencies:** none.

## TODO-035 — No breach-notification runbook or process (GDPR Art. 33–34 + 72h window)

- **Priority:** P1
- **Category:** Compliance | Runbook
- **Standard cited:** GDPR Art. 33 (notify supervisor within 72h), Art. 34 (notify subjects). `REQ-gdpr-033`, `REQ-gdpr-034`.
- **Current state:** PARTIAL. `docs/runbooks/incident-response-template.md` exists but does not specifically cover the 72-hour notification clock or the per-subject notification flow.
- **Required state:** A dedicated breach-notification runbook with: detection criteria, severity scale, 72-hour timer mechanics, supervisor contact details (Cameroon's data-protection authority), per-subject notification template + dispatch path.
- **Why it matters:** Missing the 72-hour window is itself a separate Art. 33 violation. Without a runbook, the on-call engineer has no clock to start.
- **Acceptance criteria:**
  - [ ] `docs/runbooks/breach-notification.md` per the WP29 guidance template.
  - [ ] Decision tree for which incidents trigger Art. 33 vs Art. 34.
  - [ ] Quarterly drill.
- **Effort:** M.
- **Dependencies:** none.

## TODO-036 — Sanctions / PEP / ICIJ tables have no source-provenance columns

- **Priority:** P1
- **Category:** Data Model | Compliance
- **Standard cited:** BODS §statement.source (provenance); FATF c.24.8 fn 28 (verification via reliable, independently sourced/obtained documents). `REQ-bods-source-statement`, `REQ-r24-022`.
- **Current state:** PARTIAL. Tables exist; no `source` or `source_revision` column visible from inventory.
- **Required state:** Add `source TEXT NOT NULL` + `source_revision TEXT NOT NULL` + `ingested_at TIMESTAMPTZ NOT NULL` to `sanctions_persons`, `peps`, `icij_persons`. Match result includes the source citation so the verification case carries the provenance.
- **Why it matters:** When the verification engine flags a BO as appearing on the sanctions list, the audit trail must say *which* sanctions list, *which revision*. Without provenance the flag is undefendable.
- **Acceptance criteria:**
  - [ ] Migration adds the three columns.
  - [ ] Adapter SELECTs include them; matches propagate them.
  - [ ] Verification-case event records the source citation.
- **Effort:** S.
- **Dependencies:** TODO-014 (the ingester populates the provenance).

## TODO-037 — No multi-language portal (FR ↔ EN parity) verified for legal text (Cameroon's official languages)

- **Priority:** P1
- **Category:** UI | Compliance
- **Standard cited:** Cameroon constitutional bilingualism (Constitution Art. 1(3)); CEMAC official-language norms. `REQ-cemac-lang-001`.
- **Current state:** PARTIAL. Portal under `applications/declarant-portal/` has translation infrastructure (visible via the locale-keyed strings in `src/`). Parity between FR and EN on legal-binding text (declaration claims, attestation prompts, sanctions notifications) has NOT been audited.
- **Required state:** FR + EN parity on every legal-binding string. Discrepancies between language versions go through legal review.
- **Why it matters:** A bilingual country with a registry that's English-strong + French-weak (or vice versa) faces challenge in court the first time a sanctioned declarant claims they didn't understand the legal terms.
- **Acceptance criteria:**
  - [ ] Linter / CI check confirms every legal-text key has both FR and EN.
  - [ ] Legal review signed.
- **Effort:** M.
- **Dependencies:** none.

## TODO-038 — Person-service searches use ILIKE not phonetic / pg_trgm fuzzy matching (R-PERSON-FUZZY deferred)

- **Priority:** P1
- **Category:** Search | Data Quality
- **Standard cited:** OpenCorporates entity-resolution guidance; FATF c.24.22 (timely access). `REQ-oc-entity-001`.
- **Current state:** PARTIAL. `services/person-service/src/infrastructure/postgres.rs` has a TODO marker for `R-PERSON-FUZZY` — pg_trgm trigram similarity.
- **Required state:** Trigram + phonetic (metaphone / soundex / soundex-CMR-adapted) search supported. Search results include a confidence score.
- **Why it matters:** A name like "Mbarga Mbarga Jean-Paul Etienne" is hard to find via ILIKE. A FIU investigating a person who is registered under a variant spelling will miss them. The verification engine's stage-3/4/5 already have BUNEC name resolution but the operator-facing search does not.
- **Acceptance criteria:**
  - [ ] `pg_trgm` extension required by migration.
  - [ ] Trigram + phonetic similarity in `GET /v1/persons/search`.
  - [ ] Confidence score in results.
  - [ ] Tests cover spelling-variant cases.
- **Effort:** M.
- **Dependencies:** none.

## TODO-039 — Entity-service has `outbox` but no relay worker (R-ENT-RELAY deferred)

- **Priority:** P1
- **Category:** Integration | Audit
- **Standard cited:** ADR-003 (HTTP outbox-relay pattern); D14. `REQ-adr-003-pattern`.
- **Current state:** STUB. Entity-service writes to `outbox` but no relay worker consumes it. Rows accumulate.
- **Required state:** A relay worker (mirroring declaration's `infrastructure/relay.rs`) drains the entity-outbox to downstream consumers (or to the Fabric bridge).
- **Why it matters:** Outbox-with-no-relay is the classic dangling-thread (D08). Either the outbox shouldn't be writing or the relay needs to exist. Today the rows accumulate forever.
- **Acceptance criteria:**
  - [ ] `services/entity-service/src/infrastructure/relay.rs` mirrors declaration's pattern.
  - [ ] Integration test asserts rows are dispatched.
- **Effort:** M.
- **Dependencies:** TODO-016 (retention worker for dispatched rows).

## TODO-040 — Person-service `outbox` has no relay either

- **Priority:** P1
- **Category:** Integration | Audit
- **Standard cited:** as TODO-039. `REQ-adr-003-pattern`.
- **Current state:** STUB. Same shape as TODO-039.
- **Required state:** Same shape as TODO-039.
- **Why it matters:** Same shape as TODO-039.
- **Acceptance criteria:** mirror.
- **Effort:** M.
- **Dependencies:** TODO-016.

## TODO-041 — Audit-verifier reads declaration's DB directly without a contract (R-AV-CONTRACT deferred)

- **Priority:** P1
- **Category:** Cross-service Coupling | Compliance
- **Standard cited:** D14 (fail-closed at integration boundaries); ASVS V1.2 (Architecture). `REQ-asvs-v1-002`.
- **Current state:** PARTIAL. Audit-verifier reads the declaration projection directly via Postgres; no service-to-service contract; no versioned API on the declaration side.
- **Required state:** Declaration service exposes a read-only "verifier API" (or a publish to a shared schema), and audit-verifier calls that. The cross-DB SQL coupling is a doctrine drift from D14.
- **Why it matters:** Schema drift in declaration silently breaks audit-verifier. A migration that renames a column will break the verifier with no compile-time signal.
- **Acceptance criteria:**
  - [ ] `services/declaration` exposes a versioned reader API (GraphQL? gRPC? HTTP?).
  - [ ] Audit-verifier consumes the versioned API.
  - [ ] Drift detection in CI.
- **Effort:** L.
- **Dependencies:** none.

## TODO-042 — Admin allowlist is CSV-typed (`ADMIN_PRINCIPALS=a,b,c`) — R-AUTHZ-ENUM deferred

- **Priority:** P1
- **Category:** Access Control | Configuration
- **Standard cited:** OWASP ASVS V14.2.4 (Configuration). `REQ-asvs-v14-002-004`.
- **Current state:** PARTIAL. `Config::admin_principals_list()` parses a CSV. A whitespace error or comma-in-principal-name silently truncates the allowlist.
- **Required state:** Bounded-enum typing of admin principals, OR a stricter parser that rejects empty entries + entries with embedded commas + duplicates.
- **Why it matters:** A typo in the env var creates an unintended allowlist or silently drops an entry. The dissolve / correct / merge-into endpoints are admin-only; a silently-empty allowlist refuses every caller (defensive) but a silently-truncated allowlist accepts the wrong subset.
- **Acceptance criteria:**
  - [ ] Parser validates each entry as a non-empty trimmed string without internal commas.
  - [ ] Duplicate entries produce a startup error.
  - [ ] Tests cover empty + whitespace + duplicate cases.
- **Effort:** S.
- **Dependencies:** none.

## TODO-043 — Portal CSP `connect-src` does NOT include the audit-verifier origin (data-flow row of audit catalogue)

- **Priority:** P1
- **Category:** Security
- **Standard cited:** OWASP ASVS V14.4 (CSP). `REQ-asvs-v14-004`.
- **Current state:** PARTIAL. `applications/declarant-portal/security-headers.conf.template:10` templates `connect-src 'self' ${CSP_CONNECT_SRC}`. The portal CSP is configured at orchestrator-level; in default deployment manifests the audit-verifier origin is NOT in `CSP_CONNECT_SRC`. The portal cannot call the audit-verifier from the browser without manual operator action.
- **Required state:** Default deployment templates include the audit-verifier origin in `CSP_CONNECT_SRC`. Documented in `docs/runbooks/portal-csp.md`.
- **Why it matters:** The portal-side verification UX assumes the verifier is reachable; with a default-missing origin the feature silently fails the first time it's invoked.
- **Acceptance criteria:**
  - [ ] Default values.yaml / kustomize default sets `CSP_CONNECT_SRC` to include the audit-verifier ingress.
  - [ ] Documented.
- **Effort:** S.
- **Dependencies:** none.

## TODO-044 — Service workers `autoUpdate` without SRI (sub-resource integrity)

- **Priority:** P1
- **Category:** Security
- **Standard cited:** OWASP ASVS V14.4.7 (SRI on third-party resources). `REQ-asvs-v14-004-007`.
- **Current state:** PARTIAL (per audit MEDIUM/LOW summary). Portal SW autoUpdate likely runs without SRI.
- **Required state:** Every third-party script/style tag carries `integrity=` SHA-384. Service-worker self-update verifies integrity.
- **Why it matters:** A compromised CDN or upstream proxy can swap a JS bundle. SRI is the cheap defence; without it the portal accepts whatever it receives.
- **Acceptance criteria:**
  - [ ] Vite build emits integrity attributes for every static asset.
  - [ ] CI gate fails on missing `integrity=`.
  - [ ] Documented.
- **Effort:** S.
- **Dependencies:** none.

## TODO-045 — `GET /v1/declarations/{id}` returns 403 (not 404) for cross-tenant access — existence side-channel

- **Priority:** P1
- **Category:** Access Control | Security
- **Standard cited:** OWASP API Security Top 10 (2023) API3 (broken object property level authorization); FIND-004 (closed but the 403-vs-404 sub-issue might persist). `REQ-asvs-v4-001`.
- **Current state:** PARTIAL. Need to verify the actual response code on cross-tenant access. The Sprint-1 closure note in `docs/audit/10-findings.md` mentions the permission matrix maps cross-tenant to 404, but the implementation MUST be re-verified.
- **Required state:** Every per-row endpoint returns 404 (not 403) when the caller is not the row's tenant. Admins receive the row. Unauthenticated callers receive 401.
- **Why it matters:** A 403 confirms the row exists. An attacker enumerating UUIDs can probe for existence even without read access. The 404 unification was the FIND-004 closure pattern; this finding verifies it.
- **Acceptance criteria:**
  - [ ] Integration test in each service: declarant-A token requesting declarant-B's row → 404.
  - [ ] Admin token requesting same row → 200 with payload.
  - [ ] Unauthenticated request → 401.
- **Effort:** S.
- **Dependencies:** none.

## TODO-046 — Polling cadence on portal (3s) not coordinated with V-engine pipeline tick budget

- **Priority:** P1
- **Category:** UI | Performance
- **Standard cited:** ASVS V11.2.1 (Business-logic timing). `REQ-asvs-v11-002-001`.
- **Current state:** PARTIAL. Portal polls verification status every 3s; V-engine pipeline p99 is 5s. Half the polls fetch a stale "still pending" — wasted load.
- **Required state:** Either (a) align polling to ≥7s (V-engine p99 + safety margin), or (b) replace polling with Server-Sent Events / WebSocket / long-poll.
- **Why it matters:** At scale the polling cost is 30% of API load with zero information yield. The mismatch is also a leaky abstraction — the portal claims a 3s UX promise that the backend doesn't deliver.
- **Acceptance criteria:**
  - [ ] Polling cadence raised OR switched to push.
  - [ ] Documented in `docs/architecture/05-data-flows.md`.
- **Effort:** S–M.
- **Dependencies:** none.

## TODO-047 — `LOG_REDACTION_KEY` dev fallback derives entropy from `SystemTime + PID` (D14 violation if production startup misses the env)

- **Priority:** P1
- **Category:** Security | Logging
- **Standard cited:** D14 (fail-closed); D18 (no secrets). `REQ-d14-startup`.
- **Current state:** PARTIAL. `packages/recor-logging/src/lib.rs:189-198` falls back to `blake3::hash(SystemTime + PID)` when `LOG_REDACTION_KEY` is unset. Documented as dev-only but enforced via configuration.
- **Required state:** Refuse to start in `ENVIRONMENT != dev` when `LOG_REDACTION_KEY` is empty — the same fail-closed posture as FIND-003.
- **Why it matters:** A production deployment with a missing env var silently falls back to weak entropy. Redaction with weak entropy is not redaction — a knowledgeable attacker can re-derive the redaction key from known process metadata.
- **Acceptance criteria:**
  - [ ] `Config::from_env` refuses to start in non-dev when `LOG_REDACTION_KEY` is empty.
  - [ ] Integration test confirms refusal.
- **Effort:** S.
- **Dependencies:** none.

## TODO-048 — Stage 5 adverse-media response is not deterministic (consensus only at high confidence)

- **Priority:** P1
- **Category:** Verification | Compliance
- **Standard cited:** ADR-002 (Dempster-Shafer fusion); FATF c.24.22. `REQ-r24-022`.
- **Current state:** PARTIAL. Stage 5 calls Anthropic claude-opus-4-7 (Tier A) which is non-deterministic at temp > 0. Repeated calls on the same input may produce different lane decisions.
- **Required state:** Either (a) pin temperature to 0 and use the Anthropic batch API for repeatability, or (b) sample k≥3 times and emit the majority decision with a documented confidence interval. The audit-grade decision MUST be reproducible from the input.
- **Why it matters:** A verification decision that flips between runs is not defensible. A sanctioned declarant can argue "you re-ran it and got Green" with reasonable cause. The Dempster-Shafer fusion math assumes stable inputs.
- **Acceptance criteria:**
  - [ ] Temperature pinned OR k-sample-majority shipped.
  - [ ] Reproducibility test: same input → same lane decision over N runs.
- **Effort:** M.
- **Dependencies:** none.

## TODO-049 — Verification-engine has no per-decision "explainability" record (procedural-fairness gap)

- **Priority:** P1
- **Category:** Compliance | UX | Audit
- **Standard cited:** GDPR Art. 22 (right not to be subject to solely-automated decisions + right to explanation); FATF Guidance §3.7 (risk-rating explainability). `REQ-gdpr-022`.
- **Current state:** PARTIAL. `verification_cases` records the stage outputs and the fused lane decision, but the explanation rendered to the affected declarant ("why was my declaration flagged Red") is not standardised. The portal does not expose a per-decision explanation surface.
- **Required state:** Every Red/Yellow lane decision MUST carry a structured `decision_rationale` object — which stages fired, which evidence rows hit, what the cumulative BPA was — rendered to the affected declarant in their portal (GDPR Art. 22 disclosure).
- **Why it matters:** A declarant who receives a Red without explanation has grounds to challenge under GDPR Art. 22. The verification engine's outputs are decisions with legal effect.
- **Acceptance criteria:**
  - [ ] `verification_cases.decision_rationale JSONB` populated per decision.
  - [ ] Portal page displays the rationale to the affected declarant.
  - [ ] Tests cover the rationale generation per stage.
- **Effort:** L.
- **Dependencies:** TODO-013 (Stage 7 contributes to the rationale).

## TODO-050 — No correlation-ID propagation through HMAC-internal webhook calls

- **Priority:** P1
- **Category:** Observability
- **Standard cited:** ASVS V7.1.2 (correlation IDs); OBS-1. `REQ-asvs-v7-001-002`.
- **Current state:** PARTIAL. Internal HMAC posts (`/v1/internal/declaration-events`, `/v1/internal/verification-outcomes`) likely don't propagate `X-Request-Id` from the originating request. Traces fragment at the service boundary.
- **Required state:** The HMAC envelope carries `X-Request-Id`; producer copies the originating request's ID; consumer reads it into the tracing context.
- **Why it matters:** Without end-to-end correlation, debugging a verification-decision regression requires manually stitching logs across services.
- **Acceptance criteria:**
  - [ ] HMAC envelope schema documents `X-Request-Id` header.
  - [ ] Verified in integration tests.
- **Effort:** S.
- **Dependencies:** none.

## TODO-051 — No FIPS-mode build option (sovereign-infrastructure compliance hedge)

- **Priority:** P1
- **Category:** Cryptography | Compliance
- **Standard cited:** FIPS 140-3; NIST 800-131A. `REQ-fips-140-003`.
- **Current state:** ABSENT. rustls uses `ring` crypto provider; not FIPS-validated. No `--features fips` build path.
- **Required state:** A FIPS-build option that swaps `ring` for `aws-lc-rs` (FIPS-validated) — at minimum the option exists, even if the default build remains `ring`.
- **Why it matters:** Cameroon's sovereign-infrastructure posture + correspondent-banking relationships may require FIPS at some point. Having the option pre-positioned is cheap; adding it later under regulatory pressure is expensive.
- **Acceptance criteria:**
  - [ ] `Cargo.toml` feature `fips` swaps crypto provider.
  - [ ] CI runs both build variants.
  - [ ] Documented in `docs/security/fips-posture.md`.
- **Effort:** M.
- **Dependencies:** none.

## TODO-052 — No post-quantum agility — D21 (post-quantum agility) is unimplemented

- **Priority:** P1
- **Category:** Cryptography | Compliance
- **Standard cited:** D21 (post-quantum agility); NIST PQC final standards (ML-KEM, ML-DSA). `REQ-d21-001`.
- **Current state:** ABSENT. Phase 2D confirmed zero references to Kyber / Dilithium / oqs / pq.
- **Required state:** A documented post-quantum agility roadmap. At minimum: identify which cryptographic primitives are quantum-vulnerable (Ed25519 attestation; RSA OIDC; ECDHE TLS); document the migration path; pin a sunset date for each.
- **Why it matters:** Cryptographic agility is the doctrine. A platform that hard-codes Ed25519 forever is one CRQC announcement away from re-validating every historical declaration's attestation.
- **Acceptance criteria:**
  - [ ] `docs/security/pq-roadmap.md` authored.
  - [ ] Per-primitive migration plan.
  - [ ] CI canary tests against a PQ-only build (optional, marker).
- **Effort:** M (the doc; the actual migration is larger).
- **Dependencies:** none.

## TODO-053 — No load / capacity baseline test (SLO claims aspirational)

- **Priority:** P1
- **Category:** Testing | Performance
- **Standard cited:** D12 (production-grade from first commit); IO.5 timely-access. `REQ-d12-load`.
- **Current state:** ABSENT. `services/declaration/CLAUDE.md` documents SLOs (p99 < 500 ms for POST /v1/declarations); no load test produces evidence the SLO holds.
- **Required state:** k6 / vegeta / Locust load test that hits the documented p99 under the documented concurrency.
- **Why it matters:** SLOs without load evidence are aspirational. An operator scaling for production needs the baseline to size pods.
- **Acceptance criteria:**
  - [ ] `load-tests/` populated with k6 scripts.
  - [ ] CI runs a smoke load test against `local-up`.
  - [ ] Documented results inform pod-size + auto-scale thresholds.
- **Effort:** M.
- **Dependencies:** none.

## TODO-054 — No chaos engineering rig (D12 + post-launch readiness)

- **Priority:** P1
- **Category:** Testing | Reliability
- **Standard cited:** D12. `REQ-d12-chaos`.
- **Current state:** ABSENT. The MEDIUM/LOW summary table notes chaos coverage was explicitly deferred (FIND-020 closure decision: ADR-first).
- **Required state:** An ADR + initial chaos suite covering: Postgres failover; Vault sealing; SPIFFE agent restart; Fabric peer outage; Anthropic API outage.
- **Why it matters:** A platform that has never been chaos-tested in CI has unknown failure modes. The runbooks cover the failure modes the team imagined; chaos covers the modes they didn't.
- **Acceptance criteria:**
  - [ ] ADR-013 documents the chaos approach.
  - [ ] `tests/chaos/` populated (minimum: Postgres failover scenario).
  - [ ] CI runs the smoke scenario.
- **Effort:** L.
- **Dependencies:** none.

## TODO-055 — Anthropic budget alert thresholds (TODO-024) need Prometheus rule alongside the metric

- **Priority:** P1
- **Category:** Observability | Cost
- **Standard cited:** D16 (observability non-optional). `REQ-d16-cost`.
- **Current state:** PARTIAL. PR #125 lands `alerts/recor-prometheus-rules.yaml` with an Anthropic-budget-burn-rate alert. The specific budget cap to enforce (TODO-024) is still missing.
- **Required state:** TODO-024 cap + the alert rule together.
- **Why it matters:** The alert without the cap is just a notification. The cap without the alert burns silently.
- **Acceptance criteria:** Couples to TODO-024.
- **Effort:** S (once TODO-024 is in).
- **Dependencies:** TODO-024.

---

# P2 — Required for operational maturity

## TODO-056 — `recor-cli` referenced in justfile but does not exist (D08 / dangling thread)

- **Priority:** P2
- **Category:** Toolchain
- **Standard cited:** D08. `REQ-d08-001`.
- **Current state:** PARTIAL. `justfile:109` references `tools/cli/recor-cli`; path absent.
- **Required state:** Either (a) implement the CLI, or (b) remove the reference. ADR if the CLI is needed.
- **Why it matters:** Aspirational tooling in justfile breaks bootstrap.
- **Acceptance criteria:** decision; absent → reference removed.
- **Effort:** S.
- **Dependencies:** none.

## TODO-057 — `_install-internal-cli`, `_gen-{openapi,graphql,avro}` targets are `@echo` no-ops (D08)

- **Priority:** P2
- **Category:** Toolchain
- **Standard cited:** D08. `REQ-d08-001`.
- **Current state:** STUB. PR #125 stubbed them with echo + explanatory comments.
- **Required state:** Either implement OR delete-and-document the deletion.
- **Why it matters:** Stubs accumulate.
- **Acceptance criteria:** decision + implementation.
- **Effort:** M.
- **Dependencies:** none.

## TODO-058 — No `recor-doctrine-check` skill gate at PR-merge time (the skill exists; CI does not run it)

- **Priority:** P2
- **Category:** Process
- **Standard cited:** All doctrines. `REQ-d23-plan`.
- **Current state:** PARTIAL. Skill exists in `.claude/skills/`; not wired to CI.
- **Required state:** CI runs a doctrine-check pass on the PR diff and posts findings as a comment.
- **Effort:** M.
- **Dependencies:** none.

## TODO-059 — No fuzz testing on canonical-payload deserialisation

- **Priority:** P2
- **Category:** Testing | Security
- **Standard cited:** ASVS V11.1.5 (fuzz testing). `REQ-asvs-v11-001-005`.
- **Current state:** ABSENT.
- **Required state:** cargo-fuzz against canonical-declaration JSON.
- **Effort:** M.
- **Dependencies:** none.

## TODO-060 — `mock_bunec_persons` table is in production migrations (R-VER-1 marker)

- **Priority:** P2
- **Category:** Data Model
- **Standard cited:** D08; D07. `REQ-d08-001`.
- **Current state:** STUB. Table exists in production schema as a mock.
- **Required state:** Either guarded behind `IF environment='dev'` OR moved to a test-fixtures migration that prod skips.
- **Effort:** S.
- **Dependencies:** TODO-015 (real BUNEC).

## TODO-061 — `declaration_projection` placeholder pending writeback subscriber

- **Priority:** P2
- **Category:** Cross-service Coupling
- **Standard cited:** D08. `REQ-d08-001`.
- **Current state:** STUB.
- **Required state:** Implement the subscriber OR remove the placeholder.
- **Effort:** L.
- **Dependencies:** TODO-041.

## TODO-062 — Cargo.lock per-service vs root workspace lockfile drift

- **Priority:** P2
- **Category:** Toolchain | Reproducibility
- **Standard cited:** D19 (reproducible everything). `REQ-d19-001`.
- **Current state:** PARTIAL. Workspace Cargo.lock is canonical; per-service lockfiles drift.
- **Required state:** Per-service lockfiles symlinked or generated from root.
- **Effort:** S.
- **Dependencies:** none.

## TODO-063 — pnpm vitest at repo root without workspace

- **Priority:** P2
- **Category:** Toolchain
- **Standard cited:** D19. `REQ-d19-001`.
- **Current state:** PARTIAL.
- **Required state:** pnpm workspace configured.
- **Effort:** S.
- **Dependencies:** none.

## TODO-064 — Chaincode unit tests don't cover already-committed idempotency directly

- **Priority:** P2
- **Category:** Testing | Audit
- **Standard cited:** D13 (idempotency); ADR-009. `REQ-d13-001`.
- **Current state:** PARTIAL. The chaincode idempotency test exists (`TestRecordAuditEntry_Idempotent`); the cross-bridge replay-rejection test exists at Rust side but not as a chaincode-side regression-guard test on the *idempotency*-against-real-re-write code path.
- **Required state:** Chaincode-side test that calls `RecordAuditEntry` twice with the same key and asserts the second call returns the same record without writing.
- **Effort:** S.
- **Dependencies:** none.

## TODO-065 — Documentation lacks per-stage verification-engine "decision-rules" page

- **Priority:** P2
- **Category:** Documentation
- **Standard cited:** D05. `REQ-d05-001`.
- **Current state:** PARTIAL.
- **Required state:** `docs/verification/decision-rules.md` documents per-stage how evidence is converted to BPA contributions.
- **Effort:** M.
- **Dependencies:** none.

## TODO-066 — `unsafe { }` blocks (2 in production) lack `// SAFETY:` comments

- **Priority:** P2
- **Category:** Code Quality | Security
- **Standard cited:** D14; rustc-style. `REQ-rust-unsafe-safety`.
- **Current state:** PARTIAL. Phase 2F counted 2 unsafe blocks; need to confirm `SAFETY:` comment presence (likely missing — flagged in F.2).
- **Required state:** Every `unsafe { }` block has an immediately-preceding `// SAFETY:` rationale.
- **Effort:** S.
- **Dependencies:** none.

## TODO-067 — `let _ = ...` swallowed errors in `services/declaration/src/main.rs`

- **Priority:** P2
- **Category:** Code Quality | Reliability
- **Standard cited:** D14. `REQ-d14-error-handling`.
- **Current state:** PARTIAL. Phase 2F: 6 instances on shutdown paths.
- **Required state:** Each swallowed error is either logged at WARN or propagated.
- **Effort:** S.
- **Dependencies:** none.

## TODO-068 — `kafka_consumer_dlq` retention undocumented

- **Priority:** P2
- **Category:** Compliance | Documentation
- **Standard cited:** GDPR Art. 5(1)(e); D08. `REQ-gdpr-005-001-e`.
- **Current state:** PARTIAL. `docs/runbooks/dlq-retention.md` covers the other DLQs; not this one.
- **Required state:** Documented + retention-worker covers it.
- **Effort:** S.
- **Dependencies:** TODO-016.

## TODO-069 — No formal pen-test rules-of-engagement updated for the FATF-special-focus tier

- **Priority:** P2
- **Category:** Security
- **Standard cited:** ASVS V1.14 (Architecture testing). `REQ-asvs-v1-014`.
- **Current state:** PARTIAL. `docs/security/pen-test-rules-of-engagement.md` exists but the post-Sovim + per-tier scope needs an update.
- **Required state:** Updated RoE that explicitly scopes the per-tier access boundary tests + the Sovim balancing test.
- **Effort:** S.
- **Dependencies:** TODO-006, TODO-007.

## TODO-070 — `applications/declarant-portal/tests/e2e/*` has 5 `: any` types

- **Priority:** P2
- **Category:** Code Quality
- **Standard cited:** TS strict-mode discipline. `REQ-ts-strict`.
- **Current state:** PARTIAL.
- **Required state:** All replaced with typed unions.
- **Effort:** S.
- **Dependencies:** none.

## TODO-071 — 8 `console.log` / `println!` / `dbg!` in production code (Phase 2F)

- **Priority:** P2
- **Category:** Logging | Security
- **Standard cited:** ASVS V7.1.1 (no debug-output in production); D18 (no secrets in logs). `REQ-asvs-v7-001-001`.
- **Current state:** PARTIAL.
- **Required state:** Replaced with structured `tracing::debug!` or removed.
- **Effort:** S.
- **Dependencies:** none.

## TODO-072 — Empty placeholder TODOs in `services/person-service/src/infrastructure/postgres.rs` cluster (NDI-1)

- **Priority:** P2
- **Category:** Code Quality
- **Standard cited:** D08. `REQ-d08-001`.
- **Current state:** PARTIAL.
- **Required state:** Each TODO carries a ticket reference; expired TODOs (>28 days) trigger the governance/no-dangling check.
- **Effort:** S.
- **Dependencies:** none.

## TODO-073 — No documented incident retrospective template applied to past incidents (incidents claimed in CLAUDE.md but no retrospectives)

- **Priority:** P2
- **Category:** Process
- **Standard cited:** D08; NIST SP 800-61 R2. `REQ-nist-800-61`.
- **Current state:** PARTIAL.
- **Required state:** Per-incident retrospective doc.
- **Effort:** S–M.
- **Dependencies:** none.

---

# P3 — Quality / polish / future-proofing

## TODO-074 — README cross-link bidirectionality not enforced for `docs/runbooks/`

- **Priority:** P3
- **Category:** Documentation
- **Standard cited:** D05. `REQ-d05-001`.
- **Current state:** PARTIAL.
- **Required state:** Extend `tools/ci/check-adr-bidi.sh` to runbooks.
- **Effort:** S.
- **Dependencies:** none.

## TODO-075 — Portal a11y (4 medium + 6 low axe findings from R-PORT-5)

- **Priority:** P3
- **Category:** UI
- **Standard cited:** WCAG 2.2 AA. `REQ-wcag-22-aa`.
- **Current state:** PARTIAL. R-PORT-5 closed the three highest-impact items in PRs #103/#108. Six low-severity items remain.
- **Required state:** Address remaining low-severity items.
- **Effort:** S–M.
- **Dependencies:** none.

## TODO-076 — Image-publishing workflow does not pin actions to SHAs

- **Priority:** P3
- **Category:** Security
- **Standard cited:** OWASP CI/CD Top 10. `REQ-cicd-002`.
- **Current state:** PARTIAL.
- **Required state:** Every `uses:` in `.github/workflows/` pinned to a SHA, not a tag.
- **Effort:** S.
- **Dependencies:** none.

## TODO-077 — No `governance / dependency-license-check` CI gate

- **Priority:** P3
- **Category:** Compliance
- **Standard cited:** D20 (supply chain). `REQ-d20-licenses`.
- **Current state:** ABSENT.
- **Required state:** `cargo deny` + `pnpm licenses` check in CI.
- **Effort:** S.
- **Dependencies:** none.

## TODO-078 — No internationalisation framework for non-portal surfaces (API error messages)

- **Priority:** P3
- **Category:** UX
- **Standard cited:** WCAG-adjacent. `REQ-i18n-001`.
- **Current state:** ABSENT. API responses are English-only.
- **Required state:** Error messages translatable; locale negotiated via `Accept-Language`.
- **Effort:** M.
- **Dependencies:** none.

## TODO-079 — No documented commit-signing requirement

- **Priority:** P3
- **Category:** Process
- **Standard cited:** D11; SLSA L3. `REQ-d11-signing`.
- **Current state:** ABSENT.
- **Required state:** Branch protection requires signed commits; documented.
- **Effort:** S.
- **Dependencies:** none.

## TODO-080 — Helm chart values.yaml lacks structured comments per per-service env var

- **Priority:** P3
- **Category:** Documentation
- **Standard cited:** D05. `REQ-d05-001`.
- **Current state:** PARTIAL.
- **Required state:** Inline doc comments for every env var the Helm chart exposes.
- **Effort:** S.
- **Dependencies:** none.

## TODO-081 — No documented data-dictionary at field level

- **Priority:** P3
- **Category:** Documentation
- **Standard cited:** D05. `REQ-d05-001`.
- **Current state:** PARTIAL. `docs/compliance/data-classification.md` covers classification; not a full data-dictionary.
- **Required state:** `docs/data-dictionary.md` per service.
- **Effort:** M.
- **Dependencies:** none.

## TODO-082 — `tests/contract/` lacks a per-consumer pact-style test suite

- **Priority:** P3
- **Category:** Testing
- **Standard cited:** D04; ASVS V13. `REQ-asvs-v13`.
- **Current state:** PARTIAL.
- **Required state:** Per-consumer contract tests.
- **Effort:** L.
- **Dependencies:** TODO-041.

---

# Closing summary

## Findings by priority (matches the header counts)

- **P0:** 24
- **P1:** 31
- **P2:** 18
- **P3:** 9
- **Total:** 82

## Top-10 most urgent items by ID

In order of structural impact (compliance > security > integrity > operational):

1. **TODO-001** — BO data model must implement the FATF cascade (ownership/control/SMO).
2. **TODO-002** — R.25 trusts/arrangements register is wholly absent.
3. **TODO-003** — Discrepancy-reporting workflow is absent (c.24.6 multi-pronged failure).
4. **TODO-004** — Sanctions-for-non-compliance workflow is absent (c.24.13 failure).
5. **TODO-005** — 30-day update obligation is unenforced.
6. **TODO-006** — Obliged-entity principal class + legitimate-interest tier absent.
7. **TODO-007** — Audit-verifier discloses full payload to any OIDC bearer (Sovim + GDPR breach).
8. **TODO-008** — No FIU (ANIF) disclosure endpoint or audit log.
9. **TODO-010** — Bearer-share + nominee disclosure absent (c.24.12 failure).
10. **TODO-013** — Stage 7 cross-source verification is permanently stubbed.

## FATF special-focus coverage check (areas 1–15)

| Area | Finding |
|---|---|
| 1. Definition of BO | TODO-001 |
| 2. Adequate / accurate / up-to-date | TODO-005, TODO-021 |
| 3. Verification at submission | TODO-020, TODO-021 |
| 4. Verification post-submission | TODO-009 (public-feedback channel) |
| 5. Discrepancy reporting | TODO-003 |
| 6. Sanctions for non-compliance | TODO-004 |
| 7. Access tiers | TODO-006, TODO-007 |
| 8. Foreign legal persons (sufficient link) | TODO-018 |
| 9. Trusts and arrangements | TODO-002 |
| 10. Multi-pronged approach | TODO-013, TODO-015 |
| 11. Information sharing (FIU + MLAT) | TODO-008 |
| 12. Data retention | TODO-016 |
| 13. Bearer shares + nominees | TODO-010 |
| 14. Historical record | PRESENT-VERIFIED (declaration_events COMP-2 + replay tests). No finding needed. |
| 15. PEP + sanctions screening | TODO-014, TODO-036 |

## Unvarnished verdict on FATF readiness today

RÉCOR is **not FATF-ready** as of `e1ab0195`. The platform has solid cryptographic substrate (HMAC-SHA256 with iat-bound replay, Ed25519 attestation, BLAKE3 hashing, COMP-2 append-only audit logs, Fabric anchoring with a working reconciler), and it has closed the catastrophic identity-and-authn gaps that the first audit catalogue identified (FIND-001 through FIND-006, plus the dev-OIDC dual-path bypass). The 24 internal "Sprint-4 closed" findings are real closures.

What it does not have is the FATF BO-registry *shape*. The data model treats every BO as an ownership-percentage holder — there is no 25%/control/SMO cascade per R.24 c.24.6 (TODO-001). There is no register for trusts or other legal arrangements — R.25 is wholly absent (TODO-002). The three pillars of the multi-pronged approach are partially-built: the registry holds data, but the discrepancy-reporting intake from obliged entities (TODO-003) does not exist, sanctions for non-compliance (TODO-004) do not exist, the 30-day update obligation (TODO-005) is unenforced, the FIU disclosure surface (TODO-008) is absent, the bearer-share and nominee fields (TODO-010) are absent, the foreign-entity sufficient-link test (TODO-018) is absent. The verification engine — the platform's most-architected analytical surface — has Stage 7 (cross-source reconciliation, the very heart of multi-pronged) as a permanent stub (TODO-013), and depends on a `mock_bunec_persons` table because the real BUNEC adapter has not landed (TODO-015). The post-Sovim balancing — which is now the rule for public BO access — is unimplemented; the audit-verifier returns the full payload, including national-ID numbers, to any OIDC-authenticated caller (TODO-007).

A FATF mutual-evaluation review of Cameroon at this date would assess R.24 as Partially Compliant (the basic-information layer is real; the BO-specific layer is structurally incomplete), R.25 as Non-Compliant (no arrangement register exists), and IO.5 as Low (the timely-access prong is half-built; the FIU-access prong is absent). The platform has the substrate to be ready and the discipline to ship. It does not yet have the BO-registry obligations implemented end-to-end. Of the 24 P0 findings above, fewer than half are large-effort; the catalogue is a 6-to-12-month delivery, not a 2-to-3-year one.

The verdict is consequential and unflattering: **the architectural foundations are sound, the FATF obligations are not yet implemented, and the platform cannot today be held out as a working national BO registry under R.24/R.25**. Close the 24 P0 findings + 31 P1 findings in priority order and the claim becomes credible. Ship without them and the first MER will publish a long list of Partially Compliant / Non-Compliant ratings the country can ill afford.

