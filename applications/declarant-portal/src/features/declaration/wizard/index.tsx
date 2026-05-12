/**
 * 4-step declaration wizard (R-PORT-3).
 *
 * Refactors the single-page `DeclarationForm` into four progressive
 * steps without changing the data model, schema, or signing
 * contract. The shell:
 *
 *   1. Holds a single `useForm<FormValues>` instance — all step
 *      components register against it via their `form` prop, so
 *      typed values survive forward + back navigation.
 *   2. Generates the `declaration_id` and Ed25519 keypair up front,
 *      and the signing `nonce_hex` lazily on entering step 3.
 *      Step 3 (Review) previews the EXACT canonical bytes step 4
 *      will sign over (D15 cryptographic provenance — no drift
 *      between what the declarant saw and what got signed).
 *   3. Gates the Forward button on per-step validation via
 *      `form.trigger(STEP_FIELDS[step])` (D14 fail-closed).
 *   4. Keeps Back always enabled; never wipes intermediate state.
 *
 * After a successful submission the shell hands off to the existing
 * `VerificationStatus` polling view, matching the single-page form's
 * behaviour.
 */

import { useEffect, useMemo, useState } from 'react';
import { useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { useMutation } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import clsx from 'clsx';

import { type FormValues, formSchema } from '../schema';
import {
  generateKeys,
  isEd25519Supported,
  randomNonceHex,
  randomUuid,
  signPayload,
  type DeclarantKeys,
  type SignedDeclarationRequest,
} from '../../../lib/crypto';
import {
  submitDeclaration,
  type SubmitDeclarationResponse,
} from '../../../lib/api';
import { VerificationStatus } from '../VerificationStatus';
import { EntityStep } from './EntityStep';
import { OwnersStep } from './OwnersStep';
import { ReviewStep } from './ReviewStep';
import { SignStep } from './SignStep';
import { WizardStepper } from './WizardStepper';
import { FIRST_STEP, LAST_STEP, STEP_FIELDS, type WizardStep } from './types';

interface DeclarationWizardProps {
  apiBaseUrl: string;
}

export function DeclarationWizard({ apiBaseUrl }: DeclarationWizardProps) {
  const { t } = useTranslation();
  const [keys, setKeys] = useState<DeclarantKeys | null>(null);
  const [supported, setSupported] = useState<boolean | null>(null);
  const [keyGenError, setKeyGenError] = useState<string | null>(null);
  const [step, setStep] = useState<WizardStep>(FIRST_STEP);
  const [stepValidating, setStepValidating] = useState(false);

  // Stable identifiers for this wizard session. Generated once so the
  // canonical bytes the user reviews on step 3 are byte-identical to
  // what step 4 actually signs (D15).
  const declarationId = useMemo(() => randomUuid(), []);
  const [nonceHex, setNonceHex] = useState<string | null>(null);

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

  /**
   * Validate just the fields the active step owns, then advance.
   * Step 3 (Review) and Step 4 (Sign) own no inputs, so trigger
   * over an empty list short-circuits to "valid" — the gate has
   * already fired on steps 1 + 2.
   */
  async function handleForward(): Promise<void> {
    setStepValidating(true);
    try {
      const fields = STEP_FIELDS[step];
      const ok = fields.length === 0 ? true : await form.trigger([...fields]);
      if (!ok) return;
      const next = (step + 1) as WizardStep;
      if (next > LAST_STEP) return;
      // Lazily mint the nonce when first entering Review so it is
      // stable for the signature in step 4. Re-entering step 3 from
      // step 4 keeps the same nonce.
      if (next === 3 && nonceHex === null) {
        setNonceHex(randomNonceHex());
      }
      setStep(next);
    } finally {
      setStepValidating(false);
    }
  }

  function handleBack(): void {
    if (step === FIRST_STEP) return;
    setStep((s) => Math.max(FIRST_STEP, s - 1) as WizardStep);
  }

  async function handleSignAndSubmit(): Promise<void> {
    if (!keys || nonceHex === null) {
      throw new Error('signing keys or nonce not ready');
    }
    const values = form.getValues();
    const signed = await buildWizardSignedRequest(keys, values, {
      declaration_id: declarationId,
      nonce_hex: nonceHex,
    });
    await submitMutation.mutateAsync({ signed });
  }

  /* ─── early-exit branches (unchanged from single-page form) ───── */

  if (supported === false) {
    return (
      <div role="alert" className="rounded-lg border-2 border-red-700 bg-red-50 p-6">
        <h2 className="text-xl font-semibold text-red-900">
          {t('crypto.browserUnsupportedHeading')}
        </h2>
        <p className="mt-2 text-red-800">{t('crypto.browserUnsupportedBody')}</p>
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

  /* ─── wizard body ─────────────────────────────────────────────── */

  // TypeScript can't carry the `keys` narrowing across the JSX
  // boundary; defensively assert here. The early-exit branches
  // above guarantee `keys` is non-null on this code path.
  if (!keys) {
    // unreachable in practice — early-exit covered it — but keeps
    // the narrowing explicit for the rest of the function.
    return null;
  }
  const readyKeys = keys;

  // Effective nonce for the review/sign steps. Steps 1 + 2 don't
  // need it; steps 3 + 4 always render after `handleForward` minted
  // it, but we coalesce defensively to a zero-nonce so a render
  // can't crash on a race (the gate logic still prevents submit).
  const effectiveNonce = nonceHex ?? '0'.repeat(32);

  return (
    <div className="space-y-6">
      <WizardStepper current={step} />

      <form
        // The wizard never submits via native form submit — Forward and
        // Sign use explicit buttons. `noValidate` keeps the browser
        // out of the validation loop; D14 fail-closed gating happens
        // in JS only.
        noValidate
        onSubmit={(e) => e.preventDefault()}
        aria-label={t('form.ariaLabel')}
        data-testid="wizard-form"
        className="space-y-6"
      >
        {step === 1 && <EntityStep form={form} />}
        {step === 2 && <OwnersStep form={form} />}
        {step === 3 && (
          <ReviewStep
            form={form}
            declarationId={declarationId}
            nonceHex={effectiveNonce}
          />
        )}
        {step === 4 && (
          <SignStep
            onSubmit={() => {
              void handleSignAndSubmit();
            }}
            pending={submitMutation.isPending}
            error={submitMutation.isError ? submitMutation.error : null}
            publicKeyHex={readyKeys.publicKeyHex}
          />
        )}

        <WizardNavButtons
          step={step}
          onBack={handleBack}
          onForward={() => {
            void handleForward();
          }}
          validating={stepValidating}
        />
      </form>
    </div>
  );
}

interface WizardNavButtonsProps {
  step: WizardStep;
  onBack: () => void;
  onForward: () => void;
  validating: boolean;
}

function WizardNavButtons({
  step,
  onBack,
  onForward,
  validating,
}: WizardNavButtonsProps) {
  const { t } = useTranslation();
  const onLastStep = step === LAST_STEP;
  return (
    <div className="flex items-center justify-between gap-4 border-t border-slate-200 pt-4">
      <button
        type="button"
        onClick={onBack}
        disabled={step === FIRST_STEP}
        data-testid="wizard-back"
        className={clsx(
          'rounded-md border border-slate-300 px-4 py-2 text-sm font-medium text-slate-700 transition',
          'hover:bg-slate-100',
          'disabled:cursor-not-allowed disabled:border-slate-200 disabled:text-slate-400',
        )}
      >
        {t('wizard.nav.back')}
      </button>
      {!onLastStep && (
        <button
          type="button"
          onClick={onForward}
          disabled={validating}
          data-testid="wizard-forward"
          className={clsx(
            'inline-flex items-center justify-center rounded-md px-5 py-2 text-sm font-semibold text-white shadow-sm transition',
            'bg-recor-deep hover:bg-blue-900',
            'disabled:cursor-not-allowed disabled:bg-slate-400',
          )}
        >
          {validating ? t('wizard.nav.validating') : t('wizard.nav.forward')}
        </button>
      )}
    </div>
  );
}

/**
 * Sign over the canonical bytes the user reviewed on step 3.
 *
 * Step 3 mints `declaration_id` + `nonce_hex` once and holds them
 * stable; this helper feeds those same values into `signPayload`
 * (which itself funnels through `canonicalPayloadBytes`) so the
 * bytes the user previewed are byte-identical to what gets signed
 * — D15 cryptographic provenance, with `crypto.ts` as the single
 * source of truth for the byte-level canonicalisation.
 */
async function buildWizardSignedRequest(
  keys: DeclarantKeys,
  values: FormValues,
  fixed: { declaration_id: string; nonce_hex: string },
): Promise<SignedDeclarationRequest> {
  const signatureHex = await signPayload(keys, {
    declaration_id: fixed.declaration_id,
    entity_id: values.entity_id,
    declarant_principal: values.declarant_principal,
    declarant_role: values.declarant_role,
    kind: values.kind,
    effective_from: values.effective_from,
    beneficial_owners: values.beneficial_owners,
    nonce_hex: fixed.nonce_hex,
  });
  return {
    declaration_id: fixed.declaration_id,
    entity_id: values.entity_id,
    declarant_role: values.declarant_role,
    kind: values.kind,
    effective_from: values.effective_from,
    beneficial_owners: values.beneficial_owners,
    attestation: {
      signed_by: values.declarant_principal,
      signature_algorithm: 'ed25519',
      signature_hex: signatureHex,
      public_key_hex: keys.publicKeyHex,
      nonce_hex: fixed.nonce_hex,
    },
  };
}
