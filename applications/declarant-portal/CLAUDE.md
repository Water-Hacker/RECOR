# Application: recor-declarant-portal
# Layer: 6 (Architecture V4 P17 § Declarant Portal)
# Owner: @recor/frontend-team @recor/declarant-experience

## What this application does

Web UI for declarants. Generates an Ed25519 keypair in the browser,
constructs the canonical form of a declaration, signs it, posts to the
Declaration service, displays the cryptographic receipt.

## Language and toolchain

- TypeScript 5.7 strict (no `any`, no implicit anything)
- React 19 (functional components only; React Compiler-friendly)
- Vite 6 build, Tailwind v4
- pnpm 9.12.3 (matches V3 P12 toolchain pin)
- Build via Docker: node:22-bookworm → nginx:1.27-alpine

## Architecture

- State: TanStack Query for server state; react-hook-form for form state
- Validation: Zod schemas (shared between client validation and the form resolver)
- Crypto: Web Crypto API native, no third-party crypto library
- Routing: single-route v1; multi-step wizard is `R-PORT-3`

## SLOs

| Metric | Budget |
|---|---|
| FCP (low-end Android 3G) | < 1.5s |
| LCP (low-end) | < 2.5s |
| TTI | < 3.5s |
| Bundle (gzipped) | < 250 KB total; v1 ships at ~95 KB across 4 chunks |
| API submit p99 (declaration service round-trip) | < 1.5s |

## Doctrines that apply with special weight here

- **D01 completeness** — every form path produces either a valid signed submission OR a clear error message; no silent failures
- **D04 tests** — every cryptographic primitive has unit tests asserting byte-exact output
- **D14 fail-closed** — unsupported browsers see an error block, not a degraded form
- **D15 cryptographic provenance** — the receipt the declarant sees is a real BLAKE3 hash; print it and verify years later
- **D17 zero trust** — declarant principal is sourced from auth (dev: header; prod: OIDC); never trusted from form input
- **D18 no secrets** — Ed25519 private key stays in browser memory only; never sent over the wire

## The canonical-form parity rule

`src/lib/crypto.ts:canonicalPayloadBytes` MUST produce bytes
byte-identical to what `services/declaration/src/api/rest.rs:
canonical_payload_bytes` produces.

Field order is fixed. JSON serialisation is fixed (no whitespace).
Date format is fixed (ISO 8601 YYYY-MM-DD). Numeric basis points are
fixed (integer, not float, not string). The unit tests in
`crypto.test.ts` lock the expected byte sequence; any change to the
canonical form is a coordinated breaking change requiring both
client and server to update simultaneously.

## Security headers (OPS-3)

The portal nginx emits a production-grade security header set on every
response. Source of truth: `nginx.conf.template` plus
`security-headers.conf.template`, rendered at container startup by
`docker-entrypoint.sh` via `envsubst`. Headers are reasserted in every
`location` block (nginx `add_header` does not inherit when a child
block adds its own — see the comment in `nginx.conf.template`).

| Header | Value | Rationale |
|---|---|---|
| `Content-Security-Policy` | see below | Closes the XSS-and-friends class top to bottom; templated `connect-src` lets us pin the API origin without rebuilding the image |
| `Strict-Transport-Security` | `max-age=63072000; includeSubDomains; preload` | 2-year HSTS so a single year-long cert rotation never lapses the pin; `preload`-ready for hstspreload.org submission |
| `X-Content-Type-Options` | `nosniff` | Refuses MIME sniffing; closes "polyglot upload served as JS" |
| `X-Frame-Options` | `DENY` | Legacy-browser clickjacking defense (paired with CSP `frame-ancestors 'none'`) |
| `Referrer-Policy` | `strict-origin-when-cross-origin` | Prevents leaking declaration IDs in the Referer when the user navigates off the receipt screen |
| `Permissions-Policy` | `geolocation=(), camera=(), microphone=(), payment=(), usb=(), magnetometer=(), gyroscope=(), accelerometer=(), autoplay=(), fullscreen=(), picture-in-picture=()` | Disables every browser feature the portal does not use, so a future bundle bug or compromised dependency can't silently grow privilege |
| `Cross-Origin-Opener-Policy` | `same-origin` | Isolates the browsing context against window.opener attacks + Spectre-class side-channels |
| `Cross-Origin-Resource-Policy` | `same-origin` | Refuses cross-origin sites embedding our resources |
| `Server` | `nginx` (no version) | `server_tokens off` keeps the response surface low-entropy |

