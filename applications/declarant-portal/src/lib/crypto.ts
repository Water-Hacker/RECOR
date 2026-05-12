/**
 * Browser-side Ed25519 keypair management + signing + canonical form.
 *
 * The Declaration service verifies every submission's attestation
 * signature against the canonical bytes of the declaration. THIS
 * module is responsible for producing those bytes byte-identically to
 * what the server canonicalises — any drift produces a signature
 * verification failure.
 *
 * Web Crypto API supports Ed25519 since Chrome 113 / Firefox 130 /
 * Safari 17.4. We feature-detect at startup and surface a clear error
 * to declarants on unsupported browsers.
 */

/** Single declared beneficial owner — wire shape. */
export interface BeneficialOwner {
  person_id: string;
  ownership_basis_points: number;
  interest_kind: 'equity' | 'voting' | 'family_proxy' | 'contractual' | 'other';
}

/** The declarant-supplied data that gets canonicalised + signed. */
export interface DeclarationPayload {
  declaration_id: string;
  entity_id: string;
  declarant_principal: string;
  declarant_role: 'self' | 'authorised_agent' | 'operator_assisted';
  kind:
    | 'incorporation'
    | 'annual_renewal'
    | 'change_of_control'
    | 'correction'
    | 'amendment';
  effective_from: string; // ISO 8601 YYYY-MM-DD
  beneficial_owners: BeneficialOwner[];
  nonce_hex: string;
}

/** A signed declaration ready to POST to the Declaration service. */
export interface SignedDeclarationRequest {
  declaration_id: string;
  entity_id: string;
  declarant_role: DeclarationPayload['declarant_role'];
  kind: DeclarationPayload['kind'];
  effective_from: string;
  beneficial_owners: BeneficialOwner[];
  attestation: {
    signed_by: string;
    signature_algorithm: 'ed25519';
    signature_hex: string;
    public_key_hex: string;
    nonce_hex: string;
  };
}

/* ─── feature detection ───────────────────────────────────────────── */

/** Returns true when the runtime supports Ed25519 via Web Crypto. */
export async function isEd25519Supported(): Promise<boolean> {
  if (!('crypto' in globalThis) || !('subtle' in globalThis.crypto)) {
    return false;
  }
  try {
    // Probe by attempting to generate a key. If the algorithm is
    // unrecognised, this rejects.
    await globalThis.crypto.subtle.generateKey(
      { name: 'Ed25519' } as EcKeyGenParams,
      true,
      ['sign', 'verify'],
    );
    return true;
  } catch {
    return false;
  }
}

/* ─── key generation ──────────────────────────────────────────────── */

/** A non-extractable signing key paired with the exportable public key. */
export interface DeclarantKeys {
  /** CryptoKey for signing — extractable=true so the private key can be
   *  persisted to IndexedDB across sessions if the declarant chooses.
   *  v1 keeps the key in memory only. */
  privateKey: CryptoKey;
  /** CryptoKey for verification (paired with privateKey). */
  publicKey: CryptoKey;
  /** Hex-encoded raw public key (32 bytes) sent on the wire. */
  publicKeyHex: string;
}

/** Generate a fresh Ed25519 keypair via Web Crypto. */
export async function generateKeys(): Promise<DeclarantKeys> {
  const keypair = (await globalThis.crypto.subtle.generateKey(
    { name: 'Ed25519' } as EcKeyGenParams,
    true,
    ['sign', 'verify'],
  )) as CryptoKeyPair;

  const rawPublic = await globalThis.crypto.subtle.exportKey(
    'raw',
    keypair.publicKey,
  );
  return {
    privateKey: keypair.privateKey,
    publicKey: keypair.publicKey,
    publicKeyHex: bytesToHex(new Uint8Array(rawPublic)),
  };
}

/* ─── canonical form ──────────────────────────────────────────────── */

