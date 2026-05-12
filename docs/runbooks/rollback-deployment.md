# Runbook — roll back a deployment

How to revert production from a bad deploy back to the last known-good
version. Faster than fix-forward; preferred whenever the user-visible
symptom started inside the deploy window.

## Trigger

Any of:

- The post-deploy smoke in [deploy-new-version](deploy-new-version.md)
  Step 7 failed
- Telemetry shows an SLO breach within 15 minutes of a deploy that
  was not present before
- [oncall-triage-tree](oncall-triage-tree.md) Step 3 identified a
  recent deploy as the likely cause of an active incident
- A post-deploy test or external report surfaces a regression you
  cannot quickly diagnose

**Doctrine reminder.** D14 (fail-closed): when in doubt, roll back. A
rolled-back deploy is a recoverable inconvenience; a half-debugged
production state is a sustained outage. The decision threshold for
rolling back is much lower than the decision threshold for
investigating-live.

## Prerequisites

- The previous good image tag (the immutable SHA tag, NOT `:latest`)
- `kubectl` and ArgoCD CLI configured against the production cluster
- `gh` CLI authenticated
- An open incident channel (rolling back IS an incident — declare one
  via [incident-response-template](incident-response-template.md))

## Procedure

### Step 0 — Declare the incident

```
Post in #oncall-recor:
ROLLING BACK <service or "all services"> from <bad-sha> to <previous-good-sha>.
Reason: <one-line, what is broken>
ETA: ~5 min to image-level rollback; ~5 min to verify.
```

The visible declaration matters. If you start the rollback silently
and it doesn't fix the symptom, the team will believe the deploy is
still rolling forward. See
[incident-response-template](incident-response-template.md) Step 1.

### Step 1 — Identify the previous good SHA

```bash
gh api repos/Water-Hacker/RECOR/commits?sha=main&per_page=10 \
  | jq -r '.[] | "\(.sha[0:7])  \(.commit.message | split("\n")[0])"'
```

Or via git locally (assumes your local main is up to date):

```bash
git -C /path/to/RECOR log --oneline -10 origin/main
```

The "previous good" SHA is the most recent merged commit **before** the
bad one. If three commits merged in close succession and you do not
know which is bad, roll back to before all three, then bisect on a
follow-up.

Lock it in a variable for the rest of this procedure:

```bash
BAD_SHA=<the-sha-currently-deployed-and-failing>
GOOD_SHA=<the-previous-good-sha>
echo "Rolling back from ${BAD_SHA} to ${GOOD_SHA}"
```

### Step 2 — Dry-run: confirm the good image exists

Never roll back to an image that does not exist. The publisher
workflow tags every build `:${{ github.sha }}` (immutable) plus
`:latest` (mutable), so the SHA tag is normally available for as
long as ghcr.io retains it; but registry retention is finite and the
image may have been garbage-collected. If so, you must fix-forward
instead.

```bash
for img in recor-declaration recor-verification-engine recor-portal; do
  REF="ghcr.io/water-hacker/${img}:${GOOD_SHA}"
  echo "Checking ${REF}"
  docker manifest inspect "${REF}" > /dev/null \
    && echo "  exists" \
    || { echo "  MISSING — abort rollback for this service"; exit 1; }
done
```

Also verify the image signature (same as forward deploy; see
[image-verification](image-verification.md) for the deep dive):

```bash
for img in recor-declaration recor-verification-engine recor-portal; do
  REF="ghcr.io/water-hacker/${img}:${GOOD_SHA}"
  cosign verify "${REF}" \
    --certificate-identity-regexp \
      'https://github.com/Water-Hacker/RECOR/\.github/workflows/publish-images\.yaml@.*' \
    --certificate-oidc-issuer="https://token.actions.githubusercontent.com" \
    > /dev/null \
    || { echo "BAD SIG ON ROLLBACK TARGET ${REF} — DO NOT DEPLOY"; exit 1; }
done
```

### Step 3 — Pin ArgoCD to the good SHA

ArgoCD's `targetRevision` defaults to `main`. To roll back, override it
to point at the good commit so auto-sync rebuilds with the old image
even though `main` has the bad code.

```bash
argocd app set recor-declaration --revision "${GOOD_SHA}"
argocd app set recor-verification-engine --revision "${GOOD_SHA}"
argocd app set recor-portal --revision "${GOOD_SHA}"
```

Then sync immediately:

```bash
argocd app sync recor-declaration
argocd app sync recor-verification-engine
argocd app sync recor-portal
```

> Until the per-service ArgoCD Applications ship (R-OPS-DEPLOY), the
> equivalent manual command is:
>
> ```bash
> helm -n recor upgrade recor-declaration \
>   infrastructure/helm/declaration \
>   --set image.tag="${GOOD_SHA}" \
>   --wait --timeout=5m
> ```
>
> Repeat per service. This path is bootstrap-only; once R-OPS-DEPLOY
> lands, the ArgoCD path above is the supported one.

### Step 4 — Watch the rollback complete

