# Runbook — soft-launch playbook

The staged ramp-up from internal dogfooding to public launch. Audience:
the launch decision committee (architect-team lead, security-team lead,
on-call lead, product owner) plus the on-call executing each stage
transition.

## Trigger

Run this playbook when **all** of the following hold:

- All Phase 0 tickets in `docs/PRODUCTION-TODO.md` closed (rate
  limiting, log redaction, security headers, deploy + rollback
  runbooks, ADRs 0001–0005 shipped).
- PEN-1 passed OR findings scheduled with named owners before Stage 2.
- Architect-team has signed a launch-readiness PR confirming the
  doctrines are not violated by the proposed ramp.

If any condition fails, do not start. Re-enter Phase 0 / Phase 5 work
in `docs/PRODUCTION-TODO.md` and re-trigger.

## Decision authority

| Decision | Authority |
|---|---|
| Open or hold a stage transition | architect-team lead (after committee sign-off) |
| Pull the rollback handle inside a stage | on-call lead (no committee required; D14 fail-closed) |
| Declare a security finding stage-blocking | security-team lead |
| Approve a stage retrospective and close the stage | architect-team lead + security-team lead |
| Communicate stage status externally | product owner |

The on-call lead has unilateral authority to roll back. The committee
opens the next stage; it does not vote on exiting a broken one. D14:
rollback first, post-mortem after. See
[rollback-deployment](rollback-deployment.md) § "Doctrine reminder."

## Stages

### Stage 0 — Internal dogfooding (~10 declarants)

**Audience:** RÉCOR engineering team + architect-reviewer + security-
reviewer + up to three close collaborators (BUNEC product liaison,
ARMP technical lead, one consortium observer). All have ≥ 1 day of
portal walkthroughs already.

**Goals:**

- Confirm the happy path end-to-end on production cluster (production
  OIDC, production HMAC, production-tier Postgres).
- Surface UX bugs before any external user sees them.
- Validate the OBS-1 dashboards receive the metrics named in the SLO
  tables of `applications/declarant-portal/CLAUDE.md` and
  `services/declaration/CLAUDE.md`.

**Gates to enter Stage 0:**

- [`deploy-new-version`](deploy-new-version.md) exercised against the
  production cluster with all eight steps green at least once.
- ArgoCD applications present and `Synced + Healthy`:
  ```bash
  for app in recor-declaration recor-verification-engine recor-portal; do
    kubectl -n argocd get application "$app" \
      -o jsonpath='{.status.sync.status},{.status.health.status}{"\n"}'
  done
  # Expected: Synced,Healthy on every line.
  ```
- Per-service `/healthz` and `/readyz` return 200 from outside the
  cluster (per [deploy-new-version](deploy-new-version.md) Step 7).
- Grafana `RECOR / Service health` (OBS-1) shows live
  `recor_declarations_submitted_total`,
  `recor_verification_cases_total{lane}`,
  `recor_outbox_undispatched`, `recor_outbox_dlq_size` in the last
  15 minutes (proves prod scrape, not just dev compose).
- On-call rotation handbook distributed; primary on-call has acked a
  synthetic PagerDuty page within 5 minutes.

**Gates to exit Stage 0 (every one must hold for 5 consecutive
production days):**

- ≥ 10 successful declarations submitted by ≥ 5 distinct OIDC
  principals; `recor_declarations_submitted_total` counter shows the
  count and matches what the team posted in the dogfooding channel.
- Zero SEV-1, zero SEV-2, ≤ 1 SEV-3 (per the severity scale in
  [incident-response-template](incident-response-template.md)).
- `recor_outbox_dlq_size` stayed at 0 for the entire window
  (`max_over_time(recor_outbox_dlq_size[5d]) == 0`).
- `recor_relay_delivery_latency_seconds` p99 < 5s (detects mis-wired
  metrics; load is trivial here).
