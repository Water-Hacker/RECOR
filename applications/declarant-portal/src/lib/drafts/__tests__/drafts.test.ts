/**
 * Unit tests for the Dexie drafts wrapper (R-PORT-2).
 *
 * These tests exercise:
 *   - Save → load round-trip preserves the form state byte-for-byte
 *     after the D18 strip-list pass.
 *   - `saveDraft` is idempotent on `declaration_id` — repeated calls
 *     update the SAME row instead of creating duplicates.
 *   - `deleteDraft` removes the row; subsequent `loadDraft` returns
 *     `undefined`.
 *   - `expireDrafts` removes rows whose `last_modified_at` is older
 *     than 24 h and keeps rows that are exactly 23 h old (D04: the
 *     boundary case is part of the contract).
 *   - D18 strip-list — `attestation`, `receipt`, `bearer_token`, and
 *     the rest of `DRAFT_SECRET_KEYS` NEVER survive a save, no matter
 *     how deeply nested.
 *
 * IndexedDB is provided by `fake-indexeddb` (the test imports it
 * before Dexie touches the global).
 */

// MUST be the first import so Dexie's eager IndexedDB lookup at
// module init sees the polyfill.
import 'fake-indexeddb/auto';

import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import {
  DRAFT_SECRET_KEYS,
  __resetDraftsForTest,
  deleteDraft,
  expireDrafts,
  isDraftsAvailable,
  loadDraft,
  loadLatestDraft,
  saveDraft,
  stripSecrets,
} from '../index';

beforeEach(async () => {
  await __resetDraftsForTest();
});

afterEach(async () => {
  await __resetDraftsForTest();
});

describe('isDraftsAvailable', () => {
  it('returns true under fake-indexeddb', () => {
    expect(isDraftsAvailable()).toBe(true);
  });
});

describe('saveDraft / loadDraft round-trip', () => {
  it('round-trips the form state and timestamps', async () => {
    const declarationId = 'a1111111-1111-4111-8111-111111111111';
    const state = {
      entity_id: 'e0000000-0000-4000-8000-000000000001',
      declarant_principal: 'spiffe://recor.cm/declarant-001',
      beneficial_owners: [
        {
          person_id: '01111111-1111-4111-8111-111111111111',
          ownership_basis_points: 5_000,
          interest_kind: 'equity',
        },
      ],
    };
    const fixedNow = new Date('2026-05-12T10:00:00.000Z');

    await saveDraft(declarationId, state, { now: fixedNow });
    const row = await loadDraft(declarationId);

    expect(row).toBeDefined();
    expect(row!.declaration_id).toBe(declarationId);
    expect(row!.form_state).toEqual(state);
    expect(row!.created_at).toBe('2026-05-12T10:00:00.000Z');
    expect(row!.last_modified_at).toBe('2026-05-12T10:00:00.000Z');
  });

  it('updates the existing row in place on subsequent saves (idempotent on declaration_id)', async () => {
    const declarationId = 'a2222222-2222-4222-8222-222222222222';
    const t1 = new Date('2026-05-12T10:00:00.000Z');
    const t2 = new Date('2026-05-12T10:00:05.000Z');

    await saveDraft(declarationId, { entity_id: 'first' }, { now: t1 });
    await saveDraft(
      declarationId,
      { entity_id: 'second' },
      { now: t2 },
    );

    const row = await loadDraft(declarationId);
    expect(row).toBeDefined();
    expect(row!.form_state).toEqual({ entity_id: 'second' });
    // created_at is preserved across updates.
    expect(row!.created_at).toBe(t1.toISOString());
    expect(row!.last_modified_at).toBe(t2.toISOString());
  });
});

describe('deleteDraft', () => {
  it('removes the draft and is idempotent on a missing id', async () => {
    const declarationId = 'a3333333-3333-4333-8333-333333333333';
    await saveDraft(declarationId, { entity_id: 'x' });
    expect(await loadDraft(declarationId)).toBeDefined();

    await deleteDraft(declarationId);
    expect(await loadDraft(declarationId)).toBeUndefined();

    // Second delete on the now-absent id MUST NOT throw.
    await expect(deleteDraft(declarationId)).resolves.toBeUndefined();
  });
});

describe('loadLatestDraft', () => {
  it('returns the most recently modified draft', async () => {
    const older = new Date('2026-05-12T08:00:00.000Z');
    const newer = new Date('2026-05-12T10:00:00.000Z');
    await saveDraft('a4444444-4444-4444-8444-444444444441', { v: 'older' }, { now: older });
    await saveDraft('a4444444-4444-4444-8444-444444444442', { v: 'newer' }, { now: newer });

    const latest = await loadLatestDraft();
    expect(latest).toBeDefined();
    expect(latest!.form_state).toEqual({ v: 'newer' });
  });

  it('returns undefined when no drafts exist', async () => {
    expect(await loadLatestDraft()).toBeUndefined();
  });
});

