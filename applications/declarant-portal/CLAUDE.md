# Application: recor-declarant-portal
# Layer: 6 (Architecture V4 P17 § Declarant Portal)
# Owner: @recor/frontend-team @recor/declarant-experience

## What this application does

Web UI for declarants. Generates an Ed25519 keypair in the browser,
constructs the canonical form of a declaration, signs it, posts to the
Declaration service, displays the cryptographic receipt.

## Language and toolchain

- TypeScript 5.7 strict (no `any`, no implicit anything)
- React 19 (functional components only; React Compiler-friendly)
- Vite 6 build, Tailwind v4
- pnpm 9.12.3 (matches V3 P12 toolchain pin)
- Build via Docker: node:22-bookworm → nginx:1.27-alpine

## Architecture

- State: TanStack Query for server state; react-hook-form for form state
- Validation: Zod schemas (shared between client validation and the form resolver)
- Crypto: Web Crypto API native, no third-party crypto library
- Routing: single-route v1; the declaration view is a 4-step wizard
  (see "Wizard structure (R-PORT-3)" below)

## Wizard structure (R-PORT-3)

The declaration view is a 4-step linear wizard (`src/features/declaration/wizard/`).
The single-page form was retired in this ticket — `DeclarationForm.tsx` is now
a thin pass-through that mounts `<DeclarationWizard>`.

### Files

| File | Responsibility |
|---|---|
| `wizard/index.tsx` | Shell. Holds the single `useForm<FormValues>` instance, the Ed25519 keypair, the wizard-stable `declaration_id` + `nonce_hex`, the step index, and the Forward/Back gate. |
| `wizard/types.ts` | `WizardStep` type, `STEP_FIELDS` mapping (which fields each step owns for `form.trigger()`), `FIRST_STEP`, `LAST_STEP`. |
| `wizard/WizardStepper.tsx` | Horizontal progress indicator (1/4 Entity → 2/4 Owners → 3/4 Review → 4/4 Sign). Renders as `<nav><ol>` with `aria-current="step"` on the active step. |
| `wizard/EntityStep.tsx` | Step 1. Inputs: `entity_id`, `declarant_principal`, `declarant_role`, `kind`, `effective_from`. |
| `wizard/OwnersStep.tsx` | Step 2. `useFieldArray` over `beneficial_owners`; add/remove rows. |
| `wizard/ReviewStep.tsx` | Step 3. Read-only summary plus the live canonical-payload-bytes preview (hex prefix + total byte length). The preview uses the same `canonicalPayloadBytes` the server-side canonicaliser is mirrored to (D15). |
| `wizard/SignStep.tsx` | Step 4. Public-key confirmation block and the Sign-and-Submit CTA. The actual signing happens in the shell so the mutation cache lives in one place. |
| `wizard/field.tsx` | Shared `Field`, `inputCls`, and `tValidationMessage` helpers — extracted so every step renders identical input styling and inline `role="alert"` error placement. |

### Gating contract

- **Forward** invokes `form.trigger(STEP_FIELDS[step])` against the current step's
  fields and refuses to advance unless they all pass (D14 fail-closed). Step 3 and
  step 4 own no inputs, so the trigger short-circuits to "valid" — the gate already
  fired on steps 1 + 2.
- **Back** is always enabled from step 2 onward; the first step's Back button is
  disabled rather than hidden so the navigation pattern stays visually consistent.
- **State** lives on a SINGLE `useForm()` instance shared across steps. Typed values
  survive forward + back navigation. Do NOT spawn one form per step.

### D15 cryptographic provenance — the parity invariant

The wizard mints a stable `declaration_id` on first render and a stable `nonce_hex`
on first entry to step 3. Step 3 renders the canonical bytes those values produce
(via `canonicalPayloadBytes`); step 4 signs over those EXACT same bytes by passing
the same `declaration_id` and `nonce_hex` into `signPayload`. The byte-parity
unit test in `src/lib/crypto.test.ts` remains the load-bearing guard between
client and server canonicalisation — the wizard does NOT introduce a parallel
canonicaliser.

### i18n keys

All step labels, descriptions, navigation buttons, and the cryptographic-preview
copy go through `t('wizard.…')`. The keys live under the `wizard.` namespace in
each of `src/locales/{fr,en,pidgin}.json`. The Pidgin file carries English
placeholders behind a `_translation_status: "stub"` marker, matching the rest
of the file — community translation is the same workflow R-PORT-1 documented.

