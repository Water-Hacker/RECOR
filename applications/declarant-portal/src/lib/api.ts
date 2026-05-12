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