- `POST /v1/declarations` p99 < 500ms (the SLO from
  `services/declaration/CLAUDE.md`).
- 100% of cases reach a terminal lane within 60s
  (`recor_verification_cases_total{lane!="pending"}`).
- Every UX issue raised has a ticket — not "we'll fix it." D08.
- Stage 0 retrospective filed (see § Retrospective format).

**Rollback triggers (any one fires the rollback procedure for Stage 0):**

- Any SEV-1 (data integrity or security event).
- Two or more SEV-2 incidents in one 24-hour window.
- DLQ size > 5 rows at any moment (`recor_outbox_dlq_size > 5`).
- A production-only failure that did not reproduce in dev compose
  (treat as a latent infrastructure defect; pause and investigate).
- HMAC verification failure on any production D↔V envelope; ADR-0005
  rotation invariant must hold.

### Stage 1 — Pilot (100 declarants)

**Audience:** invited partners — 50 BUNEC pilot declarants (legal
entities), 20 partner-law-firm declarants, 30 observer accounts.
Public registration closed; admission by OIDC allowlist.

**Goals:**

- Stress idempotency under real replay patterns — partner scripts will
  retry on network blips.
- Observe the verification engine under real entity-shape diversity
  (multi-tier ownership, foreign chains, partnerships).
- Validate OPS-1 rate limit (60 rpm / 10 burst) does not collapse
  legitimate batch submissions.

**Gates to enter Stage 1 (in addition to every Stage 0 exit gate):**

- PEN-1 critical findings = 0. High findings fixed or accepted-risk in
  writing by the security-team lead (with a closing ticket ≤ 30 days).
- Threat-model gaps G1 (in-DB audit chain) and G4 (DBA-statement
  audit) closed per `docs/security/threat-model.md` § "Gaps blocking
  production". G3 (PII at rest) must close before Stage 2; at Stage 1
  it may be open if the encryption-at-rest ticket is in active dev.
- OBS-1 alert rules loaded in Prometheus; ≥ 1 rule fired in test
  (`promtool test rules` returns 0 and a synthetic page delivered).
- OPS-1 rate limit exercised in production at default thresholds and
  observed returning 429 + `Retry-After`.
- OIDC issuer has issued credentials to ≥ 100 pilot principals;
  allowlist loaded.
- [`restore-database-from-backup`](restore-database-from-backup.md)
  dry-run end-to-end against non-prod within 14 days; COMP-5 RTO/RPO
  documented.
- Partner pre-stage announcement sent (see § Communication templates).

**Gates to exit Stage 1 (every one must hold for 10 consecutive
production days):**

- ≥ 100 declarations submitted by ≥ 80 distinct principals
  (`count(count by (principal_subject_redacted) (recor_declarations_submitted_total)) >= 80`).
- Zero SEV-1; ≤ 1 SEV-2; ≤ 5 SEV-3 over the window.
- `recor_declarations_submitted_total{result="2xx"}` /
  `recor_declarations_submitted_total` ≥ 0.995 (99.5% submit success).
- `POST /v1/declarations` p99 < 500ms across the entire window
  (the SLO from `services/declaration/CLAUDE.md`).
- `recor_outbox_dlq_size` average over the window < 2; max < 10.
- `recor_relay_delivery_latency_seconds` p99 < 30s (matches the OBS-1
  alert threshold).
- Lane outcome distribution is non-degenerate: every lane
  (`green`, `yellow`, `red`, `pending-evidence`) has been entered by at
  least one case (`recor_verification_cases_total{lane=*} > 0` for each
  defined lane).
- Dempster-Shafer fusion (ADR-0002) has produced ≥ 1 conflict-resolved
  outcome; debug output for ≥ 5 randomly-sampled cases attached to the
  Stage 1 retrospective.
- HMAC rotation per ADR-0005 exercised at least once via
  [`hmac-secret-rotation`](hmac-secret-rotation.md); both slots
  verified and close-out done.
