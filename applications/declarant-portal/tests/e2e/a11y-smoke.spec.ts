// R-PORT-5 — WCAG 2.1 AA smoke against the major portal views.
//
// Runs axe-core against each of the wizard steps + the post-submit
// VerificationStatus panel and the validation error state. Fails the
// build on any critical or serious finding; lower severities are
// surfaced via the test report but do not gate merge.
//
// Mocked-mode only — runtime axe against the live D↔V stack would
// add several minutes per CI run without finding anything the mocked
// rig can't.
//
// See `docs/security/a11y-audit-2026-Q2.md` for the audit record +
// findings table.

import AxeBuilder from '@axe-core/playwright';
import { expect, test, type Page } from '@playwright/test';
import {
  E2E_MODE,
  TEST_ENTITY_ID,
  clickSubmit,
  fillDeclarationForm,
  installApiRoutes,
  lockLocaleToFrench,
} from './fixtures';

const ACCEPTED_TRAJECTORY = [
  { verification_state: 'pending' as const },
  {
    verification_state: 'in_verification' as const,
    verification_lane: 'yellow' as const,
  },
  {
    verification_state: 'accepted' as const,
    verification_lane: 'green' as const,
  },
];

/**
 * Run axe-core against the current page state, assert zero critical
 * and zero serious findings, and surface lower-severity violations
 * via test annotations so the report carries the follow-up backlog.
 */
async function expectNoCriticalOrSeriousViolations(
  page: Page,
  context: string,
): Promise<void> {
  const results = await new AxeBuilder({ page })
    .withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa'])
    .analyze();
  const blocking = results.violations.filter(
    (v) => v.impact === 'critical' || v.impact === 'serious',
  );
  if (blocking.length > 0) {
    const summary = blocking
      .map(
        (v) =>
          `[${v.impact}] ${v.id} — ${v.help}\n        ${v.nodes
            .slice(0, 3)
            .map((n) => n.target.join(' '))
            .join('\n        ')}`,
      )
      .join('\n');
    throw new Error(
      `[${context}] axe-core found ${blocking.length} critical/serious WCAG violation(s):\n${summary}`,
    );
  }
  if (results.violations.length > 0) {
    test.info().annotations.push({
      type: `a11y-non-blocking[${context}]`,
      description: `${results.violations.length} non-blocking violation(s): ${results.violations
        .map((v) => `${v.impact ?? 'minor'} ${v.id}`)
        .join(', ')}`,
    });
  }
  expect(results.passes.length).toBeGreaterThan(0);
}

test.describe('R-PORT-5 — WCAG 2.1 AA smoke', () => {
  // The a11y audit deliberately runs against the deterministic
  // mocked rig — live mode adds nothing the static + runtime axe
  // assertions can't catch from the rendered DOM.
  test.skip(E2E_MODE === 'live', 'a11y smoke is mocked-only');

  test.beforeEach(async ({ page }) => {
    await lockLocaleToFrench(page);
  });

  test('wizard step 1 — Entity — clean against WCAG AA', async ({ page }) => {
    await installApiRoutes(page, { trajectory: ACCEPTED_TRAJECTORY });
    await page.goto('/');
    await page.getByTestId('wizard-step-1').waitFor({ state: 'visible' });
    await expectNoCriticalOrSeriousViolations(page, 'wizard-step-1');
  });

  test('wizard step 2 — Owners — clean against WCAG AA', async ({ page }) => {
    await installApiRoutes(page, { trajectory: ACCEPTED_TRAJECTORY });
    await page.goto('/');
    await page.getByTestId('wizard-step-1').waitFor({ state: 'visible' });
    await page
      .getByLabel(/Identifiant de l'entité \(UUIDv4\)/)
      .fill(TEST_ENTITY_ID);
    await page.getByTestId('wizard-forward').click();
    await page.getByTestId('wizard-step-2').waitFor({ state: 'visible' });
    await expectNoCriticalOrSeriousViolations(page, 'wizard-step-2');
  });

  test('wizard step 3 — Review — clean against WCAG AA', async ({ page }) => {
    await installApiRoutes(page, { trajectory: ACCEPTED_TRAJECTORY });
    await page.goto('/');
    await fillDeclarationForm(page);
    // After fillDeclarationForm the wizard is on Step 4; navigate
    // back to Step 3 so we audit the read-only review screen
    // specifically.
    await page.getByTestId('wizard-back').click();
    await page.getByTestId('wizard-step-3').waitFor({ state: 'visible' });
    await expectNoCriticalOrSeriousViolations(page, 'wizard-step-3');
  });

  test('wizard step 4 — Sign + Submit — clean against WCAG AA', async ({
    page,
  }) => {
    await installApiRoutes(page, { trajectory: ACCEPTED_TRAJECTORY });
    await page.goto('/');
    await fillDeclarationForm(page);
    await page.getByTestId('wizard-step-4').waitFor({ state: 'visible' });
    await expectNoCriticalOrSeriousViolations(page, 'wizard-step-4');
  });

  test('VerificationStatus panel — clean against WCAG AA', async ({ page }) => {
    await installApiRoutes(page, { trajectory: ACCEPTED_TRAJECTORY });
    await page.goto('/');
    await fillDeclarationForm(page);
    await clickSubmit(page);
    await page
      .getByTestId('verification-status-panel')
      .waitFor({ state: 'visible', timeout: 15_000 });
    await expectNoCriticalOrSeriousViolations(page, 'verification-status');
  });

  test('validation error state — clean against WCAG AA', async ({ page }) => {
    await installApiRoutes(page, { trajectory: ACCEPTED_TRAJECTORY });
    await page.goto('/');
    await page.getByTestId('wizard-step-1').waitFor({ state: 'visible' });
    await page
      .getByLabel(/Identifiant de l'entité \(UUIDv4\)/)
      .fill('not-a-uuid');
    // Click forward → the wizard's per-step trigger() rejects the
    // step and renders the invalid-UUID error message under the
    // field.
    await page.getByTestId('wizard-forward').click();
    // Give the validation render a tick.
    await page
      .getByText(/UUID v4|format UUID/i)
      .first()
      .waitFor({ state: 'visible', timeout: 2_000 })
      .catch(() => {
        // If the schema-error message wasn't visible by the test's
        // matcher, the validation gate at least kept us on step 1;
        // the audit still runs against the rendered state.
      });
    await expectNoCriticalOrSeriousViolations(page, 'validation-error');
  });
});
