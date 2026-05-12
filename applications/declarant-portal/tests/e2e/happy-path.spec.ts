/**
 * R-PORT-6 scenario 1 — Happy path.
 *
 * The declarant:
 *   1. lands on the portal in French (locale lock; R-PORT-1)
 *   2. fills the form for a person seeded in mock BUNEC
 *   3. signs + submits the declaration
 *   4. sees the cryptographic receipt (BLAKE3 hash + declaration id)
 *   5. watches the verification status transition pending →
 *      in_verification → accepted while the status badge re-renders
 *
 * In `mocked` mode the trajectory is canned in `fixtures.ts` and
 * progression is driven by the SPA polling on a 3-second cadence.
 * Playwright `toHaveText` retries cover the polling window, so we
 * don't manually sleep.
 *
 * In `live` mode the assertions still hold but the trajectory comes
 * from real Stages 1–2 in the verification engine. The seeded person
 * UUID (`SEEDED_PERSON_ID`) must be present in mock BUNEC; the CI
 * workflow seeds it before invoking Playwright.
 */

import { expect, test } from '@playwright/test';

import {
  E2E_MODE,
  clickSubmit,
  fillDeclarationForm,
  installApiRoutes,
  lockLocaleToFrench,
} from './fixtures';

test.describe('R-PORT-6 — happy path', () => {
  test.beforeEach(async ({ page }) => {
    await lockLocaleToFrench(page);
  });

  test('declarant fills the form, signs, submits, and lands on the accepted status', async ({
    page,
  }) => {
    if (E2E_MODE === 'mocked') {
      await installApiRoutes(page, {
        // Pending → in_verification → accepted. Three responses is
        // enough to exercise the colour-coded state machine; the
        // refetchInterval (3000ms in VerificationStatus.tsx) drives
        // the cadence.
        trajectory: [
          { verification_state: 'pending' },
          { verification_state: 'in_verification', verification_lane: 'yellow' },
          { verification_state: 'accepted', verification_lane: 'green' },
        ],
      });
    }

    await page.goto('/');

    // Form shell is visible (locale-locked in French).
    await expect(
      page.getByRole('heading', {
        level: 2,
        name: /Déposer une déclaration de bénéficiaire effectif/,
      }),
    ).toBeVisible();

    await fillDeclarationForm(page);
    await clickSubmit(page);

    // Verification-status panel takes over (status role + aria-live).
    const statusPanel = page.getByRole('status');
    await expect(statusPanel).toBeVisible({ timeout: 15_000 });

    // The receipt header is immutable — the declaration_id and the
    // BLAKE3 hash render mono-spaced and never change between polls.
    await expect(
      statusPanel.getByText(/Identifiant de la déclaration/),
    ).toBeVisible();
    await expect(
      statusPanel.getByText(/Empreinte du reçu \(BLAKE3\)/),
    ).toBeVisible();

    // The status badge displays the raw protocol token (NOT
    // translated — see VerificationStatus.tsx StatusBadge). When the
    // verification engine has emitted a lane the badge shows the
    // lane (`green`); without one it falls back to the state. For
    // the happy path the lane is always set by terminal time.
    // Polling cadence is 3s; allow generous time for the trajectory
    // to walk to the terminal entry.
    await expect(page.getByTestId('status-badge')).toHaveText('green', {
      timeout: 30_000,
    });

    // The accepted heading is translated (verification.headings.accepted
    // → "Vérification acceptée"). Assert both badge and heading so a
    // regression to either side fails.
    await expect(
      page.getByRole('heading', { name: /Vérification acceptée/i }),
    ).toBeVisible();

    // Once terminal, the polling indicator (rendered only when state
    // is non-terminal) disappears.
    await expect(page.getByTestId('polling-indicator')).toHaveCount(0);
  });
});
