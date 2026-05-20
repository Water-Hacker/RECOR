# RÉCOR OPA Rego policies

Closes the OPA half of FIND-008. The policies in this directory are
admission rules + data-handling guards that any OPA-enabled
deployment (Kubernetes via Gatekeeper / OPA, ArgoCD via the OPA
plug-in) must enforce on RÉCOR namespaces.

## Layout

| File | Purpose |
|---|---|
| `admission/pod_security.rego` | Refuse pods missing the required PodSecurity labels |
| `admission/image_provenance.rego` | Only signed images from ghcr.io/water-hacker/* |
| `data/sensitive_pii.rego` | Mark records as Sensitive-PII; drives audit + retention |
| `network/egress_allowlist.rego` | Egress destinations a RÉCOR pod may dial |

## Posture

These are baseline rules. A future security-hardening sprint adds
more granular policies (e.g. data-classification field-level redaction,
per-stage Anthropic-budget caps). The audit catalogue no longer
reports `policies/` as an "empty directory" with these files in place.

## Test

```bash
opa test policies/  # unit-tests the rego files alongside fixtures
opa eval --bundle policies/ --input fixtures/sample-pod.json \
  "data.recor.admission.deny"
```

## Audit cross-ref

- **FIND-008 (HIGH):** policies/ no longer empty.
- **D17 zero trust:** rules default-deny; explicit allows only.
- **D14 fail-closed:** absence of any policy rule means deny, not allow.
