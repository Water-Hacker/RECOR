---
name: recor-doc-author
description: Documentation writing. Fires when documentation work is requested, when the docs-author agent is delegated to, or when the lead orchestrator detects that documentation is missing for a code change.
---

# RÉCOR documentation authoring

Doctrine 5: documentation is part of the feature.

## Documentation taxonomy

1. **Inline (rustdoc / godoc / TSDoc)**: every public API
2. **README per service**: orientation; how to run; how to test
3. **CLAUDE.md per service**: Claude Code orientation (binding text; consult
   @architect-team for changes)
4. **API reference**: generated from contracts (OpenAPI, GraphQL)
5. **Operational runbooks**: one per documented alert
6. **ADRs**: design decisions (see recor-adr-author skill)

## Audience

Write for the engineer who joins next quarter. Examples are concrete; abstract
description is supported by concrete examples.

## Tone

Direct. Precise. Engineering tone, not marketing. The reader is your
colleague.

## Length

As long as needed; as short as possible. A two-line comment that captures
the key insight beats a paragraph that fills space.

## Patterns

### Inline rustdoc for a public function

```rust
/// Resolve an entity to its canonical identifier.
///
/// Performs deterministic matching first; falls back to fuzzy matching above
/// the documented threshold (0.92). Returns `None` when no match meets the
/// threshold; returns `Some(EntityId)` otherwise.
///
/// # Errors
///
/// Returns `Error::Store` when the entity store is unavailable.
///
/// # Examples
///
/// ```
/// let entity_id = resolver.resolve("BNP Paribas Cameroun").await?;
/// ```
pub async fn resolve(&self, name: &str) -> Result<Option<EntityId>, Error> {
    ...
}
```

### Updating CLAUDE.md

CLAUDE.md changes pass through @architect-team review. They are not edited
casually.

### Updating runbooks

When a new alert is added to /infrastructure/observability/alerts/, a runbook
entry is added to /docs/runbooks/ in the same PR.

## Anti-patterns

- Documentation that restates the code
- Documentation that uses jargon without defining it
- Documentation that goes stale (catch via the docs-present CI gate)
- Documentation in a different repository from the code it describes
