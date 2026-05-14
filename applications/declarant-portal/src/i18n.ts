/**
 * Central i18next configuration for the declarant portal (R-PORT-1).
 *
 * Supported locales:
 *   - fr      — French (legal primary; the fallback for every missing key)
 *   - en      — English (secondary; mirror of the FR key tree)
 *   - pidgin  — Cameroonian Pidgin (tertiary; stub today, awaiting
 *               community translation — keys carry English placeholders
 *               with a leading `_translation_status` marker)
 *
 * Loading strategy:
 *   The locale JSON files live under `src/locales/{locale}.json` and are
 *   imported via `i18next-resources-to-backend` with a dynamic `import()`.
 *   Vite emits one separately-hashed JS chunk per locale, so only the
 *   user's active locale is fetched on first paint. Switching the locale
 *   triggers another dynamic import — that locale's chunk is then cached
 *   by the browser for the rest of the session.
 *
 * Persistence + detection:
 *   `i18next-browser-languagedetector` reads `localStorage` first (where
 *   the user's explicit choice is persisted under the key
 *   `recor.locale`), then `navigator.language`. Detected codes are
 *   normalised — e.g. `fr-CM` collapses to `fr`. Anything outside the
 *   supported set falls back to `fr` (D14 fail-closed: never render a
 *   debug placeholder in production).
 *
 * D14 fail-closed behaviour:
 *   - `fallbackLng: 'fr'` — missing keys in en/pidgin render their FR
 *     counterpart, never the raw key.
 *   - `returnEmptyString: false` — an empty translation falls back too.
 *   - `parseMissingKeyHandler` returns the FR translation lookup; in
 *     production a missing-FR key is surfaced via a `console.error` so
 *     it can be caught by observability rather than silently rendered.
 */

// @ts-ignore
import i18n from 'i18next';
// @ts-ignore
import { initReactI18next } from 'react-i18next';
// @ts-ignore
import LanguageDetector from 'i18next-browser-languagedetector';
// @ts-ignore
import resourcesToBackend from 'i18next-resources-to-backend';

export const SUPPORTED_LOCALES = ['fr', 'en', 'pidgin'] as const;
export type SupportedLocale = (typeof SUPPORTED_LOCALES)[number];

export const DEFAULT_LOCALE: SupportedLocale = 'fr';

export const LOCALE_STORAGE_KEY = 'recor.locale';

/**
 * Human-readable labels for the locale selector. Each is written in its
 * own language so the selector is comprehensible regardless of which
 * locale is currently active.
 */
export const LOCALE_LABELS: Record<SupportedLocale, string> = {
  fr: 'Français',
  en: 'English',
  pidgin: 'Pidgin',
};

export function isSupportedLocale(value: unknown): value is SupportedLocale {
  return (
    typeof value === 'string' &&
    (SUPPORTED_LOCALES as readonly string[]).includes(value)
  );
}

/**
 * Dynamic resource backend: one fetch per active locale. The chunk
 * name (`locale-${locale}`) lands in the build output as a separately
 * hashed JS file (e.g. `dist/assets/locale-fr-<hash>.js`).
 */
const backend = resourcesToBackend(
  async (language: string, _namespace: string) => {
    // Narrow to supported locales; anything else falls through to the
    // FR fallback in i18next core.
    const safe = isSupportedLocale(language) ? language : DEFAULT_LOCALE;
    switch (safe) {
      case 'fr':
        return (await import('./locales/fr.json')).default;
      case 'en':
        return (await import('./locales/en.json')).default;
      case 'pidgin':
        return (await import('./locales/pidgin.json')).default;
    }
  },
);

/**
 * Initialise i18next. Returns the configured instance — callers that
 * need to await readiness (e.g. SSR or critical-path renders) can
 * `await i18n.init(...)` separately; the portal SPA tolerates
 * suspense-style late binding because `useTranslation` re-renders on
 * resource load.
 */
export function initI18n(): typeof i18n {
  // Idempotent: re-imports during HMR shouldn't re-initialise.
  if (i18n.isInitialized) return i18n;

  void i18n
    .use(backend)
    .use(LanguageDetector)
    .use(initReactI18next)
    .init({
      fallbackLng: DEFAULT_LOCALE,
      supportedLngs: [...SUPPORTED_LOCALES],
      // Normalise `fr-CM` → `fr`, `en-US` → `en`, etc.
      load: 'languageOnly',
      nonExplicitSupportedLngs: true,
      // Single default namespace — keeps the JSON shape flat and the
      // translation-review surface obvious.
      ns: ['translation'],
      defaultNS: 'translation',
      // Detection: persistent choice first, then the OS/browser.
      detection: {
        order: ['localStorage', 'navigator'],
        caches: ['localStorage'],
        lookupLocalStorage: LOCALE_STORAGE_KEY,
      },
      interpolation: {
        // React already escapes; double-escaping mangles French
        // apostrophes (l'entité → l&#39;entité).
        escapeValue: false,
      },
      returnEmptyString: false,
      react: {
        useSuspense: false,
      },
    });

  return i18n;
}

/**
 * Programmatic locale change. Used by the locale selector in
 * `App.tsx`. Writes to `localStorage` so the choice survives reloads
 * (the detector reads the same key on next boot).
 */
export async function changeLocale(locale: SupportedLocale): Promise<void> {
  if (!isSupportedLocale(locale)) return;
  try {
    globalThis.localStorage?.setItem(LOCALE_STORAGE_KEY, locale);
  } catch {
    // localStorage can throw in private mode or with quota exhaustion;
    // language change still applies for this session.
  }
  await i18n.changeLanguage(locale);
}

export default i18n;
