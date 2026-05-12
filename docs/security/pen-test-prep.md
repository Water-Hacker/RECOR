# RÉCOR — penetration test preparation package

**Ticket:** PEN-1 (Phase 5 — Pre-launch hardening, `docs/PRODUCTION-TODO.md`)
**Audience:** the vendor security team contracted to perform the external
penetration test, and the RÉCOR engineering team that prepares the
environment, supplies the evidence base, and triages findings.
**Read before the engagement starts.** This file plus
`pen-test-rules-of-engagement.md` constitute the engineering-side
preparation. The Rules of Engagement is the legal-grade contract; this
file is the technical brief that makes the contract executable.

## Why this document exists

RÉCOR is the National Beneficial Ownership Registry of Cameroon —
sovereign infrastructure whose security posture must be defensible to
ARMP, ANIF, DGI, BEAC, customs, sectoral cadastres, CONAC, INTERPOL/StAR
and the public. The threat model in `docs/security/threat-model.md` is
self-authored by the same engineering team that built the system; gap G7
of that document explicitly acknowledges this and names PEN-1 as the
closing ticket. The external penetration test is the independent review
that converts a self-asserted security posture into an externally-
verified one.

The vendor is not auditing the platform's compliance with a standard;
the vendor is **adversarially exercising every entry point** named in
the threat model and producing one of three findings per STRIDE row:
PASS (mitigation verified), FAIL (with severity, CVSS v4 score, and
reproducible repro steps), or ACCEPTED-RISK (gap already documented and
still in scope). The engagement is therefore tightly bounded by the
threat model — the vendor follows it, does not invent new scope, and
reports against it row by row.

