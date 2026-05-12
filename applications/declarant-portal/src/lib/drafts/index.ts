/**
 * Offline-drafts persistence layer (R-PORT-2).
 *
 * Why this exists
 * ===============
 * Cameroonian network conditions are intermittent. A declarant filling
 * out a 15-minute form should not lose their work when their tower
 * drops connectivity. Drafts are stored in IndexedDB via Dexie, scoped
 * to the browser+origin, and aggressively expired after 24 hours.
 *
 * The contract surface
 * ====================
 *
 *   - `saveDraft(declarationId, formState)` — idempotent upsert keyed
 *     on `declaration_id` (the wizard's stable session UUID), so the
 *     5-second autosave loop produces exactly one row per session
 *     regardless of how many keystrokes pass.
 *   - `loadLatestDraft()` — the resume banner asks for the
 *     most-recently-modified draft on app boot.
 *   - `deleteDraft(declarationId)` — called on Discard, and after a
 *     successful submission so the wizard never offers to resume an
 *     already-filed declaration.
 *   - `expireDrafts(now)` — boot-time cleanup; deletes anything whose
 *     `last_modified_at` is older than 24 h.
 *   - `isDraftsAvailable()` — feature probe; if IndexedDB is missing
 *     (Safari private mode, hardened browsers) the wizard falls back
 *     to in-memory-only state and surfaces a one-time toast (D14
 *     fail-closed, not silent partial-save).
 *
 * D18 — no secrets, no PII bleed-through
 * ======================================
 * Drafts MUST NEVER persist:
 *
 *   - The Ed25519 private key (regenerated per session anyway).
 *   - The attestation (signature + public key + nonce).
 *   - Any auth token, OIDC bearer, X-Recor-Dev-Principal header.
 *   - The signed receipt bytes returned by the Declaration service.
 *
 * `saveDraft` walks the form state through `stripSecrets` before the
 * IndexedDB write. Any key matching the strip-list (`attestation`,
 * `receipt`, `receipt_hash_hex`, `signature_hex`, `public_key_hex`,
 * `private_key`, `bearer_token`, `auth_token`, `access_token`, …) is
 * dropped at every nesting level. A unit test in `__tests__/drafts.test.ts`
 * locks this behaviour; a regression that re-introduces persisted
 * crypto material fails CI before merge.
 */

import Dexie, { type EntityTable } from 'dexie';

/* ─── public types ────────────────────────────────────────────────── */

/**
 * The serialised form state — opaque JSON typed as `unknown` here.
 * The wizard treats it as `Partial<FormValues>` at the call site
 * (`useDraftAutosave`, `DeclarationForm`); we deliberately don't
 * import `FormValues` into this module so the persistence layer
 * remains decoupled from any one form schema.
 */
export type DraftFormState = Record<string, unknown>;

/** Row shape in the `drafts` object store. */
export interface DraftRow {
  /** Auto-incrementing primary key — implementation detail. */
  id?: number;
  /** Stable wizard-session UUID — the natural dedup key (unique index). */
  declaration_id: string;
  /** Stripped form state — never contains crypto/auth material. See D18. */
  form_state: DraftFormState;
  /** ISO-8601 of first save. */
  created_at: string;
  /** ISO-8601 of most recent save. */
  last_modified_at: string;
}

/* ─── D18 strip-list ──────────────────────────────────────────────── */

/**
 * Keys that MUST NOT survive the wire boundary into the draft store.
 *
 * The list is intentionally aggressive: it matches case-insensitively,
 * at every nesting level. A future field that legitimately uses one of
 * these names (e.g. a `signature` field of an analyst comment) would
 * need either a name change OR an explicit allowlist entry — that is a
 * deliberate friction point.
 *
 * Exported for the unit test that locks the list.
 */
export const DRAFT_SECRET_KEYS: readonly string[] = [
  'attestation',
  'receipt',
  'receipt_hash_hex',
  'receipt_url',
  'signature',
  'signature_hex',
  'public_key',
  'public_key_hex',
  'private_key',
  'private_key_hex',
  'signed_by',
  'nonce_hex',
  'bearer_token',
  'auth_token',
  'access_token',
  'id_token',
  'refresh_token',
  'authorization',
];

const SECRET_KEY_SET: ReadonlySet<string> = new Set(
  DRAFT_SECRET_KEYS.map((k) => k.toLowerCase()),
);

/**
 * Recursively drop any keys in `DRAFT_SECRET_KEYS` from the form state.
 *
 * - Plain objects are walked.
 * - Arrays are mapped (a `BeneficialOwner` that contained an attestation
 *   would have it stripped from inside the array element).
 * - Primitives pass through unchanged.
 * - `null` is preserved (form state may legitimately set fields to null).
 *
 * Returns a NEW object — never mutates the input. This keeps the
 * wizard's react-hook-form state safe from accidental modification.
 */
export function stripSecrets(value: unknown): unknown {
  if (value === null || typeof value !== 'object') return value;
  if (Array.isArray(value)) {
    return value.map((item) => stripSecrets(item));
  }
  const obj = value as Record<string, unknown>;
  const out: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(obj)) {
    if (SECRET_KEY_SET.has(k.toLowerCase())) continue;
    out[k] = stripSecrets(v);
  }
  return out;
}

/* ─── Dexie schema ────────────────────────────────────────────────── */

/**
 * Dexie database for offline drafts. Versioned at v1; future schema
 * changes use `db.version(N).upgrade(...)` so installed clients
 * migrate cleanly.
 *
 * The `declaration_id` index is UNIQUE — Dexie enforces it via the
 * `&` prefix — so concurrent saves with the same id collapse to one
 * row instead of producing duplicates.
 */
export interface DraftsDb extends Dexie {
  drafts: EntityTable<DraftRow, 'id'>;
}

