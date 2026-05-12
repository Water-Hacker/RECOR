//! List-by-principal use case (COMP-1, data-subject access).
//!
//! Returns every declaration RÉCOR holds under the authenticated
//! principal. The use case is intentionally thin: the heavy lifting is
//! the repository query plus the authorisation guarantee that the
//! `principal` argument came from auth middleware, not from a
//! caller-controlled field.
//!
//! Why this lives at the application layer (not just inside the HTTP
//! handler):
//!
//!   1. It is a use case in domain-language ("the data subject asks
//!      for all data we hold about them"), distinct from "load one
//!      declaration by id". Keeping it here lets gRPC, future CLI, and
//!      future portal endpoints share the same orchestration.
//!   2. Tests against the in-memory adapter cover the leakage refusal
//!      property without standing up Postgres.
//!
//! D14 (fail-closed): an empty principal — which can only happen if
//! the auth middleware misbehaves — is refused outright rather than
//! treated as a wildcard that would dump every declaration.
//!
//! D15 (cryptographic provenance): the projection carries
//! `receipt_hash_hex` so the declarant can re-verify each receipt
//! offline against the canonical bytes they signed.
//!
//! D17 (zero trust): the `principal` argument is the authenticated
//! subject. The API handler that calls into this use case MUST source
//! it from `Principal::subject`, never from a request body or query
//! string. The repository's documentation pins this contract.

use std::sync::Arc;

use thiserror::Error;

use crate::application::port::{DeclarationRepository, RepositoryError};
use crate::application::DeclarationProjection;

#[derive(Debug, Error)]
pub enum ListByPrincipalError {
    #[error("empty principal — refused fail-closed")]
    EmptyPrincipal,
    #[error(transparent)]
    Repository(#[from] RepositoryError),
}

pub struct ListByPrincipalUseCase {
    repository: Arc<dyn DeclarationRepository>,
}

impl ListByPrincipalUseCase {
    pub fn new(repository: Arc<dyn DeclarationRepository>) -> Self {
        Self { repository }
    }

