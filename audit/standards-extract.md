# Phase 1A — FATF corpus

Extracted testable requirements from the FATF beneficial-ownership standards
corpus, intended to feed RÉCOR audit criteria for FATF R.24 + R.25 compliance.

Sources fetched directly (PDF text extracted):
- FATF/CFATF 5th Round Methodology revisions to R.24/R.25/IO.5 (Oct 2023;
  document FATF/ECG(2023)17/REV3). Authoritative criteria text — primary source
  for REQ-r24-*, REQ-r25-*, REQ-io5-*.
- FATF Public Consultation: Guidance on Beneficial Ownership (R.24), August
  2022 draft (the text that became the March 2023 final Guidance).
- World Bank / UNODC StAR, *The Puppet Masters* (2011), 14 policy
  recommendations across Parts 2, 3, and 4.
- Open Ownership Principles (January 2023 update) — corroborating
  cross-reference where FATF text is silent on data-engineering specifics.
- Open Ownership response to FATF revisions to R.25 / INR.25 (Dec 2022 draft
  with the revised text inline).

Sources unavailable via direct WebFetch (FATF returned HTTP 403; alternate
mirrors via WebSearch summaries were consulted, but no requirement below
depends on a summary-only source — every entry carries a primary-document
citation):
- fatf-gafi.org domain (entire CDN blocked WebFetch);
- 2014 FATF Guidance on Transparency & BO (not extracted standalone);
- FATF Best Practices on BO (substantive content folded into the 2023
  Guidance, which is the source used here).

## Source: FATF Recommendation 24 + INR.24 (5th Round Methodology, Oct 2023)

- **REQ-r24-001** [c.24.1(a-d)] — Registry MUST apply R.24 requirements to all forms of legal person created in the country (companies, foundations, Anstalt, Waqf, LLPs, other), with foreign-created legal persons that have sufficient links covered by c.24.3(b), 24.10, 24.14.
- **REQ-r24-002** [c.24.1(d) fn 15] — Registry MUST define and document the "sufficient link" test for foreign-created legal persons (e.g., branch, significant business, FI/DNFBP relationship, real estate, employees, tax residence).
- **REQ-r24-003** [c.24.2(a)] — Country MUST publicly publish the different types, forms, and basic features of every legal-person form that exists in the jurisdiction.
- **REQ-r24-004** [c.24.2(b)] — Country MUST publicly publish the processes for creating each legal-person form in the country.
- **REQ-r24-005** [c.24.2(c)] — Country MUST publicly publish the processes by which basic and beneficial-ownership information is obtained and recorded.
- **REQ-r24-006** [c.24.3(a)] — Country MUST conduct an ML/TF risk assessment for each type of domestically created legal person and apply mitigation measures proportionate to identified risk.
- **REQ-r24-007** [c.24.3(b)] — Country MUST assess ML/TF risks associated with each type of foreign-created legal person that has sufficient links with the country and apply mitigation measures.
- **REQ-r24-008** [c.24.4] — Registry MUST register every company created in the country in a company registry and MUST make the basic information in c.24.5(a) public.
- **REQ-r24-009** [c.24.5(a)] — Registry MUST hold for every company: name, proof of incorporation, legal form and status, registered office address, basic regulating powers (memorandum & articles), list of directors, and a unique identifier (e.g., TIN).
- **REQ-r24-010** [c.24.5(b)] — Companies MUST maintain a register of shareholders/members containing names, shares held by each, and share categories including voting rights.
- **REQ-r24-011** [c.24.5(c)] — Companies MUST maintain the shareholder register within the country at the registered office or a notified location (relaxed only where BO data is held in-country and the shareholder list can be produced promptly).
- **REQ-r24-012** [c.24.6 chapeau + Note to Assessors] — Country MUST implement all three prongs of the multi-pronged approach (company self-holding, public-body or alternative mechanism, supplementary measures); information held by FIs/DNFBPs under R.10/R.22 MUST NOT be treated as the "alternative mechanism".
- **REQ-r24-013** [c.24.6(a)] — Companies MUST obtain and hold adequate, accurate, and up-to-date information on their own beneficial ownership and provide it to competent authorities in a timely manner.
- **REQ-r24-014** [c.24.6(a)] — Companies MUST cooperate with FIs/DNFBPs by supplying adequate, accurate, and up-to-date BO information for CDD purposes.
- **REQ-r24-015** [c.24.6(b)(i)] — A public authority or body (e.g., tax authority, FIU, company registry, BO registry) MUST hold BO information of legal persons; information may be split across multiple bodies (sectoral, provincial, NPO-specific).
- **REQ-r24-016** [c.24.6(b)(ii)] — If an alternative mechanism is used in lieu of c.24.6(b)(i), it MUST provide authorities with efficient access to adequate, accurate, and up-to-date BO information; reliance on basic information alone is insufficient.
- **REQ-r24-017** [c.24.6(c)] — Country MUST use additional supplementary BO sources where necessary (regulator/stock-exchange data, FI/DNFBP CDD information).
- **REQ-r24-018** [c.24.6 fn 25] — BO threshold for "controlling shareholder" MUST be determined on a risk basis and MUST NOT exceed 25%.
- **REQ-r24-019** [c.24.6 fn 26] — Competent authorities MUST be able to determine in a timely manner whether a company has or controls an account with a financial institution in the country.
- **REQ-r24-020** [c.24.7] — All persons, authorities, entities (and the company itself, including liquidators) MUST retain BO and basic information for at least five years after dissolution or after the company ceases to be a customer.
- **REQ-r24-021** [c.24.8 + fn 27] — Registry MUST ensure BO information is *adequate* — sufficient to identify the natural person(s), including full name, all nationalities, full date and place of birth, residential address, national ID number and document type, and TIN (or equivalent).
- **REQ-r24-022** [c.24.8 + fn 28] — Registry MUST ensure BO information is *accurate* — verified by reliable, independently sourced/obtained documents, data, or information, with verification depth varying by risk; complementary measures such as discrepancy reporting SHOULD be deployed.
- **REQ-r24-023** [c.24.8 + fn 29] — Registry MUST ensure BO information is *up-to-date* — updated within a reasonable period following any change (FATF benchmark: within one month).
- **REQ-r24-024** [c.24.9] — Law enforcement and FIUs MUST have all powers necessary to obtain timely access to basic and BO information held by relevant parties, including rapid access to data held by public bodies and FIs/DNFBPs.
- **REQ-r24-025** [c.24.9] — Public authorities at national level (and others as appropriate) MUST have timely access to basic and BO information on legal persons in the course of public procurement.
- **REQ-r24-026** [c.24.10] — Competent authorities MUST be able to obtain or access in a timely fashion adequate, accurate, and up-to-date BO and control information on foreign-created legal persons that present ML/TF risks and have sufficient links with the country.
- **REQ-r24-027** [c.24.11] — Company registry MUST facilitate timely access by FIs, DNFBPs, and foreign competent authorities to the public information it holds, at minimum the c.24.5(a) data.
- **REQ-r24-028** [c.24.12(a)] — Country MUST prohibit the issuance of new bearer shares and bearer share warrants.
- **REQ-r24-029** [c.24.12(b)(i)] — For any pre-existing bearer shares/warrants, country MUST require conversion into registered form within a reasonable timeframe.
- **REQ-r24-030** [c.24.12(b)(ii)] — Alternatively, country MUST require immobilisation of existing bearer instruments at a regulated FI/professional intermediary with timely competent-authority access.
- **REQ-r24-031** [c.24.12(b)(iii)] — Pending conversion/immobilisation, bearer-instrument holders MUST notify the company and the company MUST record their identity before any associated rights can be exercised.
- **REQ-r24-032** [c.24.13(a)] — Country MUST require nominee shareholders and directors to disclose nominee status and nominator identity to the company and any relevant registry, with nominee status published in public information.
- **REQ-r24-033** [c.24.13(b)] — Alternatively, country MUST require nominee shareholders/directors to be licensed, with their nominee status and nominator identity recorded by the public body or alternative mechanism.
- **REQ-r24-034** [c.24.13(c)] — Alternatively, country MUST prohibit the use of nominee shareholders or directors outright.
- **REQ-r24-035** [c.24.13 fn 34] — Where a nominee holds a controlling interest or exercises effective control, the registry MUST establish the identity of the natural person on whose behalf the nominee is ultimately, directly or indirectly, acting.
- **REQ-r24-036** [c.24.14] — Country MUST impose clearly stated responsibility, liability, and proportionate dissuasive sanctions on any legal or natural person that fails to comply with R.24 requirements.
- **REQ-r24-037** [c.24.15(a)] — Country MUST NOT place unduly restrictive conditions on the international exchange of BO information (fiscal, tax, bank-secrecy refusals prohibited).
- **REQ-r24-038** [c.24.15(b)] — Registry MUST facilitate access by foreign competent authorities to basic information held by company registries.
- **REQ-r24-039** [c.24.15(c)] — Country MUST exchange shareholder information with foreign counterparts.
- **REQ-r24-040** [c.24.15(d)] — Country MUST use its domestic powers to obtain BO information on behalf of foreign counterparts on request.
- **REQ-r24-041** [c.24.15(e)] — Country MUST monitor the quality of assistance received from other countries in response to BO information requests.
- **REQ-r24-042** [c.24.15(f)] — Country MUST keep BO information in a readily accessible manner for the purpose of identifying beneficial ownership.
- **REQ-r24-043** [c.24.15(g)] — Country MUST designate and publicly identify the agency(ies) responsible for responding to all international requests for BO information.
- **REQ-r24-044** [INR.24 + Glossary] — Registry MUST treat as beneficial owner only one or more natural persons; legal persons MUST NOT be recorded as ultimate beneficial owners.
- **REQ-r24-045** [c.24.6(b)(i) fn 24] — Where BO is recorded across multiple bodies (sectoral, provincial, NPO-specific), the registry MUST ensure each component is linked into a coherent view available to competent authorities.

## Source: FATF Recommendation 25 + INR.25 (5th Round Methodology, Oct 2023)

- **REQ-r25-001** [c.25.1] — R.25 requirements MUST apply to all express trusts and other similar legal arrangements (including fiducie, certain Treuhand, fideicomiso, and Waqf where Waqf are not legal persons).
- **REQ-r25-002** [c.25.2(a)] — Country MUST publicly identify the different types, forms, and basic features of express trusts and similar arrangements governed under its law.
- **REQ-r25-003** [c.25.2(b)] — Country MUST publicly describe the processes for setting up legal arrangements and for obtaining basic and BO information about them.
- **REQ-r25-004** [c.25.2(c)] — The information in c.25.2(a) and (b) MUST be made publicly available.
- **REQ-r25-005** [c.25.3(a)] — Country MUST assess the ML/TF risks of express trusts and similar arrangements governed under its law and apply risk-proportionate mitigation.
- **REQ-r25-006** [c.25.3(b)] — Country MUST assess ML/TF risks of arrangements administered in the country or where the trustee resides in the country.
- **REQ-r25-007** [c.25.3(c)] — Country MUST assess ML/TF risks of foreign-created legal arrangements with sufficient links to the country.
- **REQ-r25-008** [c.25.4(a)(i-v)] — Trustees (and persons in equivalent positions) MUST obtain and hold adequate, accurate, and up-to-date BO information identifying: (i) settlor(s); (ii) trustee(s); (iii) protector(s); (iv) each beneficiary or class of beneficiaries and objects of a power; (v) any other natural person exercising ultimate effective control.
- **REQ-r25-009** [c.25.4(b)] — Where any party to a trust/arrangement is a legal person, the trustee MUST also obtain and hold adequate, accurate, up-to-date basic and BO information for that legal person.
- **REQ-r25-010** [c.25.4(c)] — Trustees MUST hold basic information on other regulated agents and service providers including investment advisors/managers, accountants, and tax advisors.
- **REQ-r25-011** [c.25.5] — Trustees MUST retain the c.25.4 information for at least five years after their involvement with the trust ceases.
- **REQ-r25-012** [c.25.6] — Information held under c.25.4 MUST be kept accurate and up-to-date, updated within a reasonable period following any change.
- **REQ-r25-013** [c.25.7(a)] — Trustees MUST disclose their trustee status to FIs/DNFBPs when forming a business relationship or carrying out an occasional transaction above threshold.
- **REQ-r25-014** [c.25.7(b)] — Trustees MUST be required by law to cooperate fully with competent authorities and MUST NOT be prevented by law or enforceable means from providing necessary information.
- **REQ-r25-015** [c.25.7(c)] — Trustees MUST NOT be prevented by law or enforceable means from providing FIs/DNFBPs, on request, with BO information and information on trust assets to be held/managed under the business relationship.
- **REQ-r25-016** [c.25.8 + fn 50] — Registry MUST ensure trust-related BO information is *adequate* — sufficient to identify natural persons in each role (settlor, trustee, protector, beneficiary class/objects of a power, ultimate controller).
- **REQ-r25-017** [c.25.8 + fn 51] — Registry MUST ensure trust BO information is *accurate* — verified by reliable documents/data/information with verification depth varying by risk.
- **REQ-r25-018** [c.25.8 + fn 52] — Registry MUST ensure trust BO information is *up-to-date* — updated within a reasonable period after any change.
- **REQ-r25-019** [c.25.9(a)] — Country SHOULD use a public authority or body (e.g., central trust registry, or asset registries for land/property/vehicles/shares) as a source of trust BO information.
- **REQ-r25-020** [c.25.9(b)] — Country SHOULD use other competent authorities (e.g., tax authority) as a source of trust BO information.
- **REQ-r25-021** [c.25.9(c)] — Country SHOULD use other agents/service providers (TCSPs, investment advisors, accountants, lawyers, FIs) as a source of trust BO information.
- **REQ-r25-022** [c.25.10(a)] — Law enforcement and FIUs MUST have powers to obtain timely access to basic and BO information on legal arrangements held by trustees, equivalents, FIs, and DNFBPs.
- **REQ-r25-023** [c.25.10(b)] — Competent authorities MUST be able to obtain the residence of trustees and their equivalents.
- **REQ-r25-024** [c.25.10(c)] — Competent authorities MUST be able to obtain information on any assets held or managed by an FI/DNFBP in connection with a trustee they have a relationship with.
- **REQ-r25-025** [c.25.11(a)] — Country MUST establish clear responsibilities to comply with INR.25 requirements.
- **REQ-r25-026** [c.25.11(b)] — Country MUST hold trustees legally liable for failure to perform c.25.4–25.7 duties OR impose effective, proportionate, dissuasive criminal/civil/administrative sanctions.
- **REQ-r25-027** [c.25.11(c)] — Country MUST impose effective, proportionate, dissuasive sanctions for failure to grant competent authorities timely access to trust information referred to in c.25.4 and 25.5.
- **REQ-r25-028** [c.25.12(a)] — Country MUST NOT place unduly restrictive conditions on the international exchange of trust BO information (no fiscal/bank-secrecy refusals).
- **REQ-r25-029** [c.25.12(b)] — Country MUST facilitate foreign-counterpart access to trust information held by registries or other domestic authorities.
- **REQ-r25-030** [c.25.12(c)] — Country MUST exchange domestically available trust information with foreign counterparts.
- **REQ-r25-031** [c.25.12(d)] — Country MUST use domestic powers to obtain BO information about trusts on behalf of foreign counterparts.
- **REQ-r25-032** [c.25.12(e)] — Country SHOULD designate and publicly name the agency responsible for responding to international BO requests, and SHOULD keep BO information in a readily accessible manner.
- **REQ-r25-033** [c.25.4 fn 47] — Where there are no ascertainable beneficiaries at trust setup, the registry MUST capture information on the class of beneficiaries, its characteristics, and objects of a power.

