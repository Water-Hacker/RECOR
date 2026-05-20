-- Migration: 0008_attestation_nonce_uniqueness
-- Service:   declaration
-- Sprint:    PR-FATF-1 (FATF-readiness Pass 1)
-- Author:    RÉCOR engineering
-- Rationale: closes TODO-017 from the production-readiness audit
--            (TODOS.md) and `REQ-asvs-v2-005` (OWASP ASVS V2.5
--            replay-resistance). Every Ed25519 attestation submitted
--            with a declaration event MUST present a previously-unseen
--            (signer_public_key, nonce) pair. Without this, a captured
--            signature can be replayed against future declarations.
--
-- Properties verified post-migration:
--   1. attestation_nonces.(signer_public_key_hex, nonce_hex) is PK
--   2. Re-insert of an existing (pubkey, nonce) raises a uniqueness
--      violation that the application maps to a NonceCollision error.
--   3. Index on declaration_id supports reverse-lookup ("which nonces
--      did this declaration use") for the audit-verifier path.
--   4. Index on used_at supports the retention worker (TODO-016).
--
-- Closes FATF special-focus-area #3 (verification at submission —
-- replay-resistance is a precondition for any signature-based
-- declarant identity verification).

BEGIN;

CREATE TABLE IF NOT EXISTS attestation_nonces (
    -- Hex-encoded Ed25519 verifying-key (64 chars). The signer
    -- identity that the uniqueness constraint scopes to. We do NOT
    -- key the constraint on declarant_principal because a single
    -- principal could rotate keys; conversely, a single key could
    -- represent multiple principals (delegated signing). Replay
    -- protection is per-key.
    signer_public_key_hex   TEXT NOT NULL CHECK (char_length(signer_public_key_hex) = 64),

    -- Hex-encoded nonce. Length validation (16+ bytes ⇒ 32+ chars)
    -- mirrors the domain attestation type. The upper bound is
    -- defensive against pathological inputs.
    nonce_hex               TEXT NOT NULL CHECK (char_length(nonce_hex) BETWEEN 32 AND 256),

    -- Which declaration consumed this nonce — supports audit-verifier
    -- reverse-lookup ("which nonces did this declaration use") and
    -- the retention worker's per-declaration cleanup path.
    declaration_id          UUID NOT NULL,

    -- Which event type the nonce was used by:
    --   declaration.submitted.v1 | declaration.amended.v1 | declaration.corrected.v1
    -- (Verified and Superseded do not carry attestations, so they
    -- never appear here.)
    event_type              TEXT NOT NULL CHECK (
        event_type IN (
            'declaration.submitted.v1',
            'declaration.amended.v1',
            'declaration.corrected.v1'
        )
    ),

    -- Time the nonce was consumed. Index supports the retention
    -- worker.
    used_at                 TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- The uniqueness constraint that delivers the FATF requirement.
    -- Replay of a previously-seen (signer_public_key, nonce_hex) pair
    -- raises a unique-violation that the application maps to a
    -- structured NonceCollision error returned to the API caller.
    PRIMARY KEY (signer_public_key_hex, nonce_hex)
);

-- Reverse-lookup: which nonces a declaration consumed.
CREATE INDEX IF NOT EXISTS idx_attestation_nonces_declaration
    ON attestation_nonces(declaration_id);

-- Retention-worker support: prune nonces older than the configured
-- window (signer-key revocation + safety margin).
CREATE INDEX IF NOT EXISTS idx_attestation_nonces_used_at
    ON attestation_nonces(used_at);

-- COMP-2 / D15: nonces are an append-only audit record. Once a
-- (pubkey, nonce) is recorded it is the historical fact that this
-- nonce was used for this declaration; mutating it would erase the
-- replay-protection guarantee. We do not install the full COMP-2
-- BEFORE-trigger machinery here (the table is not part of the
-- declaration_events audit log; it is operational state with explicit
-- retention) but we DO revoke UPDATE on PUBLIC so an accidental
-- INSERT/UPDATE pattern in application code cannot rewrite history.
REVOKE UPDATE ON attestation_nonces FROM PUBLIC;

COMMIT;