## Playwright E2E (R-PORT-6)

The portal ships a four-scenario Playwright suite at
`applications/declarant-portal/tests/e2e/`. The suite is the
production-acceptance gate for the four critical user paths called
out in `docs/PRODUCTION-TODO.md` § R-PORT-6:

| Spec file                       | Scenario                                                  |
|---------------------------------|-----------------------------------------------------------|
| `happy-path.spec.ts`            | Wizard → submit → verification polls to `accepted`        |
| `validation.spec.ts`            | Invalid entity_id; step-1 Forward gate refuses to advance |
| `verification-rejected.spec.ts` | Unseeded person yields a `rejected` / red-lane status     |
| `polling-stops.spec.ts`         | No `GET /v1/declarations/{id}` fires after terminal state |

### Run modes (`E2E_MODE`)

- **`mocked`** (default; local dev + the load-bearing CI gate) —
  Playwright `page.route()` intercepts every Declaration-service call
  and replies with deterministic fixtures from `tests/e2e/fixtures.ts`.
  No D↔V compose stack required. Boots `pnpm preview` on :5173 via
  Playwright's `webServer` block; reuses an existing preview server
  in dev (`reuseExistingServer: !process.env.CI`).
- **`live`** — talks to the real D↔V loop spun up by the CI workflow
  (`services/declaration/docker-compose.integration.yaml`) and the
  portal nginx (`docker-compose.yaml` here). Mock BUNEC is seeded
  with `SEEDED_PERSON_ID = 018f0000-0000-4000-8000-0000000000a1`
  before the suite runs; `UNSEEDED_PERSON_ID =
  018f0000-0000-4000-8000-0000000000ff` is deliberately absent so
  the rejected spec lands on red. `E2E_BASE_URL` flips to
  `http://localhost:8082` (the portal nginx). Locale is locked to
  French (legal primary; R-PORT-1) via `addInitScript`-set
  `localStorage['recor.locale']='fr'` so assertions never break on
  a developer's OS locale.

### CI workflow

`.github/workflows/portal-e2e.yaml`. Two jobs —
`portal-e2e / mocked` and `portal-e2e / live` — both running with
`fail-fast: true` and Playwright `retries: 0` (D14 fail-closed). The
Playwright browsers are cached across runs keyed on the
`@playwright/test` version. On failure the HTML report is uploaded
as a workflow artefact (`playwright-report-{mocked,live}`).

Both jobs are NOT YET in the required-checks set (per the OBS-2
deferral pattern: 10-consecutive-green-runs gate before promotion).
Status tracked in `docs/security/branch-protection.md` § Deferred
promotions.

### Local invocation

```bash
cd applications/declarant-portal
pnpm install --frozen-lockfile
pnpm exec playwright install chromium
pnpm build
pnpm exec playwright test --reporter=list
```

The wizard's stable test hooks (`data-testid="wizard-step-{n}"`,
`data-testid="wizard-forward"`, `data-testid="wizard-sign-submit"`)
are the load-bearing selectors. Translated text is asserted only for
the verification heading (`Vérification acceptée` / `Vérification
rejetée`) — the rest of the suite is locale-resilient.

### Determinism

- Three consecutive local runs produce identical pass/fail (D19).
- `Playwright retries: 0` in `playwright.config.ts` — flake hides
  bugs, and a flaky suite is its own bug.
- Mocked-mode trajectories are canned in `fixtures.ts` and the
  receipt hash is a fixed BLAKE3 value so assertions are byte-stable.
- Live-mode determinism relies on mock-BUNEC seeding being committed
  in CI before the suite runs; the seed UUIDs are documented as
  constants in `fixtures.ts` so a regenerator could rebuild the
  database from the test source alone.

## SLOs

| Metric | Budget |
|---|---|
| FCP (low-end Android 3G) | < 1.5s |
| LCP (low-end) | < 2.5s |
| TTI | < 3.5s |
| Bundle (gzipped) | < 250 KB total; v1 ships at ~95 KB across 4 chunks |
| API submit p99 (declaration service round-trip) | < 1.5s |

## Doctrines that apply with special weight here