- COMP-1 GDPR/OHADA endpoints ship; ≥ 1 pilot successfully exercised
  right-to-access.
- Stage 1 retrospective filed.

**Rollback triggers:**

- Any SEV-1.
- Two SEV-2 in one 24-hour window OR three SEV-2 in the stage.
- `recor_outbox_dlq_size > 50` for > 10 minutes (matches the OBS-1
  alert threshold for "DLQ growing"; jump straight to
  [`dlq-inundation`](dlq-inundation.md)).
- `POST /v1/declarations` p99 > 1.5s for 15 consecutive minutes
  (3 × the SLO).
- HMAC verification failure rate > 0.01% (any failure is suspicious;
  this is the trip-wire that pages the security team per the threat
  model § D↔V loop).
- A pen-test follow-up finding rated High by the security-team lead is
  discovered during the stage.
- Verification engine produces a lane outcome that the architect-team
  lead rules incorrect on review of a partner-reported case. (Data
  correctness is non-negotiable; pause the stage and audit the fusion
  outputs.)

### Stage 2 — Cohort (1000 declarants)

**Audience:** broader regulated community — every regulated entity
within a single sector (e.g. extractive industries first, per COMP-4).
Still OIDC-allowlisted, but the allowlist is now sector-wide.

**Goals:**

- Real verification-lane distribution. Validate fusion thresholds
  (ADR-0002) produce a defensible 1000-case distribution.
- Real DLQ rates. Operable, not just theoretically operable.
- Real OIDC issuer load. ADR-0004 assumed JWKS caching keeps the IdP
  cool under burst; first stage where that holds under load.

**Gates to enter Stage 2 (in addition to every Stage 1 exit gate):**

- COMP-1..5 acceptance gates green (`docs/PRODUCTION-TODO.md` Phase 4):
  - COMP-1 GDPR/OHADA endpoints shipped + legal sign-off
  - COMP-2 audit-log immutability migration applied;
    `declaration_events` grants verified INSERT/SELECT only
  - COMP-3 data classification published; OPS-2 redacts exactly the
    PII fields
  - COMP-4 regulatory mapping signed off by AML/CFT counsel
  - COMP-5 DR drill completed within 90 days; RTO/RPO met
- Threat-model gap G3 (PII at rest) closed; encryption-at-rest ticket
  shipped (`docs/security/threat-model.md` § Gaps).
- OBS-2 merged; observability-smoke required on the last 10
  consecutive merges to `main`
  (see `docs/security/branch-protection.md`).
- Supply-chain controls per [`supply-chain`](supply-chain.md) current:
  every production image has a green cosign signature + SLSA L4
  provenance.
- On-call rotation actively held ≥ 60 days; ≥ 2 trained secondaries.
- `restore-database-from-backup` dry-run within 30 days.
- COMP-5 quarterly DR drill on the calendar within 90 days.
- Pre-stage announcement to the regulated sector issued (see
  § Communication templates).

**Gates to exit Stage 2 (every one must hold for 30 consecutive
production days):**

- ≥ 1000 declarations submitted by ≥ 700 distinct principals.
- Zero SEV-1; ≤ 2 SEV-2; ≤ 15 SEV-3 over the window.
- `recor_declarations_submitted_total{result="2xx"}` /
  `recor_declarations_submitted_total` ≥ 0.999 (99.9% submit success).
- `POST /v1/declarations` p99 < 500ms across the window (the SLO).
- Portal SLOs hold against real partner network conditions: FCP < 1.5s
  on low-end-Android-3G samples in
  `applications/declarant-portal/CLAUDE.md` § SLOs; measured via
  Real-User Monitoring data (the RUM integration is part of OBS-1
  scope or its follow-up).
- `recor_outbox_dlq_size` average over the window < 5; max < 50.
- DLQ accumulation rate < 10 rows / day (computed as
  `increase(recor_outbox_dlq_size[1d])` averaged over the window).
