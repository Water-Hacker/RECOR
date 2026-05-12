---
name: recor-react-app
description: React application scaffolding and conventions. Fires when a new React app or component is being created, when working in any Layer 6 application, or when frontend code patterns are being established.
---

# RÉCOR frontend conventions

All applications are React 19 + TypeScript 5.7 strict + Vite 6 + Tailwind v4.

## Application directory structure

```
applications/<app-name>/
├── CLAUDE.md
├── package.json
├── tsconfig.json
├── vite.config.ts
├── tailwind.config.ts        -- minimal; design tokens at top level
├── src/
│   ├── main.tsx              -- entry; mounts to #root
│   ├── App.tsx
│   ├── routes/               -- file-based or config routes
│   ├── components/           -- shared components within this app
│   ├── features/             -- feature modules
│   ├── hooks/                -- shared hooks
│   ├── api/                  -- generated API clients (from contracts)
│   ├── stores/               -- Zustand stores
│   ├── i18n/                 -- translations
│   ├── assets/
│   ├── styles/
│   └── service-worker.ts     -- Workbox config (offline-capable apps)
├── tests/
│   ├── unit/                 -- vitest
│   └── e2e/                  -- playwright
└── public/
```

## State management

- Server state: TanStack Query
- Client state: Zustand (one store per feature where natural)
- Form state: react-hook-form + Zod for validation

## Component patterns

- Functional components only; the React Compiler is enabled
- Props are typed explicitly; no `any`
- Server-state hooks are colocated with the feature
- Storybook stories accompany shared components

## i18n

- Three locales: fr (primary), en, pcm (Pidgin)
- Every user-facing string is translated; no English-only strings in production
- Plurals and gendered forms use ICU MessageFormat

## Offline patterns (Declarant Portal, Public Portal)

- Workbox handles caching strategies
- IndexedDB (via Dexie) for offline data
- Idempotency keys for submissions

## Testing

- Vitest for unit tests
- Testing-library for component tests
- Playwright for E2E tests including offline-mode scenarios

## When to add a new dependency

- Per Doctrine 3: search first
- Per Doctrine 12: dependencies are production-grade
- Per Doctrine 20: dependencies pass the supply-chain checks
- Trivial helpers should be written inline rather than added as a dependency

## Performance budgets

| Application | FCP | LCP | TTI | Bundle (gzipped) |
|-------------|-----|-----|-----|------------------|
| Declarant Portal (low-end Android 3G) | 1.5s | 2.5s | 3.5s | < 250 KB |
| Officer Console (desktop) | 1s | 1.5s | 2s | < 500 KB |
| Public Portal (low-end device) | 1.5s | 2.5s | 3.5s | < 200 KB |
| Investigation Workbench (desktop high-res) | 2s | 3s | 4s | < 1 MB |

Bundle budgets are CI-checked.
