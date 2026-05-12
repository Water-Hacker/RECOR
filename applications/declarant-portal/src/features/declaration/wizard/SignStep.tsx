/**
 * Wizard step 4 — Sign + submit.
 *
 * Renders the cryptographic-attestation confirmation block plus the
 * submit affordance. The actual signing + POST is delegated to the
 * shell's mutation (so step 4 is purely the user-facing CTA).
 *
 * D15 cryptographic provenance: this step signs over the SAME
 * canonical bytes previewed on step 3 — the shell holds a stable
 * `declaration_id` + `nonce_hex` from the moment the user enters
 * step 3 forward, and passes both into `buildSignedRequest` here.
 */

import { useTranslation } from 'react-i18next';
import clsx from 'clsx';

import { ApiError } from '../../../lib/api';

interface SignStepProps {
  /** Submit handler — wired to the shell's TanStack-Query mutation. */
  onSubmit: () => void;
  /** Whether the mutation is currently in flight. */
  pending: boolean;
  /** Render the user-facing error if the mutation failed. */
  error: Error | null;
  /** Public-key hex of the keypair the declarant is about to use. */
  publicKeyHex: string;
}

export function SignStep({
  onSubmit,
  pending,
  error,
  publicKeyHex,
}: SignStepProps) {
  const { t } = useTranslation();
  return (
    <section
      aria-labelledby="wizard-step-4-heading"
      data-testid="wizard-step-4"
      className="space-y-6"
    >
      <h3 id="wizard-step-4-heading" className="text-lg font-semibold text-slate-900">
        {t('wizard.steps.4.heading')}
      </h3>
      <p className="text-sm text-slate-600">{t('wizard.steps.4.description')}</p>

      <div className="rounded-md border border-slate-200 bg-slate-50 p-4">
        <h4 className="text-sm font-semibold text-slate-900">
          {t('wizard.sign.publicKeyHeading')}
        </h4>
        <p className="text-xs text-slate-600">
          {t('wizard.sign.publicKeyExplanation')}
        </p>
        <p
          className="mt-2 break-all font-mono text-xs text-slate-800"
          data-testid="wizard-sign-public-key"
        >
          {publicKeyHex}
        </p>
      </div>

      {error && (
        <div
          role="alert"
          className="rounded-md border border-red-300 bg-red-50 p-4 text-sm text-red-900"
        >
          <strong>{t('form.errors.submissionFailedLabel')}</strong>{' '}
          {error instanceof ApiError
            ? `${error.kind} — ${error.message}`
            : error.message}
        </div>
      )}

      <button
        type="button"
        onClick={onSubmit}
        disabled={pending}
        data-testid="wizard-sign-submit"
        className={clsx(
          'inline-flex items-center justify-center rounded-md px-6 py-3 text-base font-semibold text-white shadow-sm transition',
          'bg-recor-deep hover:bg-blue-900',
          'disabled:cursor-not-allowed disabled:bg-slate-400',
        )}
      >
        {pending ? t('form.submit.pending') : t('form.submit.idle')}
      </button>
    </section>
  );
}
