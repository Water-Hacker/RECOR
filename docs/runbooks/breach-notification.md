# Runbook — Personal Data Breach Notification

**References:** GDPR Art. 4(12), Art. 33, Art. 34; OHADA AML/CFT framework
**Ticket:** TODO-035 (COMP-7)
**Status:** Operational — pending DPO and counsel sign-off on contact skeleton
**Owner:** RÉCOR security-engineering (procedure), DPO (execution authority)
**Last updated:** 2026-05-20

> **72-hour clock starts at detection, not at full assessment.** If you
> are unsure whether an event qualifies, start the clock and escalate;
> you can withdraw a precautionary notification more easily than you can
> explain a missed deadline.

---

## 1. Definition of a personal data breach

GDPR Art. 4(12): "a breach of security leading to the accidental or
unlawful destruction, loss, alteration, unauthorised disclosure of, or
access to, personal data transmitted, stored or otherwise processed."

In RÉCOR's context, the following event classes meet this definition:

| Event class | Example |
|---|---|
| Unauthorised access to PII | An actor without a valid OPA claim reads `verification_cases.case_payload` |
| Accidental disclosure | A log line containing `declarant_principal` is shipped to an unauthenticated log aggregator |
| Exfiltration | A threat actor extracts `beneficial_owners` records from the database |
| Integrity breach | An event in `declaration_events` is deleted or mutated (the immutability trigger firing *and* being bypassed counts) |
| Availability breach | The platform is made unavailable in a way that prevents ANIF from accessing data they have a legal right to access within a compliance deadline |
| Key compromise | An Ed25519 signing key, HMAC secret, or OIDC credential is leaked |

The following do **not** qualify as breaches under Art. 4(12), though
they may still trigger incident procedures:

- A Prometheus alert firing that resolves without confirmed data exposure
- A failed authentication attempt (brute-force attempt without access)
- The outbox DLQ receiving a message (that is an operational event, not a breach)

### 1.1 Threshold for notification

**To the supervisory authority (Art. 33):** Within 72 hours of becoming
aware of a breach, unless the breach "is unlikely to result in a risk to
the rights and freedoms of natural persons." In RÉCOR's threat model, any
confirmed exposure of `declarant_principal` or `beneficial_owners` payload
is assessed as likely to create risk; the 72-hour clock applies.

**To data subjects (Art. 34):** When the breach is "likely to result in
a high risk to the rights and freedoms of natural persons." See § 5 for
the severity tiers and when each tier triggers subject notification.

---

## 2. Detection paths

### 2.1 Prometheus alerts

The following Prometheus alerts indicate potential breach conditions.
Each alert links to its runbook; this document is the breach-notification
overlay that runs alongside the service runbook.

| Alert | Signal | Potential breach class |
|---|---|---|
| `RecorUnauthorizedAccessRate > 0` | Sustained 403/401 on authenticated endpoints | Credential stuffing or token replay |
| `RecorPIILeakInLogs` (planned) | OPS-2 RedactingLayer reporting a missed redaction | Accidental PII in logs |
| `RecorImmutabilityViolationAttempt` | Trigger exception SQLSTATE `insufficient_privilege` on `declaration_events` | Integrity breach attempt |
| `RecorSanctionsPublicListingCacheLag > 86400` | Cache not invalidated after withdrawal | Wrongful public exposure |
| `RecorOutboxDLQDepthSpiking` | High DLQ depth may indicate message replay | Potential duplicate dispatch |
| `RecorVerificationCaseUnexpectedUpdate` | Any row change on `verification_cases` post-close | Integrity breach |

### 2.2 Log signals

Structured log events that indicate breach conditions (query with your
log aggregator using the field names below):

```
event_kind = "auth_failure" AND count > 100 within 5m  →  brute-force signal
event_kind = "pii_redaction_miss"                       →  OPS-2 gap
event_kind = "data_subject_access" AND principal NOT IN expected_principals  →  impersonation
audit_action = "immutability_trigger_fired"             →  integrity attempt
```

### 2.3 Third-party reports

