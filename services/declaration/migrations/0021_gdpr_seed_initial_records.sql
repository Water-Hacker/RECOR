-- TODO-034 — Seed the Art. 30 register with the platform's current
-- processing activities. Each row cites the FATF / 6AMLD / GDPR clause
-- that is the legal basis.
--
-- The seed runs ONCE per environment via `INSERT ... ON CONFLICT DO
-- NOTHING` keyed on the deterministic record_id (constructed from a
-- well-known UUID namespace below). Subsequent migrations / admin
-- writes layer on top.
--
-- record_id values are stable v4 UUIDs hand-allocated for the eight
-- seed activities so the rows are idempotent across re-runs and so
-- ADRs / runbooks can reference a specific record_id forever.

BEGIN;

INSERT INTO gdpr_processing_register (
    record_id, controller, processor, purpose, legal_basis,
    data_categories, subject_categories, recipients,
    retention_period_text, transfer_safeguards, created_at, updated_at
) VALUES
-- 1. BO declaration intake.
(
    'a1111111-1111-4111-8111-000000000001',
    'RÉCOR (Cameroon BO Registry)',
    NULL,
    'Beneficial-ownership declaration intake from declarants and obliged-entity submitters; constitutes the central registry under FATF R.24.',
    'FATF R.24 criterion 24.4 (adequate, accurate, up-to-date BO information in a central registry) + EU 6AMLD Art. 30 (central registers) + GDPR Art. 6(1)(c) (processing necessary for compliance with a legal obligation to which the controller is subject).',
    '["legal_name", "national_id_reference", "nationality", "date_of_birth", "ownership_basis_points", "cascade_tier_assessment", "ed25519_attestation"]'::jsonb,
    '["natural_persons_appearing_as_beneficial_owners", "legal_representatives_acting_as_declarants"]'::jsonb,
    '["RÉCOR back-office", "verification engine (internal)", "audit-witness chaincode (internal Fabric channel)"]'::jsonb,
    'Retained for the lifetime of the declared entity plus 5 years post-cessation per FATF R.24 criterion 24.13 retention rule; event log retained indefinitely under COMP-2 immutability + D15 cryptographic provenance.',
    NULL,
    NOW(), NOW()
),
-- 2. BO verification engine processing.
(
    'a1111111-1111-4111-8111-000000000002',
    'RÉCOR (Cameroon BO Registry)',
    NULL,
    'Adversarial reasoning over ownership chains to detect cascade-tier inconsistencies, sanctions overlaps, and PEP exposure; output is a verification outcome stored against each declaration.',
    'FATF R.24 criterion 24.6 (verification of BO information by competent authorities) + GDPR Art. 6(1)(c).',
    '["beneficial_owner_graph", "cascade_tier_assessment", "sanctions_list_hits", "pep_overlap_results"]'::jsonb,
    '["natural_persons_appearing_as_beneficial_owners"]'::jsonb,
    '["RÉCOR back-office", "auditors during regulator review"]'::jsonb,
    'Verification outcomes follow the parent declaration retention period; event log retained indefinitely.',
    NULL,
    NOW(), NOW()
),
-- 3. FIU disclosure (ANIF + R.40 / Egmont MLAT).
(
    'a1111111-1111-4111-8111-000000000003',
    'RÉCOR (Cameroon BO Registry)',
    NULL,
    'Real-time disclosure of BO information to the Cameroon Financial Intelligence Unit (ANIF) and, via R.40 / Egmont MLAT routing, to foreign FIUs.',
    'FATF R.24 criterion 24.9 (timely access for the FIU) + FATF R.40 (international FIU cooperation) + GDPR Art. 6(1)(e) (processing necessary for the performance of a task carried out in the public interest).',
    '["beneficial_owner_full_payload", "verification_outcomes", "supersede_chain"]'::jsonb,
    '["natural_persons_appearing_as_beneficial_owners", "investigation_subjects"]'::jsonb,
    '["ANIF (Cameroon FIU)", "foreign FIUs via Egmont", "RÉCOR back-office for audit"]'::jsonb,
    'Disclosure log (`fiu_disclosure_log`) retained indefinitely under COMP-2 immutability for forensic and audit accountability per FATF R.40.',
    'mTLS peer-id allowlist + IP allowlist + OIDC `recor:fiu-anif` scope. R.40 routing through Egmont gateway with bilateral MLAT references logged.',
    NOW(), NOW()
),
-- 4. Obliged-entity access (CDD).
(
    'a1111111-1111-4111-8111-000000000004',
    'RÉCOR (Cameroon BO Registry)',
    NULL,
    'Lookup of BO information by supervised obliged entities (banks, notaries, DNFBPs) performing customer due diligence on legal entities.',
    'EU 6AMLD Art. 12 (access to BO registers by obliged entities) + EU AMLR Chapter IV + GDPR Art. 6(1)(c).',
    '["beneficial_owner_redacted_payload (post-Sovim minimisation: no declarant_principal, no correlation_id, no verification_case_id, no free-text evidence fields, no nominator_person_id)"]'::jsonb,
    '["natural_persons_appearing_as_beneficial_owners_of_inspected_entities"]'::jsonb,
    '["regulated FIs (COBAC/CEMAC supervision)", "DNFBPs (notaries, lawyers, accountants)"]'::jsonb,
    'Per-disclosure access log (`obliged_entity_access_log`) retained indefinitely. Sliding-window cap of 1000 reads/24h per principal (configurable).',
    'OIDC `recor:obliged-entity` scope. IdP provisioning gated by COBAC/CEMAC supervision proof or DNFBP registration.',
    NOW(), NOW()
),
-- 5. Public feedback intake.
(
    'a1111111-1111-4111-8111-000000000005',
    'RÉCOR (Cameroon BO Registry)',
    NULL,
    'Intake of public reports flagging inaccuracies in registry data; routed to back-office triage and possibly to discrepancy or sanction proceedings.',
    'CJEU C-37/20 + C-601/20 (Sovim) balancing test — limited public-facing inaccuracy-reporting channel + GDPR Art. 6(1)(e).',
    '["flagged_declaration_id", "free_text_feedback", "captcha_token_hash", "ip_address_hash"]'::jsonb,
    '["members_of_the_public_submitting_feedback"]'::jsonb,
    '["RÉCOR back-office triage"]'::jsonb,
    'Feedback log retained for the duration of any follow-up investigation plus the retention period of the parent declaration; mass-flagging detection runs over a configurable rolling window.',
    'CAPTCHA + per-IP throttle + BLAKE3 hashing of the IP and the CAPTCHA token (raw values never stored).',
    NOW(), NOW()
),
-- 6. Sanctions proceedings.
(
    'a1111111-1111-4111-8111-000000000006',
    'RÉCOR (Cameroon BO Registry)',
    NULL,
    'Initiation, escalation, withdrawal, and appeal handling for sanctions imposed on non-compliant declarants under the FATF R.24 sanctions framework.',
    'FATF R.24 criterion 24.13 (proportionate and dissuasive sanctions for non-compliance) + GDPR Art. 6(1)(c).',
    '["declaration_id", "sanctioned_entity_id", "tier_assessment", "justification_text", "appeal_record"]'::jsonb,
    '["non_compliant_declarants_and_their_entities"]'::jsonb,
    '["RÉCOR back-office", "appeals review committee", "regulator on public-listings publication"]'::jsonb,
    'Sanction proceedings, events, and appeals retained indefinitely under COMP-2 immutability; public listings retained per the regulator publication policy.',
    NULL,
    NOW(), NOW()
),
-- 7. Audit verification (cryptographic provenance).
(
    'a1111111-1111-4111-8111-000000000007',
    'RÉCOR (Cameroon BO Registry)',
    NULL,
    'Recomputation of receipt hashes from the projection and comparison with the Hyperledger-Fabric audit channel entries, accessible via `/v1/audit/verify/{declaration_id}`.',
    'D15 (RÉCOR engineering doctrine — cryptographic provenance on every consequential event) + GDPR Art. 6(1)(c).',
    '["receipt_hash_hex", "blake3_receipt", "fabric_channel_anchor"]'::jsonb,
    '["natural_persons_appearing_as_beneficial_owners_at_the_time_of_anchoring"]'::jsonb,
    '["any_authenticated_caller (admin or auditor)", "external auditors during regulator review"]'::jsonb,
    'Audit anchors retained indefinitely on the Fabric channel; the verifier app is read-only and produces no new derived data subject to retention.',
    NULL,
    NOW(), NOW()
),
-- 8. Rectification + erasure-restriction handling.
(
    'a1111111-1111-4111-8111-000000000008',
    'RÉCOR (Cameroon BO Registry)',
    NULL,
    'Receipt and handling of data-subject rectification requests (Art. 16) and erasure-restriction requests (Art. 17 + 18); erasure is structurally refused on BO data because of FATF R.24 retention obligations, with the refusal documented per request.',
    'GDPR Art. 16 (rectification) + Art. 17 (erasure, structurally refused under the Art. 17(3)(b) carve-out for legal-obligation processing) + Art. 18 (restriction of processing, granted on request with downstream disclosure notice per Art. 18(2)).',
    '["data_subject_principal", "declaration_id", "field_path", "requested_value", "reason", "resolution_notes"]'::jsonb,
    '["data_subjects_exercising_their_GDPR_rights"]'::jsonb,
    '["RÉCOR back-office", "DPO during quarterly review"]'::jsonb,
    'Rectification / restriction requests and their event logs retained indefinitely under COMP-2 immutability — the audit trail of GDPR-rights exercise is itself a regulatory record.',
    NULL,
    NOW(), NOW()
)
ON CONFLICT (record_id) DO NOTHING;

