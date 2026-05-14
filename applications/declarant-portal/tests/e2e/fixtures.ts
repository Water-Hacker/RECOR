/**
 * Shared fixtures and helpers for the R-PORT-6 Playwright suite.
 *
 * Two modes are supported:
 *
 *   - `mocked` (default): Playwright `page.route()` intercepts every
 *     call to the Declaration service so the suite is hermetic. Each
 *     scenario installs its own response chain via
 *     `installApiRoutes(...)`. Assertions are byte-stable across runs
 *     because the canned BLAKE3 receipt hash + declaration_id are
 *     fixed in the fixtures.
 *
 *   - `live`: the suite hits the real D↔V loop from
 *     `services/declaration/docker-compose.integration.yaml`. Mock
 *     BUNEC MUST be seeded with the `SEEDED_PERSON_ID` UUID below for
 *     the happy-path / polling specs; the `UNSEEDED_PERSON_ID` is
 *     intentionally absent so the verification engine produces a
 *     red-lane / rejected outcome.
 *
 * The seeded person UUID is committed in the integration compose stack
 * and re-seeded by `services/declaration/scripts/integration-smoke.sh`
 * (search: `INSERT INTO mock_bunec_persons`). Tests document the seed
 * dependency loudly so anyone re-running the suite knows what to
 * provision (D19 reproducible everything).
 */

// @ts-ignore
import type { Page, Route } from '@playwright/test';

// `process` is a Node global injected by Playwright's test runner; we
// reach for it without pulling in `@types/node` because the rest of the
// portal code is browser-only. `globalThis` plus a narrow cast is the
// minimum surface required.
const procEnv: Record<string, string | undefined> =
  (globalThis as unknown as { process?: { env: Record<string, string | undefined> } })
    .process?.env ?? {};

export const E2E_MODE: 'mocked' | 'live' =
  (procEnv.E2E_MODE as 'mocked' | 'live' | undefined) ?? 'mocked';

/* ─── deterministic seed UUIDs ─────────────────────────────────────── */

/**
 * Person UUID that mock BUNEC MUST contain for happy-path / accepted /
 * polling specs. Mirrors the seed used by the integration-smoke script
 * (services/declaration/scripts/integration-smoke.sh). The UUID is a
 * stable v4 chosen for the suite — keep it identical across spec
 * files so a shared seed call covers every test.
 */
export const SEEDED_PERSON_ID = '018f0000-0000-4000-8000-0000000000a1';

/**
 * Person UUID that mock BUNEC MUST NOT contain. The verification
 * engine's identity stage then fails-closed (BunecLookup::NotFound),
 * which propagates through fusion → lane router → `rejected` /
 * red-lane.
 */
export const UNSEEDED_PERSON_ID = '018f0000-0000-4000-8000-0000000000ff';

/** Entity UUID used by every spec; rerolling it would force CI seeds to keep up. */
export const TEST_ENTITY_ID = '018f0000-0000-4000-8000-0000000000e1';

/**
 * Declarant principal used by every spec. The portal sends it via the
 * `X-Recor-Dev-Principal` header (dev only; prod uses OIDC). The
 * principal is also persisted to the form's default value via the
 * react-hook-form `defaultValues` block in `DeclarationForm.tsx`.
 */
export const TEST_PRINCIPAL = 'spiffe://recor.cm/declarant-001';

/* ─── canned API responses (mocked mode) ──────────────────────────── */

const RECEIPT_HASH_HEX =
  '0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef';
const DECLARATION_ID_ACCEPTED = '018f0000-0000-4000-8000-000000000d01';
const CASE_ID = '018f0000-0000-4000-8000-000000000ca1';