## Source: FATF Immediate Outcome 5 (5th Round Methodology, Oct 2023)

- **REQ-io5-001** [IO.5 outcome statement] — System MUST prevent the misuse of legal persons and arrangements for ML/TF, and BO information MUST be available to competent authorities without impediments.
- **REQ-io5-002** [IO.5 Characteristics of an Effective System] — Country MUST identify, assess, and understand its ML/TF risks for both domestic and foreign-created legal persons/arrangements with sufficient links.
- **REQ-io5-003** [IO.5 Characteristics] — Certain basic information MUST be publicly available; BO information MUST be available to competent authorities.
- **REQ-io5-004** [IO.5 Characteristics] — Persons who breach the BO measures MUST be subject to effective, proportionate, and dissuasive sanctions.
- **REQ-io5-005** [Core Issue 5.1] — Country MUST demonstrate it identifies, assesses, and understands ML/TF risks associated with domestic and foreign-created legal persons and arrangements with sufficient links.
- **REQ-io5-006** [Core Issue 5.2] — Country MUST implement measures that prevent, manage, and mitigate misuse of legal persons/arrangements, including bearer-share, bearer-share-warrant, and nominee-shareholder/director risk.
- **REQ-io5-007** [Core Issue 5.3] — Competent authorities MUST be able to obtain adequate, accurate, and up-to-date basic and BO information on all domestic and risky/sufficiently-linked foreign legal persons in a timely manner.
- **REQ-io5-008** [Core Issue 5.4] — Competent authorities MUST be able to obtain timely BO information on legal arrangements (incl. trustee residence and FI/DNFBP-held assets), plus basic information on other regulated agents/service providers to the arrangement.
- **REQ-io5-009** [Core Issue 5.5] — Country MUST apply effective, proportionate, dissuasive sanctions against persons who do not comply with the BO information requirements.
- **REQ-io5-010** [IO.5 Examples of Information ¶5] — Country MUST evidence sources of basic and BO information (public-information types accessible to FIs/DNFBPs, registry data, BO-registry data, alternative-mechanism data).
- **REQ-io5-011** [IO.5 Examples of Information ¶6] — Registry MUST be able to evidence checks performed at the time of registration AND subsequently, and SHOULD implement complementary measures such as discrepancy reporting to support BO accuracy.
- **REQ-io5-012** [IO.5 Examples of Information ¶7] — Country MUST track and report the prevalence and impact of bearer shares, bearer share warrants, nominee shareholders, and nominee directors as obstacles to timely BO access.
- **REQ-io5-013** [IO.5 Examples of Specific Factors ¶13] — System MUST report the time taken for legal persons to register changes to required basic and BO information, evidencing that information is adequate, accurate, and up-to-date.
- **REQ-io5-014** [IO.5 Examples of Specific Factors ¶14] — Registry MUST facilitate access by FIs/DNFBPs (R.10/R.22) to BO and control information and to information held on trusts by the parties enumerated in c.25.9.

## Source: FATF Guidance on Beneficial Ownership of Legal Persons (March 2023; public-consultation draft Oct 2022)

- **REQ-fatfg-001** [Guidance §2 ¶15] — Country MUST have mechanisms to identify all types of legal persons that can be created and assess their ML/TF risk profile as the starting point of any BO framework.
- **REQ-fatfg-002** [Guidance §2 ¶18] — Risk assessment SHOULD consider entity type, sector of operation, geographic exposure, presence of foreign ownership, use of nominees, and use of bearer instruments.
- **REQ-fatfg-003** [Guidance §3 ¶26] — All companies created in the country MUST be registered in a company registry with the c.24.5(a) basic information held and publicly available.
- **REQ-fatfg-004** [Guidance §3 ¶28] — Country MUST take measures to enhance the reliability of registry information where data quality issues exist (independent verification, fact-checking, discrepancy reporting).
- **REQ-fatfg-005** [Guidance §4 ¶31] — Registry MUST capture BO information on the natural person(s) who ultimately own or control a legal person AND on the natural person(s) on whose behalf transactions are conducted.
- **REQ-fatfg-006** [Guidance §4 ¶32] — Where no beneficial owner is identifiable, the registry MUST capture the senior managing official as a fall-back and clearly flag this status.
- **REQ-fatfg-007** [Guidance §4 ¶36–37] — Country MUST set the ownership threshold for BO on a risk basis, at a maximum of 25%, and MUST consider lower thresholds for higher-risk sectors.
- **REQ-fatfg-008** [Guidance §4 ¶40] — Registry MUST capture indirect ownership held through chains of intermediate entities and trace ownership through to a natural person.
- **REQ-fatfg-009** [Guidance §4 ¶42] — Registry MUST capture control exercised other than through ownership (voting rights, board appointment, contractual arrangements, family relationships).
- **REQ-fatfg-010** [Guidance §5 ¶45] — Country MUST use a multi-pronged BO approach combining the company prong, the public-registry/alternative-mechanism prong, and supplementary FI/DNFBP/regulator data.
- **REQ-fatfg-011** [Guidance §6 — adequate] — BO record MUST include, at minimum, full name, all nationalities, full date and place of birth, residential address, national ID type and number, and TIN/equivalent in country of residence.
- **REQ-fatfg-012** [Guidance §7 — accurate] — Registry MUST verify BO data through reliable, independently sourced documents; verification depth MUST scale with risk; complementary measures (discrepancy reporting, cross-database checks) SHOULD be implemented.
- **REQ-fatfg-013** [Guidance §8 — up-to-date] — Registry MUST require an update within one month of any change to BO data and MUST require periodic re-confirmation of records.
- **REQ-fatfg-014** [Guidance §10 — registry approach] — Where the country adopts a registry approach, the BO registry MUST be a public authority or body, with a designated owner, the ability to enforce data quality, and powers to compel correction.
- **REQ-fatfg-015** [Guidance §12 — supplementary measures] — Country MUST identify and use supplementary BO data sources (regulator filings, stock-exchange disclosures, FI/DNFBP CDD output) to triangulate registry data.
- **REQ-fatfg-016** [Guidance §13 — access] — Country MUST grant competent authorities rapid and direct access to BO data; access SHOULD also be facilitated for FIs/DNFBPs for CDD and for procurement bodies.
- **REQ-fatfg-017** [Guidance §14 — bearer shares] — Country MUST prohibit new bearer shares/warrants AND immobilise/convert all existing instruments within a reasonable timeframe.
- **REQ-fatfg-018** [Guidance §15 — nominees] — Country MUST require nominee status and the nominator's identity to be disclosed to the registry, with this information searchable by competent authorities.
- **REQ-fatfg-019** [Guidance §16 — sanctions] — Sanctions for BO non-compliance MUST cover the company, its directors and officers, the beneficial owner if culpable, and any intermediary (TCSP, FI, DNFBP) that facilitated the breach.
- **REQ-fatfg-020** [Guidance §19 — international cooperation] — Designated BO international-cooperation agency MUST publish response-time targets and metrics for incoming/outgoing requests.

## Source: World Bank / UNODC StAR — *The Puppet Masters* (2011)

- **REQ-puppet-001** [Part 2 Rec. 1, p.30] — System MUST treat beneficial owner as always a natural person; legal entities or arrangements MUST NOT terminate the ownership chain.
- **REQ-puppet-002** [Part 2 Rec. 3, p.30] — System MUST treat the ownership threshold as a minimum standard and MUST require deeper inquiry in high-risk scenarios.
- **REQ-puppet-003** [Part 2 Rec. 4, p.31] — Reporting institutions MUST conduct ongoing due diligence and MUST verify that declarations remain consistent with observed transactions and services.
- **REQ-puppet-004** [Part 3 Rec. 1, p.66] — Jurisdiction MUST perform a systematic risk analysis of cases in which corporate vehicles are misused, producing typologies that flag heightened risk to investigators and service providers.
- **REQ-puppet-005** [Part 3 Rec. 2, p.66] — Jurisdiction MUST define and identify shelf companies (via irregular activity, simultaneous changes in officers, prolonged dormancy) and flag them as higher-risk.
- **REQ-puppet-006** [Part 3 Rec. 3, p.67] — Jurisdiction MUST require registered members of a legal entity to declare whether they act on their own behalf or for an undisclosed beneficial owner; a "Declaration of Beneficial Ownership" form SHOULD be universally required.
- **REQ-puppet-007** [Part 3 Rec. 4, p.67] — Country MUST immobilise, dematerialise, or abolish bearer shares and share warrants.
- **REQ-puppet-008** [Part 3 Rec. 5, p.67] — Country SHOULD operate a platform that brings together law enforcement and TCSPs to share typologies and training on corporate-vehicle misuse.
- **REQ-puppet-009** [Part 4 Rec. 1, pp.103–104] — Registry MUST maintain at minimum: entity name + unique identifier + alternative names; incorporation/registration date; entity type; entity status with dissolution date if applicable; principal office address; registered office or agent; directors/managers/officers (natural-person full name, former names, nationality, residential address, DOB); full filing history; required annual returns; electronic copies of filings.
- **REQ-puppet-010** [Part 4 Rec. 2, p.104] — Registry MUST evolve from passive receiver of filings to active AML actor with capacity to run statistically significant random fact-checks and to enforce penalties.
- **REQ-puppet-011** [Part 4 Rec. 3, p.104] — Registry MUST be computerised and accessible online (not paper-based, not closed-network-only).
- **REQ-puppet-012** [Part 4 Rec. 4, pp.104–105] — Registry MUST support Boolean search across at minimum: natural persons (first/last name with returned addresses, filings, positions); company secretary/registered office/agent; shareholders; addresses; business activity; country of registration; date of registration; date of incorporation.
- **REQ-puppet-013** [Part 4 Rec. 5, p.105] — Jurisdiction MUST assign a unique identifier to every legal entity incorporated in the jurisdiction.
- **REQ-puppet-014** [Part 4 Rec. 6, p.105] — TCSPs MUST be subject to an effectively enforced AML compliance regime including licensing and a supervisory authority.
- **REQ-puppet-015** [Part 4 Rec. 7, p.105] — All service providers (FIs, TCSPs, administrators) MUST collect BO information when establishing the business relationship AND MUST monitor and update that information continually.
- **REQ-puppet-016** [Part 4 Rec. 8, p.106] — Documented particulars of a legal entity's organisation (including BO information, all powers granted to non-officers, all banking documents) MUST be held physically or electronically within the jurisdiction of incorporation.
- **REQ-puppet-017** [Part 4 Rec. 9, p.106] — Non-residents forming a legal entity, or subsequently taking BO of one, MUST act through a service provider regulated under the domestic AML compliance regime, which MUST hold a certified ID copy and proof of address.
- **REQ-puppet-018** [Part 4 Rec. 10, p.106] — Country MUST clarify the scope of attorney-client privilege; privilege MUST NOT cover the identity of a client or services that are financial/fiduciary rather than advocacy.
- **REQ-puppet-019** [Part 4 Rec. 11, pp.106–107] — Financial institutions MUST gather BO information AND maintain complete BO compliance files including records of payments, fund routing, account control, and control of the controller.
- **REQ-puppet-020** [Part 4 Rec. 13, p.107] — FI CDD MUST aim at identifying the natural person who has ultimate control over the corporate vehicle's accounts (not merely the formal account holder), including screening of all signatories and powers of attorney.

## Source: Open Ownership Principles (January 2023)

