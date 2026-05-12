/**
 * Tests for the 4-step declaration wizard (R-PORT-3).
 *
 * The wizard is gated by feature-detected Ed25519 support, which
 * happy-dom does not provide. We mock `../../../lib/crypto` so the
 * wizard renders past its "browser unsupported" branch with a
 * deterministic keypair and signature; the byte-level canonical
 * form remains untouched (D15 byte-parity guard lives in
 * `src/lib/crypto.test.ts` and is intentionally NOT re-asserted
 * here).
 *
 * The mocked surface is intentionally narrow:
 *   - `isEd25519Supported` always returns true.
 *   - `generateKeys` returns dummy CryptoKey-shaped placeholders +
 *     a fixed public-key hex.
 *   - `signPayload` returns a fixed signature hex.
 *   - `randomUuid` / `randomNonceHex` are pinned so assertions
 *     about the canonical-bytes preview are deterministic.
 *   - `canonicalPayloadBytes` is RE-EXPORTED from the real module,
 *     so the bytes shown to the user are the same bytes the
 *     production code would compute (D15).
 */

import { describe, expect, it, beforeEach, afterEach, vi } from 'vitest';
import { cleanup, render, screen, waitFor, act, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

import { setTestLocale } from '../../../../../tests/i18n-test-setup';

const FIXED_DECLARATION_ID = '018f0000-0000-4000-8000-00000000aaaa';
const FIXED_NONCE_HEX = '11223344556677889900aabbccddeeff';
const FIXED_SIGNATURE_HEX = 'ab'.repeat(32);
const FIXED_PUBLIC_KEY_HEX = 'cd'.repeat(32);

vi.mock('../../../../lib/crypto', async () => {
  const actual = await vi.importActual<typeof import('../../../../lib/crypto')>(
    '../../../../lib/crypto',
  );
  return {
    ...actual,
    isEd25519Supported: vi.fn(async () => true),
    generateKeys: vi.fn(async () => ({
      privateKey: {} as CryptoKey,
      publicKey: {} as CryptoKey,
      publicKeyHex: FIXED_PUBLIC_KEY_HEX,
    })),
    signPayload: vi.fn(async () => FIXED_SIGNATURE_HEX),
    randomUuid: vi.fn(() => FIXED_DECLARATION_ID),
    randomNonceHex: vi.fn(() => FIXED_NONCE_HEX),
  };
});

import { DeclarationWizard } from '../index';
import { submitDeclaration, type SubmitDeclarationResponse } from '../../../../lib/api';

vi.mock('../../../../lib/api', async () => {
  const actual = await vi.importActual<typeof import('../../../../lib/api')>(
    '../../../../lib/api',
  );
  return {
    ...actual,
    submitDeclaration: vi.fn(),
    // Stub getDeclaration so the post-submit `VerificationStatus`
    // polling does not reach the network and pollute test stderr
    // with DNS errors. Returns a "submitted" snapshot deterministically.
    // Literals (not module-scope consts) because `vi.mock` factories
    // are hoisted above top-level bindings.
    getDeclaration: vi.fn(async () => ({
      declaration_id: '018f0000-0000-4000-8000-00000000aaaa',
      entity_id: '018f0000-0000-4000-8000-000000000001',
      declarant_principal: 'spiffe://recor.cm/declarant-001',
      state: 'submitted',
      aggregate_version: 1,
      submitted_at: '2026-05-12T01:00:00Z',
      receipt_hash_hex: 'a'.repeat(64),
      verification_state: 'pending',
    })),
  };
});

const submitDeclarationMock = vi.mocked(submitDeclaration);

const VALID_ENTITY_ID = '018f0000-0000-4000-8000-000000000001';
const VALID_PERSON_ID = '018f0000-0000-4000-8000-000000000abc';
const VALID_PERSON_ID_2 = '018f0000-0000-4000-8000-000000000def';

function renderWizard() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <DeclarationWizard apiBaseUrl="http://test" />
    </QueryClientProvider>,
  );
}

/** Drive the wizard from a clean step 1 to step N, providing valid
 *  values for every gated field along the way. Helper so the per-step
 *  assertion tests stay focused on their specific guarantee. */
async function advanceTo(step: 2 | 3 | 4, user: ReturnType<typeof userEvent.setup>) {
  // Step 1 — fill entity_id with a valid UUIDv4.
  const entityInput = screen.getByLabelText(/Entity ID/);
  await user.clear(entityInput);
  await user.type(entityInput, VALID_ENTITY_ID);
  await user.click(screen.getByTestId('wizard-forward'));
  await screen.findByTestId('wizard-step-2');
  if (step === 2) return;

  // Step 2 — default values already total 10_000bp with a placeholder
  // person_id; replace the placeholder with a valid UUID.
  const personInputs = screen.getAllByLabelText(/Person ID/);
  await user.clear(personInputs[0]!);
  await user.type(personInputs[0]!, VALID_PERSON_ID);
  await user.click(screen.getByTestId('wizard-forward'));
  await screen.findByTestId('wizard-step-3');
  if (step === 3) return;

  await user.click(screen.getByTestId('wizard-forward'));
  await screen.findByTestId('wizard-step-4');
}