The doctrines that bind this engagement: D07 (no workarounds — every
threat-model gap maps to a real test objective, not a paper one), D14
(fail-closed — the engagement explicitly forbids destructive actions),
D15 (cryptographic provenance — the vendor's deliverables are signed),
D17 (zero trust — the engineering team reproduces every finding
independently before accepting it), D24 (the standard is non-negotiable
— the engagement is not "best-effort" pentest theatre).

## Engagement scope

The engagement covers the entry points that face external network
traffic, plus the operator-surface endpoints reachable from inside the
cluster. The vendor is provisioned against a **staging environment that
is byte-parity with production** (same images, same migrations, same
config schema, seeded test data only — see "Engagement logistics" below).

### In scope

| Surface | URL (staging) | Auth requirement | Notes |
|---|---|---|---|
| Declarant portal SPA | `https://portal.staging.recor.cm` | OIDC bearer in app calls; portal itself is public-fetch | nginx serves the SPA + security-headers per `applications/declarant-portal/security-headers.conf.template`; the vendor exercises the headers, the SW/IndexedDB key-handling path (R-PORT-2 / G5), and the CSP boundary |
| Declaration service — REST | `https://api.staging.recor.cm` | OIDC bearer on `/v1/declarations*` (and dev header refused in staging) | Endpoints enumerated below; canonical contract in `docs/openapi/declaration.json` |
| Declaration service — gRPC | `grpc.staging.recor.cm:443` | OIDC bearer via auth interceptor (same verifier as REST) | gRPC reflection is **enabled on staging only** so the vendor can enumerate without the proto file; reflection is OFF in production |
| Verification engine — REST | `https://verify.staging.recor.cm` | Public endpoints `/v1/verifications*` use OIDC; `/v1/internal/*` uses HMAC | Mock-BUNEC seeded; no real BUNEC calls leave the cluster |
| DLQ admin endpoints (operator surface) | `https://api.staging.recor.cm/v1/internal/outbox-dlq*` and the V-engine mirror | OIDC bearer; principal must be in `admin_principals` allowlist; empty allowlist returns 503 fail-closed | In-cluster-reachable only in production; staging exposes them through the same path with a seeded allowlist for the engagement window |
| HMAC writeback channels | `POST /v1/internal/verification-outcomes` (declaration); `POST /v1/internal/declaration-events` (V-engine) | HMAC-SHA256 with per-direction dual-secret rotation | The vendor receives a single rotation cycle's secrets via the same out-of-band channel as the OIDC creds; the second slot is empty to test the dual-secret protocol |
| OIDC issuer (staging instance) | `https://idp.staging.recor.cm` | Vendor receives N test accounts (declarant, admin) | Issuer is treated as the IdP-vendor's responsibility (out of scope per threat model § Scope); vendor exercises **the platform's verification of the issuer**, not the issuer itself |

The complete endpoint enumeration (paths, methods, expected status
codes) is in `docs/openapi/declaration.json`. The vendor MUST verify
that the OpenAPI shipped with the test build matches the deployed surface
before testing begins (engineering checklist item below).

### Explicitly out of scope

- **DDoS / volumetric attacks.** OPS-1 ships a per-principal rate limit
  of 60 rpm with a 10-burst on POST routes (`services/declaration/src/api/rate_limit.rs`).
  The limiter is a documented denial-of-service defence; exercising it
  with synthetic floods would (a) inform nothing the threat model does
  not already cover and (b) consume the engagement window. The vendor
  MAY set `RATE_LIMIT_PER_MIN=0` for the engagement (see "Engagement
  logistics" — this is D14 reasoning: the per-principal rate limit is
  a DoS defence, not a security control, and disabling it during the
  test lets the vendor exercise endpoints without the limiter
  masking findings).
- **Pivoting into the operator workstation or corporate network.** The
  engagement is bounded to the staging cluster and its public endpoints.
  Lateral movement off the cluster is forbidden by the Rules of
  Engagement.
- **Social engineering of the operations team.** A separate engagement
  covers human-factor red-teaming on a different timeline.
- **Crypto-primitive attacks on Ed25519, BLAKE3, or HMAC-SHA256.**
  Cryptanalytic attacks on the underlying primitives are a research
  programme, not a registry pentest. The vendor MAY note primitive
  choices in the report (the post-quantum-agility gap G6 is on file
  and counts as an accepted-risk acknowledgement).
- **Identity-provider compromise.** Per threat-model § Scope, the
  OIDC issuer is the IdP vendor's responsibility; the engagement
  exercises the platform's *verification* of the issuer (alg confusion,
  JWKS handling, claim validation), not the issuer itself.
- **The Fabric audit-witness chain.** R-DECL-9 is deferred; gap G1 is
  acknowledged in the threat model. The vendor MAY include the absence
  of in-DB cryptographic chaining as an accepted-risk finding.
- **Kubernetes / Helm / ArgoCD layer.** OBS-2 / OPS-4 deliver this;
  the cluster substrate is out of scope until those tickets land.

## Adversary model

Per `docs/security/threat-model.md` § Adversary catalogue, reproduced
here so the vendor's report has a single canonical reference:

| Adversary | Capabilities | Out-of-bounds capabilities |
|---|---|---|
| **External attacker (network)** | Can speak to any public endpoint the portal or service exposes | Cannot read database directly; cannot bypass TLS (operator-terminated, out of scope) |
| **Malicious declarant** | Valid OIDC credentials; submits declarations on their own account | Cannot sign on behalf of a different principal |
| **Compromised operator workstation** | Valid admin OIDC identity AND code-edit capability | Mitigations are detective (audit log + DOC-3 incident response), not preventive |
| **Malicious or compromised dependency** | Supply-chain insertion | Mitigated by CI-1 cosign (shipped) + CI-2 SBOM + Trivy |
| **Compromised verification engine host** | Can forge HMAC-signed writeback envelopes for any case ID it sees | Mitigated by HMAC rotation cadence + per-row Ed25519 retained on every declaration event |
| **Nation-state (cryptanalytic)** | May eventually break Ed25519 / BLAKE3 | D21 (post-quantum agility) is the doctrinal commitment; not a pentest target |

