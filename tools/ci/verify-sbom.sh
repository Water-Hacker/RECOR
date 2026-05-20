#!/usr/bin/env bash
#
# tools/ci/verify-sbom.sh — TODO-022 downstream-consumer verifier.
#
# Verifies that a published RÉCOR image has the full supply-chain
# attestation set: cosign signature, SPDX + CycloneDX SBOMs, SLSA
# provenance — all keyless-signed by the publish-images workflow's
# OIDC identity.
#
# Doctrine reference: D20 (supply chain integrity, SLSA L3+). This
# script is what a downstream operator (CEMAC, consumer, audit)
# would run to assure themselves that an image they are about to
# pull was produced by the canonical workflow and not tampered with.
#
# Usage:
#   tools/ci/verify-sbom.sh <image-ref-with-digest>
#
#   IMAGE_REF must be a digest reference, e.g.
#     ghcr.io/water-hacker/recor-declaration@sha256:abcd...
#
#   Optional env vars:
#     IDENTITY_REGEX   — override the cosign cert-identity regex
#                        (default matches the canonical workflow URL)
#     ISSUER           — override the OIDC issuer
#                        (default = GitHub Actions issuer)
#     WITH_TRIVY=1     — also re-run a Trivy scan against the digest
#                        (requires `trivy` on PATH); fails on any
#                        HIGH/CRITICAL with a fix available
#     OUT_DIR          — directory to extract SBOMs to
#                        (default: ./verified-sbom/<sha>)
#
# Exit codes:
#   0  — all attestations present + verified
#   1  — usage error
#   2  — signature verification failed
#   3  — SPDX SBOM attestation missing or failed
#   4  — CycloneDX SBOM attestation missing or failed
#   5  — SLSA provenance attestation missing or failed
#   6  — Trivy gate (when WITH_TRIVY=1) found a HIGH/CRITICAL CVE

set -euo pipefail

IMAGE_REF="${1:-}"
if [[ -z "${IMAGE_REF}" ]]; then
  cat >&2 <<'EOF'
Usage: tools/ci/verify-sbom.sh <image-ref-with-digest>

Example:
  tools/ci/verify-sbom.sh \
    ghcr.io/water-hacker/recor-declaration@sha256:abcd1234...

Optional env: IDENTITY_REGEX, ISSUER, WITH_TRIVY=1, OUT_DIR=...
EOF
  exit 1
fi

if [[ "${IMAGE_REF}" != *"@sha256:"* ]]; then
  echo "ERROR: image-ref must be a digest reference (got: ${IMAGE_REF})" >&2
  echo "Pass the @sha256:... form, not a :tag — only a digest is" >&2
  echo "cryptographically pinnable." >&2
  exit 1
fi

IDENTITY_REGEX="${IDENTITY_REGEX:-https://github.com/Water-Hacker/RECOR/.github/workflows/publish-images.yaml@.*}"
ISSUER="${ISSUER:-https://token.actions.githubusercontent.com}"

digest="${IMAGE_REF#*@}"
short_digest="${digest:0:19}" # `sha256:abcd1234abcd1234`
OUT_DIR="${OUT_DIR:-./verified-sbom/${short_digest}}"
mkdir -p "${OUT_DIR}"

echo "=== Verifying ${IMAGE_REF} ==="
echo "identity regex : ${IDENTITY_REGEX}"
echo "OIDC issuer    : ${ISSUER}"
echo "output dir     : ${OUT_DIR}"
echo

# ──────────────────────────────────────────────────────────────────────
# 1. Image signature
# ──────────────────────────────────────────────────────────────────────
echo "[1/4] Verifying image signature..."
if ! cosign verify \
      --certificate-identity-regexp "${IDENTITY_REGEX}" \
      --certificate-oidc-issuer "${ISSUER}" \
      "${IMAGE_REF}" \
      > "${OUT_DIR}/signature.json"; then
  echo "  FAIL: image signature does not verify" >&2
  exit 2
fi
echo "  OK: image signature verified"

# ──────────────────────────────────────────────────────────────────────
# 2. SPDX SBOM attestation
# ──────────────────────────────────────────────────────────────────────
echo "[2/4] Verifying + downloading SPDX SBOM attestation..."
if ! cosign verify-attestation \
      --type spdxjson \
      --certificate-identity-regexp "${IDENTITY_REGEX}" \
      --certificate-oidc-issuer "${ISSUER}" \
      "${IMAGE_REF}" \
      > "${OUT_DIR}/spdx-attestation.dsse.json"; then
  echo "  FAIL: SPDX attestation missing or signature does not verify" >&2
  exit 3
