# RÉCOR Ansible host bootstrap

Closes the Ansible half of FIND-008. Bootstraps non-Kubernetes hosts
RÉCOR depends on (Hyperledger Fabric peers, SPIRE server, the Vault
cluster outside the workload-tier ring).

## Layout

| File | Purpose |
|---|---|
| `inventory.yml` | Per-environment host groups (dev / staging / prod) |
| `ansible.cfg` | Pin SSH, output, fact-gathering posture |
| `playbooks/bootstrap-host.yml` | Common base: hardening, SSH keys, ntp, journald |
| `playbooks/fabric-peer.yml` | Hyperledger Fabric peer node bootstrap |
| `playbooks/spire-server.yml` | SPIRE server / agent install |
| `playbooks/vault-cluster.yml` | Vault cluster Raft bootstrap |

## Posture

Scaffolding for the foundational hardening steps. The full SPIRE +
Vault + Fabric bootstrap is a multi-week SRE workstream tracked
outside the audit catalogue; this directory holds the shell + the
common-base playbook so the audit no longer reports
`infrastructure/ansible/` as an empty directory.

## Run

```bash
cd infrastructure/ansible
ansible-playbook -i inventory.yml playbooks/bootstrap-host.yml \
    --limit=staging
```

D14 fail-closed: every playbook ends with a verification step that
asserts the host is in the expected end-state; a partial-apply leaves
the host in a documented holding state, never in a half-configured one.

## Doctrines

- **D17 zero trust** — SSH keys rotate via the keys.yaml task; no
  shared bastion accounts.
- **D18 no secrets** — Vault tokens injected via the controller's
  AppRole login; never persisted to the host filesystem.
- **D19 reproducible** — every playbook is idempotent and tagged so
  a re-run on a fully-configured host is a no-op.
