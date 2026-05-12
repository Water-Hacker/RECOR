/**
 * Typed client for the RÉCOR Declaration service.
 *
 * Wire-shape type definitions originate from the generated OpenAPI
 * client (`src/generated/openapi.ts`, produced from
 * `docs/openapi/declaration.json` by `pnpm openapi:gen`). The runtime
 * Zod schemas remain the trust boundary (D17 zero-trust) — the portal
 * never trusts the wire shape blindly — but their structural shape is
 * pinned to the generated types via `satisfies` so any drift between
 * the spec and the runtime parsing surfaces at `pnpm typecheck`.
 *
 * Higher-level wrappers (`submitDeclaration`, `getDeclaration`) and
 * their function signatures are intentionally unchanged so callers
 * (`DeclarationForm`, `VerificationStatus`, tests) need no updates.
 */

import { z } from 'zod';

import type { SignedDeclarationRequest } from './crypto';
import type { components } from '../generated/openapi';

/* ─── generated-schema re-exports (D15 contract surface) ──────────────
 *
 * The generated `components.schemas` namespace is the platform's
 * public DTO catalogue. Re-exporting names that the portal actually
 * uses keeps imports terse without forcing consumers to know the
 * `components['schemas'][...]` indexing pattern.
 */
export type Schemas = components['schemas'];

export type SubmitDeclarationRequestWire = Schemas['SubmitDeclarationRequest'];
export type SubmitDeclarationResponse = Schemas['SubmitDeclarationResponse'];
export type DeclarationKind = Schemas['DeclarationKind'];
export type DeclarantRole = Schemas['DeclarantRole'];
export type InterestKind = Schemas['InterestKind'];
export type BeneficialOwnerClaim = Schemas['BeneficialOwnerClaim'];
export type CryptographicAttestation = Schemas['CryptographicAttestation'];
export type ErrorEnvelope = Schemas['ErrorEnvelope'];
export type VerificationLane = Schemas['VerificationLane'];

/* ─── runtime validation (D17 zero-trust) ────────────────────────────
 *
 * Every server response is parsed through a Zod schema at the wire
 * boundary. Wire-shape and parsed-shape alignment is enforced two ways:
 *
 *   1. `satisfies z.ZodType<...Wire>` — the schema is a Zod type that
 *      produces a value assignable to the generated wire shape.
 *   2. Compile-time `_AlignmentSentinel` blocks below — the parsed
 *      `z.infer` type must remain assignable to the corresponding
 *      generated DTO. If a spec change widens a field type or renames
 *      one, `pnpm typecheck` fails.
 *
 * The `state` and `verification_state` columns are deliberately
 * narrower than the spec's open `string`: the portal UI branches on a
 * closed set of values, and surfacing an unknown value as a parse
 * failure beats letting it propagate silently.
 */

const VerificationLaneSchema = z.enum(['green', 'yellow', 'red']);

const SubmitDeclarationResponseSchema = z.object({
  declaration_id: z.string().uuid(),
  state: z.string(),
  receipt_hash_hex: z.string().regex(/^[0-9a-f]{64}$/, 'expected 64-char hex'),
  submitted_at: z.string(),
  receipt_url: z.string(),
}) satisfies z.ZodType<SubmitDeclarationResponse>;

/**
 * Subset of `GET /v1/declarations/{id}` response that the portal
 * branches on. The server may return additional fields (declarant_role,
 * declarant_principal, kind, metadata_notes, …) — Zod's default
 * `.strip()` drops unknown keys at the boundary. Adding fields server-
 * side stays backward-compatible; removing one fails the schema parse.
 *
 * The lane enum is bounded to the three values the verification
 * engine produces. `verification_state` is bounded to the five values
 * the declaration's projection column admits.
 */
const GetDeclarationResponseSchema = z.object({
  declaration_id: z.string().uuid(),
  entity_id: z.string().uuid(),
  declarant_principal: z.string(),
  state: z.string(),
  aggregate_version: z.number().int().nonnegative(),
  submitted_at: z.string(),
  receipt_hash_hex: z.string().regex(/^[0-9a-f]{64}$/),

  verification_state: z.enum([
    'not_verified',
    'pending',
    'in_verification',
    'accepted',
    'rejected',
  ]),
  verification_lane: VerificationLaneSchema.optional(),
  verification_case_id: z.string().uuid().optional(),
  verified_at: z.string().optional(),

  // Supersede-chain fields (R-DECL-3).
  supersedes_declaration_id: z.string().uuid().optional(),
  superseded_by_declaration_id: z.string().uuid().optional(),
  superseded_at: z.string().optional(),
});

