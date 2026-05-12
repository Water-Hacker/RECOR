/**
 * Wizard step 2 — Beneficial owners.
 *
 * Wraps the existing `useFieldArray` sub-form. The wizard shell holds
 * the parent `useForm` instance and passes it down; this step pulls
 * the field-array helpers off `form.control`. Add/remove and
 * basis-points editing behave exactly as on the single-page form.
 *
 * The basis-points-sum and uniqueness checks are encoded as Zod
 * `.refine` clauses on `beneficial_owners`, so `form.trigger
 * (['beneficial_owners'])` from the shell exercises them and the
 * Forward button refuses to advance until ownership totals 10 000bp
 * (D14 fail-closed).
 */

import { useTranslation } from 'react-i18next';
import { useFieldArray, type UseFormReturn } from 'react-hook-form';

import type { FormValues } from '../schema';
import { inputCls, tValidationMessage } from './field';

interface OwnersStepProps {
  form: UseFormReturn<FormValues>;
}

export function OwnersStep({ form }: OwnersStepProps) {
  const { t } = useTranslation();
  const { fields, append, remove } = useFieldArray({
    control: form.control,
    name: 'beneficial_owners',
  });
  const ownersErrors = form.formState.errors.beneficial_owners;

  return (
    <section
      aria-labelledby="wizard-step-2-heading"
      data-testid="wizard-step-2"
      className="space-y-4"
    >
      <h3 id="wizard-step-2-heading" className="text-lg font-semibold text-slate-900">
        {t('wizard.steps.2.heading')}
      </h3>
      <p className="text-sm text-slate-600">
        {t('form.owners.instructions')}
      </p>

      {fields.map((field, index) => (
        <OwnerRow
          key={field.id}
          index={index}
          form={form}
          onRemove={() => remove(index)}
          removable={fields.length > 1}
        />
      ))}

      <button
        type="button"
        onClick={() =>
          append({
            person_id: '',
            ownership_basis_points: 0,
            interest_kind: 'equity',
          })
        }
        className="text-sm font-medium text-recor-deep underline hover:no-underline"
      >
        {t('form.owners.addButton')}
      </button>

      {ownersErrors?.root?.message && (
        <p role="alert" className="text-sm text-red-700">
          {tValidationMessage(t, ownersErrors.root.message)}
        </p>
      )}
      {ownersErrors?.message && (
        <p role="alert" className="text-sm text-red-700">
          {tValidationMessage(t, ownersErrors.message)}
        </p>
      )}
    </section>
  );
}

interface OwnerRowProps {
  index: number;
  form: UseFormReturn<FormValues>;
  onRemove: () => void;
  removable: boolean;
}

function OwnerRow({ index, form, onRemove, removable }: OwnerRowProps) {
  const { t } = useTranslation();
  const rowErrors = form.formState.errors.beneficial_owners?.[index] as
    | Record<string, { message?: string }>
    | undefined;
  return (
    <div className="grid grid-cols-1 gap-3 rounded-md border border-slate-200 bg-white p-4 md:grid-cols-12">
      <div className="md:col-span-6">
        <label className="block">
          <span className="mb-1 block text-sm font-medium text-slate-800">
            {t('form.owners.personId.label')}
          </span>
          <input
            type="text"
            autoComplete="off"
            className={inputCls}
            placeholder={t('form.owners.personId.placeholder')}
            {...form.register(`beneficial_owners.${index}.person_id`)}
          />
          {rowErrors?.person_id?.message && (
            <span role="alert" className="mt-1 block text-sm text-red-700">
              {tValidationMessage(t, rowErrors.person_id.message)}
            </span>
          )}
        </label>
      </div>
      <div className="md:col-span-3">
        <label className="block">
          <span className="mb-1 block text-sm font-medium text-slate-800">
            {t('form.owners.basisPoints.label')}
          </span>
          <input
            type="number"
            min={1}
            max={10_000}
            step={1}
            inputMode="numeric"
            className={inputCls}
            {...form.register(`beneficial_owners.${index}.ownership_basis_points`, {
              valueAsNumber: true,
            })}
          />
          {rowErrors?.ownership_basis_points?.message && (
            <span role="alert" className="mt-1 block text-sm text-red-700">
              {tValidationMessage(t, rowErrors.ownership_basis_points.message)}
            </span>
          )}
        </label>
      </div>
      <div className="md:col-span-2">
        <label className="block">
          <span className="mb-1 block text-sm font-medium text-slate-800">
            {t('form.owners.interestKind.label')}
          </span>
          <select
            className={inputCls}
            {...form.register(`beneficial_owners.${index}.interest_kind`)}
          >
            <option value="equity">
              {t('form.owners.interestKind.options.equity')}
            </option>
            <option value="voting">
              {t('form.owners.interestKind.options.voting')}
            </option>
            <option value="family_proxy">
              {t('form.owners.interestKind.options.family_proxy')}
            </option>
            <option value="contractual">
              {t('form.owners.interestKind.options.contractual')}
            </option>
            <option value="other">
              {t('form.owners.interestKind.options.other')}
            </option>
          </select>
        </label>
      </div>
      <div className="flex items-end md:col-span-1">
        {removable && (
          <button
            type="button"
            onClick={onRemove}
            aria-label={t('form.owners.removeButtonAria', {
              index: index + 1,
            })}
            className="rounded-md border border-slate-300 px-3 py-2 text-sm text-slate-700 hover:bg-slate-100"
          >
            {t('form.owners.removeButton')}
          </button>
        )}
      </div>
    </div>
  );
}