- Lane outcome distribution stable for ≥ 7 days (no day-to-day shift
  > 10 percentage points in any lane's share, indicating either a
  parameter drift or upstream data change).
- OIDC issuer p99 JWKS-fetch latency < 1s
  (`histogram_quantile(0.99, recor_oidc_jwks_fetch_latency_seconds_bucket)`);
  no OIDC-issuer-outage incidents triggered.
- Zero unresolved Critical or High security findings from the pen-test
  (PEN-1) or from any incident in the window.
- Stage 2 retrospective is filed.

**Rollback triggers:**

- Any SEV-1.
- Three SEV-2 in one 24-hour window.
- `recor_outbox_dlq_size > 100` for > 10 minutes
  (the OBS-1 alert threshold; see [`dlq-inundation`](dlq-inundation.md)).
- `POST /v1/declarations` 5xx rate > 1% for > 5 minutes (the
  [`incident-response-template`](incident-response-template.md) SEV-1
  threshold).
- `recor_oidc_verify_total{result="fail"}` /
  `recor_oidc_verify_total` > 0.05 for > 5 minutes (see
  [`oidc-issuer-outage`](oidc-issuer-outage.md)).
- A regulatory authority (ARMP, ANIF, BEAC, DGI) reports a data
  correctness or availability concern about a production case.
- DR drill conducted during the stage fails its RTO or RPO target.

### Stage 3 — Public launch

**Audience:** open registration. Anyone with a valid Cameroon-issued
identity credential may register and submit a declaration.

**Goals:**

- Prove the SLOs hold at full population scale (declaration service's
  99.95% availability per `services/declaration/CLAUDE.md`; portal
  FCP/LCP/TTI budgets per `applications/declarant-portal/CLAUDE.md`).
- Demonstrate rollback remains viable at scale.

**Gates to enter Stage 3 (in addition to every Stage 2 exit gate):**

- Stage 2 resident ≥ 30 days at full 1000-declarant volume.
- Launch decision committee signed a written launch-readiness PR
  citing every exit gate above with the query and observed value.
- Capacity headroom at Stage 2 peak: declaration CPU < 60%; Postgres
  pool < 50% of `db_pool_max_connections` (launch can spike ~5×).
- On-call rotation has named secondary coverage for the first two
  weeks; handbook updated.
- Public-launch announcement scheduled; press contact briefed.
- COMP-1..5 operationally exercised at Stage 2 scale (right-to-access
  requests successfully fulfilled at non-trivial volume).

**Rollback triggers:**

SEV thresholds tighten one notch at Stage 3 because the audience is
the general public; rollback bias is "early":

- Any SEV-1.
- Two SEV-2 in 24 hours.
- `recor_outbox_dlq_size > 100` for > 5 minutes (was 10 at Stage 2).
- `POST /v1/declarations` 5xx rate > 0.5% for > 5 minutes (was 1%).
- `recor_relay_delivery_latency_seconds` p99 > 60s for > 10 minutes
  (Stage 2 SLO was 30s; this is the breach threshold).
- Lane outcome distribution shifts > 10 percentage points day-to-day
  in any lane (suggests an upstream data correctness issue worth
  pausing for).
- Any data integrity event (a verification outcome reaching a
  consumer that the architect-team lead rules incorrect on review;
  see threat-model § Verification engine § R rows).
- Any security event (suspected key compromise, unauthorised access,
  secret leak) per [`incident-response-template`](incident-response-template.md)
  trigger list.

No Stage 3 exit gate. Stage 3 is steady state; the doctrines (D09,
D12) govern from this point.

## Cross-stage controls

### Feature flags and kill switches

- **Soft kill (submit):** `RATE_LIMIT_PER_MIN=0` refuses all submits;
  no deploy required:
  ```bash
  kubectl -n recor set env deploy/declaration RATE_LIMIT_PER_MIN=0
  kubectl -n recor rollout status deploy/declaration --timeout=2m
  ```
