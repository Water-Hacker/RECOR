//! Application-layer ports.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::domain::{DecisionRationale, VerificationCase, VerificationCaseId};

#[async_trait]
pub trait VerificationRepository: Send + Sync {
    /// Persist a verification case atomically with an outbox row and,
    /// when supplied, the per-case `DecisionRationale` (TODO-049).
    /// `rationale` is `Option` so callers under construction (legacy
    /// integration tests + the kafka consumer cutover path) can keep
    /// the call site compiling while the rationale composer is wired
    /// through; production callers always supply `Some(_)`.
    async fn save_case(
        &self,
        case: &VerificationCase,
        rationale: Option<&DecisionRationale>,
    ) -> Result<(), RepositoryError>;

    /// Load a previously-persisted case by id.
    async fn load_case(
        &self,
        id: VerificationCaseId,
    ) -> Result<Option<VerificationCase>, RepositoryError>;

    /// TODO-049 — load the rationale persisted with the case, if any.
    /// Production cases always have a rationale; the option accommodates
    /// historic rows persisted before this migration shipped.
    async fn load_rationale(
        &self,
        id: VerificationCaseId,
    ) -> Result<Option<DecisionRationale>, RepositoryError>;

    /// Idempotent guard: has this declaration_id already been verified?
    /// Returns the existing case id, if any.
    async fn case_for_declaration(
        &self,
        declaration_id: Uuid,
    ) -> Result<Option<VerificationCaseId>, RepositoryError>;

    /// TODO-061 closure: upsert the declaration_projection row that
    /// Stage 6 pattern queries read. Idempotent; called by
    /// `SubmitVerificationUseCase` BEFORE the orchestrator runs so
    /// Stage 6 sees the current declaration plus any predecessors.
    /// The payload is the full snapshot serialised to JSON so the
    /// projection carries beneficial_owners as JSONB for the
    /// signature queries.
    async fn upsert_declaration_projection(
        &self,
        declaration_id: Uuid,
        entity_id: Uuid,
        declarant_principal: &str,
        submitted_at: time::OffsetDateTime,
        effective_from: time::Date,
        beneficial_owners: serde_json::Value,
        entity_jurisdiction: Option<&str>,
    ) -> Result<(), RepositoryError>;
}

#[derive(Debug, thiserror::Error)]
pub enum RepositoryError {
    #[error("storage backend failure: {0}")]
    Backend(#[from] sqlx::Error),
    #[error("serialisation failure: {0}")]
    Serialisation(#[from] serde_json::Error),
}

/// BUNEC (Bureau National de l'État Civil) identity adapter.
///
/// In production, this resolves declared `person_id` → canonical
/// identity record at the national identity registry. In dev / test,
/// the `MockBunecAdapter` resolves against an in-memory or
/// Postgres-seeded record set.
///
/// Real BUNEC integration is a follow-up ticket (R-VER-1).
#[async_trait]
pub trait BunecAdapter: Send + Sync {
    async fn lookup(&self, person_id: Uuid) -> Result<BunecLookup, BunecLookupError>;
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum BunecLookup {
    /// Person record exists at BUNEC and matches expected attributes.
    /// In v1 the only attribute we validate is existence; future
    /// versions check date of birth, residential address, etc.
    Found {
        person_id: Uuid,
        canonical_full_name: String,
        nationality: String,
    },
    /// No record for that person_id at BUNEC. In production this is a
    /// strong negative signal; the declarant may have invented a
    /// person_id.
    NotFound { person_id: Uuid },
    /// The adapter could not consult BUNEC because the local circuit
    /// breaker is open. This is a "we don't know" outcome — Stage 2
    /// treats it as insufficient evidence (vacuous BPA). Returned by
    /// `RealBunecAdapter` in fail-open mode. (R-VER-1)
    CircuitOpen { since: String },
}

#[derive(Debug, thiserror::Error)]
pub enum BunecLookupError {
    #[error("BUNEC backend failure: {0}")]
    Backend(String),
}

// ─── R-VER-2 — Sanctions screening adapter ─────────────────────────────

/// Query payload for a sanctions / PEP / adverse-media name search.
/// Same shape across stages 3-5; lives here because all three stages
/// consume the same `name_match` helper under the hood.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonQuery {
    pub person_id: Uuid,
    pub full_name: String,
    /// ISO 3166-1 alpha-2 country code, when known.
    pub nationality: Option<String>,
    pub date_of_birth: Option<time::Date>,
}

/// One candidate sanctions hit returned by the adapter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SanctionMatch {
    pub list_entry_id: Uuid,
    /// Source list: 'ofac_sdn' | 'un_consolidated' | 'eu_cfsp'.
    pub source: String,
    pub canonical_full_name: String,
    pub sanction_program: String,
    /// Trigram similarity ∈ [0, 1].
    pub similarity: f64,
    /// Tier classification derived from similarity.
    pub tier: String, // "certain" | "near" | "weak"
}

#[async_trait]
pub trait SanctionsAdapter: Send + Sync {
    /// Screen one person against all configured sanctions lists.
    /// Returns up to `max_candidates` hits ordered by descending
    /// similarity. Implementations MUST filter to similarity ≥ 0.5;
    /// the stage applies its own threshold for `Certain` / `Near`.
    async fn screen(
        &self,
        query: &PersonQuery,
        max_candidates: usize,
    ) -> Result<Vec<SanctionMatch>, AdapterError>;

