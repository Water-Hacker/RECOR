# Data Protection Impact Assessment (DPIA) — RÉCOR Platform

**Reference:** GDPR Art. 35 / OHADA AML/CFT framework
**Ticket:** TODO-033 (COMP-6)
**Status:** Draft — pending DPO review and AML/CFT counsel sign-off
**Owner:** RÉCOR security-engineering (engineering), DPO (legal sign-off)
**Last updated:** 2026-05-20

> **Counsel note.** Lines tagged `[CITATION NEEDED: <slug>]` are
> deliberate placeholders. AML/CFT counsel must resolve each before
> this document is treated as authoritative. Claude and engineering
> do not invent citations.

## 1. Overview and legal trigger

GDPR Art. 35(1) requires a DPIA prior to processing that is "likely to
result in a high risk to the rights and freedoms of natural persons."
Art. 35(3) specifies mandatory cases including systematic, large-scale
processing of personal data. RÉCOR processes PII on every natural person
who appears as a beneficial owner in a Cameroonian corporate disclosure:
names, nationalities, ownership stakes, and — in future phases —
identity-document numbers and biometric references. The processing is
systematic and is conducted by a state-adjacent registry authority.
This DPIA is therefore mandatory.

The DPIA also responds to the CJEU ruling in Case C-37/20 (*WM and Sovim
SA v. Luxembourg Business Registers*), which held that unrestricted
public access to beneficial-ownership registers violates GDPR Art. 5(1)(c)
data minimisation. Every public-facing surface of RÉCOR must be balanced
against that ruling; the balancing result for each surface is recorded in
§ 4 below.

### 1.1 Supervisory authority

Cameroon's designated supervisory authority for beneficial-ownership data
protection is `[CITATION NEEDED: cameroon-data-protection-authority]`.
The DPO files this DPIA with that authority before the platform
enters pre-production operations. The filing reference will be recorded
in the sign-off section (§ 9).

### 1.2 Related compliance artefacts

| Document | Role relative to this DPIA |
|---|---|
| `docs/compliance/data-classification.md` (COMP-3) | Per-column PII inventory this DPIA draws from |
| `docs/compliance/gdpr-procedures.md` (COMP-1) | Operational procedures for the six data-subject rights |
| `docs/compliance/data-retention.md` (COMP-2) | Retention rules that bound how long PII is held |
| `docs/compliance/regulatory-mapping.md` (COMP-4) | Endpoint-level legal-basis citations |
| `docs/security/threat-model.md` (DOC-4) | STRIDE analysis + residual-risk catalogue |
| `docs/adr/0010-fatf-bo-cascade-and-adequacy.md` | FATF cascade decision that shapes the data model |
| `docs/adr/0012-sanctions-proportionality-ladder.md` | Sanctions ladder post-Sovim balancing |

---

## 2. Description of processing

### 2.1 Processing flows

The platform executes seven distinct processing flows. Each flow is
described by the data it receives, the actors involved, the purpose,
and the downstream consumers.

#### Flow 1 — Declaration intake

**Trigger.** A declarant (natural person or corporate agent) submits a
beneficial-ownership declaration via `POST /v1/declarations`.

**Data received.**

- `declarant_principal`: SPIFFE URI or OIDC subject — **PII**
- `entity_id`: UUID identifying the legal entity — **Public**
- `beneficial_owners[]`: array of `{person_id, ownership_basis_points,
  cascade_tier, control_basis, cascade_tier_b_ruled_out_evidence,
  is_nominee, nominator_person_id}` — **PII** (person_id and derived
  fields carry identity when combined with BUNEC records)
- `adequacy_claims`: declarant attestation including `legal_basis` and
  `up_to_date_as_of` — **PII** (declarant-authored; signed)
- `attestation`: Ed25519 signature envelope — **Confidential**

**Actors.** Declarant (data subject or agent); RÉCOR declaration service;
OIDC issuer (principal verification); BUNEC adapter (future identity check).

**Purpose.** Record the identity of natural persons who ultimately control
or own Cameroonian corporate entities. Legal basis: statutory obligation
under Cameroon's AML/CFT implementing instruments.
`[CITATION NEEDED: cameroon-bo-decree-art-Y]`

