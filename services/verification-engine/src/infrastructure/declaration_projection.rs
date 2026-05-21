//! Postgres adapter for `DeclarationProjectionReader` (TODO-013-graph).
//!
//! Reads from the `declaration_projection` table that the writeback
//! subscriber populates (migration 0006). The reader is intentionally
//! a thin SQL wrapper — Stage 7 owns the rule interpretation; the
//! adapter just returns rows.
//!
//! Two queries:
//!
//! 1. `prior_for_principal` — declarations by the same
//!    `declarant_principal`, excluding the current one. Used by
//!    Stage 7 Rule 4 (prior-declaration drift).
//! 2. `entities_containing_owner` — every projection row where a
//!    given `person_id` appears in the `beneficial_owners` JSONB
//!    array, excluding the current entity. Used by Stage 7 Rule 5
//!    (cross-entity ownership convergence).
//!
//! Both queries hit existing indexes:
//!   - `idx_decl_proj_owner_jsonb` (GIN over beneficial_owners) for #2
//!   - `idx_decl_proj_submitted` (btree DESC) for #1 ORDER BY
//!
//! D17 zero trust: the adapter accepts a generic `PgPool`. In
//! production this can be a read-replica pool by binding a different
//! `DATABASE_URL_READ_REPLICA` at composition time. v1 reuses the
//! primary pool; the production opt-in is a config-only change with
//! no surface impact on this code.

use async_trait::async_trait;
use serde_json::Value as JsonValue;
use sqlx::{PgPool, Row};
use time::OffsetDateTime;
use tracing::instrument;
use uuid::Uuid;

use crate::application::port::{
    AdapterError, DeclarationProjectionReader, DeclarationProjectionRow,
};

pub struct PostgresDeclarationProjectionReader {
    pool: PgPool,
}

impl PostgresDeclarationProjectionReader {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DeclarationProjectionReader for PostgresDeclarationProjectionReader {
    #[instrument(skip(self), fields(principal = %declarant_principal))]
    async fn prior_for_principal(
        &self,
        declarant_principal: &str,
        current_declaration_id: Uuid,
        limit: i64,
    ) -> Result<Vec<DeclarationProjectionRow>, AdapterError> {
        let rows = sqlx::query(
            "SELECT declaration_id, entity_id, declarant_principal, \
                    submitted_at, beneficial_owners \
             FROM declaration_projection \
             WHERE declarant_principal = $1 \
               AND declaration_id <> $2 \
             ORDER BY submitted_at DESC \
             LIMIT $3",
        )
        .bind(declarant_principal)
        .bind(current_declaration_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            out.push(row_to_projection(&row)?);
        }
        Ok(out)
    }

    #[instrument(skip(self), fields(person_id = %person_id))]
    async fn entities_containing_owner(
        &self,
        person_id: Uuid,
        current_entity_id: Uuid,
        limit: i64,
    ) -> Result<Vec<DeclarationProjectionRow>, AdapterError> {
        // jsonb_path_exists is index-friendly with the GIN
        // `jsonb_path_ops` index defined in migration 0006. The
        // containment query
        //   beneficial_owners @> jsonb_build_array(jsonb_build_object('person_id', $1))
        // is the canonical "contains owner X" form; we use the
        // explicit `jsonb_path_exists` style for clarity.
        let person_id_str = person_id.to_string();
        let rows = sqlx::query(
            "SELECT declaration_id, entity_id, declarant_principal, \
                    submitted_at, beneficial_owners \
             FROM declaration_projection \
             WHERE beneficial_owners @> jsonb_build_array( \
                       jsonb_build_object('person_id', $1::text) \
                   ) \
               AND entity_id <> $2 \
             ORDER BY submitted_at DESC \
             LIMIT $3",
        )
        .bind(&person_id_str)
        .bind(current_entity_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            out.push(row_to_projection(&row)?);
        }
        Ok(out)
    }
}

fn row_to_projection(
    row: &sqlx::postgres::PgRow,
) -> Result<DeclarationProjectionRow, AdapterError> {
    let declaration_id: Uuid =
        row.try_get("declaration_id").map_err(sqlx_to_adapter)?;
    let entity_id: Uuid = row.try_get("entity_id").map_err(sqlx_to_adapter)?;
    let declarant_principal: String = row
        .try_get("declarant_principal")
        .map_err(sqlx_to_adapter)?;
    let submitted_at: OffsetDateTime =
        row.try_get("submitted_at").map_err(sqlx_to_adapter)?;
    let owners_json: JsonValue =
        row.try_get("beneficial_owners").map_err(sqlx_to_adapter)?;

    let owners = decode_owner_uuids(&owners_json);
    Ok(DeclarationProjectionRow {
        declaration_id,
        entity_id,
        declarant_principal,
        submitted_at,
        beneficial_owner_person_ids: owners,
    })
}

fn sqlx_to_adapter(e: sqlx::Error) -> AdapterError {
    AdapterError::Backend(e.to_string())
}

/// Best-effort decode of the JSONB array into a list of `person_id`
/// UUIDs. Unknown entries are skipped — Stage 7 treats missing data
/// as "no signal" rather than a hard error (D14 fail-closed for
/// adapter errors only, not for malformed-but-old projection rows).
fn decode_owner_uuids(value: &JsonValue) -> Vec<Uuid> {
    let Some(arr) = value.as_array() else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(arr.len());
    for entry in arr {
        if let Some(s) = entry.get("person_id").and_then(|v| v.as_str()) {
            if let Ok(uuid) = Uuid::parse_str(s) {
                out.push(uuid);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn decode_owner_uuids_skips_malformed_entries() {
        let v = json!([
            {"person_id": "01900000-0000-7000-8000-000000000001"},
            {"person_id": "not-a-uuid"},
            {"unrelated": "value"},
            {"person_id": "01900000-0000-7000-8000-000000000002"},
        ]);
        let out = decode_owner_uuids(&v);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn decode_owner_uuids_handles_non_array() {
        let v = json!({"not": "an array"});
        assert!(decode_owner_uuids(&v).is_empty());
    }
}