describe('DeclarationWizard (R-PORT-3)', () => {
  beforeEach(async () => {
    submitDeclarationMock.mockReset();
    globalThis.localStorage?.clear();
    await act(async () => {
      await setTestLocale('en');
    });
  });

  afterEach(() => {
    // Tear down the rendered tree so the post-submit VerificationStatus
    // polling does not leak into the next test as a pending query.
    cleanup();
  });

  it('renders the stepper and starts on step 1 (Entity)', async () => {
    renderWizard();
    await screen.findByTestId('wizard-step-1');
    const stepper = screen.getByTestId('wizard-stepper');
    expect(within(stepper).getByText(/1\/4/)).toBeInTheDocument();
    expect(within(stepper).getByText(/4\/4/)).toBeInTheDocument();
    expect(screen.getByTestId('wizard-stepper-1')).toHaveAttribute(
      'aria-current',
      'step',
    );
    // Each step title appears in the stepper, translated.
    expect(within(stepper).getByText('Entity')).toBeInTheDocument();
    expect(within(stepper).getByText('Owners')).toBeInTheDocument();
    expect(within(stepper).getByText('Review')).toBeInTheDocument();
    expect(within(stepper).getByText('Sign')).toBeInTheDocument();
  });

  it('step 1 Forward refuses to advance when entity_id is empty, allows it when valid', async () => {
    const user = userEvent.setup();
    renderWizard();
    await screen.findByTestId('wizard-step-1');

    // Empty entity_id — Forward stays on step 1 and surfaces a
    // validation error.
    await user.click(screen.getByTestId('wizard-forward'));
    await waitFor(() => {
      expect(screen.getByText(/expected UUIDv4/i)).toBeInTheDocument();
    });
    expect(screen.getByTestId('wizard-step-1')).toBeInTheDocument();
    expect(screen.queryByTestId('wizard-step-2')).not.toBeInTheDocument();

    // Valid entity_id — Forward advances to step 2.
    await user.type(screen.getByLabelText(/Entity ID/), VALID_ENTITY_ID);
    await user.click(screen.getByTestId('wizard-forward'));
    await screen.findByTestId('wizard-step-2');
    expect(screen.queryByTestId('wizard-step-1')).not.toBeInTheDocument();
  });

  it('step 2 refuses to proceed when ownership basis points do not sum to 10_000', async () => {
    const user = userEvent.setup();
    renderWizard();
    await screen.findByTestId('wizard-step-1');
    await advanceTo(2, user);

    // Default state is 10_000bp on a single owner. Drop it to 5000bp
    // so the .refine on `beneficial_owners` fails.
    const bpInput = screen.getByLabelText(/Basis points/);
    await user.clear(bpInput);
    await user.type(bpInput, '5000');

    await user.click(screen.getByTestId('wizard-forward'));
    await waitFor(() => {
      expect(
        screen.getByText(/must collectively hold 100%/i),
      ).toBeInTheDocument();
    });
    expect(screen.getByTestId('wizard-step-2')).toBeInTheDocument();
    expect(screen.queryByTestId('wizard-step-3')).not.toBeInTheDocument();
  });

  it('step 2 → step 3 → back to step 2 preserves typed beneficial-owner values', async () => {
    const user = userEvent.setup();
    renderWizard();
    await screen.findByTestId('wizard-step-1');
    await advanceTo(2, user);

    // Type a valid person_id; default basis points already 10_000.
    const personInput = screen.getByLabelText(/Person ID/);
    await user.clear(personInput);
    await user.type(personInput, VALID_PERSON_ID);

    await user.click(screen.getByTestId('wizard-forward'));
    await screen.findByTestId('wizard-step-3');

    // Step 3 (Review) renders the person_id in its read-only summary.
    expect(screen.getByText(VALID_PERSON_ID)).toBeInTheDocument();

    // Go back to step 2 — the typed person_id is still in the field.
    await user.click(screen.getByTestId('wizard-back'));
    await screen.findByTestId('wizard-step-2');
    expect(
      (screen.getByLabelText(/Person ID/) as HTMLInputElement).value,
    ).toBe(VALID_PERSON_ID);
  });

  it('step 3 (Review) renders the canonical-bytes preview deterministically', async () => {
    const user = userEvent.setup();
    renderWizard();
    await screen.findByTestId('wizard-step-1');
    await advanceTo(3, user);

    // The preview block exists and contains a non-empty hex prefix +
    // a byte length matching the JSON of the canonical payload.
    const lengthEl = screen.getByTestId('wizard-canonical-bytes-length');
    const prefixEl = screen.getByTestId('wizard-canonical-bytes-prefix');
    const length = Number(lengthEl.textContent);
    expect(length).toBeGreaterThan(0);
    // The hex prefix is at most 64 hex chars (32 bytes); may be
    // followed by an ellipsis if the payload exceeds the prefix.
    expect(prefixEl.textContent).toMatch(/^[0-9a-f]+(…)?$/);
    // Canonical bytes start with the JSON opening brace + first
    // field name; the byte parity test in crypto.test.ts pins the
    // exact sequence — here we sanity-check the leading two bytes
    // are `{"` (7b 22) so a regression in the canonicalisation is
    // surfaced loudly.
    expect(prefixEl.textContent?.startsWith('7b22')).toBe(true);
  });

  it('step 4 sign-and-submit invokes submitDeclaration with the wizard-stable declaration_id + nonce', async () => {
    const user = userEvent.setup();
    submitDeclarationMock.mockResolvedValueOnce({
      declaration_id: FIXED_DECLARATION_ID,
      state: 'submitted',
      receipt_hash_hex: 'a'.repeat(64),
      submitted_at: '2026-05-12T01:00:00Z',
      receipt_url: `http://test/v1/declarations/${FIXED_DECLARATION_ID}`,
    } satisfies SubmitDeclarationResponse);

    renderWizard();
    await screen.findByTestId('wizard-step-1');
    await advanceTo(4, user);

    await user.click(screen.getByTestId('wizard-sign-submit'));

    await waitFor(() => {
      expect(submitDeclarationMock).toHaveBeenCalledTimes(1);
    });
    const [, signed] = submitDeclarationMock.mock.calls[0]!;
    expect(signed.declaration_id).toBe(FIXED_DECLARATION_ID);
    expect(signed.attestation.nonce_hex).toBe(FIXED_NONCE_HEX);
    expect(signed.attestation.signature_hex).toBe(FIXED_SIGNATURE_HEX);
    expect(signed.attestation.public_key_hex).toBe(FIXED_PUBLIC_KEY_HEX);
    expect(signed.entity_id).toBe(VALID_ENTITY_ID);
    expect(signed.beneficial_owners[0]?.person_id).toBe(VALID_PERSON_ID);
  });

  it('Back is always enabled from step 2 onward; clicking it returns to the previous step', async () => {
    const user = userEvent.setup();
    renderWizard();
    await screen.findByTestId('wizard-step-1');

    // Step 1: Back is disabled (first step).
    expect(screen.getByTestId('wizard-back')).toBeDisabled();

    await advanceTo(2, user);
    expect(screen.getByTestId('wizard-back')).not.toBeDisabled();
    await user.click(screen.getByTestId('wizard-back'));
    await screen.findByTestId('wizard-step-1');
    // The entity_id we typed before is still there.
    expect(
      (screen.getByLabelText(/Entity ID/) as HTMLInputElement).value,
    ).toBe(VALID_ENTITY_ID);
  });

  it('locale switch mid-wizard re-renders step labels live (R-PORT-1 integration)', async () => {
    renderWizard();
    await screen.findByTestId('wizard-step-1');

    // Baseline: English labels.
    expect(screen.getByTestId('wizard-forward').textContent).toMatch(/Next/);

    // Switch to French.
    await act(async () => {
      await setTestLocale('fr');
    });

    await waitFor(() => {
      expect(screen.getByTestId('wizard-forward').textContent).toMatch(
        /Suivant/,
      );
    });
    const stepper = screen.getByTestId('wizard-stepper');
    expect(within(stepper).getByText('Entité')).toBeInTheDocument();

    // Reset back to English for downstream tests in this run.
    await act(async () => {
      await setTestLocale('en');
    });
  });

  it('step 2 cannot proceed with duplicate person_id across two owners', async () => {
    const user = userEvent.setup();
    renderWizard();
    await screen.findByTestId('wizard-step-1');
    await advanceTo(2, user);

    // Set first owner to 5000bp + valid id.
    const personInputs0 = screen.getAllByLabelText(/Person ID/);
    await user.clear(personInputs0[0]!);
    await user.type(personInputs0[0]!, VALID_PERSON_ID);
    const bpInputs0 = screen.getAllByLabelText(/Basis points/);
    await user.clear(bpInputs0[0]!);
    await user.type(bpInputs0[0]!, '5000');

    // Add a second owner with the SAME person_id (duplicate) +
    // 5000bp so totals reach 10_000 — only the uniqueness refine
    // should fire.
    await user.click(screen.getByRole('button', { name: /Add another/i }));
    const personInputs1 = screen.getAllByLabelText(/Person ID/);
    await user.clear(personInputs1[1]!);
    await user.type(personInputs1[1]!, VALID_PERSON_ID);
    const bpInputs1 = screen.getAllByLabelText(/Basis points/);
    await user.clear(bpInputs1[1]!);
    await user.type(bpInputs1[1]!, '5000');

    await user.click(screen.getByTestId('wizard-forward'));
    await waitFor(() => {
      expect(
        screen.getByText(/must appear only once/i),
      ).toBeInTheDocument();
    });
    expect(screen.getByTestId('wizard-step-2')).toBeInTheDocument();
    expect(screen.queryByTestId('wizard-step-3')).not.toBeInTheDocument();

    // Fix the duplicate — second owner uses a distinct id — Forward
    // now advances.
    await user.clear(personInputs1[1]!);
    await user.type(personInputs1[1]!, VALID_PERSON_ID_2);
    await user.click(screen.getByTestId('wizard-forward'));
    await screen.findByTestId('wizard-step-3');
  });
});