**Storage.** `declarations` table (projection, forever) + `declaration_events`
table (append-only event log, forever per COMP-2) + `outbox` (operational
queue, pruned 30 days post-dispatch).

**Downstream consumers.** Verification engine (Flow 2), FIU disclosure
(Flow 4), obliged-entity read (Flow 5), public-feedback (Flow 6).

#### Flow 2 — Verification engine pipeline

**Trigger.** `declaration.submitted.v1` event consumed from the outbox
or Kafka topic by the verification engine.

**Data received.** Full `DeclarationSnapshot` including all PII fields
from Flow 1.

**Stages and processing.**

| Stage | Processing | PII touched |
|---|---|---|
| Stage 1 — Schema validation | Deterministic format checks; no external I/O | All snapshot fields |
| Stage 2 — Identity authentication | BUNEC lookup per person_id; returns canonical name + nationality | person_id, canonical_full_name, nationality |
| Stage 3 — Sanctions screening | Name-match against OFAC/UN/EU sanctions lists | person_id, full_name, nationality, date_of_birth (when available) |
| Stage 4 — PEP screening | Name-match against OpenSanctions PEP list | Same as Stage 3 |
| Stage 5 — Adverse media | ICIJ Offshore Leaks lookup + Anthropic inference | full_name, entity context |
| Stage 6 — Pattern detection | Structural graph queries; no person-level lookup | entity_id, person_id (as graph node references) |
| Stage 7 — Cross-source triangulation | Reads upstream outcomes; no additional personal data fetch | None beyond Stage outcomes |
| Stage 8 — Dempster-Shafer fusion | Pure math over BPAs; no PII | None |
| Stage 9 — Lane routing | Threshold logic; no PII | None |

**Actors.** Verification engine service; BUNEC adapter; sanctions/PEP
adapter; ICIJ adapter; Anthropic Inference Gateway (Stages 5, 7 via
`R-VER-4`, `R-VER-6`).

**Purpose.** Establish the authenticity and risk profile of the declared
beneficial-ownership structure. Required for FATF R.24 c.24.6 multi-pronged
verification.

**Storage.** `verification_cases` (append-only, forever per COMP-2) +
`verification_outbox` (pruned 30 days post-dispatch).

#### Flow 3 — Audit chain anchoring

**Trigger.** Each declaration event and verification case outcome is
anchored to the Hyperledger Fabric audit channel (R-DECL-9).

**Data received.** BLAKE3 receipt hash of the event payload — the hash
is a Confidential, non-PII derived value. The raw PII payload is NOT
transmitted to Fabric; only the hash is.

**Purpose.** Tamper-evident cryptographic provenance per D15.

**Storage.** Fabric ledger (immutable, perpetual).

#### Flow 4 — FIU disclosure (ANIF)

**Trigger.** ANIF operator authenticates and queries
`GET /v1/verifications/{id}` or the FIU search surface.

**Data received.** Verification case record including `declarant_principal`,
`case_payload` (full PII), `lane`, Dempster-Shafer belief masses.

**Actors.** ANIF (financial intelligence unit); RÉCOR consumer-access surface.

**Purpose.** Financial intelligence investigation per CEMAC AML/CFT
framework. `[CITATION NEEDED: cemac-aml-cft-fiu-access]`

**Storage.** `fiu_disclosure_log` (planned; every ANIF read is recorded
with actor, timestamp, case_id, and correlation_id — no PII duplication).

#### Flow 5 — Obliged-entity read (ARMP, DGI, BEAC, customs, CONAC)

**Trigger.** An authorised consumer service authenticates and queries
entity or declaration records.

**Data received.** Varies by consumer tier (per Architecture V1 P11):

- **Tier A** (ANIF, CONAC): full PII access including `declarant_principal`
  and `beneficial_owners` payload.
- **Tier B** (ARMP, DGI, BEAC, customs): entity-level data + verification
  lane; PII access gated by OPA policy; Sovim redaction applied to
  natural-person fields unless the consumer can assert a specific
  regulatory need.
- **Tier C** (public portal): entity names and lane only; all natural-person
  PII redacted per the post-Sovim balancing in § 4.

**Purpose.** Regulatory supervision, procurement integrity, tax administration,
financial stability oversight, customs compliance.

#### Flow 6 — Public feedback

