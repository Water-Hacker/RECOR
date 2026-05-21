//! `recor-i18n` — localised API error messages.
//!
//! Closes TODO-078. The RÉCOR declaration handler emits typed [`ErrorKind`]
//! codes; the HTTP response builder calls [`lookup`] to obtain a localised
//! message string based on the caller's `Accept-Language` header.
//!
//! ## Design
//!
//! - Two locales are mandatory: `fr` (Cameroonian official language) and
//!   `en` (working language of international observers / FATF reviewers).
//! - The locale is resolved from the first `Accept-Language` tag that the
//!   platform supports. Tags are matched case-insensitively; subtags
//!   (`fr-CM`, `fr-FR`) fall back to the primary tag (`fr`).
//! - If no supported tag is found, `en` is the default (D14 fail-closed:
//!   an unresolvable locale never silences the error message).
//!
//! ## Usage
//!
//! ```rust
//! use recor_i18n::{ErrorKind, Locale, lookup};
//!
//! let accept = "fr-CM, fr;q=0.9, en;q=0.8";
//! let locale = Locale::from_accept_language(accept);
//! let msg = lookup(ErrorKind::DeclarationNotFound, locale);
//! assert!(msg.contains("déclaration"));
//! ```
//!
//! ## Wiring into the handler
//!
//! The declaration handler's error response builder reads the
//! `Accept-Language` header (falling back to `en` when absent), calls
//! [`Locale::from_accept_language`], then calls [`lookup`] to obtain the
//! message. The message is embedded in the JSON error body alongside the
//! stable `error_kind` code:
//!
//! ```json
//! {
//!   "error_kind": "DECLARATION_NOT_FOUND",
//!   "message": "La déclaration demandée est introuvable.",
//!   "correlation_id": "..."
//! }
//! ```
//!
//! The `error_kind` code is stable across locales; the `message` is for
//! human consumption only and must not be parsed by callers.

/// The set of locales the platform supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Locale {
    /// French — Cameroonian official language. Tag: `fr`.
    Fr,
    /// English — international working language. Tag: `en` (the default).
    En,
}

impl Default for Locale {
    fn default() -> Self {
        Locale::En
    }
}

impl Locale {
    /// Resolve a locale from an `Accept-Language` header value.
    ///
    /// Parses the comma-separated list of language tags (with optional
    /// quality values `q=...`). Returns the first supported locale found
    /// in order of appearance. Falls back to [`Locale::En`] when no
    /// supported tag is present or the header is empty.
    ///
    /// Tag matching is case-insensitive. Subtags (`fr-CM`) fall back to
    /// the primary tag (`fr`).
    ///
    /// # Examples
    ///
    /// ```rust
    /// use recor_i18n::Locale;
    ///
    /// assert_eq!(Locale::from_accept_language("fr-CM, en"), Locale::Fr);
    /// assert_eq!(Locale::from_accept_language("en-US"), Locale::En);
    /// assert_eq!(Locale::from_accept_language(""), Locale::En);
    /// assert_eq!(Locale::from_accept_language("de, ja"), Locale::En);
    /// ```
    pub fn from_accept_language(header: &str) -> Self {
        for token in header.split(',') {
            // Strip quality value: "fr-CM;q=0.9" → "fr-CM"
            let tag = token
                .trim()
                .split(';')
                .next()
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase();

            // Match primary tag (subtags included via starts_with).
            if tag.starts_with("fr") {
                return Locale::Fr;
            }
            if tag.starts_with("en") {
                return Locale::En;
            }
        }
        Locale::En
    }
}

/// Typed error kinds emitted by RÉCOR services.
///
/// Each variant corresponds to a stable `error_kind` string in the API
/// response body. Callers must parse the `error_kind` string, not the
/// human-readable `message`.
///
/// New variants may be added; existing variants must never be renamed
/// without a versioned API migration (the `error_kind` string is part
/// of the consumer contract).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErrorKind {
    // --- Declaration service ---
    /// The requested declaration does not exist.
    DeclarationNotFound,
    /// The caller is not authorised to access this declaration.
    DeclarationForbidden,
    /// The submitted declaration payload failed domain validation.
    DeclarationInvalid,
    /// The declaration is in a state that does not permit this operation.
    DeclarationStateConflict,
    /// An idempotency replay was requested but the prior response body
    /// could not be retrieved.
    IdempotencyReplayFailed,
    /// The declaration's Ed25519 attestation signature did not verify.
    AttestationInvalid,
    /// The beneficial-owner list is empty or ownership basis points do
    /// not sum to 10 000.
    BeneficialOwnersInvalid,
    /// The FATF cascade invariant was violated (e.g. SMO without Control).
    CascadeTierInvalid,
    /// The adequacy claims block is missing or contains a future/stale
    /// `up_to_date_as_of` date.
    AdequacyClaimsInvalid,

    // --- Verification engine ---
    /// The verification case does not exist or the caller is not the
    /// declarant / admin.
    VerificationCaseNotFound,
    /// The verification pipeline rejected the declaration.
    VerificationRejected,

    // --- Generic / infrastructure ---
    /// An internal server error occurred; the correlation ID is provided
    /// for debugging.
    InternalError,
    /// The request body could not be parsed as valid JSON.
    MalformedJson,
    /// The `Authorization` header is missing, malformed, or contains an
    /// expired token.
    Unauthenticated,
    /// The `Idempotency-Key` header is missing on a mutating endpoint that
    /// requires it.
    IdempotencyKeyMissing,
}