- **REQ-oop-001** [Principle 1 — Definition] — Country MUST define beneficial ownership in primary legislation as a natural person, with a single unified definition referenced by all secondary legislation.
- **REQ-oop-002** [Principle 1 — Definition] — Definition MUST include a broad catch-all formulation plus a non-exhaustive list of example ways BO can be held.
- **REQ-oop-003** [Principle 1 — Definition] — Definition MUST explicitly disqualify agents, custodians, intermediaries, and nominees as the terminal beneficial owner.
- **REQ-oop-004** [Principle 1 — Definition] — Where the BO criteria are met by multiple individuals acting jointly, each MUST be recorded as a beneficial owner with combined ownership assumed in full.
- **REQ-oop-005** [Principle 2 — Coverage] — Disclosure requirements MUST apply to all corporate vehicles by default; exemptions MUST be clearly defined, justified, periodically reassessed, and narrowly interpreted.
- **REQ-oop-006** [Principle 2 — Coverage] — Even exempt entities MUST file a declaration stating the basis of their exemption.
- **REQ-oop-007** [Principle 3 — Detail] — Declaration MUST collect data on (a) the beneficial owner, (b) the means through which ownership/control is held, and (c) the declaring corporate vehicle and the individual submitting the declaration.
- **REQ-oop-008** [Principle 3 — Detail] — Declarations MUST be collected through standardised online forms with clear guidance to facilitate compliance.
- **REQ-oop-009** [Principle 3 — Detail] — Where BO is expressed as a percentage of ownership/control, the registry MUST collect absolute values (not ranges).
- **REQ-oop-010** [Principle 3 — Detail] — Where BO is held indirectly, the registry MUST capture sufficient information to reconstruct the full ownership chain.
- **REQ-oop-011** [Principle 4 — Central register] — BO disclosures MUST be collated in a central register that serves as the authoritative source with a designated responsible body.
- **REQ-oop-012** [Principle 5 — Access] — All government users with policy-justified need MUST have direct, rapid, per-record AND bulk access to BO data, searchable by both vehicle name and beneficial-owner name.
- **REQ-oop-013** [Principle 5 — Access] — The public MUST have free-of-charge access to a clearly defined subset of BO information sufficient for meaningful use, subject to legitimate-interest balancing where required.
- **REQ-oop-014** [Principle 5 — Access] — Where data is withheld under a protection regime, the public record MUST state why information is missing.
- **REQ-oop-015** [Principle 6 — Structured data] — BO data MUST be collected, stored, and shared in structured, machine-readable form using clear identifiers (LEI for entities, TIN/equivalent for persons) per a published data template.
- **REQ-oop-016** [Principle 6 — Structured data] — Registry MUST make data available via per-record browse, bulk download, and an API.
- **REQ-oop-017** [Principle 6 — Structured data] — Every change to a record MUST capture date and reason for the change to create an auditable record.
- **REQ-oop-018** [Principle 7 — Verification] — Submitted BO data MUST be verified against (a) known patterns, (b) cross-checks against existing authoritative government registers, and (c) supporting evidence checked against original documents.
- **REQ-oop-019** [Principle 7 — Verification] — Post-submission, the responsible agency MUST proactively check, query, remove, or update data, with the legal mandate and powers to do so.
- **REQ-oop-020** [Principle 7 — Verification] — Discrepancy-reporting mechanisms MUST be in place; ownership types difficult or impossible to verify (e.g., bearer shares) MUST be prohibited.
- **REQ-oop-021** [Principle 8 — Up-to-date and historical] — Initial registration and subsequent changes MUST be submitted within a short, defined statutory time period after the change occurs.
- **REQ-oop-022** [Principle 8 — Up-to-date and historical] — BO data MUST be periodically confirmed as correct on at least an annual basis, even when unchanged.
- **REQ-oop-023** [Principle 8 — Up-to-date and historical] — Historical records MUST be retained for a reasonable specified number of years, including for dormant and dissolved corporate vehicles.
- **REQ-oop-024** [Principle 9 — Sanctions] — Sanctions MUST be defined for: non-submission, late submission, incomplete submission, incorrect submission, deliberately false submission, and persistent non-compliance.
- **REQ-oop-025** [Principle 9 — Sanctions] — Sanctions MUST cover the beneficial owner, the declaring person, the company officers, and the declaring corporate vehicle, and MUST include both administrative and criminal options.
- **REQ-oop-026** [Principle 9 — Sanctions] — Financial sanctions MUST be set sufficiently high to be dissuasive (not "cost of doing business") and MUST be complemented by non-financial sanctions.
- **REQ-oop-027** [Principle 9 — Sanctions] — A specific authority MUST be designated with the resources, mandate, and powers to enforce sanctions, with automation where possible.

# Phase 1D — Reference implementations + CEMAC/Cameroon corpus

## Source: UK PSC (Companies House People with Significant Control)

- **REQ-uk-psc-001** [gov.uk PSC guidance, Data Required] — Register MUST capture, for each natural-person PSC, full name, date of birth, nationality, country of residence, service address, home address (non-disclosed), and date PSC status commenced.
- **REQ-uk-psc-002** [gov.uk PSC, Nature of Control] — Register MUST categorise share/voting rights in three bands: over 25% to 50%, more than 50% to less than 75%, and 75% or more.
- **REQ-uk-psc-003** [gov.uk PSC, Conditions] — Register MUST record at least one "nature of control" per PSC from the codelist: shareholding, voting, director appointment, significant influence, or trust/partnership control.
- **REQ-uk-psc-004** [gov.uk PSC, Verification] — Each PSC MUST hold a personal identity-verification code issued by the registrar before details are accepted.
- **REQ-uk-psc-005** [gov.uk PSC, Deadlines] — Filer MUST notify the registrar of any PSC change within 14 days of confirmation, and the registrar MUST publish the update promptly.
- **REQ-uk-psc-006** [gov.uk PSC, Enforcement] — Non-disclosure or false disclosure MUST trigger criminal sanctions (imprisonment, fine, or both) enforceable against the company and its officers.
- **REQ-uk-psc-007** [gov.uk PSC, Notices] — Register MUST allow service of statutory notices to suspected PSCs with a one-month response deadline before sanctions attach.
- **REQ-uk-psc-008** [gov.uk PSC, Home Address] — Home address MUST be collected but MUST NOT appear on the public-facing register; only the service address is disclosed.

## Source: Open Ownership Principles + UK PSC critique

- **REQ-oo-principles-001** [OO Principles 2023, Definition] — Platform MUST adopt a single statutory definition of beneficial owner covering shareholding, voting rights, control by other means, and senior managing official fallback.
- **REQ-oo-principles-002** [OO Principles 2023, Coverage] — Register MUST cover the widest range of corporate vehicles in scope, including companies, trusts, foundations, cooperatives, NGOs, and foreign vehicles operating domestically.
- **REQ-oo-principles-003** [OO Principles 2023, Detail] — Each declaration MUST capture sufficient identifying detail to support downstream verification, matching, and sanctions screening of the named beneficial owner.
- **REQ-oo-principles-004** [OO Principles 2023, Central Register] — A single consolidated register MUST exist; fragmented per-sector registers do not satisfy the principle.
- **REQ-oo-principles-005** [OO Principles 2023, Access] — Register MUST publish data with role-tiered access (public, competent authority, obliged entity) with audit logging on every consultation.
- **REQ-oo-principles-006** [OO Principles 2023, Structured Data] — Register MUST publish in a machine-readable structured format conforming to a published schema, not only PDF or HTML.
- **REQ-oo-principles-007** [OO Principles 2023, Verification] — Responsible agency MUST hold legal mandate and powers to verify submissions against authoritative external systems and to flag, query, or remove suspect entries.
- **REQ-oo-principles-008** [OO Principles 2023, Up-to-Date] — Register MUST keep data current AND retain full historical records so prior beneficial-ownership states are reproducible at any past date.
- **REQ-oo-principles-009** [OO Principles 2023, Sanctions] — Sanctions MUST be effective, proportionate, dissuasive, and MUST cover non-submission, late submission, incomplete submission, and false submission, and MUST be actively enforced.
- **REQ-oo-principles-010** [OO Verification principle] — Register MUST prohibit ownership types that are difficult or impossible to verify (e.g., bearer shares).
- **REQ-oo-principles-011** [OO Verification principle] — Register MUST run pattern/format validation on submission and proactive post-submission integrity checks (cross-system reconciliation, anomaly detection).
- **REQ-oo-principles-012** [OO Verification principle] — Register MUST accept third-party discrepancy reports from obliged entities and treat them as actionable workflow inputs.

## Source: Open Ownership Beneficial Ownership Data Standard (BODS)

- **REQ-bods-001** [BODS schema, statement types] — Platform MUST model disclosures as three statement types: personStatement, entityStatement, and ownershipOrControlStatement.
- **REQ-bods-002** [BODS schema, statementID] — Every statement MUST carry a statementID that is globally unique, persistent, and stable across republications of the same fact.
- **REQ-bods-003** [BODS schema, identifiers array] — Person and entity statements MUST carry an identifiers array of (scheme, id) pairs supporting LEI, national-ID, and registry-number cross-referencing.
- **REQ-bods-004** [BODS schema, statementDate] — Every statement MUST record the date the fact was true (statementDate) distinct from publication date.
- **REQ-bods-005** [BODS schema, source] — Every statement MUST record source provenance (type of source, supporting document URL/hash, retrieval date, asserting party).
- **REQ-bods-006** [BODS schema, interests] — Ownership-or-control statements MUST carry an interests array of (type, share min/max/exact, startDate, endDate) drawn from the BODS interestType codelist.
- **REQ-bods-007** [BODS schema, annotations] — Statements SHOULD carry annotations recording verification status, redactions, and validator comments to expose audit trail.
- **REQ-bods-008** [BODS schema, publicationDetails] — Bulk publications MUST include publicationDetails (publisher, license, publicationDate) at the file level.
- **REQ-bods-009** [BODS conformance] — New publications SHOULD meet strict-mode conformance (full ISO dates, ISO 3166 country codes, fully-typed interests) rather than relaxed mode.
- **REQ-bods-010** [BODS chains] — Indirect ownership MUST be expressible as a chain of statements linked by statementID references, not as a single flattened percentage.

## Source: France Registre des Bénéficiaires Effectifs (INPI)

- **REQ-fr-rbe-001** [Decree 2017-1094 / INPI RBE] — All non-listed legal persons registered at the RCS MUST file a beneficial-ownership declaration with INPI within 15 days of registration formality.
- **REQ-fr-rbe-002** [INPI RBE, threshold] — Filing MUST identify any natural person holding directly or indirectly more than 25% of capital or voting rights, or otherwise exercising control over the entity.
- **REQ-fr-rbe-003** [INPI RBE, fields] — Declaration MUST include name, usage name, pseudonyms, date and place of birth, nationality, personal address, nature and extent of the beneficial interest, and date the person became a beneficial owner.
- **REQ-fr-rbe-004** [INPI RBE, updates] — Any modification of beneficial ownership MUST be filed within 30 days of the event giving rise to it.
- **REQ-fr-rbe-005** [INPI RBE, sanctions] — Failure to file or false declaration MUST be punishable by imprisonment up to six months and a fine up to EUR 7,500 for natural persons and EUR 37,500 for legal persons.
- **REQ-fr-rbe-006** [INPI RBE, access (post-CJEU 2022)] — Public access MUST be restricted to persons with legitimate interest; competent authorities and obliged entities retain full access.

## Source: Denmark CVR (Centrale Virksomhedsregister) reelle ejere

- **REQ-dk-cvr-001** [DBA reelle ejere guidance] — Register MUST treat 25% as a presumptive threshold but MUST NOT treat it as exclusive; control by other means (board appointment, voting agreements) MUST also trigger registration.
- **REQ-dk-cvr-002** [DBA reelle ejere fields] — Register MUST capture beneficial owner name, nationality, place of residence, country of residence, and the nature and extent of ownership/control relationship.
- **REQ-dk-cvr-003** [DBA CVR] — Beneficial ownership data MUST be centralised in one register covering both legal entities and legal arrangements (no per-sector silos).
- **REQ-dk-cvr-004** [DBA, post-AMLD6] — Public access MUST be limited to parties with legitimate interest after 1 September 2025; the platform MUST implement legitimate-interest gating with auditable approvals.
- **REQ-dk-cvr-005** [DBA reelle ejere, senior managing fallback] — If no natural person can be identified, the entity MUST register its senior managing official as fallback beneficial owner with explicit reason.

## Source: Slovakia RPVS (Register partnerov verejneho sektora)

- **REQ-sk-rpvs-001** [Act 315/2016] — Any entity contracting with the public sector MUST register its beneficial owners in RPVS before contract execution; failure to do so MUST void the contract.
- **REQ-sk-rpvs-002** [Act 315/2016, authorised person] — Registration MUST be submitted by an "authorised person" (lawyer, notary, bank, auditor, tax advisor) under a written mandate, not by the entity directly.
- **REQ-sk-rpvs-003** [Act 315/2016, attestation] — The authorised person MUST attest under personal professional liability that beneficial-ownership information is verified against source documents.
- **REQ-sk-rpvs-004** [Act 315/2016, verification deed] — A "verification document" (verifikačný dokument) MUST be filed describing the verification methodology, supporting evidence, and reasoning for each declared owner.
- **REQ-sk-rpvs-005** [Act 315/2016, re-verification] — Beneficial-ownership data MUST be re-verified on every change and at minimum annually with the financial-statement filing.
- **REQ-sk-rpvs-006** [Act 315/2016, sanctions] — Sanctions MUST include fines on the entity, statutory body members, and the authorised person personally for false attestation.

## Source: OpenCorporates entity-resolution patterns

- **REQ-oc-er-001** [OC reconciliation API docs] — Entity records MUST be keyed by the composite (jurisdiction_code, company_number); jurisdiction MUST be explicit on every entity reference.
- **REQ-oc-er-002** [OC knowledge base] — Company-number identifiers MUST be normalised (lowercase, strip spaces/dots/dashes) for matching while preserving the original-source format alongside.
- **REQ-oc-er-003** [OC primary/secondary identifiers] — Entity records SHOULD carry secondary identifiers (LEI, TIN, EIN, national-registry IDs) to enable cross-jurisdiction resolution.
- **REQ-oc-er-004** [OC reconciliation API] — Matching SHOULD apply name normalisation (legal-form stripping, diacritic folding) and produce confidence scores rather than binary matches.
- **REQ-oc-er-005** [OC blog, golden source] — Resolved entities MUST cite their authoritative source registry record; aggregated copies MUST NOT be treated as primary.

## Source: ISO 17442 LEI / GLEIF

- **REQ-iso17442-001** [ISO 17442] — Where a legal entity holds a Legal Entity Identifier, the LEI MUST be stored as a 20-character alphanumeric code conforming to ISO 17442 with a valid ISO 7064 mod-97-10 check digit.
- **REQ-iso17442-002** [GLEIF Level 1] — Platform MUST persist Level-1 "who is who" data when sourced from GLEIF: legal name, registered address, legal jurisdiction, entity legal form (ELF), registration authority, registration status.
- **REQ-iso17442-003** [GLEIF Level 2] — Platform SHOULD persist Level-2 "who owns whom" parent and ultimate-parent LEI references when available and reconcile them with locally declared beneficial-ownership chains.
- **REQ-iso17442-004** [GLEIF data quality] — LEI lookups MUST be revalidated periodically; lapsed or duplicate LEIs MUST be flagged as data-quality issues, not silently retained.

## Source: ISO 20275 Entity Legal Forms

- **REQ-iso20275-001** [ISO 20275:2017] — Entity records MUST express legal form as a four-character alphanumeric ELF code drawn from the GLEIF-maintained ISO 20275 codelist.
- **REQ-iso20275-002** [ISO 20275 codelist] — Legal-form coding MUST be jurisdiction-aware; the same ELF code MUST resolve to the native-language form name for the entity's jurisdiction.
- **REQ-iso20275-003** [GLEIF ELF maintenance] — Platform MUST track the published ELF codelist version and ingest updates published by GLEIF acting as Maintenance Agency.

## Source: CEMAC Règlement N°01/CEMAC/UMAC/CM (11 April 2016, revised by N°02/24 of 20 Dec 2024)

