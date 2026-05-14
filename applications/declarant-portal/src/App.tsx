// @ts-ignore
import { useTranslation } from 'react-i18next';

import { DeclarationForm } from './features/declaration/DeclarationForm';
import {
  LOCALE_LABELS,
  SUPPORTED_LOCALES,
  changeLocale,
  isSupportedLocale,
  type SupportedLocale,
} from './i18n';

const API_BASE_URL =
  (import.meta.env.VITE_DECLARATION_API_URL as string | undefined) ??
  'http://localhost:8080';

export function App() {
  const { t, i18n } = useTranslation();

  return (
    <div className="min-h-screen">
      <header className="bg-recor-deep text-white">
        <div className="mx-auto flex max-w-4xl items-start justify-between gap-4 px-4 py-8">
          <div>
            <h1 className="text-3xl font-semibold">{t('header.title')}</h1>
            <p className="mt-1 text-sm text-blue-100">
              {t('header.tagline')}
            </p>
          </div>
          <LocaleSelector
            current={
              isSupportedLocale(i18n.resolvedLanguage)
                ? i18n.resolvedLanguage
                : isSupportedLocale(i18n.language)
                  ? i18n.language
                  : 'fr'
            }
          />
        </div>
      </header>

      <main className="mx-auto max-w-4xl px-4 py-8">
        <section className="space-y-2">
          <h2 className="text-2xl font-semibold text-slate-900">
            {t('intro.heading')}
          </h2>
          <p className="text-slate-700">{t('intro.body')}</p>
        </section>

        <section className="mt-8 rounded-lg bg-white p-6 shadow-sm ring-1 ring-slate-200">
          <DeclarationForm apiBaseUrl={API_BASE_URL} />
        </section>

        <footer className="mt-12 text-center text-xs text-slate-500">
          <p>{t('footer.version')}</p>
          <p className="mt-1">{t('footer.encryption')}</p>
        </footer>
      </main>
    </div>
  );
}

interface LocaleSelectorProps {
  current: SupportedLocale;
}

/**
 * Header-positioned locale switcher. Persists the explicit choice to
 * `localStorage` via `changeLocale` so it survives reloads (the
 * i18next detector reads the same key on next boot).
 *
 * Rendered as a native `<select>` for accessibility — screen readers,
 * keyboard navigation, and mobile native pickers all work without
 * extra ARIA wiring.
 */
function LocaleSelector({ current }: LocaleSelectorProps) {
  const { t } = useTranslation();
  return (
    <label className="flex items-center gap-2 text-sm">
      <span className="sr-only">{t('header.languageSelectorAria')}</span>
      <span aria-hidden="true" className="text-blue-100">
        {t('header.languageSelectorLabel')}
      </span>
      <select
        aria-label={t('header.languageSelectorAria')}
        value={current}
        onChange={(e) => {
          const next = e.target.value;
          if (isSupportedLocale(next)) {
            void changeLocale(next);
          }
        }}
        className="rounded-md border border-blue-300/40 bg-white/10 px-2 py-1 text-sm text-white focus:border-white focus:outline-none focus:ring-2 focus:ring-white/40"
        data-testid="locale-selector"
      >
        {SUPPORTED_LOCALES.map((loc) => (
          <option key={loc} value={loc} className="text-slate-900">
            {LOCALE_LABELS[loc]}
          </option>
        ))}
      </select>
    </label>
  );
}
