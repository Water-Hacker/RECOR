# RÉCOR Kubernetes baseline

Closes the Kubernetes half of FIND-008. Ships the foundational
manifests every cluster needs before ArgoCD reconciles the per-service
Helm charts.

## Layout

| File | Purpose |
|---|---|
| `00-namespace.yaml` | The `recor` namespace + labels |
| `10-rbac.yaml` | ServiceAccounts + Role / RoleBinding baseline |
| `20-resource-quotas.yaml` | Per-namespace CPU / memory / pod count caps |
| `30-pod-security-admission.yaml` | PodSecurity profile labels (restricted, baseline) |

## Apply order

```bash
kubectl apply -f infrastructure/kubernetes/
kubectl apply -f infrastructure/networks/   # NetworkPolicies (FIND-007)
```

ArgoCD app-of-apps reconciles these as the first wave; the per-service
Helm releases follow.

## Doctrines

- **D14 fail-closed.** PodSecurity Admission is set to `restricted`
  on the `recor` namespace; pods that violate are admission-denied.
- **D17 zero trust.** Every ServiceAccount is least-privilege; no
  cluster-admin bindings.
- **D19 reproducible.** Manifest order is encoded in the filename
  prefix (`00-`, `10-`, etc.) so a fresh cluster bootstraps
  deterministically.