- Vendor pen-test findings (per `docs/security/pen-test-rules-of-engagement.md` § 6,
  the vendor must report PII captured by accident within 4 hours)
- ANIF, CONAC, or other consumer reports of data they received that they
  should not have received
- External security researcher reports via `security@recor.cm`
  (provisioning tracked under OPS ticket TBD)
- BUNEC adapter vendor reports of anomalous query volumes against the
  identity register

---

## 3. 72-hour escalation procedure

### T+0: Detection and initial triage (0–30 min)

1. **Declare a breach-candidate incident.** Open an incident record at
   `docs/incidents/INC-YYYY-MMDD-NN.md` using the template in
   `docs/runbooks/incident-response-template.md`. Set `data_integrity`
   to `possibly-affected` as the default.

2. **Page the security-team lead** via the primary on-call channel.
   Do not page only the on-call engineer; breach response requires
   security-team lead authority from T+0.

3. **Isolate the affected surface.** Apply the minimum containment
   action that stops ongoing exposure without destroying evidence:
   - If a DB query path is involved: revoke the service role's SELECT
     on the affected table; restart the service in a maintenance mode.
   - If a credential is compromised: rotate the credential via
     `docs/runbooks/hmac-secret-rotation.md` or
     `docs/runbooks/vault-rotation.md`.
   - If a log aggregator received PII: quarantine the log stream; do
     not flush or delete (evidence preservation).

4. **Preserve evidence.** Export the relevant log window, Prometheus
   snapshot, and DB query log to an access-controlled location. Do not
   allow the log retention window to expire during the investigation.

### T+30 min: Breach assessment (30–120 min)

1. **Determine whether the event meets the Art. 4(12) definition.**
   The security-team lead makes this determination in consultation with
   counsel. Use the event-class table in § 1.

2. **Classify severity.** Apply the severity tiers in § 5. This
   determines whether subject notification is required (§ 6).

3. **Estimate scope.** Answer:
   - Which data subjects are affected? (list `person_id` or
     `declarant_principal` values where known)
   - What data categories were exposed? (Public / PII / Sensitive-PII
     per `data-classification.md`)
   - How many records?
   - How long was the exposure window?
   - Is the exposure ongoing or contained?

4. **Make the 72-hour commitment.** At T+120 min the security-team lead
   decides: "Will we notify the supervisory authority?" If the answer
   is "yes" or "possibly yes," file the precautionary notification
   (§ 4.1) within the 72-hour window regardless of whether the
   investigation is complete. Art. 33(4) explicitly permits a phased
   notification where not all information is available at T+72h.

### T+2h: Internal communications

1. **Brief consortium partners** as required by the consortium's
   information-sharing agreement. The security-team lead identifies
   which partners need to be notified.

2. **Brief the DPO.** The DPO is the authority for all external
    notifications. No notification leaves RÉCOR without DPO sign-off.

### T+24h: Notification preparation

1. **Draft the supervisory authority notification.** Use the template
    in § 4.1. The DPO reviews and approves.

2. **Draft the data-subject notification** if § 5 indicates it is
    required. Use the template in § 4.2.

### T+72h: Notification deadline

1. **File the supervisory authority notification.** The DPO transmits
    via the authority's secure filing channel. Record the filing
    reference number in the incident record.

2. **Send subject notifications** if required. Record the dispatch
    method and count in the incident record.

### T+72h to T+30d: Post-notification phase

1. **Continue the investigation.** If the full scope was not known
    at T+72h, file supplementary notifications as the picture
    clarifies (Art. 33(4) phased notification).

2. **Implement mitigations.** Every identified gap gets a ticket.
    The security-team lead owns the mitigation plan; the DPO confirms
    when the breach is contained.

3. **Post-mortem.** Within 3 business days of containment, complete
    the incident retrospective per
    `docs/runbooks/incident-retrospective-template.md`.

---

## 4. Notification templates

### 4.1 Supervisory authority notification (Art. 33)

File with: `[CITATION NEEDED: cameroon-supervisory-authority-filing-channel]`

