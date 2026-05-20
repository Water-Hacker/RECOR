# Runbook — supply chain (SBOM audit, Trivy overrides, drift detection)

Authoritative operating procedure for the supply-chain controls
attached to every RÉCOR container image: SPDX + CycloneDX SBOM
attestations, Trivy vulnerability gating, and registry-side drift
detection.

The producer is the workflow at `.github/workflows/publish-images.yaml`
(tickets CI-1 + CI-2). On every merge to `main` it:

1. Builds the three images (`recor-declaration`,
   `recor-verification-engine`, `recor-portal`).
2. Scans each with Trivy (HIGH+CRITICAL, fixed-only) and fails the
   job on any unsuppressed finding.
3. Generates two SBOMs per image (SPDX-JSON and CycloneDX-JSON).
4. Cosign-signs the image and attaches both SBOMs as keyless-signed
   in-toto attestations.
5. Uploads the SARIF to GitHub code-scanning and the raw artefacts
   (`sbom-*.spdx.json`, `sbom-*.cdx.json`, `trivy-*.json`,
   `trivy-*.sarif`) under the workflow run for 90 days.

This runbook tells on-call how to use those outputs.

## When this runbook fires

- Pre-deploy provenance audit (every promotion to staging or
  production)
- A consumer (auditor, regulator, downstream integrator) asks for the
  SBOM of a deployed image
- A Trivy finding has triggered the publish-images gate and the
  on-call needs to decide between *fix*, *wait for fix*, or *suppress
  with justification*
- Drift suspected on a deployed image (digest no longer matches the
  digest signed by the workflow)
- Quarterly supply-chain review

For pure image-signature verification, use the companion runbook
`docs/runbooks/image-verification.md`.

## What gets published per image

| Artefact | Where | Format | Signed |
|---|---|---|---|
| Image | `ghcr.io/water-hacker/recor-*:${sha}` | OCI manifest | cosign keyless |
| SPDX SBOM | cosign attestation on the image | SPDX-JSON 2.3 | cosign keyless |
| CycloneDX SBOM | cosign attestation on the image | CycloneDX 1.5 JSON | cosign keyless |
| SLSA provenance (TODO-022) | cosign attestation on the image | SLSA-Provenance v1.0 in-toto | cosign keyless |
| Trivy SARIF | GitHub code-scanning (Security tab) | SARIF 2.1 | n/a |
| Trivy JSON + SARIF + both SBOMs + SLSA provenance | Workflow run artefact `supply-chain-<image>` | raw files | n/a |

### Downstream-consumer verifier — `tools/ci/verify-sbom.sh`

Anyone (a downstream operator, an auditor, a CEMAC reviewer) can
re-derive the full attestation chain for a published image:

```sh
tools/ci/verify-sbom.sh ghcr.io/water-hacker/recor-declaration@sha256:<digest>
```

The script verifies the image signature, both SBOM attestations, and
the SLSA provenance — all keyless-bound to this workflow's OIDC
identity — and writes the verified SBOMs + provenance to a local
directory. Optional `WITH_TRIVY=1` re-runs the GHSA-feed gate. See
the script header for full options.

### SLSA assurance ceiling

The publish-images workflow targets **SLSA L3**: GitHub-hosted runner
is the trusted builder, provenance is cryptographically signed by the
workflow's OIDC identity, and the predicate names the source commit
plus the workflow file path. **SLSA L4** (hermetic + reproducible
builds, third-party verifiable build environment) is not achievable
on hosted GitHub runners; the next-step ceiling — a self-hosted
hermetic builder with a deterministic Cargo lockfile and a SOURCE_DATE_EPOCH-
honouring `Dockerfile` — is tracked in the supply-chain backlog.

The two cosign attestations and the image signature all bind to the
same certificate identity:
`https://github.com/Water-Hacker/RECOR/.github/workflows/publish-images.yaml@<ref>`.
A successful `cosign verify-attestation` therefore proves the SBOM
was produced by this workflow on a GitHub-hosted runner — the same
guarantee as the image signature.

## How to audit an SBOM for a deployed image

Prerequisite: `cosign` 2.x and `jq` on the workstation. See
`docs/runbooks/image-verification.md` for the install command.

### 1. Resolve the deployed digest

```bash
IMAGE=ghcr.io/water-hacker/recor-declaration:latest
DIGEST=$(cosign triangulate --type digest "${IMAGE}")
echo "${DIGEST}"
# → ghcr.io/water-hacker/recor-declaration@sha256:abcd...
```