- **REQ-cemac-001** [Règlement 01/2016 Art. 23] — Obliged entities MUST perform customer due diligence including identifying the beneficial owner and verifying that identity using reliable independent source documents.
- **REQ-cemac-002** [Règlement 01/2016] — Member states MUST establish a register of beneficial owners of legal persons and legal arrangements established on their territory.
- **REQ-cemac-003** [Règlement 01/2016] — Legal persons MUST declare their beneficial owners and keep that information current; the obligation MUST attach at incorporation.
- **REQ-cemac-004** [Règlement 01/2016] — Competent authorities (FIU, supervisors, judicial authorities) MUST have timely access to adequate, accurate, and current beneficial-ownership information.
- **REQ-cemac-005** [Règlement 02/2024, revision] — Platform MUST track the 2024 revision aligning CEMAC AML/CFT with revised FATF Recommendations 24/25 and apply its tightened standards on legal arrangements and nominee disclosures.

## Source: COBAC supervisory expectations

- **REQ-cobac-001** [COBAC supervisory framework] — Banks under COBAC supervision MUST hold verified beneficial-ownership information on all business account holders before account opening.
- **REQ-cobac-002** [COBAC] — Banks MUST be able to demonstrate independent verification of declared beneficial ownership and SHOULD reconcile against the national BO register where available.
- **REQ-cobac-003** [COBAC] — The platform SHOULD provide a supervised-institution integration channel allowing COBAC-licensed banks to query and reconcile against the register as part of KYC refresh cycles.
- **REQ-cobac-004** [COBAC on-site supervision] — Platform MUST log all banking-supervisor consultations to support COBAC on-site inspections and prudential review.

## Source: GABAC mutual-evaluation criteria (FATF-style regional body)

- **REQ-gabac-001** [GABAC ME methodology, R.24] — Platform MUST satisfy FATF Recommendation 24 as assessed by GABAC: adequate, accurate, current beneficial-ownership information on legal persons available to competent authorities.
- **REQ-gabac-002** [GABAC ME, R.25] — Platform MUST satisfy FATF Recommendation 25 on legal arrangements (trusts, fiducies) including identification of settlor, trustee, protector, beneficiaries, and any other natural person exercising effective control.
- **REQ-gabac-003** [GABAC ME, Immediate Outcome 5] — Platform MUST demonstrate that beneficial-ownership information is actually used by competent authorities, not merely collected (effectiveness, not just technical compliance).
- **REQ-gabac-004** [GABAC ME] — Cameroon's increased-monitoring status MUST be treated as a binding compliance driver; deficiencies cited in mutual evaluation reports MUST appear as remediation backlog with owners and deadlines.
- **REQ-gabac-005** [GABAC, sanctions] — Sanctions for non-disclosure MUST be applied and the platform MUST publish aggregate enforcement metrics annually for GABAC follow-up reporting.

## Source: Cameroon ANIF + DGI beneficial-ownership regime

- **REQ-cm-anif-001** [Décret 2005/187] — ANIF MUST receive, analyse, and where appropriate transmit suspicious-transaction reports to judicial authorities; the platform MUST expose a structured channel for ANIF to query beneficial-ownership data.
- **REQ-cm-anif-002** [Art. L8 quinquies CGI; Décret 2023/06801/CAB/PM] — All legal persons, collective investment funds, NGOs, and foreign legal constructs operating in Cameroon MUST file a beneficial-ownership declaration with the central register.
- **REQ-cm-anif-003** [Art. L8 quinquies CGI] — Declaration MUST identify any natural person holding 20% or more of capital or voting rights (direct or indirect), exercising effective control, holding principal management positions, or bearing unlimited joint liability.
- **REQ-cm-anif-004** [Décret 2023/06801/CAB/PM] — Initial declaration MUST be filed upon submission of the entity's existence declaration; foreign legal constructs MUST file within 30 days of establishment in Cameroon.
- **REQ-cm-anif-005** [Décret 2023/06801/CAB/PM] — Changes to beneficial ownership MUST be filed within 45 days of the modification; annual reaffirmation MUST accompany the annual tax filing.
- **REQ-cm-anif-006** [Décret 2023/06801/CAB/PM, sanctions] — Non-compliance with identification, maintenance, or update obligations MUST be sanctioned up to 5,000,000 FCFA; late filing MUST be sanctioned at 1,000,000 FCFA.
- **REQ-cm-anif-007** [Décision MINFI 00000723 of 21 Oct 2022] — Where an obliged entity cannot identify the beneficial owner despite diligence, it MUST refuse to establish or continue the business relationship and MUST file a suspicious-transaction report with ANIF.
- **REQ-cm-anif-008** [DGI Registre Central guide] — The Direction Générale des Impôts MUST administer the central register; the platform MUST support DGI-led compliance workflows including reminder, sanction, and reconciliation with annual tax filings.
- **REQ-cm-anif-009** [Cameroon AML/CFT framework] — Platform MUST expose audit-grade evidence that ANIF, DGI, judicial authorities, and supervised obliged entities each access only the data their mandate permits, with cryptographic provenance on every consultation.

# Phase 1B — BODS + Open Ownership + EU corpus

This section enumerates concrete, testable requirements extracted from the
Beneficial Ownership Data Standard, Open Ownership principles, OECD/Global
Forum implementation guidance, the EU AML directive corpus (4AMLD, 5AMLD,
6AMLD), the AMLR Regulation, the CJEU WM/Sovim ruling, and the EITI Standard.
Each requirement is testable in the sense that an auditor can ask "does this
exist? show me." against the RÉCOR platform.

## Source: BODS v0.4 — Beneficial Ownership Data Standard

- **REQ-BODS-001** [schema/reference §personStatement] — The platform MUST publish a `personStatement` for every beneficial owner with the required `isComponent` boolean and `personType` enum from the personType codelist.
- **REQ-BODS-002** [schema/reference §personStatement] — Where a `personType` value is `anonymousPerson` or `unknownPerson`, the statement MUST include `unspecifiedPersonDetails` explaining why.
- **REQ-BODS-003** [schema/reference §entityStatement] — Every `entityStatement` MUST carry an `isComponent` boolean and an `entityType` object whose nested `type` field is drawn from the entityType codelist.
- **REQ-BODS-004** [schema/reference §entityStatement] — Where `entityType.subtype` is populated it MUST be a value compatible with the parent `entityType.type` per the codelist alignment.
- **REQ-BODS-005** [schema/reference §relationshipStatement] — Every relationship statement MUST contain `subject` (recordId resolving to an entity), `interestedParty` (recordId or UnspecifiedRecord), and a non-empty `interests` array.
- **REQ-BODS-006** [schema/reference §relationshipStatement] — The `subject` of a relationship statement MUST resolve to an entity record; it MUST NOT be a person record.
- **REQ-BODS-007** [schema/reference §relationshipStatement] — When intermediaries exist in the ownership chain, the relationship MUST carry `componentRecords` listing the recordIds of every component relationship.
- **REQ-BODS-008** [schema/reference §publicationDetails] — Every published record MUST include `publicationDate` (YYYY-MM-DD or RFC 3339 date-time), `bodsVersion` (major.minor), and a `publisher` object with a `name`.
- **REQ-BODS-009** [schema/reference §publicationDetails] — Each record SHOULD declare an open `license` URI consistent with public-data publication.
- **REQ-BODS-010** [schema/reference §recordId] — Every statement MUST carry a `recordId` unique within the publisher's namespace.
- **REQ-BODS-011** [schema/reference §interest] — Each `interest` object MUST set `type` from the interestType codelist (shareholding, votingRights, appointmentOfBoard, etc.).
- **REQ-BODS-012** [schema/reference §interest] — `directOrIndirect` MUST be set to `indirect` whenever intermediaries are present in the chain; otherwise `direct` or `unknown`.
- **REQ-BODS-013** [schema/reference §interest] — When `beneficialOwnershipOrControl` is `true`, the `interestedParty` MUST be a natural person record.
- **REQ-BODS-014** [schema/reference §interest] — Interest `share` objects SHOULD express percentages as numeric values and support `minimum`, `maximum`, or `exact` ranges.
- **REQ-BODS-015** [schema/reference §interest] — Interest `startDate` and `endDate` MUST be ISO 8601 dates (YYYY-MM-DD).
- **REQ-BODS-016** [schema/reference §address] — Each address object MUST carry a `type` drawn from the addressType codelist and SHOULD include a `country` object.
- **REQ-BODS-017** [schema/reference §identifier] — Every identifier object MUST include at least one of `scheme` or `schemeName`; for entities, `scheme` MUST come from org-id.guide.
- **REQ-BODS-018** [schema/reference §identifier] — Person identifiers SHOULD follow the `{JURISDICTION}-{TYPE}` pattern with TYPE drawn from {PASSPORT, TAXID, IDCARD}.
- **REQ-BODS-019** [schema/reference §name] — Each name object MUST contain `fullName`; structured breakdowns (`familyName`, `givenName`, `patronymicName`) SHOULD be supplied where culturally appropriate.
- **REQ-BODS-020** [schema/reference §country] — Every country/jurisdiction object MUST include `name` and SHOULD include an ISO 3166-1 alpha-2 code.
- **REQ-BODS-021** [schema/reference §isComponent] — Component records (`isComponent: true`) MUST be serialised before the primary records that reference them in any stream or package.
- **REQ-BODS-022** [schema/reference §replaces] — Updated statements MUST carry a `replaces` array of the prior statementIDs they supersede so historical lineage is preserved.
- **REQ-BODS-023** [schema/reference §source] — Each statement SHOULD carry a `source` object with `type` (selfDeclaration, officialRegister, thirdParty, primaryResearch, etc.) and a retrievedAt timestamp.
- **REQ-BODS-024** [schema/reference §politicalExposure] — Person statements MUST declare `politicalExposure.status` as one of `isPep`, `isNotPep`, or `unknown`.
- **REQ-BODS-025** [schema §statementID] — Every statement MUST carry a globally unique `statementID` stable across re-publication of the same fact.

## Source: Open Ownership — Principles for Effective Beneficial Ownership Disclosure

- **REQ-OO-001** [Principle 1: Definition] — National law MUST define beneficial ownership in terms consistent across every disclosure regime and obliged sector.
- **REQ-OO-002** [Principle 1: Definition] — The BO definition MUST cover both ownership interests and control through other means (board appointment, contractual rights, dominant influence).
- **REQ-OO-003** [Principle 2: Coverage] — Disclosure obligations MUST apply to the widest possible range of corporate vehicles (companies, partnerships, foundations, trusts, SOEs, foreign entities operating domestically).
- **REQ-OO-004** [Principle 3: Detail] — Disclosed information MUST be specific enough to uniquely identify the BO and the nature/extent of their interest (no class-only or unspecified disclosures except temporary).
- **REQ-OO-005** [Principle 4: Central register] — A single, consolidated central repository MUST hold all BO disclosures within the jurisdiction.
- **REQ-OO-006** [Principle 5: Access] — Access tiers MUST be defined in law for authorities, obliged entities, and (subject to the WM/Sovim balancing test) persons with legitimate interest.
- **REQ-OO-007** [Principle 6: Structured data] — BO information MUST be published in a structured, machine-readable format (BODS recommended) enabling automated processing.
- **REQ-OO-008** [Principle 7: Verification] — The register operator MUST verify submitted BO information against independent authoritative sources (national ID, corporate register, tax records).
- **REQ-OO-009** [Principle 8: Up-to-date data] — The register MUST capture changes within a defined short window and MUST preserve historical records of every change.
- **REQ-OO-010** [Principle 9: Sanctions and enforcement] — National law MUST impose proportionate, dissuasive sanctions for non-disclosure, late disclosure, and false disclosure, and MUST provide an enforcement body.
- **REQ-OO-011** [Principles synthesis] — Open-source tools and a visualisation library SHOULD be provided to enable consumer reuse of the BO data.

## Source: OECD/Global Forum — Beneficial Ownership Implementation Toolkit (2nd ed., 2024)

- **REQ-OECD-001** [Toolkit Ch. 1] — Legislation MUST establish a BO definition aligned with FATF Recommendation 24 covering natural persons who ultimately own or control a legal vehicle.
- **REQ-OECD-002** [Toolkit Ch. 2] — The BO regime MUST cover both legal persons (companies, partnerships, foundations) and legal arrangements (express trusts, fiducies, similar).
- **REQ-OECD-003** [Toolkit Ch. 3] — A public authority or body MUST function as the beneficial ownership registry, or an alternative mechanism enabling efficient access by competent authorities (per FATF R24 amended).
- **REQ-OECD-004** [Toolkit Ch. 4] — Legal entities MUST be required by law to obtain, hold, and update adequate, accurate, and current BO information.
- **REQ-OECD-005** [Toolkit Ch. 5] — Identification documentation submitted by entities MUST be verifiable against official ID, residency, and corporate-registry sources.
- **REQ-OECD-006** [Toolkit Ch. 6] — The register operator MUST run verification mechanisms (data cross-checks, sample audits, on-site inspections) commensurate with risk.
- **REQ-OECD-007** [Toolkit Ch. 7] — Tax authorities MUST have direct, timely access to BO information for both domestic enforcement and international exchange.
- **REQ-OECD-008** [Toolkit Ch. 7] — BO information MUST be available for international exchange of information on request (EOIR) and where applicable for automatic exchange.
- **REQ-OECD-009** [Toolkit Ch. 8] — Sanctions for non-compliance MUST be proportionate, dissuasive, and effective against both the legal entity and the responsible natural persons.
- **REQ-OECD-010** [Toolkit Ch. 9] — Supervision of compliance MUST be assigned to a named authority with adequate powers, resources, and operational independence.
- **REQ-OECD-011** [Toolkit Ch. 10] — Technology underpinning the register MUST guarantee data integrity, audit logs, and a defined retention period (typically minimum five years after entity dissolution).
- **REQ-OECD-012** [Toolkit Ch. 10] — Jurisdictions SHOULD adopt an open data standard (e.g., BODS) to support interoperability of BO information.

## Source: EU Directive 2015/849 — Fourth Anti-Money Laundering Directive (4AMLD)