- **D01 completeness** — every form path produces either a valid signed submission OR a clear error message; no silent failures
- **D04 tests** — every cryptographic primitive has unit tests asserting byte-exact output
- **D14 fail-closed** — unsupported browsers see an error block, not a degraded form
- **D15 cryptographic provenance** — the receipt the declarant sees is a real BLAKE3 hash; print it and verify years later
- **D17 zero trust** — declarant principal is sourced from auth (dev: header; prod: OIDC); never trusted from form input
- **D18 no secrets** — Ed25519 private key stays in browser memory only; never sent over the wire

## The canonical-form parity rule

`src/lib/crypto.ts:canonicalPayloadBytes` MUST produce bytes
byte-identical to what `services/declaration/src/api/rest.rs:
canonical_payload_bytes` produces.

Field order is fixed. JSON serialisation is fixed (no whitespace).
Date format is fixed (ISO 8601 YYYY-MM-DD). Numeric basis points are
fixed (integer, not float, not string). The unit tests in
`crypto.test.ts` lock the expected byte sequence; any change to the
canonical form is a coordinated breaking change requiring both
client and server to update simultaneously.

## Security headers (OPS-3)

The portal nginx emits a production-grade security header set on every
response. Source of truth: `nginx.conf.template` plus
`security-headers.conf.template`, rendered at container startup by
`docker-entrypoint.sh` via `envsubst`. Headers are reasserted in every
`location` block (nginx `add_header` does not inherit when a child
block adds its own — see the comment in `nginx.conf.template`).

| Header | Value | Rationale |
|---|---|---|
| `Content-Security-Policy` | see below | Closes the XSS-and-friends class top to bottom; templated `connect-src` lets us pin the API origin without rebuilding the image |
| `Strict-Transport-Security` | `max-age=63072000; includeSubDomains; preload` | 2-year HSTS so a single year-long cert rotation never lapses the pin; `preload`-ready for hstspreload.org submission |
| `X-Content-Type-Options` | `nosniff` | Refuses MIME sniffing; closes "polyglot upload served as JS" |
| `X-Frame-Options` | `DENY` | Legacy-browser clickjacking defense (paired with CSP `frame-ancestors 'none'`) |
| `Referrer-Policy` | `strict-origin-when-cross-origin` | Prevents leaking declaration IDs in the Referer when the user navigates off the receipt screen |
| `Permissions-Policy` | `geolocation=(), camera=(), microphone=(), payment=(), usb=(), magnetometer=(), gyroscope=(), accelerometer=(), autoplay=(), fullscreen=(), picture-in-picture=()` | Disables every browser feature the portal does not use, so a future bundle bug or compromised dependency can't silently grow privilege |
| `Cross-Origin-Opener-Policy` | `same-origin` | Isolates the browsing context against window.opener attacks + Spectre-class side-channels |
| `Cross-Origin-Resource-Policy` | `same-origin` | Refuses cross-origin sites embedding our resources |
| `Server` | `nginx` (no version) | `server_tokens off` keeps the response surface low-entropy |

### CSP, directive by directive

The policy literal:

```
default-src 'self';
script-src 'self';
style-src 'self' 'unsafe-inline';
connect-src 'self' ${CSP_CONNECT_SRC};
img-src 'self' data:;
font-src 'self';
frame-ancestors 'none';
base-uri 'self';
form-action 'self';
object-src 'none'
```