For production manifests always pin the `:${sha}` tag; never
`:latest`. The digest you audit MUST match the digest the cluster
is running.

### 2. Verify the attestation chain

```bash
cosign verify-attestation \
  --type spdxjson \
  --certificate-identity-regexp 'https://github.com/Water-Hacker/RECOR/.github/workflows/publish-images.yaml@.*' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com \
  "${DIGEST}"
```

Repeat with `--type cyclonedx` for the OWASP-format SBOM. Both must
succeed before the SBOM payload is trusted.

### 3. Download the SBOM payload

`cosign download attestation` returns one in-toto bundle per
attestation found. The SBOM is in the `predicate` field, base64-
encoded inside the DSSE envelope.

```bash
cosign download attestation "${DIGEST}" \
  | jq -r 'select(.payloadType == "application/vnd.in-toto+json") | .payload' \
  | base64 -d \
  | jq 'select(.predicateType | test("spdx")) | .predicate' \
  > recor-declaration.spdx.json
```

For CycloneDX swap the predicateType filter:

```bash
cosign download attestation "${DIGEST}" \
  | jq -r 'select(.payloadType == "application/vnd.in-toto+json") | .payload' \
  | base64 -d \
  | jq 'select(.predicateType | test("cyclonedx")) | .predicate' \
  > recor-declaration.cdx.json
```

### 4. Inspect the SBOM

Top-level questions an auditor will ask, with the jq one-liners:

**Total component count (SPDX):**

```bash
jq '.packages | length' recor-declaration.spdx.json
```

**Components by license (SPDX):**

```bash
jq '[.packages[].licenseConcluded] | group_by(.) | map({license: .[0], count: length}) | sort_by(-.count)' \
  recor-declaration.spdx.json
```

**Components by name + version (CycloneDX):**

```bash
jq '.components[] | {name, version, purl}' recor-declaration.cdx.json
```

**Search for a specific dependency (e.g. openssl):**

```bash
jq '.packages[] | select(.name | test("openssl"; "i"))' \
  recor-declaration.spdx.json
```

**Tool that generated the SBOM (provenance of the SBOM itself):**

```bash
jq '.creationInfo.creators' recor-declaration.spdx.json
```

The `creators` field includes the syft version used by the
`anchore/sbom-action` running in CI. Auditors who require an SBOM
"of known toolchain" reference this field plus the cosign attestation
proving the SBOM came from our workflow.

## How to verify a deployed image hasn't drifted

Drift = the image running in the cluster has a digest other than the
one published by `publish-images.yaml`. Causes include manual `docker
push` (forbidden), registry compromise, or mid-air manifest swap.

### 1. Capture the running digest

```bash
# Kubernetes
kubectl get pod -n recor-prod -l app=declaration \
  -o jsonpath='{.items[0].status.containerStatuses[0].imageID}'
# → ghcr.io/water-hacker/recor-declaration@sha256:abcd...
```

### 2. Capture the published digest

```bash
gh run list --workflow publish-images.yaml --branch main --limit 1 \
  --json conclusion,headSha,databaseId
# Take the `headSha` of the most recent successful run.
```

```bash
SHA=<headSha>
cosign triangulate --type digest \
  "ghcr.io/water-hacker/recor-declaration:${SHA}"
```

### 3. Compare

If the two digests match exactly, the cluster is running what the
workflow signed. If they diverge:

1. Run `cosign verify` on the running digest using the identity regex
   from `docs/runbooks/image-verification.md`. If verification fails,
   open a P1 incident (`docs/runbooks/incident-response-template.md`)
   and treat it as suspected supply-chain compromise.
2. If verification succeeds but the digest differs, it's an
   *older* image — check whether a rollback was performed and
   reconcile with the deploy log.

### 4. Cross-check the SBOM matches the running image

Every SBOM attestation includes the image digest in its subject. A
drift-resistant audit must verify the SBOM's subject digest equals
the running digest:

```bash
RUNNING_DIGEST=sha256:abcd...
cosign download attestation \
  "ghcr.io/water-hacker/recor-declaration@${RUNNING_DIGEST}" \
  | jq -r 'select(.payloadType == "application/vnd.in-toto+json") | .payload' \
  | base64 -d \
  | jq '.subject[].digest.sha256' \
  | grep -F "${RUNNING_DIGEST#sha256:}" \
  && echo "SBOM subject matches running digest" \
  || { echo "SBOM subject MISMATCH — investigate"; exit 1; }
```

