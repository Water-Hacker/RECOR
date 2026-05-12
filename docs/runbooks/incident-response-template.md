# Runbook — incident response template

The post-mortem format. Use this template to write the post-mortem
after an incident is closed. The template doubles as the operational
log the on-call keeps **during** the incident; the final post-mortem
is a clean copy of the log with reflective sections added.

## Trigger

- An incident has been declared (a page was acknowledged via
  [oncall-triage-tree](oncall-triage-tree.md)) AND
- The user-visible symptom is one of:
  - Sustained 5xx error rate > 1 % for any RÉCOR-owned API for > 5 min
  - Any declared SLO breach (see service CLAUDE.md SLO tables)
  - Data integrity event (incorrect verification outcome reaching a
    consumer; outbox row dispatched twice; audit ledger anomaly)
  - Security event (suspected key compromise, unauthorised access,
    secret leak)

A page that resolves in < 5 minutes with no user impact is a
**near-miss** and gets a near-miss note (see § "Near-miss notes" at
the bottom), not a full post-mortem.

### If the trigger is a pen-test finding

If the incident was triggered by an external penetration-test
finding (PEN-1, `docs/security/pen-test-prep.md` /
`docs/security/pen-test-rules-of-engagement.md`), the post-mortem
flow is the same as for any other incident, with these differences:

- **Severity floor**: a Critical or High finding from the vendor
  that the engineering team independently reproduces (per D17 and
  RoE § 13) is at minimum a SEV-2 even if no user was affected,
  because the gap existed in production code. Medium and Low
  findings file as post-engagement tickets in `docs/PRODUCTION-TODO.md`
  rather than incidents.
- **Data integrity field**: defaults to `possibly-affected` for any
  Critical / High finding until the engineering team's reproduction
  confirms otherwise. Audit the affected paths even if the vendor
  did not exfiltrate; the vendor's restraint is not the platform's
  defence.
- **External communications**: governed by the engagement's
  disclosure protocol (RoE § 10). The 90-day embargo holds for
  external comms; internal comms within the consortium follow the
  normal channel. **Do not publish a public notice during the
  embargo without security-team lead + Vendor concurrence.**
- **Regulatory reporting obligation**: re-check. Some pen-test
  findings may trigger ANIF / BEAC notification obligations that
  the routine incident flow does not pattern-match against; the
  security-team lead consults counsel.
- **Action items**: every Critical / High finding closes with a
  mitigation PR within the engagement's embargo window. The
  post-mortem links the mitigation PR; the vendor re-test (RoE
  § 11) verifies closure.
- **Doctrine review**: add a row to the doctrine checklist below
  for D14 (was the engagement explicitly forbidding a destructive
  action that the platform fail-opened on?), D17 (was a trust
  boundary improperly extended? — the answer is usually "yes" for
  any S/T/E finding), and D24 (the standard is non-negotiable;
  the finding by definition crossed it).

See also: `docs/security/pen-test-prep.md` § "Engagement logistics"
for the in-engagement escalation path (which routes through this
runbook).

## Prerequisites

- The incident has a unique ID. Format: `INC-YYYY-MMDD-NN` where NN is
  the incident sequence number that day (`INC-2026-0512-01` is the
  first incident of 2026-05-12).
- You have authoring access to `docs/incidents/` (CODEOWNERS routes
  the directory to `@recor/incident-team` + the implicated service's
  team).
- The incident channel `#inc-INC-YYYY-MMDD-NN` is archived but not
  deleted (channel-export PDF attached to the post-mortem).

## Procedure

### Step 1 — During the incident, keep the live log

Open `docs/incidents/INC-YYYY-MMDD-NN.md` and start filling the
template below as the incident unfolds. Treat it like a flight
recorder, not a polished document. Tense: past — every entry is "I
ran X and saw Y," not "we should run X."

### Step 2 — After resolution, fill in the reflective sections

The reflective sections (Root cause, Contributing factors,
What went well, What did not, Action items) are filled within
**three business days** of resolution. D08 (no dangling threads):
the incident is not closed until the post-mortem ships.

### Step 3 — Review with the team

Within five business days of resolution:

1. Run a 45-minute blameless review meeting. Required attendees: the
   on-call who held the incident, the team that owns the implicated
   service, the SRE lead.
