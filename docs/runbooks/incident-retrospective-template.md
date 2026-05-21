# Incident Retrospective Template

**Ticket:** TODO-073
**Owner:** @recor/incident-team
**Last updated:** 2026-05-20

This is the canonical retrospective template. It is used for every SEV-1
and SEV-2 post-mortem and for SEV-3 incidents where the root cause
warrants deeper analysis. Copy the template section below into
`docs/incidents/INC-YYYY-MMDD-NN.md` before or during the retrospective
meeting.

The retrospective is **blameless**. The platform is the subject; the
goal is "what could the platform, its tooling, and its processes have
done differently" — never "who broke it." Every contributing factor is
a system property, not a character flaw.

## How this template relates to the incident-response runbook

`docs/runbooks/incident-response-template.md` governs the **live** log
kept during the incident. This document is the **reflective** post-mortem
filled in within three business days of resolution. The two artefacts
are separate files. The live log becomes the evidence base for the
retrospective; the retrospective is the clean summary shipped to
main.

## Prerequisites before the retrospective meeting

- The incident is closed: telemetry confirms full recovery.
- The incident record `docs/incidents/INC-YYYY-MMDD-NN.md` exists and
  contains the live-log timeline.
- The on-call incident channel is archived (not deleted).
- Any one-off scripts run during mitigation are committed to
  `docs/incidents/INC-YYYY-MMDD-NN/scripts/`.

## Retrospective meeting format

| Time | Activity |
|---|---|
| 0–5 min | Facilitator opens, states blameless norm, confirms everyone has read the live log |
| 5–20 min | Walk the timeline; surface disagreements on what actually happened |
| 20–35 min | Root cause and contributing factors; Socratic method — ask "why" five times |
| 35–45 min | Action items; each must have an owner, a ticket, and a due date |

Required attendees: primary on-call, team lead for the implicated service,
SRE lead. Optional: secondary on-call, security-team lead (for SEV-1
events involving security).

---

## Template

Copy from the `---BEGIN TEMPLATE---` marker below into the incident file.

---BEGIN TEMPLATE---