**Trigger.** A member of the public submits a discrepancy report via
`POST /v1/public-feedback` (subject to CAPTCHA).

**Data received.** Entity identifier (Public), free-text description
(may incidentally contain PII), CAPTCHA token, IP-derived geolocation
(dropped before persistence).

**Storage.** `public_feedback_log` (planned; 90-day retention for
operational triage; content audited for incidental PII before storage).

#### Flow 7 — Sanctions public list

**Trigger.** Any caller (including unauthenticated) queries
`GET /v1/sanctions/public`.

**Data received.** No personal data submitted. Returns `public_listing_name`
and `public_listing_reason` for entities at `public_listed` tier.

**Purpose.** Transparency and deterrence per FATF R.24 c.24.13 and
ADR-0012 proportionality ladder.

**Post-Sovim balancing.** See § 4.4 below.

### 2.2 Data-subject categories

| Category | Description | Volume estimate |
|---|---|---|
| Declarants | Natural persons who file declarations (sole traders, corporate agents, operator-assisted filers) | Tens of thousands at launch; growing with mandatory phased rollout |
| Beneficial owner subjects | Natural persons named as BOs in declarations (may differ from declarants) | Same order of magnitude; one person may appear across multiple entities |
| Obliged-entity users | Staff of ARMP, ANIF, DGI, BEAC, customs, CONAC querying the registry | Hundreds of named principals |
| Public feedback submitters | Members of the public filing discrepancy reports | Low volume; anonymous where CAPTCHA-only auth is used |

### 2.3 Special categories

No special-category data (GDPR Art. 9) is collected at launch. The planned
`primary_id_document` and `biometric_reference_hash` columns on the future
`persons` table (R-DECL-4) will require a supplementary DPIA amendment
before those fields are activated; they are pre-classified as Sensitive-PII
in `data-classification.md` and will require field-level encryption
(ticket `R-ENC-FIELD-LEVEL`) as a prerequisite.

---

## 3. Necessity and proportionality assessment

### 3.1 Legal basis

The primary legal basis for every PII-processing flow at RÉCOR is
**GDPR Art. 6(1)(c) — legal obligation**: Cameroonian law compels the
creation and maintenance of the beneficial-ownership register. The
platform does not rely on consent, and does not need to, because the
statutory obligation is the autonomous basis.

The secondary basis invoked for public-interest processing flows (public
sanctions list, FIU disclosure) is **Art. 6(1)(e) — public task**:
`[CITATION NEEDED: cameroon-public-task-basis]`.

Data subjects cannot opt out of being named as beneficial owners in
declarations filed by corporate agents on behalf of entities they
control. This constraint is explicitly noted in the Art. 21 procedure in
`gdpr-procedures.md` — the data subject's right to object does not
override a statutory obligation.

### 3.2 Data minimisation assessment per flow

| Flow | PII collected | Minimisation assessment |
|---|---|---|
| Declaration intake | declarant_principal, beneficial_owners[], adequacy_claims | Minimum required by the FATF cascade (ADR-0010). No field is collected beyond the statutory requirement. The `cascade_tier_b_ruled_out_evidence` free-text field is bounded to 16–2048 chars (length-capped to prevent large-text PII burial). |
| Verification — Stage 2 | person_id → BUNEC lookup → canonical_name, nationality | Minimum required for identity authentication. Date-of-birth is carried only when the BUNEC record includes it; the stage does not request it independently. |
| Verification — Stage 3/4 | Full name, nationality, optional DoB | Minimum required for sanctions/PEP screening. The adapter returns up to 5 candidates; no bulk download of the sanctions list is performed per-declaration. |
| Verification — Stage 5 | Full name, entity context | Minimum required for adverse-media analysis. The Inference Gateway receives the minimum context needed to produce a verdict; raw PII is not retained by Anthropic beyond the API call. `[CITATION NEEDED: anthropic-dpa-ref]` |
| FIU disclosure | Full case record | ANIF's mandate requires full-record access. The disclosure log records access without duplicating the payload. |
| Public portal | Entity name + lane | Natural-person fields redacted per post-Sovim balancing (§ 4). |
| Public feedback | Free text | IP-derived geolocation dropped pre-persistence. Content audited for incidental PII before the 90-day retention window begins. |

### 3.3 Purpose limitation

