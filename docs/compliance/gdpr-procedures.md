# GDPR / OHADA Data-Subject Rights — Operational Procedures

**Ticket:** COMP-1
**Status:** Draft — engineering complete, **pending AML/CFT counsel sign-off**.
**Owners:** RÉCOR domain team (engineering), [counsel name TBD] (legal).
**Last updated:** 2026-05-12.

> **Read this first.** Every line below tagged
> `[CITATION NEEDED: <slug>]` is a deliberate placeholder. Cameroonian
> AML/CFT counsel must annotate each placeholder with the actual
> legal-instrument citation before this document is treated as
> authoritative. Claude does not invent citations; engineering does
> not invent citations; only counsel with formal sign-off authority
> on this register may resolve the placeholders.

## Why this document exists

RÉCOR is the National Beneficial-Ownership Registry of Cameroon. The
register holds personally-identifying information (PII) on the natural
persons who ultimately own or control corporate entities incorporated
or operating in Cameroon. PII handling triggers obligations under:

- **Cameroon's national framework** for beneficial-ownership disclosure
  and AML/CFT supervision (legal-basis citations are reserved for
  counsel — see the placeholders below).
- **The OHADA Uniform Act on Commercial Companies (Acte uniforme révisé
  du 30 janvier 2014)** and the OHADA AML/CFT framework, which together
  govern company-information transparency and financial-intelligence
  obligations across the OHADA member states. `[CITATION NEEDED:
  ohada-aml-cft-art-X]`.
- **EU General Data Protection Regulation (GDPR), Regulation (EU)
  2016/679**, applicable to any data subject located in the EU at the
  time their data is processed (Article 3 territorial scope). The EU
  Court of Justice ruled in Case C-37/20 *WM and Sovim SA v.
  Luxembourg Business Registers* that unrestricted public access to
  beneficial-ownership data exceeds GDPR Article 5(1)(c) data
  minimisation; the register's downstream consumer-access design must
  reflect that judgment (see the Architecture Document V1 P11 §
  Consumer Access).

The procedures below translate those obligations into concrete
operational steps. Each procedure cites the implementing endpoint(s)
in this codebase and identifies the legal basis the platform asserts
for the action.

## The six data-subject rights

### 1. Right of access (GDPR Art. 15)

**What the data subject can ask for.** "Show me all data RÉCOR holds
about me."

**Implementation.** `GET /v1/declarations/by-principal`. The endpoint
returns every declaration where the authenticated principal is the
declarant. The principal is sourced exclusively from the authenticated
session (D17 zero trust) — no path parameter, no query string, no
request body. The response payload is JSON in the shape:

```json
{
  "principal": "<authenticated-subject>",
  "count": <int>,
  "declarations": [
    {
      "declaration_id": "...",
      "entity_id": "...",
      "declarant_principal": "<authenticated-subject>",
      "declarant_role": "...",
      "kind": "...",
      "effective_from": "YYYY-MM-DD",
      "beneficial_owners": [...],
      "state": "...",
      "aggregate_version": <int>,
      "submitted_at": "...",
      "receipt_hash_hex": "...",
      "correlation_id": "...",
      "verification_state": "...",
      "...": "..."
    }
  ]
}
```

Each row carries its `receipt_hash_hex`, a BLAKE3-256 hash over the
canonical receipt bytes. The declarant can re-verify each receipt
offline against the canonical bytes they originally signed; this is
the D15 cryptographic-provenance guarantee.

**Authentication.** OIDC bearer token (production) or
`X-Recor-Dev-Principal` header (dev only — refused outside
`ENVIRONMENT=dev`). The endpoint is unreachable to an anonymous
caller; it returns 401 Unauthorized.

**Audit.** The handler emits a `tracing::info!` event with
`event_kind = "data_subject_access"`. The OPS-2 RedactingLayer
suppresses the raw principal value in the rendered log line; the event
itself is the audit record that someone exercised their access right
at this time, the registry's audit chain captures who exercised which
right and when.

**Response time.** Synchronous — the endpoint returns within the
service's `GET` SLO (p99 < 50 ms for a typical principal with ≤ 100
records). GDPR Art. 12(3) gives one month for response; the
self-service endpoint satisfies that obligation immediately and
without manual intervention.

**Legal basis.** The right of access is unrestricted under GDPR Art.
15. There is no AML/CFT carve-out that overrides it; the register
must show the data subject what is held under their principal.