- **Hard kill:** scale to zero — last-resort, API fully down:
  ```bash
  kubectl -n recor scale deploy/declaration --replicas=0
  ```
  Portal shows the connectivity error block (D14 fail-closed).
- **V-engine pause:** scale verification-engine to zero. Submits queue
  in the outbox and resume on restart (ADR-0003).
- **OIDC dev-override is NOT a kill switch.** Refused in production by
  `Config::from_env` (threat model § Auth § E). Bypassing it is a D17
  violation.

### Rate limits per stage

OPS-1 default is `RATE_LIMIT_PER_MIN=60` / `RATE_LIMIT_BURST=10` per
principal. Stage overrides:

| Stage | per min | burst | Rationale |
|---|---|---|---|
| 0 | 60 | 10 | Humans are slow. |
| 1 | 60 | 10 | Partner scripts retry; burst=10 absorbs blips. |
| 2 | 60 | 20 | Larger burst for batch submission tooling. |
| 3 | 120 | 30 | Public launch peak; revisit after 30 days. |

Set env on the declaration deployment and roll. Do not co-mingle with
a code deploy — change one variable at a time for attribution.

### Communication plan per stage

- **Stage 0:** internal channel `#dogfood-recor` only. No external
  communications.
- **Stage 1:** partner email list; partners must acknowledge receipt.
  A status page (URL to be added when the status page lands) is
  internal-only but visible to partners on request.
- **Stage 2:** sector-wide email + ARMP regulatory bulletin.
  Status page becomes public.
- **Stage 3:** press release coordinated with the consortium
  communications lead.

### Retrospective format (per stage)

A stage closes when exit gates hold AND the retrospective PR is
merged. Use [`incident-response-template`](incident-response-template.md)
as the structural reference (the stage retrospective is the
non-incident analog). Required sections:

- Stage scope (audience, date range, declarant count reached).
- Gate-by-gate evidence: each exit gate, the query, the observed value.
- What went well (2–5 specifics).
- What did not (2–5 specifics).
- Action items with owners + tickets, per the incident template.
- Doctrine review checklist (D01, D14, D16 minimum).

File at `docs/launch/stage-{0,1,2}-retrospective.md`. Reviewed by the
launch decision committee before the next stage opens.

## Rollback procedures by stage

### Rollback from Stage 0

Stage 0 population is small; code-level rollback suffices in most
cases.

1. Run [`rollback-deployment`](rollback-deployment.md) end-to-end.
2. Notify `#dogfood-recor`: "Stage 0 rolled back at HH:MM UTC.
   Dogfooding paused. INC-YYYY-MMDD-NN."
3. Stage 0 data is disposable. If the bad deploy wrote bad rows, the
   operator may run a targeted delete on `declaration_events` with
   security-team-lead approval — the sole Stage 0 exception to the
   threat-model § Database § R procedural gate.
4. File a Stage 0 retrospective citing the rollback as the closing
   event; Stage 0 does not auto-reopen.

### Rollback from Stage 1

Pilot data is real; database delete is no longer an option. Rollback
returns code but preserves data.

1. Run [`rollback-deployment`](rollback-deployment.md).
2. Suspect DLQ rows from the bad deploy: work
   [`dlq-inundation`](dlq-inundation.md) to inspect and replay
   post-rollback. Do not delete DLQ rows without a documented
   data-correctness rationale (threat-model § Operator surface § E).
3. Postgres corruption: run
   [`restore-database-from-backup`](restore-database-from-backup.md).
   COMP-5 RPO governs the loss window; if breached, on-call lead
   escalates to architect-team lead.
4. Hold Stage 1 closed: OIDC allowlist remains, but the "active"
   announcement is withdrawn (see § Rollback announcement template).
