# Runbook — BUNEC adapter cutover (TODO-015)

**Doctrine reference:** D14 (fail-closed at integration boundaries),
D17 (zero trust), D19 (reproducible everything). **ADR:**
[ADR-0011 — BUNEC adapter pluggability](../adr/0011-bunec-adapter-pluggability.md).

This runbook is the step-by-step procedure to flip the verification
engine from the `mock` BUNEC adapter to the `real` adapter, the day
the cross-organisational data-sharing agreement with BUNEC is in place.

## Pre-conditions

Before starting the cutover, all four of these MUST be true:

1. **Agreement signed.** The data-sharing agreement is countersigned;
   the platform has a documented refresh cadence (used to set the
   cache TTL), an agreed JSON wire contract, and a published incident
   SLO from BUNEC's side.
2. **mTLS / Bearer authentication negotiated.** Either:
   - BUNEC has accepted the platform's SPIFFE workload IDs, OR
   - The platform has provisioned a Bearer API key with BUNEC and the
     key has been stored in Vault at `recor/v-engine/bunec`.
3. **BUNEC sandbox accessible.** The platform can reach the BUNEC
   sandbox endpoint from the staging cluster and at least one happy-
   path lookup succeeds end-to-end.
4. **Runbook acknowledged by on-call.** The on-call engineer is aware
   the cutover is happening; the outage runbook
   [`bunec-adapter-outage.md`](bunec-adapter-outage.md) has been read.

If any of the four is false, **stop**. Falling back to `mock` after a
public claim that "we query BUNEC" is a credibility-eroding event.

## Cutover procedure

1. **Verify the cluster can reach BUNEC.** From a verification-engine
   pod:
   ```sh
   curl -fsS --max-time 5 "${BUNEC_BASE_URL}/healthz" || echo "UNREACHABLE"
   ```
   If `UNREACHABLE`, **do not proceed** — file an incident with BUNEC
   liaison.

2. **Stage the manifest change.** Apply to `staging` first, never
   directly to prod:
   ```yaml
   # k8s/staging/v-engine.yaml
   spec:
     template:
       spec:
         containers:
           - name: v-engine
             env:
               - { name: BUNEC_ADAPTER_KIND, value: "real" }
               - { name: BUNEC_BASE_URL, value: "https://api.bunec.cm/v1" }
               - { name: BUNEC_TIMEOUT_SECS, value: "2" }
               - { name: BUNEC_RETRY_ATTEMPTS, value: "3" }
               - { name: BUNEC_BREAKER_CONSECUTIVE_FAILURES, value: "5" }
               - { name: BUNEC_FAIL_POLICY, value: "fail-closed" }
   ```
   The Vault loader in `main.rs` pulls `BUNEC_API_KEY` from
   `recor/v-engine/bunec`. **Do not** put the key in the env field.

3. **Apply + observe.** `kubectl rollout restart deploy/v-engine -n
   staging`. Watch the start-up log:
   ```
   info: BUNEC adapter: REAL (TODO-015 production path)
   ```
   If you see `BUNEC adapter: MOCK`, the env did not land — debug the
   manifest before proceeding.

4. **Smoke-test against the sandbox.** Submit a verification with a
   `person_id` that is known to exist in the BUNEC sandbox. The
   pipeline log should show a Stage 2 outcome that is sourced from
   the real adapter (not the mock-backed identity row).

5. **Inspect the new metrics.** Confirm `recor_bunec_calls_total`
   has a `result=success` increment. If it has `result=transport_err`
   on every call, see the outage runbook before proceeding to prod.

6. **Promote to prod.** Repeat steps 2-5 against the prod manifest.

7. **Update the platform's status page** to record the date of the
   cutover and BUNEC's SLO. The status page is the source of truth
   for external auditors asking "when did you start querying BUNEC?".

## Rolling back

If post-cutover monitoring shows sustained breaker-open events or a
verification-engine error rate above the SLO:

1. **Set the breaker policy to fail-open temporarily.**
   `kubectl set env deploy/v-engine -n prod BUNEC_FAIL_POLICY=fail-open`
   The pipeline keeps running but flags verifications with
   "BUNEC unreachable" instead of failing them outright.
2. **Investigate.** Pull `recor_bunec_calls_total` by `result` label
   and check the BUNEC status page. Open an incident with the BUNEC
   liaison.
3. **Only if BUNEC is confirmed-down for an extended period**, fall
   back to mock:
   `kubectl set env deploy/v-engine -n prod BUNEC_ADAPTER_KIND=mock`.
   This is a public-credibility decision, not a routine on-call
   action — escalate to the lead architect first.

## Verification (post-cutover audit checklist)

- [ ] Production manifest has `BUNEC_ADAPTER_KIND=real`
- [ ] Vault path `recor/v-engine/bunec` populated
- [ ] `recor_bunec_calls_total{result=success}` counter incrementing
- [ ] `recor_bunec_calls_total{result=transport_err}` counter near zero
- [ ] Breaker has not opened in the last 24h
  (`recor_bunec_breaker_state{state=open}` ≈ 0)
- [ ] Status page updated
- [ ] Incident runbook ([`bunec-adapter-outage.md`](bunec-adapter-outage.md)) handed off
  to the on-call rotation

## Cross-references

- ADR: [`docs/adr/0011-bunec-adapter-pluggability.md`](../adr/0011-bunec-adapter-pluggability.md)
- Adapter code: [`services/verification-engine/src/infrastructure/bunec_real.rs`](../../services/verification-engine/src/infrastructure/bunec_real.rs)
- Config: [`services/verification-engine/src/config.rs`](../../services/verification-engine/src/config.rs)
- Outage runbook: [`bunec-adapter-outage.md`](bunec-adapter-outage.md)
