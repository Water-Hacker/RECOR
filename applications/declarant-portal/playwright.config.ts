/**
 * Playwright configuration for the declarant portal (R-PORT-6).
 *
 * The suite exercises the four production-acceptance gates described in
 * `docs/PRODUCTION-TODO.md` § R-PORT-6:
 *
 *   1. Happy path — fill the form, sign, submit, see the receipt and
 *      watch verification status poll to `accepted`.
 *   2. Validation — a non-UUID `entity_id` surfaces an error and the
 *      submit button does not advance to the receipt screen.
 *   3. Verification rejected — submission for a person NOT seeded in
 *      mock BUNEC lands on the `rejected` red-lane status.
 *   4. Polling stops on terminal state — after `accepted` / `rejected`
 *      lands, no further `GET /v1/declarations/{id}` calls fire.
 *
 * Run modes (D19 reproducible everything):
 *
 *   - `mocked` (default) — Playwright `page.route()` intercepts API
 *     calls and replies with deterministic fixtures. Lets the suite run
 *     on a developer laptop against `pnpm preview` alone, with no D↔V
 *     compose stack required. The receipt is a fixed BLAKE3 hash so
 *     assertions are byte-stable across runs.
 *   - `live` — talks to the real Declaration + Verification services in
 *     `services/declaration/docker-compose.integration.yaml`. Use this
 *     in CI to prove the contract end-to-end. Selected by exporting
 *     `E2E_MODE=live`; baseURL flips to `http://localhost:8082` (the
 *     portal nginx in the compose stack). Mock BUNEC must be seeded
 *     with the person UUIDs the live specs depend on — see
 *     `tests/e2e/fixtures.ts`.
 *
 * Locale lock (D04 + R-PORT-1):
 *   Every test sets `localStorage['recor.locale']='fr'` in a
 *   `beforeEach` so assertion text never breaks when a developer
 *   switches the OS locale. French is the legal primary (V4 P17 §
 *   Declarant Portal i18n).
 */

import { defineConfig, devices } from '@playwright/test';

/**
 * Mode selection. Default to mocked because that's the path a fresh
 * `pnpm install && pnpm exec playwright test` should succeed on without
 * spinning up Docker. CI sets `E2E_MODE=live` and supplies the compose
 * stack.
 */
const E2E_MODE = (process.env.E2E_MODE ?? 'mocked') as 'mocked' | 'live';

/**
 * The baseURL the test process navigates to. In mocked mode this is the
 * `pnpm preview` server on :5173 (vite.config.ts pins the preview port
 * with `strictPort: true`, so 5173 is the only valid value here). In
 * live mode this is the portal nginx exposed by the integration compose
 * stack (docker-compose.yaml at the portal root + the D↔V stack).
 */
const BASE_URL =
  E2E_MODE === 'live'
    ? (process.env.E2E_BASE_URL ?? 'http://localhost:8082')
    : (process.env.E2E_BASE_URL ?? 'http://localhost:5173');

export default defineConfig({
  testDir: './tests/e2e',
  // Each spec file runs its scenarios serially against a single browser
  // context so the localStorage locale-lock and route mocks do not
  // cross-contaminate; spec files themselves run in parallel.
  fullyParallel: true,
  // D14 fail-closed in CI: a `.only` left in a spec must not silently
  // shrink the gate. Failing the whole run is the right consequence.
  forbidOnly: !!process.env.CI,
  // Retries are 0 — flake hides bugs. If the suite is flaky the bug is
  // in the suite or the stack, not a justification to retry. D19.
  retries: 0,
  // Single worker keeps timing-sensitive polling assertions stable;
  // expand if/when scenarios are factored to be timing-independent.
  workers: process.env.CI ? 1 : undefined,
  reporter: process.env.CI
    ? [['list'], ['html', { outputFolder: 'playwright-report', open: 'never' }]]
    : 'list',
  use: {
    baseURL: BASE_URL,
    trace: 'on-first-retry',
    screenshot: 'only-on-failure',
    video: 'retain-on-failure',
    // Lock the viewport so layout-shift tests are reproducible.
    viewport: { width: 1280, height: 800 },
  },
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
  ],
  /**
   * Local-dev convenience: spin up `pnpm preview` if no portal is
   * already listening on :5173. In CI (`E2E_MODE=live`) we skip the
   * webServer block entirely — the compose stack owns the lifecycle.
   */
  webServer:
    E2E_MODE === 'live'
      ? undefined
      : {
          command: 'pnpm preview --port 5173',
          url: 'http://localhost:5173',
          reuseExistingServer: !process.env.CI,
          // Vite preview boots in well under 10s on dev hardware; 60s
          // is a generous ceiling that fails fast on a broken build.
          timeout: 60_000,
          stdout: 'pipe',
          stderr: 'pipe',
        },
});