5. File a SEV-2-or-greater post-mortem per
   [`incident-response-template`](incident-response-template.md).
6. Re-enter Stage 1 only after post-mortem action items ship AND a
   Stage 0 dry-run is re-executed against the rolled-back system.

### Rollback from Stage 2

Same as Stage 1, plus three additions:

1. Notify the regulated sector explicitly (see § Rollback announcement
   template). Not optional — regulatory authorities learn from us, not
   the press.
2. If rollback requires
   [`restore-database-from-backup`](restore-database-from-backup.md),
   COMP-5 RTO governs the announcement timeline. On-call lead
   announces an expected recovery window within 15 minutes (per
   [`incident-response-template`](incident-response-template.md)
   § Step 1).
3. Stage 2 reopens only after the launch decision committee meets and
   signs a stage-reopen PR citing every Stage 2 entry gate again.

### Rollback from Stage 3

Rollback from Stage 3 returns the platform to the Stage 2 OIDC
allowlist. Public registration is paused at the IdP, not in RÉCOR;
the platform keeps running for the existing Stage 2 cohort.

1. Run [`rollback-deployment`](rollback-deployment.md) for the
   code-level fix.
2. Product owner instructs the IdP operator to revert to the Stage 2
   sector-only allowlist. New public registrations refused at the IdP;
   onboarded declarants' tokens stay valid.
3. Public communications go out within 60 minutes:
   "Public launch paused while we investigate INC-YYYY-MMDD-NN.
   Existing declarants continue to operate; new registrations
   temporarily closed."
4. Post-mortem is SEV-1 by definition; published externally within
   five business days.
5. Re-entering Stage 3 requires the full Stage 3 entry-gate checklist
   plus architect + security + product owner triple sign-off.

## Gates referenced (summary index)

Quick reference so any reviewer can audit readiness without re-reading
every stage.

| Gate text | Observable |
|---|---|
| Verification lane distribution stable | `recor_verification_cases_total{lane}` rate-of-change < 10 pp / day over 7 days |
| Zero unresolved critical security findings | PEN-1 report file + GitHub issues labelled `pentest-followup` with no `critical` or `high` open |
| All COMP-1..5 acceptance gates green | `docs/PRODUCTION-TODO.md` Phase 4 tickets marked CLOSED + linked PRs merged |
| Observability dashboards calibrated | OBS-1 dashboards present in production Grafana; metrics named in this runbook visible with non-empty data in the last 1 h |
| DLQ accumulation rate < N per day | `increase(recor_outbox_dlq_size[1d])` averaged over the gate window |
| `POST /v1/declarations` p99 within SLO | `histogram_quantile(0.99, recor_request_duration_seconds_bucket{route="POST /v1/declarations"}) < 0.5` |
| Submit success rate within SLO | `sum(rate(recor_declarations_submitted_total{result="2xx"}[5m])) / sum(rate(recor_declarations_submitted_total[5m]))` |
| HMAC verification failure rate | `recor_hmac_verify_total{result="fail"} / recor_hmac_verify_total` over the gate window |
| OIDC issuer load | `histogram_quantile(0.99, recor_oidc_jwks_fetch_latency_seconds_bucket)` |
| Image-signature provenance | `cosign verify` per [`image-verification`](image-verification.md) returns 0 for every production image SHA |

Metric names whose OBS-1 PR has not yet landed are listed verbatim
above; the OBS-1 implementation must satisfy them.

## Communication templates

### Pre-stage announcement (to partners)

```
Subject: RÉCOR — opening Stage <N> on <YYYY-MM-DD>

We are opening Stage <N> on <YYYY-MM-DD UTC>. Audience: <from the
stage section above>.

Capabilities: <high-level summary>.
Rate limit: <X> requests / minute / principal (OPS-1; see this
playbook § Rate limits per stage).
Status page: <URL>.

Please submit via the portal at <URL>; do not script the API without
acknowledging the rate-limit contract. Report anomalies to
<support email> with case_id and UTC timestamp. Office hour on
<date> at <HH:MM UTC>.

Stage <N> runs for at least <gate window>. Retrospective published at
<docs/launch/stage-<N>-retrospective.md> when the stage closes.

— The RÉCOR launch team
```

