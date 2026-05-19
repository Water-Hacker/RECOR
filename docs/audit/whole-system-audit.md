# RÉCOR — Whole-system forensic production-readiness audit

**Date:** 2026-05-13
**Auditor:** Engineering team (Claude Code agents under typescript-frontend-engineer + rust-service-engineer + security-engineer roles, with the lead-orchestrator composing)
**Branch:** `docs/whole-system-audit`
**Scope:** every file in the repository at HEAD = `b5d49bc` (post-merge of the 41-ticket production roadmap)

---

## Executive summary

The RÉCOR codebase is **structurally sound but not yet production-defensible** in its current state. The system implements the full 41-ticket production roadmap with real Ed25519 attestation, real BLAKE3 receipts, real Dempster-Shafer fusion, real Kafka transport, real SPIFFE/mTLS, real Hyperledger Fabric audit anchoring, real OIDC + JWKS authentication, real PII redaction, and real disaster-recovery tooling. **The cryptographic substrate is correct.** Every load-bearing cryptographic operation is real, the canonical-form byte-parity invariant is unit-tested, and the audit chain (declaration_events + Fabric audit-witness) is append-only enforced at the SQL trigger layer.

**Findings.** The audit surfaced **6 critical, 14 high, ~52 medium, ~28 low** findings. The six critical findings cluster into a single doctrinal theme: **D17 (zero trust) is violated at five service endpoints, and one access-control oversight on the audit-verifier exposes the system's PII surface to unauthenticated callers.** Every CRITICAL finding has a closing change in the **cheap-to-close (<1 day)** column of [`12-recommendations.md`](12-recommendations.md) except for FIND-005 (person-service GET/search) which is medium-effort (~5 days). The high findings cluster around infrastructure-as-code (the `infrastructure/{terraform,kubernetes,ansible,networks}/` directories are EMPTY — the system cannot be deployed to production as-is), V-engine integration-test coverage, and audit-chain reconciliation tooling.

**Single most important next action.** Close **FIND-003** (dev-header + OIDC double-accept authentication bypass). It is the smallest patch (~1 day) that closes the largest blast radius (complete impersonation with a stray `ENVIRONMENT=dev` env var in production). It is the only finding where an attacker with no credentials at all can reach administrative state. Close this in Sprint 0.

**Single biggest source of risk.** The cluster of five D17-violation findings (FIND-001 through FIND-006). They reflect a consistent pattern in newly-shipped handler code where the principal extension is sourced but discarded via `let _ = principal`. The pattern shipped because the V-engine + person-service + entity-service were stood up faster than their cross-tenant tenancy story matured. Closing the cluster takes ~2 weeks across the four services; doing so converts the system from "useful skeleton" to "defensible."

**Single biggest source of strength.** The cryptographic + audit-chain substrate. The Ed25519 + BLAKE3 + Fabric anchoring chain composes correctly. Canonical-form byte-parity is enforced by tests. COMP-2 SQL triggers refuse UPDATE/DELETE/TRUNCATE on event logs at the database layer, independently of any application code. R-DECL-9 wires Fabric anchoring so a DB admin cannot silently rewrite history. The OPS-2 PII redaction layer + OPS-1 rate limiting + DOC-1 OpenAPI + DOC-4 STRIDE threat model all carry their weight. **When the access-control findings close, the system has the strongest cryptographic posture in its peer class of national-registry projects we know of.**

---

## Section index

| # | Document | Section of audit spec | Status |
|---|---|---|---|
| 00 | [`00-orientation.md`](00-orientation.md) | §2 Repository orientation | done |
| 01 | [`01-system-map.md`](01-system-map.md) | §3 Every directory, every connection | done |
| 02 | [`02-surfaces.md`](02-surfaces.md) | §4 Every external interface | done |
| 03 | [`03-data-flows.md`](03-data-flows.md) | §5 End-to-end data flows | done |
| 04 | [`04-failure-modes.md`](04-failure-modes.md) | §6 Failure mode catalogue | done |
| 05 | [`05-permissions.md`](05-permissions.md) | §7 Permission + visibility enforcement | done |
| 06 | [`06-ui.md`](06-ui.md) | §8 UI + accessibility | done |
| 07 | [`07-cryptography.md`](07-cryptography.md) | §9 Cryptographic posture verification | done |
| 08 | [`08-audit-chain.md`](08-audit-chain.md) | §10 Audit trail + write-ahead integrity | done (static analysis; live replay deferred — see §11) |
| 09 | [`09-stress-test.md`](09-stress-test.md) | §11 Live-fire stress test | partial — 2 of 15 exercises verified via CI history; 13 require live stack (production acceptance gates documented) |
| 10 | [`10-findings.md`](10-findings.md) | §12 Ranked findings catalogue | done |
| 11 | [`11-doctrine.md`](11-doctrine.md) | §13 Doctrinal observations | done |
| 12 | [`12-recommendations.md`](12-recommendations.md) | §14 Remediation plan | done |