export const DRAFTS_DB_NAME = 'recor.drafts';

function createDb(name: string = DRAFTS_DB_NAME): DraftsDb {
  const db = new Dexie(name) as DraftsDb;
  db.version(1).stores({
    // `++id` auto-increment primary key + UNIQUE `declaration_id`
    // index for dedup-on-save; `last_modified_at` indexed for the
    // expiration sweep + "most recent draft" lookup.
    drafts: '++id,&declaration_id,last_modified_at',
  });
  return db;
}

/* ─── feature detection ──────────────────────────────────────────── */

/**
 * Returns true if IndexedDB is available in this runtime. The Dexie
 * constructor itself throws on hardened browsers (Safari private,
 * Firefox `dom.indexedDB.enabled=false`), so we feature-detect once
 * and cache the result.
 *
 * D14 fail-closed: callers that need to know whether the drafts
 * feature is live should branch on this result and surface a banner
 * to the declarant instead of silently dropping their work.
 */
export function isDraftsAvailable(): boolean {
  try {
    return typeof globalThis !== 'undefined' && 'indexedDB' in globalThis;
  } catch {
    return false;
  }
}

/* ─── singleton handle ───────────────────────────────────────────── */

let dbHandle: DraftsDb | null = null;

/**
 * Lazy singleton accessor. Test suites that need isolation can call
 * `__resetDraftsForTest()` after each test (see helper at file end).
 */
function getDb(): DraftsDb {
  if (!dbHandle) {
    dbHandle = createDb();
  }
  return dbHandle;
}

/* ─── public API ─────────────────────────────────────────────────── */

/**
 * Upsert a draft for the given declaration id. Idempotent on
 * `declaration_id`: the autosave loop can call this every 5 seconds
 * without exploding the DB.
 *
 * Crypto / auth fields are stripped via `stripSecrets` BEFORE the
 * IndexedDB write — see D18 strip-list at the top of this module.
 *
 * Returns the primary key of the row that ended up in the store.
 *
 * @throws if IndexedDB is unavailable; callers MUST check
 *   `isDraftsAvailable()` first.
 */
export async function saveDraft(
  declarationId: string,
  formState: DraftFormState,
  options: { now?: Date } = {},
): Promise<number> {
  if (typeof declarationId !== 'string' || declarationId.length === 0) {
    throw new Error('saveDraft: declarationId is required');
  }
  const stripped = stripSecrets(formState) as DraftFormState;
  const now = (options.now ?? new Date()).toISOString();

  const db = getDb();
  return db.transaction('rw', db.drafts, async () => {
    const existing = await db.drafts
      .where('declaration_id')
      .equals(declarationId)
      .first();
    if (existing) {
      // Preserve the row's primary key + created_at so consumers can
      // distinguish "first draft" from "still editing" if we ever
      // surface that. The unique-index constraint means there is at
      // most one row to update.
      await db.drafts.update(existing.id!, {
        form_state: stripped,
        last_modified_at: now,
      });
      return existing.id!;
    }
    const id = await db.drafts.add({
      declaration_id: declarationId,
      form_state: stripped,
      created_at: now,
      last_modified_at: now,
    });
    return id as number;
  });
}

/**
 * Load a specific draft by declaration id. Returns `undefined` if
 * none exists (the wizard treats absence as "no draft to resume").
 */
export async function loadDraft(
  declarationId: string,
): Promise<DraftRow | undefined> {
  const db = getDb();
  return db.drafts.where('declaration_id').equals(declarationId).first();
}

/**
 * Resume-banner helper: return the most-recently-modified draft, or
 * `undefined` if no drafts exist.
 *
 * The wizard's resume banner uses this on app boot; if a fresh draft
 * exists (< 24 h old after the expiration sweep), the user sees the
 * "Resume your saved draft?" prompt.
 */
export async function loadLatestDraft(): Promise<DraftRow | undefined> {
  const db = getDb();
  return db.drafts.orderBy('last_modified_at').reverse().first();
}

/**
 * Delete a draft by declaration id. Idempotent: deleting a
 * non-existent draft is not an error.
 */
export async function deleteDraft(declarationId: string): Promise<void> {
  const db = getDb();
  await db.drafts.where('declaration_id').equals(declarationId).delete();
}

/**
 * Boot-time cleanup. Deletes every draft whose `last_modified_at` is
 * older than `now - maxAgeMs` (default 24 hours, the brief's
 * expiration window).
 *
 * Returns the number of rows deleted, for observability / testing.
 */
export async function expireDrafts(options: {
  now?: Date;
  maxAgeMs?: number;
} = {}): Promise<number> {
  const now = options.now ?? new Date();
  const maxAgeMs = options.maxAgeMs ?? 24 * 60 * 60 * 1000;
  const cutoffIso = new Date(now.getTime() - maxAgeMs).toISOString();

  const db = getDb();
  // `below(cutoffIso)` is half-open: strictly less than the cutoff,
  // so a draft saved exactly at the boundary survives.
  return db.drafts.where('last_modified_at').below(cutoffIso).delete();
}

/* ─── testing helper ─────────────────────────────────────────────── */

/**
 * Resets the singleton + clears the underlying IndexedDB store. Used
 * exclusively from the unit + integration test suites. Not exported
 * from the package barrel — tests import directly from this module.
 *
 * In a fresh test (with `fake-indexeddb`) calling this is harmless;
 * in a real browser this would wipe the user's drafts, which is why
 * it's namespaced with `__`.
 */
export async function __resetDraftsForTest(): Promise<void> {
  if (dbHandle) {
    try {
      await dbHandle.delete();
    } catch {
      // Already-closed handles are tolerable here; we're tearing
      // down for the next test.
    }
    dbHandle = null;
  }
}