## How to override a Trivy false-positive finding

The Trivy gate (`exit-code: 1` on HIGH+CRITICAL with a fix) is
non-negotiable per doctrine D14. The override path exists for
*genuine* false positives — typically:

- A CVE that affects a code path our build does not include (Trivy
  scans the package; it can't know we never call the vulnerable
  function).
- A CVE whose upstream fix breaks our toolchain in a way that's
  *worse* than the vulnerability, and a real fix is in flight.

It does NOT exist to silence a finding so the deploy can proceed.
That is a D7 violation; the reviewer will reject the PR.

### Procedure

1. **Open a tracking ticket** before adding the entry. The ticket
   captures: CVE ID, affected package + version, why the finding is a
   false positive (or why the fix is worse than the vuln), the
   upstream issue link, and the *expiry date* by which the entry
   must be removed (default: 30 days from today).

2. **Add a single line to `.trivyignore` at repo root.** Format:

   ```
   CVE-2026-1234  # RECOR-1234: <one-line rationale>; expires 2026-06-12
   ```

   The comment is not optional. A reviewer who finds a `.trivyignore`
   entry without a ticket reference and an expiry will block the PR.

3. **Add a row to the table below** ("Active suppressions") in the
   same PR. The runbook is the human-readable index; `.trivyignore`
   is the machine-readable filter. They must agree.

4. **Get review.** A `.trivyignore` change requires review from the
   security-engineer + infrastructure-engineer (per CODEOWNERS for
   this path). One reviewer is not enough.

5. **Open the removal PR before the expiry date.** Either the
   upstream fix has landed and you remove the entry, or you renew the
   entry with a new ticket and a new expiry date. Stale entries are
   technical debt that compounds; budget the work.

### Active suppressions

(empty — no suppressions in effect)

When this table becomes non-empty, the row shape is:

| CVE | Package | Image(s) | Why | Tracking | Expires |
|---|---|---|---|---|---|
| `CVE-2026-1234` | `foo@1.2.3` | `recor-declaration` | upstream regression in 1.2.4 breaks libfoo; PR open at <link> | RECOR-1234 | 2026-06-12 |

The expiry-date column is the SLO. A row older than its expiry blocks
the next supply-chain quarterly review.

## How to triage a live Trivy failure on the publish-images workflow

1. Click the failed job in GitHub Actions; open the "trivy scan —
   SARIF + fail-closed gate" step. The failing finding is in the
   logs and on the Security tab (under `trivy-<image>` category).
2. Identify the CVE, the affected package, and whether a fixed
   version exists.
3. Decide between three paths:

   - **Fix it (preferred):** bump the dependency in `Cargo.toml` /
     portal `package.json` / Dockerfile base image, re-run the
     workflow. This is D7's expected path.
   - **Wait for a fix:** if Trivy says "no fix available", the
     `ignore-unfixed: true` flag already filtered it out. If you're
     seeing the failure anyway, the fix IS available somewhere — go
     to the "Fix it" path.
   - **Suppress as false-positive:** follow the override procedure
     above. This is the last resort.

4. If the on-call cannot complete a fix within the deploy window,
   the deploy is blocked. Promote nothing. Open an incident if the
   blocked deploy is itself an incident-response action.

## Re-running the workflow without a code change

For base-image CVE refreshes (a new Debian patch landed upstream
since the last image build):

```bash
gh workflow run publish-images.yaml --ref main
```

The build will re-pull the base layers, re-scan, re-SBOM, re-sign,
and re-attest. Because the digest changes when base layers change,
the new run produces a new attestation; the old attestations for
prior digests remain at the registry but are not used by anything
pinning the new SHA.

## Related

- `.github/workflows/publish-images.yaml` — the workflow that
  produces every artefact described here
- `.trivyignore` — the machine-readable counterpart to the "Active
  suppressions" table
- `docs/runbooks/image-verification.md` — image-signature
  verification (the prerequisite for trusting any SBOM attestation)
- Ticket: `docs/PRODUCTION-TODO.md` § CI-2
- Follow-up: CI-3 (branch protection wired to required checks)
- Architecture: V5 P21 § Supply chain integrity (target SLSA L4)
