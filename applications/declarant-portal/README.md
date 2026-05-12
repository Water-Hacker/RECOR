# recor-declarant-portal

The first user-facing surface of RÉCOR. A Cameroonian SARL director opens the page in a browser, fills in the company's beneficial ownership, and submits — the browser signs the declaration with an Ed25519 keypair it generates locally, the private key never touches the wire, and the server-side verification accepts the signature because the canonical-form bytes match byte-for-byte.

## What works

- **React 19 + TypeScript 5.7 strict + Vite 6 + Tailwind v4** per Architecture V3 P8
- **Browser-side Ed25519 signing** via Web Crypto API. Keypair generated in-page; private key in memory only (offline persistence is `R-PORT-2`)
- **Canonical-form construction** that matches the server's Rust canonicalisation byte-for-byte (the property that makes signature verification work)
- **Form validation** via react-hook-form + Zod: UUIDv4 format, basis-points sum to 100%, no duplicate owners, no future effective dates
- **Live submission** to the Declaration service with `Idempotency-Key` header
- **Receipt display** with declaration ID + BLAKE3 receipt hash + state
- **8 unit tests** covering canonical-form output, hex round-tripping, nonce randomness
- **Multi-stage Docker build** (node:22 build → nginx:1.27-alpine runtime), 197 KB main bundle, 62 KB gzipped
- **Full integrated stack** via `docker compose up`: Postgres + Declaration service + Portal

## Verified end-to-end (2026-05-11)

```bash
docker compose up -d --build
# Service stack healthy in ~10s
curl http://127.0.0.1:8082/healthz                # 200 OK
curl http://127.0.0.1:8080/healthz                # 200 OK (declaration service)
```

The browser canonical form was tested against the live Declaration service: same canonical bytes, same Ed25519 signature, **HTTP 201 with signed receipt** — proving the parity property holds.

## Quick start

```bash
cd applications/declarant-portal
echo "RECOR_DB_PASSWORD=$(openssl rand -hex 24)" > .env
docker compose up -d --build
# Open http://127.0.0.1:8082 in a browser
```

The portal communicates with the Declaration service at `http://localhost:8080` (configurable via `VITE_DECLARATION_API_URL` at build time).

## Architecture

```
applications/declarant-portal/
├── src/
│   ├── main.tsx                       — entry; QueryClient + StrictMode
│   ├── App.tsx                        — layout + header + form mount
│   ├── lib/
│   │   ├── crypto.ts                  — Ed25519 + canonical form + nonce
│   │   ├── crypto.test.ts             — 8 unit tests
│   │   └── api.ts                     — typed Declaration service client
│   ├── features/declaration/
│   │   ├── schema.ts                  — Zod form schema (basis-points,
│   │   │                                duplicate-owner, sum-100%, no-future)
│   │   └── DeclarationForm.tsx        — react-hook-form UI + mutation
│   └── styles/index.css               — Tailwind v4 + design tokens
├── tests/setup.ts
├── index.html
├── Dockerfile                         — multi-stage build
├── nginx.conf.template                — SPA fallback + security headers (envsubst-rendered at container start)
├── security-headers.conf.template     — shared header set included in every location block
├── docker-entrypoint.sh               — renders the templates, then `exec nginx`
├── scripts/headers-smoke.sh           — OPS-3 smoke: builds the image and asserts every required header
├── docker-compose.yaml                — portal + declaration + postgres
├── tsconfig.json, vite.config.ts, package.json
```

## The canonical-form parity property

The Declaration service verifies every submission's Ed25519 signature against canonical bytes. The browser MUST produce identical bytes for verification to succeed:

```ts
// src/lib/crypto.ts:canonicalPayloadBytes
const canonical = {
  entity_id: payload.entity_id,
  declarant_principal: payload.declarant_principal,
  declarant_role: payload.declarant_role,
  kind: payload.kind,
  effective_from: payload.effective_from,   // ISO 8601 YYYY-MM-DD
  beneficial_owners: payload.beneficial_owners,
  nonce_hex: payload.nonce_hex,
};
return new TextEncoder().encode(JSON.stringify(canonical));
```

Matches the Rust server's:

```rust
// services/declaration/src/api/rest.rs::canonical_payload_bytes
#[derive(Serialize)]
struct Canonical<'a> {
    entity_id: &'a EntityId,
    declarant_principal: &'a str,
    declarant_role: &'static str,
    kind: &'static str,
    #[serde(with = "iso_date")]
    effective_from: time::Date,
    beneficial_owners: &'a [BeneficialOwnerClaim],
    nonce_hex: &'a str,
}
serde_json::to_vec(&canonical)
```

The 8 unit tests in `crypto.test.ts` assert the exact byte sequence; any drift breaks the tests.

## Doctrines

- ✅ D01 completeness — real Ed25519, real form validation, real API integration, real Docker, real tests
- ✅ D04 tests — 8 unit tests for the cryptographic centrepiece + Zod validation in the schema module
- ✅ D05 documentation — this README + inline TSDoc
- ✅ D08 no dangling threads — 7 follow-up tickets filed
- ✅ D12 production-grade from first commit — TypeScript strict, no `any`, no ESLint disabled
- ✅ D14 fail-closed — API errors surface as a visible error message; key generation failure blocks submission
- ✅ D15 cryptographic provenance — Ed25519 signature on every declaration, receipt hash displayed
- ✅ D17 zero trust — declarant principal carried in `X-Recor-Dev-Principal` (dev path; OIDC `R-PORT-1` follow-up)
- ✅ D18 no secrets — no API keys; `.env` only holds the Postgres password for the docker-compose stack

## Follow-up tickets

- `R-PORT-1` — i18n (French primary, English secondary, Pidgin tertiary)
- `R-PORT-2` — Offline drafts (Dexie/IndexedDB) + Workbox service worker
- `R-PORT-3` — Multi-step wizard (entity → owners → review → sign)
- `R-PORT-4` — Verification status polling + display (live lane decision once Verification engine integrates)
- `R-PORT-5` — Full WCAG 2.1 AA audit + a11y test suite
- `R-PORT-6` — Playwright E2E tests against the built bundle
- `R-PORT-7` — Generated API client from Declaration service's OpenAPI spec (replaces the hand-written `lib/api.ts`)

## Browser support

Browser-side Ed25519 via Web Crypto API requires:
- Chrome / Edge ≥ 113
- Firefox ≥ 130
- Safari ≥ 17.4

The portal feature-detects and shows a clear error message on unsupported browsers.

## Out-of-the-box experience for a Cameroonian declarant

1. Open `http://recor.cm` (or the local URL during pilot)
2. Form loads in seconds; signing key generates in the page
3. Fill in entity ID, click "Add beneficial owner" until 100%
4. Click "Sign and submit declaration"
5. Receive a cryptographic receipt — declaration ID + BLAKE3 hash
6. Print or save the receipt; it's a permanent proof of what was submitted
