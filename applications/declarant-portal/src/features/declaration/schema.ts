/**
 * Zod schema for the declaration form (D17 zero-trust: validation runs
 * client-side before signing; server re-validates the canonical
 * payload independently).
 *
 * i18n note: messages here are i18next translation keys
 * (e.g. `'validation.uuid'`), not user-visible strings. The form
 * component resolves them via `t()` at render time so the error text
 * follows the active locale (fr / en / pidgin). Keep the keys flat
 * under the `validation.*` namespace so the translation review surface
 * is obvious.
 */

import { z } from 'zod';

// UUIDv4 format (lowercase, hyphenated). The Declaration service
// accepts any UUID; we constrain to v4 here to catch declarant typos.
const uuidRe =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[1-7][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;

// 1..100% expressed as basis points (1..10000).
const basisPoints = z
  .number()
  .int('validation.basisPointsInt')
  .min(1, 'validation.basisPointsMin')
  .max(10_000, 'validation.basisPointsMax');

export const ownerSchema = z.object({
  person_id: z.string().regex(uuidRe, 'validation.uuid'),
  ownership_basis_points: basisPoints,
  interest_kind: z.enum([
    'equity',
    'voting',
    'family_proxy',
    'contractual',
    'other',
  ]),
});

export const formSchema = z
  .object({
    entity_id: z.string().regex(uuidRe, 'validation.uuid'),
    declarant_principal: z
      .string()
      .min(3, 'validation.principalTooShort')
      .max(512, 'validation.principalTooLong'),
    declarant_role: z.enum([
      'self',
      'authorised_agent',
      'operator_assisted',
    ]),
    kind: z.enum([
      'incorporation',
      'annual_renewal',
      'change_of_control',
      'correction',
      'amendment',
    ]),
    effective_from: z
      .string()
      .regex(/^\d{4}-\d{2}-\d{2}$/, 'validation.dateFormat'),
    beneficial_owners: z
      .array(ownerSchema)
      .min(1, 'validation.ownersMin')
      .max(64, 'validation.ownersMax'),
  })
  .refine(
    (data) => {
      const sum = data.beneficial_owners.reduce(
        (acc, o) => acc + o.ownership_basis_points,
        0,
      );
      return sum === 10_000;
    },
    {
      message: 'validation.ownersTotal',
      path: ['beneficial_owners'],
    },
  )
  .refine(
    (data) => {
      const seen = new Set<string>();
      for (const o of data.beneficial_owners) {
        if (seen.has(o.person_id)) return false;
        seen.add(o.person_id);
      }
      return true;
    },
    {
      message: 'validation.ownersUnique',
      path: ['beneficial_owners'],
    },
  )
  .refine(
    (data) => {
      const today = new Date().toISOString().slice(0, 10);
      return data.effective_from <= today;
    },
    {
      message: 'validation.effectiveFromFuture',
      path: ['effective_from'],
    },
  );

export type FormValues = z.infer<typeof formSchema>;