The Art. 33 notification must contain (per Art. 33(3)):

```
NOTIFICATION OF PERSONAL DATA BREACH
GDPR Art. 33 / [CITATION NEEDED: cameroon-data-protection-law-art]

Controller: Consortium de Registre des Bénéficiaires Effectifs
           du Cameroun (RÉCOR)
Contact:    DPO — dpo@recor.cm
            security-team lead — [out-of-band contact; not committed to files]
Incident:   INC-YYYY-MMDD-NN
Filed:      YYYY-MM-DD HH:MM (Africa/Douala)

1. NATURE OF THE BREACH
   [Describe the event-class from § 1. Be specific:
   "Unauthorised read access to the verification_cases table
   via a compromised service credential" rather than "data breach."]

2. DATA SUBJECT CATEGORIES AND APPROXIMATE NUMBERS
   Categories: [declarants / beneficial owners / obliged-entity users]
   Approximate count: [number or range if exact count is unknown]
   Uncertainty: [state if this is a preliminary estimate]

3. DATA CATEGORIES AND APPROXIMATE RECORDS
   [List the classification tiers affected: Public / PII / Sensitive-PII.
   For PII and Sensitive-PII, list the specific fields involved
   (e.g. declarant_principal, beneficial_owners, canonical_full_name).]
   Approximate records affected: [count or range]

4. LIKELY CONSEQUENCES
   [Describe the likely harm to data subjects:
   identity fraud, reputational harm, financial harm, safety risk.
   Reference the risk-to-rights rows from docs/compliance/dpia.md § 5.]

5. MEASURES TAKEN OR PROPOSED
   Containment: [actions taken to stop ongoing exposure]
   Mitigation:  [actions taken to limit harm to affected subjects]
   Prevention:  [planned technical or organisational measures]

6. PHASED NOTIFICATION NOTICE (if applicable)
   This notification is preliminary. A supplementary notification
   will be filed by YYYY-MM-DD with the complete scope assessment.
   [Delete this section if the notification is complete.]

7. CONTACT FOR FURTHER INFORMATION
   DPO: dpo@recor.cm
   PGP key fingerprint: [fingerprint; out-of-band]
```

### 4.2 Data-subject notification (Art. 34)

Send via: email to the `declarant_principal` address on file (for
declarants); postal notice or portal notification for BO subjects
who do not have a direct email on file (coordinate with DPO).

```
Subject: RÉCOR — Notification of data breach affecting your information

Dear [data subject name or "Sir/Madam"],

We are writing to inform you that RÉCOR (the National Beneficial
Ownership Registry of Cameroon) has identified a personal data
breach that may affect your information.

What happened:
[One paragraph. Plain language. Do not use technical jargon.
"On [date], we discovered that..." Avoid admissions of liability
in the first draft; have counsel review before dispatch.]

What data was involved:
[List the specific fields that may have been exposed, in plain language.
"Your name and nationality as recorded in the BO register" rather than
"declarant_principal and beneficial_owners.canonical_full_name."]

What we are doing:
[Containment and mitigation actions already taken.
"We have rotated the compromised credential and revoked access
for the affected service account."]

What you can do:
[Practical advice. If the breach involved identity data:
"Monitor your credit report and identity documents for unusual activity."
If the breach involved an OIDC credential:
"Change your password and revoke active sessions at [IDP URL]."]

Your data-subject rights:
You have the right to lodge a complaint with the supervisory authority
at [authority contact — CITATION NEEDED].
You may also contact our DPO at dpo@recor.cm.

Reference:  INC-YYYY-MMDD-NN
Date:       YYYY-MM-DD

RÉCOR Data Protection Officer
dpo@recor.cm
```

---

## 5. Severity tiers and subject-notification trigger

