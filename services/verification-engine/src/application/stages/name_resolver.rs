//! Production `NameResolver` implementations.
//!
//! Stages 3 (sanctions), 4 (PEP), and 5 (adverse media) all consume
//! the same `NameResolver` trait — defined in `stage3_sanctions` for
//! historical reasons. The trait maps a beneficial-owner `person_id`
//! to a resolved name + nationality the screening adapters need.
//!
//! For v1 the authoritative source of names is BUNEC (or its mock
//! during dev). This module provides the wrapping adapter that
//! turns a `BunecAdapter::lookup` into a `ResolvedName` so the real
//! stages can be wired in main.rs alongside the existing stubs.
//!
//! Closes FIND-009: prior to this module the only `NameResolver`
//! impl was a test double; the production wiring fell back to the
//! stubbed Stage 3-7 implementations because there was no resolver
//! to construct the real stages with.

use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

use crate::application::port::{BunecAdapter, BunecLookup};
use crate::application::stages::stage3_sanctions::{NameResolver, ResolvedName};

/// Resolves beneficial-owner names by consulting the BUNEC adapter.
/// Returns `None` on `NotFound`, `CircuitOpen`, or any backend error
/// — the screening stages treat absence-of-name as "insufficient
/// evidence" (vacuous BPA).
pub struct BunecNameResolver {
    bunec: Arc<dyn BunecAdapter>,
}

impl BunecNameResolver {
    pub fn new(bunec: Arc<dyn BunecAdapter>) -> Self {
        Self { bunec }
    }
}

#[async_trait]
impl NameResolver for BunecNameResolver {
    async fn resolve(&self, person_id: Uuid) -> Option<ResolvedName> {
        match self.bunec.lookup(person_id).await.ok()? {
            BunecLookup::Found {
                canonical_full_name,
                nationality,
                ..
            } => Some(ResolvedName {
                full_name: canonical_full_name,
                nationality: Some(nationality),
                date_of_birth: None,
            }),
            BunecLookup::NotFound { .. } | BunecLookup::CircuitOpen { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::port::{BunecLookup, BunecLookupError};
    use async_trait::async_trait;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeBunec {
        outcome: Mutex<Option<BunecLookup>>,
    }

    impl FakeBunec {
        fn with(outcome: BunecLookup) -> Arc<Self> {
            Arc::new(Self {
                outcome: Mutex::new(Some(outcome)),
            })
        }
    }

    #[async_trait]
    impl BunecAdapter for FakeBunec {
        async fn lookup(
            &self,
            person_id: Uuid,
        ) -> Result<BunecLookup, BunecLookupError> {
            // Always return the stored outcome; the trait is async
            // so we clone out under the lock.
            let guard = self.outcome.lock().unwrap();
            Ok(guard
                .clone()
                .unwrap_or(BunecLookup::NotFound { person_id }))
        }
    }

    #[tokio::test]
    async fn found_lookup_resolves_to_canonical_name_and_nationality() {
        let id = Uuid::now_v7();
        let bunec = FakeBunec::with(BunecLookup::Found {
            person_id: id,
            canonical_full_name: "Ngono Marie".to_string(),
            nationality: "CM".to_string(),
        });
        let resolver = BunecNameResolver::new(bunec);
        let name = resolver.resolve(id).await.expect("name present");
        assert_eq!(name.full_name, "Ngono Marie");
        assert_eq!(name.nationality.as_deref(), Some("CM"));
    }

    #[tokio::test]
    async fn not_found_lookup_returns_none() {
        let id = Uuid::now_v7();
        let bunec = FakeBunec::with(BunecLookup::NotFound { person_id: id });
        let resolver = BunecNameResolver::new(bunec);
        assert!(resolver.resolve(id).await.is_none());
    }

    #[tokio::test]
    async fn circuit_open_lookup_returns_none() {
        let id = Uuid::now_v7();
        let bunec = FakeBunec::with(BunecLookup::CircuitOpen {
            since: "2026-05-20T00:00:00Z".to_string(),
        });
        let resolver = BunecNameResolver::new(bunec);
        // CircuitOpen ⇒ "we don't know" ⇒ resolver returns None and the
        // downstream stage treats this as insufficient evidence.
        assert!(resolver.resolve(id).await.is_none());
    }
}
