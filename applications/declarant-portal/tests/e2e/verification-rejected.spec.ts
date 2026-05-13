/**
 * R-PORT-6 scenario 3 — Verification rejected (red lane).
 *
 * The declarant submits a declaration for a person NOT seeded in mock
 * BUNEC. The verification engine's identity stage (Stage 1) returns
 * `BunecLookup::NotFound`; fusion then routes the case to the red
 * lane and the declaration's projection lands on
 * `verification_state = 'rejected'`.
 *
 * The portal renders the rejected status with:
 *   - heading "Vérification rejetée"
 *   - container styled red (border-red-700 bg-red-50)
 *   - status badge "rejected" (raw protocol token, not translated)
 *
 * In `mocked` mode we feed a `pending → in_verification → rejected`
 * trajectory through `installApiRoutes`. In `live` mode the
 * `UNSEEDED_PERSON_ID` UUID drives the same outcome via the real
 * pipeline; CI seeds the OTHER person UUID before running this spec,
 * leaving `UNSEEDED_PERSON_ID` deliberately absent.
 */

import { expect, test } from '@playwright/test';

import {
  E2E_MODE,
  UNSEEDED_PERSON_ID,
  clickSubmit,
  fillDeclarationForm,
  installApiRoutes,
  lockLocaleToFrench,
} from './fixtures';

test.describe('R-PORT-6 — verification rejected (red lane)', () => {
  test.beforeEach(async ({ page }) => {
    await lockLocaleToFrench(page);
  });

  test('unseeded person yields a red-lane rejected status', async ({ page }) => {
    if (E2E_MODE === 'mocked') {
      await installApiRoutes(page, {
        // Walk to rejected. Two intermediate frames mirror what the
        // real pipeline emits while Stage 1 + fusion are still resolving.
        trajectory: [
          { verification_state: 'pending' },
          { verification_state: 'in_verification', verification_lane: 'yellow' },
          { verification_state: 'rejected', verification_lane: 'red' },
        ],
        declarationId: '018f0000-0000-4000-8000-000000000d02',
      });
    }

    await page.goto('/');

    await fillDeclarationForm(page, { personId: UNSEEDED_PERSON_ID });
    await clickSubmit(page);

    // Status panel takes over. Use the explicit data-testid rather
    // than role="status" because the wizard's drafts-unavailable
    // notice is also role="status" and `getByRole` would match the
    // first one (live IndexedDB availability varies by runner).
    const statusPanel = page.getByTestId('verification-status-panel');
    await expect(statusPanel).toBeVisible({ timeout: 15_000 });

    // The "rejected" heading is translated (fr.json
    // verification.headings.rejected → "Vérification rejetée"). The
    // badge is NOT translated; when a lane is present (`red` here)
    // the badge prefers it (see VerificationStatus.tsx
    // `StatusBadge` — `lane ?? state`). Assert both so any future
    // translation drift or badge logic regression is caught.
    await expect(
      statusPanel.getByRole('heading', { name: /Vérification rejetée/i }),
    ).toBeVisible({ timeout: 30_000 });

    await expect(page.getByTestId('status-badge')).toHaveText('red', {
      timeout: 30_000,
    });

    // The container styling switches to red on rejected/red-lane. We
    // assert via class substring rather than computed colour so the
    // assertion survives a Tailwind palette tweak that keeps the
    // `border-red-700` token intact.
    await expect(statusPanel).toHaveClass(/border-red-700/);

    // Terminal state ⇒ polling indicator disappears.
    await expect(page.getByTestId('polling-indicator')).toHaveCount(0);
  });
});
