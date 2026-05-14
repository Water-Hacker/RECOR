/**
 * Reusable form-field primitives for the wizard steps.
 *
 * Extracted from the single-page `DeclarationForm` so every wizard
 * step renders inputs with identical styling, error placement, and
 * `role="alert"` semantics — the assistive-tech contract that
 * R-PORT-5 (a11y audit) will lean on.
 */

import type { ReactNode } from 'react';
// @ts-ignore
import type { TFunction } from 'i18next';

/** Tailwind classes shared by every text/number/select input. */
export const inputCls =
  'block w-full rounded-md border border-slate-300 bg-white px-3 py-2 text-slate-900 shadow-sm focus:border-recor-deep focus:ring-2 focus:ring-recor-accent';

/**
 * Resolve a Zod validation message into a localised string.
 *
 * The Zod schema emits i18next keys (e.g. `'validation.uuid'`);
 * anything that does not begin with `validation.` is treated as a
 * raw fallback string. Mirrors the helper that lived on the
 * single-page form (D14 fail-closed — never render a raw key in
 * production).
 */
export function tValidationMessage(
  t: TFunction,
  message?: string,
): string | undefined {
  if (!message) return undefined;
  if (message.startsWith('validation.')) {
    return t(message);
  }
  return message;
}

interface FieldProps {
  label: string;
  error?: string;
  children: ReactNode;
}

/**
 * Labelled form field with an inline `role="alert"` error region.
 *
 * The error region only renders when an error is present so screen
 * readers do not announce empty regions on first render.
 */
export function Field({ label, error, children }: FieldProps) {
  return (
    <label className="block">
      <span className="mb-1 block text-sm font-medium text-slate-800">
        {label}
      </span>
      {children}
      {error && (
        <span role="alert" className="mt-1 block text-sm text-red-700">
          {error}
        </span>
      )}
    </label>
  );
}
