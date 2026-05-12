/**
 * Tests for the Declaration service client.
 *
 * The schema tests are the contract — they assert the portal's view
 * of the server response. If the server's wire format changes in a
 * non-backwards-compatible way, these tests fail. The
 * `isTerminalVerificationState` helper is the contract for when the
 * UI stops polling.
 *
 * The `submitDeclaration round-trip` test exercises the generated-type
 * boundary: a happy-path 201 body parses cleanly through the runtime
 * Zod schema and the parsed value is assignable to the generated
 * `SubmitDeclarationResponse` (the latter is enforced at compile time
 * via the `satisfies` in `api.ts`; the runtime test seals the contract
 * end-to-end).
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  ApiError,
  isTerminalVerificationState,
  submitDeclaration,
  type ApiConfig,
  type SubmitDeclarationResponse,
} from './api';
import type { SignedDeclarationRequest } from './crypto';
import type { components } from '../generated/openapi';

describe('isTerminalVerificationState', () => {
  it('returns true for accepted', () => {
    expect(isTerminalVerificationState('accepted')).toBe(true);
  });

  it('returns true for rejected', () => {
    expect(isTerminalVerificationState('rejected')).toBe(true);
  });

  it('returns false for in_verification (yellow lane awaits analyst)', () => {
    expect(isTerminalVerificationState('in_verification')).toBe(false);
  });

  it('returns false for pending (engine has not run yet)', () => {
    expect(isTerminalVerificationState('pending')).toBe(false);
  });

  it('returns false for not_verified (initial column default)', () => {
    expect(isTerminalVerificationState('not_verified')).toBe(false);
  });

  it('returns false for unknown states (defensive)', () => {
    expect(isTerminalVerificationState('on_fire')).toBe(false);
    expect(isTerminalVerificationState('')).toBe(false);
  });
});

/**
 * Generated-type alignment round-trip.
 *
 * Builds a happy-path response body that conforms to the generated
 * `SubmitDeclarationResponse` schema, sends it through the wrapper,
 * and asserts the parsed value preserves every field. If the
 * generated type or runtime Zod schema drifts away from one another,
 * either the compile-time `satisfies` in `api.ts` or this runtime
 * assertion will fail.
 */
describe('submitDeclaration generated-type round-trip', () => {
  const config: ApiConfig = {
    baseUrl: 'http://test.local',
    declarantPrincipal: 'spiffe://recor.cm/test',
  };

  // A minimal SignedDeclarationRequest — the body content isn't
  // validated by the wrapper before send (the server enforces shape);
  // any object that JSON-stringifies cleanly will do.
  const signed: SignedDeclarationRequest = {
    declaration_id: '018f0000-0000-4000-8000-000000000001',
    entity_id: '018f0000-0000-4000-8000-000000000002',
    declarant_role: 'self',
    kind: 'incorporation',
    effective_from: '2026-01-01',
    beneficial_owners: [
      {
        person_id: '018f0000-0000-4000-8000-000000000003',
        ownership_basis_points: 10000,
        interest_kind: 'equity',
      },
    ],
    attestation: {
      signed_by: 'spiffe://recor.cm/test',
      signature_algorithm: 'ed25519',
      signature_hex: 'ab'.repeat(32),
      public_key_hex: 'cd'.repeat(32),
      nonce_hex: '00112233445566778899aabbccddeeff',
    },
  };

  beforeEach(() => {
    vi.useRealTimers();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it('parses a generated-shape 201 response without loss', async () => {
    // Body explicitly typed as the generated DTO — if the field set
    // here drifts from the spec, typecheck fails before this test
    // runs.
    const body: components['schemas']['SubmitDeclarationResponse'] = {
      declaration_id: '018f0000-0000-4000-8000-000000000001',
      state: 'submitted',
      receipt_hash_hex: 'a'.repeat(64),
      submitted_at: '2026-05-12T01:00:00Z',
      receipt_url:
        'https://recor.cm/v1/declarations/018f0000-0000-4000-8000-000000000001',
    };

    vi.stubGlobal(
      'fetch',
      vi.fn(async () =>
        new Response(JSON.stringify(body), {
          status: 201,
          headers: { 'Content-Type': 'application/json' },
        }),
      ),
    );

    const result = await submitDeclaration(config, signed);

    // The wrapper's return type IS the generated DTO; the assignment
    // below is a compile-time check that runtime parse output remains
    // assignable to the generated wire type. If a future Zod widening
    // breaks that contract, tsc fails before the runtime assertion.
    const _alignment: SubmitDeclarationResponse = result;
    void _alignment;

    expect(result).toEqual(body);
  });

  it('throws ApiError on a structured error body', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () =>
        new Response(
          JSON.stringify({
            error: { kind: 'bad_attestation', message: 'signature mismatch' },
          }),
          { status: 401, headers: { 'Content-Type': 'application/json' } },
        ),
      ),
    );

    await expect(submitDeclaration(config, signed)).rejects.toMatchObject({
      name: 'ApiError',
      status: 401,
      kind: 'bad_attestation',
    });
  });

  it('throws ApiError(http_error) on an unparseable error body', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () =>
        new Response('upstream collapsed', {
          status: 502,
          headers: { 'Content-Type': 'text/plain' },
        }),
      ),
    );

    const err = await submitDeclaration(config, signed).catch((e) => e);
    expect(err).toBeInstanceOf(ApiError);
    expect((err as ApiError).status).toBe(502);
    expect((err as ApiError).kind).toBe('http_error');
  });
});
