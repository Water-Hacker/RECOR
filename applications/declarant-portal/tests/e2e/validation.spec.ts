/**
 * R-PORT-6 scenario 2 — Validation gate.
 *
 * The wizard's Forward button calls `form.trigger(STEP_FIELDS[step])`;
 * a non-UUID `entity_id` fails the Zod regex on step 1 and Forward
 * refuses to advance to step 2. This spec exercises that gate
 * explicitly and asserts:
 *
 *   1. The inline error renders under the entity_id field
 *      (translated French message "format UUIDv4 attendu", role=alert).
 *   2. The wizard stays on step 1 — `wizard-step-2` is NOT in the DOM.
 *   3. No POST to /v1/declarations is issued (the form never reaches
 *      step 4 to fire it).
 *
 * D14 fail-closed: the gate is structural (next step does not render),
 * not just visual.
 */

import { expect, test } from '@playwright/test';

import {
  E2E_MODE,
  installApiRoutes,
  lockLocaleToFrench,
} from './fixtures';

test.describe('R-PORT-6 — validation', () => {
  test.beforeEach(async ({ page }) => {
    await lockLocaleToFrench(page);
  });

  test('invalid entity_id surfaces a UUID error and forward gate refuses to advance', async ({
    page,
  }) => {
    // Track POSTs to /v1/declarations. An accidental advance through
    // the wizard would surface here.
    let networkPostCount = 0;
    if (E2E_MODE === 'mocked') {
      await installApiRoutes(page, {
        trajectory: [{ verification_state: 'pending' }],
      });
    }
    page.on('request', (req) => {
      if (
        req.method() === 'POST' &&
        req.url().endsWith('/v1/declarations')
      ) {
        networkPostCount += 1;
      }
    });

    await page.goto('/');

    // The wizard renders step 1 once keys are ready.
    await page.getByTestId('wizard-step-1').waitFor({ state: 'visible' });

    // Type a non-UUID value into entity_id. Other step-1 fields stay
    // at their defaults (already valid), so entity_id is the single
    // failing field.
    const entityInput = page.getByLabel(
      /Identifiant de l'entité \(UUIDv4\)/,
    );
    await entityInput.fill('definitely-not-a-uuid');
    await entityInput.press('Tab');

    // Click the wizard's Forward button — should refuse.
    await page.getByTestId('wizard-forward').click();

    // Inline error text under the field. The French translation of
    // `validation.uuid` is "format UUIDv4 attendu" (see locales/fr.json).
    const entityError = page
      .getByRole('alert')
      .filter({ hasText: /format UUIDv4 attendu/i });
    await expect(entityError).toBeVisible();

    // Structural assertion: the wizard is still on step 1, and step 2
    // has NOT mounted. This is the doctrine-D14 fail-closed property.
    await expect(page.getByTestId('wizard-step-1')).toBeVisible();
    await expect(page.getByTestId('wizard-step-2')).toHaveCount(0);

    // And no POST was issued — the wizard never reached the sign step.
    expect(networkPostCount).toBe(0);
  });
});