```bash
kubectl -n recor rollout status deploy/declaration --timeout=5m
kubectl -n recor rollout status deploy/verification-engine --timeout=5m
kubectl -n recor rollout status deploy/declarant-portal --timeout=5m
```

Kubernetes treats the rollback as a normal rolling update — new pods
come up on the old image, old (broken) pods drain. The order matters
only if the bad deploy introduced a wire-format incompatibility (see
the canonical-form parity rule in
`applications/declarant-portal/CLAUDE.md`); in that case roll back
declaration and verification-engine **before** the portal, because
the portal continues to talk to the deployed services.

### Step 5 — Re-run smoke (production)

Same commands as [deploy-new-version](deploy-new-version.md) Step 7:

```bash
curl -sf https://api.recor.cm/healthz | jq .
curl -sf https://api.recor.cm/readyz  | jq .
curl -sf https://verify.recor.cm/healthz | jq .
curl -sf https://verify.recor.cm/readyz  | jq .
curl -sIf https://app.recor.cm/ | head -1
```

Confirm all return 200 / `"ok"`. If smoke is still failing AFTER the
rollback, the bad SHA is not the cause — escalate per
[oncall-triage-tree](oncall-triage-tree.md).

### Step 6 — Confirm telemetry recovers

Open Grafana → `RECOR / Service health`. The bad deploy's
error-rate / latency spike should be returning to baseline within 3-5
minutes of pod replacement. If it does not, two cases:

1. **The rollback worked but a side-effect persists** — e.g. the bad
   deploy wrote bad data to Postgres that the rolled-back code still
   reads. Inspect data integrity: are any outbox / DLQ rows from the
   bad-deploy window suspect? See
   [dlq-inundation](dlq-inundation.md) and possibly
   [restore-database-from-backup](restore-database-from-backup.md).
2. **The bad SHA was misidentified** — return to Step 1 and pick an
   earlier commit. Do not roll forward into the bad SHA looking for
   the cause; that wastes time and prolongs the incident.

### Step 7 — Open the revert PR

A rolled-back ArgoCD pin is a temporary state. Within 24 hours, the
bad commit must be reverted on `main` so `targetRevision: main` is
safe to restore.

```bash
cd /path/to/RECOR
git checkout main
git pull --ff-only
git revert "${BAD_SHA}"
# Conventional Commits subject; revert prefix is the convention.
git push -u origin "revert-${BAD_SHA:0:7}"
gh pr create \
  --title "revert: <one-line of bad commit subject>" \
  --body "Rolling back ${BAD_SHA:0:7}. See incident INC-<id>."
```

The revert PR goes through the normal review path — branch protection
does not exempt reverts. Once merged, restore the ArgoCD pin to
`main`:

```bash
argocd app set recor-declaration --revision main
argocd app set recor-verification-engine --revision main
argocd app set recor-portal --revision main
```

The next sync will reconcile to the post-revert `main`, which is byte
-equivalent to `GOOD_SHA`. No further action needed.

### Step 8 — Write the post-mortem

Per [incident-response-template](incident-response-template.md). A
rollback is by definition at least SEV-2 (deploy-induced production
degradation). Action items typically include:

- Improve the CI check that should have caught the bad commit
- Add a smoke step that exercises the failing path before merge
- If the deploy-window symptom was missing from telemetry, add a metric

## Verification

The rollback is complete when:

- All three deployments are running the `${GOOD_SHA}` image (image
  names are `recor-declaration`, `recor-verification-engine`,
  `recor-portal`):
  ```bash
  for svc in declaration verification-engine declarant-portal; do
    kubectl -n recor get deploy "${svc}" -o jsonpath='{.spec.template.spec.containers[0].image}'
    echo
  done
  # Each line should match:
  #   ghcr.io/water-hacker/recor-declaration:<GOOD_SHA>
  #   ghcr.io/water-hacker/recor-verification-engine:<GOOD_SHA>
  #   ghcr.io/water-hacker/recor-portal:<GOOD_SHA>
  ```
- All healthz / readyz endpoints return 200
- Grafana shows the error-rate / latency spike has resolved
- The revert PR is opened (it does not need to be merged before this
  runbook closes; the 24-hour clock is in Step 7)
- The incident channel notes the rollback timestamp

## Rollback

The rollback of a rollback is a fix-forward, executed as a normal
forward deploy via [deploy-new-version](deploy-new-version.md). Do not
"undo" Step 3 by setting `targetRevision` back to `main` while `main`
still has the bad commit — that would re-deploy the broken version.

If the rollback itself made things worse (an old SHA's incompatibility
with current data, for instance), the correct path is forward to a
**third** SHA that addresses both — escalate, do not improvise.

## Related runbooks

- [deploy-new-version](deploy-new-version.md)
- [image-verification](image-verification.md)
- [oncall-triage-tree](oncall-triage-tree.md)
- [dlq-inundation](dlq-inundation.md)
- [restore-database-from-backup](restore-database-from-backup.md)
- [incident-response-template](incident-response-template.md)
- [observability-prod-stack](observability-prod-stack.md)