- **REQ-4AMLD-001** [Art. 3(6)(a)] — National law MUST define beneficial owner as the natural person(s) who ultimately owns or controls a corporate entity through direct or indirect ownership of a sufficient percentage of shares or voting rights.
- **REQ-4AMLD-002** [Art. 3(6)(a)(i)] — A shareholding of 25% plus one share, or an ownership interest of more than 25%, held by a natural person MUST be treated as an indication of direct/indirect ownership.
- **REQ-4AMLD-003** [Art. 3(6)(a)(ii)] — Where no natural person is identified after exhaustive analysis, the senior managing official MUST be recorded as BO, with that fallback documented.
- **REQ-4AMLD-004** [Art. 30(1)] — Corporate and other legal entities incorporated in the territory MUST obtain and hold adequate, accurate, and current information on their beneficial ownership.
- **REQ-4AMLD-005** [Art. 30(3)] — That information MUST be held in a central register (commercial register, companies register, or a dedicated public register).
- **REQ-4AMLD-006** [Art. 30(1)] — The information held MUST include, at minimum, the BO's name, month and year of birth, nationality, country of residence, and the nature and extent of beneficial interest.
- **REQ-4AMLD-007** [Art. 30(5)] — Competent authorities and FIUs MUST have timely, unrestricted access to the central BO register without alerting the entity concerned.
- **REQ-4AMLD-008** [Art. 30(5)(b)] — Obliged entities MUST be able to access the central BO register when conducting customer due diligence.
- **REQ-4AMLD-009** [Art. 30(5)(c) — original] — Persons or organisations demonstrating a legitimate interest MUST be granted access to BO information (as enacted; modified by 5AMLD and constrained by WM/Sovim ruling).
- **REQ-4AMLD-010** [Art. 31] — Trustees of express trusts MUST obtain and hold adequate, accurate, and up-to-date BO information on settlor, trustee(s), protector (if any), beneficiaries (or class), and any other person exercising effective control.
- **REQ-4AMLD-011** [Art. 31(4)] — Where the trust generates tax consequences, BO information MUST be held in a central register accessible to competent authorities and FIUs.

## Source: EU Directive 2018/843 — Fifth Anti-Money Laundering Directive (5AMLD)

- **REQ-5AMLD-001** [Art. 30(5)(c) amended] — Member States MUST (as legislated; later invalidated for general public by C-37/20) ensure BO information is accessible to any member of the general public without requiring legitimate interest.
- **REQ-5AMLD-002** [Art. 30(5a)] — A general-public requester MUST be able to access at minimum the BO's name, month and year of birth, country of residence, nationality, and nature/extent of interest.
- **REQ-5AMLD-003** [Art. 30(5a)] — Member States MAY make access conditional on online registration and payment of a fee not exceeding administrative cost.
- **REQ-5AMLD-004** [Art. 30(9)] — Member States MAY provide exemptions from access on a case-by-case basis where it would expose the BO to disproportionate risk of fraud, kidnapping, blackmail, extortion, harassment, violence, or intimidation, or where the BO is a minor or otherwise legally incapable.
- **REQ-5AMLD-005** [Art. 30(10)] — National BO registers MUST be interconnected via the European Central Platform (BORIS) for cross-border access.
- **REQ-5AMLD-006** [Art. 30(4)] — Member States MUST require mechanisms to ensure the information in the register is adequate, accurate, and current, including obligations on obliged entities and competent authorities to report discrepancies.
- **REQ-5AMLD-007** [Art. 31] — Trusts and similar legal arrangements MUST be registered with BO data even where they generate no tax consequences, where the trustee is established or resident in the Member State.
- **REQ-5AMLD-008** [Art. 31(3a)] — BO information on trusts and similar arrangements MUST be held in a central register and interconnected via BORIS.
- **REQ-5AMLD-009** [Art. 31(4)] — Access to trust BO information MUST be granted to competent authorities, FIUs, obliged entities, and (subject to safeguards) persons with legitimate interest.

## Source: EU Directive 2024/1640 — Sixth Anti-Money Laundering Directive (6AMLD)

- **REQ-6AMLD-001** [Art. 10] — Member States MUST ensure BO information of legal entities, legal arrangements, nominee arrangements, and foreign legal entities/arrangements with sufficient nexus is held in a central register.
- **REQ-6AMLD-002** [Art. 10] — Entities in charge of central registers MUST carry out their functions free from undue political or industry influence and MUST have a conflict-of-interest policy.
- **REQ-6AMLD-003** [Art. 10] — The central register MUST be operated by a named public-sector body (or designated public-private body subject to public oversight).
- **REQ-6AMLD-004** [Art. 10] — Member States MUST require legal entities to provide BO information within a short defined window of incorporation and updates within a similarly short window after change.
- **REQ-6AMLD-005** [Art. 11] — Register operators MUST verify submitted BO information within a reasonable time after submission against authoritative sources.
- **REQ-6AMLD-006** [Art. 11] — Where verification finds inconsistencies, errors, or non-compliance, the operator MUST be empowered to withhold or suspend proof of registration in the register.
- **REQ-6AMLD-007** [Art. 11] — Discrepancy reports from obliged entities and competent authorities MUST be investigated and resolved within a defined time-bound process.
- **REQ-6AMLD-008** [Art. 12] — Persons demonstrating a legitimate interest in combating money laundering and terrorist financing MUST be granted immediate, unfiltered, direct, and free access to BO information.
- **REQ-6AMLD-009** [Art. 12] — Journalists, civil-society organisations, academic researchers, and persons assessing counterparties before transactions MUST be presumed to have legitimate interest, subject to evidentiary support.
- **REQ-6AMLD-010** [Art. 12] — The register operator MUST verify the legitimate-interest claim based on documents and information the applicant provides.
- **REQ-6AMLD-011** [Art. 13] — Competent authorities (FIUs, supervisors, tax, customs, law enforcement, AMLA) MUST have immediate, unfiltered, direct, and free access, including bulk and historical access.
- **REQ-6AMLD-012** [Art. 14] — Mutual recognition of legitimate-interest determinations across Member States MUST be established, with secure information transfer between BO registers.
- **REQ-6AMLD-013** [Art. 10] — BO information MUST be retained for at least five years (and up to ten under national law) after dissolution of the legal entity or termination of the arrangement.
- **REQ-6AMLD-014** [Art. 16] — Sanctions for non-compliance with BO registration obligations MUST be effective, proportionate, dissuasive, and applied to both the entity and the responsible natural persons.
- **REQ-6AMLD-015** [Art. 10] — National registers MUST remain interconnected via BORIS, exposing structured BO data to other Member States' competent authorities and persons with verified legitimate interest.
- **REQ-6AMLD-016** [Art. 10] — Foreign legal entities or arrangements that acquire real estate, are awarded public contracts above a threshold, or engage in high-risk transactions in the Member State MUST register their BO.
- **REQ-6AMLD-017** [Art. 10] — Nominee arrangements (nominee shareholders, nominee directors) MUST be disclosed in the central register including the identity of the nominator.
- **REQ-6AMLD-018** [Art. 15] — Member States MAY exempt BO information from public-interest access where the BO is a minor, legally incapable, or the disclosure would expose them to disproportionate risk; exemptions MUST be reviewed regularly.
- **REQ-6AMLD-019** [Art. 17] — Member States MUST notify the Commission of the designated register operator and competent supervisory body.

## Source: EU Regulation 2024/1624 — Anti-Money Laundering Regulation (AMLR)

- **REQ-AMLR-001** [Art. 51] — Legal entities MUST identify their beneficial owners through both ownership-interest analysis and control analysis, applied independently.
- **REQ-AMLR-002** [Art. 52] — The ownership-interest threshold MUST be set at 25% or more (direct or indirect); the European Commission MAY lower the threshold to 15% for high-risk sectors.
- **REQ-AMLR-003** [Art. 52] — Indirect ownership MUST be calculated by multiplying chains of ownership and aggregating results across chains.
- **REQ-AMLR-004** [Art. 52] — Ownership analysis MUST include profit-participation and liquidation rights, not solely voting rights.
- **REQ-AMLR-005** [Art. 53] — Control MUST be identified where a person can dominate voting rights, appoint or remove a majority of management, exercise veto, or determine profit distribution, regardless of whether actually exercised.
- **REQ-AMLR-006** [Art. 54] — Where ownership and control coexist at different layers of a structure, BOTH the controllers of owning entities AND the owners of controlling entities MUST be identified as BOs.
- **REQ-AMLR-007** [Art. 55] — Where ownership structures involve trusts or similar legal arrangements, identification MUST look through to the natural persons exercising control over those arrangements.
- **REQ-AMLR-008** [Art. 56] — Legal entities and arrangements MUST notify the central register of BO information within the timeframes set by national law transposing 6AMLD.
- **REQ-AMLR-009** [Art. 57] — For legal entities similar to express trusts (foundations, fiducies), settlor-equivalent, trustee-equivalent, protector-equivalent, beneficiary, and any controlling person MUST be identified as BO.
- **REQ-AMLR-010** [Art. 58] — For express trusts, the settlor, trustee(s), protector (if any), each beneficiary (or class of beneficiaries), and any other natural person exercising effective control MUST be identified.
- **REQ-AMLR-011** [Art. 59-60] — Class-based identification of beneficiaries is permitted only temporarily and only for low-risk categories; discretionary trusts MUST identify objects of power and default takers.
- **REQ-AMLR-012** [Art. 61] — For collective investment undertakings, BOs are natural persons holding 25% or more of units OR persons defining investment policy.
- **REQ-AMLR-013** [Art. 62] — BO records MUST include exhaustive identity data, ownership/control description, structural mapping, and timestamps for each fact.
- **REQ-AMLR-014** [Art. 62] — Legal entities MUST update BO information within 28 days of any change and MUST confirm the information at least annually.
- **REQ-AMLR-015** [Art. 63] — Legal entities MUST obtain, maintain, and provide BO information upon request to competent authorities and obliged entities.
- **REQ-AMLR-016** [Art. 64] — Trustees of express trusts MUST identify, hold, and report BO information for every party to the trust.
- **REQ-AMLR-017** [Art. 65] — Limited exceptions from BO disclosure MAY apply only to entities subject to equivalent public-transparency obligations (e.g., listed on a regulated market).
- **REQ-AMLR-018** [Art. 66] — Nominee shareholders and nominee directors MUST disclose the identity of the true BO/nominator to the entity and to the register.
- **REQ-AMLR-019** [Art. 67] — Foreign legal entities and arrangements MUST register their BO when acquiring real estate in the EU, being awarded public contracts, or engaging in high-risk transactions.
- **REQ-AMLR-020** [Art. 68] — Sanctions for breach of these obligations MUST be effective, proportionate, dissuasive, and applied to both legal entity and responsible natural persons.

## Source: CJEU joined cases C-37/20 & C-601/20 — WM and Sovim v. Luxembourg Business Registers (22 Nov 2022)

- **REQ-WMSOVIM-001** [Operative §1] — National BO registers MUST NOT grant unconditional public access to BO data; the 5AMLD Art. 30(5)(c) general-public-access provision was declared invalid as incompatible with Charter Arts. 7 and 8.
- **REQ-WMSOVIM-002** [Reasoning §85-88] — Any access regime MUST satisfy the necessity test: access MUST be limited to what is strictly necessary to achieve the AML/CFT objective.
- **REQ-WMSOVIM-003** [Reasoning §86] — Difficulty in defining "legitimate interest" does NOT justify substituting unrestricted public access; the registry MUST implement a workable legitimate-interest gate.
- **REQ-WMSOVIM-004** [Reasoning §74-84] — Access provisions MUST be proportionate stricto sensu: the benefit to AML/CFT must outweigh the interference with privacy rights.
- **REQ-WMSOVIM-005** [Reasoning §76] — Making BO data publicly accessible constitutes a serious interference with the fundamental rights guaranteed by Charter Arts. 7 (private life) and 8 (personal data).
- **REQ-WMSOVIM-006** [Reasoning §82] — The registry MUST implement data-minimisation safeguards: only the data fields strictly necessary for the legitimate purpose may be disclosed to any given requester tier.
- **REQ-WMSOVIM-007** [Reasoning §87] — The registry MUST provide sufficient safeguards enabling data subjects to protect their personal data effectively against risks of abuse; optional case-by-case exemptions alone are insufficient.
- **REQ-WMSOVIM-008** [Reasoning §75] — Access MUST be presumptively restricted: disclosure granted only on demonstrated legitimate interest or specified authorisation, not by default.
- **REQ-WMSOVIM-009** [Reasoning §83] — The registry MUST log who accessed which BO record, when, and for what asserted purpose, to enable downstream accountability and abuse detection.

## Source: EITI Standard 2023 — Requirement 2.5 (Beneficial Ownership)

- **REQ-EITI-001** [Req. 2.5.a] — National law MUST require corporate entities that apply for or hold an exploration or production oil, gas, or mining licence or contract to disclose their beneficial owners.
- **REQ-EITI-002** [Req. 2.5.b] — The disclosed BO MUST be defined as the natural person(s) who directly or indirectly ultimately own(s) or control(s) the corporate entity.
- **REQ-EITI-003** [Req. 2.5.b] — The BO disclosure regime MUST adopt at least one ownership threshold; implementing countries are encouraged to adopt 10% or lower.
- **REQ-EITI-004** [Req. 2.5.c] — Disclosures MUST include the BO's name, nationality, country of residence, and identifying details (national ID, date of birth, address, contact info encouraged).
- **REQ-EITI-005** [Req. 2.5.c] — Disclosures MUST identify any politically exposed persons (PEPs) holding an interest, with full disclosure of PEP BOs regardless of ownership level.
- **REQ-EITI-006** [Req. 2.5.d] — BO information MUST be publicly available, in a machine-readable format, ideally integrated into existing corporate-regulator filings or stock-exchange platforms.
- **REQ-EITI-007** [Req. 2.5.e] — The multi-stakeholder group MUST agree an approach for corporate entities to assure the accuracy of submitted BO information (e.g., senior-management attestation, supporting documentation).
- **REQ-EITI-008** [Req. 2.5.f] — National policy on BO disclosure MUST be documented, including relevant legal provisions, actual disclosure practices, and any planned or in-progress reforms.
- **REQ-EITI-009** [Req. 2.5.g] — Sanctions MUST exist for non-disclosure or false disclosure of beneficial ownership in the extractive sector.
- **REQ-EITI-010** [Req. 7.2] — BO data made publicly available MUST comply with EITI Requirement 7.2 on data accessibility, including open licensing and machine-readable formats.

# Phase 1C — Security + Identity + Privacy corpus

## Source: OWASP ASVS 4.0.3 — V1 Architecture (L2/L3)