- **`default-src 'self'`** — fallback for any directive not listed. Same-origin everything.
- **`script-src 'self'`** — same-origin scripts only. No `unsafe-inline`, no `unsafe-eval`. Vite's prod bundle emits external JS only; if a future feature needs a third-party script (e.g. an analytics tag) it MUST go through this CLAUDE.md as an explicit, reviewed widening.
- **`style-src 'self' 'unsafe-inline'`** — Tailwind v4 + React 19 still emit a small set of inline `<style>` tags (critical CSS + dynamic style props). `'unsafe-inline'` here is a known, accepted exposure. To remove it we would need to migrate every inline style to a hashed style or a nonce, which costs more than it saves at this stage.
- **`connect-src 'self' ${CSP_CONNECT_SRC}`** — `'self'` covers `/healthz` and same-origin XHR; `${CSP_CONNECT_SRC}` is templated at container startup with the declaration service origin (e.g. `https://api.recor.cm` in prod, `http://127.0.0.1:8080` in dev compose). To add another XHR target (e.g. an OIDC IdP for discovery), extend the value at the orchestrator layer — NEVER edit the template inline. Whitespace-separated multiple origins are supported (`envsubst` does not split the value).
- **`img-src 'self' data:`** — `data:` lets the bundle inline small SVG/PNG icons without a round-trip. No remote images.
- **`font-src 'self'`** — fonts are bundled in the build output; no Google Fonts.
- **`frame-ancestors 'none'`** — refuses iframe embedding outright. Paired with `X-Frame-Options: DENY` for browsers that don't honour CSP frame-ancestors (Safari < 16 in particular).
- **`base-uri 'self'`** — refuses `<base>` tag attacks that could rewrite the resolution of relative URLs.
- **`form-action 'self'`** — refuses native form submissions to other origins (the declaration form posts via `fetch`, not native submit, so this is belt-and-braces).
- **`object-src 'none'`** — no `<object>` / `<embed>` / `<applet>`.

### What's deliberately NOT set

- **`Cross-Origin-Embedder-Policy: require-corp`** is omitted. It would force every same-origin resource to declare CORP and would block any third-party assets (favicon CDN, analytics) without a coordinated rollout. Re-enable when a documented audit confirms every loaded resource is CORP-compatible.
- **CSP `report-uri`** / **`Reporting-Endpoints`** are omitted in v1. Add when we have a CSP-report collector (CSP-Reporter or similar); without one, violations are silently dropped.
- **CSP `nonce-…`** for scripts is omitted because Vite's prod bundle does not emit inline scripts — `'self'` is strict enough. If we add SSR or templating that needs inline scripts, switch to a per-request nonce strategy (NOT `'unsafe-inline'`).

### CSP_CONNECT_SRC contract

- Set via container env. Read by `docker-entrypoint.sh` at startup.
- Whitespace-separated list of origins (e.g. `https://api.recor.cm` or `https://api.recor.cm https://idp.recor.cm`).
- Validated for `; ' "` and newline characters before substitution; the container fails to start if any are present (defense against env-injection that would broaden the policy).
- Unset / empty: the directive collapses to `connect-src 'self'`. The portal will then fail to talk to any external API — intentional fail-closed behaviour.

### How to verify

1. Build the image and start it with a test origin:
   ```
   docker build -t recor/declarant-portal:test .
   docker run --rm -p 18082:8082 -e CSP_CONNECT_SRC=https://api.test.recor.cm recor/declarant-portal:test
   ```
2. `curl -I http://127.0.0.1:18082/` — every header above should be present.
3. Load the SPA in a browser; the dev console must not show any CSP violations (a handful of inline-style notices from Tailwind are expected and explicitly allowed).
4. Or just run `scripts/headers-smoke.sh`, which automates all of the above and asserts every header is present + the templated origin is in CSP.

## API client generation (R-PORT-7)

The portal's wire-shape types are generated from the committed
OpenAPI spec, not hand-written.

- **Spec source of truth:** `docs/openapi/declaration.json`,
  regenerated from the service by `tools/ci/check-openapi-drift.sh`
  (DOC-1).
- **Generated client:** `src/generated/openapi.ts`. The file is
  committed so consumers do not need network access at build time and
  so review diffs surface contract changes alongside the code that
  uses them.
- **Regenerate locally:** `pnpm openapi:gen` (writes the file in
  place).
- **Drift check:** `pnpm openapi:check` or `UPDATE=1
  tools/ci/check-portal-openapi-client-drift.sh` to regenerate then
  diff. CI runs the drift check on every PR as job
  `portal / openapi-client-drift` and fail-closes when the committed
  client lags behind the spec (D14). There is no `--allow-drift`
  escape; the fix is always "regenerate, commit".
- **Where the types are consumed:** `src/lib/api.ts` re-exports the
  generated `components['schemas']` namespace under stable names
  (`SubmitDeclarationResponse`, `VerificationLane`, etc.). The
  runtime Zod schemas remain the trust boundary (D17) — wire shape
  is never trusted blindly — but their structural shape is pinned to
  the generated types via `satisfies` and compile-time
  `extends`-sentinels, so spec drift surfaces at `pnpm typecheck`
  and is caught before merge.
