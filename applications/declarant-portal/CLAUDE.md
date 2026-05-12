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

## When in doubt

1. Read this document
2. Architecture V4 P17 § Declarant Portal
3. Companion V4 P20 § frontend scaffolding
4. The Declaration service CLAUDE.md (where the server-side canonical
   form lives)
5. Ask the architect-reviewer agent