-- Mirror creation as event-log entries so the events table reflects the
-- seed deterministically. event_id is a deterministic offset of the
-- record_id namespace so re-runs are idempotent.
INSERT INTO gdpr_processing_register_events (
    event_id, record_id, event_type, payload, actor_principal,
    occurred_at, sequence_no
)
SELECT
    -- Deterministic event_id: replace the leading 'a' marker of the
    -- record_id with 'b' so the value is a distinct UUID per row.
    ('b' || substr(record_id::text, 2))::uuid,
    record_id,
    'gdpr.processing_record.created.v1',
    jsonb_build_object(
        'controller', controller,
        'purpose', purpose,
        'legal_basis', legal_basis,
        'seeded', true
    ),
    'system:migration:0021',
    created_at,
    1
FROM gdpr_processing_register
WHERE record_id IN (
    'a1111111-1111-4111-8111-000000000001',
    'a1111111-1111-4111-8111-000000000002',
    'a1111111-1111-4111-8111-000000000003',
    'a1111111-1111-4111-8111-000000000004',
    'a1111111-1111-4111-8111-000000000005',
    'a1111111-1111-4111-8111-000000000006',
    'a1111111-1111-4111-8111-000000000007',
    'a1111111-1111-4111-8111-000000000008'
)
ON CONFLICT (event_id) DO NOTHING;

COMMIT;
