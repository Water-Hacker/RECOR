/**
 * Wizard step 1 — Entity.
 *
 * Owns the entity-identifying fields: entity_id, declarant_principal,
 * declarant_role, kind, effective_from. The wizard shell holds the
 * `useForm` instance; this step receives it as a prop and registers
 * its inputs against it so values persist across step navigation.
 */

import { useTranslation } from 'react-i18next';
import type { UseFormReturn } from 'react-hook-form';

import type { FormValues } from '../schema';
import { Field, inputCls, tValidationMessage } from './field';

interface EntityStepProps {
  form: UseFormReturn<FormValues>;
}

export function EntityStep({ form }: EntityStepProps) {
  const { t } = useTranslation();
  const { errors } = form.formState;
  return (
    <section
      aria-labelledby="wizard-step-1-heading"
      data-testid="wizard-step-1"
      className="space-y-6"
    >
      <h3 id="wizard-step-1-heading" className="text-lg font-semibold text-slate-900">
        {t('wizard.steps.1.heading')}
      </h3>
      <p className="text-sm text-slate-600">{t('wizard.steps.1.description')}</p>

      <Field
        label={t('form.fields.entityId.label')}
        error={tValidationMessage(t, errors.entity_id?.message)}
      >
        <input
          type="text"
          inputMode="text"
          autoComplete="off"
          className={inputCls}
          placeholder={t('form.fields.entityId.placeholder')}
          {...form.register('entity_id')}
        />
      </Field>

      <Field
        label={t('form.fields.declarantPrincipal.label')}
        error={tValidationMessage(t, errors.declarant_principal?.message)}
      >
        <input
          type="text"
          autoComplete="off"
          className={inputCls}
          {...form.register('declarant_principal')}
        />
      </Field>

      <div className="grid grid-cols-1 gap-6 md:grid-cols-3">
        <Field
          label={t('form.fields.declarantRole.label')}
          error={tValidationMessage(t, errors.declarant_role?.message)}
        >
          <select className={inputCls} {...form.register('declarant_role')}>
            <option value="self">
              {t('form.fields.declarantRole.options.self')}
            </option>
            <option value="authorised_agent">
              {t('form.fields.declarantRole.options.authorised_agent')}
            </option>
            <option value="operator_assisted">
              {t('form.fields.declarantRole.options.operator_assisted')}
            </option>
          </select>
        </Field>
        <Field
          label={t('form.fields.kind.label')}
          error={tValidationMessage(t, errors.kind?.message)}
        >
          <select className={inputCls} {...form.register('kind')}>
            <option value="incorporation">
              {t('form.fields.kind.options.incorporation')}
            </option>
            <option value="annual_renewal">
              {t('form.fields.kind.options.annual_renewal')}
            </option>
            <option value="change_of_control">
              {t('form.fields.kind.options.change_of_control')}
            </option>
            <option value="correction">
              {t('form.fields.kind.options.correction')}
            </option>
            <option value="amendment">
              {t('form.fields.kind.options.amendment')}
            </option>
          </select>
        </Field>
        <Field
          label={t('form.fields.effectiveFrom.label')}
          error={tValidationMessage(t, errors.effective_from?.message)}
        >
          <input
            type="date"
            className={inputCls}
            {...form.register('effective_from')}
          />
        </Field>
      </div>
    </section>
  );
}
