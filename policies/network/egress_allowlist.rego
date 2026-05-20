# Egress allowlist for RÉCOR pods. The Kubernetes NetworkPolicies in
# `infrastructure/networks/` are the enforcement layer; this Rego
# file is the audit-time declaration so a policy reviewer can read
# the egress contract without running through YAML.
package recor.network

# Hard-coded destinations RÉCOR services dial outward to.
allowed_egress_dns := {
    # OIDC issuer (per environment; placeholder).
    "idp.recor.cm",
    # Anthropic Inference Gateway (Stage 5 adverse-media).
    "api.anthropic.com",
    # Hyperledger Fabric gateway shim.
    "fabric-gateway.recor.cm",
    # Vault (when external to the workload tier).
    "vault.recor.cm",
}

# Kubernetes DNS resolves in-cluster names; required by every pod.
allowed_egress_in_cluster := {
    "kube-dns.kube-system.svc.cluster.local",
    "kubernetes.default.svc.cluster.local",
}
