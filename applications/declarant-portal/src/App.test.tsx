/**
 * Tests for the App shell and the R-PORT-1 locale selector.
 *
 * The DeclarationForm under happy-dom will fall through to its
 * "browser unsupported" branch (no Ed25519 in happy-dom's Web Crypto
 * stub) — that's fine for our purposes: those branch strings are also
 * translated and exercise the same `t()` plumbing.
 */

import { describe, expect, it, beforeEach } from 'vitest';
import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

import { App } from './App';
import { setTestLocale } from '../tests/i18n-test-setup';
import { LOCALE_STORAGE_KEY } from './i18n';

function renderApp() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <App />
    </QueryClientProvider>,
  );
}

describe('App (i18n + locale selector — R-PORT-1)', () => {
  beforeEach(async () => {
    globalThis.localStorage?.clear();
    // Reset to English for deterministic baseline assertions.
    await act(async () => {
      await setTestLocale('en');
    });
  });

  it('renders the English heading and tagline by default', async () => {
    renderApp();
    expect(
      screen.getByRole('heading', {
        level: 2,
        name: 'File a beneficial ownership declaration',
      }),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/National Beneficial Ownership Registry of Cameroon/),
    ).toBeInTheDocument();
  });

  it('exposes a locale selector with fr / en / pidgin options', () => {
    renderApp();
    const select = screen.getByTestId('locale-selector') as HTMLSelectElement;
    const values = Array.from(select.options).map((o) => o.value);
    expect(values).toEqual(['fr', 'en', 'pidgin']);
  });

  it('switching the locale changes the rendered strings and persists to localStorage', async () => {
    const user = userEvent.setup();
    renderApp();

    // Baseline: English heading.
    expect(
      screen.getByRole('heading', {
        level: 2,
        name: 'File a beneficial ownership declaration',
      }),
    ).toBeInTheDocument();

    // Switch to French.
    const select = screen.getByTestId('locale-selector') as HTMLSelectElement;
    await user.selectOptions(select, 'fr');

    await waitFor(() => {
      expect(
        screen.getByRole('heading', {
          level: 2,
          name: 'Déposer une déclaration de bénéficiaire effectif',
        }),
      ).toBeInTheDocument();
    });

    // English heading is now gone — confirms full re-render, not
    // accidental coexistence.
    expect(
      screen.queryByRole('heading', {
        level: 2,
        name: 'File a beneficial ownership declaration',
      }),
    ).not.toBeInTheDocument();

    // localStorage persisted the explicit choice (D14 fail-closed:
    // the legal default `fr` is also what survives a missing pref).
    expect(globalThis.localStorage?.getItem(LOCALE_STORAGE_KEY)).toBe('fr');

    // And the footer copy also translated, confirming the switch is
    // global, not scoped to the heading.
    expect(
      screen.getByText(/Portail Déclarant RÉCOR/),
    ).toBeInTheDocument();
  });
});
