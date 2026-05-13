// ESLint flat-config for the declarant portal.
//
// R-PORT-5: this config enforces the jsx-a11y rule-set on the SPA
// source. The wider TS / React rules are left at the recommended
// defaults so a future strict-lint pass can dial them up without
// touching the a11y gate.

import js from '@eslint/js';
import tseslint from 'typescript-eslint';
import reactHooks from 'eslint-plugin-react-hooks';
import reactRefresh from 'eslint-plugin-react-refresh';
import jsxA11y from 'eslint-plugin-jsx-a11y';

export default tseslint.config(
  {
    // Hard exclude generated + vendored content.
    ignores: [
      'dist/**',
      'node_modules/**',
      'src/generated/**',
      'tests/e2e/**',
      'playwright.config.ts',
      'vite.config.ts',
      'eslint.config.js',
    ],
  },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  {
    files: ['src/**/*.{ts,tsx}'],
    plugins: {
      'react-hooks': reactHooks,
      'react-refresh': reactRefresh,
      'jsx-a11y': jsxA11y,
    },
    languageOptions: {
      ecmaVersion: 'latest',
      sourceType: 'module',
      parserOptions: {
        ecmaFeatures: { jsx: true },
      },
    },
    rules: {
      // jsx-a11y recommended rule-set — load-bearing for R-PORT-5.
      ...jsxA11y.flatConfigs.recommended.rules,

      // React Hooks: standard.
      'react-hooks/rules-of-hooks': 'error',
      'react-hooks/exhaustive-deps': 'warn',

      // TS: leave noise down for the v1 audit pass; the a11y bar is
      // the load-bearing one.
      '@typescript-eslint/no-unused-vars': [
        'warn',
        { argsIgnorePattern: '^_', varsIgnorePattern: '^_' },
      ],
      '@typescript-eslint/no-explicit-any': 'warn',
    },
  },
);
