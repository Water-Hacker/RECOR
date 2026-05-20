# ADR-0012 — Sanctions for non-compliance — proportionality ladder

- **Status:** Accepted (2026-05-20)
- **Deciders:** Domain team, Legal counsel, Lead architect
- **Closes:** TODO-004
- **Related:** ADR-0008 (SPIFFE mTLS), ADR-0010 (FATF cascade + adequacy)

## Context

FATF Recommendation 24, c.24.13 requires that countries "ensure that
there are proportionate, dissuasive, and effective sanctions" for
failure to comply with BO requirements. The 2024-onwards MER pattern
treats this as unwaivable: a registry whose only enforcement
mechanism is "the operator will be unhappy" cannot demonstrate that
its requirements have teeth.

The platform's pre-TODO-004 surface had **no sanction concept** —
`dissolve` operates on entities (administrative dissolution of a
defunct entity), and `correct` operates on declaration metadata. No
mechanism existed to penalise an entity that fails to update its BO
record within the 30-day window (TODO-005's threshold).

This ADR defines the proportionality ladder, the per-tier
justifications required, and the operator workflow.

## Decision: the ladder

| Tier | State | When applied | Operator action |
|---|---|---|---|
| **0** | `submitted` | Proceeding opened (book-keeping; no external effect) | `POST /v1/sanctions/initiate` |
| **1** | `reminder` | First missed update window (≥30 days past last_event_observed_at) | `POST /v1/sanctions/{id}/escalate` `to_state=reminder` |
| **2** | `fined` (tier 1) | Second missed window; minor infraction | `POST /v1/sanctions/{id}/escalate` `to_state=fined` `tier=1` |
| **3** | `fined` (tier 2-5) | Escalating fines — tier 2-5 reserved for repeat or wilful non-compliance | Same; advance `tier` field |
| **4** | `suspended` | Registry status suspended; declaration flagged non-current; entity flagged | `POST /v1/sanctions/{id}/escalate` `to_state=suspended` |
| **5** | `referred` | Referral to ANIF/COBAC under the regulated-counterparty path | `POST /v1/sanctions/{id}/escalate` `to_state=referred` |
| **6** | `public_listed` | Persistent non-complier — name published on `GET /v1/sanctions/public` per the post-Sovim balancing test | `POST /v1/sanctions/{id}/escalate` `to_state=public_listed` `public_listing_name=...` `public_listing_reason=...` |
| **Terminal** | `withdrawn` | Discrepancy resolved / declaration corrected / appeal upheld | `POST /v1/sanctions/{id}/withdraw` |

The ladder is **forward-only** except `withdrawn`, which is the
universal exit. The handler ENFORCES a non-empty `justification` on
every transition (D14 + R.24 c.24.13).

### Egregious-case bypass

The R.24 framing of "proportionate" admits cases where the operator
should bypass the early rungs (e.g. a shell-entity laundering scheme
detected by Stage 5 of the verification engine). The initiate
endpoint accepts an `initial_tier` parameter that opens the
proceeding directly at `fined` (tier 1-5); the audit log captures
the bypass with the operator's justification. This is the
**documented** carve-out — undocumented skips are refused at the API
boundary.

## Rationale

**Why a single ladder enum rather than a state machine of separate
events?** Operationally an administrator's mental model is "this
proceeding is currently at tier N" — a single state column matches
that. The event log captures the per-transition payload, so the
state machine is reconstructable from the audit trail.

**Why `withdrawn` not `terminated`?** Withdrawn is reversible at the
audit-trail level — the public list refuses to surface withdrawn
proceedings (cache TTL 24h), but the row itself is preserved with
the public_listing_name + reason recorded so a future investigator
can prove what was once listed. Terminated would imply finality, which
is incompatible with appeal/reinstatement.

**Why is the public list cached 24h?** Sovim balancing tests
recognise that public listing has a real-world impact on the named
entity (correspondent banking, procurement eligibility); 24-hour
removal latency is the operator's commitment that a wrongful listing
is correctable within a business day. Cache invalidation is enforced
by the back-office workflow + a Prometheus alert on
`recor_sanctions_public_listing_cache_lag_seconds` (operator follow-
up; the alert wiring is the next deliverable).

**Why fail-closed on empty `ADMIN_PRINCIPALS`?** Same posture as the
DLQ-admin endpoints — an operator who has not configured the
allowlist is not in a state to authorise sanctions. The endpoints
refuse all callers.

## Consequences

### Positive

- TODO-005 (staleness watcher) can now route its observations into a
  sanction-initiate call once the back-office workflow consumes the
  signal — closing the c.24.8 + c.24.13 loop end-to-end.
- TODO-003 (discrepancy resolution) gets a clear `resolution_kind =
  sanction_imposed` path that lands in the same audit substrate.
- The public list satisfies the Open Ownership principle 5.5
  "consequences for non-compliance" element.

### Negative

- Public listing has reputational weight. The 24-hour cache means
  one missed back-office step can leave a withdrawn entity on the
  list for up to a day — operationally tolerable, legally a wash.
  ADR is the operator's standing instruction to treat public
  listings with extreme care.
- No appeals workflow at v1. Appeals route through the back-office;
  the `withdraw` endpoint is sufficient for the operator side, but
  the entity-facing notification + appeal-submission surface is a
  follow-up (`TODO-004-appeals`).

### Operator burden

A new role: the **sanctions officer** who decides escalations. The
permission matrix (admin allowlist) is the gate; in practice the
operator will sub-allowlist the sanctions officers via a separate
config knob in a follow-up. For v1, every admin is a sanctions
officer.

## Alternatives considered

1. **Implicit ladder via tier number only (no `state` column).**
   Rejected: `referred` and `public_listed` are categorically
   different from `fined` (tier 1-5); collapsing them onto a
   numeric ladder loses the type-system signal.
2. **No public listing — keep all sanctions internal.** Rejected:
   removes the deterrent effect that R.24 c.24.13 calls out.
3. **Public listing only at tier 5 (after fines exhausted).**
   Rejected: the operator's discretion should include "the
   egregiousness justifies the listing skip" — the egregious-bypass
   pattern.

## Linked from

- TODOS.md § TODO-004
- services/declaration/migrations/0014_sanctions_proceedings.sql
- services/declaration/src/api/sanctions.rs
- docs/security/permission-matrix.md
- docs/runbooks/sanctions-workflow.md (planned operator runbook)
