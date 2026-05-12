---
name: recor-security-review
description: Security review of code changes. Fires when security review is explicitly requested or when the lead orchestrator delegates to the security-reviewer agent. Loads the STRIDE method, OWASP/CWE checklist, project-specific threat model references.
---

# RÉCOR security review

The security-reviewer agent uses this skill.

## Methods

- **STRIDE**: Spoofing, Tampering, Repudiation, Information disclosure,
  Denial of service, Elevation of privilege
- **OWASP Top 10** for application layer
- **CWE Top 25** for code-level patterns
- **Project threat models** at /docs/security/threat-models/<service>.md

## What to check

### Authentication and authorisation
- Every state-changing operation is authorised through the Access service
- Authorisation granularity matches the data sensitivity
- Justification capture is enforced for restricted-tier data access

### Cryptographic patterns
- No DIY crypto
- Approved primitives only (ed25519, AES-256-GCM, BLAKE3, ML-KEM-1024)
- Constant-time comparison for secrets
- Nonces and IVs never reused

### Input handling
- Trust boundaries are documented and enforced
- Validation uses the schema
- No string concatenation for SQL queries
- HTML/SVG/JSON inputs sanitised before rendering or serialisation

### Logging discipline
- No PII in logs (per the classification policy)
- No secrets in logs
- Audit-channel events use the audit service, not service logs

### Error surface
- Error messages exposed to clients do not leak internal details
- Stack traces and database errors are never returned to clients
- Internal errors carry correlation IDs for the audit channel

## Output

Findings ordered by severity per project classification:
- Critical: blocks merge; remediated immediately
- High: blocks merge; remediated within one sprint
- Medium: tracked; remediated within one PI
- Low: tracked; remediated within next operational quarter
- Info: noted; not necessarily acted upon
