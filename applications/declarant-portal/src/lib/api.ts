/**
 * Typed client for the RÉCOR Declaration service.
 *
 * v1 wraps fetch directly. A future ticket replaces this with a
 * generated client from the service's OpenAPI spec.
 */

import { z } from 'zod';
import type { SignedDeclarationRequest } from './crypto';

const SubmitDeclarationResponseSchema = z.object({
  declaration_id: z.string().uuid(),
  state: z.string(),
  receipt_hash_hex: z.string().regex(/^[0-9a-f]{64}$/, 'expected 64-char hex'),
  submitted_at: z.string(),
  receipt_url: z.string(),
});

export type SubmitDeclarationResponse = z.infer<
  typeof SubmitDeclarationResponseSchema
>;

/**
 * Subset of `GET /v1/declarations/{id}` response that the portal
 * needs. The server may return additional fields; Zod's default
 * `.passthrough()` is off, so unknown fields are silently dropped at
 * the boundary. This is deliberate — when the server adds a field,
 * the portal continues to work; when it removes one, parsing fails
 * loudly. Both behaviours are desirable.
 *
 * The lane enum is bounded to the three values the verification
 * engine produces. `verification_state` is bounded to the five
 * values the declaration's projection column admits.
 */
const VerificationLaneSchema = z.enum(['green', 'yellow', 'red']);

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
export type VerificationLane = z.infer<typeof VerificationLaneSchema>;

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
});

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