describe('expireDrafts', () => {
  it('removes drafts older than 24 h and keeps fresher ones', async () => {
    const now = new Date('2026-05-12T12:00:00.000Z');
    const tHoursAgo = (h: number) =>
      new Date(now.getTime() - h * 60 * 60 * 1000);

    // 25 h ago — stale, should be deleted.
    await saveDraft(
      'a5555555-5555-4555-8555-000000000001',
      { v: 'stale-25h' },
      { now: tHoursAgo(25) },
    );
    // 23 h ago — fresh, should be kept.
    await saveDraft(
      'a5555555-5555-4555-8555-000000000002',
      { v: 'fresh-23h' },
      { now: tHoursAgo(23) },
    );

    const removed = await expireDrafts({ now });
    expect(removed).toBe(1);

    expect(await loadDraft('a5555555-5555-4555-8555-000000000001')).toBeUndefined();
    const surviving = await loadDraft('a5555555-5555-4555-8555-000000000002');
    expect(surviving).toBeDefined();
    expect(surviving!.form_state).toEqual({ v: 'fresh-23h' });
  });

  it('honours a custom maxAgeMs override', async () => {
    const now = new Date('2026-05-12T12:00:00.000Z');
    await saveDraft(
      'a6666666-6666-4666-8666-000000000001',
      { v: '2h-ago' },
      { now: new Date(now.getTime() - 2 * 60 * 60 * 1000) },
    );

    const removed = await expireDrafts({
      now,
      maxAgeMs: 60 * 60 * 1000, // 1 h
    });
    expect(removed).toBe(1);
  });
});

describe('D18 — strip-list', () => {
  it('refuses to persist crypto / auth values inside the form state', async () => {
    const declarationId = 'a7777777-7777-4777-8777-777777777777';
    const polluted = {
      entity_id: 'e0000000-0000-4000-8000-000000000001',
      // Top-level secrets.
      attestation: {
        signed_by: 'spiffe://recor.cm/declarant-001',
        signature_hex: 'deadbeef'.repeat(16),
        public_key_hex: 'cafebabe'.repeat(8),
        nonce_hex: '00112233445566778899aabbccddeeff',
      },
      receipt: { receipt_hash_hex: 'a'.repeat(64) },
      bearer_token: 'eyJhbGciOi...',
      auth_token: 'tok_secret',
      // Nested secret inside an array element (a future correctness
      // regression that bled the attestation into the owners array
      // would still get scrubbed).
      beneficial_owners: [
        {
          person_id: '01111111-1111-4111-8111-111111111111',
          ownership_basis_points: 10_000,
          interest_kind: 'equity',
          signature_hex: 'should-not-persist',
        },
      ],
    };

    await saveDraft(declarationId, polluted);
    const row = await loadDraft(declarationId);
    expect(row).toBeDefined();

    const persisted = row!.form_state as Record<string, unknown>;
    // None of the secret-list keys appear at the top level.
    for (const key of DRAFT_SECRET_KEYS) {
      expect(Object.keys(persisted)).not.toContain(key);
    }
    // The benign field survives.
    expect(persisted.entity_id).toBe(
      'e0000000-0000-4000-8000-000000000001',
    );
    // Nested signature_hex was scrubbed without dropping the owner row.
    const owners = persisted.beneficial_owners as Array<Record<string, unknown>>;
    expect(owners).toHaveLength(1);
    expect(owners[0]!.person_id).toBe(
      '01111111-1111-4111-8111-111111111111',
    );
    expect(Object.keys(owners[0]!)).not.toContain('signature_hex');
  });

  it('does not mutate the input form state', async () => {
    const original = {
      entity_id: 'e0000000-0000-4000-8000-000000000001',
      attestation: { signature_hex: 'x' },
    };
    const copy = JSON.parse(JSON.stringify(original));

    await saveDraft('a8888888-8888-4888-8888-888888888888', original);

    // The caller's object is untouched — `attestation` still there
    // for whatever in-memory use it had.
    expect(original).toEqual(copy);
  });
});

describe('stripSecrets — pure transform', () => {
  it('handles primitives + null + nested structures', () => {
    expect(stripSecrets(null)).toBeNull();
    expect(stripSecrets(42)).toBe(42);
    expect(stripSecrets('x')).toBe('x');
    expect(stripSecrets(['a', { receipt: 'x', keep: 1 }])).toEqual([
      'a',
      { keep: 1 },
    ]);
  });

  it('matches secret keys case-insensitively', () => {
    expect(
      stripSecrets({
        Bearer_Token: 't',
        ATTESTATION: { x: 1 },
        Signature_Hex: 'sig',
        keep: 'ok',
      } as Record<string, unknown>),
    ).toEqual({
      // No secret-list key survives despite the unconventional casing.
      keep: 'ok',
    });
  });
});
