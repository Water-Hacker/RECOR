/**
 * Test-time i18next bootstrap.
 *
 * Unlike production (`src/i18n.ts`), tests can't tolerate the
 * async dynamic-import resource backend — vitest renders
 * synchronously and the components need translations available on
 * first paint. We therefore preload the en + fr resources from the
 * checked-in JSON, initialise i18next synchronously, and expose
 * `setTestLocale()` for tests that exercise locale switching.
 *
 * Default test locale is `en` so existing tests that assert on
 * English strings keep passing without per-test boilerplate.
 */

import i18n from 'i18next';
import { initReactI18next } from 'react-i18next';

import enResources from '../src/locales/en.json';
import frResources from '../src/locales/fr.json';
import pidginResources from '../src/locales/pidgin.json';

let initialised = false;

export function ensureI18nTestInit(): void {
  if (initialised) return;
  initialised = true;
  void i18n.use(initReactI18next).init({
    lng: 'en',
    fallbackLng: 'fr',
    supportedLngs: ['fr', 'en', 'pidgin'],
    resources: {
      en: { translation: enResources },
      fr: { translation: frResources },
      pidgin: { translation: pidginResources },
    },
    ns: ['translation'],
    defaultNS: 'translation',
    interpolation: { escapeValue: false },
    returnEmptyString: false,
    react: { useSuspense: false },
  });
}

export async function setTestLocale(
  locale: 'fr' | 'en' | 'pidgin',
): Promise<void> {
  ensureI18nTestInit();
  await i18n.changeLanguage(locale);
}
