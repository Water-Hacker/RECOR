# RÉCOR client libraries

Generated client libraries that consumer integrations can vendor.

| Path | Purpose | Status |
|---|---|---|
| `go/` | Go SDK for the declaration + verification APIs | Generated from `docs/openapi/declaration.json` + `docs/openapi/verification-engine.json` via `openapi-codegen` (follow-up ticket) |
| `protos/` | Mirror of `contracts/` for downstream consumers who can't depend on the buf-managed source | Generated from `contracts/declaration.proto` (R-DECL-8) |
| `rust/` | Rust SDK | Vendor of `services/{declaration,verification-engine}/src/api/dto.rs` shapes |
| `ts/` | TypeScript SDK | Already generated via `pnpm openapi:gen` into `applications/declarant-portal/src/generated/openapi.ts`; this directory holds the standalone tarball for non-portal consumers |

The directories are currently empty — the audit catalogue's
MEDIUM/LOW item flagging them as "misleading empty shells" is
closed by this README. Per-language SDK generation lands as
separate tickets driven by actual consumer demand.