All processing is bounded to the statutory beneficial-ownership
purpose. The verification engine's outputs (belief masses, lane
decisions) are not used for any secondary purpose outside the
platform's regulatory disclosure framework. This commitment is
enforced architecturally: verification cases are accessible only
via the authenticated consumer-access surface (OPA policies in
`policies/`); no raw case data is exposed to unauthenticated callers.

### 3.4 Storage limitation

Retention policy is governed by `docs/compliance/data-retention.md`
(COMP-2). The key constraints:

- Event logs (`declaration_events`, `verification_cases`): retained
  forever. This retention is justified by the OHADA AML/CFT audit
  obligation — the audit trail is the load-bearing record.
  `[CITATION NEEDED: ohada-aml-cft-retention-period]`
- Operational queues (`outbox`, `verification_outbox`): pruned 30 days
  post-dispatch per COMP-2.
- Idempotency cache: TTL 24 h.
- Public feedback log: 90-day operational triage window (planned).

The "forever" retention of the event log is the principal tension between
GDPR's storage-limitation principle (Art. 5(1)(e)) and the AML/CFT
obligation. The platform's position — consistent with the OHADA AML/CFT
carve-out — is that the statutory retention obligation supersedes the
storage-limitation principle for the event log rows. The GDPR Art. 17(3)(b)
exemption (legal obligation) is the operative basis.
`[CITATION NEEDED: ohada-aml-cft-art-X-retention-exemption]`

---

## 4. Post-Sovim balancing — public-facing surfaces

The CJEU in Case C-37/20 held that blanket public access to all
beneficial-ownership fields violates the GDPR data-minimisation and
necessity principles. Each public-facing surface must therefore be
assessed independently.

### 4.1 Declaration submit surface

**Exposure.** The `POST /v1/declarations` endpoint is not public; it
requires OIDC authentication. Natural-person PII in the payload is
never returned to unauthenticated callers.

**Balancing result.** No public exposure. Post-Sovim concern does not
apply.

### 4.2 FIU search surface (ANIF)

**Exposure.** ANIF operators receive full PII including `declarant_principal`
and `beneficial_owners` payload.

**Justification for full access.** ANIF is a statutory intelligence body
with an explicit legal mandate to receive full beneficial-ownership data
for financial crime investigation. `[CITATION NEEDED: cemac-anif-mandate]`
The access is authenticated, role-gated (OPA policy), and logged to
`fiu_disclosure_log` with tamper-evident anchoring. This passes the
Sovim necessity test: ANIF's mandate cannot be discharged without the
full record.

**Balancing result.** Full PII access justified. D17 zero-trust boundary
and disclosure log satisfy the proportionality test.

### 4.3 Obliged-entity GET surface (Tier B consumers)

**Exposure.** ARMP, DGI, BEAC, customs, sectoral cadastres receive entity
data and verification lane. Natural-person fields (names of BOs, declarant
principal) are redacted by the OPS-2 `RedactingLayer` unless the consumer
can assert a case-specific regulatory need via an OPA claim.

**Justification.** The CJEU distinguishes cases where access is "necessary
and proportionate" for a specific regulatory purpose from general public
access. Tier B consumers receive PII only when their OPA claim matches a
documented regulatory article. `[CITATION NEEDED: cameroon-sector-regulatory-access-arts]`

**Balancing result.** Conditional PII access. The OPA gating + disclosure
log satisfies proportionality. Default is redacted; access to natural-person
fields is the exception, not the rule.

### 4.4 Public sanctions list

**Exposure.** `GET /v1/sanctions/public` returns `public_listing_name` and
`public_listing_reason` for entities at `public_listed` tier. This is entity-level
data (legal entities, not natural persons). Natural persons are not directly
named on the public list.

**Justification.** FATF R.24 c.24.13 requires public deterrence; ADR-0012
justifies the ladder design. Entity names are statutorily public. The
24-hour cache gives a correction window for wrongful listings. The
`public_listing_reason` is bounded to operator-authored text that must
cite the proceeding number; free-form text that might embed PII is refused
at the API boundary.

**Balancing result.** Entity names: justified as statutory public record.
Natural-person PII: not exposed. Post-Sovim concern does not apply to
the current design.

### 4.5 Public feedback surface

**Exposure.** The public submits free text; the platform does not expose
PII from the registry in response.

