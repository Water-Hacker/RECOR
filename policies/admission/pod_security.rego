# OPA admission rule: refuse pods that don't meet RÉCOR's
# PodSecurity contract. D14 fail-closed — the `deny` set is the
# union of every individual rule; emptiness ⇒ admit.
package recor.admission

import future.keywords.contains
import future.keywords.if
import future.keywords.in

# Refuse privileged containers outright.
deny contains msg if {
    input.request.kind.kind == "Pod"
    container := input.request.object.spec.containers[_]
    container.securityContext.privileged == true
    msg := sprintf(
        "container %v is privileged — refused by recor.admission.pod_security",
        [container.name],
    )
}

# Require runAsNonRoot on every container.
deny contains msg if {
    input.request.kind.kind == "Pod"
    container := input.request.object.spec.containers[_]
    not container.securityContext.runAsNonRoot == true
    msg := sprintf(
        "container %v must set securityContext.runAsNonRoot=true",
        [container.name],
    )
}

# Refuse hostNetwork / hostPID / hostIPC on RÉCOR pods.
deny contains msg if {
    input.request.kind.kind == "Pod"
    input.request.object.spec.hostNetwork == true
    msg := "hostNetwork pods are not allowed in recor namespace"
}
deny contains msg if {
    input.request.kind.kind == "Pod"
    input.request.object.spec.hostPID == true
    msg := "hostPID pods are not allowed in recor namespace"
}
deny contains msg if {
    input.request.kind.kind == "Pod"
    input.request.object.spec.hostIPC == true
    msg := "hostIPC pods are not allowed in recor namespace"
}