/**
 * Build the canonical bytes the server-side canonicaliser produces.
 *
 * Server side (Rust, src/api/rest.rs::canonical_payload_bytes):
 *
 *   #[derive(Serialize)]
 *   struct Canonical<'a> {
 *     entity_id: &'a EntityId,
 *     declarant_principal: &'a str,
 *     declarant_role: &'static str,
 *     kind: &'static str,
 *     #[serde(with = "iso_date")]
 *     effective_from: time::Date,
 *     beneficial_owners: &'a [BeneficialOwnerClaim],
 *     nonce_hex: &'a str,
 *   }
 *
 *   serde_json::to_vec — fields emitted in declaration order, no
 *   whitespace, ISO-8601 date for effective_from, basis points as
 *   integers, interest_kind serialised lowercase.
 *
 * THIS function must match field-for-field, byte-for-byte. Drift
 * here breaks signature verification at the server.
 */
export function canonicalPayloadBytes(payload: DeclarationPayload): Uint8Array {
  // Field order MUST match the Rust struct order above.
  const canonical = {
    entity_id: payload.entity_id,
    declarant_principal: payload.declarant_principal,
    declarant_role: payload.declarant_role,
    kind: payload.kind,
    effective_from: payload.effective_from,
    beneficial_owners: payload.beneficial_owners,
    nonce_hex: payload.nonce_hex,
  };
  const json = JSON.stringify(canonical);
  return new TextEncoder().encode(json);
}

/* ─── signing ─────────────────────────────────────────────────────── */

/** Sign the canonical bytes with the declarant's Ed25519 private key. */
export async function signPayload(
  keys: DeclarantKeys,
  payload: DeclarationPayload,
): Promise<string> {
  const bytes = canonicalPayloadBytes(payload);
  // Cast through BufferSource: Uint8Array<ArrayBufferLike> generic
  // includes SharedArrayBuffer in lib.dom; the actual ArrayBuffer is
  // always non-shared for TextEncoder output.
  const sig = await globalThis.crypto.subtle.sign(
    'Ed25519',
    keys.privateKey,
    bytes as unknown as BufferSource,
  );
  return bytesToHex(new Uint8Array(sig));
}

/**
 * Build a `SignedDeclarationRequest` from the declarant-supplied data.
 * Generates the nonce; signs; returns a payload ready to POST.
 */
export async function buildSignedRequest(
  keys: DeclarantKeys,
  inputs: Omit<DeclarationPayload, 'nonce_hex'>,
): Promise<SignedDeclarationRequest> {
  const nonce_hex = randomNonceHex();
  const payload: DeclarationPayload = { ...inputs, nonce_hex };
  const signature_hex = await signPayload(keys, payload);
  return {
    declaration_id: payload.declaration_id,
    entity_id: payload.entity_id,
    declarant_role: payload.declarant_role,
    kind: payload.kind,
    effective_from: payload.effective_from,
    beneficial_owners: payload.beneficial_owners,
    attestation: {
      signed_by: payload.declarant_principal,
      signature_algorithm: 'ed25519',
      signature_hex,
      public_key_hex: keys.publicKeyHex,
      nonce_hex,
    },
  };
}

/* ─── helpers ─────────────────────────────────────────────────────── */

/** 16-byte cryptographically secure nonce, hex-encoded. */
export function randomNonceHex(): string {
  const bytes = new Uint8Array(16);
  globalThis.crypto.getRandomValues(bytes);
  return bytesToHex(bytes);
}

/** UUIDv4. v7 would be nicer (time-sortable) but Web Crypto only
 *  ships v4; v7 is a small follow-up. */
export function randomUuid(): string {
  return globalThis.crypto.randomUUID();
}

export function bytesToHex(bytes: Uint8Array): string {
  let s = '';
  for (let i = 0; i < bytes.length; i++) {
    s += bytes[i]!.toString(16).padStart(2, '0');
  }
  return s;
}

export function hexToBytes(hex: string): Uint8Array {
  if (hex.length % 2 !== 0) {
    throw new Error('hex string length must be even');
  }
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i++) {
    out[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  }
  return out;
}