impl ErrorKind {
    /// The stable snake_case code string embedded in the API response body.
    pub fn code(self) -> &'static str {
        match self {
            Self::DeclarationNotFound => "DECLARATION_NOT_FOUND",
            Self::DeclarationForbidden => "DECLARATION_FORBIDDEN",
            Self::DeclarationInvalid => "DECLARATION_INVALID",
            Self::DeclarationStateConflict => "DECLARATION_STATE_CONFLICT",
            Self::IdempotencyReplayFailed => "IDEMPOTENCY_REPLAY_FAILED",
            Self::AttestationInvalid => "ATTESTATION_INVALID",
            Self::BeneficialOwnersInvalid => "BENEFICIAL_OWNERS_INVALID",
            Self::CascadeTierInvalid => "CASCADE_TIER_INVALID",
            Self::AdequacyClaimsInvalid => "ADEQUACY_CLAIMS_INVALID",
            Self::VerificationCaseNotFound => "VERIFICATION_CASE_NOT_FOUND",
            Self::VerificationRejected => "VERIFICATION_REJECTED",
            Self::InternalError => "INTERNAL_ERROR",
            Self::MalformedJson => "MALFORMED_JSON",
            Self::Unauthenticated => "UNAUTHENTICATED",
            Self::IdempotencyKeyMissing => "IDEMPOTENCY_KEY_MISSING",
        }
    }
}