- **REQ-ASVS-001** [V1.1.1] — The platform MUST use a secure software development lifecycle that addresses security at every stage of development.
- **REQ-ASVS-002** [V1.1.2] — Threat modelling MUST be performed for every design change or sprint, identifying threats, countermeasures, risk responses, and security tests.
- **REQ-ASVS-003** [V1.1.3] — All user stories and features MUST contain functional security constraints.
- **REQ-ASVS-004** [V1.1.4] — Documentation MUST justify all trust boundaries, components, and significant data flows.
- **REQ-ASVS-005** [V1.1.5] — High-level architecture and all connected remote services MUST be subject to documented security analysis.
- **REQ-ASVS-006** [V1.1.6] — Security controls MUST be centralised, simple, vetted, secure, and reusable.
- **REQ-ASVS-007** [V1.1.7] — A secure-coding checklist, security requirements, guideline, or policy MUST be available to developers and testers.
- **REQ-ASVS-008** [V1.2.1] — All application components, services, and servers MUST use unique, special, low-privilege operating-system accounts.
- **REQ-ASVS-009** [V1.2.2] — Communications between application components (APIs, middleware, data layers) MUST be authenticated.
- **REQ-ASVS-010** [V1.2.3] — The platform MUST use a single vetted authentication mechanism known to be secure.
- **REQ-ASVS-011** [V1.2.4] — All authentication pathways and identity-management APIs MUST implement consistent authentication-security control strength.
- **REQ-ASVS-012** [V1.4.1] — Trusted enforcement points (gateways, servers, serverless functions) MUST enforce access controls.
- **REQ-ASVS-013** [V1.4.4] — A single well-vetted access-control mechanism MUST govern access to protected data and resources.
- **REQ-ASVS-014** [V1.4.5] — Attribute- or feature-based access control MUST be used so the code checks user authorization per feature/data item.
- **REQ-ASVS-015** [V1.5.1] — Input/output requirements MUST define how to handle and process data by type, content, and applicable laws.
- **REQ-ASVS-016** [V1.5.2] — Serialization MUST NOT be used when communicating with untrusted clients.
- **REQ-ASVS-017** [V1.5.3] — Input validation MUST be enforced on a trusted service layer.
- **REQ-ASVS-018** [V1.5.4] — Output encoding MUST occur close to or by the interpreter for which it is intended.
- **REQ-ASVS-019** [V1.6.1] — An explicit cryptographic key-management policy and key lifecycle following a key-management standard MUST exist.
- **REQ-ASVS-020** [V1.6.2] — Consumers of cryptographic services MUST protect key material via key vaults or API-based alternatives.
- **REQ-ASVS-021** [V1.6.3] — All keys and passwords MUST be replaceable as part of a defined re-encryption process.
- **REQ-ASVS-022** [V1.6.4] — The architecture MUST treat client-side secrets as insecure and never use them to protect sensitive data.
- **REQ-ASVS-023** [V1.7.1] — A common logging format and approach MUST be used across the system.
- **REQ-ASVS-024** [V1.7.2] — Logs MUST be securely transmitted to a preferably remote system for analysis, detection, alerting, and escalation.
- **REQ-ASVS-025** [V1.8.1] — All sensitive data MUST be identified and classified into protection levels.
- **REQ-ASVS-026** [V1.8.2] — Every protection level MUST have an associated set of protection requirements (encryption, integrity).
- **REQ-ASVS-027** [V1.9.1] — Inter-component communications MUST be encrypted, particularly across containers.
- **REQ-ASVS-028** [V1.9.2] — Components MUST verify the authenticity of each side of a communication link to prevent MITM attacks.
- **REQ-ASVS-029** [V1.10.1] — A source-code control system MUST be in use with check-ins tied to issues or change tickets.
- **REQ-ASVS-030** [V1.11.1] — All application components MUST be documented in terms of the business or security functions they provide.
- **REQ-ASVS-031** [V1.11.2] — All high-value business-logic flows MUST NOT share unsynchronised state.
- **REQ-ASVS-032** [V1.11.3] — (L3) All high-value business-logic flows MUST be thread-safe and resistant to TOCTOU race conditions.
- **REQ-ASVS-033** [V1.12.2] — User-uploaded files MUST be served as octet-stream downloads or from an unrelated domain.
- **REQ-ASVS-034** [V1.14.1] — Components of differing trust levels MUST be segregated by firewall rules, API gateways, or equivalent controls.
- **REQ-ASVS-035** [V1.14.2] — Binary signatures, trusted connections, and verified endpoints MUST be used to deploy binaries to remote devices.
- **REQ-ASVS-036** [V1.14.3] — The build pipeline MUST warn of out-of-date or insecure components and take appropriate action.
- **REQ-ASVS-037** [V1.14.4] — The build pipeline MUST automatically build and verify secure deployment of the application.
- **REQ-ASVS-038** [V1.14.5] — Deployments MUST be sandboxed, containerised, or network-isolated.
- **REQ-ASVS-039** [V1.14.6] — The application MUST NOT use unsupported, insecure, or deprecated client-side technologies.

## Source: OWASP ASVS 4.0.3 — V2 Authentication (L2/L3)

- **REQ-ASVS-040** [V2.1.1] — User-set passwords MUST be at least 12 characters in length.
- **REQ-ASVS-041** [V2.1.2] — Passwords of at least 64 characters MUST be permitted; passwords over 128 characters MUST be denied.
- **REQ-ASVS-042** [V2.1.3] — Password truncation MUST NOT be performed.
- **REQ-ASVS-043** [V2.1.4] — Any printable Unicode character MUST be permitted in passwords.
- **REQ-ASVS-044** [V2.1.7] — Submitted passwords MUST be checked against a breached-password list, locally or via API.
- **REQ-ASVS-045** [V2.1.9] — There MUST be no password composition rules limiting character types.
- **REQ-ASVS-046** [V2.1.10] — There MUST be no periodic credential-rotation or password-history requirements.
- **REQ-ASVS-047** [V2.2.1] — Anti-automation controls MUST mitigate credential testing, brute force, and account lockout abuse.
- **REQ-ASVS-048** [V2.2.2] — Weak authenticators MUST be limited to secondary verification, not primary authentication.
- **REQ-ASVS-049** [V2.2.3] — Secure notifications MUST be sent after authentication-detail updates.
- **REQ-ASVS-050** [V2.2.4] — (L2) Impersonation resistance MUST be provided via MFA or cryptographic devices.
- **REQ-ASVS-051** [V2.2.5] — (L2) Mutually-authenticated TLS MUST be in place between credential service provider and verifier.
- **REQ-ASVS-052** [V2.2.6] — (L2) Replay resistance MUST be provided via OTPs or cryptographic authenticators.
- **REQ-ASVS-053** [V2.2.7] — (L2) Authentication intent MUST be required (OTP entry or button press).
- **REQ-ASVS-054** [V2.3.1] — System-generated initial passwords MUST be securely randomised and expire quickly.
- **REQ-ASVS-055** [V2.4.1] — Passwords MUST be salted and hashed using an approved one-way key-derivation function.
- **REQ-ASVS-056** [V2.4.2] — Each salt MUST be at least 32 bits and unique per credential.
- **REQ-ASVS-057** [V2.4.3] — PBKDF2 iteration count MUST be at least 100,000.
- **REQ-ASVS-058** [V2.4.4] — Bcrypt work factor MUST be at least 10.
- **REQ-ASVS-059** [V2.4.5] — An additional key-derivation iteration with a secret salt stored separately MUST be applied.
- **REQ-ASVS-060** [V2.5.1] — System-generated activation secrets MUST NOT be sent in cleartext.
- **REQ-ASVS-061** [V2.5.2] — Password hints and knowledge-based authentication MUST NOT be present.
- **REQ-ASVS-062** [V2.5.4] — Shared or default accounts MUST NOT be present.
- **REQ-ASVS-063** [V2.5.5] — Users MUST be notified when authentication factors change or are replaced.
- **REQ-ASVS-064** [V2.5.6] — Recovery MUST use a secure mechanism (TOTP, soft token, or mobile push).
- **REQ-ASVS-065** [V2.7.2] — Out-of-band authentication requests MUST expire after 10 minutes.
- **REQ-ASVS-066** [V2.7.3] — Out-of-band codes MUST be usable only once for the originating request.
- **REQ-ASVS-067** [V2.7.5] — The verifier MUST retain only a hashed version of the authentication code.
- **REQ-ASVS-068** [V2.7.6] — The initial authentication code MUST be generated with at least 20 bits of entropy.
- **REQ-ASVS-069** [V2.8.1] — Time-based OTPs MUST have a defined lifetime before expiring.
- **REQ-ASVS-070** [V2.8.2] — Symmetric keys protecting OTPs MUST be stored in an HSM or secure storage.
- **REQ-ASVS-071** [V2.8.4] — Time-based OTPs MUST be usable only once within the validity period.
- **REQ-ASVS-072** [V2.9.1] — Cryptographic keys MUST be stored securely via TPM, HSM, or OS service.
- **REQ-ASVS-073** [V2.9.2] — Challenge nonces MUST be at least 64 bits in length and statistically unique.
- **REQ-ASVS-074** [V2.10.1] — Intra-service secrets MUST avoid unchanging credentials like passwords or API keys.
- **REQ-ASVS-075** [V2.10.4] — Passwords, integrations, and secrets MUST NOT be embedded in source code; a secure key store MUST be used.

## Source: OWASP ASVS 4.0.3 — V3 Session Management (L2/L3)

- **REQ-ASVS-076** [V3.1.1] — The application MUST NOT reveal session tokens in URL parameters.
- **REQ-ASVS-077** [V3.2.1] — A new session token MUST be generated on user authentication.
- **REQ-ASVS-078** [V3.2.2] — Session tokens MUST possess at least 64 bits of entropy.
- **REQ-ASVS-079** [V3.2.3] — Session tokens MUST be stored in the browser only via secure methods (secured cookies or HTML5 session storage).
- **REQ-ASVS-080** [V3.2.4] — (L2) Session tokens MUST be generated using approved cryptographic algorithms.
- **REQ-ASVS-081** [V3.3.1] — Logout and expiration MUST invalidate the session token so the back button or relying party cannot resume the session.
- **REQ-ASVS-082** [V3.3.2] — Re-authentication MUST occur after 12 hours or 30 minutes idle (L2); 15 minutes idle with 2FA (L3).
- **REQ-ASVS-083** [V3.3.3] — Users MUST be able to terminate all other active sessions after a successful password change.
- **REQ-ASVS-084** [V3.3.4] — Users MUST be able to view and log out of any or all currently active sessions and devices.
- **REQ-ASVS-085** [V3.4.1] — Cookie-based session tokens MUST set the Secure attribute.
- **REQ-ASVS-086** [V3.4.2] — Cookie-based session tokens MUST set the HttpOnly attribute.
- **REQ-ASVS-087** [V3.4.3] — Cookie-based session tokens MUST use the SameSite attribute to limit CSRF exposure.
- **REQ-ASVS-088** [V3.4.4] — Cookie-based session tokens MUST use the "__Host-" prefix.
- **REQ-ASVS-089** [V3.5.3] — Stateless session tokens MUST use digital signatures, encryption, and countermeasures against tampering and replay.
- **REQ-ASVS-090** [V3.7.1] — A full valid login session, re-authentication, or secondary verification MUST be required before sensitive transactions or account modifications.

## Source: OWASP ASVS 4.0.3 — V4 Access Control (L2/L3)

- **REQ-ASVS-091** [V4.1.1] — Access-control rules MUST be enforced on a trusted service layer.
- **REQ-ASVS-092** [V4.1.2] — User/data attributes and policy information used by access control MUST NOT be manipulable by end users without authorization.
- **REQ-ASVS-093** [V4.1.3] — The principle of least privilege MUST be enforced for all functions, data files, URLs, controllers, and services.
- **REQ-ASVS-094** [V4.1.5] — Access controls MUST fail securely, including when an exception occurs.
- **REQ-ASVS-095** [V4.2.1] — Sensitive data and APIs MUST be protected against IDOR attacks across CRUD operations.
- **REQ-ASVS-096** [V4.2.2] — A strong anti-CSRF mechanism MUST protect authenticated functionality; anti-automation MUST protect unauthenticated functionality.
- **REQ-ASVS-097** [V4.3.1] — Administrative interfaces MUST use appropriate multi-factor authentication.
- **REQ-ASVS-098** [V4.3.2] — Directory browsing MUST be disabled and file/directory metadata MUST NOT be discoverable.
- **REQ-ASVS-099** [V4.3.3] — Step-up or adaptive authentication and segregation of duties MUST be applied to high-value applications.

## Source: OWASP ASVS 4.0.3 — V5 Validation, Sanitization, Encoding (L2/L3)

- **REQ-ASVS-100** [V5.1.1] — The application MUST defend against HTTP parameter pollution.
- **REQ-ASVS-101** [V5.1.2] — Frameworks MUST protect against mass parameter assignment attacks, or fields MUST be marked private.
- **REQ-ASVS-102** [V5.1.3] — All input MUST be validated using positive validation (allow lists).
- **REQ-ASVS-103** [V5.1.4] — Structured data MUST be strongly typed and validated against a defined schema (characters, length, pattern).
- **REQ-ASVS-104** [V5.1.5] — URL redirects and forwards MUST only allow destinations on an allow list or display a warning.
- **REQ-ASVS-105** [V5.2.1] — Untrusted HTML input from WYSIWYG editors MUST be sanitised with an HTML sanitiser library.
- **REQ-ASVS-106** [V5.2.4] — Use of eval() or dynamic code execution MUST be avoided; any user input executed MUST be sanitised or sandboxed.
- **REQ-ASVS-107** [V5.2.5] — The application MUST protect against template-injection attacks by sanitising or sandboxing user input.
- **REQ-ASVS-108** [V5.2.6] — The application MUST protect against SSRF by validating untrusted URIs and using allow lists of protocols, domains, paths, and ports.
- **REQ-ASVS-109** [V5.3.1] — Output encoding MUST be relevant to the interpreter and context (HTML, JS, URL, headers, SMTP).
- **REQ-ASVS-110** [V5.3.3] — Context-aware output escaping MUST protect against reflected, stored, and DOM XSS.
- **REQ-ASVS-111** [V5.3.4] — Database queries MUST use parameterised queries, ORMs, or equivalent injection protection.
- **REQ-ASVS-112** [V5.3.8] — OS calls MUST use parameterised queries or contextual command-line output encoding to prevent OS command injection.
- **REQ-ASVS-113** [V5.3.9] — The application MUST protect against Local File Inclusion and Remote File Inclusion.
- **REQ-ASVS-114** [V5.5.1] — Serialised objects MUST use integrity checks or encryption to prevent hostile object creation.
- **REQ-ASVS-115** [V5.5.2] — XML parsers MUST be configured most-restrictively, disabling external-entity resolution to prevent XXE.
- **REQ-ASVS-116** [V5.5.3] — Deserialisation of untrusted data MUST be avoided or protected in custom code and third-party libraries.

