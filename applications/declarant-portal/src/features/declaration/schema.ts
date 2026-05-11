import { z } from 'zod';

// UUIDv4 format (lowercase, hyphenated). The Declaration service
// accepts any UUID; we constrain to v4 here to catch declarant typos.
const uuidRe =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[1-7][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;

// 1..100% expressed as basis points (1..10000).
const basisPoints = z
  .number()
  .int('whole basis points only')
  .min(1, 'percentage must be at least 0.01%')
  .max(10_000, 'percentage cannot exceed 100%');

export const ownerSchema = z.object({
  person_id: z.string().regex(uuidRe, 'expected UUIDv4'),
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
    entity_id: z.string().regex(uuidRe, 'expected UUIDv4'),
    declarant_principal: z
      .string()
      .min(3, 'principal too short')
      .max(512, 'principal too long'),
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
      .regex(/^\d{4}-\d{2}-\d{2}$/, 'expected YYYY-MM-DD'),
    beneficial_owners: z
      .array(ownerSchema)
      .min(1, 'at least one beneficial owner required')
      .max(64, 'more than 64 owners is unusual; contact support'),
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
      message:
        'beneficial owners must collectively hold 100% (10_000 basis points)',
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
      message: 'each beneficial owner person_id must appear only once',
      path: ['beneficial_owners'],
    },
  )
  .refine(
    (data) => {
      const today = new Date().toISOString().slice(0, 10);
      return data.effective_from <= today;
    },
    {
      message: 'effective_from cannot be in the future',
      path: ['effective_from'],
    },
  );

export type FormValues = z.infer<typeof formSchema>;