/// Return the localised human-readable error message for the given
/// [`ErrorKind`] in the given [`Locale`].
///
/// The returned string is suitable for embedding in the JSON `message`
/// field of an API error response. It is for human consumption; callers
/// must not parse it programmatically.
///
/// # Examples
///
/// ```rust
/// use recor_i18n::{ErrorKind, Locale, lookup};
///
/// let msg_fr = lookup(ErrorKind::DeclarationNotFound, Locale::Fr);
/// let msg_en = lookup(ErrorKind::DeclarationNotFound, Locale::En);
/// assert_ne!(msg_fr, msg_en);
/// assert!(msg_fr.len() > 0);
/// assert!(msg_en.len() > 0);
/// ```
pub fn lookup(kind: ErrorKind, locale: Locale) -> &'static str {
    match (kind, locale) {
        // --- French ---
        (ErrorKind::DeclarationNotFound, Locale::Fr) =>
            "La déclaration demandée est introuvable.",
        (ErrorKind::DeclarationForbidden, Locale::Fr) =>
            "Vous n'êtes pas autorisé à accéder à cette déclaration.",
        (ErrorKind::DeclarationInvalid, Locale::Fr) =>
            "Le contenu de la déclaration est invalide. Vérifiez les champs obligatoires.",
        (ErrorKind::DeclarationStateConflict, Locale::Fr) =>
            "L'opération demandée n'est pas autorisée dans l'état actuel de la déclaration.",
        (ErrorKind::IdempotencyReplayFailed, Locale::Fr) =>
            "La réponse précédente pour cette clé d'idempotence est introuvable.",
        (ErrorKind::AttestationInvalid, Locale::Fr) =>
            "La signature d'attestation Ed25519 de la déclaration est invalide.",
        (ErrorKind::BeneficialOwnersInvalid, Locale::Fr) =>
            "La liste des bénéficiaires effectifs est absente ou la somme des parts est incorrecte.",
        (ErrorKind::CascadeTierInvalid, Locale::Fr) =>
            "La cascade FATF est incorrecte : vérifiez les niveaux de contrôle et les déclarations d'officier.",
        (ErrorKind::AdequacyClaimsInvalid, Locale::Fr) =>
            "La déclaration d'exactitude est absente ou la date « à jour au » est hors de la fenêtre de 30 jours.",
        (ErrorKind::VerificationCaseNotFound, Locale::Fr) =>
            "Le dossier de vérification est introuvable.",
        (ErrorKind::VerificationRejected, Locale::Fr) =>
            "La déclaration a été rejetée par le moteur de vérification.",
        (ErrorKind::InternalError, Locale::Fr) =>
            "Une erreur interne s'est produite. Mentionnez l'identifiant de corrélation au support.",
        (ErrorKind::MalformedJson, Locale::Fr) =>
            "Le corps de la requête ne peut pas être analysé en JSON valide.",
        (ErrorKind::Unauthenticated, Locale::Fr) =>
            "L'en-tête Authorization est absent, mal formé ou contient un jeton expiré.",
        (ErrorKind::IdempotencyKeyMissing, Locale::Fr) =>
            "L'en-tête Idempotency-Key est requis pour cette opération.",

        // --- English (and default) ---
        (ErrorKind::DeclarationNotFound, Locale::En) =>
            "The requested declaration was not found.",
        (ErrorKind::DeclarationForbidden, Locale::En) =>
            "You are not authorised to access this declaration.",
        (ErrorKind::DeclarationInvalid, Locale::En) =>
            "The declaration payload is invalid. Check all required fields.",
        (ErrorKind::DeclarationStateConflict, Locale::En) =>
            "The requested operation is not permitted in the declaration's current state.",
        (ErrorKind::IdempotencyReplayFailed, Locale::En) =>
            "The prior response for this idempotency key could not be retrieved.",
        (ErrorKind::AttestationInvalid, Locale::En) =>
            "The declaration's Ed25519 attestation signature did not verify.",
        (ErrorKind::BeneficialOwnersInvalid, Locale::En) =>
            "The beneficial-owner list is empty or ownership basis points do not sum to 10 000.",
        (ErrorKind::CascadeTierInvalid, Locale::En) =>
            "The FATF cascade is invalid: check control-tier and senior-managing-official declarations.",
        (ErrorKind::AdequacyClaimsInvalid, Locale::En) =>
            "The adequacy claims block is missing or the up-to-date-as-of date falls outside the 30-day window.",
        (ErrorKind::VerificationCaseNotFound, Locale::En) =>
            "The verification case was not found.",
        (ErrorKind::VerificationRejected, Locale::En) =>
            "The declaration was rejected by the verification engine.",
        (ErrorKind::InternalError, Locale::En) =>
            "An internal error occurred. Provide the correlation ID to support.",
        (ErrorKind::MalformedJson, Locale::En) =>
            "The request body could not be parsed as valid JSON.",
        (ErrorKind::Unauthenticated, Locale::En) =>
            "The Authorization header is missing, malformed, or contains an expired token.",
        (ErrorKind::IdempotencyKeyMissing, Locale::En) =>
            "The Idempotency-Key header is required for this operation.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr_cm_subtag_resolves_to_french() {
        assert_eq!(Locale::from_accept_language("fr-CM, fr;q=0.9, en;q=0.8"), Locale::Fr);
    }

    #[test]
    fn en_us_subtag_resolves_to_english() {
        assert_eq!(Locale::from_accept_language("en-US"), Locale::En);
    }

    #[test]
    fn empty_header_defaults_to_english() {
        assert_eq!(Locale::from_accept_language(""), Locale::En);
    }

    #[test]
    fn unsupported_locale_defaults_to_english() {
        assert_eq!(Locale::from_accept_language("de, ja, zh-TW"), Locale::En);
    }

    #[test]
    fn declaration_not_found_fr_contains_declaration() {
        let msg = lookup(ErrorKind::DeclarationNotFound, Locale::Fr);
        assert!(
            msg.contains("déclaration"),
            "FR message for DeclarationNotFound should mention déclaration; got: {msg}"
        );
    }

    #[test]
    fn declaration_not_found_en_non_empty() {
        let msg = lookup(ErrorKind::DeclarationNotFound, Locale::En);
        assert!(!msg.is_empty(), "EN message for DeclarationNotFound must not be empty");
    }

    #[test]
    fn fr_and_en_messages_differ_for_all_kinds() {
        let kinds = [
            ErrorKind::DeclarationNotFound,
            ErrorKind::DeclarationForbidden,
            ErrorKind::DeclarationInvalid,
            ErrorKind::DeclarationStateConflict,
            ErrorKind::IdempotencyReplayFailed,
            ErrorKind::AttestationInvalid,
            ErrorKind::BeneficialOwnersInvalid,
            ErrorKind::CascadeTierInvalid,
            ErrorKind::AdequacyClaimsInvalid,
            ErrorKind::VerificationCaseNotFound,
            ErrorKind::VerificationRejected,
            ErrorKind::InternalError,
            ErrorKind::MalformedJson,
            ErrorKind::Unauthenticated,
            ErrorKind::IdempotencyKeyMissing,
        ];
        for kind in kinds {
            let fr = lookup(kind, Locale::Fr);
            let en = lookup(kind, Locale::En);
            assert_ne!(
                fr, en,
                "FR and EN messages for {:?} must differ (both defaulted to the same string?)",
                kind
            );
        }
    }

    #[test]
    fn error_code_is_stable_and_uppercase() {
        // Codes must be stable identifiers; they must not contain lowercase.
        let code = ErrorKind::BeneficialOwnersInvalid.code();
        assert_eq!(code, code.to_uppercase(), "Error code must be uppercase: {code}");
        assert!(!code.is_empty());
    }

    #[test]
    fn quality_value_stripped_correctly() {
        // "fr;q=0.9" must still resolve to French.
        assert_eq!(Locale::from_accept_language("fr;q=0.9"), Locale::Fr);
    }
}