## Supporting evidence

`docs/audit/evidence/` is scaffolded with subdirectories for
accessibility, audit-chain, cryptography, and stress-test artifacts.
Live-fire artifacts will land in those subdirectories during the
production verification cycle described in
[`09-stress-test.md`](09-stress-test.md).

## Critical interrupts

The audit's Pass B raised two CRITICAL findings before completion;
they were surfaced via [`CRITICAL-INTERRUPT.md`](CRITICAL-INTERRUPT.md)
and are folded into the catalogue as FIND-002 + FIND-003.

---

## Findings summary by severity

| Severity | Count | IDs |
|---|---|---|
| **CRITICAL** | 6 | FIND-001 (audit-verifier unauthenticated PII disclosure), FIND-002 (V-engine submit open), FIND-003 (dev+oidc auth bypass), FIND-004 (V-engine cross-tenant case read), FIND-005 (person GET/search Sensitive-PII grant), FIND-006 (person POST identity injection) |
| **HIGH** | 14 | FIND-007..020 — `/metrics` exposure, missing infrastructure-as-code, stub-stage wiring, doctrine drift, integration-test gaps, audit-chain reconciliation, V-engine OpenAPI, HMAC iat, Vault on new services, etc. |
| **MEDIUM** | ~52 | summarised by category in `10-findings.md` |
| **LOW** | ~28 | summarised by category in `10-findings.md` |

## Doctrines posture

24 doctrines graded: **18 held, 4 held-with-caveat, 2 violated**.
Detail in [`11-doctrine.md`](11-doctrine.md). The two outright
violations are D11 (two reviewers — solo maintainer transitional)
and D17 (zero trust — five CRITICAL findings).

## Production verification status

| Layer | Verifiable from this audit | Status |
|---|---|---|
| Cryptographic substrate | Yes (static + unit tests) | sound |
| Audit chain | Yes (static + COMP-2 testcontainers tests) | sound; reconciliation cron missing |
| Permission model | Yes (static walkthrough of every surface) | **5 CRITICAL violations to close** |
| Failure-mode handling | Partial (catalogue from static + tests; live-fire deferred) | sound posture; live verification needed |
| Build + supply chain | Yes (CI history) | sound; Trivy v0.36.0 + cosign keyless + SBOM attestations |
| Infrastructure-as-code | No (directories empty) | **must be authored before deploy** |
| Live-fire stress test | No (no live stack stood up) | acceptance gates documented; PEN-1 cycle to execute |

---

## How to use this audit

The architect uses this audit as:

1. **The pre-launch verification record.** Cite this document when an
   external reviewer asks "how do you know the system works as
   claimed."

2. **The remediation plan input.** [`12-recommendations.md`](12-recommendations.md)
   sequences the next work passes. Sprint 0 closes the cheap
   criticals (1 week); Sprint 1 closes the medium-effort highs (2
   weeks); Sprint 2 closes the infrastructure layer (4 weeks).

3. **The PEN-1 engagement input.** The pen-test vendor receives this
   audit + the existing [`docs/security/pen-test-prep.md`](../security/pen-test-prep.md)
   before the engagement. The CRITICAL findings here become the
   pen-tester's first verification objectives.

4. **The honesty baseline.** Every finding cites file:line. Every
   claim has evidence. The drift between docs and code is itself
   catalogued as findings. Future external audits start from this
   document.

---

## Calibration statement

This audit was conducted by Claude Code agents operating under the
lead-orchestrator role. The audit's value is bounded by:

- **Depth of reading.** Three parallel audit passes (A, B, C) read
  every populated source file in the repository. Pass A covered
  orientation, system map, and 13 surfaces; Pass B covered 16
  data flows, the failure-mode catalogue, and the permission
  model; Pass C covered the portal UI, every cryptographic
  operation, and the audit chain.

- **Live-fire substitution.** Section 11 (stress test) is the one
  section where the audit ran against the CI test harness +
  static analysis rather than a stood-up production-shaped stack.
  Every `requires-live-fire` exercise is named, with the
  production acceptance gate documented. The audit does NOT
  pretend a live-fire test ran when it didn't.

- **Solo-auditor reality.** D11 requires two reviewers with at
  least one cross-team. This audit had one author. An independent
  re-audit by a security-team or external red-team is the next
  defensive step — this audit's primary value is making that
  re-audit cheaper by surfacing the obvious findings first.

The system, after the Sprint 0 + Sprint 1 fixes close, is
**defensible**. Today, it is **structurally sound and not yet
defensible** because of the five D17 access-control violations.
The path from here to defensible is approximately three weeks of
engineering work plus the infrastructure-as-code workstream
(separately).

That is the honest assessment.
