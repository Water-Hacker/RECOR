/**
 * Shared wizard-step typing.
 *
 * The wizard is a 4-step linear flow (Entity → Owners → Review → Sign).
 * Each step exposes the subset of form-field names it owns so the
 * shell can call `form.trigger(STEP_FIELDS[step])` to validate just
 * the active step's inputs (D14 fail-closed: Forward refuses to
 * advance until the active step's fields validate).
 */

import type { FieldPath } from 'react-hook-form';

import type { FormValues } from '../schema';

export type WizardStep = 1 | 2 | 3 | 4;

export const WIZARD_STEPS = [1, 2, 3, 4] as const satisfies readonly WizardStep[];

/**
 * Fields that each step owns. Step 3 (Review) and Step 4 (Sign) own
 * no inputs of their own — they are read-only summaries / signing
 * affordances — so they return an empty list and the Forward gate
 * collapses to "always allowed once the previous steps validated".
 *
 * The arrays are typed as `FieldPath<FormValues>[]` so a future
 * rename of a Zod field surfaces as a typecheck error here rather
 * than a silent runtime mismatch.
 */
export const STEP_FIELDS: Record<WizardStep, ReadonlyArray<FieldPath<FormValues>>> = {
  1: ['entity_id', 'declarant_principal', 'declarant_role', 'kind', 'effective_from'],
  2: ['beneficial_owners'],
  3: [],
  4: [],
};

export const FIRST_STEP: WizardStep = 1;
export const LAST_STEP: WizardStep = 4;

export function isWizardStep(value: number): value is WizardStep {
  return value === 1 || value === 2 || value === 3 || value === 4;
}