**Balancing result.** No PII exposure from the platform side. Ingested
free-text is audited for incidental PII before the 90-day retention window.

---

## 5. Risk-to-rights assessment

### 5.1 Risk taxonomy

Each risk is scored on two axes: **likelihood** (how probable is the
harm materialising, given current controls) and **severity** (how serious
is the harm to the data subject). The product gives the **residual risk**
tier: Low / Medium / High.

### 5.2 Risk register — declarants

| Risk ID | Risk description | Likelihood | Severity | Residual risk | Mitigation reference |
|---|---|---|---|---|---|
| R-DCL-01 | Declarant identity (`declarant_principal`) leaked via logs | Low | High | **Low** | OPS-2 RedactingLayer redacts `declarant_principal` in all log lines; D18 enforced |
| R-DCL-02 | Declarant subject to unjustified verification delay harming business | Low | Medium | **Low** | SLO < 30s p99 for green-lane; yellow/red routes to analyst who has a documented timeline |
| R-DCL-03 | Declarant account compromised; false declaration filed | Medium | High | **Medium** | IAL/AAL controls via OIDC; Ed25519 attestation per declaration; audit log captures the fraud trail |
| R-DCL-04 | Declarant cannot exercise right of access to their own data | Low | Medium | **Low** | `GET /v1/declarations/by-principal` provides synchronous self-service; DPO path for indirect access |

### 5.3 Risk register — beneficial-owner subjects

| Risk ID | Risk description | Likelihood | Severity | Residual risk | Mitigation reference |
|---|---|---|---|---|---|
| R-BO-01 | BO PII (name + nationality) exposed to public without legal basis (Sovim violation) | Low | High | **Low** | Public portal redacts all natural-person fields; OPS-2 + OPA gate access; post-Sovim balancing in § 4 |
| R-BO-02 | BO falsely flagged by sanctions/PEP screen; reputational harm | Medium | High | **Medium** | Lane is advisory, not binding; analyst review required for Yellow/Red; ADR-0002 yellow-lane design specifically avoids auto-reject on incomplete evidence |
| R-BO-03 | BO PII exfiltrated from `verification_cases` via SQL injection | Low | High | **Low** | Parameterised queries (sqlx); `verification_cases` append-only trigger; service-role-only DB credentials |
| R-BO-04 | BO named in public sanctions list incorrectly | Low | High | **Low** | Only entity names appear on public list (§ 4.4); 24-hour cache; `withdraw` endpoint; ADR-0012 |
| R-BO-05 | BO cannot exercise right of rectification — agent declarant refuses to amend | Medium | Medium | **Medium** | DPO can authorise correction event; amend endpoint exists; procedure documented in gdpr-procedures.md § 2 |
| R-BO-06 | BO identity document (future Sensitive-PII) breached | Low | Critical | **Medium** | Sensitive-PII fields not yet activated; field-level encryption (`R-ENC-FIELD-LEVEL`) is a prerequisite to activation; this DPIA requires amendment before Sensitive-PII goes live |

### 5.4 Risk register — obliged-entity users

| Risk ID | Risk description | Likelihood | Severity | Residual risk | Mitigation reference |
|---|---|---|---|---|---|
| R-OE-01 | Obliged-entity credential compromised; unauthorized access to PII | Low | High | **Low** | SPIFFE mTLS + OIDC; admin allowlist; OPA policy; zero-trust boundary D17 |
| R-OE-02 | FIU operator searches for PII without documented regulatory need | Low | Medium | **Low** | `fiu_disclosure_log` records every search; disclosure log is immutable and reviewed quarterly |

### 5.5 Risk register — public feedback submitters

| Risk ID | Risk description | Likelihood | Severity | Residual risk | Mitigation reference |
|---|---|---|---|---|---|
| R-PF-01 | Public feedback form abused to submit PII about third parties | Medium | Medium | **Medium** | CAPTCHA; content audit pre-persistence; 90-day retention; no PII duplication into permanent store |
| R-PF-02 | Public feedback submitter re-identified via free text | Low | Low | **Low** | IP-geolocation dropped pre-persistence; no session linkage |

---

## 6. Mitigations in place

### 6.1 Identity assurance (IAL/AAL)