### CSP, directive by directive

The policy literal:

```
default-src 'self';
script-src 'self';
style-src 'self' 'unsafe-inline';
connect-src 'self' ${CSP_CONNECT_SRC};
img-src 'self' data:;
font-src 'self';
frame-ancestors 'none';
base-uri 'self';
form-action 'self';
object-src 'none'
```

- **`default-src 'self'`** — fallback for any directive not listed. Same-origin everything.
- **`script-src 'self'`** — same-origin scripts only. No `unsafe-inline`, no `unsafe-eval`. Vite's prod bundle emits external JS only; if a future feature needs a third-party script (e.g. an analytics tag) it MUST go through this CLAUDE.md as an explicit, reviewed widening.
- **`style-src 'self' 'unsafe-inline'`** — Tailwind v4 + React 19 still emit a small set of inline `<style>` tags (critical CSS + dynamic style props). `'unsafe-inline'` here is a known, accepted exposure. To remove it we would need to migrate every inline style to a hashed style or a nonce, which costs more than it saves at this stage.
- **`connect-src 'self' ${CSP_CONNECT_SRC}`** — `'self'` covers `/healthz` and same-origin XHR; `${CSP_CONNECT_SRC}` is templated at container startup with the declaration service origin (e.g. `https://api.recor.cm` in prod, `http://127.0.0.1:8080` in dev compose). To add another XHR target (e.g. an OIDC IdP for discovery), extend the value at the orchestrator layer — NEVER edit the template inline. Whitespace-separated multiple origins are supported (`envsubst` does not split the value).
- **`img-src 'self' data:`** — `data:` lets the bundle inline small SVG/PNG icons without a round-trip. No remote images.
- **`font-src 'self'`** — fonts are bundled in the build output; no Google Fonts.
- **`frame-ancestors 'none'`** — refuses iframe embedding outright. Paired with `X-Frame-Options: DENY` for browsers that don't honour CSP frame-ancestors (Safari < 16 in particular).
- **`base-uri 'self'`** — refuses `<base>` tag attacks that could rewrite the resolution of relative URLs.
- **`form-action 'self'`** — refuses native form submissions to other origins (the declaration form posts via `fetch`, not native submit, so this is belt-and-braces).
- **`object-src 'none'`** — no `<object>` / `<embed>` / `<applet>`.

### What's deliberately NOT set

- **`Cross-Origin-Embedder-Policy: require-corp`** is omitted. It would force every same-origin resource to declare CORP and would block any third-party assets (favicon CDN, analytics) without a coordinated rollout. Re-enable when a documented audit confirms every loaded resource is CORP-compatible.
- **CSP `report-uri`** / **`Reporting-Endpoints`** are omitted in v1. Add when we have a CSP-report collector (CSP-Reporter or similar); without one, violations are silently dropped.
- **CSP `nonce-…`** for scripts is omitted because Vite's prod bundle does not emit inline scripts — `'self'` is strict enough. If we add SSR or templating that needs inline scripts, switch to a per-request nonce strategy (NOT `'unsafe-inline'`).

### CSP_CONNECT_SRC contract

- Set via container env. Read by `docker-entrypoint.sh` at startup.
- Whitespace-separated list of origins (e.g. `https://api.recor.cm` or `https://api.recor.cm https://idp.recor.cm`).
- Validated for `; ' "` and newline characters before substitution; the container fails to start if any are present (defense against env-injection that would broaden the policy).
- Unset / empty: the directive collapses to `connect-src 'self'`. The portal will then fail to talk to any external API — intentional fail-closed behaviour.

### How to verify

1. Build the image and start it with a test origin:
   ```
   docker build -t recor/declarant-portal:test .
   docker run --rm -p 18082:8082 -e CSP_CONNECT_SRC=https://api.test.recor.cm recor/declarant-portal:test
   ```
2. `curl -I http://127.0.0.1:18082/` — every header above should be present.
3. Load the SPA in a browser; the dev console must not show any CSP violations (a handful of inline-style notices from Tailwind are expected and explicitly allowed).
4. Or just run `scripts/headers-smoke.sh`, which automates all of the above and asserts every header is present + the templated origin is in CSP.

## When in doubt

1. Read this document
2. Architecture V4 P17 § Declarant Portal
3. Companion V4 P20 § frontend scaffolding
4. The Declaration service CLAUDE.md (where the server-side canonical
   form lives)
5. Ask the architect-reviewer agent