    /// Sample row count for the metrics gauge. Cheap O(table-stats).
    async fn index_rows(&self) -> Result<i64, AdapterError>;
}

// ─── R-VER-3 — PEP adapter ─────────────────────────────────────────────

/// One candidate PEP hit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PepMatch {
    pub list_entry_id: Uuid,
    pub source: String,
    pub canonical_full_name: String,
    pub position: Option<String>,
    pub country: Option<String>,
    pub is_current: bool,
    /// 'confirmed' | 'associate'.
    pub relationship_kind: String,
    pub similarity: f64,
    pub tier: String,
}

#[async_trait]
pub trait PepAdapter: Send + Sync {
    async fn screen(
        &self,
        query: &PersonQuery,
        max_candidates: usize,
    ) -> Result<Vec<PepMatch>, AdapterError>;

    async fn index_rows(&self) -> Result<i64, AdapterError>;
}

// ─── R-VER-4 — ICIJ adverse-media retrieval ────────────────────────────

/// One ICIJ leak candidate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IcijCandidate {
    pub id: Uuid,
    pub node_kind: String,
    pub source_dataset: String,
    pub canonical_full_name: String,
    pub country_raw: Option<String>,
    pub snippet: Option<String>,
    pub similarity: f64,
    pub tier: String,
}

#[async_trait]
pub trait IcijAdapter: Send + Sync {
    /// Retrieve top-N ICIJ candidates for a name. Used by Stage 5.
    async fn retrieve(
        &self,
        query: &PersonQuery,
        max_candidates: usize,
    ) -> Result<Vec<IcijCandidate>, AdapterError>;
}

#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    #[error("storage backend failure: {0}")]
    Backend(String),
}

impl From<sqlx::Error> for AdapterError {
    fn from(e: sqlx::Error) -> Self {
        Self::Backend(e.to_string())
    }
}

// ─── TODO-013-graph — declaration projection reader ────────────────────

/// A trimmed `declaration_projection` row sufficient for Stage 7's
/// drift / cross-entity rules. Mirrors the columns Stage 6 already
/// reads (`declaration_projection` lives in this service's schema —
/// migration 0006 — and is populated by the writeback subscriber).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclarationProjectionRow {
    pub declaration_id: Uuid,
    pub entity_id: Uuid,
    pub declarant_principal: String,
    pub submitted_at: OffsetDateTime,
    /// The beneficial owners as `person_id`s; Stage 7 doesn't need the
    /// full `OwnerSnapshot` shape for its set-difference logic, so the
    /// reader projects to UUIDs at the boundary.
    pub beneficial_owner_person_ids: Vec<Uuid>,
}

/// Read-side port for the declaration projection that the writeback
/// subscriber populates. Stage 7's prior-declaration drift + cross-
/// entity ownership convergence rules consult this port; the adapter
/// (`infrastructure::PostgresDeclarationProjectionReader`) is the
/// production wiring.
///
/// All methods take a `now`-style cutoff implicitly via the caller — the
/// port does NOT filter by time; the caller composes time semantics.
///
/// D14 fail-closed: implementations MUST surface backend errors via
/// `AdapterError::Backend`. Stage 7 treats an error as "no signal"
/// (vacuous BPA + `InsufficientEvidence`) rather than fabricating a
/// negative finding.
#[async_trait]
pub trait DeclarationProjectionReader: Send + Sync {
    /// Return the most recent prior declarations made by the given
    /// `declarant_principal`, excluding the supplied `current_declaration_id`.
    /// Implementations cap the result set at `limit` rows ordered by
    /// `submitted_at DESC`.
    async fn prior_for_principal(
        &self,
        declarant_principal: &str,
        current_declaration_id: Uuid,
        limit: i64,
    ) -> Result<Vec<DeclarationProjectionRow>, AdapterError>;

    /// Return every projection row where `person_id` appears in the
    /// `beneficial_owners` JSONB array, excluding the supplied
    /// `current_entity_id` (so the caller sees only "other" entities
    /// the person co-owns). Capped at `limit` rows.
    async fn entities_containing_owner(
        &self,
        person_id: Uuid,
        current_entity_id: Uuid,
        limit: i64,
    ) -> Result<Vec<DeclarationProjectionRow>, AdapterError>;
}