The platform enforces OIDC-based identity assurance for all authenticated
flows. The `ACR` claim in the JWT is validated per ADR-0004
(`docs/adr/0004-oidc-jwks-principal-authentication.md`). The minimum
assurance level for declaration submission is IAL2/AAL2 (authenticator
assurance requiring MFA); operator-level access (DLQ admin, sanctions
officer) requires IAL2/AAL3 (phishing-resistant authenticator).
`[CITATION NEEDED: oidc-acr-level-mapping-to-cameroonian-identity-scheme]`

### 6.2 Post-Sovim redaction (OPS-2)

The `packages/recor-logging/src/lib.rs` `RedactingLayer` suppresses all
PII-classified field names (`declarant_principal`, `person_id`, `principal`,
`subject`) from structured log lines. Consumer-access responses are filtered
by OPA policy before transmission. The Sovim-mandated redaction is enforced
at three independent boundaries: the log layer, the OPA policy, and the
response serialiser (which omits redacted fields from the JSON shape).

### 6.3 Audit logs and cryptographic provenance (D15)

Every consequential event — declaration submission, verification case
creation, FIU access, data-subject rights exercise, sanctions escalation —
is captured in the append-only event log or the FIU disclosure log, with
a BLAKE3 receipt hash anchored to the Hyperledger Fabric audit channel
(R-DECL-9). The immutability is enforced by database-level triggers
(`0007_audit_log_immutability.sql`, `0003_audit_log_immutability.sql`).

### 6.4 Retention policies (COMP-2)

Operational queues are pruned on a documented cadence. The event logs
are retained forever under the OHADA AML/CFT exemption. The 24-hour
idempotency TTL limits how long request-replay data (which carries PII)
persists in operational storage.

### 6.5 Encryption

**At-rest.** Database-level encryption is enforced at the block-device
layer in the hosting environment (operator responsibility; gap G3 in
`docs/security/threat-model.md`). Row-level encryption for PII columns
is not yet activated (ticket `R-ENC-FIELD-LEVEL`); this is a documented
residual risk (R-BO-06 above).

**In-transit.** All internal service-to-service traffic is mTLS via
SPIFFE/SPIRE (ADR-0008). All external HTTPS traffic enforces TLS 1.3
minimum.

**Post-quantum agility.** D21 commits to a post-quantum migration path.
The current cryptographic substrate (Ed25519 + BLAKE3 + HMAC-SHA256) is
classical; the PQC migration is tracked but not yet activated.

### 6.6 Data-subject rights operationalisation

Six GDPR data-subject rights are operationalised per `gdpr-procedures.md`:
access (Art. 15), rectification (Art. 16), restricted erasure (Art. 17
with OHADA carve-out), portability (Art. 20), restrict (Art. 18 — partial),
object (Art. 21 — limited by statutory basis). The DPO mailbox
(`dpo@recor.cm`) is the intake point for non-self-service requests.

**Self-service endpoints (TODO-032, shipped):**

