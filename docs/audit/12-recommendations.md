# Remediation plan — RÉCOR forensic audit, Section 14

For each CRITICAL and HIGH finding in [`10-findings.md`](10-findings.md),
this section recommends the specific work to close it. Grouped by
cost class, ordered within each by impact.

The architect uses this as the input to the next work pass.

---

## Cheap to close (< 1 day each)

### 1. FIND-011 — Toolchain split-brain

Pick rust 1.88.0. Update `rust-toolchain.toml`, `mise.toml`,
root `Cargo.toml` `rust-version` to match. Open a PR titled
`chore(toolchain): align rust 1.88.0 across rust-toolchain / mise /
Cargo.toml`. Effort: 30 min.

### 2. FIND-003 — `ENVIRONMENT=dev` + configured OIDC double-accept

Tighten `Config::from_env` in declaration / V-engine /
person-service / entity-service:

```rust
if cfg.environment == "dev" && !cfg.oidc_issuer_url.is_empty() {
    return Err(ConfigError::DevWithOidcIsIncoherent);
}
```

Plus a regression integration test on each service. Effort: ~1 day
across all four services.

### 3. FIND-002 — `POST /v1/verifications` open to any declarant

Either remove the endpoint OR gate on `admin_principals_list()`.
Recommendation: gate on admin allowlist (consistent with DLQ
admin). The internal HMAC-authenticated path
(`/v1/internal/declaration-events`) is the legitimate entry point
for verification submissions from the declaration service. Effort:
~1 day.

### 4. FIND-019 — Bazel target in justfile is aspirational

Delete the `just build-with-bazel` target. Effort: 30 min.

### 5. FIND-015 — Worker-fabric-bridge HMAC has no rotation slot

Add `fabric_bridge_hmac_secret_old: SecretString` per the ADR-005
dual-secret pattern. Effort: ~1 day.

### 6. FIND-020 (partial) — Empty `tests/{chaos,performance,e2e}` dirs

Delete the three empty directories OR commit `README.md` placeholders
explaining the deferral. Effort: 15 min.

### 7. FIND-001 (partial — start with auth gate) — Audit-verifier unauthenticated

Add OIDC bearer auth to the audit-verifier service. Use the same
`auth_middleware` pattern as declaration. The verifier becomes
"authenticated-only verification report" for v1; the "public
verifier" design becomes a follow-up (FIND-001 option B). Effort:
~1 day for the auth gate alone.

---

## Medium effort (1-5 days each)

### 1. FIND-004 — V-engine cross-tenant case reads

Add a `declarant_principal` column to `verification_cases`
(migration); populate from the inbound `declaration_events`
payload; gate `get_verification` on
`principal == case.declarant_principal OR principal IN admin_allowlist`.
Effort: ~4 days (migration + sqlx-cache regen + handler change +
integration test).

### 2. FIND-005 — Person-service GET/search Sensitive-PII grant

Add an `owner_principal` column to `persons` (the principal who
registered the row); gate `get_person` + `search_persons` on
principal match OR admin allowlist. Search must filter by
principal until a broader permissions model lands. Effort: ~5 days
(migration + handler + integration tests).

### 3. FIND-006 — Person-service POST identity injection

Gate `register_person` on admin-allowlist initially. Add OIDC
sub → principal mapping for the registration audit log. Effort:
~2 days for the gate; the long-term fix (NDI integration) is
`requires-external-action`.

### 4. FIND-013 — V-engine OpenAPI snapshot missing

Mirror DOC-1 utoipa setup on V-engine. Commit
`docs/openapi/verification-engine.json`. Wire the drift check.
Effort: ~3 days.

### 5. FIND-012 — D↔V HMAC channel iat-bound replay window

Bind `iat` (issued-at) into the HMAC payload + enforce 5-minute
clock-skew window on receipt. Apply to both directions (D→V + V→D).
Effort: ~3 days.

### 6. FIND-017 — mTLS peer-SPIFFE-ID integration test

Author testcontainers + SPIRE-stub test that submits with wrong-
SPIFFE-ID peer → asserts 403. Effort: ~3 days.

### 7. FIND-016 — Audit-chain reconciliation cron

Author a cron-style job (cronjob in K8s OR `tokio::spawn` interval
in worker-fabric-bridge) that:
- Joins `declaration_events` against the chaincode KV by `event_id`
- Alerts on events in the event log but missing from chaincode for > N minutes
- Surfaces the alert via OBS-1 (`recor_audit_chain_drift_total`)
Effort: ~3 days.

### 8. FIND-007 — `/metrics` NetworkPolicy

Land a NetworkPolicy in `infrastructure/networks/` restricting
`/metrics` to the Prometheus scraper's pod CIDR. Effort: ~2 days
once the infrastructure-as-code workstream is unblocked.

### 9. FIND-014 — V-engine integration test files

Author `services/verification-engine/tests/{api_integration,
pipeline_integration,grpc_integration}.rs` mirroring the
declaration test suite. Effort: ~5 days.

### 10. FIND-009 — V-engine pipeline stage wiring

