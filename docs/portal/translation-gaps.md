# Declarant portal — translation gap register

This file is the queue for the localisation team. Engineering ships
keys; this list records which keys are *not yet* authoritatively
translated and need a Cameroonian sign-off before the portal goes
public.

Workflow context: see
`applications/declarant-portal/CLAUDE.md` § Translation review
workflow. The three locales the portal supports are:

- `fr` — French, **legal primary**. Source of truth for every other
  locale; a Cameroonian beneficial-ownership lawyer signs off.
- `en` — English, secondary. Bilingual project liaison reviews.
- `pidgin` — Cameroonian Pidgin / Kamtok, tertiary stub. Community
  linguist contracted via the declarant-experience team.

Engineering MUST NOT silently replace a `[FR-TRANSLATION-NEEDED]`
marker — re-trigger the review workflow first.

## How findings are tagged in source

A value carrying a `[FR-TRANSLATION-NEEDED]` prefix is an English
placeholder waiting for the proper locale translation. The portal
prefers showing the marker over silently rendering a key name (D14
fail-closed); declarants who land on a marker should never reach the
production-public path before the marker is replaced.

```bash
# Find every gap from the repo root:
grep -rn '\[FR-TRANSLATION-NEEDED\]' applications/declarant-portal/src/locales/
```

## TODO-037 — Multi-language parity for legal text

The 2026-Q2 audit added a new `legal.*` namespace covering the five
legal blocks called out in `docs/PRODUCTION-TODO.md`:

1. Declarant consent (`legal.consent`)
2. Cryptographic attestation (`legal.attestation`)
3. Sanctions notification (`legal.sanctions`)
4. Public-feedback CAPTCHA disclaimer
   (`legal.publicFeedbackCaptcha`)
5. ANIF/FIU privacy notice (`legal.fiuPrivacy`)

The keys exist in all three locale files; the table below records
the per-locale review state.

| Key | `fr` | `en` | `pidgin` |
|---|---|---|---|
| `legal.consent.heading` | pending legal review | translated | `[FR-TRANSLATION-NEEDED]` |
| `legal.consent.body` | pending legal review | translated | `[FR-TRANSLATION-NEEDED]` |
| `legal.attestation.heading` | pending legal review | translated | `[FR-TRANSLATION-NEEDED]` |
| `legal.attestation.body` | pending legal review | translated | `[FR-TRANSLATION-NEEDED]` |
| `legal.sanctions.heading` | pending legal review | translated | `[FR-TRANSLATION-NEEDED]` |
| `legal.sanctions.body` | pending legal review | translated | `[FR-TRANSLATION-NEEDED]` |
| `legal.publicFeedbackCaptcha.heading` | pending legal review | translated | `[FR-TRANSLATION-NEEDED]` |
| `legal.publicFeedbackCaptcha.body` | pending legal review | translated | `[FR-TRANSLATION-NEEDED]` |
| `legal.fiuPrivacy.heading` | pending legal review | translated | `[FR-TRANSLATION-NEEDED]` |
| `legal.fiuPrivacy.body` | pending legal review | translated | `[FR-TRANSLATION-NEEDED]` |

States:

- **translated** — engineering's best effort; awaiting reviewer pass.
- **pending legal review** — engineering's draft; legal sign-off
  required before public launch (the `fr` locale is the legal primary).
- **[FR-TRANSLATION-NEEDED]** — placeholder English text; community
  translation outstanding.

## Other a11y / i18n nits surfaced

| Key | State | Note |
|---|---|---|
| `a11y.skipToMain` | translated in FR + EN; English placeholder in Pidgin | Low priority. The skip-link is a navigation affordance, not a legal commitment; English fallback is acceptable until the community pass. |

## Discharging a gap

1. Replace the placeholder value in
   `applications/declarant-portal/src/locales/{locale}.json`.
2. Remove the `[FR-TRANSLATION-NEEDED]` prefix.
3. Remove the corresponding row from this table.
4. PR review by the appropriate reviewer per the workflow:
   - `fr`: Cameroonian beneficial-ownership lawyer.
   - `en`: bilingual project liaison.
   - `pidgin`: community linguist.
5. Record the sign-off in `docs/decisions/` as documented in the
   portal CLAUDE.md (ADR or PR review comment per the locale row).

## Cross-references

- `applications/declarant-portal/CLAUDE.md` — Translation review
  workflow + Pidgin stub policy
- `applications/declarant-portal/tests/e2e/i18n-parity.spec.ts` —
  Playwright assertion that locale switching renders structurally
  identical legal pages (TODO-037 verification)
- `docs/PRODUCTION-TODO.md` § TODO-037
