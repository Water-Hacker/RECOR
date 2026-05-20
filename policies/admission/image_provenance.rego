# Refuse pods whose images don't come from the RÉCOR registry
# AND aren't pinned by digest. D20 supply-chain integrity.
package recor.admission

import future.keywords.contains
import future.keywords.if

allowed_registries := [
    "ghcr.io/water-hacker/",
    # Operator-managed mirror for upstream images.
    "ghcr.io/water-hacker/mirror/",
]

deny contains msg if {
    input.request.kind.kind == "Pod"
    container := input.request.object.spec.containers[_]
    not has_allowed_prefix(container.image)
    msg := sprintf(
        "container %v image %v is outside the RÉCOR allowed-registry list",
        [container.name, container.image],
    )
}

deny contains msg if {
    input.request.kind.kind == "Pod"
    container := input.request.object.spec.containers[_]
    not contains_digest(container.image)
    msg := sprintf(
        "container %v image %v must be pinned by digest (D20 / SLSA L4)",
        [container.name, container.image],
    )
}

has_allowed_prefix(img) if {
    some prefix in allowed_registries
    startswith(img, prefix)
}

contains_digest(img) if {
    contains(img, "@sha256:")
}