### Mid-stage status update

```
Subject: RÉCOR — Stage <N> status, <YYYY-MM-DD>

Declarations submitted: <count> | Distinct declarants: <count>
Open incidents: <count> (SEV-2+: <count>)
Exit gates green: <X of Y>

Next update: <YYYY-MM-DD>.
```

### Rollback announcement

```
Subject: RÉCOR — Stage <N> rollback at <HH:MM UTC>

Reason: <one-line>.

Platform status: <available / unavailable / existing declarants only —
match the stage rollback procedure above>.

Receipts already issued remain valid; submissions before <HH:MM UTC>
are not at risk. Incident INC-YYYY-MMDD-NN declared; post-mortem at
<docs/incidents/INC-YYYY-MMDD-NN.md> within three business days (per
incident-response-template.md § Step 2).

Next status update: <HH:MM UTC>.

— The RÉCOR on-call team
```

### Post-stage retrospective publication

```
Subject: RÉCOR — Stage <N> retrospective published

Retrospective: <docs/launch/stage-<N>-retrospective.md>.

Outcomes: <declarant count + reliability summary>.
Action items: <count> tracked under the `launch-followup` label.

Stage <N+1> opens on <date or "pending committee sign-off">.
```

## Verification (when has this runbook been executed)

A stage is executed when its entry gates were green at open-time (PR
evidence), its exit gates were green at close-time (retrospective
evidence), the retrospective is merged, and the next stage's checklist
references the previous retrospective by file path.

## Rollback (this runbook itself)

Docs-only: `git revert <sha>` + PR. No operational state to undo.

## Related runbooks

- [`deploy-new-version`](deploy-new-version.md)
- [`rollback-deployment`](rollback-deployment.md)
- [`incident-response-template`](incident-response-template.md)
- [`oncall-triage-tree`](oncall-triage-tree.md)
- [`dlq-inundation`](dlq-inundation.md)
- [`oidc-issuer-outage`](oidc-issuer-outage.md)
- [`hmac-secret-rotation`](hmac-secret-rotation.md)
- [`restore-database-from-backup`](restore-database-from-backup.md)
- [`observability-prod-stack`](observability-prod-stack.md)
- [`image-verification`](image-verification.md)
- [`supply-chain`](supply-chain.md)

## Related ADRs

- [`ADR-0001`](../adr/0001-event-sourcing-declaration-aggregate.md) — the event-sourced aggregate that makes data-preserving rollback feasible
- [`ADR-0002`](../adr/0002-dempster-shafer-fusion.md) — the fusion math whose lane distribution Stage 2 must validate
- [`ADR-0003`](../adr/0003-http-outbox-relay-d-v.md) — the outbox-relay topology that determines DLQ behaviour at each stage
- [`ADR-0004`](../adr/0004-oidc-jwks-principal-authentication.md) — the auth contract that the OIDC issuer must hold through Stages 1–3
- [`ADR-0005`](../adr/0005-hmac-channel-rotation.md) — the HMAC rotation that Stage 1 must exercise

## Related tickets (`docs/PRODUCTION-TODO.md`)

- OPS-1 (rate limiting) — drives the stage rate-limit table
- OPS-2 (PII redaction) — required before any external user sees the platform
- OBS-1 (Prometheus + Grafana) — the metric names cited in every gate
- OBS-2 (observability-smoke required check) — Stage 2 entry gate
- COMP-1..COMP-5 (compliance + DR) — Stage 2 entry gates
- PEN-1 (penetration test) — Stage 1 entry gate
- R-DECL-9 (Fabric anchoring) — closes threat-model gap G1