export interface MockedRouteOptions {
  /**
   * The verification trajectory the mocked `GET /v1/declarations/{id}`
   * walks through. Each entry is one poll response; once the array is
   * exhausted the last entry is repeated indefinitely. Tests assert
   * that polling halts on terminal state by counting actual GETs after
   * the terminal entry, so don't pad the trajectory artificially.
   */
  trajectory: Array<{
    verification_state:
      | 'pending'
      | 'in_verification'
      | 'accepted'
      | 'rejected'
      | 'not_verified';
    verification_lane?: 'green' | 'yellow' | 'red';
  }>;
  /**
   * Override the declaration_id the POST returns. Defaults to the
   * accepted-flow id so happy-path specs do not need to specify it.
   */
  declarationId?: string;
}

export interface MockedRouteState {
  /** Number of GETs the test page has issued so far. */
  getCalls: number;
  /** Number of POSTs the test page has issued so far. */
  postCalls: number;
}

/**
 * Install Playwright `route()` handlers that mock the Declaration
 * service. Returns a `state` object the test can inspect to assert
 * polling stopped (D14 + the R-PORT-6 polling-stops-on-terminal-state
 * scenario).
 */
export async function installApiRoutes(
  page: Page,
  options: MockedRouteOptions,
): Promise<MockedRouteState> {
  const state: MockedRouteState = { getCalls: 0, postCalls: 0 };
  const declarationId = options.declarationId ?? DECLARATION_ID_ACCEPTED;

  // The portal targets http://localhost:8080 by default
  // (VITE_DECLARATION_API_URL fallback in src/App.tsx). The match is
  // exact-path so we don't accidentally intercept the static assets
  // the portal preview server serves itself.
  const declarationsCollection =
    'http://localhost:8080/v1/declarations';
  const declarationItem = `http://localhost:8080/v1/declarations/${declarationId}`;

  await page.route(declarationsCollection, async (route: Route) => {
    if (route.request().method() !== 'POST') {
      return route.fallback();
    }
    state.postCalls += 1;
    return route.fulfill({
      status: 201,
      contentType: 'application/json',
      body: JSON.stringify({
        declaration_id: declarationId,
        state: 'submitted',
        receipt_hash_hex: RECEIPT_HASH_HEX,
        submitted_at: '2026-05-12T10:00:00Z',
        receipt_url: `/v1/declarations/${declarationId}`,
      }),
    });
  });

  await page.route(declarationItem, async (route: Route) => {
    if (route.request().method() !== 'GET') {
      return route.fallback();
    }
    // Safe-by-construction: trajectories carry at least one entry,
    // and we clamp the index to the last element so polling that
    // outlives the trajectory keeps replying with the terminal frame.
    // `noUncheckedIndexedAccess` still wants the optional handled, so
    // we fall through to a neutral pending frame if a test author
    // mistakenly passes an empty trajectory — better than crashing.
    const index = Math.min(state.getCalls, options.trajectory.length - 1);
    const entry = options.trajectory[index] ?? {
      verification_state: 'pending' as const,
    };
    state.getCalls += 1;
    return route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        declaration_id: declarationId,
        entity_id: TEST_ENTITY_ID,
        declarant_principal: TEST_PRINCIPAL,
        state: 'submitted',
        aggregate_version: 1,
        submitted_at: '2026-05-12T10:00:00Z',
        receipt_hash_hex: RECEIPT_HASH_HEX,
        verification_state: entry.verification_state,
        verification_lane: entry.verification_lane,
        verification_case_id: CASE_ID,
        verified_at:
          entry.verification_state === 'accepted' ||
          entry.verification_state === 'rejected'
            ? '2026-05-12T10:00:05Z'
            : undefined,
      }),
    });
  });

  return state;
}

/* ─── locale lock (R-PORT-1) ──────────────────────────────────────── */

/**
 * Seed both localStorage keys before the SPA boots so first paint is
 * already in French. `recor.locale` is the load-bearing key (see
 * `src/i18n.ts` `LOCALE_STORAGE_KEY`); we set `i18nextLng` defensively
 * in case a future detector swap reaches for the i18next default.
 *
 * The seed runs as an `addInitScript` so it executes before any page
 * script, including i18next's language detector. Goto-then-seed is
 * wrong because the detector has already chosen by then.
 */