2. Walk the timeline. The goal is "what could the platform have done
   to make this less likely / shorter / less damaging" — never
   "who broke it."
3. Action items get owners + tickets + due dates. An action item
   without a ticket is performative.

### Step 4 — Open a PR and merge

```bash
git checkout -b incident/INC-YYYY-MMDD-NN
git add docs/incidents/INC-YYYY-MMDD-NN.md
git commit -m "docs(incidents): INC-YYYY-MMDD-NN — <one-line summary>"
git push -u origin incident/INC-YYYY-MMDD-NN
gh pr create --title "docs(incidents): INC-YYYY-MMDD-NN — <summary>" \
             --body "Post-mortem for INC-YYYY-MMDD-NN."
```

CODEOWNERS routes the PR to the implicated team plus
`@recor/incident-team`. Both sign off.

### Step 5 — Track action items to completion

Action items live as GitHub issues, labelled `incident-followup`.
They are reviewed monthly by the SRE lead. Stale > 60 days =
escalated to engineering leadership. D08 again.

## Verification

- The file `docs/incidents/INC-YYYY-MMDD-NN.md` exists on `main`
- Every action item has a ticket
- The incident channel is archived
- The on-call rotation channel notes link to the post-mortem
- If the incident touched a runbook, that runbook has been updated
  (separate PR is fine) to incorporate any lessons learned

## Rollback

A post-mortem PR does not modify production state, so there is no
operational rollback. If the post-mortem itself is inaccurate, open a
follow-up PR to correct it; never silently rewrite history on `main`.

---

## The template

Copy the section below into `docs/incidents/INC-YYYY-MMDD-NN.md` and
fill it in.

```markdown
# INC-YYYY-MMDD-NN — <one-line summary>

**Date:** YYYY-MM-DD
**Severity:** SEV-1 | SEV-2 | SEV-3 | SEV-4
**Duration:** HH:MM (from first user impact to confirmed resolution)
**On-call (primary):** <name>
**On-call (secondary, if engaged):** <name>
**Implicated service(s):** declaration | verification-engine | declarant-portal | infra | other
**Status:** resolved | mitigated-only | open

## Summary

Two-to-four sentences. What broke, who was affected, how was it
restored. Plain language; a stakeholder who does not know the
codebase should be able to read this section and understand the
impact.

## Impact

- **Users affected:** estimated count + tenant scope
- **Requests affected:** error count from telemetry
- **Data integrity:** none | possibly-affected | confirmed-affected
  (if anything other than "none", file an immediate follow-up to
  audit the affected rows / events)
- **External communications sent:** yes/no; if yes, link the notice
- **Regulatory reporting obligation:** yes/no (e.g. ANIF
  notification thresholds, BEAC reporting); if yes, owner + status

## Timeline (UTC)

All times are UTC. Use 24-hour format.

| Time | Event |
|---|---|
| HH:MM | <event 1> |
| HH:MM | <event 2> |
| ... | ... |

The timeline starts at "first observable signal of the failure" — not
"page received" — and ends at "telemetry confirms full recovery."

Minimum entries any timeline must have:

- First observable signal (the earliest dashboard / log / metric that
  showed the problem in retrospect)
- Detection (the moment the on-call learned of it: page, manual
  report, dashboard glance)
- Acknowledgement
- Mitigation deployed (the thing that stopped the bleeding)
- Root cause identified
- Full recovery confirmed via telemetry
- Incident declared resolved

## Root cause

One-paragraph technical explanation. The root cause is the answer to
"why did this happen, in a sentence" — not "what we did to fix it"
(that's mitigation).

If the root cause is "unknown," say so. An unknown root cause is a
SEV-2 follow-up: investigate or accept the risk explicitly.

## Contributing factors

The conditions that made the root cause possible or harder to detect.
Examples: insufficient telemetry on the failing path; a deferred
ticket that would have prevented this; a deployment-time validation
that did not run.

## What went well

Two-to-five bullets. Be specific. "The runbook was useful" is not
specific; "the dlq-inundation runbook's psql query at step 3 surfaced
the stuck row in under a minute" is.

## What did not

Two-to-five bullets. Same specificity rule. Blameless: phrase as
"the alerting threshold was too high" rather than "X did not notice."

## Action items

Numbered list. Each item: title, owner, ticket, due date, doctrine
addressed if any.

1. **<title>** — Owner: @<handle> — Ticket: #<num> — Due:
   YYYY-MM-DD — Doctrine: D## (if applicable)
2. ...

Action items fall into three categories:

- **Prevent recurrence** — make this specific failure mode impossible
  or much rarer
- **Reduce time-to-detection** — make the next instance of this class
  of failure visible sooner
- **Reduce time-to-mitigation** — shorten the path from detection to
  fix (often a runbook update)

A post-mortem with zero action items is suspicious. Either the
incident was a non-event (in which case it should be a near-miss
note, not a post-mortem) or the team is not looking hard enough.

## Doctrine review

Walk the doctrines (V1 P2) and note which ones are implicated by this
incident:

- [ ] D01 (completeness) — did partial delivery contribute?
- [ ] D04 (tests) — would a test we could have written have caught
      this?
- [ ] D13 (idempotency) — was a non-idempotent operation involved?
- [ ] D14 (fail-closed) — did a boundary fail-open when it should
      not have?
- [ ] D15 (cryptographic provenance) — was provenance violated?
- [ ] D16 (observability) — was the gap an observability gap?
- [ ] D17 (zero trust) — was a trust boundary improperly extended?
- [ ] D18 (no secrets) — did a secret leak / get logged?

Each box ticked → an action item against that doctrine.

## Communication log

- T+ HH:MM — first internal post in #oncall-recor
- T+ HH:MM — external notice (if any) at <link>
- T+ HH:MM — resolution notice in #oncall-recor
- T+ HH:MM — post-mortem PR opened (#<num>)

## Attachments

- Grafana dashboard snapshots (export as PDF; commit to
  `docs/incidents/INC-YYYY-MMDD-NN/grafana-*.pdf`)
- Relevant log excerpts (redacted; PII redaction per OPS-2)
- Incident channel transcript (PDF export)
- Any one-off scripts run during mitigation (commit to
  `docs/incidents/INC-YYYY-MMDD-NN/scripts/`)
```

