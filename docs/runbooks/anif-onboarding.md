# Runbook — ANIF onboarding (TODO-008)

**Doctrine reference:** D14 (fail-closed), D15 (cryptographic
provenance), D17 (zero trust), D18 (no secrets in logs).
**Standards reference:** FATF R.24 c.24.9 (FIU access to BO), FATF
R.40 (international cooperation), Egmont Group Operational
Procedures, Cameroon AML law (ANIF mandate).

This runbook is the procedure to onboard a new ANIF principal — or a
foreign FIU routed via R.40 / Egmont — to the platform's FIU
disclosure surface.

## Surface summary

| Endpoint | Purpose | Auth |
|---|---|---|
| `POST /v1/fiu/search` | Resolve a search subject to one or more declarations; record the disclosure in the COMP-2-immutable log | OIDC `recor:fiu-anif` scope + mTLS peer-ID + IP allowlist |
| `GET /v1/fiu/disclosure/{id}` | Retrieve a prior disclosure log row (per-FIU; FIUs do not see each other's case files) | Same |

Every successful `POST /v1/fiu/search` writes a row to
`fiu_disclosure_log` (migration 0012) with:

- The requesting principal subject
- ANIF case reference
- Free-text justification (required for GDPR Art. 30 records-of-processing)
- Subject discriminator (`person_id` | `national_id` | `declaration_id` | `entity_id` | `full_name`)
- Subject value
- BLAKE3-stable disclosure_id
- Field-level audit (`disclosed_columns` JSONB)
- For MLAT requests: foreign FIU identifier + Egmont request id

## Pre-conditions

1. ANIF (or the Egmont gateway) has a documented contact for incident
   escalation.
2. The OIDC issuer at ANIF has been configured to issue tokens with
   `acr` resolving to IAL3 (or the agreed equivalent) and `scope`
   containing `recor:fiu-anif`.
3. The platform operations team has provisioned:
   - A SPIFFE workload identity for the ANIF egress pod
     (e.g. `spiffe://recor.cm/fiu-anif-gateway`)
   - The egress source IP for that pod added to the cluster ingress
     allowlist
   - A test case file in the ANIF case-management system that can be
     used for the dry-run

## Onboarding procedure

1. **Verify the ANIF token end-to-end.** Have ANIF mint a test token
   and decode it locally:
   ```bash
   echo "$TOKEN" | cut -d. -f2 | base64 -d | jq '{sub, scope, acr}'
   ```
   The output MUST contain `scope: "recor:fiu-anif"` (or include the
   scope in a space-delimited list) and `acr` resolving to IAL3.
   If `acr` is below `IAL2`, the OIDC verifier will refuse the token
   before it reaches the FIU handler — fix the IdP side before
   proceeding.

2. **Smoke-test the search endpoint against staging.** From the ANIF
   gateway pod, against `staging`:
   ```bash
   curl -fsS -X POST https://staging-recor.cm/v1/fiu/search \
     -H "Authorization: Bearer $TOKEN" \
     -H "Content-Type: application/json" \
     --data '{
       "anif_case_reference": "ANIF-TEST-001",
       "justification_text": "End-to-end onboarding smoke test",
       "subject_kind": "declaration_id",
       "subject_value": "00000000-0000-0000-0000-000000000000"
     }'
   ```
   Expected response: `200 OK` with `matches: []` (the test UUID
   doesn't exist) and a `disclosure_id`. The empty match list is
   correct — the audit log row IS still written, which is the
   important behaviour.

3. **Verify the disclosure log row landed.** Operator runs (against
   staging):
   ```bash
   psql -d declaration -c "SELECT disclosure_id, anif_case_reference,
       subject_kind, disclosed_at FROM fiu_disclosure_log
       WHERE anif_case_reference = 'ANIF-TEST-001';"
   ```
   One row should be present. The row CANNOT be UPDATED or DELETED —
   the COMP-2 trigger refuses both.

4. **Confirm read-back works.**
   ```bash
   curl -fsS https://staging-recor.cm/v1/fiu/disclosure/${DISCLOSURE_ID} \
     -H "Authorization: Bearer $TOKEN"
   ```
   The response should echo the row inserted in step 2. A
   **different** ANIF principal reading the same id MUST get `404`
   (per-FIU audit scope; FIND-004 enumeration protection).

5. **Promote to production.** Repeat steps 1–4 against the production
   cluster. Update the platform status page to record:
   - The date ANIF was admitted
   - The OIDC issuer URL
   - The SPIFFE workload identity
   - The contact for ANIF-side incident escalation

## Egmont / R.40 (foreign FIU) routing

Foreign-FIU requests are NOT a self-serve surface. The pathway is:

1. The foreign FIU sends a request via Egmont to ANIF.
2. ANIF reviews under R.40 and, if admissible, issues the platform a
   `POST /v1/fiu/search` carrying `mlat_foreign_fiu` (the foreign
   FIU's identifier) + `mlat_egmont_request_id` (the Egmont request
   id). The justification text references the Egmont request.
3. The platform records the disclosure with the MLAT fields populated.
4. ANIF transmits the response back to the foreign FIU via Egmont.

There is no direct foreign-FIU principal class on the platform. The
foreign-FIU request appears in the audit log under ANIF's principal
with the MLAT identifiers attached — this is the documented R.40
pathway.

## Operational metrics

The on-call dashboard surfaces:

- `recor_oidc_verify_total{result="success"}` filtered to the FIU
  principal subject — every successful authentication
- `fiu_disclosure_log` row count per day (grafana / panel)
- 401 / 403 from the FIU endpoints (anomaly → ANIF token rotation
  needed?)

## Failure modes and remediation

| Symptom | Cause | Remediation |
|---|---|---|
| Every ANIF request returns 401 | ANIF token's IdP discovery URL not configured on the platform; `OIDC_ISSUER_URL` does not match | Re-run the IdP wiring; verify discovery doc is reachable |
| 403 `insufficient_assurance` | Token's `acr` resolves below IAL2 | ANIF-side IdP step-up enrolment; see `docs/runbooks/oidc-idp-acr-config.md` |
| 403 `fiu/search is gated to the FIU principal class` | Token's `scope` claim is missing `recor:fiu-anif` | Add the scope to the OIDC client's default scopes; re-issue token |
| `mtls handshake failed` (network log) | SPIFFE peer-ID not on allowlist; cert expired | Re-bootstrap the ANIF gateway pod; verify SPIRE issued a fresh SVID |

## Cross-references

- Migration: [`services/declaration/migrations/0012_fiu_disclosure_log.sql`](../../services/declaration/migrations/0012_fiu_disclosure_log.sql)
- Handler: [`services/declaration/src/api/fiu.rs`](../../services/declaration/src/api/fiu.rs)
- Permission matrix: [`docs/security/permission-matrix.md`](../security/permission-matrix.md)
- TODOS.md § TODO-008
