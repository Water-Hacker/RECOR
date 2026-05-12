import { useEffect, useState } from 'react';
import { useFieldArray, useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { useMutation } from '@tanstack/react-query';
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

export function DeclarationForm({ apiBaseUrl }: DeclarationFormProps) {
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
          Your browser does not support Ed25519 signing
        </h2>
        <p className="mt-2 text-red-800">
          RÉCOR requires browser-side cryptographic signing of beneficial-ownership
          declarations. Please use a recent version of Chrome (113+), Firefox (130+),
          or Safari (17.4+).
        </p>
      </div>
    );
  }

  if (supported === null || (supported && !keys)) {
    return (
      <div className="text-slate-600">
        {keyGenError ? (
          <span role="alert" className="text-red-800">
            Key generation failed: {keyGenError}
          </span>
        ) : (
          'Preparing your signing key…'
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
      aria-label="Beneficial ownership declaration form"
    >
      <Field
        label="Entity ID (UUIDv4)"
        error={form.formState.errors.entity_id?.message}
      >
        <input
          type="text"
          inputMode="text"
          autoComplete="off"
          className={inputCls}
          placeholder="018f0000-0000-4000-8000-000000000001"
          {...form.register('entity_id')}
        />
      </Field>

      <Field
        label="Your principal identifier"
        error={form.formState.errors.declarant_principal?.message}
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
          label="Role"
          error={form.formState.errors.declarant_role?.message}
        >
          <select className={inputCls} {...form.register('declarant_role')}>
            <option value="self">Self (you are the beneficial owner)</option>
            <option value="authorised_agent">Authorised agent</option>
            <option value="operator_assisted">Operator-assisted</option>
          </select>
        </Field>
        <Field label="Kind" error={form.formState.errors.kind?.message}>
          <select className={inputCls} {...form.register('kind')}>
            <option value="incorporation">Incorporation</option>
            <option value="annual_renewal">Annual renewal</option>
            <option value="change_of_control">Change of control</option>
            <option value="correction">Correction</option>
            <option value="amendment">Amendment</option>
          </select>
        </Field>
        <Field
          label="Effective from"
          error={form.formState.errors.effective_from?.message}
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
          Beneficial owners
        </legend>
        <p className="text-sm text-slate-600">
          List every natural person who ultimately controls the entity.
          Ownership must total exactly 100% (10 000 basis points).
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
          + Add another beneficial owner
        </button>

        {form.formState.errors.beneficial_owners?.root && (
          <p role="alert" className="text-sm text-red-700">
            {form.formState.errors.beneficial_owners.root.message}
          </p>
        )}
        {form.formState.errors.beneficial_owners?.message && (
          <p role="alert" className="text-sm text-red-700">
            {form.formState.errors.beneficial_owners.message}
          </p>
        )}
      </fieldset>

      {submitMutation.isError && (
        <div
          role="alert"
          className="rounded-md border border-red-300 bg-red-50 p-4 text-sm text-red-900"
        >
          <strong>Submission failed:</strong>{' '}
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
          ? 'Signing and submitting…'
          : 'Sign and submit declaration'}
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
  return (
    <div className="grid grid-cols-1 gap-3 rounded-md border border-slate-200 bg-white p-4 md:grid-cols-12">
      <div className="md:col-span-6">
        <label className="block">
          <span className="mb-1 block text-sm font-medium text-slate-800">
            Person ID (UUIDv4)
          </span>
          <input
            type="text"
            autoComplete="off"
            className={inputCls}
            placeholder="018f0000-0000-4000-8000-000000000abc"
            {...register(`beneficial_owners.${index}.person_id`)}
          />
          {error?.person_id?.message && (
            <span role="alert" className="mt-1 block text-sm text-red-700">
              {error.person_id.message}
            </span>
          )}
        </label>
      </div>
      <div className="md:col-span-3">
        <label className="block">
          <span className="mb-1 block text-sm font-medium text-slate-800">
            Basis points
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
              {error.ownership_basis_points.message}
            </span>
          )}
        </label>
      </div>
      <div className="md:col-span-2">
        <label className="block">
          <span className="mb-1 block text-sm font-medium text-slate-800">
            Interest kind
          </span>
          <select
            className={inputCls}
            {...register(`beneficial_owners.${index}.interest_kind`)}
          >
            <option value="equity">Equity</option>
            <option value="voting">Voting</option>
            <option value="family_proxy">Family proxy</option>
            <option value="contractual">Contractual</option>
            <option value="other">Other</option>
          </select>
        </label>
      </div>
      <div className="flex items-end md:col-span-1">
        {removable && (
          <button
            type="button"
            onClick={onRemove}
            aria-label={`Remove owner ${index + 1}`}
            className="rounded-md border border-slate-300 px-3 py-2 text-sm text-slate-700 hover:bg-slate-100"
          >
            Remove
          </button>
        )}
      </div>
    </div>
  );
}