export async function lockLocaleToFrench(page: Page): Promise<void> {
  await page.addInitScript(() => {
    try {
      window.localStorage.setItem('recor.locale', 'fr');
      window.localStorage.setItem('i18nextLng', 'fr');
    } catch {
      // private mode / disabled storage — language detector will then
      // fall back to navigator.language; not our concern for E2E.
    }
  });
}

/* ─── wizard navigation helpers (R-PORT-3) ────────────────────────── */

/**
 * The portal renders a 4-step wizard (Entity → Owners → Review → Sign;
 * see `src/features/declaration/wizard/`). These helpers click the
 * "Suivant" / Forward button after each step's required fields are
 * filled, and finally click the step-4 "Signer et soumettre la
 * déclaration" submit button.
 *
 * The wizard exposes `data-testid="wizard-step-{n}"` per step section
 * and `data-testid="wizard-forward"` for the Forward affordance —
 * stable hooks that survive translation drift.
 */

async function clickForward(page: Page): Promise<void> {
  await page.getByTestId('wizard-forward').click();
}

export interface FillFormOptions {
  entityId?: string;
  personId?: string;
  /** Defaults to today (form's own default; matches the date-not-future check). */
  effectiveFrom?: string;
}

/**
 * Fill every required field in the 4-step wizard with deterministic,
 * schema-valid values, then advance from Step 1 (Entity) → Step 2
 * (Owners) → Step 3 (Review) → Step 4 (Sign).
 *
 * The wizard's own default basis-points (10_000) and one-owner array
 * satisfy the sum=10_000 and uniqueness invariants; we only need to
 * type the entity_id (step 1) and the person_id (step 2). The
 * declarant_principal, declarant_role, kind, effective_from defaults
 * are already valid.
 *
 * Returns when the wizard is sitting on Step 4 with the submit button
 * visible. Callers click submit via `clickSubmit`.
 */
export async function fillDeclarationForm(
  page: Page,
  opts: FillFormOptions = {},
): Promise<void> {
  const entityId = opts.entityId ?? TEST_ENTITY_ID;
  const personId = opts.personId ?? SEEDED_PERSON_ID;

  // Step 1: Entity. Wait for the section to render before touching
  // fields so flake from a slow keypair-generation effect cannot
  // race the fill (the wizard renders a loading state until the
  // Ed25519 keypair is ready — see `index.tsx` early-exit branch).
  await page.getByTestId('wizard-step-1').waitFor({ state: 'visible' });
  await page
    .getByLabel(/Identifiant de l'entité \(UUIDv4\)/)
    .fill(entityId);
  if (opts.effectiveFrom) {
    await page.getByLabel(/Date d'effet/).fill(opts.effectiveFrom);
  }
  await clickForward(page);

  // Step 2: Owners. Default array has one owner with
  // basis_points=10_000; we only need to set person_id.
  await page.getByTestId('wizard-step-2').waitFor({ state: 'visible' });
  await page
    .getByLabel(/Identifiant de la personne \(UUIDv4\)/)
    .first()
    .fill(personId);
  await clickForward(page);

  // Step 3: Review. No inputs; just advance.
  await page.getByTestId('wizard-step-3').waitFor({ state: 'visible' });
  await clickForward(page);

  // Step 4: Sign. The submit button now renders; caller invokes it.
  await page.getByTestId('wizard-step-4').waitFor({ state: 'visible' });
}

/**
 * Click the step-4 submit button (cognitive Step 4: Sign + Submit).
 * The button is identified by its `wizard-sign-submit` test-id; we
 * deliberately do NOT rely on the translated button text because a
 * translation review could rephrase it.
 */
export async function clickSubmit(page: Page): Promise<void> {
  await page.getByTestId('wizard-sign-submit').click();
}
