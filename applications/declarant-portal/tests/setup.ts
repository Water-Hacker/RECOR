import '@testing-library/jest-dom';
import { ensureI18nTestInit } from './i18n-test-setup';

// Initialise i18next synchronously with the en + fr + pidgin resources
// preloaded so component renders never wait on the dynamic-import
// backend used in production (`src/i18n.ts`). Default test locale is
// `en` to preserve assertion text in existing suites.
ensureI18nTestInit();