---

## Near-miss notes

A near-miss is a page that resolved before user impact, or a manual
discovery of a latent defect that would have caused an incident if
unaddressed. Near-misses get a short note, not a full post-mortem.

Location: append to `docs/incidents/NEAR-MISSES.md`.

Format (≤ 200 words):

```markdown
## YYYY-MM-DD — <one-line summary>

**Discovered by:** <name>
**Discovered via:** <how — page, dashboard, code review, customer report>
**What would have happened if unaddressed:** <one-sentence impact>
**What was done:** <one-sentence mitigation>
**Follow-up ticket:** #<num> (if any)
```

Near-misses are reviewed monthly with the same audience as
post-mortems. A near-miss that recurs becomes a post-mortem even if
no user is affected.

## Severity scale

| SEV | Definition | Example |
|---|---|---|
| SEV-1 | Platform unavailable for > 5 minutes OR confirmed data integrity event OR security event | Declaration service down for 20 min; outbox row dispatched twice; HMAC secret leaked |
| SEV-2 | Material degradation but core function preserved; SLO breach without complete outage | Verification engine p99 latency > 2 × SLO for 30 min |
| SEV-3 | Minor degradation; some users affected but workaround exists | Declarant portal 5xx on one page; users can submit via API |
| SEV-4 | Internal-only impact; non-prod affected; near-miss with rapid recovery | Staging deploy failed; observability stack 5xx for < 5 min |

Every SEV-1 and SEV-2 gets a full post-mortem. SEV-3 gets a short
post-mortem (Summary, Impact, Timeline, Root cause, Action items —
the rest is optional). SEV-4 gets a near-miss note.

## Related runbooks

- [oncall-triage-tree](oncall-triage-tree.md)
- [rollback-deployment](rollback-deployment.md)
- [dlq-inundation](dlq-inundation.md)
- [oidc-issuer-outage](oidc-issuer-outage.md)
- [bunec-adapter-outage](bunec-adapter-outage.md)
- [restore-database-from-backup](restore-database-from-backup.md)
- [observability-prod-stack](observability-prod-stack.md)
