/**
 * Unit tests for the crypto module.
 *
 * The CRITICAL property: the canonical payload bytes this module
 * produces in the browser must be byte-identical to what the server
 * canonicalises in Rust (services/declaration/src/api/rest.rs::
 * canonical_payload_bytes). Any drift here breaks signature
 * verification at submission. We test the JSON byte sequence
 * explicitly.
 */

import { describe, expect, it } from 'vitest';

import {
  canonicalPayloadBytes,
  hexToBytes,
  bytesToHex,
  randomNonceHex,
  type DeclarationPayload,
} from './crypto';

function fixture(): DeclarationPayload {
  return {
    declaration_id: '018f0000-0000-4000-8000-000000000001',
    entity_id: '018f0000-0000-4000-8000-000000000002',
    declarant_principal: 'spiffe://recor.cm/declarant-001',
    declarant_role: 'self',
    kind: 'incorporation',
    effective_from: '2026-01-01',
    beneficial_owners: [
      {
        person_id: '018f0000-0000-4000-8000-000000000003',
        ownership_basis_points: 6000,
        interest_kind: 'equity',
      },
      {
        person_id: '018f0000-0000-4000-8000-000000000004',
        ownership_basis_points: 4000,
        interest_kind: 'voting',
      },
    ],
    nonce_hex: '00112233445566778899aabbccddeeff',
  };
}

describe('canonicalPayloadBytes', () => {
  it('produces field-ordered no-whitespace JSON', () => {
    const bytes = canonicalPayloadBytes(fixture());
    const text = new TextDecoder().decode(bytes);

    // Expected exact form (field order matches server Rust struct).
    const expected = [
      '{"entity_id":"018f0000-0000-4000-8000-000000000002",',
      '"declarant_principal":"spiffe://recor.cm/declarant-001",',
      '"declarant_role":"self",',
      '"kind":"incorporation",',
      '"effective_from":"2026-01-01",',
      '"beneficial_owners":[',
      '{"person_id":"018f0000-0000-4000-8000-000000000003",',
      '"ownership_basis_points":6000,',
      '"interest_kind":"equity"},',
      '{"person_id":"018f0000-0000-4000-8000-000000000004",',
      '"ownership_basis_points":4000,',
      '"interest_kind":"voting"}],',
      '"nonce_hex":"00112233445566778899aabbccddeeff"}',
    ].join('');

    expect(text).toBe(expected);
  });

  it('omits declaration_id (which is not part of the signed payload)', () => {
    const bytes = canonicalPayloadBytes(fixture());
    const text = new TextDecoder().decode(bytes);
    expect(text).not.toContain('declaration_id');
  });

  it('emits ownership_basis_points as an integer not a float', () => {
    const bytes = canonicalPayloadBytes(fixture());
    const text = new TextDecoder().decode(bytes);
    expect(text).toContain('"ownership_basis_points":6000');
    expect(text).not.toContain('"ownership_basis_points":6000.0');
    expect(text).not.toContain('"ownership_basis_points":"6000"');
  });

  it('emits the date as YYYY-MM-DD string', () => {
    const bytes = canonicalPayloadBytes(fixture());
    const text = new TextDecoder().decode(bytes);
    expect(text).toContain('"effective_from":"2026-01-01"');
  });
});

describe('bytesToHex / hexToBytes', () => {
  it('round-trips arbitrary bytes', () => {
    const bytes = new Uint8Array([0x00, 0x01, 0xff, 0x7f, 0x80]);
    const hex = bytesToHex(bytes);
    expect(hex).toBe('0001ff7f80');
    const back = hexToBytes(hex);
    expect(Array.from(back)).toEqual(Array.from(bytes));
  });

  it('rejects odd-length hex', () => {
    expect(() => hexToBytes('abc')).toThrow();
  });
});

describe('randomNonceHex', () => {
  it('returns 32 hex chars (16 bytes)', () => {
    const n = randomNonceHex();
    expect(n).toMatch(/^[0-9a-f]{32}$/);
  });

  it('produces a different value each call', () => {
    const a = randomNonceHex();
    const b = randomNonceHex();
    expect(a).not.toBe(b);
  });
});
