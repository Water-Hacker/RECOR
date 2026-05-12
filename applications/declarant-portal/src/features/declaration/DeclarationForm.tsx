/**
 * Top-level declaration entry point.
 *
 * As of R-PORT-3 the single-page form has been refactored into a
 * 4-step wizard (Entity → Owners → Review → Sign). The wizard shell
 * holds the `useForm` instance, the Ed25519 keypair, the
 * stable-per-session declaration_id + nonce, and the per-step
 * Forward/Back gate. This module is now a thin pass-through so the
 * existing App-level layout, auth wrapper, and test harnesses keep
 * importing `DeclarationForm` without churn.
 *
 * If you need to add a step, change a field, or alter the signing
 * contract, do it in `./wizard/` — not here.
 */

import { DeclarationWizard } from './wizard';

interface DeclarationFormProps {
  apiBaseUrl: string;
}

export function DeclarationForm({ apiBaseUrl }: DeclarationFormProps) {
  return <DeclarationWizard apiBaseUrl={apiBaseUrl} />;
}