**Limitations.** Today the match is strictly by `declarant_principal`.
A future identity-linkage ticket (TBD; mapped to the planned
person-registry service) will broaden the result set to include rows
where the principal appears as a beneficial owner via their
linked `person_id`. Until that lands, declarants whose data appears
*only* as a beneficial owner naming someone *else* as the declarant
must rely on procedure 1.b below.

#### 1.b Right of access — beneficial-owner indirect path

Where a data subject is named as a beneficial owner on a declaration
submitted by another principal (e.g. a corporate agent declared on
their behalf), the data subject can submit a written request to the
RÉCOR data-protection officer at `dpo@recor.cm` (mailbox provisioning
tracked under OPS ticket TBD). The DPO uses internal-only tooling
to enumerate every declaration naming the subject's `person_id` and
delivers the data in the same JSON shape as procedure 1.

**Verification.** The DPO requires proof of identity matching the
`person_id` (national identity document plus a freshly-signed
challenge using the key bound to the person's declared identity).

**Response time.** Within one month per GDPR Art. 12(3); within two
weeks in practice for the first-launch period.

### 2. Right of rectification (GDPR Art. 16)

**What the data subject can ask for.** "Some of the data RÉCOR holds
about me is wrong; fix it."

**Implementation.** Two existing endpoints together cover
rectification:

- **`POST /v1/declarations/{id}/amend` (R-DECL-3-AMEND).** The
  declarant submits a fresh declaration body with the corrected
  beneficial-owner set, effective date, or role. Aggregate state
  transitions Submitted → Submitted (or InVerification → InVerification)
  with a new `aggregate_version`; an `amended_at` timestamp records
  when the rectification was applied. The amendment is itself
  cryptographically attested (Ed25519); the audit chain captures the
  before/after.
- **`POST /v1/declarations/{id}/correct` (R-DECL-3-CORRECT).** Used
  for metadata-only fixes (the canonical declaration body is unchanged;
  only the `metadata_notes` annotation changes). Restricted to the
  pre-verification window.

The data subject sees the corrected data immediately via procedure 1.
Prior versions remain in the event log — the audit chain MUST NOT be
mutated. This is a deliberate architectural choice: the register's
integrity guarantees depend on the immutability of the event log; what
"rectification" means here is "a new corrected event appears, and the
projection reflects the corrected current state."

**Where the data subject is not the declarant.** They must contact the
declarant (typically the corporate agent or operator-assisted
intake) or, if that fails, the DPO. The DPO has the authority to
issue a correction event on behalf of the data subject when the
declarant refuses to act and counsel confirms the rectification claim.

**Legal basis.** GDPR Art. 16 is unrestricted; the OHADA framework
also requires the register to reflect current beneficial ownership
accurately.

### 3. Right of erasure (GDPR Art. 17) — partial only

**What the data subject can ask for.** "Delete my data from your
register."

**Implementation.** The register **cannot honour a request for full
erasure**. Two reasons:

1. **Statutory retention obligation.** AML/CFT registries are exempt
   from full erasure under the OHADA framework — the registers exist
   precisely to maintain a long-lived auditable record of beneficial
   ownership against which financial-intelligence units, tax
   authorities, and procurement regulators run lookups.
   `[CITATION NEEDED: ohada-aml-cft-art-X]`. The Cameroonian
   implementing instruments are expected to mirror this position;
   counsel sign-off required. `[CITATION NEEDED: cameroon-bo-decree-art-Y]`.
2. **Cryptographic integrity.** Every declaration is event-sourced;
   the event log is append-only at the database level (CHECK + REVOKE
   on `declaration_events`, see `services/declaration/migrations/0001_initial.sql`
   and the COMP-2 lockdown migration), and each event's hash is
   anchored downstream to the Hyperledger Fabric audit channel (see
   R-DECL-9 Fabric anchoring). Deleting a row would invalidate the
   Merkle anchor; we treat that as forbidden by design.

#### Partial-erasure procedure (the actual operational answer)

What the register *can* do:

1. **Redact the PII fields in the projection** — the `declarations`
   table's `attestation`, `beneficial_owners` (JSONB), and any other
   PII-classified columns are overwritten with redaction sentinels.
   The redaction is a UPDATE, not a DELETE; the row continues to
   exist so the audit hash chain remains valid.
2. **Retain the event log unchanged.** The `declaration_events` rows
   that carry the original payload remain. They are
   database-level-protected against UPDATE/DELETE (COMP-2). The
   event payload is encrypted at rest (DOC-4 threat model § E1);
   access to the encrypted-tier records is logged.
3. **Anchor the redaction event.** A new `declaration.redacted.v1`
   event is appended (event-type slot is reserved; the
   implementation lands under the partial-erasure follow-up ticket
   noted in the issues backlog) so the audit chain captures that the
   redaction was applied at a specific time, by a specific operator,
   under a specific legal authority.

**Who can authorise a partial erasure.** Only the DPO, acting on
written legal authority (court order, counsel opinion, or the data
subject's own request when the AML/CFT retention period for that
declaration has elapsed). The retention period is documented in
`docs/compliance/data-retention.md` (COMP-2; pending).
`[CITATION NEEDED: ohada-aml-cft-retention-period]`.

**Where the right cannot be honoured at all.** When the
beneficial-ownership data is the subject of an active investigation
by ANIF (Cameroon's financial intelligence unit) or CONAC (the
anti-corruption commission), the register refuses erasure outright
under the litigation-hold exemption (GDPR Art. 17(3)(e) carve-out for
legal claims; OHADA AML/CFT investigative-hold provisions).
`[CITATION NEEDED: ohada-aml-cft-investigative-hold]`.

### 4. Right to data portability (GDPR Art. 20)

**What the data subject can ask for.** "Give me my data in a
structured, commonly used, machine-readable format so I can transfer
it elsewhere."

**Implementation.** The same `GET /v1/declarations/by-principal`
endpoint as the right of access. The response is structured JSON
following the OpenAPI specification at
`docs/openapi/declaration.json`; the schema is stable, versioned, and
documented. JSON is the commonly-used machine-readable format in this
context — the Architecture Document standardises JSON over the wire,
and the register's downstream consumers (BEAC, ARMP, ANIF) consume
the same shape.

**Authentication, audit, response time.** Identical to procedure 1.

**Legal basis.** GDPR Art. 20 applies only to data processed on the
basis of consent or contract. The register's processing basis is
statutory obligation (Cameroon AML/CFT law), so Art. 20 does not
*compel* portability — but the platform extends it voluntarily because
the same endpoint serves the right of access and offering portability
costs us nothing. `[CITATION NEEDED: cameroon-aml-cft-statutory-basis]`.

### 5. Right to restrict processing (GDPR Art. 18)

**What the data subject can ask for.** "Stop processing my data while
we work out whether the data is accurate / whether you have a basis to
process it."

**Implementation.** Limited applicability. The register's processing
basis is statutory (AML/CFT supervision, not consent); the register
*cannot* suspend the legal duty to hold beneficial-ownership records
on the request of an affected data subject. What the register *can*
suspend is:

- **Downstream consumer access.** A restriction request triggers a
  flag on the declaration; verification cases continue, but the
  consumer-access surface (e.g. the public-record portal) suppresses
  the declarant's records until the dispute is resolved.
- **Verification re-runs.** New verification stages are paused while
  the dispute is open; the current verification state is frozen.

**Implementation status.** This flag is not yet implemented; it is a
follow-up ticket. Until it lands, restriction requests are handled
manually by the DPO via internal-only tooling. `[CITATION NEEDED:
ohada-aml-cft-supervisory-duty]`.

**Legal basis for limited applicability.** GDPR Art. 23(1)(d) and (e)
allow restrictions on Art. 18 where necessary for the prevention,
investigation, detection, or prosecution of criminal offences and for
other monitoring functions connected with the exercise of official
authority. The Cameroonian AML/CFT regime asserts that authority.
`[CITATION NEEDED: ohada-aml-cft-art-X-supervisory-restriction]`.

### 6. Right to object (GDPR Art. 21)

**What the data subject can ask for.** "I object to your processing of
my data."

**Implementation.** Limited applicability. Art. 21 applies to
processing on the basis of legitimate interests (Art. 6(1)(f)) or
public interest (Art. 6(1)(e)). The register's processing basis is
statutory obligation (Art. 6(1)(c)) — *the law compels the
processing*, so Art. 21 does not authorise the data subject to stop
the processing.

The register documents this position and informs the data subject of
the statutory basis in its privacy notice (notice text is delivered
via the Declarant Portal; see DOC-PRIVACY-NOTICE follow-up).
Objections are logged and routed to the DPO for review; if the
objection identifies a legal defect in the asserted statutory basis,
the DPO escalates to counsel.

**No automated decision-making with legal effect (GDPR Art. 22).**
The verification engine produces lane decisions (green / yellow / red)
that feed the platform's case-management workflows; *no individual
authoritative outcome* is determined solely by automated processing.
The lane is an advisory signal to human reviewers under ARMP / ANIF /
DGI / BEAC / CONAC; their human decision is the legally-binding act.
The register exposes the lane and the Dempster-Shafer fusion inputs to
those reviewers so they can audit the recommendation.

`[CITATION NEEDED: cameroon-administrative-procedure-art-Z]`.

## Operational checklists

### Declarant self-service (procedures 1 and 4)

The declarant can:

1. Open the Declarant Portal at `https://portal.recor.cm` (URL
   provisional).
2. Authenticate with their OIDC identity provider.
3. Click "My declarations" to invoke `GET /v1/declarations/by-principal`.
4. Download the JSON via the portal's "Export" button.

The portal MUST display, alongside the export button, the explanation
of which six rights are covered by the export and which require an
email to the DPO. (Portal copy is owned by the docs-author agent;
landed under the same compliance review cycle as this document.)

### DPO procedures (procedures 1.b, 2 indirect, 3, 5, 6)

For every request that reaches the DPO:

1. **Verify the requester's identity.** Confirm the requester's
   national identity document plus a freshly-signed challenge using
   the key bound to the person's declared identity. A request with
   no verifiable identity is refused (fail-closed; D14).
2. **Determine the right(s) invoked.** Map the request to one or
   more of the six rights above.
3. **Confirm the legal basis** for honouring or refusing the right
   per the procedure above. Consult counsel when the response is
   refusal.
4. **Execute** via the relevant endpoint or via internal-only tooling.
   Every action MUST emit a `data_subject_*` event so the audit chain
   captures the operation.
5. **Notify the requester** in writing, with a clear explanation of
   the action taken or the reason for refusal. The notification MUST
   reference the legal basis cited above. Reference number is the
   `correlation_id` of the recorded audit event.
6. **Within the time bound.** One month per GDPR Art. 12(3); two
   weeks target for the first-launch period.

### Engineering procedures (when implementation surface changes)

Whenever a new endpoint, table, or projection is added that affects
the data RÉCOR holds:

1. **Update the right-of-access shape.** If the new field is
   PII-classified, confirm it's included in the `GET
   /v1/declarations/by-principal` response so the right of access
   remains complete (D01 completeness; D05 documentation is part of
   the feature).
2. **Update the redaction set.** If the new field is PII-classified,
   confirm OPS-2's `RedactingLayer` redacts it from logs.
3. **Update the partial-erasure surface.** If the new field is
   PII-classified and persisted, confirm the partial-erasure UPDATE
   covers it.
4. **Update this document.** The procedures above must continue to
   reflect the implementation.

## Cross-references

- **OPS-2 PII redaction** — `packages/recor-logging/src/lib.rs` — the
  log-line redaction layer that protects against PII leaking into
  operational logs.
- **R-DECL-3-AMEND, R-DECL-3-CORRECT** — the rectification endpoints.
- **R-DECL-9 Fabric anchoring** — the audit-chain immutability layer
  that constrains erasure.
- **COMP-2 Audit log immutability + retention policy** — locks down
  `declaration_events` against UPDATE/DELETE at the database level;
  documents the retention schedule.
- **COMP-3 Data classification** — the per-column PII inventory the
  procedures above depend on.
- **DOC-4 Threat model** — `docs/security/threat-model.md` — the
  reference for "what counts as PII" and the corresponding handling
  controls.
- **ADR-0001** — `docs/adr/0001-event-sourcing-declaration-aggregate.md`
  — the architectural decision that makes the event log append-only.
- **ADR-0004** — `docs/adr/0004-oidc-jwks-principal-authentication.md`
  — the principal-source-of-truth decision that the right-of-access
  endpoint depends on.

## Outstanding actions before this document is authoritative

1. **AML/CFT counsel sign-off** on each `[CITATION NEEDED: ...]`
   placeholder. The placeholders are intentionally non-resolvable
   without counsel.
2. **DPO mailbox provisioning** (`dpo@recor.cm`) and DPO appointment.
3. **Portal copy alignment** — the Declarant Portal must surface the
   six rights and the self-service / DPO routing in user-facing text.
4. **Restriction-flag implementation** — the partial-restriction flag
   discussed in procedure 5 is a follow-up ticket.
5. **Identity-linkage ticket** — the mapping from authenticated
   principal to one or more `person_id` values, which extends the
   right-of-access surface to the beneficial-owner path (procedure 1.b).
6. **Redaction event type** — `declaration.redacted.v1` event-type
   slot and its handler, to support the partial-erasure procedure
   under procedure 3.

These items are tracked in the Phase 4 — Compliance backlog;
`docs/PRODUCTION-TODO.md` is the source of truth for ticket numbers.