## Source: OWASP ASVS 4.0.3 — V7 Error Handling and Logging (L2/L3)

- **REQ-ASVS-117** [V7.1.1] — The application MUST NOT log credentials or payment details; session tokens MUST only appear hashed.
- **REQ-ASVS-118** [V7.1.2] — The application MUST NOT log sensitive data as defined under local privacy laws or security policy.
- **REQ-ASVS-119** [V7.1.3] — Security-relevant events (auth, access-control failures, deserialisation, input-validation) MUST be logged.
- **REQ-ASVS-120** [V7.1.4] — Each log event MUST include information sufficient for a detailed investigation of the timeline.
- **REQ-ASVS-121** [V7.2.1] — All authentication decisions MUST be logged without storing sensitive tokens or passwords.
- **REQ-ASVS-122** [V7.2.2] — All access-control decisions MUST be loggable and all failed decisions MUST be logged.
- **REQ-ASVS-123** [V7.3.1] — All logging components MUST appropriately encode data to prevent log injection.
- **REQ-ASVS-124** [V7.3.3] — Security logs MUST be protected from unauthorized access and modification.
- **REQ-ASVS-125** [V7.3.4] — Time sources MUST be synchronised; UTC logging is strongly recommended for global systems.
- **REQ-ASVS-126** [V7.4.1] — A generic message MUST be shown for unexpected or security-sensitive errors, optionally with a unique support ID.

## Source: OWASP ASVS 4.0.3 — V8 Data Protection (L2/L3)

- **REQ-ASVS-127** [V8.1.1] — Sensitive data MUST be protected from caching in load balancers and application caches.
- **REQ-ASVS-128** [V8.1.2] — Cached or temporary copies of sensitive data MUST be protected or purged after the authorized user accesses them.
- **REQ-ASVS-129** [V8.1.4] — The application MUST detect and alert on abnormal request volumes per IP, user, or time window.
- **REQ-ASVS-130** [V8.2.1] — Sufficient anti-caching headers MUST be set so that sensitive data is not cached by modern browsers.
- **REQ-ASVS-131** [V8.2.2] — Sensitive data MUST NOT be stored in browser storage (localStorage, sessionStorage, IndexedDB, cookies).
- **REQ-ASVS-132** [V8.3.1] — Sensitive data MUST be sent via HTTP body or headers; query-string parameters MUST NOT contain sensitive data.
- **REQ-ASVS-133** [V8.3.2] — Users MUST be able to remove or export their data on demand.
- **REQ-ASVS-134** [V8.3.3] — Users MUST be given clear language about personal-data collection and provide opt-in consent.
- **REQ-ASVS-135** [V8.3.5] — (L2) Access to sensitive data MUST be audited without logging the sensitive data itself.
- **REQ-ASVS-136** [V8.3.6] — Sensitive information in memory MUST be overwritten with zeros or random data as soon as it is no longer required.
- **REQ-ASVS-137** [V8.3.7] — Sensitive data requiring encryption MUST be encrypted using approved algorithms providing confidentiality and integrity.
- **REQ-ASVS-138** [V8.3.8] — Sensitive personal information MUST be subject to data-retention classification and scheduled deletion.

## Source: OWASP ASVS 4.0.3 — V9 Communications (L2/L3)

- **REQ-ASVS-139** [V9.1.1] — TLS MUST be used for all client connectivity and MUST NOT fall back to insecure communications.
- **REQ-ASVS-140** [V9.1.2] — Only strong cipher suites MUST be enabled, with the strongest set as preferred.
- **REQ-ASVS-141** [V9.1.3] — Only the latest recommended TLS versions (1.2, 1.3) MUST be enabled.
- **REQ-ASVS-142** [V9.2.1] — Server connections MUST use trusted TLS certificates; internal CAs MUST be explicitly trusted.
- **REQ-ASVS-143** [V9.2.2] — TLS MUST be used for all inbound and outbound connections, including management, monitoring, database, and partner connections.
- **REQ-ASVS-144** [V9.2.3] — Encrypted connections to external systems involving sensitive information MUST be authenticated.
- **REQ-ASVS-145** [V9.2.4] — Proper certificate-revocation checking (e.g., OCSP stapling) MUST be enabled and configured.
- **REQ-ASVS-146** [V9.2.5] — Backend TLS connection failures MUST be logged.

## Source: OWASP ASVS 4.0.3 — V10 Malicious Code (L2/L3)

- **REQ-ASVS-147** [V10.2.1] — Source code and third-party libraries MUST NOT contain unauthorized phone-home or data-collection capability.
- **REQ-ASVS-148** [V10.2.2] — The application MUST NOT request unnecessary or excessive permissions to privacy features or sensors.
- **REQ-ASVS-149** [V10.3.1] — Auto-update features MUST obtain updates over secure channels and verify digital signatures.
- **REQ-ASVS-150** [V10.3.2] — The application MUST employ integrity protections such as code signing or subresource integrity.
- **REQ-ASVS-151** [V10.3.3] — The application MUST protect against subdomain takeovers where DNS entries are relied upon.

## Source: OWASP ASVS 4.0.3 — V11 Business Logic (L2/L3)

- **REQ-ASVS-152** [V11.1.1] — Business-logic flows MUST process steps in sequential order without skipping for the same user.
- **REQ-ASVS-153** [V11.1.2] — Business-logic flows MUST only process steps within realistic human time (no rapid submissions).
- **REQ-ASVS-154** [V11.1.3] — The application MUST enforce per-user limits for specific business actions or transactions.
- **REQ-ASVS-155** [V11.1.4] — Anti-automation controls MUST protect against excessive calls (mass exfiltration, mass uploads, DoS).
- **REQ-ASVS-156** [V11.1.5] — Business-logic limits/validation MUST address threats identified by threat modelling.
- **REQ-ASVS-157** [V11.1.6] — (L3) The application MUST NOT suffer from TOCTOU issues or race conditions for sensitive operations.
- **REQ-ASVS-158** [V11.1.7] — (L3) The application MUST monitor for unusual events or activity from a business-logic perspective.
- **REQ-ASVS-159** [V11.1.8] — (L3) Configurable alerting MUST trigger when automated attacks or unusual activity are detected.

## Source: OWASP ASVS 4.0.3 — V12 Files and Resources (L2/L3)

- **REQ-ASVS-160** [V12.1.1] — The application MUST NOT accept files large enough to fill storage or cause denial of service.
- **REQ-ASVS-161** [V12.1.2] — Compressed files MUST be checked against maximum uncompressed size and maximum file count before extraction.
- **REQ-ASVS-162** [V12.1.3] — A file-size quota and maximum file count per user MUST be enforced.
- **REQ-ASVS-163** [V12.2.1] — Files from untrusted sources MUST be validated as the expected type based on content.
- **REQ-ASVS-164** [V12.3.1] — User-submitted filename metadata MUST NOT be used directly by filesystems; a URL API MUST protect against path traversal.
- **REQ-ASVS-165** [V12.3.5] — Untrusted file metadata MUST NOT be passed directly to system APIs to prevent OS command injection.
- **REQ-ASVS-166** [V12.4.1] — Files from untrusted sources MUST be stored outside the web root with limited permissions.
- **REQ-ASVS-167** [V12.4.2] — Files from untrusted sources MUST be scanned by antivirus before serving.
- **REQ-ASVS-168** [V12.5.2] — Direct requests to uploaded files MUST never be executed as HTML/JavaScript.
- **REQ-ASVS-169** [V12.6.1] — The web/application server MUST be configured with an allow list of resources for outbound requests.

## Source: OWASP ASVS 4.0.3 — V13 API and Web Service (L2/L3)

- **REQ-ASVS-170** [V13.1.1] — All application components MUST use the same encodings and parsers to avoid parsing-difference attacks.
- **REQ-ASVS-171** [V13.1.3] — API URLs MUST NOT expose sensitive information such as API keys or session tokens.
- **REQ-ASVS-172** [V13.1.4] — Authorisation decisions MUST be enforced at both URI and resource levels.
- **REQ-ASVS-173** [V13.1.5] — Requests with unexpected or missing content types MUST be rejected with HTTP 406 or 415.
- **REQ-ASVS-174** [V13.2.1] — RESTful HTTP methods MUST be restricted to valid choices per user or action (e.g., no DELETE/PUT for normal users).
- **REQ-ASVS-175** [V13.2.2] — JSON-schema validation MUST be applied before accepting input.
- **REQ-ASVS-176** [V13.2.3] — RESTful services using cookies MUST be CSRF-protected via double-submit cookie, CSRF nonces, or Origin header checks.
- **REQ-ASVS-177** [V13.2.5] — REST services MUST explicitly check incoming Content-Type to match expected (e.g., application/json).
- **REQ-ASVS-178** [V13.2.6] — Message headers and payload MUST be transport-protected via TLS for confidentiality and integrity.
- **REQ-ASVS-179** [V13.4.1] — GraphQL queries MUST use a query allow-list or combined depth+amount limits to prevent DoS.
- **REQ-ASVS-180** [V13.4.2] — GraphQL authorisation logic MUST be implemented at the business-logic layer, not the GraphQL layer.

## Source: OWASP ASVS 4.0.3 — V14 Configuration (L2/L3)

- **REQ-ASVS-181** [V14.1.1] — Build and deployment MUST be secure and repeatable via CI/CD automation.
- **REQ-ASVS-182** [V14.1.2] — Compiler flags MUST enable all available buffer-overflow protections and warnings.
- **REQ-ASVS-183** [V14.1.3] — Server configuration MUST be hardened per application-server and framework recommendations.
- **REQ-ASVS-184** [V14.1.4] — Application, configuration, and dependencies MUST be re-deployable via automated scripts.
- **REQ-ASVS-185** [V14.1.5] — (L3) Authorised administrators MUST be able to verify integrity of all security-relevant configurations.
- **REQ-ASVS-186** [V14.2.1] — All components MUST be kept up to date, preferably via dependency checker during build.
- **REQ-ASVS-187** [V14.2.2] — Unneeded features, documentation, samples, and configurations MUST be removed.
- **REQ-ASVS-188** [V14.2.3] — Externally hosted application assets MUST use Subresource Integrity for validation.
- **REQ-ASVS-189** [V14.2.4] — Third-party components MUST come from predefined, trusted, continually maintained repositories.
- **REQ-ASVS-190** [V14.2.5] — A Software Bill of Materials (SBOM) MUST be maintained of all third-party libraries.
- **REQ-ASVS-191** [V14.3.2] — Debug modes MUST be disabled in production for both web/application server and framework.
- **REQ-ASVS-192** [V14.3.3] — HTTP responses MUST NOT expose detailed version information.
- **REQ-ASVS-193** [V14.4.1] — Every HTTP response MUST contain a Content-Type header matching the content.
- **REQ-ASVS-194** [V14.4.3] — A Content Security Policy response header MUST be in place to mitigate XSS impact.
- **REQ-ASVS-195** [V14.4.4] — All responses MUST contain X-Content-Type-Options: nosniff.
- **REQ-ASVS-196** [V14.4.5] — A Strict-Transport-Security header MUST be included on all responses and subdomains.
- **REQ-ASVS-197** [V14.4.6] — A suitable Referrer-Policy header MUST be included to avoid sensitive-info leakage.
- **REQ-ASVS-198** [V14.4.7] — Content MUST NOT be embeddable in third-party sites by default (frame-options).
- **REQ-ASVS-199** [V14.5.1] — The application server MUST accept only HTTP methods used by the application or API.
- **REQ-ASVS-200** [V14.5.2] — The Origin header MUST NOT be used for authentication or access-control decisions.
- **REQ-ASVS-201** [V14.5.3] — The CORS Access-Control-Allow-Origin header MUST use a strict allow list.
- **REQ-ASVS-202** [V14.5.4] — HTTP headers added by trusted proxies or SSO devices MUST be authenticated by the application.

## Source: OWASP API Security Top 10 (2023)

- **REQ-APITOP10-001** [API1:2023] — Object-level authorisation checks MUST be applied in every function that accesses a data source using an ID supplied by the user.
- **REQ-APITOP10-002** [API2:2023] — Authentication mechanisms MUST be robust against token compromise and implementation flaws enabling identity assumption.
- **REQ-APITOP10-003** [API3:2023] — Authorisation MUST be validated at the object-property level to prevent unauthorised property exposure or manipulation.
- **REQ-APITOP10-004** [API4:2023] — Resource-consumption controls MUST be in place to prevent exhaustion of network, CPU, memory, storage, or paid-service quota.
- **REQ-APITOP10-005** [API5:2023] — Access-control policies MUST cleanly separate administrative from regular functions to prevent privilege escalation.
- **REQ-APITOP10-006** [API6:2023] — Business-critical API flows MUST include protections against excessive automated use (anti-bot, anti-abuse).
- **REQ-APITOP10-007** [API7:2023] — User-supplied URIs MUST be validated; APIs MUST NOT send crafted requests to unintended destinations (SSRF).
- **REQ-APITOP10-008** [API8:2023] — API configuration and deployment settings MUST be reviewed and hardened; defaults MUST NOT be exposed.
- **REQ-APITOP10-009** [API9:2023] — A comprehensive inventory of API hosts, versions, and environments MUST be maintained; deprecated endpoints MUST be retired.
- **REQ-APITOP10-010** [API10:2023] — Data received from third-party APIs MUST be validated and sanitised with the same scrutiny as user input.

## Source: NIST SP 800-63B — AAL2 / AAL3 Authenticator Requirements

