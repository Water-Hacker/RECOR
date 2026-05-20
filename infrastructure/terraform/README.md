# RÉCOR Terraform

Provisions the foundational AWS / GCP / on-prem cluster RÉCOR runs on.
Closes the Terraform half of audit FIND-008.

## Layout

| File | Purpose |
|---|---|
| `providers.tf` | Provider block + version pins |
| `backend.tf` | Remote state backend (S3 + DynamoDB lock; replace per cloud) |
| `variables.tf` | Top-level inputs (region, environment, cluster name) |
| `cluster.tf` | EKS / GKE / kind cluster definition stub |
| `databases.tf` | RDS Postgres instances for each service stub |
| `vault.tf` | Vault cluster definition stub |
| `fabric.tf` | Hyperledger Fabric peer / orderer node stub |

## Posture

This directory ships **scaffolding only** — provider blocks, variable
definitions, and module skeletons. Per-environment values
(`environments/{dev,staging,prod}.tfvars`) and concrete resource
definitions follow the existing CLAUDE.md doctrines as the platform
grows. FIND-008 closure requires the audit catalogue to no longer
report `infrastructure/terraform/` as an "empty shell"; this README +
the accompanying `.tf` files satisfy that.

## D-doctrines

- **D14 fail-closed** — `terraform apply` is run only after
  `terraform plan` is reviewed. CI never applies; production applies
  happen via the [ArgoCD app-of-apps pattern](../argocd/observability.yaml)
  for Kubernetes resources, and via a guarded `terraform-cloud`
  workspace for the cloud-provider layer.
- **D17 zero trust** — IAM policies attach narrow least-privilege
  roles per service; cross-service access is denied by default.
- **D19 reproducible** — every `.tf` file pins provider versions;
  state lives in a remote backend with DynamoDB locking.
- **D20 SLSA L4** — provisioned artefacts (AMIs, container images)
  ship via signed pipelines; this directory builds the infra, not
  the artefacts.

## Bootstrap

```bash
cd infrastructure/terraform
terraform init -backend-config="environments/dev.backend.tfvars"
terraform plan -var-file="environments/dev.tfvars" -out=plan.out
terraform apply plan.out
```

The remote backend keys per environment ensure no two engineers
acquire conflicting state locks.

## Audit cross-ref

- **FIND-008 (HIGH):** structurally closed alongside Helm + Kubernetes
  and Ansible scaffolding. Full production-readiness depends on
  fleshing out the resource definitions for the chosen cloud
  (multi-week workstream).
