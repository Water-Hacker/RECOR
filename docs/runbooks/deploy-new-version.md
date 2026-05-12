# Runbook — deploy a new version

End-to-end walkthrough of the supported deployment path: PR merged to
`main` → image built and signed at `ghcr.io` → cluster reconciles.

This is the **only** supported deployment path. Manual `kubectl apply`
or out-of-band image promotion is a doctrine violation (D19,
reproducible everything).

## Trigger

- A PR has been approved, all required checks are green, and you want
  the change in production.
- A previously merged change needs to be re-deployed (e.g. after a
  cluster rebuild).

## Prerequisites

- Push access to `main` (via PR merge, never direct push)
- `gh` CLI authenticated against `Water-Hacker/RECOR`
- `cosign` v2+ installed locally for image-signature verification
- `kubectl` configured against the production cluster context
  (`recor-prod`)
- Read access to ArgoCD UI
- Pager set: deployment-induced incidents are the most common kind;
  see [oncall-triage-tree](oncall-triage-tree.md) Step 3

## Procedure

The four implicated services are:

| Service | Image repo | Container port |
|---|---|---|
| Declaration | `ghcr.io/water-hacker/recor-declaration` | 8080 |
| Verification engine | `ghcr.io/water-hacker/recor-verification-engine` | 8081 |
| Declarant portal | `ghcr.io/water-hacker/recor-portal` | 8082 |

The publisher is `.github/workflows/publish-images.yaml` (delivered
by ticket CI-1). On every push to `main` it builds and keyless-signs
the three images with cosign, binding each signature to the
workflow's own OIDC identity. Provenance verification in Step 3
below is the on-call gate between "an image at the registry" and "an
image we are willing to run" — see
[image-verification](image-verification.md) for the deep dive on the
signature chain.

### Step 1 — Merge the PR

Confirm before clicking Merge:

```bash
gh pr view <PR-number> --json mergeable,statusCheckRollup \
  | jq '{mergeable, checks: (.statusCheckRollup | map(.conclusion))}'
```

Expected: `mergeable: "MERGEABLE"` and every check is `"SUCCESS"`.

If anything is `null`, `PENDING`, or `FAILURE` — do not merge. Branch
protection (`docs/security/branch-protection.md`) will refuse the
merge anyway; the dry-run above is to avoid clicking a button that
fails noisily.

Squash-merge from the GitHub UI (the only path; force-push to `main`
is forbidden per branch protection).

### Step 2 — Wait for `publish-images.yaml` to complete

```bash
# Watch the publish workflow triggered by the merge.
gh run watch --workflow=publish-images.yaml
```

Or list and inspect:

```bash
gh run list --workflow=publish-images.yaml --limit 5
gh run view <run-id> --log | tail -100
```

Expected: every job under `publish-images` is green. The workflow
produces two image tags per service:

- `:latest` (mutable; tracks `main`)
- `:${{ github.sha }}` (immutable; the deployable tag)

The immutable tag is the one we deploy. `:latest` is for
human-readability and convenience; never deploy it.

### Step 3 — Verify image provenance (dry-run first)

Before any cluster change, confirm the images exist and were signed
by the expected GitHub Actions OIDC identity. The detailed runbook
for this verification is [image-verification](image-verification.md);
the loop below is the same procedure, pre-bound to the three RÉCOR
images.

```bash
SHA=$(git rev-parse origin/main)
for img in recor-declaration recor-verification-engine recor-portal; do
  REF="ghcr.io/water-hacker/${img}:${SHA}"
  echo "Verifying ${REF}"
  cosign verify "${REF}" \
    --certificate-identity-regexp \
      'https://github.com/Water-Hacker/RECOR/\.github/workflows/publish-images\.yaml@.*' \
    --certificate-oidc-issuer="https://token.actions.githubusercontent.com" \
    > /dev/null \
    || { echo "SIGNATURE VERIFICATION FAILED FOR ${REF}"; exit 1; }
done
```

This is the dry-run that protects against deploying an unsigned or
mis-signed image. **If any image fails verification, STOP and page the
security on-call.** Do not proceed to Step 4.

### Step 4 — Confirm what ArgoCD will reconcile

```bash
kubectl -n argocd get application recor-declaration -o yaml \
  | yq '.spec.source.targetRevision, .status.sync.status, .status.health.status'
```

Repeat for `recor-verification-engine` and `recor-portal`.

Expected:

- `targetRevision: main`
- `status.sync.status: OutOfSync` (because the new commit just merged)
- `status.health.status: Healthy` (the current running version is still
  healthy)

If `targetRevision` is anything other than `main`, the ArgoCD app is
pinned to a specific revision (typically by an in-flight rollback);
investigate before proceeding.

