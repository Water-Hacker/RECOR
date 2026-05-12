import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { App } from './App';
import './styles/index.css';
// i18next is initialised at boot so the language detector resolves
// the active locale (localStorage → navigator.language → 'fr')
// before first paint. The locale's translation JSON itself is
// dynamically imported — see `src/i18n.ts` for the per-locale
// code-splitting strategy.
import { initI18n } from './i18n';

initI18n();

const rootEl = document.getElementById('root');
if (!rootEl) {
  throw new Error('Root element #root not found in document');
}

const queryClient = new QueryClient({
  defaultOptions: {
    queries: { retry: 1, refetchOnWindowFocus: false },
    mutations: { retry: 0 },
  },
});

createRoot(rootEl).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <App />
    </QueryClientProvider>
  </StrictMode>,
);
