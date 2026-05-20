# Runbook — SPIFFE trust-bundle refresh cadence

**Doctrine:** D17 (zero trust at every network boundary).
**Audit reference:** closes the Cryptography row of the MEDIUM/LOW
summary table — "SPIFFE trust-bundle refresh cadence not documented."

See also `docs/runbooks/spiffe-onboarding.md` for first-time setup of
the SPIRE server + per-service agents.

## Refresh cadence

The trust bundle is a JWKS-style document mapping each trust-domain CA
to its current public key. SPIRE rotates these keys automatically; each
service's `recor-spiffe` client subscribes to the SPIFFE Workload API
and receives bundle updates in-process without restart.

| Layer | Refresh cadence | Mechanism |
|---|---|---|
| Service in-memory bundle | every 60s | `recor-spiffe::SpiffeClient` polls Workload API; updates atomically |
| SPIRE-agent local bundle cache | every 30s | SPIRE agent → SPIRE server gRPC stream |
| SPIRE-server CA root key | every 24h (default) | SPIRE server's built-in upstream CA rotation |
| Trust-domain federation bundle (cross-cluster) | every 6h | Federated bundle sync between trust domains (BUNEC chaincode peer, etc.) |

Operators **do not** rotate the trust bundle manually except during
incident response. The cadences above are the steady state.

## When to force a refresh manually

| Trigger | Action | Severity |
|---|---|---|
| SPIRE-server CA root key compromise (suspected) | Force CA rotation; bounce all SPIRE agents | critical |
| Trust-domain federation peer changes their CA | Re-fetch the federated bundle | high |
| Operator removes a service from the SPIFFE registry | Refresh the bundle so peers stop accepting the retired service's SVID | medium |
| Routine drill | Force a rotation in staging; observe steady-state recovery | low |

## Force a CA root rotation (incident response)

```bash
# 1. Generate a new upstream CA bundle on the SPIRE server.
kubectl exec -n spire-system spire-server-0 -- \
    /opt/spire/bin/spire-server \
    upstream-authority rotate \
    -socketPath /tmp/spire-server/private/api.sock

# 2. Verify the new bundle has propagated to every SPIRE agent.
for node in $(kubectl get nodes -o name); do
    kubectl exec -n spire-system "${node#node/}-spire-agent" -- \
        /opt/spire/bin/spire-agent \
        api fetch jwt \
        -socketPath /tmp/spire-agent/public/api.sock
done

# 3. Verify every recor-* service has picked up the new bundle.
#    The metric recor_spiffe_bundle_age_seconds should drop to <60s on
#    every service shortly after the rotation completes.
```

## Federation refresh (cross-cluster)

For trust-domain federation (e.g. the BUNEC chaincode peer in a
separate cluster):

```bash
# Fetch the federated trust-domain's current bundle.
curl -sf https://spire.bunec.cm/.well-known/spiffe-bundle.json \
    -o /tmp/bunec-bundle.json

# Update the SPIRE server's federated bundle.
kubectl exec -n spire-system spire-server-0 -- \
    /opt/spire/bin/spire-server \
    federation update \
    -id spiffe://bunec.cm \
    -bundlePath /tmp/bunec-bundle.json
```

## Post-refresh verification

1. Every recor-* service's metric
   `recor_spiffe_bundle_age_seconds` returns to baseline (<60s).
2. The mTLS peer-SPIFFE-ID gate test
   (`services/verification-engine/tests/peer_spiffe_id_gate.rs`)
   passes against the cluster's actual SPIRE agent (`#[ignore]`-gated;
   un-ignore for ad-hoc verification).
3. The audit-immutability smoke (`tests/contract/audit-immutability.sh`)
   passes — confirms no service started rejecting peers that should
   still be accepted.
4. Log the rotation in `docs/audit/rotation-log.md`.

## Failure modes

| Symptom | Likely cause | Remediation |
|---|---|---|
| `recor_spiffe_bundle_age_seconds` climbs unbounded | Workload API connectivity lost | Check SPIRE agent reachability; restart agent socket sidecar |
| mTLS handshake failures across the platform | Federated bundle out of sync; client trusts an old CA | Force-refresh federated bundle (steps above) |
| Single service rejects every peer with `unknown_peer_spiffe_id` | Allowlist drifted | Reconcile `INTERNAL_PEER_SPIFFE_IDS` env vs current registry; redeploy |
| New service can't reach the platform after deploy | SVID not yet issued by SPIRE | Verify the service's SPIFFE registration; `spire-server entry show` |

## On-call hooks

- Page on: any service's `recor_spiffe_bundle_age_seconds > 300`
  (alert rule `spiffe_bundle_stale` in
  `alerts/recor-prometheus-rules.yaml`).
- Ticket on: federation peer changes — incoming notification from
  BUNEC ops triggers the federation-refresh procedure above.
