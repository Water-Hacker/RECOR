/**
 * Wizard step 3 — Review.
 *
 * Read-only summary of every typed field. Renders the canonical
 * payload bytes the declarant is about to sign — short hex prefix
 * plus length — so the user has a chance to recognise tampering
 * before committing the Ed25519 signature (D15 cryptographic
 * provenance).
 *
 * The wizard shell pre-generates a nonce on entering step 3 and
 * holds it stable for steps 3 and 4. Step 4 then signs over the
 * EXACT bytes previewed here — the parity between preview and
 * signature payload is the load-bearing invariant.
 */

import { useTranslation } from 'react-i18next';
import { useWatch, type UseFormReturn } from 'react-hook-form';

import type { FormValues } from '../schema';
import {
  bytesToHex,
  canonicalPayloadBytes,
  type DeclarationPayload,
} from '../../../lib/crypto';

interface ReviewStepProps {
  form: UseFormReturn<FormValues>;
  declarationId: string;
  nonceHex: string;
}

/** Number of leading bytes of the canonical payload rendered as hex. */
const PREVIEW_PREFIX_BYTES = 32;

export function ReviewStep({ form, declarationId, nonceHex }: ReviewStepProps) {
  const { t } = useTranslation();

  // useWatch ensures the preview re-renders when the user edits an
  // upstream step and navigates forward again. Reading values via
  // form.getValues() would snapshot once and miss the update.
  const values = useWatch({ control: form.control });

  const payload: DeclarationPayload = {
    declaration_id: declarationId,
    entity_id: values.entity_id ?? '',
    declarant_principal: values.declarant_principal ?? '',
    declarant_role: values.declarant_role ?? 'self',
    kind: values.kind ?? 'incorporation',
    effective_from: values.effective_from ?? '',
    beneficial_owners: (values.beneficial_owners ?? []).map((o) => ({
      person_id: o?.person_id ?? '',
      ownership_basis_points: o?.ownership_basis_points ?? 0,
      interest_kind: o?.interest_kind ?? 'equity',
    })),
    nonce_hex: nonceHex,
  };

  const canonical = canonicalPayloadBytes(payload);
  const previewHex = bytesToHex(canonical.slice(0, PREVIEW_PREFIX_BYTES));
  const ownerSum = payload.beneficial_owners.reduce(
    (acc, o) => acc + (Number.isFinite(o.ownership_basis_points) ? o.ownership_basis_points : 0),
    0,
  );

  return (
    <section
      aria-labelledby="wizard-step-3-heading"
      data-testid="wizard-step-3"
      className="space-y-6"
    >
      <h3 id="wizard-step-3-heading" className="text-lg font-semibold text-slate-900">
        {t('wizard.steps.3.heading')}
      </h3>
      <p className="text-sm text-slate-600">{t('wizard.steps.3.description')}</p>

      <dl className="grid grid-cols-1 gap-y-3 rounded-md border border-slate-200 bg-slate-50 p-4 text-sm md:grid-cols-3">
        <ReviewRow label={t('form.fields.entityId.label')} value={payload.entity_id} />
        <ReviewRow
          label={t('form.fields.declarantPrincipal.label')}
          value={payload.declarant_principal}
        />
        <ReviewRow
          label={t('form.fields.declarantRole.label')}
          value={t(`form.fields.declarantRole.options.${payload.declarant_role}`)}
        />
        <ReviewRow
          label={t('form.fields.kind.label')}
          value={t(`form.fields.kind.options.${payload.kind}`)}
        />
        <ReviewRow
          label={t('form.fields.effectiveFrom.label')}
          value={payload.effective_from}
        />
        <ReviewRow
          label={t('wizard.review.declarationIdLabel')}
          value={declarationId}
        />
      </dl>

      <div className="space-y-2">
        <h4 className="text-sm font-semibold text-slate-900">
          {t('form.owners.legend')}
        </h4>
        <ul className="space-y-2" data-testid="wizard-review-owners">
          {payload.beneficial_owners.map((o, idx) => (
            <li
              key={`${o.person_id}-${idx}`}
              className="rounded-md border border-slate-200 bg-white p-3 text-sm"
            >
              <div className="font-mono text-slate-800">{o.person_id || '—'}</div>
              <div className="text-slate-600">
                {(o.ownership_basis_points / 100).toFixed(2)}%{' · '}
                {t(`form.owners.interestKind.options.${o.interest_kind}`)}
              </div>
            </li>
          ))}
        </ul>
        <p className="text-xs text-slate-600">
          {t('wizard.review.ownershipTotal', {
            percent: (ownerSum / 100).toFixed(2),
          })}
        </p>
      </div>

      <div className="space-y-2 rounded-md border border-amber-300 bg-amber-50 p-4">
        <h4 className="text-sm font-semibold text-amber-900">
          {t('wizard.review.canonicalBytesHeading')}
        </h4>
        <p className="text-xs text-amber-800">
          {t('wizard.review.canonicalBytesExplanation')}
        </p>
        <dl className="grid grid-cols-1 gap-2 text-xs text-amber-900 md:grid-cols-3">
          <div>
            <dt className="font-semibold">
              {t('wizard.review.canonicalBytesLengthLabel')}
            </dt>
            <dd data-testid="wizard-canonical-bytes-length">
              {canonical.length}
            </dd>
          </div>
          <div className="md:col-span-2">
            <dt className="font-semibold">
              {t('wizard.review.canonicalBytesPrefixLabel', {
                bytes: PREVIEW_PREFIX_BYTES,
              })}
            </dt>
            <dd
              className="break-all font-mono"
              data-testid="wizard-canonical-bytes-prefix"
            >
              {previewHex}
              {canonical.length > PREVIEW_PREFIX_BYTES && '…'}
            </dd>
          </div>
        </dl>
      </div>
    </section>
  );
}

function ReviewRow({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <dt className="text-xs font-semibold uppercase text-slate-500">
        {label}
      </dt>
      <dd className="text-slate-900">{value || '—'}</dd>
    </div>
  );
}