The vendor's testing posture is "external attacker + malicious
declarant + compromised operator workstation" as the three actively-
exercised profiles. The supply-chain and host-compromise adversaries
are documented in the report against the existing mitigations but are
not exercised directly (CI-1 + CI-2 are the test targets for those
adversaries, and they live in CI workflows, not in the staging
runtime).

## Test objectives (numbered, each maps to a threat-model STRIDE row)

For every STRIDE row in `docs/security/threat-model.md`, the vendor
produces exactly one finding: PASS, FAIL (with severity, CVSS v4
score, and repro steps), or ACCEPTED-RISK. The objective number maps
to the threat-model component number + STRIDE letter for unambiguous
cross-reference. The vendor's final report MUST present findings in
this order so cross-checking against the threat model is mechanical.

### Objectives 1.x — Declarant portal

- **1-S**: verify that a stolen-session attack is blocked by OIDC
  short-lived tokens and that the portal never persists the private key.
- **1-T-canonical**: verify that the canonical signing payload is built
  from typed Zod-validated state, not from any string under user
  control (try Unicode-normalisation tricks, JSON key-reordering,
  whitespace injection, prototype pollution on the payload object).
- **1-T-xss**: attempt CSP bypass on every route (`script-src 'self'`,
  no `unsafe-inline`, no `unsafe-eval`); attempt DOM-clobbering via
  `id`/`name` collisions; attempt SVG-XSS via uploaded content (the
  portal does not accept uploads in v1 — verify that the absence is
  enforced, not assumed).
- **1-R**: verify Ed25519 receipt signature is bound to canonical
  bytes and is reproducible offline from the receipt PDF.
- **1-I-referer**: verify `Referrer-Policy: strict-origin-when-cross-origin`
  is enforced on every response, including error pages.
- **1-I-keystore**: probe whether the Ed25519 private key can be
  persisted (localStorage, IndexedDB, sessionStorage, BroadcastChannel,
  ServiceWorker cache) — Gap G5 acknowledged; the vendor verifies the
  current state is memory-only and notes whether any code path could
  regress this in future.
- **1-D-cpu**: attempt CSP bypass to inject CPU-consuming script.
- **1-D-flood**: confirm the OPS-1 rate limit is in effect at
  60 rpm/principal on POST `/v1/declarations` (rate-limit override
  during the engagement is documented below; this objective confirms
  the limiter is wired correctly when re-enabled).
- **1-E-permissions**: confirm `Permissions-Policy` denies every
  browser feature listed in the portal CLAUDE.md.

### Objectives 2.x — Declaration service

- **2-S-body-principal**: attempt to forge declarant identity by
  injecting `signed_by`, `principal`, or analogue fields into the
  request body; confirm the principal source is always the
  authenticated session (`INV-AUTH-PRINCIPAL-SOURCE`).
- **2-S-alg-confusion**: attempt JWT alg-confusion (sign with HS256,
  claim RS256); verify HMAC algs are refused before signature check
  (R-DECL-1 closed).
- **2-T-event-log**: attempt direct UPDATE/DELETE/TRUNCATE against
  `declaration_events` as both service role and (if any path exists)
  superuser; verify the BEFORE trigger fires regardless of role
  (COMP-2, migration `0007_audit_log_immutability.sql`).
- **2-T-outbox**: attempt to mutate an outbox row between write and
  relay; verify the transaction boundary holds.
- **2-R-repudiation**: dispute a declaration after the fact — confirm
  Ed25519 attestation + receipt hash reproduce offline.
- **2-I-pii-tracing**: drive arbitrary requests and confirm OPS-2
  redacting layer masks SPIFFE paths, UUID PII fields, and partial
  receipt hashes in all log output streams (stdout, stderr, file
  sinks if configured).
- **2-I-backup**: Gap G3 acknowledged (declaration body PII
  unencrypted at rest); vendor confirms the gap is still scoped, not
  silently regressed (e.g. no plaintext PII leaked through unexpected
  channels beyond the documented backup-tier exposure).