    #[tracing::instrument(skip(self, principal))]
    pub async fn execute(
        &self,
        principal: &str,
    ) -> Result<Vec<DeclarationProjection>, ListByPrincipalError> {
        let trimmed = principal.trim();
        if trimmed.is_empty() {
            return Err(ListByPrincipalError::EmptyPrincipal);
        }
        let rows = self.repository.find_by_principal(trimmed).await?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests for the list-by-principal use case.
    //!
    //! Two behaviours under test:
    //!
    //!   1. Strict tenancy: a query as principal A returns only A's
    //!      rows, never B's. This is the leakage-refusal property the
    //!      GDPR data-access right depends on — if it breaks, one
    //!      declarant could enumerate another's submissions.
    //!   2. Empty-principal fail-closed: an empty string is refused
    //!      with a typed error rather than treated as a wildcard.
    //!
    //! The repository double is an in-memory implementation seeded
    //! with synthetic projections; the use case is exercised in
    //! isolation from Postgres so this test runs in milliseconds.

    use std::collections::HashMap;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use time::macros::{date, datetime};
    use uuid::Uuid;

    use super::*;
    use crate::application::DeclarationProjection;
    use crate::domain::attestation::{CryptographicAttestation, SignatureAlgorithm};
    use crate::domain::{
        DeclarantRole, DeclarationEvent, DeclarationId, DeclarationKind, DeclarationState,
        EntityId,
    };

    struct InMemoryRepo {
        // declaration_id -> projection
        rows: Mutex<HashMap<Uuid, DeclarationProjection>>,
    }

    impl InMemoryRepo {
        fn new() -> Self {
            Self {
                rows: Mutex::new(HashMap::new()),
            }
        }

        fn insert(&self, projection: DeclarationProjection) {
            self.rows
                .lock()
                .unwrap()
                .insert(projection.declaration_id.0, projection);
        }
    }

    #[async_trait]
    impl DeclarationRepository for InMemoryRepo {
        async fn load_events(
            &self,
            _id: DeclarationId,
        ) -> Result<Vec<DeclarationEvent>, RepositoryError> {
            Ok(Vec::new())
        }

        async fn save_event(
            &self,
            _event: &DeclarationEvent,
            _expected_version: u64,
        ) -> Result<(), RepositoryError> {
            unimplemented!("list_by_principal tests don't exercise save_event")
        }

        async fn load_projection(
            &self,
            id: DeclarationId,
        ) -> Result<Option<DeclarationProjection>, RepositoryError> {
            Ok(self.rows.lock().unwrap().get(&id.0).cloned())
        }

        async fn find_by_principal(
            &self,
            principal: &str,
        ) -> Result<Vec<DeclarationProjection>, RepositoryError> {
            let mut out: Vec<DeclarationProjection> = self
                .rows
                .lock()
                .unwrap()
                .values()
                .filter(|row| row.declarant_principal == principal)
                .cloned()
                .collect();
            // Mirror the Postgres adapter's ORDER BY submitted_at DESC.
            out.sort_by(|a, b| b.submitted_at.cmp(&a.submitted_at));
            Ok(out)
        }

        async fn save_supersede(
            &self,
            _new_event: &DeclarationEvent,
            _new_expected_version: u64,
            _old_id: DeclarationId,
            _old_event: &DeclarationEvent,
            _old_expected_version: u64,
        ) -> Result<(), RepositoryError> {
            unimplemented!("list_by_principal tests don't exercise save_supersede")
        }
    }

    fn synthetic_projection(
        principal: &str,
        receipt_byte: u8,
    ) -> DeclarationProjection {
        DeclarationProjection {
            declaration_id: DeclarationId(Uuid::now_v7()),
            entity_id: EntityId(Uuid::now_v7()),
            declarant_principal: principal.to_string(),
            declarant_role: DeclarantRole::SelfDeclaration,
            kind: DeclarationKind::Incorporation,
            effective_from: date!(2026 - 01 - 01),
            beneficial_owners: Vec::new(),
            attestation: CryptographicAttestation {
                signed_by: principal.to_string(),
                signature_algorithm: SignatureAlgorithm::Ed25519,
                signature_hex: "00".repeat(64),
                public_key_hex: "00".repeat(32),
                nonce_hex: "00".repeat(16),
            },
            state: DeclarationState::Submitted,
            version: 1,
            submitted_at: datetime!(2026 - 05 - 01 12:00 UTC),
            receipt_hash_hex: hex::encode([receipt_byte; 32]),
            correlation_id: Uuid::now_v7(),
            verification_state: "pending".into(),
            verification_lane: None,
            verification_case_id: None,
            verified_at: None,
            supersedes_declaration_id: None,
            superseded_by_declaration_id: None,
            superseded_at: None,
            metadata_notes: None,
            amended_at: None,
            corrected_at: None,
        }
    }

    #[tokio::test]
    async fn returns_only_declarations_for_the_authenticated_principal() {
        // GIVEN: two declarations under principal A and one under
        // principal B sitting in the repository at the same time.
        let principal_a = "spiffe://recor.cm/declarant-alpha";
        let principal_b = "spiffe://recor.cm/declarant-beta";
        let repo = Arc::new(InMemoryRepo::new());
        repo.insert(synthetic_projection(principal_a, 0xAA));
        repo.insert(synthetic_projection(principal_a, 0xAB));
        repo.insert(synthetic_projection(principal_b, 0xBB));

        let usecase = ListByPrincipalUseCase::new(repo);

        // WHEN: A queries.
        let a_rows = usecase.execute(principal_a).await.expect("query A");
        // THEN: A sees their two rows. B's row never surfaces.
        assert_eq!(a_rows.len(), 2, "A should see exactly their two rows");
        for row in &a_rows {
            assert_eq!(
                row.declarant_principal, principal_a,
                "every returned row must belong to the querying principal"
            );
        }

        // WHEN: B queries.
        let b_rows = usecase.execute(principal_b).await.expect("query B");
        // THEN: B sees their one row. A's rows never surface.
        assert_eq!(b_rows.len(), 1);
        assert_eq!(b_rows[0].declarant_principal, principal_b);
    }

    #[tokio::test]
    async fn empty_principal_is_refused_rather_than_treated_as_wildcard() {
        // An empty principal can only arise from an auth-middleware
        // bug. Fail-closed (D14): refuse rather than enumerate every
        // declarant's data.
        let repo = Arc::new(InMemoryRepo::new());
        repo.insert(synthetic_projection("spiffe://recor.cm/x", 0x01));
        let usecase = ListByPrincipalUseCase::new(repo);
        let err = usecase.execute("").await.expect_err("must refuse empty");
        assert!(matches!(err, ListByPrincipalError::EmptyPrincipal));
        // Whitespace-only too.
        let err = usecase.execute("   ").await.expect_err("must refuse blank");
        assert!(matches!(err, ListByPrincipalError::EmptyPrincipal));
    }

    #[tokio::test]
    async fn no_rows_for_unknown_principal_is_ok_empty_vec() {
        // A first-time visitor with no submissions yet should see an
        // empty list, not an error — they have the right to know that
        // no data is held.
        let repo = Arc::new(InMemoryRepo::new());
        let usecase = ListByPrincipalUseCase::new(repo);
        let rows = usecase
            .execute("spiffe://recor.cm/never-submitted")
            .await
            .expect("ok empty");
        assert!(rows.is_empty());
    }
}
