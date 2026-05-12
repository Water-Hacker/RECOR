import { useEffect, useState } from 'react';
import { useFieldArray, useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { useMutation } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import type { TFunction } from 'i18next';
import clsx from 'clsx';

import { type FormValues, formSchema } from './schema';
import {
  buildSignedRequest,
  generateKeys,
  isEd25519Supported,
  randomUuid,
  type DeclarantKeys,
  type SignedDeclarationRequest,
} from '../../lib/crypto';
import { submitDeclaration, type SubmitDeclarationResponse, ApiError } from '../../lib/api';
import { VerificationStatus } from './VerificationStatus';

interface DeclarationFormProps {
  apiBaseUrl: string;
}

/**
 * Resolve a Zod validation message into a localised string. The
 * schema emits i18next keys (e.g. `'validation.uuid'`); anything that
 * does not begin with `validation.` is treated as a raw fallback
 * message (defensive: should not happen if schema and translations
 * are in sync, but keeps a missing-key from rendering as the literal
 * key in production — D14 fail-closed).
 */
function tValidationMessage(t: TFunction, message?: string): string | undefined {
  if (!message) return undefined;
  if (message.startsWith('validation.')) {
    return t(message);
  }
  return message;
}

export function DeclarationForm({ apiBaseUrl }: DeclarationFormProps) {
  const { t } = useTranslation();
  const [keys, setKeys] = useState<DeclarantKeys | null>(null);
  const [supported, setSupported] = useState<boolean | null>(null);
  const [keyGenError, setKeyGenError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      const ok = await isEd25519Supported();
      if (cancelled) return;
      setSupported(ok);
      if (!ok) return;
      try {
        const k = await generateKeys();
        if (!cancelled) setKeys(k);
      } catch (e) {
        if (!cancelled) {
          setKeyGenError(e instanceof Error ? e.message : 'key generation failed');
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  const form = useForm<FormValues>({
    resolver: zodResolver(formSchema),
    defaultValues: {
      entity_id: '',
      declarant_principal: 'spiffe://recor.cm/declarant-001',
      declarant_role: 'self',
      kind: 'incorporation',
      effective_from: new Date().toISOString().slice(0, 10),
      beneficial_owners: [
        {
          person_id: '',
          ownership_basis_points: 10_000,
          interest_kind: 'equity',
        },
      ],
    },
    mode: 'onBlur',
  });

  const { fields, append, remove } = useFieldArray({
    control: form.control,
    name: 'beneficial_owners',
  });

  const submitMutation = useMutation<
    SubmitDeclarationResponse,
    Error,
    { signed: SignedDeclarationRequest }
  >({
    mutationFn: async ({ signed }) =>
      submitDeclaration(
        {
          baseUrl: apiBaseUrl,
          declarantPrincipal: form.getValues('declarant_principal'),
        },
        signed,
      ),
  });

  async function onSubmit(values: FormValues) {
    if (!keys) {
      throw new Error('signing keys not ready');
    }
    const signed = await buildSignedRequest(keys, {
      declaration_id: randomUuid(),
      entity_id: values.entity_id,
      declarant_principal: values.declarant_principal,
      declarant_role: values.declarant_role,
      kind: values.kind,
      effective_from: values.effective_from,
      beneficial_owners: values.beneficial_owners,
    });
    await submitMutation.mutateAsync({ signed });
  }

  if (supported === false) {
    return (
      <div role="alert" className="rounded-lg border-2 border-red-700 bg-red-50 p-6">
        <h2 className="text-xl font-semibold text-red-900">
          {t('crypto.browserUnsupportedHeading')}
        </h2>
        <p className="mt-2 text-red-800">
          {t('crypto.browserUnsupportedBody')}
        </p>
      </div>
    );
  }

  if (supported === null || (supported && !keys)) {
    return (
      <div className="text-slate-600">
        {keyGenError ? (
          <span role="alert" className="text-red-800">
            {t('crypto.keyGenFailed', { message: keyGenError })}
          </span>
        ) : (
          t('crypto.preparingKey')
        )}
      </div>
    );
  }

  if (submitMutation.isSuccess) {
    return (
      <VerificationStatus
        apiBaseUrl={apiBaseUrl}
        declarantPrincipal={form.getValues('declarant_principal')}
        receipt={submitMutation.data}
      />
    );
  }

  return (
    <form
      onSubmit={form.handleSubmit(onSubmit)}
      className="space-y-6"
      noValidate
      aria-label={t('form.ariaLabel')}
    >
      <Field
        label={t('form.fields.entityId.label')}
        error={tValidationMessage(t, form.formState.errors.entity_id?.message)}
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
        error={tValidationMessage(
          t,
          form.formState.errors.declarant_principal?.message,
        )}
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
          error={tValidationMessage(
            t,
            form.formState.errors.declarant_role?.message,
          )}
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
          error={tValidationMessage(t, form.formState.errors.kind?.message)}
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
          error={tValidationMessage(
            t,
            form.formState.errors.effective_from?.message,
          )}
        >
          <input
            type="date"
            className={inputCls}
            {...form.register('effective_from')}
          />
        </Field>
      </div>

      <fieldset className="space-y-4">
        <legend className="text-lg font-semibold text-slate-900">
          {t('form.owners.legend')}
        </legend>
        <p className="text-sm text-slate-600">
          {t('form.owners.instructions')}
        </p>

        {fields.map((field, index) => (
          <OwnerRow
            key={field.id}
            index={index}
            error={
              form.formState.errors.beneficial_owners?.[index] as
                | Record<string, { message?: string }>
                | undefined
            }
            register={form.register}
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

        {form.formState.errors.beneficial_owners?.root && (
          <p role="alert" className="text-sm text-red-700">
            {tValidationMessage(
              t,
              form.formState.errors.beneficial_owners.root.message,
            )}
          </p>
        )}
        {form.formState.errors.beneficial_owners?.message && (
          <p role="alert" className="text-sm text-red-700">
            {tValidationMessage(
              t,
              form.formState.errors.beneficial_owners.message,
            )}
          </p>
        )}
      </fieldset>

      {submitMutation.isError && (
        <div
          role="alert"
          className="rounded-md border border-red-300 bg-red-50 p-4 text-sm text-red-900"
        >
          <strong>{t('form.errors.submissionFailedLabel')}</strong>{' '}
          {submitMutation.error instanceof ApiError
            ? `${submitMutation.error.kind} — ${submitMutation.error.message}`
            : submitMutation.error.message}
        </div>
      )}

      <button
        type="submit"
        disabled={submitMutation.isPending}
        className={clsx(
          'inline-flex items-center justify-center rounded-md px-6 py-3 text-base font-semibold text-white shadow-sm transition',
          'bg-recor-deep hover:bg-blue-900',
          'disabled:cursor-not-allowed disabled:bg-slate-400',
        )}
      >
        {submitMutation.isPending
          ? t('form.submit.pending')
          : t('form.submit.idle')}
      </button>
    </form>
  );
}

const inputCls =
  'block w-full rounded-md border border-slate-300 bg-white px-3 py-2 text-slate-900 shadow-sm focus:border-recor-deep focus:ring-2 focus:ring-recor-accent';

function Field({
  label,
  error,
  children,
}: {
  label: string;
  error?: string;
  children: React.ReactNode;
}) {
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

function OwnerRow({
  index,
  error,
  register,
  onRemove,
  removable,
}: {
  index: number;
  error: Record<string, { message?: string }> | undefined;
  register: ReturnType<typeof useForm<FormValues>>['register'];
  onRemove: () => void;
  removable: boolean;
}) {
  const { t } = useTranslation();
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
            {...register(`beneficial_owners.${index}.person_id`)}
          />
          {error?.person_id?.message && (
            <span role="alert" className="mt-1 block text-sm text-red-700">
              {tValidationMessage(t, error.person_id.message)}
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
            {...register(`beneficial_owners.${index}.ownership_basis_points`, {
              valueAsNumber: true,
            })}
          />
          {error?.ownership_basis_points?.message && (
            <span role="alert" className="mt-1 block text-sm text-red-700">
              {tValidationMessage(t, error.ownership_basis_points.message)}
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
            {...register(`beneficial_owners.${index}.interest_kind`)}
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