| Tier | Criteria | Subject notification? | Notes |
|---|---|---|---|
| **Tier 1 — Low** | Breach of Internal or Confidential data only; no PII exposure; contained before any external party accessed the data | No | Record in incident log; post-mortem; no regulatory notification required |
| **Tier 2 — Medium** | PII exposure confirmed; low number of subjects (< 50); data unlikely to enable harm (e.g. only entity-level PII, no names or IDs); contained | Supervisory authority: Yes (Art. 33). Subjects: No (Art. 34(3)(b) exemption: unlikely to result in high risk) | Notify authority within 72h; document the basis for not notifying subjects |
| **Tier 3 — High** | PII exposure confirmed; > 50 subjects OR names/nationalities exposed OR the breach enables identity fraud or financial harm; contained or ongoing | Both: supervisory authority (Art. 33) and data subjects (Art. 34) | 72-hour authority deadline; subject notifications within 7 days of confirmation |
| **Tier 4 — Critical** | Sensitive-PII exposed (identity documents, biometric references); or the breach enables active harm (blackmail, physical safety risk); or an ongoing breach where containment is not yet achieved | Both, immediately. Do not wait for the 72h window to fully expire — notify as soon as the assessment is complete | Activate the consortium's crisis-communication plan in parallel |

---

## 6. Pre-vetted contact skeleton

The specific named contacts and their out-of-band credentials (phone
numbers, PGP keys) are **not committed to this file** (D18 — no secrets
in code or committed documents). They are maintained by the DPO and
the security-team lead in the secure operations runbook held out-of-band.

The skeleton below documents the role set; operators populate it at
onboarding:

| Role | Organisation | Purpose | How to reach |
|---|---|---|---|
| Supervisory authority — breach intake | `[CITATION NEEDED: cameroon-data-protection-authority]` | Art. 33 filing recipient | Filing channel documented out-of-band |
| ANIF security contact | ANIF | Notify if ANIF data was involved in the breach | Out-of-band; see secure ops runbook |
| CONAC security contact | CONAC | Notify if CONAC access credentials were involved | Out-of-band |
| INTERPOL / StAR liaison | INTERPOL | Notify if cross-border subject data was exposed | Out-of-band |
| DPO | RÉCOR | Authority for all external notifications | dpo@recor.cm (provisioning pending) |
| Security-team lead | RÉCOR | Incident authority; breach classification | Out-of-band; primary on-call channel |
| Counsel | RÉCOR | Legal sign-off on notifications | Out-of-band |
| Anthropic DPA contact | Anthropic | If Inference Gateway call logs were involved | Per Anthropic DPA — `[CITATION NEEDED: anthropic-dpa-ref]` |

---

## 7. Post-incident retrospective

After every Art. 33 notification (i.e., every Tier 2–4 breach):

1. Complete `docs/runbooks/incident-retrospective-template.md` within
   3 business days of containment.
2. Add a retrospective section that records:
   - Was the 72-hour deadline met? If not, why not, and what process
     change prevents recurrence?
   - Were the notification templates adequate? Were they updated?
   - Did the detection paths (§ 2) detect the breach promptly?
   - What controls failed or were absent?
3. Open tickets for every identified gap. Label them `breach-followup`.
4. The DPO files the retrospective as a supplementary notification
   with the supervisory authority if the authority requested one.

---

## References

- GDPR Art. 4(12), 33, 34 — breach definition and notification obligations
- `docs/compliance/dpia.md` (COMP-6) — risk-to-rights assessment
- `docs/compliance/data-classification.md` (COMP-3) — PII classification per field
- `docs/compliance/gdpr-procedures.md` (COMP-1) — data-subject rights procedures
- `docs/runbooks/incident-response-template.md` — incident log template
- `docs/runbooks/incident-retrospective-template.md` — retrospective template
- `docs/runbooks/hmac-secret-rotation.md` — credential rotation on breach
- `docs/runbooks/vault-rotation.md` — Vault credential rotation
- `docs/security/pen-test-rules-of-engagement.md` § 6 — vendor PII-capture reporting
- `docs/security/threat-model.md` (DOC-4) — STRIDE threat context
- Architecture V1 P2 — doctrines D14 (fail-closed), D15 (cryptographic provenance),
  D17 (zero trust), D18 (no secrets in committed files)