```markdown
# INC-YYYY-MMDD-NN — Retrospective

**Incident date:** YYYY-MM-DD
**Severity:** SEV-1 | SEV-2 | SEV-3
**Duration:** HH:MM (first-signal to confirmed-recovery)
**Retrospective date:** YYYY-MM-DD
**Facilitator:** <name>
**Attendees:** <comma-separated names>
**Implicated service(s):** declaration | verification-engine | infra | other

## Executive summary

Two to four sentences. Plain language. A stakeholder without deep
technical knowledge of the codebase should read this and understand:
(a) what happened, (b) who was affected, (c) what was done, and
(d) what will be different next time.

## Impact

- **Users affected:** <count or "none confirmed" + explanation>
- **Requests affected:** <count from telemetry or "unknown">
- **Data integrity:** none | possibly-affected | confirmed-affected
  (If "possibly-affected" or "confirmed-affected": a follow-up audit
  of the affected rows/events is required before this incident is closed.)
- **External communications sent:** yes/no
  (If yes: link the communication and record who authorised it.)
- **Regulatory reporting:** yes/no
  (If yes: ANIF / BEAC / supervisory authority, owner, status, deadline.)

## Timeline (UTC, 24h)

Start from the first observable signal, not from the page.

| Time (UTC) | Event |
|---|---|
| HH:MM | First observable signal (earliest retrospective indicator) |
| HH:MM | Detection (on-call became aware) |
| HH:MM | Acknowledgement (on-call accepted the page) |
| HH:MM | First mitigation action taken |
| HH:MM | Mitigation deployed (bleeding stopped) |
| HH:MM | Root cause identified |
| HH:MM | Full recovery confirmed in telemetry |
| HH:MM | Incident declared resolved |

## Root cause

One paragraph. The root cause is the **single proximate technical
explanation** for why the failure happened — not the list of things
that made it worse (those are contributing factors). "The root cause
was X" where X is a specific, falsifiable, technical claim.

If the root cause is genuinely unknown after investigation, state that
explicitly. An unknown root cause is itself an action item: either
investigate further or explicitly accept the risk in an ADR.

## Contributing factors

The conditions that made the root cause possible, made it harder to
detect, or made it harder to mitigate. Each factor is a system property
(alert threshold too high, test coverage absent, runbook step ambiguous)
not a person's fault.

- **CF-1:** <factor> — <why it mattered>
- **CF-2:** <factor> — <why it mattered>
- (Add as many as are genuinely true; do not pad.)

## What went well

Two to five bullets. Be specific. "The monitoring was useful" is not
specific; "the `RecorOutboxDLQDepthSpiking` alert fired within 90
seconds of the first dead-letter and gave us the exact event_id" is.

- **WW-1:** <specific thing that worked>
- **WW-2:** <specific thing that worked>

## What did not

Two to five bullets. Same specificity rule. Blameless framing: "the
alert threshold was calibrated for p50 traffic, not burst traffic" rather
than "X didn't notice."

- **WN-1:** <specific thing that failed or was absent>
- **WN-2:** <specific thing that failed or was absent>

## Action items

Each action item must have: title, owner, ticket number (opened at or
before the retrospective meeting), due date, and the doctrine or
quality dimension it addresses.

| # | Title | Owner | Ticket | Due | Category |
|---|---|---|---|---|---|
| 1 | <title> | @<handle> | #<num> | YYYY-MM-DD | Prevent recurrence |
| 2 | <title> | @<handle> | #<num> | YYYY-MM-DD | Reduce time-to-detect |
| 3 | <title> | @<handle> | #<num> | YYYY-MM-DD | Reduce time-to-mitigate |

Categories:
- **Prevent recurrence** — make this failure mode impossible or rare
- **Reduce time-to-detect** — earlier signal on the next instance
- **Reduce time-to-mitigate** — shorter path from detection to fix

A retrospective with zero action items requires explicit justification.
Either the incident was a non-event that should have been a near-miss
note, or the review is not looking hard enough.

## Doctrine review

Check every doctrine (Architecture V1 P2) that this incident implicates.
Each checked box mandates at least one action item.

- [ ] **D01 (completeness)** — did partial delivery of a feature leave
      a gap that contributed?
- [ ] **D04 (tests)** — would a test we could have written have caught
      this before it reached production?
- [ ] **D08 (no dangling threads)** — was there a known TODO, deferred
      ticket, or documented gap that this incident exploited?
- [ ] **D13 (idempotency)** — was a non-idempotent operation involved?
      Did a retry cause a duplicate side-effect?
- [ ] **D14 (fail-closed)** — did a boundary fail-open when it should
      have refused the request or defaulted to the safe state?
- [ ] **D15 (cryptographic provenance)** — was a receipt, signature, or
      audit anchor not produced, not checked, or not readable?
- [ ] **D16 (observability)** — was the root cause invisible until after
      the fact? Was a metric, trace, or log line absent or too noisy?
- [ ] **D17 (zero trust)** — was a trust boundary improperly extended?
      Did one service accept another's claims without verifying them?
- [ ] **D18 (no secrets)** — did a secret, credential, or private key
      appear in a log, a ticket, a commit message, or a chat channel?

## Communication log

- T+HH:MM — first internal post in `#oncall-recor`
- T+HH:MM — breach/regulatory notification (if applicable; link)
- T+HH:MM — resolution notice in `#oncall-recor`
- T+HH:MM — retrospective meeting held
- T+HH:MM — retrospective PR opened (#<num>)

## Attachments

Commit supporting files to `docs/incidents/INC-YYYY-MMDD-NN/`:

- `grafana-*.pdf` — Grafana dashboard snapshots at incident time
- `log-excerpt-*.txt` — Relevant log excerpt (redacted per OPS-2;
  no PII in committed files)
- `incident-channel.pdf` — Channel transcript export
- `scripts/` — Any one-off mitigation scripts

## Open questions

Items that are unresolved at retrospective time and need follow-up
before this incident record can be marked closed:

- [ ] <question 1>
- [ ] <question 2>
```

---END TEMPLATE---

## Related runbooks

- `docs/runbooks/incident-response-template.md` — the live-log template
  kept during the incident; evidence base for this retrospective
- `docs/runbooks/oncall-triage-tree.md` — escalation entrypoint
- `docs/runbooks/rollback-deployment.md` — rollback procedure referenced
  in mitigation steps
- `docs/runbooks/breach-notification.md` — GDPR Art. 33 72-hour procedure
  triggered when the incident involves PII exposure
- `docs/runbooks/dlq-inundation.md` — DLQ-specific triage
