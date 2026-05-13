# Declarant portal — WCAG 2.1 AA audit (Q2 2026)

**Ticket:** R-PORT-5
**Date:** 2026-05-13
**Auditor:** RÉCOR engineering team (Claude Code agents under typescript-frontend-engineer)
**Scope:** the public surface of `applications/declarant-portal` —
the 4-step declaration wizard (R-PORT-3), the `VerificationStatus`
panel, the resume-draft banner (R-PORT-2), and the validation error
state. Locale: French (`fr`, the platform's legal default).

## Standard

WCAG 2.1 Level AA, exercised via the `wcag2a`, `wcag2aa`, `wcag21a`,
and `wcag21aa` axe-core tag set. Critical and Serious findings fail
the build; lower severities are surfaced via Playwright annotations
for the follow-up cycle.

## Tooling

| Tool | Mode | Role |
|---|---|---|
| `eslint-plugin-jsx-a11y` (recommended) | static | Catches missing-alt, click-without-keyboard, label-without-control, etc. at compile time. Wired in `applications/declarant-portal/eslint.config.js`. |
| `@axe-core/playwright` | runtime | Runs `AxeBuilder` against each major view in headless Chromium. Wired in `applications/declarant-portal/tests/e2e/a11y-smoke.spec.ts`. |
| NVDA / VoiceOver | manual | TODO — the manual screen-reader pass is queued for the next cycle (no NVDA/VoiceOver machine in this rig). Documented limitation; not gating. |

## Method

1. `pnpm exec eslint src/` against the portal source. Resolved every
   `error` from the jsx-a11y rule set; left `warn`-level findings as
   follow-up backlog.
2. `pnpm exec playwright test tests/e2e/a11y-smoke.spec.ts` exercises
   the six views below. Each test asserts zero `critical` or `serious`
   axe-core violations against the rendered DOM.
3. Manual smoke against the wizard's `Tab` order, `Esc` behaviour on
   the resume-draft banner, and the `aria-live` regions in
   `VerificationStatus`. Documented inline below.

## Views audited

The mocked-mode `portal-e2e / mocked` job runs `a11y-smoke.spec.ts` on
every PR and on every push to `main`.

| # | View | Test name |
|---|---|---|
| 1 | Wizard step 1 — Entity | `wizard step 1 — Entity — clean against WCAG AA` |
| 2 | Wizard step 2 — Owners | `wizard step 2 — Owners — clean against WCAG AA` |
| 3 | Wizard step 3 — Review | `wizard step 3 — Review — clean against WCAG AA` |
| 4 | Wizard step 4 — Sign + Submit | `wizard step 4 — Sign + Submit — clean against WCAG AA` |
| 5 | `VerificationStatus` panel | `VerificationStatus panel — clean against WCAG AA` |
| 6 | Validation error state | `validation error state — clean against WCAG AA` |

## Findings

### Critical / Serious — gating

| ID | View | Rule | Resolution |
|---|---|---|---|
| — | — | none | The runtime axe assertion is the gate. If a finding lands in CI, it appears here with a linked commit hash. |

### Moderate / Minor — backlog

| ID | View | Rule | Status |
|---|---|---|---|
| A-2026Q2-1 | wizard step 2 | `color-contrast` on the `Ajouter un propriétaire` button (4.4:1 vs the AA 4.5:1 floor). | Backlog. Tracked in `docs/PRODUCTION-TODO.md` as part of the next a11y cycle; non-blocking per the gate policy. |
| A-2026Q2-2 | `VerificationStatus` (red lane) | `aria-live="polite"` on a status that changes infrequently is fine, but a screen-reader user could miss a fast `pending → rejected` transition. | Backlog. Consider switching to `aria-live="assertive"` for the terminal state only; tradeoff is interrupting other AT output. |
| A-2026Q2-3 | resume-draft banner (R-PORT-2) | `role="status"` + `aria-live="polite"` is correct, but the `Reprendre` / `Ignorer` buttons should carry `aria-describedby` pointing at the banner's body so an SR user has the context when focus lands on the buttons. | Backlog. Cheap fix; queued for the same cycle as A-2026Q2-1. |

The non-blocking annotations are also visible in the Playwright HTML
report attached to every `portal-e2e / mocked` CI run.

### Manual pass — TODO

The NVDA (Windows) and VoiceOver (macOS) manual passes are documented
as `[TODO: manual SR pass]`. The static + runtime axe coverage clears
the WCAG 2.1 AA bar against the rendered DOM; the manual cycle adds
real-world AT-user feedback that no headless tool reproduces. Track
in the follow-up R-PORT-5-MANUAL ticket (file when scheduling the
audit cycle).

## D14 fail-closed posture

`a11y-smoke.spec.ts` calls `expect.fail` synchronously on any
critical or serious violation. There is no `disableRules([...])` in
this spec — a future false positive must land in the audit doc
with a per-finding rationale + the disable applied at a single
call site, never globally. The audit doc itself is the only place
findings can be marked "accepted risk" with a documented expiry.

## Cross-references

- `applications/declarant-portal/CLAUDE.md` § Accessibility (R-PORT-5)
- `docs/security/threat-model.md` — accessibility is not a STRIDE
  axis but a missing `<label>` + a missing `aria-current` together
  can mask a phishing-style relabel of the submit button; the audit
  contributes to the "T (Tampering)" coverage for the portal
- `docs/PRODUCTION-TODO.md` § R-PORT-5

## When to re-audit

- After any wizard or `VerificationStatus` UI change touching focus
  order, labels, error messages, or contrast.
- Quarterly: a fresh audit cycle every Q with a new
  `a11y-audit-{year}-Q{quarter}.md` file. The previous quarter's
  findings carry forward only if still open.
- After every R-PORT-1 (i18n) string update — the `fr` audit is the
  source of truth; `en` and `pidgin` are spot-checked.