fi
# The DSSE bundle wraps a base64-encoded in-toto statement whose
# `predicate` is the SBOM. Extract it for downstream consumers.
jq -r '.payload' < "${OUT_DIR}/spdx-attestation.dsse.json" \
  | base64 -d \
  | jq '.predicate' \
  > "${OUT_DIR}/sbom.spdx.json"
echo "  OK: SPDX SBOM verified (${OUT_DIR}/sbom.spdx.json)"

# ──────────────────────────────────────────────────────────────────────
# 3. CycloneDX SBOM attestation
# ──────────────────────────────────────────────────────────────────────
echo "[3/4] Verifying + downloading CycloneDX SBOM attestation..."
if ! cosign verify-attestation \
      --type cyclonedx \
      --certificate-identity-regexp "${IDENTITY_REGEX}" \
      --certificate-oidc-issuer "${ISSUER}" \
      "${IMAGE_REF}" \
      > "${OUT_DIR}/cyclonedx-attestation.dsse.json"; then
  echo "  FAIL: CycloneDX attestation missing or signature does not verify" >&2
  exit 4
fi
jq -r '.payload' < "${OUT_DIR}/cyclonedx-attestation.dsse.json" \
  | base64 -d \
  | jq '.predicate' \
  > "${OUT_DIR}/sbom.cdx.json"
echo "  OK: CycloneDX SBOM verified (${OUT_DIR}/sbom.cdx.json)"

# ──────────────────────────────────────────────────────────────────────
# 4. SLSA provenance (TODO-022)
# ──────────────────────────────────────────────────────────────────────
echo "[4/4] Verifying + downloading SLSA provenance attestation..."
if ! cosign verify-attestation \
      --type slsaprovenance1 \
      --certificate-identity-regexp "${IDENTITY_REGEX}" \
      --certificate-oidc-issuer "${ISSUER}" \
      "${IMAGE_REF}" \
      > "${OUT_DIR}/slsa-attestation.dsse.json"; then
  echo "  FAIL: SLSA provenance missing or signature does not verify" >&2
  exit 5
fi
jq -r '.payload' < "${OUT_DIR}/slsa-attestation.dsse.json" \
  | base64 -d \
  | jq '.predicate' \
  > "${OUT_DIR}/slsa-provenance.json"
echo "  OK: SLSA provenance verified (${OUT_DIR}/slsa-provenance.json)"

# Surface the build-defining fields to stdout so an operator can see
# WHO built this image without opening the file.
builder_id=$(jq -r '.runDetails.builder.id // "<absent>"' \
  "${OUT_DIR}/slsa-provenance.json")
commit_sha=$(jq -r '.buildDefinition.resolvedDependencies[0].digest.gitCommit // "<absent>"' \
  "${OUT_DIR}/slsa-provenance.json")
event=$(jq -r '.buildDefinition.internalParameters.github.event_name // "<absent>"' \
  "${OUT_DIR}/slsa-provenance.json")
echo "  builder.id       : ${builder_id}"
echo "  source commit    : ${commit_sha}"
echo "  triggering event : ${event}"

# ──────────────────────────────────────────────────────────────────────
# Optional re-scan against the GHSA feed (Trivy)
# ──────────────────────────────────────────────────────────────────────
if [[ "${WITH_TRIVY:-0}" == "1" ]]; then
  echo
  echo "=== Optional GHSA gate (trivy) ==="
  if ! command -v trivy >/dev/null 2>&1; then
    echo "WARNING: WITH_TRIVY=1 but trivy is not on PATH; skipping" >&2
  else
    if ! trivy image \
        --severity HIGH,CRITICAL \
        --ignore-unfixed \
        --exit-code 1 \
        --format table \
        "${IMAGE_REF}"; then
      echo "  FAIL: trivy found HIGH/CRITICAL CVEs with fixes available" >&2
      exit 6
    fi
    echo "  OK: trivy gate clean (no fixed HIGH/CRITICAL)"
  fi
fi

echo
echo "=== All attestations verified for ${IMAGE_REF} ==="
echo "Output: ${OUT_DIR}"
