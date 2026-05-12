/**
 * `useDraftAutosave` — react-hook-form ↔ Dexie autosave bridge (R-PORT-2).
 *
 * Contract
 * ========
 *
 *   - Watches the supplied react-hook-form instance via `form.watch()`.
 *   - On the FIRST dirty change, schedules a write `intervalMs` (default
 *     5_000) into the future. Subsequent changes within that window
 *     coalesce — the hook fires at MOST one Dexie write per 5 seconds.
 *   - When the form becomes clean again (e.g. after `form.reset(...)`)
 *     the pending timer is cleared; we do not write empty / unchanged
 *     state.
 *   - On unmount the pending timer is cleared; if there's an
 *     outstanding dirty change we flush it synchronously so a tab
 *     close doesn't lose the last 0–5 s of edits.
 *   - When `enabled` is false (e.g. IndexedDB unavailable per
 *     `isDraftsAvailable()`) the hook becomes a no-op. The wizard
 *     surfaces the disabled state via a banner; we don't fall back to
 *     localStorage or any other secret-leakable store.
 *
 * D18 — crypto / auth values are NEVER persisted. `saveDraft` strips
 * them at the persistence boundary; this hook is intentionally
 * agnostic about the form shape so that the strip list is the single
 * source of truth.
 */

import { useEffect, useRef } from 'react';
import type { FieldValues, UseFormReturn } from 'react-hook-form';

import {
  saveDraft,
  type DraftFormState,
} from '../../../lib/drafts';

export interface UseDraftAutosaveOptions<TValues extends FieldValues> {
  /** react-hook-form instance the wizard owns. */
  form: UseFormReturn<TValues>;
  /** Stable wizard-session declaration id; used as the Dexie dedup key. */
  declarationId: string;
  /**
   * Minimum interval between Dexie writes, in milliseconds. Defaults
   * to 5 000 per the R-PORT-2 brief.
   */
  intervalMs?: number;
  /**
   * If false, the hook becomes a no-op. The wizard sets this to false
   * when `isDraftsAvailable()` reports IndexedDB is missing — see
   * D14 fail-closed at the call site.
   */
  enabled?: boolean;
  /**
   * Test-only callback. When provided, called every time the hook
   * commits a save to Dexie (after the write resolves). Production
   * code never sets this; integration tests use it to await the
   * autosave deterministically.
   */
  onSaved?: (declarationId: string) => void;
  /**
   * Test-only error sink. When provided, called with any error
   * returned by `saveDraft`. Production code logs to the console.
   */
  onError?: (err: unknown) => void;
}

/**
 * Wire react-hook-form's dirty stream to the Dexie drafts store.
 *
 * Returns nothing — the hook is purely side-effecting. The wizard
 * does not need to observe its state; the resume banner reads from
 * Dexie directly on mount.
 */
export function useDraftAutosave<TValues extends FieldValues>(
  options: UseDraftAutosaveOptions<TValues>,
): void {
  const {
    form,
    declarationId,
    intervalMs = 5_000,
    enabled = true,
    onSaved,
    onError,
  } = options;

  // The pending-write timer handle. Held in a ref so the cleanup
  // function can cancel it without taking the timer into the
  // useEffect dependency set.
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Latest typed values + dirty flag, kept in refs so the unmount
  // flush sees the freshest snapshot without re-running the watcher
  // subscription.
  const latestValuesRef = useRef<TValues | null>(null);
  const dirtyRef = useRef<boolean>(false);

  useEffect(() => {
    if (!enabled) return undefined;

    // `watch` returns a subscription handle; `.unsubscribe()` is the
    // documented teardown.
    const subscription = form.watch((values) => {
      // `formState.isDirty` is a getter-driven proxy; touching it
      // inside the watcher avoids stale closures.
      const isDirty = form.formState.isDirty;
      latestValuesRef.current = values as TValues;
      dirtyRef.current = isDirty;

      if (!isDirty) {
        // Cleared by form.reset(...); cancel any pending write so we
        // don't persist a stale "dirty" snapshot after the reset.
        if (timerRef.current !== null) {
          clearTimeout(timerRef.current);
          timerRef.current = null;
        }
        return;
      }

      if (timerRef.current !== null) {
        // Already a pending write; coalesce — this enforces the
        // "one DB write per intervalMs" invariant.
        return;
      }

      timerRef.current = setTimeout(() => {
        timerRef.current = null;
        const snapshot = latestValuesRef.current;
        if (!snapshot || !dirtyRef.current) return;
        void saveDraft(declarationId, snapshot as DraftFormState)
          .then(() => {
            onSaved?.(declarationId);
          })
          .catch((err: unknown) => {
            if (onError) {
              onError(err);
            } else {
              // D14: surface failure to operators. The wizard does
              // not show a per-write toast — autosave failures are
              // best-effort and frequent retries would spam the user.
              // eslint-disable-next-line no-console
              console.error('[recor.drafts] autosave failed', err);
            }
          });
      }, intervalMs);
    });

    return () => {
      subscription.unsubscribe();
      if (timerRef.current !== null) {
        clearTimeout(timerRef.current);
        timerRef.current = null;
      }
      // Best-effort flush on unmount. We fire-and-forget the promise:
      // if the page is unloading, the browser may not let us await it,
      // but Dexie's write enqueues onto the IDB transaction queue
      // before the next microtask tick.
      //
      // Dexie surfaces a `DatabaseClosedError` if the test harness
      // (or the parent app) closed the DB before the unmount flush
      // landed; that race is expected and we silently swallow it.
      if (dirtyRef.current && latestValuesRef.current) {
        void saveDraft(
          declarationId,
          latestValuesRef.current as DraftFormState,
        ).catch((err: unknown) => {
          if (isDatabaseClosedError(err)) return;
          if (onError) {
            onError(err);
          } else {
            // eslint-disable-next-line no-console
            console.error('[recor.drafts] flush-on-unmount failed', err);
          }
        });
      }
    };
    // form is intentionally stable across the hook's lifetime (one
    // `useForm()` per wizard). `intervalMs`, `declarationId`,
    // `enabled`, `onSaved`, `onError` participate so callers can
    // reconfigure if they need to.
  }, [form, declarationId, intervalMs, enabled, onSaved, onError]);
}

/**
 * Narrow check for Dexie's `DatabaseClosedError`. We don't import
 * the Dexie error class directly to keep the hook decoupled from
 * the persistence implementation; matching on `name` is sufficient
 * to silence the unmount-after-close race in tests.
 */
function isDatabaseClosedError(err: unknown): boolean {
  return (
    typeof err === 'object' &&
    err !== null &&
    'name' in err &&
    (err as { name: string }).name === 'DatabaseClosedError'
  );
}