> The Argo Application manifests for the three Rust/TS services do
> not yet exist; `infrastructure/argocd/observability.yaml` is the
> only one shipped today. The application manifests are **TBD —
> depends on R-OPS-DEPLOY** (the ArgoCD rollout ticket). Until they
> land, the equivalent commands are `helm upgrade --install` invoked
> manually from a deploy bastion; see the bootstrap ops runbook.

### Step 5 — Trigger sync (auto or manual)

ArgoCD apps are configured with `syncPolicy.automated.selfHeal: true`
(see `infrastructure/argocd/observability.yaml` for the canonical
pattern). The new commit reconciles **automatically** within the
default poll interval (3 min in production).

To accelerate (e.g. for a fix-forward):

```bash
argocd app sync recor-declaration --revision $(git rev-parse origin/main)
argocd app sync recor-verification-engine --revision $(git rev-parse origin/main)
argocd app sync recor-portal --revision $(git rev-parse origin/main)
```

Or via the ArgoCD UI: click each app → Sync → Synchronize.

The deployment strategy is **rolling update** with `maxSurge: 1,
maxUnavailable: 0` (configured in the Helm chart). New pods come up,
pass health checks, and old pods drain. The whole rollout for one
service is typically 60–120 s.

### Step 6 — Watch the rollout

```bash
kubectl -n recor rollout status deploy/declaration --timeout=5m
kubectl -n recor rollout status deploy/verification-engine --timeout=5m
kubectl -n recor rollout status deploy/declarant-portal --timeout=5m
```

If any of these times out, the new pods are not becoming Ready. Go to
[rollback-deployment](rollback-deployment.md) — do NOT investigate
in production with the half-rolled deploy live.

### Step 7 — Smoke-test in production

Production smoke must pass against the deployed version, not against
an arbitrary build. From a host with network access to the production
ingress:

```bash
# Declaration service
curl -sf https://api.recor.cm/healthz | jq .
curl -sf https://api.recor.cm/readyz  | jq .
# Expect: status "ok" on both.

# Verification engine
curl -sf https://verify.recor.cm/healthz | jq .
curl -sf https://verify.recor.cm/readyz  | jq .

# Declarant portal (served by nginx; no JSON, just a 200)
curl -sIf https://app.recor.cm/ | head -1
```

> The hostnames above are the contract per `infrastructure/helm`
> values; if the cluster uses different ingress hostnames, substitute.
> The portal smoke that asserts security headers is at
> `applications/declarant-portal/scripts/headers-smoke.sh` (referenced
> in `applications/declarant-portal/CLAUDE.md` § "How to verify").

### Step 8 — Confirm telemetry

Open Grafana → `RECOR / Service health` dashboard. Confirm in the
post-deploy window (5 minutes):

- Error rate (5xx) for each service is < 0.1 %
- p99 latency for each implicated endpoint is within its CLAUDE.md SLO
- DLQ depth (`recor_outbox_dlq_size`) is not growing
- No new ERROR-level log spikes in Loki for the three services

If any of these breaches its budget for > 3 minutes:

→ Go to [rollback-deployment](rollback-deployment.md).

### Step 9 — Announce

Post in `#deploys-recor`:

```
Deployed <sha-short> to production at <UTC-time>.
PR: <link>
Services: declaration, verification-engine, declarant-portal
Smoke: clean
Telemetry: clean
On-call for next 24 h: <handle>
```

The 24-hour ownership window is yours; the next deploy is blocked
until you have either declared the previous deploy healthy or rolled
it back.

## Verification

The deploy is **complete** when all of the following are true:

- `kubectl -n recor get pods -l app in (declaration, verification-engine, declarant-portal) -o wide`
  shows all pods in `Running` state with the new image tag
- The `Image:` field in `kubectl describe deploy <name>` matches
  `ghcr.io/water-hacker/recor-<image>:<sha>` where `<image>` is one
  of `declaration | verification-engine | portal` and `<sha>` is
  `git rev-parse origin/main`
- All three healthz / readyz endpoints respond 200
- Grafana shows no SLO breach in the 15 minutes after deploy
- The deploy is announced in `#deploys-recor`

If you cannot verify ALL FIVE of these, the deploy is **not complete**;
either finish it or roll it back.

## Rollback

If anything above failed mid-procedure:

→ [rollback-deployment](rollback-deployment.md)

A partial deploy (some services on the new image, some on the old) is
not a stable state. Do not leave one service partially rolled out; the
HMAC + canonical-form invariants between declaration and
verification-engine assume both services have been deployed from the
same monorepo commit. A mismatch can cause silent verification
failures that present as `last_error` strings on outbox rows.

## Related runbooks

- [rollback-deployment](rollback-deployment.md)
- [image-verification](image-verification.md)
- [oncall-triage-tree](oncall-triage-tree.md)
- [observability-prod-stack](observability-prod-stack.md)
- [hmac-secret-rotation](hmac-secret-rotation.md)
- [restore-database-from-backup](restore-database-from-backup.md)
- [incident-response-template](incident-response-template.md)
