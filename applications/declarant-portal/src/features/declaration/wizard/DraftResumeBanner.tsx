/**
 * `DraftResumeBanner` — non-modal banner offering to restore a saved
 * draft on app boot (R-PORT-2 acceptance criterion 4).
 *
 * The wizard mounts this above its form. It receives:
 *
 *   - `draft` — the latest un-submitted draft loaded from Dexie
 *     (`loadLatestDraft()`), or `null` if there's nothing to offer.
 *   - `onResume` — invoked with the draft's `form_state`; the wizard
 *     calls `form.reset(form_state)` to repopulate.
 *   - `onDiscard` — invoked when the declarant rejects the offer; the
 *     wizard deletes the draft from Dexie.
 *
 * The banner is hidden entirely when `draft` is null — there is no
 * "nothing to resume" copy because that would create noise on every
 * cold-load.
 *
 * Every visible string flows through `t('drafts.*')` per R-PORT-1.
 * The wrapping `<aside role="status">` plus the `aria-live="polite"`
 * announces the offer to screen readers without interrupting the
 * declarant's keyboard focus.
 */

import { useTranslation } from 'react-i18next';

import type { DraftRow } from '../../../lib/drafts';

export interface DraftResumeBannerProps {
  draft: DraftRow | null;
  onResume: (draft: DraftRow) => void;
  onDiscard: (draft: DraftRow) => void;
}

export function DraftResumeBanner({
  draft,
  onResume,
  onDiscard,
}: DraftResumeBannerProps) {
  const { t } = useTranslation();
  if (!draft) return null;
  return (
    <aside
      role="status"
      aria-live="polite"
      data-testid="draft-resume-banner"
      className="mb-6 flex flex-col gap-3 rounded-md border border-recor-deep bg-blue-50 p-4 text-sm text-slate-800 md:flex-row md:items-center md:justify-between"
    >
      <div>
        <p className="font-semibold text-recor-deep">
          {t('drafts.resumePrompt')}
        </p>
        <p className="mt-1 text-slate-700">
          {t('drafts.savedAt', { when: formatLocaleDateTime(draft.last_modified_at) })}
        </p>
      </div>
      <div className="flex gap-2">
        <button
          type="button"
          onClick={() => onResume(draft)}
          className="rounded-md bg-recor-deep px-4 py-2 text-sm font-semibold text-white shadow-sm hover:bg-blue-900"
          data-testid="draft-resume-button"
        >
          {t('drafts.resumeButton')}
        </button>
        <button
          type="button"
          onClick={() => onDiscard(draft)}
          className="rounded-md border border-slate-300 bg-white px-4 py-2 text-sm font-medium text-slate-700 hover:bg-slate-100"
          data-testid="draft-discard-button"
        >
          {t('drafts.discardButton')}
        </button>
      </div>
    </aside>
  );
}

/**
 * Render the saved-at ISO timestamp in the user's locale. We use the
 * browser's `Intl.DateTimeFormat` rather than i18next's number/date
 * subsystem to keep the locale JSON files free of date format strings.
 *
 * Falls back to the raw ISO string on failure — better than a blank.
 */
function formatLocaleDateTime(iso: string): string {
  try {
    const d = new Date(iso);
    if (Number.isNaN(d.getTime())) return iso;
    return d.toLocaleString(undefined, {
      dateStyle: 'short',
      timeStyle: 'short',
    });
  } catch {
    return iso;
  }
}
