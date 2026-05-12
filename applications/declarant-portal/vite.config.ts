import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';
import { VitePWA } from 'vite-plugin-pwa';

/**
 * Vite configuration for the RÉCOR declarant portal.
 *
 * R-PORT-2 (Offline drafts) wires `vite-plugin-pwa` into the build:
 *
 *   - **Workbox** generates a service worker that precaches the SPA
 *     shell + hashed JS/CSS chunks so the portal loads when the
 *     declarant has lost connectivity. `registerType: 'autoUpdate'`
 *     means a new build silently updates installed clients on next
 *     navigation (no user prompt — the portal has no user-modifiable
 *     local state outside drafts, which are owned by a separate Dexie
 *     database and are not invalidated by SW updates).
 *
 *   - **navigateFallback: 'index.html'** routes every same-origin
 *     navigation through the SPA shell so deep-linking still works
 *     offline.
 *
 *   - **API calls** (POST /v1/declarations, GET /v1/declarations/:id)
 *     are deliberately NOT cached: `navigateFallbackDenylist` keeps
 *     them out of the fallback, so the wizard sees a real fetch
 *     failure when offline rather than a stale 200. Drafts handle
 *     offline editing; the wire boundary stays fail-closed (D14).
 *
 *   - **Bundle budget:** vite-plugin-pwa's runtime is the
 *     `register-sw` loader (~1 KB) plus the generated `sw.js` (~6 KB,
 *     served separately from the SPA bundle). Dexie is ~30 KB
 *     minified / ~10 KB gzipped, isolated to its own chunk so the
 *     critical-path remains under the 250 KB SLO documented in
 *     CLAUDE.md.
 */
export default defineConfig({
  plugins: [
    react(),
    tailwindcss(),
    VitePWA({
      registerType: 'autoUpdate',
      injectRegister: 'auto',
      strategies: 'generateSW',
      manifest: {
        name: 'RÉCOR — Declarant Portal',
        short_name: 'RÉCOR',
        description:
          'RÉCOR — Registry of Effective Control and Real Origin. Submit a beneficial-ownership declaration. Works offline; drafts are saved locally for 24 h.',
        theme_color: '#1e3a8a',
        background_color: '#f8fafc',
        display: 'standalone',
        start_url: '/',
        scope: '/',
        icons: [
          {
            src: 'favicon.svg',
            sizes: 'any',
            type: 'image/svg+xml',
            purpose: 'any',
          },
        ],
      },
      workbox: {
        // Precache the SPA shell + hashed assets at SW install time.
        globPatterns: ['**/*.{js,css,html,svg,woff2}'],
        // Deep links inside the SPA must serve index.html when offline.
        navigateFallback: '/index.html',
        // Never serve a cached response for declaration-service calls;
        // the wire boundary stays fail-closed (D14). Drafts are the
        // offline-editing surface, not response replay.
        navigateFallbackDenylist: [/^\/v1\//, /^\/healthz/],
        cleanupOutdatedCaches: true,
        clientsClaim: true,
        skipWaiting: true,
      },
      devOptions: {
        // Don't register the SW in `pnpm dev`; HMR + SW caching
        // interact poorly. The SW is only active in `pnpm build` /
        // `pnpm preview` and the production container.
        enabled: false,
      },
    }),
  ],
  server: {
    host: '0.0.0.0',
    port: 5173,
    strictPort: true,
  },
  preview: {
    host: '0.0.0.0',
    port: 5173,
    strictPort: true,
  },
  build: {
    outDir: 'dist',
    target: 'es2022',
    sourcemap: true,
    rollupOptions: {
      output: {
        manualChunks: {
          react: ['react', 'react-dom'],
          query: ['@tanstack/react-query'],
          forms: ['react-hook-form', 'zod', '@hookform/resolvers'],
          // Dexie lands in its own chunk so the offline-drafts feature
          // can be lazily imported at the call site if the bundle
          // budget tightens in future. Today it's eagerly imported
          // from src/lib/drafts/index.ts.
          drafts: ['dexie'],
        },
      },
    },
  },
  test: {
    environment: 'happy-dom',
    globals: true,
    setupFiles: ['./tests/setup.ts'],
    coverage: {
      provider: 'v8',
      reporter: ['text', 'html'],
    },
  },
});