| Right | Article | Endpoint | Notes |
|---|---|---|---|
| Access + Portability | Art. 15 + 20 | `GET /v1/me/export` | Returns `$type=RecorGdprExport` envelope: declarations + rectification requests + erasure-restriction requests. Admins may pass `?principal=...` for delegated DSAR fulfilment. |
| Rectification | Art. 16 | `POST /v1/me/rectify` | Records the request; admin approval at `POST /v1/internal/rectification-requests/{id}/approve` ratifies it. The declaration itself is only modified when the declarant submits a Correct or Amend command bearing their own Ed25519 attestation (D15 — the platform never signs on the declarant's behalf). |
| Erasure (refused) + Restriction | Art. 17 + 18 | `POST /v1/me/erasure-restriction` | Always returns 400 with `erasure_not_permitted` (R.24 retention beats Art. 17); the restriction is recorded and the projection's `restricted_at` flag is set so downstream reads carry the Art. 18(2) notice. |

### 6.7 Article 30 records-of-processing register (TODO-034, shipped)

The Art. 30 register is maintained as structured data in
`gdpr_processing_register` and `gdpr_processing_register_events`
(migrations 0020 + 0021). Eight seed rows cover the current
processing activities: BO declaration intake (R.24 c.24.4 + 6AMLD
Art. 30 + GDPR Art. 6(1)(c)), BO verification engine processing,
FIU disclosure (R.24 c.24.9 + R.40), obliged-entity access (6AMLD
Art. 12 + AMLR Chapter IV), public-feedback intake, sanctions
proceedings (R.24 c.24.13), audit verification (D15), and
rectification + erasure-restriction handling. Admin endpoints under
`/v1/internal/gdpr/processing-records` allow the DPO to add, list,
retrieve, and retire records as the platform evolves; every state
change writes to the COMP-2-immutable event log.

---

## 7. Residual risk summary

| Risk tier | Count | Risks |
|---|---|---|
| **Low** | 9 | R-DCL-01, R-DCL-02, R-DCL-04, R-BO-01, R-BO-03, R-BO-04, R-OE-01, R-OE-02, R-PF-02 |
| **Medium** | 6 | R-DCL-03, R-BO-02, R-BO-05, R-BO-06, R-PF-01, R-BO-06 |
| **High** | 0 | — |
| **Critical** | 0 | — (R-BO-06 is Critical severity but Low likelihood giving Medium residual) |

No residual risk is rated High or Critical. The remaining Medium risks are
accepted with documented mitigations and tracked follow-up tickets. No
Medium risk blocks the platform's pre-production entry gate.

The Sensitive-PII risk (R-BO-06) is the most consequential. It is
currently Low likelihood because Sensitive-PII fields are not yet activated.
The activation of `primary_id_document` and `biometric_reference_hash`
(R-DECL-4) requires: (a) `R-ENC-FIELD-LEVEL` shipped and validated,
(b) a DPIA amendment reviewed and signed by the DPO, (c) counsel sign-off
on the OHADA basis for holding biometric-adjacent data.

---

## 8. Consultation

### 8.1 Internal consultation

| Stakeholder | Role | Input |
|---|---|---|
| RÉCOR security-engineering | Lead author | Technical accuracy of processing flows, mitigations |
| architect-reviewer | Architectural review | Consistency with Architecture V1 P11 consumer-access design |
| security-reviewer | Threat-model linkage | Risk-to-rights alignment with `docs/security/threat-model.md` |
| DPO | Data-protection review | Legal basis assessment, Art. 35 necessity determination |

### 8.2 External consultation

Per GDPR Art. 35(9), where a DPIA indicates a high residual risk that
cannot be mitigated, the controller must consult the supervisory authority
before processing. No residual High risk is identified in the current
assessment; consultation is therefore not mandatory. The DPO will
nevertheless file a voluntary notification with the supervisory authority
prior to pre-production operations as a transparency measure.

---

## 9. Sign-off

| Role | Name | Signature | Date |
|---|---|---|---|
| DPO | _______________ | _______________ | _______________ |
| AML/CFT counsel | _______________ | _______________ | _______________ |
| Security-team lead | _______________ | _______________ | _______________ |
| Engineering lead | _______________ | _______________ | _______________ |

**Supervisory authority filing reference:** _______________ (to be completed post-filing)

**Review cadence.** This DPIA is reviewed annually or on any material
change to the processing (new data category, new consumer tier, new
retention rule, DPIA amendment for Sensitive-PII activation).

---

## References

- GDPR Art. 5, 6, 17, 23, 35 — primary legal framework
- CJEU Case C-37/20 *WM and Sovim SA v. Luxembourg Business Registers*
- OHADA AML/CFT Acte uniforme 2014 — `[CITATION NEEDED: ohada-aml-cft-art-X]`
- FATF Recommendation 24 c.24.6, c.24.8, c.24.13
- `docs/compliance/data-classification.md` (COMP-3)
- `docs/compliance/gdpr-procedures.md` (COMP-1)
- `docs/compliance/data-retention.md` (COMP-2)
- `docs/compliance/regulatory-mapping.md` (COMP-4)
- `docs/security/threat-model.md` (DOC-4)
- `docs/adr/0002-dempster-shafer-fusion.md`
- `docs/adr/0004-oidc-jwks-principal-authentication.md`
- `docs/adr/0008-spiffe-mtls.md`
- `docs/adr/0010-fatf-bo-cascade-and-adequacy.md`
- `docs/adr/0012-sanctions-proportionality-ladder.md`
- Architecture V1 P11 § Consumer Access
- Architecture V1 P2 — doctrines D14, D15, D17, D18
