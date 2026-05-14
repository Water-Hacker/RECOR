/**
 * R-PORT-6 scenario 4 — Polling halts on terminal state.
 *
 * `VerificationStatus.tsx` uses TanStack Query with a
 * `refetchInterval` function that returns `false` when
 * `isTerminalVerificationState(state)` is true (accepted / rejected).
 * This spec proves the SPA actually honours that contract: after the
 * status walks to a terminal value, no further `GET /v1/declarations/{id}`
 * call fires.
 *
 * Method:
 *   1. Mock the API with a trajectory that lands on `accepted` on the
 *      third poll.
 *   2. Wait until the status badge reads `accepted`.
 *   3. Sample the GET-count from the mocked-route state.
 *   4. Wait several polling intervals (3s × 3 ≈ 10s).
 *   5. Assert the GET-count has NOT moved.
 *
 * Live-mode caveat: this scenario is mocked-only because counting
 * network calls precisely against a live polling stack is racy
 * (network jitter widens the post-terminal window). The contract
 * being tested is purely SPA-side — the live mode's value-add is
 * covered by the other three scenarios. CI runs this spec in mocked
 * mode regardless of `E2E_MODE`.
 */

// @ts-ignore
import { expect, test, type Page } from '@playwright/test';

import {
  clickSubmit,
  fillDeclarationForm,
  installApiRoutes,
  lockLocaleToFrench,
} from './fixtures';

test.describe('R-PORT-6 — polling stops on terminal state', () => {
  test.beforeEach(async ({ page }: { page: Page }) => {
    await lockLocaleToFrench(page);
  });

  test('no GET fires after the declaration reaches accepted', async ({
    page,
  }: {
    page: Page;
  }) => {
    const state = await installApiRoutes(page, {
      trajectory: [
        { verification_state: 'pending' },
        { verification_state: 'in_verification', verification_lane: 'yellow' },
        { verification_state: 'accepted', verification_lane: 'green' },
      ],
    });

    await page.goto('/');
    await fillDeclarationForm(page);
    await clickSubmit(page);

    // Wait for the terminal state to render. The mocked trajectory
    // emits `verification_lane: 'green'` alongside the terminal
    // state, so the badge shows the lane token (`StatusBadge` uses
    // `lane ?? state`).
    await expect(page.getByTestId('status-badge')).toHaveText('green', {
      timeout: 30_000,
    });

    // Sample the GET-count immediately after terminal. The polling
    // indicator disappears as soon as the terminal frame is parsed,
    // but there may be one in-flight GET still completing when the
    // badge re-renders, so we tolerate one additional GET in the
    // 500ms grace window.
    await page.waitForTimeout(500);
    const baseline = state.getCalls;

    // Wait three full polling intervals. Were the SPA still polling,
    // we would see at least three more GETs by now (refetchInterval =
    // 3000ms in VerificationStatus.tsx).
    await page.waitForTimeout(10_000);

    expect(state.getCalls).toBe(baseline);
  });
});