- **REQ-NIST63B-001** [§4.2.1] — AAL2 MUST permit multi-factor OTP, multi-factor cryptographic software/device, or memorised-secret combined with a possession-based authenticator.
- **REQ-NIST63B-002** [§4.2.2] — AAL2 authenticators procured by government agencies MUST be validated to FIPS 140 Level 1.
- **REQ-NIST63B-003** [§4.2.2] — At least one AAL2 authenticator MUST be replay-resistant.
- **REQ-NIST63B-004** [§4.2.2] — AAL2 communication MUST occur over an authenticated protected channel resistant to MitM attacks.
- **REQ-NIST63B-005** [§4.2.2] — AAL2 SHOULD demonstrate authentication intent from at least one authenticator.
- **REQ-NIST63B-006** [§4.2.3] — AAL2 sessions MUST re-authenticate every 12 hours and after 30 minutes of inactivity.
- **REQ-NIST63B-007** [§4.3.1] — AAL3 MUST require a multi-factor cryptographic device, or a single-factor cryptographic device with memorised secret, or hardware-OTP + cryptographic combination.
- **REQ-NIST63B-008** [§4.3.2] — AAL3 multi-factor authenticators MUST be FIPS 140 Level 2 overall with Level 3 physical security.
- **REQ-NIST63B-009** [§4.3.2] — At least one AAL3 cryptographic authenticator MUST be verifier-impersonation-resistant.
- **REQ-NIST63B-010** [§4.3.2] — At least one AAL3 cryptographic authenticator MUST be replay-resistant.
- **REQ-NIST63B-011** [§4.3.2] — AAL3 verifiers MUST be verifier-compromise-resistant with respect to at least one authentication factor.
- **REQ-NIST63B-012** [§4.3.2] — All AAL3 authentication and reauthentication processes MUST demonstrate authentication intent from at least one authenticator.
- **REQ-NIST63B-013** [§4.3.2] — AAL3 communication between claimant and verifier MUST occur over an authenticated protected channel.
- **REQ-NIST63B-014** [§4.3.3] — AAL3 sessions MUST re-authenticate every 12 hours and after 15 minutes of inactivity, requiring both factors.

## Source: NIST SP 800-207 — Zero Trust Architecture (tenets)

- **REQ-NIST207-001** [§2.1 Tenet 1] — All data sources and computing services MUST be classified as resources subject to zero-trust access controls.
- **REQ-NIST207-002** [§2.1 Tenet 2] — All communication MUST be secured regardless of network location (no implicit trust by topology).
- **REQ-NIST207-003** [§2.1 Tenet 3] — Access to individual enterprise resources MUST be granted on a per-session basis, with re-evaluation each session.
- **REQ-NIST207-004** [§2.1 Tenet 4] — Access decisions MUST be determined by dynamic policy including identity, application/service state, requesting-asset state, and environmental attributes.
- **REQ-NIST207-005** [§2.1 Tenet 5] — The enterprise MUST monitor and measure the integrity and security posture of all owned and associated assets.
- **REQ-NIST207-006** [§2.1 Tenet 6] — All resource authentication and authorisation MUST be dynamic and strictly enforced before access is allowed.
- **REQ-NIST207-007** [§2.1 Tenet 7] — The enterprise MUST collect telemetry on assets, network infrastructure, and communications and use it to improve security posture.
- **REQ-NIST207-008** [§2.2] — The enterprise MUST assume the entire network is not trusted; remote and local access alike MUST be authenticated and authorised.
- **REQ-NIST207-009** [§2.2] — The enterprise MUST assume devices on the network may not be owned or configurable by the enterprise; posture-checks MUST be enforced.
- **REQ-NIST207-010** [§2.2] — No resource may be inherently trusted; every access request MUST be evaluated by the policy decision point before authorisation.

## Source: GDPR — Article 5 (Principles)

- **REQ-GDPR-001** [Art. 5(1)(a)] — Personal data MUST be processed lawfully, fairly, and in a transparent manner in relation to the data subject.
- **REQ-GDPR-002** [Art. 5(1)(b)] — Personal data MUST be collected for specified, explicit, and legitimate purposes and not further processed incompatibly with those purposes.
- **REQ-GDPR-003** [Art. 5(1)(c)] — Personal data processed MUST be adequate, relevant, and limited to what is necessary for the stated purpose (data minimisation).
- **REQ-GDPR-004** [Art. 5(1)(d)] — Personal data MUST be accurate and kept up to date; inaccurate data MUST be erased or rectified without delay.
- **REQ-GDPR-005** [Art. 5(1)(e)] — Personal data MUST be kept in identifiable form no longer than necessary for the purposes (storage limitation).
- **REQ-GDPR-006** [Art. 5(1)(f)] — Personal data MUST be processed with appropriate security against unauthorised processing, accidental loss, destruction, or damage.
- **REQ-GDPR-007** [Art. 5(2)] — The controller MUST be responsible for and able to demonstrate compliance with all Article 5 principles (accountability).

## Source: GDPR — Article 25 (Data Protection by Design and by Default)

- **REQ-GDPR-008** [Art. 25(1)] — Controllers MUST implement appropriate technical and organisational measures both at time of design and at time of processing.
- **REQ-GDPR-009** [Art. 25(1)] — Measures MUST effectively implement data-protection principles such as data minimisation.
- **REQ-GDPR-010** [Art. 25(1)] — Necessary safeguards MUST be integrated into processing to meet GDPR requirements and protect data-subject rights.
- **REQ-GDPR-011** [Art. 25(2)] — By default only personal data necessary for each specific purpose MUST be processed (collection, extent, storage, accessibility).
- **REQ-GDPR-012** [Art. 25(2)] — By default personal data MUST NOT be accessible to an indefinite number of persons without individual intervention.
- **REQ-GDPR-013** [Art. 25(3)] — Approved certification under Article 42 MAY be used to demonstrate compliance with paragraphs 1 and 2.

## Source: GDPR — Article 30 (Records of Processing)

- **REQ-GDPR-014** [Art. 30(1)(a)] — The controller's record MUST contain the name and contact details of the controller, joint controllers, representative, and DPO.
- **REQ-GDPR-015** [Art. 30(1)(b)] — The controller's record MUST document the purposes of the processing.
- **REQ-GDPR-016** [Art. 30(1)(c)] — The controller's record MUST describe the categories of data subjects and categories of personal data.
- **REQ-GDPR-017** [Art. 30(1)(d)] — The controller's record MUST identify the categories of recipients including third-country recipients.
- **REQ-GDPR-018** [Art. 30(1)(e)] — Transfers to a third country MUST be documented, including identification of the country and safeguards.
- **REQ-GDPR-019** [Art. 30(1)(f)] — Envisaged time limits for erasure of different categories of data MUST be recorded where possible.
- **REQ-GDPR-020** [Art. 30(1)(g)] — A general description of the technical and organisational security measures MUST be recorded.
- **REQ-GDPR-021** [Art. 30(2)] — Processors MUST maintain a record of all categories of processing activities carried out on behalf of each controller.
- **REQ-GDPR-022** [Art. 30(3)–(4)] — Records MUST be in writing or electronic form and made available to the supervisory authority on request.

## Source: GDPR — Article 32 (Security of Processing)

- **REQ-GDPR-023** [Art. 32(1)(a)] — Controllers and processors MUST implement pseudonymisation and encryption of personal data where appropriate.
- **REQ-GDPR-024** [Art. 32(1)(b)] — Processing systems MUST maintain ongoing confidentiality, integrity, availability, and resilience.
- **REQ-GDPR-025** [Art. 32(1)(c)] — The ability to restore availability and access to personal data MUST exist in a timely manner after physical or technical incident.
- **REQ-GDPR-026** [Art. 32(1)(d)] — A process MUST exist for regularly testing, assessing, and evaluating the effectiveness of security measures.
- **REQ-GDPR-027** [Art. 32(2)] — Security measures MUST address risks of accidental or unlawful destruction, loss, alteration, unauthorised disclosure of, or access to personal data.
- **REQ-GDPR-028** [Art. 32(4)] — Persons acting under the authority of the controller or processor MUST process personal data only on instructions from the controller.

## Source: GDPR — Article 33 (Breach Notification to Authority)

- **REQ-GDPR-029** [Art. 33(1)] — Personal-data breaches MUST be notified to the supervisory authority without undue delay and not later than 72 hours after awareness.
- **REQ-GDPR-030** [Art. 33(1)] — A notification beyond 72 hours MUST be accompanied by reasons for the delay.
- **REQ-GDPR-031** [Art. 33(2)] — Processors MUST notify the controller without undue delay after becoming aware of a personal-data breach.
- **REQ-GDPR-032** [Art. 33(3)] — The notification MUST describe breach nature, categories of data subjects and records, DPO contact, likely consequences, and remedial measures.
- **REQ-GDPR-033** [Art. 33(4)] — Where information cannot be provided at once, it MAY be provided in phases without undue further delay.
- **REQ-GDPR-034** [Art. 33(5)] — The controller MUST document all personal-data breaches in sufficient detail for the supervisory authority to verify compliance.

## Source: GDPR — Article 34 (Breach Communication to Data Subject)

- **REQ-GDPR-035** [Art. 34(1)] — Where a breach is likely to result in high risk to natural persons, the data subject MUST be communicated to without undue delay.
- **REQ-GDPR-036** [Art. 34(2)] — The communication MUST describe the breach in clear and plain language and include Article 33(3)(b)-(d) information.
- **REQ-GDPR-037** [Art. 34(3)(a)] — Communication MAY be omitted if appropriate technical measures (e.g., encryption) rendered the data unintelligible to unauthorised persons.
- **REQ-GDPR-038** [Art. 34(3)(b)] — Communication MAY be omitted if subsequent measures eliminated the likelihood of high risk to data subjects.
- **REQ-GDPR-039** [Art. 34(3)(c)] — If individual communication would require disproportionate effort, a public communication of equivalent effectiveness MUST be used.
- **REQ-GDPR-040** [Art. 34(4)] — The supervisory authority MAY require communication to the data subject if it considers high risk to be likely.

## Source: GDPR — Article 35 (DPIA)

- **REQ-GDPR-041** [Art. 35(1)] — Where processing is likely to result in high risk, a DPIA MUST be carried out prior to the processing.
- **REQ-GDPR-042** [Art. 35(2)] — The DPO MUST be consulted when carrying out a DPIA.
- **REQ-GDPR-043** [Art. 35(3)(a)] — A DPIA MUST be required for systematic, extensive evaluation including profiling that produces legal or significant effects.
- **REQ-GDPR-044** [Art. 35(3)(b)] — A DPIA MUST be required for large-scale processing of special-category data (Art. 9) or criminal-conviction data (Art. 10).
- **REQ-GDPR-045** [Art. 35(3)(c)] — A DPIA MUST be required for systematic monitoring of a publicly accessible area on a large scale.
- **REQ-GDPR-046** [Art. 35(7)] — The DPIA MUST describe processing, assess necessity and proportionality, evaluate risks, and document mitigating safeguards.
- **REQ-GDPR-047** [Art. 35(9)] — Views of data subjects or their representatives SHOULD be sought where appropriate.
- **REQ-GDPR-048** [Art. 35(11)] — The DPIA MUST be reviewed periodically when risk circumstances change.

## Source: GDPR — Articles 44-49 (International Transfers)

- **REQ-GDPR-049** [Art. 44] — Any transfer of personal data to a third country or international organisation MUST comply with Chapter V conditions to maintain the level of protection.
- **REQ-GDPR-050** [Art. 44] — Onward transfers from a third country to another third country MUST also comply with Chapter V conditions.
- **REQ-GDPR-051** [Art. 45(1)] — Transfers MAY occur to a third country only where the Commission has decided it ensures an adequate level of protection.
- **REQ-GDPR-052** [Art. 45(2)] — Adequacy assessments MUST consider rule of law, human-rights protections, supervisory-authority independence, and international commitments.
- **REQ-GDPR-053** [Art. 45(3)] — Adequacy decisions MUST be subject to a periodic review at least every four years.
- **REQ-GDPR-054** [Art. 45(5)] — The Commission MUST be able to amend, suspend, or revoke adequacy decisions when protection standards decline.
- **REQ-GDPR-055** [Art. 46(1)] — Where no adequacy decision exists, transfers MUST occur only with appropriate safeguards and enforceable data-subject rights.
- **REQ-GDPR-056** [Art. 46(2)(a)] — Legally binding and enforceable instruments between public authorities MAY constitute appropriate safeguards.
- **REQ-GDPR-057** [Art. 46(2)(b)] — Binding corporate rules under Article 47 MAY constitute appropriate safeguards.
- **REQ-GDPR-058** [Art. 46(2)(c)] — Commission-adopted standard data-protection clauses MAY constitute appropriate safeguards.
- **REQ-GDPR-059** [Art. 46(2)(e)] — Approved codes of conduct with binding third-country commitments MAY constitute appropriate safeguards.
- **REQ-GDPR-060** [Art. 46(2)(f)] — Approved certification mechanisms with binding third-country commitments MAY constitute appropriate safeguards.
- **REQ-GDPR-061** [Art. 46(3)(a)] — Bespoke contractual clauses MAY be used only with supervisory-authority authorisation.
- **REQ-GDPR-062** [Art. 47(1)(a)] — Binding corporate rules MUST be legally binding and enforced by every member of the group concerned.
- **REQ-GDPR-063** [Art. 47(1)(b)] — Binding corporate rules MUST expressly confer enforceable rights on data subjects regarding processing of their data.
- **REQ-GDPR-064** [Art. 47(2)(d)] — Binding corporate rules MUST apply data-protection principles including purpose limitation, minimisation, quality, and security.
- **REQ-GDPR-065** [Art. 47(2)(f)] — A controller or processor established in the EU MUST accept liability for breaches by non-EU group members under BCRs.
- **REQ-GDPR-066** [Art. 47(2)(k)] — Personnel with regular personal-data access MUST receive appropriate data-protection training.
- **REQ-GDPR-067** [Art. 48] — Third-country court judgments or administrative decisions requiring data transfer MUST only be recognised when based on an international agreement (e.g., MLAT).
- **REQ-GDPR-068** [Art. 49(1)(a)] — Without an adequacy decision or safeguards, transfers MAY occur where the data subject has explicitly consented after being informed of risks.
- **REQ-GDPR-069** [Art. 49(1)(b)] — Transfer MAY occur where necessary for performance of a contract between the data subject and the controller.
- **REQ-GDPR-070** [Art. 49(1)(d)] — Transfer MAY occur where necessary for important reasons of public interest recognised in Union or Member-State law.
- **REQ-GDPR-071** [Art. 49(1)(e)] — Transfer MAY occur where necessary for the establishment, exercise, or defence of legal claims.
- **REQ-GDPR-072** [Art. 49(1)(f)] — Transfer MAY occur where necessary to protect the vital interests of the data subject or others when consent cannot be obtained.
- **REQ-GDPR-073** [Art. 49(1) last subpara] — Non-repetitive transfers concerning a limited number of data subjects MAY occur only where compelling legitimate interests are not overridden by data-subject interests and suitable safeguards are documented.