Update `services/verification-engine/src/application/stages/mod.rs`
to register real stages behind config switches; delete the unused
stub modules (or annotate them clearly). Effort: ~2 days for the
wiring; the real-data activation is `requires-external-action`.

---

## Expensive (> 5 days or requires external precondition)

### 1. FIND-008 — `infrastructure/{terraform,kubernetes,ansible,networks}/` and `policies/` empty

Author the Helm charts + ArgoCD applications + Terraform for the
cluster + OPA policies. Substantial pre-launch workstream. Likely
multi-week. Effort: 3-4 weeks.

### 2. FIND-010 — Architecture binders are `.docx`

Convert Architecture + Companion + Concept Note to Markdown.
One-time pass. Ongoing edits then become PR-reviewable. Effort:
~2 weeks for the conversion + review.

### 3. FIND-018 — Person + entity services need Vault + SPIFFE + internal HMAC

Mirror OPS-4 + R-LOOP-3 wiring across the two new services.
Effort: ~1 week per service.

### 4. FIND-001 option B — Audit-verifier as hash-only public surface

Re-architect the verifier to return only `{on_chain_hash}` from
chaincode + a "compute from your own canonical payload"
documentation pattern. Update runbook. Update threat-model.
Effort: ~5 days.

### 5. FIND-021 — Activate real-data verification stages

`requires-external-action`. Specifically:
- Anthropic API key + budget allocation (for Stage 5 adverse
  media via Inference Gateway)
- BUNEC API access agreement + sandbox credentials (Stage 2)
- ICIJ Offshore Leaks Database licence (Stage 5 adverse-media,
  ICIJ side)
- OpenSanctions PEP dataset ingestion schedule (Stage 4)
- OFAC + UN + EU CFSP feed ingestion schedule (Stage 3)
- NDI (Cameroonian national ID) integration agreement (Stage 2 +
  R-DECL-4 person registry)
- Production Fabric cluster + chaincode deployment (R-DECL-9)
- Production Vault deployment (OPS-4)

None of these are "code waiting to ship" — they're external
agreements that must close before the corresponding code switches
on. Each has a tracking entry in `docs/PRODUCTION-TODO.md`.

### 6. FIND-022 (Section 11) — Live-fire stress test

Run the 13 `requires-live-fire` exercises from
[`09-stress-test.md`](09-stress-test.md) against a stood-up
staging stack. Belongs in the PEN-1 vendor engagement window OR
an internal red-team cycle. Effort: ~1 week (the engineering team
running the exercises) + the vendor engagement schedule.

---

## Sequence recommendation

This is NOT a feature roadmap. It is a remediation plan ordered by
"what closes the most institutional risk per unit of work."

### Sprint 0 (this week — clears the cheap CRITICALs)

Days 1-2: FIND-003 (dev+oidc bypass), FIND-002 (V-engine submit
auth), FIND-001 partial (audit-verifier auth gate), FIND-015
(bridge HMAC rotation slot), FIND-011 (toolchain align), FIND-019
(remove Bazel target), FIND-020 (clean empty test dirs).

By end of week, six of the cheap items are closed. **No critical
finding remains in `cheap`.**

### Sprint 1 (next 2 weeks — clears the medium-effort HIGHs)

Weeks 1-2: FIND-004 (V-engine case-read tenancy), FIND-005
(person GET/search), FIND-006 (person POST), FIND-013 (V-engine
OpenAPI), FIND-012 (HMAC iat), FIND-017 (SPIFFE test), FIND-016
(reconciliation cron), FIND-014 (V-engine integration tests),
FIND-009 (stage wiring), FIND-007 (metrics NetworkPolicy after
FIND-008 progress).

By end of week 2, every HIGH finding has a closing change open or
merged. **The system is defensible against a Wave-1 pen test.**

### Sprint 2 (next 4 weeks — closes the infrastructure layer)

Weeks 3-6: FIND-008 (terraform / k8s / ansible / OPA), FIND-018
(person + entity service Vault + SPIFFE), FIND-010 (architecture
markdown conversion). Run FIND-022 (live-fire stress test) in
parallel.

By end of week 6, the system has production-grade
infrastructure-as-code and live-fire-verified failure modes.

### Sprint 3+ (external preconditions)

FIND-021 components close as their external agreements close.
Each is `requires-external-action` and cannot be calendar-
forecast from inside engineering. Track in
`docs/PRODUCTION-TODO.md`.

---

## How to read this section

The architect uses the **Cheap** column to plan the work pass
that follows this audit. The **Medium** column shapes the next
sprint or two. The **Expensive** column shapes the cluster-of-
sprints leading into launch.

The single most-impactful item, ordered by `risk-closed-per-day`:

1. **FIND-003** (dev+oidc bypass) — closes a complete
   authentication bypass in <1 day
2. **FIND-001 partial** (audit-verifier auth) — closes the
   public-PII-disclosure surface in <1 day
3. **FIND-005** (person GET/search) — closes Sensitive-PII
   over-disclosure (~5 days)
4. **FIND-004** (V-engine cross-tenant) — closes cross-tenant
   verification-case reads (~4 days)
5. **FIND-008** (infrastructure-as-code) — without this, nothing
   else gets deployed (expensive, but unblocks everything else)
