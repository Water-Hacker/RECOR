# Regulatory mapping — RÉCOR platform

**Ticket:** COMP-4
**Status:** Draft — engineering complete, **pending AML/CFT counsel sign-off**.
**Owners:** RÉCOR domain team (engineering), [counsel name TBD] (legal).
**Last updated:** 2026-05-12.

> **Read this first.** Every line below tagged
> `[CITATION NEEDED: <slug>]` is a deliberate placeholder. Cameroonian
> AML/CFT counsel must annotate each placeholder with the actual
> legal-instrument citation (specific article, paragraph, page) before
> this document is treated as authoritative. Per doctrine D07 ("no
> workarounds where the real fix exists") and the COMP-4 brief's
> explicit instruction — "Get the citations from AML/CFT counsel —
> don't invent them" — engineering does not resolve these placeholders.
> Resolution is reserved to counsel with formal sign-off authority on
> this register.

## Why this document exists

A national beneficial-ownership registry is statutory infrastructure.
Every endpoint, every persisted column, every domain invariant exists
because some legal instrument compels it (or, in a smaller number of
cases, because a legal instrument forbids the alternative). When a
regulator audits the platform, the auditor's first question is "which
provision compels this behaviour?" — and the platform's defensibility
depends on having a specific, citable answer in every case.

This file is that answer. It maps the platform's public surface
(REST endpoints, gRPC endpoints) and its load-bearing domain
invariants to the legal instruments that govern them.

Where the legal instrument's specific article is not yet confirmed,
the row carries a `[CITATION NEEDED: <slug>]` placeholder. The
placeholders are tracked in a single per-slug table at the end of
this document so counsel can resolve them in one pass.

## Authority hierarchy

Where two instruments speak to the same control, the resolution order
is:

1. Cameroonian national instruments (the statute that creates the
   registry).
2. OHADA Uniform Acts (regional harmonised commercial law, directly
   applicable in Cameroon).
3. International standards Cameroon has formally adopted (FATF
   Recommendations via the Egmont/GIABA channel; UN Convention against
   Corruption; the African Union Convention on Preventing and Combating
   Corruption).
4. The EU GDPR (applies only when the data subject is located in the
   EU at the time of processing; territorial scope per GDPR Art. 3).

Where Cameroonian or OHADA AML/CFT obligations conflict with GDPR
data-subject rights, the AML/CFT regime prevails for the registry's
core purpose; the GDPR derogations in Art. 6(1)(c) and Art. 23(1)(d)/(e)
authorise this. See `docs/compliance/gdpr-procedures.md` for the
operational consequences (right of erasure is only partial; right to
object is limited; etc.).

## Legal-basis catalogue

The instruments cited throughout this document. Every row in the
endpoint and invariant tables below references one or more of these
by short slug.

| Slug | Instrument | What it does | Status |
|---|---|---|---|
| `CM-LAW-2014-007` | **Loi N° 2014/007 du 23 avril 2014 portant fixation des incriminations et des sanctions pénales pour la transparence du secteur extractif** (Cameroon) | Criminal-law transparency obligations for the extractive sector; the load-bearing national instrument for beneficial-ownership disclosure in Cameroon's resource sectors. | Cited from the COMP-4 brief; specific articles `[CITATION NEEDED: cm-law-2014-007-art-N]` per row below. |
| `CM-BO-DECREE` | **Décret fixant les seuils et modalités de déclaration des bénéficiaires effectifs** (Cameroon — the implementing decree applying the threshold "≥ 25% direct or indirect ownership / control" rule to legal persons incorporated or operating in Cameroon) | Defines the 25% beneficial-ownership threshold; basis for the platform's `ownership_basis_points` invariant. | `[CITATION NEEDED: cameroon-bo-decree-citation]` — counsel to confirm the decree reference and article. |
| `OHADA-AUSCGIE-2014` | **Acte uniforme révisé du 30 janvier 2014 relatif au droit des sociétés commerciales et du groupement d'intérêt économique** (OHADA) | Harmonised company law across the OHADA member states; basis for the registry's commercial-transparency obligations on entities incorporated or operating in Cameroon. | `[CITATION NEEDED: ohada-auscgie-2014-art-N]` per row below. |
| `OHADA-AML-CFT` | **OHADA AML/CFT framework** — the Uniform Act and associated regulations governing anti-money-laundering and counter-financing-of-terrorism obligations across the OHADA states | Audit-trail, retention, investigative-hold, and reporting obligations on beneficial-ownership data. | Article citations reserved to counsel. `[CITATION NEEDED: ohada-aml-cft-art-N]` per row below. |
| `FATF-R24` | **FATF Recommendation 24 — Transparency and beneficial ownership of legal persons** (2003, revised 2012 and 2022) | International standard on beneficial-ownership transparency; Cameroon's commitments arise through GIABA membership. Informs the registry's core scope: identifying natural persons who ultimately own or control legal entities. | Public standard; the platform implements R24's INR 24 interpretive note. |
| `FATF-R25` | **FATF Recommendation 25 — Transparency and beneficial ownership of legal arrangements** (2022 revision) | Companion to R24 for trusts and other legal arrangements. Currently out of v1 scope (legal persons only); referenced for the roadmap. | Public standard. |
| `GDPR-EU-2016-679` | **Regulation (EU) 2016/679 — General Data Protection Regulation** | Data-protection framework; applies to data subjects located in the EU at the time of processing (Art. 3). Articles relevant to the registry: Art. 5 (principles), Art. 6 (lawfulness — the registry's basis is Art. 6(1)(c) legal obligation), Art. 12-22 (data-subject rights, covered in `gdpr-procedures.md`), Art. 23 (restrictions), Art. 32 (security of processing). | Public regulation; the relevant articles are cited inline. |
| `CJEU-C-37-20` | **Case C-37/20 *WM and Sovim SA v. Luxembourg Business Registers* (CJEU, 22 November 2022)** | Ruled that unrestricted public access to beneficial-ownership data exceeds GDPR Art. 5(1)(c) data minimisation. Constrains the registry's public-portal design. | Public judgment; binding interpretation of GDPR for the EU access path. |
| `UN-UNCAC` | **United Nations Convention against Corruption (UNCAC, 2003)** | Cameroon is a State Party (ratified 6 February 2006). Articles 12 (private sector), 14 (AML measures), 52 (prevention and detection of transfers of proceeds of crime) underpin the registry's accountability and information-sharing obligations. | Ratified. `[CITATION NEEDED: uncac-art-N]` per row below where invoked. |
| `AU-AUCPCC` | **African Union Convention on Preventing and Combating Corruption (Maputo, 2003)** | Cameroon is a State Party (ratified). Article 7 (corruption in the public service) and Article 12 (civil society and media) inform the registry's transparency posture. | Ratified. `[CITATION NEEDED: au-aucpcc-art-N]` per row below. |
| `CM-DATA-PROTECTION` | **Cameroon's data-protection framework** — the national instruments governing personal-data processing and electronic-communications privacy | Domestic counterpart to GDPR; the registry's PII handling is bound by these instruments for data subjects in Cameroon. | `[CITATION NEEDED: cameroon-data-protection-citation]` — counsel to confirm the controlling instrument(s). |

## Endpoint → legal-provision map

This table covers every public REST endpoint advertised in
`docs/openapi/declaration.json` and every gRPC RPC declared in
`contracts/declaration.proto`. The gRPC RPCs are shaded as such with
the suffix `[gRPC]` and the corresponding service / method name; their
canonical-payload bytes are byte-parity with REST (see D15 in the
Declaration service CLAUDE.md), so the legal basis is identical.

The verification-engine REST surface is included for completeness;
its endpoints inherit the same regulatory framing because the
verification outcome's authoritative status is what makes the
platform load-bearing.

Endpoints under `internal` are internal-only (operator / service-to-
service) and do not face external consumers; they are listed because
they still process regulated data and counsel will review them on the
same basis.

### Declaration service — REST

| Endpoint | Method | Legal basis (citation) | Source-of-citation | Notes |
|---|---|---|---|---|
| `/healthz` | GET | None — operational liveness probe; not regulated. | n/a | Returns `{"status":"ok"}`. No PII, no business logic. |
| `/readyz` | GET | None — operational readiness probe; not regulated. | n/a | Returns service-and-dependency state. |
| `/v1/declarations` | POST | `CM-LAW-2014-007` (declaration obligation in the extractive sector — extends by the implementing decree to all entities subject to BO registration); `OHADA-AUSCGIE-2014` (commercial-transparency obligation); `FATF-R24` para. INR 24.4 (mechanisms to ensure accurate and up-to-date BO information). | `[CITATION NEEDED: cm-law-2014-007-art-declaration]`; `[CITATION NEEDED: ohada-auscgie-2014-art-BO-disclosure]`. | Primary intake. Honours `Idempotency-Key` (financial-regulatory replay-safety, see invariant `INV-IDEMPOTENCY`). Idempotent-replay returns 200; first-write returns 201. Captures the Ed25519 attestation (D15 cryptographic provenance) — the load-bearing artefact for `OHADA-AML-CFT` audit-trail obligations. |
| `/v1/declarations/by-principal` | GET | `GDPR-EU-2016-679` Art. 15 (right of access); `GDPR-EU-2016-679` Art. 20 (right to data portability, voluntary — see `gdpr-procedures.md` § 4); `CM-DATA-PROTECTION` (national right-of-access mirror). | GDPR — Art. 15 and Art. 20 (public text); `[CITATION NEEDED: cameroon-data-protection-right-of-access]`. | Sources principal from the authenticated session (D17); no request-side identifier. Each returned row carries `receipt_hash_hex` for offline re-verification (D15). See `docs/compliance/gdpr-procedures.md` § 1 and § 4. |
| `/v1/declarations/{declaration_id}` | GET | `OHADA-AML-CFT` (audit-trail access for the declarant and authorised consumers); `CJEU-C-37-20` (constrains public-portal access — this endpoint is authenticated, NOT public). | `[CITATION NEEDED: ohada-aml-cft-audit-trail-access]`; CJEU public judgment. | Per-record projection. Authentication required; 403 if the caller is not the declarant. Public-record consumer access is a separate, lane-gated surface (out of scope for this endpoint; see V4 P11 § Consumer Access). |
| `/v1/declarations/{declaration_id}/amend` | POST | `OHADA-AUSCGIE-2014` (obligation to keep BO data current); `OHADA-AML-CFT` (declaration must reflect current BO accurately); `GDPR-EU-2016-679` Art. 16 (right of rectification, where the data subject *is* the declarant). | `[CITATION NEEDED: ohada-auscgie-2014-art-update]`; `[CITATION NEEDED: ohada-aml-cft-art-currency]`; GDPR Art. 16 public text. | Pre-verification (Submitted) or in-flight (InVerification) only; Accepted declarations require Supersede. Honours `INV-IDEMPOTENCY` and `INV-ATTESTATION`. Captured as a `declaration.amended.v1` event with `before`/`after` snapshots (audit-trail completeness). |
| `/v1/declarations/{declaration_id}/correct` | POST | `OHADA-AML-CFT` (audit-trail integrity — metadata corrections recorded as separate events, never destructive overwrites); `CM-LAW-2014-007` (consistent with the statutory obligation to maintain accurate filings). | `[CITATION NEEDED: ohada-aml-cft-art-correction-trail]`; `[CITATION NEEDED: cm-law-2014-007-art-accurate-filings]`. | Pre-verification only (Submitted state). Touches only metadata (`metadata_notes`); the canonical declaration body is untouched. Captured as `declaration.corrected.v1`. |
| `/v1/declarations/{declaration_id}/supersede` | POST | `OHADA-AUSCGIE-2014` (change-of-control disclosure); `CM-LAW-2014-007` (change-of-control disclosure for extractive entities); `FATF-R24` INR 24.4 (timely updates). | `[CITATION NEEDED: ohada-auscgie-2014-art-change-of-control]`; `[CITATION NEEDED: cm-law-2014-007-art-change-of-control]`. | Replaces an Accepted or InVerification declaration with a freshly-attested successor. Linear chain — each declaration may be superseded at most once. Captured as `declaration.superseded.v1`. |
| `/v1/internal/outbox-dlq` | GET | `OHADA-AML-CFT` (operational integrity of audit-relay channels — operators must be able to inspect failed deliveries); `GDPR-EU-2016-679` Art. 32 (security of processing — DLQ visibility is a control). | `[CITATION NEEDED: ohada-aml-cft-art-operational-integrity]`; GDPR Art. 32 public text. | Admin allowlist (`Config::admin_principals_list()`); empty allowlist → 503. Internal-only. |
| `/v1/internal/outbox-dlq/{id}/replay` | POST | `OHADA-AML-CFT` (operational integrity — failed deliveries must be replayable to preserve the audit trail); `GDPR-EU-2016-679` Art. 32. | `[CITATION NEEDED: ohada-aml-cft-art-operational-integrity]`; GDPR Art. 32 public text. | Admin allowlist as above. Moves a DLQ row back to the outbox; idempotency preserved by the outbox relay's own replay-detection. |
| `/v1/internal/verification-outcomes` | POST | `OHADA-AML-CFT` (cross-service audit-trail relay — the verification engine's outcome is a regulated event and must be persisted alongside the declaration's lifecycle); `CM-LAW-2014-007` (verification is the statutory mechanism by which the declaration is adjudicated). | `[CITATION NEEDED: ohada-aml-cft-art-cross-service-relay]`; `[CITATION NEEDED: cm-law-2014-007-art-verification-mechanism]`. | HMAC-authenticated (shared writeback secret, dual-secret rotation per ADR-005); 503 if no secret configured. Idempotency on `case_id` (replay-safe). |

### Declaration service — gRPC

The gRPC surface (service `recor.declaration.v1.DeclarationService` in
`contracts/declaration.proto`) mirrors the REST surface RPC-for-RPC.
Every RPC inherits the corresponding REST endpoint's legal basis
unchanged; the canonical-payload bytes are byte-parity, so a single
attestation is valid under either transport (D15).

| RPC | REST counterpart | Legal basis | Source | Notes |
|---|---|---|---|---|
| `DeclarationService.SubmitDeclaration` `[gRPC]` | `POST /v1/declarations` | Inherits from REST counterpart. | See above. | Server-side principal resolution via the SAME OIDC verifier as REST (D17). |
| `DeclarationService.GetDeclaration` `[gRPC]` | `GET /v1/declarations/{declaration_id}` | Inherits from REST counterpart. | See above. | Same per-record projection. |
| `DeclarationService.SupersedeDeclaration` `[gRPC]` | `POST /v1/declarations/{declaration_id}/supersede` | Inherits from REST counterpart. | See above. | Same supersede chain semantics. |
| `DeclarationService.AmendDeclaration` `[gRPC]` | `POST /v1/declarations/{declaration_id}/amend` | Inherits from REST counterpart. | See above. | Same amendable-fields set; same before/after snapshots. |
| `DeclarationService.CorrectDeclaration` `[gRPC]` | `POST /v1/declarations/{declaration_id}/correct` | Inherits from REST counterpart. | See above. | Same pre-verification restriction. |

The Declaration service does NOT expose a gRPC equivalent of:

- `/v1/declarations/by-principal` — the GDPR data-subject access path
  is REST-only by design; the consumer is the data subject through the
  Declarant Portal (`applications/declarant-portal/`), not a
  service-to-service caller.
- `/v1/internal/outbox-dlq` and `/v1/internal/outbox-dlq/{id}/replay` —
  operator surface; REST is the canonical transport.
- `/v1/internal/verification-outcomes` — REST + HMAC is the canonical
  writeback channel; the verification engine writes here from its
  outbox relay.

### Verification engine — REST

The verification engine's public surface is small. Its `/v1/internal/declaration-events`
endpoint is the inbound writeback channel that mirrors the Declaration
service's `/v1/internal/verification-outcomes` — same HMAC dual-secret
rotation, same idempotency on event_id.

| Endpoint | Method | Legal basis (citation) | Source-of-citation | Notes |
|---|---|---|---|---|
| `/healthz` | GET | None — operational liveness probe. | n/a | |
| `/readyz` | GET | None — operational readiness probe. | n/a | |
| `/v1/verifications` | POST | `CM-LAW-2014-007` (verification is the statutory adjudication mechanism for the declaration); `OHADA-AML-CFT` (verification produces the audited disposition record); `FATF-R24` INR 24.4 (mechanisms to ensure accurate BO information). | `[CITATION NEEDED: cm-law-2014-007-art-adjudication]`; `[CITATION NEEDED: ohada-aml-cft-art-disposition]`. | Submits a declaration snapshot to the nine-stage pipeline; returns a lane (green / yellow / red) plus Dempster-Shafer fusion outputs. |
| `/v1/verifications/{case_id}` | GET | `OHADA-AML-CFT` (case-level audit-trail access for authorised consumers); `CM-LAW-2014-007` (transparency to the declarant and to oversight bodies). | `[CITATION NEEDED: ohada-aml-cft-case-access]`. | Returns the full case projection: stage decisions, fused beliefs, lane, completion timestamp. |
| `/v1/internal/verification-outbox-dlq` | GET | `OHADA-AML-CFT` (operational integrity of audit-relay channels). | `[CITATION NEEDED: ohada-aml-cft-art-operational-integrity]`. | Admin allowlist; mirrors the Declaration service DLQ admin surface. |
| `/v1/internal/verification-outbox-dlq/{id}/replay` | POST | `OHADA-AML-CFT` (operational integrity). | `[CITATION NEEDED: ohada-aml-cft-art-operational-integrity]`. | Admin allowlist; replay semantics mirror Declaration. |
| `/v1/internal/declaration-events` | POST | `OHADA-AML-CFT` (cross-service audit-trail relay). | `[CITATION NEEDED: ohada-aml-cft-art-cross-service-relay]`. | HMAC-authenticated webhook from the Declaration service's outbox relay. |

The verification engine does not yet expose a gRPC surface;
`R-VER-GRPC` is the follow-up ticket. When it lands, every RPC will
inherit the corresponding REST endpoint's legal basis on the same
parity principle as the Declaration service.

## Invariants → legal-provision map

Domain invariants enforced by the platform are not engineering
preferences — each one exists because some legal instrument compels
the behaviour. The table below covers the load-bearing invariants and
the legal basis for each.

The Declaration aggregate's full invariant set lives in
`services/declaration/src/domain/aggregate.rs` (see the module-level
docstring); the rows below select the invariants whose legal basis is
load-bearing.

| Invariant ID | Description | Where it lives | Legal basis (citation) | Source-of-citation | Notes |
|---|---|---|---|---|---|
| `INV-OWNERSHIP-SUM` | Sum of `ownership_basis_points` across all beneficial owners equals exactly `10_000` (i.e. 100.00%). Basis points are integers (1/100 of a percent) — there is no floating-point arithmetic. | `aggregate.rs::validate_beneficial_owners()`; tested in `aggregate.rs::tests::ownership_sum_must_equal_10000`. | `CM-BO-DECREE` (the implementing decree fixes the beneficial-ownership threshold and requires full disclosure of the ownership chain summing to 100%); `OHADA-AUSCGIE-2014` (commercial-transparency obligation extends to ownership-share completeness); `FATF-R24` INR 24.4 (BO records must be adequate, accurate, and current). | `[CITATION NEEDED: cameroon-bo-decree-art-threshold-and-completeness]`; `[CITATION NEEDED: ohada-auscgie-2014-art-ownership-completeness]`; FATF R24 public text. | The 25%-threshold rule on individual owners is enforced by the verification engine's pattern stage and surfaces on Yellow / Red lanes — NOT enforced by this aggregate-level invariant. This invariant is the *completeness* check (every share is declared), not the *threshold* check. |
| `INV-NO-DUPLICATE-OWNERS` | No two beneficial-owner claims on the same declaration may share a `person_id`. | `aggregate.rs::validate_beneficial_owners()`; tested in `aggregate.rs::tests::duplicate_owner_rejects`. | `OHADA-AUSCGIE-2014` (BO data must be unambiguous); `FATF-R24` INR 24.4 (accurate records). | `[CITATION NEEDED: ohada-auscgie-2014-art-unambiguous-bo]`; FATF R24 public text. | A duplicate would make ownership-share aggregation ambiguous and break downstream lookup keys. |
| `INV-NON-EMPTY-OWNERS` | A declaration must declare at least one beneficial owner. | `aggregate.rs::validate_beneficial_owners()`; tested in `aggregate.rs::tests::no_beneficial_owners_rejects`. | `CM-LAW-2014-007` (declaration obligation requires identifying at least one beneficiary); `OHADA-AUSCGIE-2014`. | `[CITATION NEEDED: cm-law-2014-007-art-beneficiary-presence]`. | A zero-owner declaration is not a declaration; submission must be refused. |
| `INV-EFFECTIVE-FROM-WINDOW` | `effective_from` is in the past (or today) and not more than five years before `submitted_at`. | `aggregate.rs::validate_effective_from()`; tested in `aggregate.rs::tests::future_effective_from_rejects`. | `OHADA-AML-CFT` (declarations must reflect the period for which the entity is in scope); platform-imposed 5-year window aligns with `OHADA-AML-CFT` retention floors. | `[CITATION NEEDED: ohada-aml-cft-art-declaration-window]`. | Future-dated `effective_from` rejects with `EffectiveFromInFuture`; >5y-back rejects with `EffectiveFromTooOld`. |
| `INV-ATTESTATION` | Every `SubmitDeclaration` / `AmendDeclaration` / `CorrectDeclaration` carries an Ed25519 attestation signed over the canonical payload; `attestation.signed_by` must equal the authenticated principal (D17). | `aggregate.rs::validate_command()`, `aggregate.rs::handle_amend()`, `aggregate.rs::handle_correct()`; verified at the API boundary in `api/rest.rs` and `api/grpc.rs`. | `OHADA-AML-CFT` (non-repudiation of audit-trail events); `CM-LAW-2014-007` (signature requirement on declarations); `UN-UNCAC` Art. 9 (public procurement and management of public finances — accountability); `GDPR-EU-2016-679` Art. 32 (security of processing). | `[CITATION NEEDED: ohada-aml-cft-art-non-repudiation]`; `[CITATION NEEDED: cm-law-2014-007-art-signature]`; `[CITATION NEEDED: uncac-art-9-or-equivalent]`; GDPR Art. 32 public text. | The canonical bytes are byte-parity across REST and gRPC so a single signature is valid under either transport. The receipt hash (BLAKE3-256) is computed over the canonical command form (D15). |
| `INV-IDEMPOTENCY` | State-changing endpoints (`POST /v1/declarations`, amend, correct, supersede, verification-outcome writeback) are replay-safe: the same logical request yields the same logical outcome, with no duplicate state mutation. | `Idempotency-Key` header (submit); aggregate replay-detection on case_id (verification outcome); event_id deduplication on outbox/DLQ replay. | `OHADA-AML-CFT` (financial-regulatory requirement for replay-safety of audit-trail writes); `GDPR-EU-2016-679` Art. 32 (security of processing — integrity); industry expectation from financial-supervisory bodies (e.g. ARMP-procurement / BEAC-central-bank operational guidance). | `[CITATION NEEDED: ohada-aml-cft-art-replay-safety]`; `[CITATION NEEDED: armp-or-beac-operational-guidance-replay]`; GDPR Art. 32 public text. | Engineering doctrine D13 ("idempotency on every state-changing operation") encodes this. A new state-changing endpoint without idempotency is a doctrine violation regardless of the legal-basis row. |
| `INV-EVENT-LOG-APPEND-ONLY` | `declaration_events` and `verification_cases` reject UPDATE / DELETE / TRUNCATE at the database boundary (BEFORE-trigger + REVOKE on PUBLIC). | Migrations `0007_audit_log_immutability.sql` (Declaration) and `0003_audit_log_immutability.sql` (V-engine); see `docs/compliance/data-retention.md`. | `OHADA-AML-CFT` (audit-trail immutability is the load-bearing AML control); `FATF-R24` INR 24.4 (records must remain available for verification and law enforcement); `UN-UNCAC` Art. 14 (AML measures — record-keeping); `AU-AUCPCC` Art. 7. | `[CITATION NEEDED: ohada-aml-cft-art-audit-trail-immutability]`; `[CITATION NEEDED: uncac-art-14-record-keeping]`; `[CITATION NEEDED: au-aucpcc-art-7-record-keeping]`. | This is what makes partial-erasure-only the operational answer to GDPR Art. 17 (see `gdpr-procedures.md` § 3). Receipt hashes (D15) pin to rows in this table; pruning would invalidate the entire receipt chain. |
| `INV-OUTBOX-RETENTION` | `outbox` / `verification_outbox` rows are pruned 30 days after `dispatched_at`; un-dispatched rows survive regardless of age. DLQs are NEVER pruned. | Retention workers in `services/declaration/src/infrastructure/retention.rs` and `services/verification-engine/src/infrastructure/retention.rs`. | `OHADA-AML-CFT` (retention floors apply to audit-trail records, not operational queues); `GDPR-EU-2016-679` Art. 5(1)(e) storage limitation (operational queues are bounded). | `[CITATION NEEDED: ohada-aml-cft-art-retention-floors]`; GDPR Art. 5(1)(e) public text. | See `docs/compliance/data-retention.md` for the full per-table policy. |
| `INV-PII-REDACTION-IN-LOGS` | PII fields (declarant principal subject, beneficial-owner person_id, attestation public key, free-form metadata) are redacted from operational logs by the `RedactingLayer` in `packages/recor-logging`. | `packages/recor-logging/src/lib.rs`; deployed at the service tracing-layer construction site. | `GDPR-EU-2016-679` Art. 32 (security of processing — confidentiality); `CM-DATA-PROTECTION`; `OHADA-AML-CFT` (operational confidentiality of regulated data). | GDPR Art. 32 public text; `[CITATION NEEDED: cameroon-data-protection-confidentiality]`; `[CITATION NEEDED: ohada-aml-cft-art-operational-confidentiality]`. | Tracked under OPS-2 in `docs/PRODUCTION-TODO.md`. The redaction layer is mandatory in all environments. |
| `INV-AUTH-PRINCIPAL-SOURCE` | Authenticated principal is sourced from the OIDC bearer token (production) or the dev-only `X-Recor-Dev-Principal` header (development only); never from request body, path, or query string. | `services/declaration/src/api/auth.rs`, `oidc.rs`, `grpc.rs::auth_interceptor`; ADR-0004. | `OHADA-AML-CFT` (non-repudiation requires authoritative identity binding); `GDPR-EU-2016-679` Art. 5(1)(f) integrity and confidentiality; doctrine D17 zero-trust. | `[CITATION NEEDED: ohada-aml-cft-art-non-repudiation]`; GDPR Art. 5(1)(f) public text. | The principal flows from the verified token into both the canonical bytes and the audit record. Client-supplied identity fields (e.g. `attestation.signed_by`) are validated *against* the authoritative principal, not used as the source. |
| `INV-NO-AUTOMATED-DECISION-WITH-LEGAL-EFFECT` | The verification engine's lane decisions (green / yellow / red) and Dempster-Shafer outputs are advisory; *no* legally-binding outcome is determined solely by automated processing. Human review by ARMP / ANIF / DGI / BEAC / CONAC is the legally-effective act. | Architecture V4 P11 § Consumer Access; `gdpr-procedures.md` § 6. | `GDPR-EU-2016-679` Art. 22 (no automated decision with legal effect); `CM-LAW-2014-007` (statutory decisions are vested in named authorities); UNCAC accountability articles. | GDPR Art. 22 public text; `[CITATION NEEDED: cameroon-administrative-procedure-art-decision-authority]`; `[CITATION NEEDED: uncac-art-N-accountability]`. | This is a constraint on consumer-facing surfaces — the lane is shown to human reviewers; the platform does not deliver a final adverse decision to a data subject without a human in the loop. |

## Cross-references and adjacency

This document does not stand alone. It is the legal-basis projection
of the operational policies already documented in:

- **`docs/compliance/gdpr-procedures.md`** (COMP-1) — the per-right
  operational procedures for GDPR / OHADA data-subject requests. The
  endpoints `INV-AUTH-PRINCIPAL-SOURCE` and the GDPR rows above all
  surface there.
- **`docs/compliance/data-retention.md`** (COMP-2) — the per-table
  retention policy; the legal basis for `INV-EVENT-LOG-APPEND-ONLY` and
  `INV-OUTBOX-RETENTION`.
- **`docs/compliance/data-classification.md`** (COMP-3 — pending) — the
  per-column Public / Internal / Confidential / PII / Sensitive-PII
  inventory; the underpinning for `INV-PII-REDACTION-IN-LOGS`.
- **`docs/security/threat-model.md`** (DOC-4) — STRIDE coverage; the
  technical-control side of the GDPR Art. 32 obligation cited above.
- **`docs/openapi/declaration.json`** — the authoritative endpoint
  list. Any change there MUST be reflected here (per the doctrine that
  documentation is part of the feature, D05).
- **`contracts/declaration.proto`** — the authoritative gRPC contract.
  Same change-propagation discipline.
- **ADR-0001** (`docs/adr/0001-event-sourcing-declaration-aggregate.md`)
  — the architectural decision that makes `INV-EVENT-LOG-APPEND-ONLY`
  load-bearing.
- **ADR-0004** (`docs/adr/0004-oidc-jwks-principal-authentication.md`)
  — the architectural decision that makes `INV-AUTH-PRINCIPAL-SOURCE`
  load-bearing.
- **ADR-0005** — HMAC dual-secret rotation; basis for the writeback-
  channel idempotency / non-repudiation properties.
- **`services/declaration/CLAUDE.md`** and
  **`services/verification-engine/CLAUDE.md`** — service-level
  operational instructions; their SLO sections cross-link here for the
  regulatory framing.

## How to use this document

### When introducing a new endpoint or RPC

1. Add a row to the appropriate endpoint table (REST or gRPC) for the
   service.
2. Cite the legal basis. If you do not have a confirmed article,
   write `[CITATION NEEDED: <slug>]` — do not invent.
3. Cross-link from the OpenAPI spec (or the `.proto` file) by adding
   `regulatory-mapping.md` to the endpoint's docstring.
4. Update `gdpr-procedures.md` if the new surface affects any of the
   six GDPR rights.
5. Have AML/CFT counsel resolve the `[CITATION NEEDED: ...]` markers
   in the same review cycle.

### When introducing a new domain invariant

1. Add a row to the invariant table. The row must point to the
   specific source location (file:line or function name) so an
   auditor can verify the enforcement.
2. Cite the legal basis as above.
3. The Architecture or an ADR must already justify the invariant —
   compliance documentation does not introduce invariants on its own.

### When a regulator asks "which provision compels this?"

The auditor's first question is "show me the endpoint table." If the
row says `[CITATION NEEDED]`, the engineering answer is "counsel
confirms within <date>"; the platform does not assert citations it has
not verified.

## Outstanding `[CITATION NEEDED: ...]` markers

The slugs below appear in the tables above and need counsel resolution.
Each slug is unique; counsel can resolve them in a single review pass.

| Slug | Where it appears | What to confirm |
|---|---|---|
| `cm-law-2014-007-art-declaration` | Endpoint table — `POST /v1/declarations` | The specific article of Loi N° 2014/007 that compels the declaration. |
| `cm-law-2014-007-art-change-of-control` | Endpoint table — `POST .../supersede` | Article on change-of-control disclosure. |
| `cm-law-2014-007-art-accurate-filings` | Endpoint table — `POST .../correct` | Article on the duty to maintain accurate filings. |
| `cm-law-2014-007-art-adjudication` | V-engine endpoint table — `POST /v1/verifications` | Article on the statutory adjudication mechanism. |
| `cm-law-2014-007-art-verification-mechanism` | Endpoint table — `POST /v1/internal/verification-outcomes` | Article on the verification mechanism. |
| `cm-law-2014-007-art-beneficiary-presence` | Invariant — `INV-NON-EMPTY-OWNERS` | Article requiring at least one identified beneficiary. |
| `cm-law-2014-007-art-signature` | Invariant — `INV-ATTESTATION` | Article on the signature requirement. |
| `cameroon-bo-decree-citation` | Legal-basis catalogue — `CM-BO-DECREE` | The implementing decree's reference (number and date). |
| `cameroon-bo-decree-art-threshold-and-completeness` | Invariant — `INV-OWNERSHIP-SUM` | Article on the 25% threshold and ownership-completeness obligation. |
| `cameroon-data-protection-citation` | Legal-basis catalogue — `CM-DATA-PROTECTION` | The controlling national data-protection instrument(s). |
| `cameroon-data-protection-right-of-access` | Endpoint table — `GET .../by-principal` | National-law equivalent of GDPR Art. 15. |
| `cameroon-data-protection-confidentiality` | Invariant — `INV-PII-REDACTION-IN-LOGS` | National-law equivalent of GDPR Art. 32 confidentiality. |
| `cameroon-administrative-procedure-art-decision-authority` | Invariant — `INV-NO-AUTOMATED-DECISION-WITH-LEGAL-EFFECT` | Article on which administrative authority holds the decision power. |
| `ohada-auscgie-2014-art-BO-disclosure` | Endpoint table — `POST /v1/declarations` | Article on BO disclosure under the OHADA Uniform Act on Commercial Companies. |
| `ohada-auscgie-2014-art-update` | Endpoint table — `POST .../amend` | Article on the obligation to keep BO data current. |
| `ohada-auscgie-2014-art-change-of-control` | Endpoint table — `POST .../supersede` | Article on change-of-control disclosure. |
| `ohada-auscgie-2014-art-ownership-completeness` | Invariant — `INV-OWNERSHIP-SUM` | Article on ownership-share completeness. |
| `ohada-auscgie-2014-art-unambiguous-bo` | Invariant — `INV-NO-DUPLICATE-OWNERS` | Article on unambiguous BO identification. |
| `ohada-aml-cft-art-currency` | Endpoint table — `POST .../amend` | Article on the duty to keep records current. |
| `ohada-aml-cft-art-correction-trail` | Endpoint table — `POST .../correct` | Article on the requirement to record corrections as separate events. |
| `ohada-aml-cft-audit-trail-access` | Endpoint table — `GET /v1/declarations/{id}` | Article on audit-trail access by declarant + authorised consumers. |
| `ohada-aml-cft-art-operational-integrity` | Endpoint table — DLQ admin endpoints; invariant tables | Article on operational-integrity duties (DLQ visibility / replay). |
| `ohada-aml-cft-art-cross-service-relay` | Endpoint table — `POST /v1/internal/verification-outcomes`; `POST /v1/internal/declaration-events` | Article on cross-service audit-trail relay. |
| `ohada-aml-cft-art-disposition` | V-engine endpoint table — `POST /v1/verifications` | Article on the verification-disposition record. |
| `ohada-aml-cft-case-access` | V-engine endpoint table — `GET /v1/verifications/{id}` | Article on case-level audit-trail access. |
| `ohada-aml-cft-art-declaration-window` | Invariant — `INV-EFFECTIVE-FROM-WINDOW` | Article on the declaration-period window. |
| `ohada-aml-cft-art-non-repudiation` | Invariant — `INV-ATTESTATION`; `INV-AUTH-PRINCIPAL-SOURCE` | Article on non-repudiation requirements for BO events. |
| `ohada-aml-cft-art-replay-safety` | Invariant — `INV-IDEMPOTENCY` | Article (or supervisory guidance) on replay-safety of audit-trail writes. |
| `ohada-aml-cft-art-audit-trail-immutability` | Invariant — `INV-EVENT-LOG-APPEND-ONLY` | Article on audit-trail immutability. |
| `ohada-aml-cft-art-retention-floors` | Invariant — `INV-OUTBOX-RETENTION` | Article on the retention floors applicable to BO records. |
| `ohada-aml-cft-art-operational-confidentiality` | Invariant — `INV-PII-REDACTION-IN-LOGS` | Article on operational confidentiality of regulated data. |
| `armp-or-beac-operational-guidance-replay` | Invariant — `INV-IDEMPOTENCY` | The specific ARMP-procurement / BEAC-central-bank operational-guidance reference (if any) on replay-safety. |
| `uncac-art-9-or-equivalent` | Invariant — `INV-ATTESTATION` | The UNCAC article on accountability in public procurement / public finance. |
| `uncac-art-14-record-keeping` | Invariant — `INV-EVENT-LOG-APPEND-ONLY` | UNCAC Art. 14 (AML measures — record-keeping); confirm article numbering. |
| `uncac-art-N-accountability` | Invariant — `INV-NO-AUTOMATED-DECISION-WITH-LEGAL-EFFECT` | The UNCAC article on accountability of decision-making. |
| `au-aucpcc-art-7-record-keeping` | Invariant — `INV-EVENT-LOG-APPEND-ONLY` | AU AUCPCC Art. 7 reference. |

(Where a slug appears in the GDPR-procedures document with the same
spelling, counsel resolves it once and both documents inherit the
resolution.)

## Outstanding actions before this document is authoritative

1. **AML/CFT counsel sign-off** on every `[CITATION NEEDED: ...]`
   marker in the table above and inline in the body. The placeholders
   are deliberate and not resolvable by engineering.
2. **Architecture cross-link** — once this document lands, the
   Architecture Document V1 P11 § Consumer Access section should
   reference it from the legal-basis paragraph (next Architecture
   revision).
3. **Service docstrings** — when a new endpoint is added to
   `rest.rs` / `grpc.rs`, its docstring should reference this file by
   path so future engineers find the legal-basis row without ambient
   knowledge.
4. **Public-portal access surface** — when `R-VER-2` / `R-VER-3` and
   the consumer-access ticket land, the public-record portal's
   endpoints will be added to this table with `CJEU-C-37-20` as the
   constraining citation.
5. **Person-registry service** — once the person-registry service is
   created, its endpoints will be added here.