- **2-D-flood**: confirm OPS-1 rate-limiting covers the submit
  endpoint at the service layer (companion to objective 1-D-flood
  at the portal layer; the service-layer test exercises the limiter
  directly without the portal in the loop).
- **2-D-slowloris**: attempt slow-loris on `/healthz` and confirm
  per-route TimeoutLayer terminates the connection.
- **2-E-idempotency**: replay a request with the same `Idempotency-Key`
  and verify the previous response is returned verbatim with no new
  state mutation.

### Objectives 3.x — Verification engine

- **3-S-forged-envelope**: attempt to inject a verification-outcome
  event without the D→V HMAC; attempt cross-direction misuse
  (D→V secret on V→D path).
- **3-T-bpa**: confirm that BPA outputs are deterministic from inputs;
  attempt to produce a non-reproducible BPA by perturbing only the
  stage's internal state (no input change should change output).
- **3-R-disputed-outcome**: verify deterministic replay re-derives the
  same fusion outcome from the persisted inputs.
- **3-I-mock-bunec**: confirm the mock BUNEC fixture contains only
  synthetic data; no production PII present.
- **3-D-dlq**: drive a stage stall and confirm DLQ admin endpoints
  list / replay correctly (`services/verification-engine/src/api/dlq.rs`).
- **3-E-stage-secrets**: attempt to escalate a stage to read secret
  material outside the `Config::from_env` boundary.

### Objectives 4.x — D↔V loop

- **4-S-hmac**: attempt to forge an envelope without the shared HMAC;
  attempt timing-side-channel attack on the constant-time compare.
- **4-S-rotation**: exercise the dual-secret rotation window;
  confirm the runbook in `docs/runbooks/hmac-secret-rotation.md`
  enforces close-out (old secret deactivated).
- **4-T-replay**: capture an envelope, replay it. Gap G2 acknowledged
  (replay window not bound to envelope `iat`); the vendor confirms
  the idempotency-on-event_id semantics still hold (no observable
  effect on replay).
- **4-R-denial**: confirm both sides retain the original envelope
  bytes for non-repudiation.
- **4-I-secret-tracing**: confirm HMAC secrets are wrapped in
  `SecretString` and never logged (search every log site for
  `expose_secret()` — should return zero hits outside test fixtures).
- **4-D-dlq-disk**: verify DLQ admin endpoints can drain a flooded
  queue; OBS-1 alert wiring is Phase 2 and is acknowledged.
- **4-E-cross-channel**: confirm each side reads only its own slot.

### Objectives 5.x — Auth (OIDC + dev header)

- **5-S-alg**: re-test alg-confusion at the OIDC layer
  (`services/declaration/src/api/oidc.rs`).
- **5-S-jwks-mitm**: attempt JWKS-endpoint downgrade or MITM; confirm
  HTTPS-only fetch and TTL-bounded cache.