- **Canonical-form parity (D15):** the generated types do NOT
  replace `src/lib/crypto.ts:canonicalPayloadBytes`. Canonical-form
  bytes are constructed from the declarant's typed inputs in a fixed
  field order matching the Rust server; if a future spec change
  reorders or renames a field, the adapter at the form boundary (the
  `buildSignedRequest` argument shape) normalises it BEFORE signing.
  The byte-parity unit test in `crypto.test.ts` is the load-bearing
  guard.

## Internationalisation (R-PORT-1)

The portal is trilingual: **French (legal primary)**, **English
(secondary)**, **Cameroonian Pidgin / Kamtok (tertiary, stub)**.

### Stack

- `i18next` + `react-i18next` for the runtime
- `i18next-browser-languagedetector` for first-boot detection
  (order: `localStorage` → `navigator.language`)
- `i18next-resources-to-backend` for per-locale dynamic `import()`,
  so each locale ships as its own hashed JS chunk
  (`dist/assets/locale-{fr,en,pidgin}-<hash>.js`)
- Central config: `src/i18n.ts`
- Locale JSON: `src/locales/{fr,en,pidgin}.json`
- Selector: `App.tsx`'s header `<select data-testid="locale-selector">`
- Persistence key: `recor.locale` in `localStorage`

### D14 fail-closed behaviour

- `fallbackLng: 'fr'` — the legal primary catches every missing key,
  so a half-translated en.json or stub pidgin.json never renders a
  raw key in production
- Anything outside `['fr', 'en', 'pidgin']` collapses to `fr`
- Zod validation messages are i18n keys (`'validation.uuid'`, …); the
  form component resolves them via `t()` and defensively falls back
  to the raw message if a key is somehow absent

### Bundle-size invariant

- Each locale chunk is < 30 KB gzipped (measured at R-PORT-1: ~2.2–2.6 KB)
- The active-locale chunk is loaded on first paint; switching the
  locale triggers a separate dynamic import that the browser caches
  for the session
- Re-measure when adding a fourth locale or growing the key tree
  beyond ~150 keys per locale

### Translation review workflow

The codebase ships translations; **only Cameroonian humans sign them
off as authoritative**. Engineering never alters fr.json without
re-triggering the legal review.

| Locale | Reviewer | Cadence | Sign-off artefact |
|---|---|---|---|
| `fr.json` (legal primary) | Cameroonian beneficial-ownership lawyer (engagement via project legal lead, @recor/legal) | Before every public-facing release; after any new `validation.*` or `verification.*` key | ADR in `docs/decisions/` capturing the reviewing lawyer + commit SHA they signed off |
| `en.json` (secondary) | Bilingual project liaison (declarant-experience team) | Before every public-facing release | PR review comment from the liaison's GitHub handle |
| `pidgin.json` (tertiary, stub) | Community linguist contracted via the declarant-experience team | One-shot full translation, then ad-hoc updates | ADR + linguist's attestation file under `docs/decisions/` |

### Pidgin stub policy

Until the community translation lands, `pidgin.json` carries English
placeholders so the UI never regresses to raw key names if a
declarant selects Pidgin. The leading `_translation_status` key
documents the stub state and is the search-marker reviewers use to
locate the in-progress file. When the linguist returns translations,
replace every value, change `_translation_status.completeness` from
`"stub"` to `"complete-pending-legal-review"`, and remove the
`todo` field.

### What MUST flow through `t()`

- Every visible string in `App.tsx`, `DeclarationForm.tsx`,
  `VerificationStatus.tsx`, and any future component
- Every Zod validation message (stored as keys under
  `validation.*`)
- Every `aria-label` and `sr-only` text

### What MUST NOT flow through `t()`

- Protocol tokens displayed for cross-locale citation
  (verification lane `green` / `yellow` / `red`, verification state
  `accepted` / `pending` / …). These are stable identifiers analysts
  and declarants use across languages and the `StatusBadge`
  deliberately renders them raw.
- Cryptographic primitives (declaration_id, receipt hash, public
  key, signature hex)

## When in doubt

1. Read this document
2. Architecture V4 P17 § Declarant Portal
3. Companion V4 P20 § frontend scaffolding
4. The Declaration service CLAUDE.md (where the server-side canonical
   form lives)
5. Ask the architect-reviewer agent