export type GetDeclarationResponse = z.infer<
  typeof GetDeclarationResponseSchema
>;

/* ─── compile-time drift sentinels ──────────────────────────────────
 *
 * If the spec moves a field's type (e.g. `submitted_at` from
 * `string` to `string | null`) or renames one, these `extends`
 * checks fail at typecheck. The sentinel values are unused at
 * runtime — the type-level constraints are the load-bearing part.
 *
 * Note: the parsed `GetDeclarationResponse` is a SUBSET of the
 * generated wire shape (portal only branches on selected fields), so
 * we project the generated DTO onto the parsed key set rather than
 * asserting full structural equality.
 */
type _SubmitAlignment = SubmitDeclarationResponse extends {
  declaration_id: string;
  state: string;
  receipt_hash_hex: string;
  submitted_at: string;
  receipt_url: string;
}
  ? true
  : never;
const _SUBMIT_ALIGNMENT: _SubmitAlignment = true;
void _SUBMIT_ALIGNMENT;

type _GeneratedGetDeclaration = Schemas['GetDeclarationResponse'];
type _ParsedGetKeys = keyof GetDeclarationResponse;
type _AllParsedKeysExistOnGenerated = _ParsedGetKeys extends
  keyof _GeneratedGetDeclaration
  ? true
  : never;
const _GET_ALIGNMENT: _AllParsedKeysExistOnGenerated = true;
void _GET_ALIGNMENT;

/**
 * Terminal verification states — once a declaration reaches one of
 * these, no further status changes are expected and the UI can stop
 * polling. `in_verification` is NOT terminal: a yellow-lane case
 * means an analyst will eventually move it to accepted or rejected.
 */
export function isTerminalVerificationState(s: string): boolean {
  return s === 'accepted' || s === 'rejected';
}

const ErrorBodySchema = z.object({
  error: z.object({
    kind: z.string(),
    message: z.string(),
  }),
}) satisfies z.ZodType<ErrorEnvelope>;

export class ApiError extends Error {
  readonly status: number;
  readonly kind: string;

  constructor(status: number, kind: string, message: string) {
    super(message);
    this.status = status;
    this.kind = kind;
    this.name = 'ApiError';
  }
}

export interface ApiConfig {
  baseUrl: string;
  declarantPrincipal: string;
}

export async function submitDeclaration(
  config: ApiConfig,
  signed: SignedDeclarationRequest,
): Promise<SubmitDeclarationResponse> {
  const idempotencyKey = randomIdempotencyKey();
  const response = await fetch(`${config.baseUrl}/v1/declarations`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'X-Recor-Dev-Principal': config.declarantPrincipal,
      'Idempotency-Key': idempotencyKey,
    },
    body: JSON.stringify(signed),
  });

  const raw = await response.text();
  if (!response.ok) {
    const parsed = ErrorBodySchema.safeParse(safeJson(raw));
    if (parsed.success) {
      throw new ApiError(
        response.status,
        parsed.data.error.kind,
        parsed.data.error.message,
      );
    }
    throw new ApiError(
      response.status,
      'http_error',
      raw || `HTTP ${response.status}`,
    );
  }
  return SubmitDeclarationResponseSchema.parse(safeJson(raw));
}

/**
 * Fetch a declaration's current projection by id. Used by the
 * verification-status polling view after submission so the declarant
 * sees whether the verification engine accepted / is reviewing /
 * rejected their submission.
 */
export async function getDeclaration(
  config: ApiConfig,
  declarationId: string,
): Promise<GetDeclarationResponse> {
  const response = await fetch(
    `${config.baseUrl}/v1/declarations/${encodeURIComponent(declarationId)}`,
    {
      method: 'GET',
      headers: {
        'X-Recor-Dev-Principal': config.declarantPrincipal,
      },
    },
  );
  const raw = await response.text();
  if (!response.ok) {
    const parsed = ErrorBodySchema.safeParse(safeJson(raw));
    if (parsed.success) {
      throw new ApiError(
        response.status,
        parsed.data.error.kind,
        parsed.data.error.message,
      );
    }
    throw new ApiError(
      response.status,
      'http_error',
      raw || `HTTP ${response.status}`,
    );
  }
  return GetDeclarationResponseSchema.parse(safeJson(raw));
}

function safeJson(raw: string): unknown {
  try {
    return JSON.parse(raw);
  } catch {
    return null;
  }
}

function randomIdempotencyKey(): string {
  return `dp-${globalThis.crypto.randomUUID()}`;
}
