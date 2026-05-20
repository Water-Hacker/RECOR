# Runbook — Anthropic token budget cap (TODO-024)

**Audit reference:** closes TODO-024 in `TODOS.md`. Stage-5 adverse-
media inference is the only paid-on-every-call surface in the
platform; without a hard ceiling, an attacker who can trigger
verification cases (TODO-002 admin-allowlist closure now in place)
can drain the operating budget.

## Configuration

`ANTHROPIC_TOKEN_CEILING` — operator-controlled hard ceiling per
process. Once `TokenBudget::total() >= ANTHROPIC_TOKEN_CEILING`, the
gateway returns `GatewayError::BudgetExceeded` and refuses further
paid calls. Fixture mode (no API key) is unaffected.

| Service | Recommended value | Reset cadence |
|---|---|---|
| `services/verification-engine` | 5_000_000 tokens / process | per process restart (effectively per day under normal deploy churn) |
| `apps/audit-reconciler` | 0 (no AI calls expected) | n/a |
| Other services | 0 | n/a |

The cap is per-process, not platform-wide. If the V-engine runs N
replicas, the platform-wide ceiling is N × per-process ceiling.
Operators must size the per-replica ceiling against the deployed
replica count.

## When the cap trips

`recor_inference_budget_exceeded_total` increments. The
`anthropic_budget_exhausted` Prometheus alert fires (see
`alerts/recor-prometheus-rules.yaml`):

```
ALERT anthropic_budget_exhausted
IF rate(recor_inference_budget_exceeded_total[5m]) > 0
FOR 1m
LABELS { severity = "page" }
ANNOTATIONS { summary = "Anthropic budget cap reached — Stage-5 verifications degrading to fixture mode" }
```

## Operator response

1. **Triage.** Inspect `recor_anthropic_tokens_used_total{purpose,model}`
   to see WHICH purpose consumed the budget. Adverse-media
   (`purpose=adverse_media`) at high rate suggests a bulk-verification
   storm; pattern-explain at high rate suggests one declarant
   producing many high-suspicion cases.
2. **Decide.** Either:
   - **Raise the ceiling** (rolling restart with a larger
     `ANTHROPIC_TOKEN_CEILING`) if the spend is legitimate and the
     monthly budget allows.
   - **Throttle inbound** (lower `RATE_LIMIT_PER_MIN` on declaration
     `POST /v1/declarations`) so the upstream stops generating new
     verification cases until the budget resets.
   - **Investigate** (cross-reference with admin-allowlist replay
     activity; rule out TODO-024-style cost-DoS attempt).
3. **Verify return-to-service.** After the restart / throttle, the
   alert should clear within 5 minutes.

## Rollback

If the budget enforcement was raised to refuse legitimate calls, the
operator can:

- Set `ANTHROPIC_TOKEN_CEILING=0` (disables the cap entirely)
- Or set a larger numeric value
- Rolling restart picks up the new ceiling

Fixture mode continues to work at any ceiling — Stage 5 silently
falls back to deterministic responses, preserving D14 fail-closed
posture (the lane decision becomes Yellow under fusion uncertainty
rather than failing the verification case).

## Related

- [docs/audit/10-findings.md § FIND-009] — Stage 5 (adverse media)
- `packages/recor-inference-gateway/src/budget.rs` — TokenBudget
  module
- `packages/recor-inference-gateway/src/lib.rs` — gateway-side
  enforcement at the top of `messages()`