- **5-T-claim-tampering**: attempt to modify JWT claims after signing.
- **5-R-issuer-denial**: out of scope (IdP vendor's audit log).
- **5-I-token-tracing**: confirm `sub` claim is keyed-MAC'd in logs.
- **5-D-issuer-down**: simulate IdP outage; confirm DOC-3
  `oidc-issuer-outage.md` runbook is reachable and the decision tree
  is fail-closed.
- **5-E-dev-header**: attempt to use the dev header path in staging;
  confirm `Config::from_env` refuses to start with both `ENVIRONMENT
  != dev` and an empty `OIDC_ISSUER_URL` (this is the production-
  posture check that catches deploy-time misconfig).

### Objectives 6.x — Database

- **6-S-credential**: attempt to access the database with anything
  other than the service-role credential; confirm `DATABASE_URL` is
  a `SecretString`.
- **6-T-direct-row**: re-test 2-T-event-log from the database side.
- **6-T-sqlx**: attempt SQL injection on every endpoint that accepts
  free-form text (`metadata_notes`, declarant identifiers, search
  filters); confirm sqlx parameterisation holds.
- **6-R-dba-denial**: Gap G4 acknowledged (DBA-role audit deferred
  to OBS-1); vendor confirms the gap is scoped.
- **6-I-backup**: see 2-I-backup; per-column classification in
  `docs/compliance/data-classification.md` (COMP-3) defines the
  closure path.
- **6-D-pool-exhaustion**: exercise the connection pool against the
  configured maximum; confirm per-request timeout.
- **6-E-pg-extension**: confirm only `pgcrypto` is installed; attempt
  to load an additional extension and confirm it fails.

### Objectives 7.x — Operator surface (DLQ admin)

- **7-S-allowlist**: attempt admin endpoint access with a non-allowlisted
  principal; confirm 403; attempt with an empty allowlist
  configuration; confirm 503 (fail-closed).
- **7-T-replay**: confirm DLQ replay is idempotent (no new state
  written).
- **7-R-operator-audit**: confirm operator principal + DLQ row id
  recorded in the tracing span (recoverable via OPS-2 keyed-MAC
  inverse with operations-team key).
- **7-I-dlq-content**: accepted-risk per threat model (operator
  already has admin role).
- **7-D-operator-flood**: accepted-risk per threat model (compromised-
  operator scenario).
- **7-E-cross-endpoint**: confirm admin allowlist is per-endpoint-pair
  only; no shared privilege escalation path.

### Gaps in the threat model (G1-G7)

Each gap from `docs/security/threat-model.md` § "Gaps blocking
production" is exercised as an ACCEPTED-RISK confirmation objective.
The vendor verifies the gap is still as scoped (no silent worsening)
and notes the named closing ticket. Gap G7 — threat-model independence
— closes when this engagement's report is delivered.

## Engagement logistics

- **Staging environment URL set:** all URLs in the "In scope" table
  above; final list confirmed in the kickoff session.
- **Credentials:** delivered out-of-band (PGP-encrypted to the vendor
  lead's public key listed in the Rules of Engagement). NEVER inline
  in any committed file. Per D18.
- **Test data limits:** maximum **5000 declarations** submitted during
  the engagement (this is well above any realistic test scenario; the
  cap exists so a runaway script does not exhaust staging storage).
  All declarations MUST use the seeded synthetic person UUIDs from
  `tests/e2e/fixtures.ts`; submission of real-PII synthetic-looking
  identifiers is forbidden by the Rules of Engagement.
- **Rate-limit override:** the engineering team MAY set
  `RATE_LIMIT_PER_MIN=0` on the declaration service for the
  engagement window. Doctrinal reasoning (D14): the per-principal
  rate limit is a denial-of-service defence, not a security control;
  disabling it during the test prevents the limiter from masking
  findings. Objective 1-D-flood re-enables the limiter and verifies
  it is wired correctly. The override is reverted before any
  re-test cycle.
- **gRPC reflection:** enabled on staging only for the engagement;
  reverted before the staging build is promoted to production.
- **Incident escalation during the engagement:** if the vendor
  discovers a Critical finding mid-engagement (e.g. an exploitable
  remote-code-execution path against the staging cluster, or evidence
  that a finding may also exist in production), the vendor pages the
  primary on-call via the contact graph in the Rules of Engagement.
  The on-call follows DOC-3 `oncall-triage-tree.md` and pages the
  security-team lead. Production is paused (LAUNCH-1 stage rollback
  if applicable) until the engineering team confirms the finding is
  staging-only or has rolled out a mitigation.

## Reporting

The vendor delivers:

1. **A primary report** — encrypted, PGP-signed PDF, one finding per
   STRIDE objective above, in the order documented above. Each FAIL
   finding contains:
   - STRIDE category (component number + S/T/R/I/D/E letter)
   - Severity (Critical / High / Medium / Low) — informed by CVSS v4
     base score but the severity reflects the vendor's contextual
     judgement, not a raw CVSS lookup.
   - CVSS v4.0 vector string + base score
   - Reproduction steps (numbered, copy-pasteable, against the
     staging environment specified above)
   - Evidence (HTTP transcripts, screenshots, log excerpts —
     redacted of any secrets per D18)
   - Remediation guidance (specific to RÉCOR's architecture; the
     vendor MAY reference the relevant ADR or threat-model row)

2. **A redacted public summary** — short markdown document suitable
   for publication at `docs/security/pen-test-report-{date}.md`
   listing findings by severity with no exploit details. Critical
   and High findings have their CVSS vectors public; reproduction
   detail stays in the primary report until the closing ticket
   ships.

3. **Signed deliverable bundle** — PGP-signed zip containing both
   reports, the contact graph, the engagement timeline, and the
   evidence corpus.

### Re-test scope

Any **Critical** or **High** finding triggers a vendor-cost re-test
on the engineering team's mitigation PR. The re-test is bounded to
the affected objective(s) only. The vendor confirms (a) the original
repro now fails and (b) no analogous bypass exists. Medium and Low
findings do not automatically trigger a re-test; the engineering team
fixes them at normal cadence and the next quarterly engagement
re-checks.

## Engineering checklist (BEFORE the engagement starts)

The checklist below MUST be all-green before the vendor begins
testing. The engineering team treats this as a hard gate; any unchecked
item is a doctrine D01 (completeness) violation and pushes the
engagement start date.

- [ ] Staging environment cleanly mirrors production (same images,
      same migrations, same config schema; only secrets and external
      endpoints differ).
- [ ] All Phase 0 + Phase 2 features deployed per LAUNCH-1's Stage 0
      entry gates (`docs/runbooks/soft-launch-playbook.md` § "Gates
      to enter Stage 0"). PEN-1's pre-requisite is the same fully-
      featured system that LAUNCH-1 needs.
- [ ] OBS-1 metrics + dashboards live (so the vendor's traffic is
      visible in Grafana; un-instrumented traffic looks like a hidden
      test and degrades the engagement's evidentiary value).
- [ ] Latest OpenAPI spec at `docs/openapi/declaration.json` matches
      the deployed staging surface byte-for-byte (the vendor
      enumerates from the spec; a mismatch means the vendor tests
      the wrong system).
- [ ] gRPC reflection enabled on staging only.
- [ ] Audit log in Fabric is being written (R-DECL-9 dependency)
      OR the absence is documented as Gap G1 acknowledged for this
      engagement.
- [ ] Threat model (`docs/security/threat-model.md`), ADRs
      (`docs/adr/*`), and runbooks (`docs/runbooks/*`) shared with
      the vendor at engagement kickoff.
- [ ] Rules of Engagement signed by both sides
      (`docs/security/pen-test-rules-of-engagement.md`).
- [ ] Bug-bounty disclosure policy drafted (separate from this
      engagement; informs how the vendor's findings flow into the
      public disclosure track).
- [ ] OPS-1 rate-limit override decision recorded in the engagement
      log (set `RATE_LIMIT_PER_MIN=0` for the window OR run with the
      limiter on and accept a slower engagement).
- [ ] Synthetic-data fixture seeded into the staging declaration
      database (`tests/e2e/fixtures.ts` person UUIDs, plus a fresh
      set of admin-allowlist principals for objectives 7-S).
- [ ] PGP key exchange complete: vendor lead's public key
      registered with the engineering team and vice versa, for
      out-of-band credential delivery and final-report signing.

## Vendor selection criteria

The vendor MUST satisfy all of the following:

- **ISO 27001 certified** (the audit body's findings are reviewable
  during procurement).
- **Demonstrated experience** with government registries OR
  financial-regulator systems. Beneficial-ownership registries are a
  small specialist field; a vendor whose portfolio is e-commerce
  pentests is unlikely to find the class of issues that matter.
- **Multi-language team** — French and English at minimum, given the
  Cameroon legal context. The vendor's lead engagement engineer
  reads the regulatory-mapping document (COMP-4) in its source
  language without translation latency.
- **Specific Rust experience** — the load-bearing services
  (declaration, verification-engine) are Rust; the vendor should be
  able to read the source. A pentest team without Rust experience
  spends the engagement learning the language rather than finding
  bugs.
- **Specific TypeScript / web-platform experience** — the declarant
  portal is React + Vite; modern-web-platform familiarity is
  table-stakes for CSP / SW / IndexedDB testing.
- **References from at least two prior comparable engagements** —
  contactable, willing to discuss scope and outcome quality on a
  procurement call.
- **NDA + Rules of Engagement signed before any access** — every
  staging credential, every internal document, every architectural
  detail crosses an NDA boundary first.

## Post-engagement

- **Re-test scope** as above (every Critical / High finding).
- **Public summary** published at `docs/security/pen-test-report-{date}.md`
  on the same day the primary report is delivered (or the next
  business day if delivered after-hours).
- **Findings tracked as tickets** in `docs/PRODUCTION-TODO.md`: one
  ticket per Critical or High finding, with the vendor's repro and
  CVSS vector quoted into the ticket. Medium and Low findings batch
  into a single follow-up ticket per service.
- **Threat-model update PR** — gap G7 closes; the engagement is cited
  as the source of independence. New gaps surfaced by the engagement
  are added to the gaps table with closing tickets named.
- **Re-engagement quarterly** OR after the next major feature drop,
  whichever is sooner. The cadence cap of one quarter prevents the
  threat model drifting from reality during a multi-quarter feature
  build. A "major feature drop" is any merge that adds a new
  service, a new external trust boundary, or a new consumer-access
  surface.

## Doctrines invoked

This engagement is the operational instantiation of:

- **D01 (completeness)** — the engagement covers every STRIDE row in
  the threat model; partial coverage is not acceptable.
- **D07 (no workarounds)** — gaps acknowledged by the threat model
  remain acknowledged; the engagement does not paper over them.
- **D14 (fail-closed)** — the engagement explicitly forbids
  destructive actions; rate-limit override is the documented
  exception with reasoning.
- **D15 (cryptographic provenance)** — the vendor's final deliverable
  is PGP-signed; the engineering team verifies the signature before
  treating the report as authoritative.
- **D17 (zero trust)** — the engineering team reproduces every
  Critical / High finding independently before accepting it; the
  vendor's claim alone is not load-bearing.
- **D18 (no secrets)** — credentials never appear in committed
  files; all out-of-band channels are encrypted.
- **D24 (the standard is non-negotiable)** — the engagement gate is
  binary: every checklist item green, or the engagement does not
  start.

## Related documents

- `docs/security/pen-test-rules-of-engagement.md` — the legal-grade
  RoE document the vendor signs before any access.
- `docs/security/threat-model.md` (DOC-4) — source of truth for
  what the engagement targets.
- `docs/security/branch-protection.md` (CI-3) — change-control
  posture; any vendor-supplied PR follows this.
- `docs/security/README.md` — index of security documentation.
- `docs/compliance/regulatory-mapping.md` (COMP-4) — legal-basis
  framing for every endpoint the vendor exercises.
- `docs/runbooks/oncall-triage-tree.md` (DOC-3) — escalation path
  during the engagement.
- `docs/runbooks/incident-response-template.md` (DOC-3) — post-mortem
  template; the engagement's report flows into this if a finding
  produces an incident.
- `docs/runbooks/soft-launch-playbook.md` — Stage 0 entry gates
  that PEN-1 shares.
- `docs/openapi/declaration.json` — endpoint enumeration the vendor
  works from.
- `applications/declarant-portal/CLAUDE.md` § Security headers —
  the headers the vendor exercises against bypass.
- `services/declaration/CLAUDE.md` and
  `services/verification-engine/CLAUDE.md` — service specs.
- `docs/PRODUCTION-TODO.md` — the ticket index; PEN-1 sits in
  Phase 5.
