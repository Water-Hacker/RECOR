REPUBLIC OF CAMEROON

Ministry of Finance · Ministry of Justice · RÉCOR Consortium

**SOFTWARE ARCHITECTURE DOCUMENT**

*Authoritative reference for the implementing engineering team*

**RÉCOR**

*Registre de l’Effective Contrôle et Origine Réelle*

National Beneficial Ownership Registry of Cameroon

**DOCUMENT CONTROL**

|  |  |
|----|----|
| **Field** | **Value** |
| Title | RÉCOR — Software Architecture Document |
| Version | 1.0 — Reference architecture for engineering team |
| Audience | Implementing engineering team; Claude Code agents operating under Opus 4.7; senior reviewers; security operations; quality assurance |
| Classification | Restricted · Distributed under engineering NDA |
| Authority | RÉCOR Consortium Technical Advisory Function, approved by the Steering Committee |
| Companion documents | RÉCOR Sovereign Build Specification (92 p.); RÉCOR Concept Note for Funder Consultation (45 p.) |
| Implementing model | Anthropic Claude Opus 4.7 via Claude Code, with Sonnet 4.6 fallback for cost-sensitive workloads; sovereign Llama 3.3 70B Instruct on in-country GPUs for raw-PII reasoning |
| Build duration | 18–24 months from funding close to production launch |
| Document length | Substantive; designed for end-to-end implementation reference |

**How to Use This Document**

**Authority and scope**

This Software Architecture Document is the authoritative reference for the engineering team building RÉCOR. Where the document conflicts with other artefacts produced during the build (sprint tickets, code comments, Slack discussions, individual judgement), this document wins until it is formally updated through the change procedure defined in section 1.4. Where the document is silent, the strict engineering doctrines documented in Volume I, Part 2 govern the gap. Where the doctrines are also silent, the engineer escalates to the lead architect rather than improvising.

The document is the joint product of the consortium’s Technical Advisory Function and senior engineering leadership. It has been reviewed by the consortium’s security and integrity functions, by the funders’ technical evaluation teams during pre-financing review, and by external technical advisors retained for independent assurance. The version dated on the cover page is the binding version; changes apply only after the formal change procedure has been completed.

**Reader paths**

The document is approximately two hundred pages. No reader is expected to read it linearly from cover to cover. The intended reader paths are documented below. A reader should identify their role, follow the indicated path on first contact with the document, and return to the table of contents for targeted lookup thereafter.

|  |  |
|----|----|
| **Reader** | **Path on first contact** |
| Lead architect | Read everything. The lead architect is the only role expected to read the document in full. |
| Implementing engineer (full-stack, generalist) | Volume I (Foundations), Volume II (Claude Code), Volume III (Stack), Volume IV chapters relevant to your service, Volume VI Parts 24–26 (dev environment, CI, CD). |
| Backend engineer (Layer 2 services) | Add Volume IV Parts 11–16. Skip Volume V Part 21 (offline) and Volume IV Part 17 (applications). |
| Frontend engineer (Layer 6 applications) | Volume I Parts 2–3 (doctrines, SDLC), Volume II (Claude Code), Volume III Part 8 (TypeScript section), Volume IV Part 17, Volume V Part 21 (offline-first), Volume VI Parts 24–26. |
| Cryptography engineer (Layer 0) | Volume I Parts 2–4 (doctrines, SDLC, OPSEC), Volume III, Volume IV Part 11, Volume V Part 23 (security), Volume VI Part 27 (IaC for HSM). |
| Verification engineering (Layer 3) | Add Volume IV Part 14, Volume V Part 18 (AI inference). |
| Integration engineer (Layer 5) | Volume IV Parts 15–16; cross-reference Layer 2 service contracts. |
| Site reliability engineer | Volume I Parts 2–4, Volume V Part 22 (observability), Volume VI Parts 26–29. |
| Security operations | Volume I Parts 2 and 4 (doctrines, OPSEC), Volume V Part 23, Volume VI Part 28. |
| Quality assurance lead | Volume I Part 2 (doctrines), Volume VII Part 31 (test strategy). |
| Senior reviewer / external auditor | Volume I, Volume IV (sample 2–3 chapters), Volume VII Part 30 (build plan). |
| Claude Code agent (Opus 4.7) | The agent’s entry point is the CLAUDE.md in each repository, which references the relevant sections of this document. The agent loads sections on demand through the Skills mechanism; see Volume II. |

**Document conventions**

**Typography and structure**

Each Volume corresponds to a major aspect of the system. Volumes are numbered with Roman numerals (Volume I through Volume VII). Within each Volume, Parts are numbered in Arabic numerals and have descriptive titles. Each Part is structured uniformly: an introduction stating the Part’s scope, the substantive content, and a closing section listing the Part’s deliverables and acceptance criteria where applicable. Headings at three levels are used (H1 for Volume titles and standalone Parts of high importance, H2 for sub-sections within a Part, H3 for sub-sub-sections).

**Doctrines**

Doctrines are non-negotiable engineering policies that govern decisions across the entire codebase. They are documented in Volume I, Part 2, numbered DOCTRINE 01 through DOCTRINE 24, and presented in a distinctive boxed format throughout this document. When a doctrine is referenced in a substantive section, the reference is made by number (“per DOCTRINE 07”), and the reader is expected to look up the doctrine in Part 2.

**Callouts**

Four callout types are used throughout the document to draw attention to specific information.

> **NOTE —** Informational supplement. Reading the callout enriches understanding but is not strictly required to follow the surrounding prose.
>
> **WARN —** Operational warning. Failure to attend to the callout’s content is likely to produce a bug, a security weakness, or a compliance gap. Engineers must engage with warn callouts.
>
> **DANGER —** Critical safety information. The action described in the callout would cause irreversible harm — data loss, security breach, key compromise, or regulatory violation. Danger callouts are strictly governed; deviation requires named approval from the lead architect and the security function.
>
> **SUCCESS —** Outcome marker. The criterion described in the callout is the explicit success condition for the surrounding section. The Outcomes mechanism in Claude Code uses success callouts as evaluation rubrics.

**Code samples and configuration excerpts**

Code samples are presented in a fixed-width font on a light grey background. Code samples are not always complete and runnable; they illustrate the pattern that the engineer is expected to follow, with the complete implementation residing in the codebase. Configuration excerpts are presented identically. Where the configuration excerpt is itself the source of truth (a Kubernetes manifest, a Terraform module), the document indicates the file path in the codebase where the canonical version resides.

**Cross-references**

Cross-references within this document use the form “V2 P5” (Volume 2, Part 5) or “V4 P11 §2.3” (Volume 4, Part 11, sub-section 2.3) where finer granularity is needed. Cross-references to companion documents use the document’s short name and section: “Build Spec §5.2” refers to the Sovereign Build Specification, section 5.2.

**Status and stability of content**

Content in this document is at one of three stability levels, indicated where the level is not stable. Stable content is the default and is not annotated. Provisional content carries an inline marker \[provisional\] and is subject to change before its corresponding sprint commences. Deprecated content carries \[deprecated as of version X\] and identifies the replacement; deprecated content is retained in the document for historical context until the next major version.

**Change procedure**

This document is changed through a formal procedure to prevent uncoordinated drift between the architecture and the implementation. The procedure applies to all material changes: changes to a doctrine, changes to a technology choice, changes to the architectural pattern of any service, changes to a public contract, changes to the build sequence, and changes to acceptance criteria.

**Step 1.** The change is proposed in writing as an Architecture Decision Record (ADR) following the template in V4 P9. The proposal identifies the section of this document affected, the proposed new content, the rationale, and the consequences.

**Step 2.** The proposal is reviewed by the Technical Advisory Function, with one named reviewer assigned per affected area. The reviewer’s decision is one of: approve, request revision, escalate.

**Step 3.** Material proposals (those affecting a doctrine, a top-level technology choice, or a cross-cutting concern) are escalated to the Steering Committee for approval. Routine proposals (within a single service or component) are approved at the Technical Advisory Function level.

**Step 4.** Approved proposals are incorporated into the document with the version incremented per semantic versioning (MAJOR.MINOR.PATCH). MAJOR changes affect doctrines or top-level technology choices and produce a versioned reissue. MINOR changes add new content or refine existing content non-breakingly. PATCH changes are typographic or clarifying without substantive effect.

**Step 5.** Implementation must catch up to the document within a defined window: MAJOR changes within ninety days, MINOR changes within thirty days, PATCH changes within the next sprint. Where catch-up is not possible within the window, an exception is documented with the lead architect’s approval.

**Relationship to companion documents**

This document is one of three artefacts that govern the RÉCOR project. The relationship is hierarchical and is documented here so that readers know which artefact to consult for which question.

|  |  |  |
|----|----|----|
| **Document** | **Length** | **Question it answers** |
| RÉCOR Concept Note for Funder Consultation | 45 pages | Why is the project worth funding? Who funds what? What is the development case? |
| RÉCOR Sovereign Build Specification | 92 pages | What is the system, at the institutional and operational register? What are the components and integrations? What is the budget? |
| RÉCOR Software Architecture Document (this document) | ~200 pages | How is the system built? In what languages? With what patterns? By what process? With what tests? By Claude Code in what configuration? |

Readers needing the strategic, funding, or institutional narrative consult the Concept Note. Readers needing the institutional and component-level description consult the Build Specification. Readers building the system consult this document. Conflicts between this document and the Build Specification are resolved in favour of this document where the conflict concerns implementation; conflicts concerning institutional or budget commitments are resolved in favour of the Build Specification.

**Document deliverables**

By the end of this document, the reader should be able to: (a) name every technology used in the system and its version; (b) understand every doctrine that governs engineering decisions; (c) operate Claude Code under the configuration specified for the project; (d) navigate any service in the codebase and locate its tests, contracts, runbooks, and ADRs; (e) execute the build sequence in Volume VII Part 30 against the project plan; (f) defend any individual architectural decision in front of an external assessor by reference to this document. If a reader cannot do all six, the document has either failed in its content or has been read incompletely; the failure mode is identified through the team’s onboarding completion criteria documented in V6 P24.

**The Strict Engineering Doctrines**

> *The doctrines are the non-negotiable engineering policies that govern every decision in the codebase. They exist because no document can anticipate every situation, and because in their absence a project of this scale degrades through accumulated small compromises that individually look reasonable. The doctrines are the discipline that prevents the degradation.*

Twenty-four doctrines are documented below. Each doctrine is numbered, named, and stated with operational specificity. Each doctrine is binding on every engineer, every reviewer, every Claude Code agent invocation, and every CI policy gate. Violations are recorded; persistent violations are escalation events. Waivers exist for narrowly-scoped cases under named approval; the waiver procedure is documented at the end of this Part.

**The doctrines**

**DOCTRINE 01 · Completeness over partial delivery**

The marginal cost of completeness, with Claude Opus 4.7 in the loop, is near zero. A feature is shipped complete: implementation, tests, documentation, runbook, alerts. Partial delivery is forbidden even when the partial state would technically work. “I’ll add the tests later”, “I’ll write the runbook next sprint”, “I’ll document this once it stabilises” — each is a doctrine violation. The standard is not good enough; the standard is holy shit, that’s done. The corollary is that no engineer is asked to defend completeness against a deadline; completeness is the deadline.

**DOCTRINE 02 · Plan before writing code**

No code is written before the plan exists in a form a reviewer can read. The plan identifies the problem, the chosen solution, the alternatives considered, the touched surfaces, the tests that will exist, the rollback path, and the failure modes. For features above a trivial threshold (one file, fewer than fifty lines of net change, no schema change, no public-contract change), the plan is an Architecture Decision Record or a design document. For Claude Code agents, this means Plan Mode (Shift+Tab twice) is the default; the agent presents a plan and waits for human approval before any edit. Skipping planning is a doctrine violation regardless of how confident the engineer or the agent is.

**DOCTRINE 03 · Search before building**

No engineer or agent builds something the codebase already contains. Before writing a function, the engineer searches for an existing function that does the job. Before adding a dependency, the engineer searches for an existing dependency that already provides the capability. Before introducing a new pattern, the engineer searches for an existing pattern that already solves the problem. The search is a real search of the codebase, the dependency graph, and the project’s ADR record — not a guess that the thing probably does not exist. The doctrine is enforced through code review; reviewers who pass a duplicate-introduction without challenge are themselves in violation.

**DOCTRINE 04 · Tests are part of the feature, not after the feature**

Tests are written in the same pull request as the implementation. A pull request without tests is not reviewed; it is returned with a request for tests. The test discipline is layered: unit tests for pure logic, integration tests for cross-component behaviour, contract tests for API surfaces, end-to-end tests for primary user workflows, property tests for invariants, fuzz tests for parser-like surfaces, mutation tests for the verification engine and the cryptographic substrate. Tests written after the implementation tend to encode the implementation’s mistakes; tests written alongside discipline the implementation.

**DOCTRINE 05 · Documentation is part of the feature, not after the feature**

Every public function, every public type, every service, every API endpoint, every Kubernetes manifest, every Terraform module, every Rego policy carries inline documentation in the same pull request as the implementation. Documentation that lives outside the codebase (runbooks, ADRs, this document) is updated in the same merge train as the change that requires it. Documentation drift is treated as a defect with severity equivalent to a logic bug.

**DOCTRINE 06 · The complete answer, not the plan to build it**

When asked to do something — by a human, by another agent, by a ticket — the engineer or agent delivers the finished product, not a plan to build it. This is the operational corollary of Doctrine 1. Time is not an excuse. Fatigue is not an excuse. Complexity is not an excuse. The doctrine recognises that the human asking has limited context and will not know to ask for the parts of completeness they did not specify; the engineer or agent supplies those parts proactively. “Do you want me to also write the tests?” is not a question to ask; the tests are part of the delivery.

**DOCTRINE 07 · No workarounds where the real fix exists**

When a problem has both a real fix and a workaround, the real fix is chosen. The workaround appears acceptable in the moment because it is faster and because the cost of the real fix is visible while the cost of the workaround compounds invisibly. The doctrine asks the engineer to recognise this asymmetry. Workarounds are admissible only when the real fix is genuinely outside the scope of the current change (it touches a different team’s service, it requires a schema migration that the change is not authorised to perform, it requires a security review the current change does not have). The workaround in that case is documented as a \[TODO\] with the linked ticket for the real fix, and the \[TODO\] does not survive past the next sprint.

**DOCTRINE 08 · No dangling threads**

Tying off a thread takes five more minutes; leaving it dangling produces compounding cost. A dangling thread is: a TODO without a linked ticket; a commented-out section without a remove-by date; a code path that no caller reaches; a configuration field that no service reads; a metric that no dashboard renders; a documented capability that the codebase does not implement. The doctrine’s enforcement is mechanical: CI flags TODOs older than two sprints, dead code older than four sprints, dashboards with unused metrics, and documentation references that do not resolve.

**DOCTRINE 09 · Holy shit, that’s done — the standard for delivery**

The standard for any delivery is not “politely satisfied”. The standard is “holy shit, that’s done.” The engineer or agent applies the standard self-critically before requesting review. The standard is met when: the feature works in the happy path; the feature handles every failure mode the engineer can enumerate; the tests cover the feature at the appropriate ratio for its layer; the documentation explains the feature to a competent engineer who has never seen it before; the observability surfaces (metrics, logs, traces, alerts) are in place; the runbook for the on-call team is updated. Until all six are true, the work is not done and the doctrine is not satisfied.

**DOCTRINE 10 · Reviewability over speed of merge**

Pull requests are sized for reviewability, not for the engineer’s convenience. The target size is under five hundred lines of net change with strict justification for anything larger. Large changes are decomposed into a sequence of smaller, individually-reviewable changes that compose to the final intent. Review of a large pull request is slow, error-prone, and miss-prone; review of a sequence of small changes is fast, accurate, and miss-free. The doctrine applies to Claude Code agent output: the agent must produce reviewable diffs, not monolithic rewrites.

**DOCTRINE 11 · Two reviewers, at least one cross-team**

Every pull request requires two reviewer approvals before merge. At least one reviewer is from a domain team different from the author’s. Pull requests touching cryptographic substrate, the verification engine, or public API contracts require a third reviewer from the corresponding specialist team. Reviewer approval is not a courtesy; the reviewer is accountable for the quality of what they approve. Approvals issued on pull requests the reviewer did not read are themselves doctrine violations and are detected through retrospective sampling.

**DOCTRINE 12 · Production-grade from the first commit**

There is no “it will be production-grade later”. Code that runs in any environment carries the production-grade properties from the first commit: structured logging, error handling, input validation, secrets discipline, dependency hygiene, and licensing compliance. “Prototype” is not a category of code in this project; every line is either deleted or production-grade.

**DOCTRINE 13 · Idempotency on every state-changing operation**

Every state-changing operation carries an idempotency token that the operation’s implementation honours. Retried operations on the same idempotency token produce the same outcome and the same side effects as the original attempt. This is the structural property that permits the platform to deliver at-least-once semantics in messaging and synchronous APIs without producing duplicate state. Operations that cannot be made idempotent for genuine reasons (true side effects on external systems with no compensating action) require explicit documentation and named approval.

**DOCTRINE 14 · Fail closed at integration boundaries**

At every boundary where the platform integrates with a consumer system that takes a consequential action — ARMP awarding a tender, customs releasing a container, a bank opening an account — the integration contract is fail-closed: if the platform cannot respond within the SLO, the consumer holds the action rather than approving it. The doctrine is implemented through timeouts that produce explicit hold signals, through circuit breakers that surface unhealthy upstream, and through consumer-side enforcement of the hold semantics. Fail-open at a consequential boundary is a doctrine violation regardless of how convenient the fail-open default would be.

**DOCTRINE 15 · Cryptographic provenance on every consequential event**

Every consequential event in the platform — declaration submission, verification outcome, lane decision, access to restricted-tier data, policy change, schema change — carries a cryptographic signature by the responsible principal and is anchored in the audit channel of the Fabric ledger. The cryptographic provenance is verifiable years later by a party that has access only to the ledger and the platform’s public verification surfaces. “We’ll trust the logs” is not an acceptable substitute for cryptographic provenance; logs can be edited, signatures cannot.

**DOCTRINE 16 · Observability is non-optional**

Every service exposes the four observability surfaces: metrics (Prometheus-format, with naming per V5 P22), structured logs (JSON, with the canonical schema), distributed traces (OpenTelemetry, with context propagation across every gRPC and HTTP boundary), and health probes (liveness, readiness, startup). A service that does not expose all four cannot be deployed. The doctrine is enforced at the admission controller in Kubernetes; deployments without the observability surfaces are rejected.

**DOCTRINE 17 · Zero trust at every network boundary**

Every network call is authenticated, every network call is authorised, every network call is encrypted in transit, and every network call is logged. The doctrine applies even to calls within the platform’s own perimeter; there is no “inside the perimeter, things are trusted”. SPIFFE/SPIRE issues workload identities; mTLS encrypts every connection; Open Policy Agent evaluates every authorisation decision; the audit service logs every consequential call. “We’ll add auth later” is forbidden; auth is the first commit, not the last.

**DOCTRINE 18 · No secrets in code, in tickets, in chat, or in logs**

Secrets — API keys, certificates, passwords, signing keys, encryption keys, and any material that grants access — do not appear in source code, in commit messages, in ticket descriptions, in chat channels, or in log output. Secrets reside in the platform’s secret manager (Vault for sovereign secrets; sealed-secrets in Kubernetes for non-sensitive deployment configuration), are injected into pods through CSI drivers at runtime, and are rotated on the documented cadence. CI is configured to detect and reject any commit that introduces a secret pattern.

**DOCTRINE 19 · Reproducible everything**

Every build is reproducible. Every test is reproducible. Every deployment is reproducible. Reproducibility is the property that running the same procedure on a different machine, at a different time, by a different engineer, produces a bit-identical or behaviourally-identical result. The doctrine is implemented through hermetic builds, pinned dependency versions with cryptographic hash verification, deterministic test ordering, and infrastructure-as-code for every environment. Non-reproducible state is the precondition for many production incidents; the doctrine prevents the precondition.

**DOCTRINE 20 · Supply chain integrity, SLSA Level 4**

Every artefact deployed to production carries SLSA Level 4 build provenance. The build occurs on isolated infrastructure with no human access during the build. The provenance is signed by Sigstore at build time and verified at deployment. The Software Bill of Materials is generated and retained. Dependencies are pinned with cryptographic hash and consumed only from approved registries. The supply chain doctrine is the defence against the class of attacks where the adversary compromises a dependency or the build infrastructure rather than the platform itself.

**DOCTRINE 21 · Post-quantum agility**

The platform’s cryptographic substrate is engineered for post-quantum migration even before post-quantum migration is technically required. The doctrine is implemented through cryptographic agility: every cryptographic operation goes through an indirection layer that permits algorithm substitution, every key has a documented rotation procedure that doubles as a migration procedure, every protocol negotiation supports algorithm versioning. The platform is not yet running post-quantum primitives; the platform is engineered to switch to them when the FIPS 203/204/205 ecosystem matures sufficiently for production deployment without rebuild.

**DOCTRINE 22 · Anthropic-primary AI inference**

Claude Opus 4.7 is the primary model for every reasoning task whose data classification and capability requirements match its profile. Claude Sonnet 4.6 is the cost-sensitive fallback within the Anthropic family. Sovereign on-premises Llama 3.3 70B Instruct (with Mistral Large 2 as secondary sovereign option) is reserved for the residual class where raw personally identifiable information must remain on Cameroonian soil. The routing discipline is enforced at the inference API gateway, not by convention in calling services. Approximately ninety percent of inference workload routes to Anthropic models. The doctrine reflects the empirical capability superiority of Opus 4.7 on adversarial reasoning, which is the verification engine’s load-bearing capability.

**DOCTRINE 23 · Claude Code Plan Mode is the default**

Every Claude Code agent session for substantive feature work begins in Plan Mode (Shift+Tab twice). The agent produces a plan, the human reviews it, the agent then enters implementation mode. The doctrine reflects the empirical finding documented in Anthropic’s internal testing that unguided agent attempts succeed approximately one-third of the time, while planned agent attempts approach the success rate of a well-prepared human engineer. The cost of planning is small; the cost of unplanned agent failure is large; the discipline is unconditional. Exceptions exist for trivial changes (single-file, under fifty lines, no schema, no contract), where the agent may proceed without an explicit plan.

**DOCTRINE 24 · The standard is non-negotiable; the path to meet it is negotiable**

When an engineer or an agent finds the standard difficult to meet on a particular task, the response is to find a different path to the standard, not to lower the standard. The doctrines are not aspirational; they are operational. “We didn’t have time”, “It was complex”, “The deadline was tight” — these are not waivers. The doctrines are designed against the expectation of time pressure, complexity, and deadlines; if they could be set aside under pressure they would not be doctrines. The corollary is that scope, schedule, or resourcing must be negotiated when the standard cannot be met as planned; the standard is not what gets cut.

**Doctrine enforcement**

Doctrines are enforced through three mechanisms operating in concert.

**Mechanism 1 — CI policy gates.** Where a doctrine can be expressed as a machine-checkable property, CI enforces it at every pull request. Test coverage minimums, dependency hash verification, SBOM generation, secrets scanning, dead-code detection, and license compliance are all CI-enforced. A pull request that fails any CI policy gate cannot be merged regardless of reviewer approval.

**Mechanism 2 — Code review.** Reviewers are accountable for verifying that doctrines applicable to the change are honoured. Where a doctrine is qualitative (“completeness over partial delivery”), the reviewer makes the call. Reviewer accountability is itself audited through retrospective sampling: a sample of merged pull requests is re-reviewed monthly by the lead architect, and reviewers who systematically pass doctrine violations are themselves escalated.

**Mechanism 3 — Retrospective audit.** Quarterly the security and integrity functions audit a stratified sample of merged work for doctrine compliance. The audit produces a doctrine-compliance report shared with the consortium’s Technical Advisory Function. Findings drive corrective action including additional CI gates, reviewer guidance updates, and where necessary, individual escalation.

**Doctrine waivers**

A doctrine waiver is admissible only under named approval. The waiver procedure is as follows.

- The engineer requests the waiver in writing, with the specific doctrine identified, the specific work the waiver applies to, and the rationale for the waiver.

- The waiver is reviewed by the lead architect and, for doctrines pertaining to security or to cryptographic substrate, by the security function. Either approver may decline the waiver.

- Approved waivers are scoped: a waiver applies only to the specifically identified work, not to a broader class of work.

- Approved waivers are time-bounded: the maximum waiver duration is one sprint, with renewal requiring fresh approval.

- Approved waivers are logged in the consortium’s engineering record and reviewed quarterly. Systemic waiver patterns trigger doctrine refinement (clarifying language, adjusting threshold) rather than continued waiver.

> **DANGER —** Doctrine 15 (cryptographic provenance), Doctrine 17 (zero trust), Doctrine 18 (no secrets), and Doctrine 20 (supply chain integrity) cannot be waived under any circumstance. These doctrines protect against attack classes whose realisation would terminate the platform’s credibility; a waiver would defeat the doctrine’s purpose. Engineers needing to deviate from one of these four must propose a change to the doctrine itself through the document change procedure, not request a waiver.

**Onboarding to the doctrines**

Every engineer joining the project completes a doctrine onboarding before being granted commit access. The onboarding is a structured walkthrough of each doctrine with concrete examples drawn from the codebase, conducted by an existing team member. The onboarding ends with a written acknowledgement signed by the engineer. Claude Code agents are configured with the doctrines through the CLAUDE.md project file, which references this Part directly; the agent’s adherence is then verified through the Outcomes mechanism documented in V2 P5.

> **SUCCESS —** An engineer has completed doctrine onboarding when they can recite the twenty-four doctrines by name, identify which doctrines apply to a sample pull request, and articulate the rationale for each doctrine without reference to this document. The onboarding completion is recorded in the team’s onboarding tracking and is a prerequisite for commit access.

**System Development Life Cycle**

> *RÉCOR is built under a hybrid agile-with-stage-gates SDLC. Sprints deliver iterative value; stage gates impose the formal sign-offs that funder agreements, regulatory commitments, and the system’s sovereign character require. Neither pure agile nor pure waterfall is appropriate for a national platform; the hybrid is.*

**SDLC framework**

The project’s SDLC framework is built on three layers operating in concert. The base layer is the Scaled Agile Framework (SAFe) configured as Essential SAFe, which provides the cadence of two-week sprints, eight-week Program Increments, and the planning and synchronisation rituals that keep the engineering team coordinated. The middle layer is a stage-gate governance model adapted from the Defence Acquisition Framework, which imposes formal sign-off points at the transitions between project phases. The top layer is the NIST Secure Software Development Framework (SSDF) version 1.1 with the Anthropic AI-coded software adjustments documented in V2 P5, which provides the security-engineering practices that integrate throughout the SDLC.

This three-layer composition produces an SDLC that is empirically appropriate for sovereign-grade software construction: fast enough to maintain engineering team velocity, formal enough to satisfy the funders’ fiduciary and audit requirements, and secure enough to satisfy the integrity properties documented in the doctrines.

**SDLC phases and stage gates**

The RÉCOR build progresses through six SDLC phases. Each phase has defined entry criteria, defined deliverables, and defined exit criteria. The transition from one phase to the next is a stage gate at which the consortium’s Steering Committee, the lead funder’s representative, and the platform’s security and quality assurance functions evaluate the project against the exit criteria. The gate is passed when every exit criterion is met; partial gates are not passed. The project does not enter the next phase until its gate has been passed.

|  |  |  |  |
|----|----|----|----|
| **\#** | **Phase** | **Indicative duration** | **Principal outcomes** |
| I | Inception | 0–2 months | Vision and scope confirmed; consortium formally established; lead architect appointed; engineering team’s key roles staffed; toolchain decisions ratified; Build Specification and this Architecture Document re-baselined against any inception findings. |
| II | Foundation | 2–6 months | Kubernetes platforms operational at both sites; cryptographic substrate (HSMs, Fabric, FROST coordination, OpenTimestamps anchoring) operational; data model implemented in PostgreSQL with migrations under version control; CI/CD operational; observability operational; first Layer 2 services (Entity, Person, Declaration) deployable; security baseline tested against a designed adversary in tabletop exercise. |
| III | Verification Engine | 6–11 months | Nine-stage verification pipeline operational stage by stage; identity authentication adapters live for BUNEC, NIU, immigration; sanctions, PEP, adverse-media stages live with daily-refreshed feeds; pattern detection signatures 1–6 live; Dempster–Shafer fusion live; lane routing live; Declarant Portal operational; pilot with two hundred ARMP-registered bidders running by month nine, calibration complete by month eleven. |
| IV | Consumer Integrations | 11–16 months | Eight institutional consumer integrations operational under contract; SLO compliance measured and reported against negotiated targets; consumer onboarding training delivered; consumer-side acceptance tests passed; legal-framework progression at parliamentary second reading minimum. |
| V | Applications & ML maturity | 16–20 months | Investigation Workbench, Public Portal, Whistleblower Intake, Administrative Console operational; supervised pattern-detection classifier (Signature 7) trained on Phase III/IV accumulated data and in production; community detection (Signature 8) in production; pre-launch security audit passed; accessibility audit passed. |
| VI | Launch & operations | 20–24 months | Mandatory rollout phased by entity class and sector; BODS export consumed by Open Ownership; international cooperation through INTERPOL/StAR operational; full mandatory declaration in effect; steady-state operating model in effect; Phase VI gate is the formal end of build and the start of operations. |

> **NOTE —** The phase durations above are indicative ranges. Actual durations are managed against the stage-gate criteria, not against the calendar. A phase is permitted to run longer than its indicative range if the exit criteria are not yet met; a phase is not permitted to be foreshortened by relaxing the exit criteria. The lead funder’s representative on the gate review is empowered to enforce this discipline.

**Stage gate criteria**

Each gate is evaluated against a structured set of criteria. The criteria are deliberately specific so that gate decisions are non-arbitrary; reasonable people reviewing the same evidence reach the same conclusion. The criteria for each gate are documented in the consortium’s gate-review templates; the criteria below are representative.

**Gate I-to-II — from Inception to Foundation**

- Consortium charter signed by all ten member organisations; non-state seats formally designated.

- Lead architect, lead verification engineer, lead security engineer, lead SRE, and lead application engineer in role with documented engagement letters or seconded-staff agreements.

- Toolchain ratification document signed off: programming languages, dependency policies, CI/CD framework, observability stack, IDE policy, Claude Code configuration.

- Engineering procurement initiated: HSM order placed, GPU cluster order placed, sovereign data centre capacity confirmed.

- Legal-framework drafting initiated with parliamentary first reading scheduled within Phase II.

**Gate II-to-III — from Foundation to Verification Engine**

- Both Kubernetes platforms operational with documented operational ownership; failover between sites tested in a non-trivial scenario.

- HSMs initialised in ceremony with the ceremony report published; FROST key shares distributed; threshold-signed operations tested.

- Fabric ten-organisation network operational with all consortium members’ peers committing to channels; OpenTimestamps anchoring operational with one anchor cycle verified externally.

- Data model implemented; migrations under version control; first synthetic-data pipeline producing realistic test data.

- CI/CD operational with the doctrine-enforced policy gates active; SBOM generation operational; Sigstore signing operational; SLSA Level 4 build provenance verified by independent rebuilder.

- Observability operational across the foundation services; alert routing configured; on-call rotation established.

- Security baseline tabletop exercise completed with the security function’s sign-off.

- Layer 2 Entity, Person, Declaration services in pre-production with contract tests passing.

**Gate III-to-IV — from Verification Engine to Consumer Integrations**

- Verification engine end-to-end operational with all nine stages live; quarterly inference audit framework operational with the first audit performed.

- Pilot operation with the two hundred ARMP-registered bidder cohort completed with documented findings and incorporated improvements.

- Verification accuracy thresholds documented from pilot: precision and recall at the targeted ranges with the calibration adjustments incorporated.

- Performance SLOs measured against pilot traffic with documented compliance.

- Declarant Portal accessibility tested against WCAG 2.1 AA standard with the audit firm’s sign-off.

- Legal framework at parliamentary first-reading completed minimum.

**Gate IV-to-V — from Consumer Integrations to Applications maturity**

- All eight consumer integrations operational with the negotiated SLO compliance demonstrated over thirty consecutive days.

- Each consumer institution has completed its training and acceptance testing with documented sign-off from the consumer’s designated representative.

- ANIF goAML integration enriching at least 90% of submitted STRs.

- ARMP webhook integration handling at least one full procurement cycle with the conflict-of-interest analysis verified against known cases.

- Bank KYC API operational with at least the largest five Cameroonian commercial banks consuming the endpoint.

- Legal framework at parliamentary second-reading minimum.

**Gate V-to-VI — from Applications to Launch**

- All six user-facing applications operational with WCAG accessibility audit passed.

- Investigation Workbench tested by ANIF, CONAC, and TCS designated investigators against representative case loads with sign-off.

- Public Portal launch communications strategy approved by the consortium’s communications function with media and civil society partner engagements scheduled.

- Whistleblower Intake operational with the protected-investigator team trained and the operational-isolation tested.

- Pre-launch security audit by independent firm completed with findings remediated.

- Supervised pattern detection classifier in production with documented accuracy on holdout test set.

- Disaster-recovery rehearsal completed within the prior thirty days with documented recovery objectives met.

- Legal framework promulgated to permit mandatory declaration.

**Gate VI exit — the formal end of build**

- Mandatory declaration in effect for all targeted entity classes with documented compliance trajectory.

- All consortium operational ownerships formally transferred from build teams to steady-state operations teams with documented handover.

- First quarterly inference audit under operational conditions passed.

- First post-launch security audit passed.

- ISO 27001 certification engagement initiated with the certification body.

**Sprint-level cadence**

Within each phase, the engineering team operates on a two-week sprint cadence with the following discipline.

- **Sprint planning.** Held on the first day of each sprint. The team commits to a set of work items for the sprint with explicit definition of done. Definition of done is consistent with the doctrines (V1 P2): implementation, tests, documentation, observability, runbook updates where applicable. Items that cannot be done within a single sprint are decomposed before commit.

- **Daily standups.** Fifteen minutes maximum. The standup is asynchronous-first (written) with synchronous follow-up only where coordination is needed. The standup is not a status report; it is a coordination event around blockers and dependencies.

- **Mid-sprint architectural review.** Held on the fifth working day of each sprint. The lead architect reviews material technical decisions in flight, addresses architectural questions, and approves Architecture Decision Records initiated during the sprint.

- **Sprint review.** Held on the last working day of each sprint. The team demonstrates completed work to the Technical Advisory Function and to a rotating consortium observer. Incomplete work is not demonstrated; doctrines preclude demoing incompleteness.

- **Sprint retrospective.** Held immediately after the sprint review. The team identifies one process improvement to implement in the next sprint. The improvement is committed to in the next sprint plan.

- **Program Increment planning.** Held every four sprints (eight weeks). The team plans the work of the next Program Increment with explicit dependency identification, capacity allocation, and outcomes alignment. The PI plan is reviewed by the Technical Advisory Function and approved by the lead architect.

**NIST SSDF integration**

The NIST Secure Software Development Framework (SP 800-218 version 1.1) is integrated into the SDLC as a cross-cutting concern. Each SSDF practice is mapped to a specific operational mechanism in the RÉCOR SDLC.

|  |  |  |
|----|----|----|
| **SSDF group** | **Selected practices** | **RÉCOR operational mechanism** |
| PO — Prepare the Organization | PO.1–PO.5 | Doctrine onboarding (V1 P2); consortium technical governance (V1 P3); secure development environment (V6 P24); supply chain doctrine (Doctrine 20). |
| PS — Protect the Software | PS.1–PS.3 | Signed commits required (Sigstore-via-gitsign); branch protection rules; reviewer approval requirements (Doctrine 11); secrets discipline (Doctrine 18); supply chain integrity (Doctrine 20). |
| PW — Produce Well-Secured Software | PW.1–PW.8 | Threat modelling per service (V5 P23); secure coding standards by language (V3 P8); peer review (Doctrine 11); test discipline (Doctrine 4); static analysis in CI (V6 P25); dynamic analysis in staging (V5 P23). |
| RV — Respond to Vulnerabilities | RV.1–RV.3 | Vulnerability disclosure policy (V5 P23); bug bounty post-launch; quarterly vulnerability assessment; coordinated disclosure process; SBOM-driven CVE matching (Doctrine 20). |

**DevSecOps integration**

Security engineering is integrated throughout the SDLC rather than added at the end. The integration operates through three mechanisms.

**Mechanism 1 — Shift left.** Security testing runs in CI on every pull request, not at a pre-release security gate. Semgrep and CodeQL run on every change; secrets scanning runs on every change; SBOM generation runs on every build; supply chain provenance runs on every artefact. Security findings are bugs handled on the same triage queue as functional bugs, not items separated into a security backlog reviewed at end of cycle.

**Mechanism 2 — Security as design partner.** The security function participates in design reviews from the earliest stage of every substantive feature. Threat modelling per service is updated continuously with the service. The security function’s representative is empowered to block a feature from progressing if its threat model is materially under-developed.

**Mechanism 3 — Security telemetry from launch.** Security monitoring is part of the platform’s observability stack from the first commit, not added after launch. Audit logs are continuously analysed by the security function; alerts are routed to the security on-call. Production deployment includes security monitoring readiness as part of the readiness check.

**Quality Assurance integration**

QA is a first-class function alongside engineering, with QA leads embedded in each engineering team. The QA discipline operates through the following mechanisms.

- **Test-pyramid discipline.** Unit tests dominate by count, integration tests are the middle tier, end-to-end tests are sparingly used for the highest-value flows. The exact ratios per layer are documented in V7 P31. QA leads enforce the pyramid; teams that produce inverted pyramids are corrected in sprint review.

- **Test data governance.** Production-like test data is generated synthetically from declared statistical distributions of the production data, never copied from production. Synthetic data generation is itself version-controlled and documented. The doctrine on no-production-data-in-non-production is non-negotiable.

- **Contract testing.** Every API contract has consumer-driven contracts via Pact. The contract is the source of truth; service and consumer evolve against the contract. A breaking contract change is blocked in CI.

- **Performance regression testing.** Each Program Increment includes a performance regression test against the prior PI’s baseline. Regression beyond the documented thresholds blocks the next PI from commencing until the regression is investigated and addressed.

- **Chaos engineering.** Weekly chaos exercises run in staging with documented fault scenarios. Quarterly game days run in pre-production with full operations team participation. Findings drive resilience improvements.

**Change management**

Changes to the architecture (this document), to the doctrines, to the technology stack, to public contracts, and to consumer integration agreements are governed by the change procedure documented in V1 P1. Changes within a service (internal implementation choices, refactoring, test additions) are governed by the standard pull-request review process. Changes to ledger-anchored state are themselves consequential events under Doctrine 15 and are signed by the responsible principal and anchored. The discipline that change management is a first-class concern, not a process overhead, is the operational reality of building a system that operates under audit for decades.

**Operating model post-launch**

Post-launch the SDLC continues with adjusted cadence. The two-week sprint cadence is retained for feature work; the Program Increment cadence is retained for planning. The phase-gate model is replaced by the regular release cadence: minor releases monthly, patch releases as needed, major releases quarterly with formal release notes and consortium review. The doctrines remain non-negotiable. The Technical Advisory Function transitions from a build-phase function to an evolutionary function with the same membership and the same authority.

> **SUCCESS —** The SDLC has succeeded when, from the outside, RÉCOR appears as a stable, reliable, continuously improving platform that ships substantive change every month, never ships a doctrine-violating release, and earns the trust of consumer institutions and the public through the predictability and integrity of its operation. The success criterion is observable in the platform’s release telemetry, incident telemetry, and consumer satisfaction metrics; it is not a subjective assessment.

**OPSEC Doctrine**

> *Operational security for RÉCOR is not a chapter that ends; it is a discipline that conditions every interaction with the platform from the first day of the build through the platform’s indefinite operational lifetime. The doctrine documented in this Part is the binding standard against which every engineer’s practice, every consortium operational decision, and every external engagement is evaluated.*

**Information classification model**

Information handled by the platform and by the build team is classified into one of five levels. The classification determines who may access the information, what handling rules apply, what storage technologies are permitted, and what disclosure consequences attend a breach. Classification is assigned at creation by the originator and is enforced by the platform’s access service and by the consortium’s personnel security procedures.

|  |  |
|----|----|
| **Level** | **Definition and handling rules** |
| Public | Information whose disclosure produces no harm to any party. Examples: published technical roadmaps, open-source code, the BODS public-tier export. Handling: no restrictions; may be transmitted over public networks without encryption. Storage: any compliant system. |
| Internal | Information whose disclosure is not desirable but produces only minor harm. Examples: internal architecture decisions before publication, draft documents, non-sensitive operational metrics. Handling: must not be shared outside the consortium and its named contractors. Storage: consortium-managed systems with at-rest encryption. |
| Restricted | Information whose disclosure produces material harm to identified parties or to the platform’s integrity. Examples: declarant personally identifiable information, verification engine evidence packages, access logs that name principals, draft policy decisions concerning specific entities, the domestic PEP register. Handling: access on a need-to-know basis with role-based authorisation; every access logged with structured justification. Storage: platform restricted-tier with envelope encryption; export to laptops requires named approval. Transmission: only over mutual-TLS authenticated channels. |
| Encrypted | Information whose disclosure produces severe harm and which requires threshold-signed quorum approval for access. Examples: beneficial-ownership records of sitting senior officials and their immediate family during their term, ongoing investigation files, classified national-security-relevant entities. Handling: access requires the FROST 7-of-10 threshold-signed quorum with at least one non-state seat in the quorum; access produces a permanent ledger-anchored audit entry. Storage: platform encrypted-tier with HSM-resident key wrap. Transmission: only over the platform’s authenticated channels with the requesting principal’s identity verified at issuance. |
| Cryptographic-critical | Information whose disclosure compromises the platform’s cryptographic substrate itself. Examples: HSM master key material, FROST key share material, threshold signature private shares, certificate authority private keys, OpenTimestamps signing keys. Handling: never leaves the HSM. The cryptographic-critical category is technically inaccessible to humans, by construction. The cryptographic officer who participates in a key ceremony does so without seeing the key material; the ceremony’s security properties hold even against a malicious ceremony participant. |

> **DANGER —** Misclassification — marking information at a lower level than it warrants — is a serious doctrine violation regardless of intent. The discipline of classification is conservative: when uncertain, classify at the higher level. Reclassification downward is permitted only by the classification owner and only with documented rationale.

**Personnel security**

**Vetting**

Every individual with access to information at Restricted or higher is vetted before access is granted. Vetting comprises identity verification through national identity systems and Cameroonian background-check procedures, criminal-record verification, financial-history review for indicators of vulnerability (debt disclosure, sanctions-list cross-check, PEP cross-check on the individual and their immediate family), and structured reference checks with prior employers. Vetting is repeated every twenty-four months. The consortium’s personnel security function operates the vetting under documented standard operating procedures; vetting outcomes are themselves Restricted information.

For Encrypted-tier access — the eight to twelve individuals across the consortium who serve as cryptographic officers and as threshold-signature key-holders — vetting is enhanced. Enhanced vetting adds polygraph examination at the consortium’s discretion, a security-clearance equivalence to Cameroon’s highest administrative clearance level, and explicit attestation by the individual to the consortium’s personnel security function regarding foreign-government contacts and financial holdings.

**Need to know**

Access to Restricted and Encrypted information is granted on a strict need-to-know basis. Access is not granted by role alone; access requires both the role and a documented current operational need. Access expires when the operational need ends. The Access Service in Layer 2 enforces this discipline at the data layer: every restricted-tier query carries a structured justification, and the policy engine evaluates the justification against the requestor’s role and against the current operational case to which the access applies.

**Separation of duties**

No single individual is permitted to perform the full lifecycle of any consequential operation. The separations enforced by the platform include: the engineer who writes code is not the reviewer who approves it (Doctrine 11); the engineer who deploys to staging is not the engineer who promotes to production; the cryptographic officer who initialises an HSM partition is not the operator who issues operational signatures; the analyst who reviews a verification outcome is not the analyst who confirms an investigation finding; the security operator who responds to an incident is not the auditor who reviews the response. The separation patterns are documented per role and enforced through identity provisioning.

**Onboarding and offboarding**

Onboarding to the project follows a documented sequence: vetting, doctrine onboarding (V1 P2), OPSEC training (this Part), tooling provisioning, identity issuance with hardware-token second factor, role assignment, access grants per role. The sequence is owned by the personnel security function with engineering and SRE participation. Onboarding completion is a prerequisite for substantive access.

Offboarding is operationally critical and is treated as such. On the announced departure date or earlier on involuntary separation, the individual’s access is revoked across every system in a documented procedure: identity provider session termination, hardware token revocation, certificate revocation, signing-key share rotation (for threshold-signature key-holders whose departure changes the quorum composition), workstation collection, key-card revocation, and HR records update. Offboarding for cryptographic officers triggers a partial key ceremony to redistribute their share to a successor. Failure to complete offboarding within twenty-four hours is itself a security incident.

**Cryptographic key handling protocols**

The platform’s cryptographic substrate depends on key material that is structurally never accessible to humans — the HSM-resident master keys, the FROST key shares held within HSM-attested partitions — but the ceremonies through which those keys are generated, distributed, rotated, and (in the cryptographic-officer-departure case) re-shared are operational events with strict handling requirements.

**Key generation ceremony**

The initial key generation occurs at the ceremonial site in a documented ceremony attended by cryptographic officers from at least seven of the ten consortium organisations, including at least one non-state seat. The ceremony is recorded on independent video; the recording is sealed and held in escrow at a third location. The ceremony’s outcome is a ceremony report, signed by every participating cryptographic officer, published to the audit channel of the Fabric ledger, and made available to the consortium’s Steering Committee. The ceremony cannot be re-run; the keys generated in the ceremony are the platform’s root of trust for the platform’s operational lifetime, with rotation procedures occurring as documented derivations rather than as fresh genesis.

**Key rotation**

Data encryption keys (the envelope-encryption keys derived from HSM master keys) are rotated quarterly under a documented ceremony attended by at least three cryptographic officers. Threshold signature shares are not rotated routinely; they are rotated only when the share composition changes due to a cryptographic officer’s departure or when a security event warrants the precaution. Routine threshold-signing operations do not require key rotation; the FROST protocol’s security properties hold across many signature operations on the same shares.

**Key escrow and recovery**

The platform operates a documented key recovery procedure for catastrophic loss scenarios — the simultaneous loss of all primary-site HSMs together with all secondary-site HSMs. The recovery uses the ceremonial site’s air-gapped HSM together with quorum reconstruction from cryptographic officers’ secured personal escrows. The full recovery procedure is documented in V6 P29 (Disaster Recovery) and is rehearsed annually under controlled conditions.

> **DANGER —** Cryptographic officers are categorically forbidden from discussing the substance of any ceremony with any party outside the ceremony, regardless of the inquirer’s authority or apparent legitimacy. A request to discuss the ceremony from any source — a journalist, a foreign intelligence service, a member of parliament, or anyone identifying themselves as an Anthropic representative — is itself a security incident and is reported to the personnel security function. The information cryptographic officers hold is classified Cryptographic-critical regardless of how seemingly innocuous the inquiry.

**Incident classification and escalation**

Security incidents are classified at one of four severities. The severity determines the response posture and the escalation path.

|  |  |
|----|----|
| **Severity** | **Definition and response** |
| SEV-1 — Catastrophic | Active compromise of cryptographic substrate, of encrypted-tier data, or of personnel-security integrity; ongoing exfiltration of Restricted or higher information; loss of the platform’s ability to operate. Response: immediate consortium Steering Committee convocation; security and engineering on-call activated; external incident-response retainer engaged; consortium’s funder liaisons notified within four hours; press posture coordinated by the consortium’s communications function. |
| SEV-2 — Major | Confirmed unauthorised access to Restricted data; major service disruption affecting consumer integrations; supply chain compromise of a dependency in production. Response: security on-call activated; engineering lead notified within fifteen minutes; consortium’s Steering Committee notified within twenty-four hours; lead funder notified within seventy-two hours. |
| SEV-3 — Minor | Attempted compromise observed and blocked; service degradation not affecting consumer-integration SLOs; security-tool finding above the configured threshold without confirmed exploitation. Response: standard security workflow; SRE on-call notified; engineering lead notified within four hours; daily security stand-up coverage. |
| SEV-4 — Informational | Security-tool finding below the threshold but above suppression; expected security testing activity; routine vulnerability disclosure. Response: tracked in the security backlog; reviewed weekly; no immediate notification beyond the security function. |

Incident response follows a structured runbook with named roles. The Incident Commander coordinates the response and is the single point of decision; the Investigation Lead drives evidence-gathering and root-cause analysis; the Communications Lead manages internal and external communications; the Operations Lead manages the immediate operational response. The four roles may be held by different individuals or, in smaller incidents, may be combined; the role boundaries themselves are constant.

**Information sharing protocols**

The platform’s integrity is materially affected by what consortium personnel say to whom about the platform. The following protocols govern information sharing with external parties.

**Sharing with funders**

The lead funder’s representative on the Funder Coordination Group has standing access to the operational metrics, security incident reports above SEV-3, and the platform’s public roadmap. Access to Restricted operational data, to specific declarant or verification information, or to the cryptographic substrate detail is by request, with the request reviewed by the consortium’s Steering Committee and granted on a documented need-to-know basis. Funder participation in technical reviews and audits is supported but does not extend automatic access beyond what the review requires.

**Sharing with the press**

Press inquiries are routed to the consortium’s communications function. Engineers, cryptographic officers, and operations staff do not speak to the press without the communications function’s coordination, regardless of the topic or the apparent benignity of the inquiry. The discipline is not about secrecy; it is about consistency — inconsistent statements produced under press pressure damage the platform’s political resilience. The communications function maintains pre-approved statements on the platform’s capabilities, on its operational record, and on its policy positions; off-script statements require approval.

**Sharing with international partners**

Information sharing with INTERPOL, the StAR Initiative, foreign FIUs, foreign tax administrations, and other international partners operates under the cooperation frameworks documented in the legal-framework chapter (V1 P3) and under the integration contracts documented in V4 P16. Ad hoc information sharing outside these frameworks is forbidden regardless of the apparent worthiness of the requesting party’s purpose.

**Sharing with researchers and civil society**

Public-tier data is statutorily open and may be consumed by researchers and civil society without restriction. Restricted-tier data is not shared with researchers or civil society except under formal research agreements that include data-protection commitments, with the research design reviewed by the consortium’s Technical Advisory Function and approved by the Steering Committee. Civil-society oversight of the platform’s operation is operationalised through the civil society seat on the consortium (V1 P3), not through ad hoc data access.

**Travel, remote work, and operational discipline**

The engineering team operates under specific OPSEC discipline that recognises the platform’s political and security sensitivity.

- **Workstation security.** Engineering workstations are managed by the consortium’s IT function with full-disk encryption, endpoint detection and response (EDR), tamper-resistant logging, and remote-wipe capability. Personal use of engineering workstations is prohibited. Workstation provisioning includes the secure baseline image documented in V6 P24.

- **Bring-your-own-device.** Personal devices are not permitted to access Restricted or higher information. Personal devices may access Internal information through documented bring-your-own-device interfaces that route through the consortium’s VPN and that do not permit local storage of consortium information.

- **Travel discipline.** Engineering personnel travelling outside Cameroon notify the personnel security function before travel. Travel to designated jurisdictions of concern requires explicit approval and may require travel-specific device configuration (loan devices, restricted access during travel). Discussion of platform internals in transit, in hotels, in airports, and at conferences is restricted to the published roadmap and the publicly-disclosed architecture.

- **Remote work.** Remote work is permitted only from approved locations with documented network properties (no public Wi-Fi for Restricted-tier access; consortium VPN required; physical workspace privacy required). Cryptographic officers do not perform cryptographic operations from remote locations except in declared emergencies.

- **Phone and chat discipline.** Platform-substantive discussions occur in the consortium’s authenticated and audited channels (the consortium’s instance of Mattermost or equivalent). Platform-substantive discussions are not held over personal email, personal phone calls, consumer messaging apps, or public chat platforms regardless of the convenience of doing so. The discipline is uncomfortable in routine operation and is non-negotiable.

**Adversary modelling**

OPSEC operates against a specific threat model. The model identifies the adversaries the platform expects to face and the capabilities those adversaries are assumed to possess. The model is updated quarterly by the security function and is itself classified Restricted; the summary below is the public-shareable level.

- Adversary class A — Politically motivated domestic actors with privileged access. Capability: insider access through compromise of a single consortium member’s personnel; institutional knowledge of the platform’s structure; legitimate operational access used outside legitimate purpose. Defence: multi-organisation consortium with non-state seats; threshold-signed approval for sensitive operations; separation of duties; cryptographic audit trail.

- Adversary class B — Foreign state intelligence services. Capability: sophisticated supply chain attacks; advanced persistent threat operations; targeting of cryptographic officers and senior engineering personnel. Defence: SLSA Level 4 supply chain integrity; hardware-rooted trust; sovereign deployment of the substrate; enhanced vetting and OPSEC discipline for cryptographic officers.

- Adversary class C — Organised crime networks operating in Cameroon and the region. Capability: financial motivation; ability to bribe insiders; some capability for sophisticated cyber operations through hired specialists; willingness to use physical intimidation. Defence: vetting and separation of duties; independent audit; personnel security protections for staff facing intimidation.

- Adversary class D — Cybercriminals seeking financial gain. Capability: standard cybercriminal toolkit; opportunistic exploitation of disclosed vulnerabilities. Defence: vulnerability management discipline; defence in depth; bug bounty programme post-launch.

- Adversary class E — Adversarial declarants attempting to defeat the verification engine. Capability: sophisticated knowledge of beneficial-ownership concealment techniques; resources to acquire proxy identities, route ownership through complex chains, time declarations to evade detection. Defence: the verification engine itself, with eight pattern-detection signatures, three-tier AI inference, and Dempster–Shafer fusion.

**OPSEC training**

OPSEC training is mandatory for every individual with project access, repeated annually. The training covers: information classification and handling rules; personnel security expectations; incident reporting; information-sharing protocols; travel and remote-work discipline; recognition of social-engineering and phishing attempts; the adversary model; the specific operational risks the individual’s role exposes them to. Training completion is recorded in the personnel security function’s records and is itself audited.

> **SUCCESS —** OPSEC has succeeded in any given period when the consortium’s incident reporting shows no SEV-1 or SEV-2 incidents traceable to OPSEC failures, no successful social-engineering attacks against engineering personnel, no inadvertent disclosure events involving Restricted or higher information, and a culture in which personnel proactively report potential security concerns without fear of blame. The success criterion is observable in the security telemetry and in the personnel security function’s reports; it is not subjective.

**Claude Code Operating Manual**

> *RÉCOR is implemented by a small senior engineering team working with Claude Code agents running on Claude Opus 4.7. The agents are not auxiliary tools; they are first-class collaborators that produce the majority of the codebase under human direction. This Part is the authoritative reference for how the team operates that collaboration.*

**Why Claude Code, why Opus 4.7**

Anthropic’s own engineering practice as of mid-2026 has the majority of its production code written by Claude Code. The capability is mature: Wiz migrated a fifty-thousand-line Python library to Go in approximately twenty hours of active development against a manual estimate of two to three months; Ramp reduced incident investigation time by eighty percent; Rakuten reduced feature delivery time from twenty-four working days to five. These outcomes are not promotional artefacts; they are operational evidence that the appropriate use of Claude Code on Opus 4.7 produces both throughput and quality at levels unavailable through pure-human engineering at comparable team size.

RÉCOR’s build envelope — eighteen to twenty-four months for a national-scale platform with the architectural depth documented in V4 — is achievable only with this leverage. A pure-human engineering team capable of delivering the platform within the envelope would be three to four times the size of the team this project is funded to assemble. The Claude Code leverage is the structural property that makes the project feasible at the funded size.

The capability profile is Opus 4.7 with extended thinking enabled at the xhigh effort level. Opus 4.7 is the empirical leader on coding benchmarks at the time of this document’s baseline and on the adversarial-reasoning and architecture-decision tasks that the verification engine and integration design require. Sonnet 4.6 is the cost-sensitive fallback for routine work — bulk refactors, mechanical migrations, well-scoped feature implementations where the planning context is fully specified. The model selection is itself a governed choice; engineers do not freely substitute models without documented rationale.

**The multi-agent orchestration model for RÉCOR**

Claude Code’s multi-agent orchestration capability (shipped Q1 2026 and matured through the Code with Claude 2026 announcements) is the operating model for substantive feature work. The pattern is: a lead agent decomposes the feature into pieces, delegates each piece to a specialist sub-agent with its own context window and tool restrictions, the sub-agents work in parallel on a shared filesystem, results flow back to the lead agent which synthesises and presents the unified deliverable for human review.

For RÉCOR, the specialist sub-agent roster is documented below. Each agent has a defined scope, a defined tool restriction, and a defined model assignment. The agent roster is operated centrally and may not be modified without lead-architect approval.

|  |  |  |
|----|----|----|
| **Specialist agent** | **Model** | **Scope and tool restrictions** |
| lead-orchestrator | Opus 4.7 xhigh | Decomposes feature work, delegates to specialists, integrates outputs, presents to human. Tools: full filesystem, read-only repo access, sub-agent invocation. No direct write to production-critical paths without delegation. |
| architect-reviewer | Opus 4.7 xhigh | Reviews proposed changes against this Architecture Document and the doctrines. Tools: read-only. Output: structured review comments referencing specific document sections and doctrines. |
| security-reviewer | Opus 4.7 xhigh | Reviews proposed changes for security implications using STRIDE methodology, OWASP Top 10, and the project’s threat model. Tools: read-only plus Semgrep, CodeQL invocation. Output: structured findings with severity classification. |
| test-author | Sonnet 4.6 | Writes tests against existing or proposed implementation. Tools: filesystem read-write within test directories only. Output: complete test suites at the layer-appropriate pyramid ratio. |
| docs-author | Sonnet 4.6 | Writes documentation: API reference, runbooks, inline doc comments. Tools: filesystem read-write within documentation directories. Output: documentation conforming to the project’s documentation style guide. |
| refactor-specialist | Opus 4.7 xhigh | Performs scoped refactors. Tools: full filesystem read-write. Constrained to refactor-only operations: no new feature work, no contract changes. |
| migration-specialist | Opus 4.7 xhigh | Database and schema migrations. Tools: filesystem read-write within migration directories; sandbox database access. Output: forward and reverse migrations with property tests. |
| integration-specialist | Opus 4.7 xhigh | Consumer integration implementation. Tools: filesystem read-write within integration directories; mock-server invocation for testing. Output: integration with contract tests against the consumer’s mock surface. |
| incident-investigator | Opus 4.7 xhigh | Investigates production incidents by traversing logs, traces, metrics, and the codebase. Tools: read-only repo, read-only observability stack, sub-agent invocation for parallel exploration. Output: structured incident reports with root-cause hypotheses ranked by evidence. |
| verification-engine-specialist | Opus 4.7 xhigh | Builds and modifies the verification engine specifically. Tools: filesystem read-write within verification-engine directories; access to test fixtures. Cross-references the inference audit framework. Constrained from modifying threshold parameters without dedicated review. |

**Sub-agent invocation pattern**

Sub-agents are invoked by the lead orchestrator via Claude Code’s Task tool. The lead orchestrator presents a structured task with: a clear objective, the input artefacts (file paths, ticket references, design documents), the output expectations, and the success criteria. The sub-agent works in its own context window, returns a compressed summary, and the lead orchestrator integrates the outputs.

A typical feature implementation invokes the following sub-agents in sequence: architect-reviewer (to validate the proposed design against the document), test-author (to produce the test scaffolding alongside the implementation), the implementation (performed by the lead orchestrator with delegated specialists as needed), security-reviewer (to validate the implementation), docs-author (to produce the inline documentation and runbook updates). The human reviews the final integrated output, not the individual sub-agent transcripts; the lead orchestrator’s summary surfaces the material decisions for human attention.

**Project configuration files**

Claude Code’s behaviour is governed by configuration files at multiple scopes. The RÉCOR project uses these scopes deliberately.

**CLAUDE.md per repository and per service**

Each repository has a top-level CLAUDE.md that orients agents to the repository. Each substantial service has its own CLAUDE.md within the service directory. The CLAUDE.md is the agent’s first reading; it must contain the essential context for an agent to begin productive work without rediscovering the repository’s conventions from scratch.

The canonical CLAUDE.md structure for a RÉCOR service is documented below.

> \# Service: \<service name\>
>
> \# Layer: \<V4 P11/12/13/etc\>
>
> \# Owner: \<engineering team\>
>
> \# Doctrines reference: V1 P2
>
> \## What this service does
>
> \<one-paragraph plain English\>
>
> \## Language and toolchain
>
> \<Rust 1.84 / Go 1.26 / TypeScript 5.7\>
>
> Build: \<just commands\>
>
> Test: \<just commands\>
>
> Lint: \<just commands\>
>
> \## Architecture
>
> \- Persistence: \<PostgreSQL/Neo4j/etc\>
>
> \- Events: \<Kafka topics emitted, consumed\>
>
> \- gRPC contracts: \<reference to proto path\>
>
> \- Public APIs: \<REST/GraphQL surfaces\>
>
> \## SLOs
>
> \<latency budgets per operation\>
>
> \## Active development context
>
> \<links to relevant ADRs, in-flight tickets, current sprint goals\>
>
> \## Doctrines that apply with special weight here
>
> \<service-specific doctrine emphasis\>
>
> \## When in doubt
>
> 1\. Read this document section \<V4 P11 §X\>
>
> 2\. Check ADRs in \<path\>
>
> 3\. Ask the lead architect (do not improvise)

**.claude/settings.json — permission policy**

Each repository carries a .claude/settings.json file under version control that documents the permission policy for Claude Code agents operating in the repository. The policy specifies allow-listed shell commands, deny-listed paths, and the operations that require interactive human approval. A representative settings.json for a Layer 2 service follows.

> {
>
> "permissions": {
>
> "allow": \[
>
> "Bash(just \*)",
>
> "Bash(cargo \*)",
>
> "Bash(cargo-clippy \*)",
>
> "Bash(cargo-test \*)",
>
> "Bash(git status)",
>
> "Bash(git diff \*)",
>
> "Bash(git log \*)",
>
> "Bash(rg \*)",
>
> "Bash(fd \*)",
>
> "Read(\*)",
>
> "Edit(src/\*\*)",
>
> "Edit(tests/\*\*)",
>
> "Edit(migrations/\*\*)",
>
> "Edit(proto/\*\*)"
>
> \],
>
> "deny": \[
>
> "Bash(rm -rf \*)",
>
> "Bash(git push \*)",
>
> "Bash(git reset --hard \*)",
>
> "Bash(kubectl \*)",
>
> "Bash(terraform apply \*)",
>
> "Edit(.github/\*\*)",
>
> "Edit(/etc/\*\*)",
>
> "Edit(\*\*/secrets.\*)",
>
> "Edit(\*\*/.env\*)",
>
> "Read(\*\*/secrets.\*)",
>
> "Read(\*\*/.env\*)"
>
> \],
>
> "ask": \[
>
> "Bash(git commit \*)",
>
> "Bash(cargo install \*)",
>
> "Edit(Cargo.toml)",
>
> "Edit(package.json)",
>
> "Edit(go.mod)"
>
> \]
>
> },
>
> "hooks": {
>
> "PreToolUse": \[
>
> ".claude/hooks/pre-edit-doctrine-check.sh",
>
> ".claude/hooks/pre-bash-allowlist.sh"
>
> \],
>
> "PostToolUse": \[
>
> ".claude/hooks/post-edit-format.sh",
>
> ".claude/hooks/post-bash-audit.sh"
>
> \]
>
> }
>
> }

The deny list is the policy’s most important element. It encodes the operations that no Claude Code agent in this repository may perform without explicit human override: never delete recursively, never push to remote, never reset hard, never touch Kubernetes or Terraform from inside a service repository (those operations live in dedicated infrastructure repositories with their own policies), never edit GitHub Actions workflows (those are reviewed separately), never read or edit secrets-bearing files.

**.claude/skills/ — the RÉCOR skills catalogue**

Claude Code’s Skills mechanism (auto-discovered SKILL.md files) is the project’s primary mechanism for codifying repeatable workflows. The RÉCOR skills catalogue is published in the central engineering repository and is installed in each service repository as a git submodule. The canonical skills are documented below.

|  |  |
|----|----|
| **Skill** | **Trigger and behaviour** |
| recor-doctrine-check | Triggers on any code-generation request. Loads the doctrines, applies the relevant doctrine subset to the proposed work, surfaces any doctrine concerns before implementation begins. The skill is the first line of defence against doctrine drift. |
| recor-adr-author | Triggers when the agent or user indicates a design decision is being made. Produces an ADR draft following the project’s ADR template, places it in the docs/adr/ directory, and surfaces it for human review. |
| recor-test-pyramid | Triggers when test writing is requested. Identifies the layer of the system being tested and produces tests at the appropriate pyramid ratio (heavy unit, moderate integration, sparing e2e). Includes the project’s standard test scaffolding patterns. |
| recor-rust-service | Triggers when a new Rust service is being created. Produces the service scaffolding: cargo workspace structure, gRPC server skeleton with axum HTTP companion, observability instrumentation, health endpoints, configuration loader, structured-error type hierarchy, test fixtures. |
| recor-go-service | Same as recor-rust-service but for Go services. |
| recor-react-app | Triggers when a new React application or substantial component is being created. Produces the scaffolding conforming to the project’s frontend doctrine: TypeScript strict, Tailwind v4, the design-token system, the offline-first PWA skeleton, the i18n setup. |
| recor-migration | Triggers on database migration work. Produces forward and reverse migrations with property tests, references the migration governance procedure (V4 P12), and never modifies a migration already applied to a non-development environment. |
| recor-integration-contract | Triggers when consumer integration work is in scope. Loads the relevant integration’s contract specification, the consumer’s mock surface, and the contract test framework. Produces the integration with its contract tests in the same change. |
| recor-incident-investigation | Triggers when an incident investigation is initiated. Establishes the structured investigation workspace, loads the observability stack queries, and produces the structured incident report template populated as the investigation progresses. |
| recor-security-review | Triggers when security review is explicitly requested or when the change touches security-sensitive paths. Performs STRIDE threat-modelling for the change, cross-references the project’s threat model, runs static analysis, and produces structured findings. |
| recor-doc-author | Triggers when documentation is requested or when the doctrine on documentation completeness would otherwise be violated. Produces inline doc comments, API documentation, and runbook updates aligned with the project’s documentation style. |

Skills are versioned in the central engineering repository. Updates to skills follow the change procedure for this Architecture Document (V1 P1); skills are part of the binding governance surface, not optional convenience. A skill update applies to every service repository through the next git submodule update.

**.claude/agents/ — specialist agent definitions**

The specialist agent roster is materialised as files in .claude/agents/ at the central engineering repository, distributed to each service through the same submodule mechanism. Each agent file specifies the model, the system prompt, the tool restrictions, and the invocation patterns. A representative agent file follows.

> \# .claude/agents/security-reviewer.md
>
> ---
>
> name: security-reviewer
>
> description: Performs security review of proposed code changes using STRIDE methodology
>
> model: claude-opus-4-7
>
> tools: \[Read, Grep, Bash(semgrep \*), Bash(codeql \*)\]
>
> ---
>
> You are the security reviewer for the RÉCOR project. You operate as a sub-agent
>
> invoked by the lead orchestrator when security review is required.
>
> Your reviews apply the following methodologies:
>
> \- STRIDE threat modelling against the change
>
> \- OWASP Top 10 web vulnerabilities (where applicable)
>
> \- CWE Top 25 for the languages involved
>
> \- The project's threat model (see CLAUDE.md or docs/security/threat-model.md)
>
> For each review, produce structured output:
>
> \- Findings list, each with: severity (Critical/High/Medium/Low/Informational),
>
> CWE reference where applicable, file:line locations, recommended remediation
>
> \- Doctrine compliance: which security doctrines apply, which are honoured,
>
> which are violated
>
> \- Overall assessment: APPROVED / APPROVED WITH FINDINGS / REJECTED with reasoning
>
> Read-only. You may invoke Semgrep and CodeQL through Bash. You do not modify code.
>
> You do not approve or merge pull requests; you produce a review that the lead
>
> orchestrator and the human reviewer use to decide.

**Plan Mode as the default operating discipline**

Doctrine 23 makes Plan Mode the default for substantive work. Operationally this means every Claude Code session that will produce more than a trivial change begins with the agent in Plan Mode — entered by pressing Shift+Tab Shift+Tab in the interactive CLI or by setting the flag in scripted invocations. The agent produces a plan, the human reviews, the agent then enters implementation mode (Shift+Tab).

The plan must be substantive. The empirical finding documented in Anthropic’s internal testing — unguided agent attempts succeed approximately one-third of the time, planned attempts approach the success rate of a well-prepared human engineer — holds because the planning step collapses ambiguous decision points into reviewed decisions. A plan that does not collapse decision points (“I will implement the feature”, “I will write tests”) provides no leverage. A useful plan looks like the following representative example.

> PLAN: Add transfer-pricing risk signal to the DGI bulk export
>
> GOAL
>
> Produce a daily diff of entities sharing beneficial ownership that trade with
>
> each other. DGI's large-taxpayer audit team prioritises audits from this signal.
>
> DESIGN
>
> 1\. New Rust binary in services/exports/dgi-tpr-export
>
> 2\. Reads from Neo4j for ownership-graph queries; PostgreSQL for trade-relationship
>
> facts (the trade-relationship view will need to exist; ADR-127 to be filed)
>
> 3\. Output format: structured JSON conforming to the DGI integration schema in
>
> contracts/dgi/transfer-pricing-risk-v1.proto
>
> 4\. Run schedule: daily at 03:00 Africa/Douala via the Workflow Service Temporal
>
> schedule; not via cron
>
> 5\. Diff semantics: only entities whose beneficial ownership changed in the prior
>
> 24 hours OR whose trading partner's beneficial ownership changed
>
> TOUCHED SURFACES
>
> \- contracts/dgi/transfer-pricing-risk-v1.proto (new file)
>
> \- services/exports/dgi-tpr-export/ (new directory)
>
> \- services/workflow/src/schedules/dgi-tpr-export.rs (new schedule entry)
>
> \- docs/adr/0127-dgi-trade-relationship-view.md (new ADR)
>
> \- docs/runbooks/dgi-tpr-export.md (new runbook)
>
> TESTS
>
> \- Unit tests against the diff-computation logic with synthetic ownership graphs
>
> \- Property tests asserting: (a) the diff is monotonic across consecutive days,
>
> \(b\) entities not in the prior day's snapshot can only appear if their BO
>
> changed, (c) the diff is idempotent under re-run with same input
>
> \- Integration test against an ephemeral Neo4j+Postgres environment via testcontainers
>
> \- Contract test verifying output conforms to the DGI schema
>
> RISKS
>
> \- The trade-relationship view does not yet exist. The view definition is itself
>
> a design point that the ADR will address; if the view turns out to require
>
> schema additions to the canonical model, the work expands accordingly.
>
> \- DGI integration SLO commitment (V4 P16) is for the daily file by 06:00; a
>
> 03:00 schedule gives three hours of slack which is enough for normal runs
>
> but tight for re-runs after failures.
>
> ROLLBACK
>
> The export is additive and idempotent. Disabling the schedule in the Workflow
>
> Service stops production of new exports; existing exports remain consumable.
>
> DOCTRINES APPLIED
>
> \- 02 (plan before writing code): this plan
>
> \- 04 (tests are part of the feature): the test list above
>
> \- 05 (documentation is part of the feature): runbook and ADR
>
> \- 13 (idempotency): the rerun-safe diff semantics
>
> \- 16 (observability): metrics + structured logs + traces added per service skeleton
>
> ESTIMATED EFFORT
>
> 2 days of agent-assisted implementation; 1 day of contract negotiation with DGI
>
> to validate the schema; 1 sprint elapsed time to land

The plan is substantive enough that the human reviewer can identify whether the agent has understood the work correctly before any implementation begins. The plan identifies risks that the implementation must address; it identifies doctrines that apply with their specific instantiation; it identifies the rollback path. If the human reviewer is unconvinced by the plan, the implementation does not begin; the plan is revised.

> **WARN —** A plan that does not surface risks, that elides the doctrine application, or that omits the touched surfaces is a low-quality plan and the agent must be returned to planning. Accepting low-quality plans is the failure mode that erodes the doctrine. The reviewer is empowered to reject planning as much as implementation.

**Outcomes as quality gates**

Claude Code’s Outcomes mechanism (shipped at Code with Claude 2026) provides rubric-based evaluation: a separate grading agent scores the output against a defined rubric without having seen the producing agent’s reasoning. RÉCOR uses Outcomes for every substantive feature delivery.

Each feature carries an outcomes rubric. The rubric is part of the planning artefact: the plan identifies the success criteria, and those criteria become the rubric that the grading agent applies. A representative outcomes rubric for the transfer-pricing risk export above follows.

> OUTCOMES RUBRIC: DGI transfer-pricing risk export
>
> CRITERION 1 (P=1.0): Doctrines honoured
>
> \- Plan present and approved by human reviewer before implementation
>
> \- Tests present at the planned layers (unit, property, integration, contract)
>
> \- Documentation present (inline, ADR, runbook)
>
> \- Observability surfaces present (metrics, logs, traces, health)
>
> \- No secrets in any committed artefact
>
> \- Idempotency property tests pass
>
> PASS CRITERION: All present and verified
>
> CRITERION 2 (P=1.0): Functional correctness
>
> \- The implementation matches the plan's design as approved
>
> \- Output conforms to the DGI contract schema (validated against the schema)
>
> \- Diff semantics are correct against three handcrafted test scenarios:
>
> \(a\) no changes day-over-day produces empty diff
>
> \(b\) BO change on Entity A produces A and all trading partners
>
> \(c\) BO change on a trading partner of A produces both
>
> PASS CRITERION: All three scenarios verified in integration test
>
> CRITERION 3 (P=0.8): Code quality
>
> \- Rust idioms followed (use of ? for error propagation, no unwrap in production paths)
>
> \- No clippy warnings at deny level
>
> \- Test coverage above 85% for the new service
>
> \- No dead code introduced
>
> PASS CRITERION: All present; minor stylistic findings acceptable
>
> CRITERION 4 (P=0.8): Performance
>
> \- Daily export job completes within the 60-minute SLO under projected load
>
> \- Memory footprint stays under 2 GB
>
> PASS CRITERION: Measured against the synthetic load test in integration
>
> GRADING AGENT INSTRUCTIONS
>
> \- You have not seen the implementation transcript. Evaluate the delivered code,
>
> tests, and documentation directly against this rubric.
>
> \- Score each criterion on the 0-1 scale where 1 is full pass.
>
> \- For any criterion below 0.8, surface the specific gap that needs to be closed.
>
> \- Do not approve the deliverable until all P=1.0 criteria are at 1.0.

The grading agent’s output is the evaluation that the human reviewer reads first. If the grading agent surfaces gaps, the producing agent is sent back to close them. The human reviewer is then evaluating a deliverable that has already been graded against the explicit criteria, which is materially more efficient than evaluating a deliverable cold.

**Hooks and CI integration**

Claude Code’s hooks (PreToolUse, PostToolUse) run scripts before and after every tool invocation. RÉCOR uses hooks to enforce doctrine compliance and to integrate Claude Code activity with the project’s audit and CI streams.

- pre-edit-doctrine-check.sh: runs before any Edit invocation. Inspects the proposed edit against a set of regex patterns that detect doctrine violations (unwrap in Rust production paths, console.log in TypeScript, hardcoded secrets, dead code addition). Blocks the edit and surfaces the violation if matched.

- pre-bash-allowlist.sh: runs before any Bash invocation. Verifies the command against the allow list defined in settings.json. Blocks and logs unmatched commands.

- post-edit-format.sh: runs after every Edit. Applies the language-specific formatter (rustfmt, gofmt, prettier) automatically. Records the edit in the project’s engineering audit log.

- post-bash-audit.sh: runs after every Bash invocation. Logs the command, the exit code, and the output excerpt to the project’s engineering audit log. The audit log is retained for the project’s compliance horizon.

Hooks are not a replacement for CI; they are a complement. CI runs on every pull request and is authoritative for merge decisions. Hooks run during the engineer’s session and surface issues earlier than CI would. The combination produces faster feedback on the engineer’s side and equally authoritative gating on the merge side.

**Dispatch and Channels for managed multi-agent jobs**

For asynchronous and observable multi-agent work — the kind of work that runs longer than an interactive session and that benefits from structured progress streaming — RÉCOR uses Claude Code’s Dispatch and Channels capabilities.

- **Dispatch.** Manages asynchronous task execution with observability. Used for: nightly verification engine recalibration runs, scheduled adversarial test campaigns against the verification engine, large-scale refactors that span weeks. Dispatch jobs are submitted by the engineering lead, run on dedicated infrastructure, and produce structured progress through Channels.

- **Channels.** Streams real-time events from Claude Code sessions. Used for: incident investigation where multiple agents are exploring different evidence sources in parallel; release rehearsal where multiple environments are being prepared simultaneously; daily report generation aggregating engineering activity.

**MCP server inventory for the project**

Claude Code’s Model Context Protocol (MCP) integration lets the agents access tools and information outside the standard filesystem and shell. The RÉCOR project operates a controlled inventory of MCP servers, listed below.

|  |  |
|----|----|
| **MCP server** | **Purpose** |
| recor-codebase-search | Project-specific code search across the monorepo with semantic understanding of the architecture. Used by the lead orchestrator and the architect-reviewer. |
| recor-docs-search | Search across this Architecture Document, the Build Specification, ADRs, and runbooks. Surfaces specific sections relevant to the current task. |
| recor-observability | Read-only access to the platform’s observability stack (Prometheus metrics, Loki logs, Tempo traces). Used by incident-investigator and during performance work. |
| recor-issue-tracker | Read-write access to the project’s issue tracker (Linear or equivalent). Used to update ticket status, link pull requests, query in-flight work. |
| recor-security-tools | Invocation of security tools (Semgrep, CodeQL, Trivy, Snyk) on specified code surfaces. Used by security-reviewer. |
| recor-test-data-generator | Generates synthetic test data conforming to the canonical model with declared statistical distributions. Used by test-author and migration-specialist. |
| recor-deployment-status | Read-only access to the platform’s deployment state across environments. Used by SRE-flavoured agents and during release planning. |

> **WARN —** MCP servers that access production data (recor-observability, recor-deployment-status) operate in read-only mode against environments above staging. Any MCP server that would mutate production state requires named human approval per invocation, enforced through the MCP server itself rather than relying on agent discipline. This is an instantiation of Doctrine 17 (zero trust at every boundary).

**Sensitive operations that always require human approval**

The following operations always require explicit human approval, regardless of the agent’s confidence and regardless of the human reviewer’s prior approval of the broader work. The list is non-exhaustive but is the operational floor.

- Any modification to ledger-anchored data.

- Any modification to encrypted-tier records.

- Any modification to the verification engine’s threshold parameters, basic probability assignments, or signature class definitions.

- Any modification to the platform’s identity provider configuration.

- Any modification to the platform’s access policy (Rego files in the Access Service).

- Any modification to the cryptographic substrate code paths.

- Any deployment to pre-production or production environments.

- Any modification to consumer integration contracts.

- Any modification to the doctrines documented in V1 P2.

- Any modification to this Architecture Document.

The approval mechanism is enforced at multiple layers: the .claude/settings.json deny and ask lists prevent the agent from initiating the operation; the CI gates prevent the operation from completing without the required reviewer signatures; the deployment pipeline’s gates prevent the operation from reaching the target environment without the documented approvals. The defence-in-depth is the operational instantiation of the doctrine that no single mechanism is permitted to be the sole protection.

**Anti-patterns to avoid**

Specific anti-patterns are documented and the team is trained to recognise them. The patterns below are recorded because they have empirical evidence of producing harm in agent-assisted engineering.

- **The over-eager merger.** The pattern: the agent produces extensive work in one session, the human accepts the work without re-deriving the plan, the work merges, the doctrines are not honoured. Counter: every substantive delivery passes through the Outcomes grading agent before human review.

- **The unbounded exploration.** The pattern: the agent is asked an open-ended question, explores the codebase widely, produces a sprawling but unfocused response. The human is unsure what to act on. Counter: the lead orchestrator always frames work as specific tasks with success criteria, never as open exploration. Open exploration is reserved for incident investigation and is itself structured.

- **The doctrine bypass.** The pattern: the agent encounters a doctrine that makes the immediate task more cumbersome (tests, documentation, observability), and produces work that omits the doctrine’s requirement on the implicit theory that the doctrine doesn’t apply to “just this small change”. Counter: the recor-doctrine-check skill is the first line of defence; the architect-reviewer sub-agent catches what the skill misses; the human reviewer is accountable for catching the residual.

- **The plan that wasn’t a plan.** The pattern: the engineer asks the agent to plan; the agent produces a list of headings that are essentially the task restated; the engineer accepts the plan because the agent produced something; the work proceeds essentially unplanned. Counter: a plan that does not surface risks, decisions, doctrines, and tests is returned for revision. Reviewer accountability includes plan quality.

- **The model-substitution drift.** The pattern: the engineer encounters cost concerns and substitutes Sonnet 4.6 for Opus 4.7 on work that warrants Opus capability, producing measurably lower quality. Counter: model selection is governed; substitution requires documented justification. The cost discipline is honoured by reducing scope where appropriate, not by reducing capability on the work that proceeds.

- **The unsafe automation.** The pattern: an automation that the agent set up over time accumulates power until it is performing operations the agent should not perform unsupervised — modifying production configs, deploying without explicit approval, modifying the doctrines themselves. Counter: the permission discipline in .claude/settings.json is reviewed quarterly with the audit team; automations that accumulate scope are pared back.

**Onboarding to Claude Code**

Every engineer joining the project completes a Claude Code onboarding before being granted agent-assisted commit access. The onboarding sequence covers: the philosophy of agent-assisted engineering at RÉCOR; the doctrines as they apply to agent-assisted work; the specialist agent roster and when to invoke which; Plan Mode discipline with practical examples; Outcomes rubric authorship; the skill catalogue; the settings.json policy; hook configuration; MCP server inventory; anti-pattern recognition; the team’s shared incident library of past agent-assisted failures and their corrections.

The onboarding ends with a structured exercise: the engineer is given a representative ticket, asked to plan it with the lead orchestrator agent, asked to deliver it with the appropriate sub-agents, and the deliverable is reviewed by a senior engineer against the doctrines. Successful completion is recorded; the engineer is then granted agent-assisted commit access on a probationary basis (additional review for the first month) before transitioning to standard access.

> **SUCCESS —** An engineer has completed Claude Code onboarding when they can: configure and invoke the specialist agent roster correctly for a given task class; produce substantive plans that pass the doctrine-check skill on the first attempt; author Outcomes rubrics that grading agents can apply without ambiguity; recognise and articulate the anti-patterns when prompted; demonstrate completed work that meets the holy-shit-that’s-done standard. Onboarding completion is recorded; standard access is granted upon completion.

**Developer Workflows with Claude Code**

This Part documents the canonical engineering workflows on the project. Each workflow describes the steps, the agent invocations, the artefacts produced, and the human accountability points. The workflows are the operational instantiation of the doctrines and of the Claude Code operating model documented in V2 P5.

**Workflow: feature development**

Feature development is the predominant workflow. The flow below is canonical; deviations require documented rationale.

**Step 1 — Read the ticket and the relevant document sections.** The engineer reads the ticket, the linked design discussion, and the relevant sections of this document. Where the engineer is uncertain about the architectural intent, the lead architect is consulted before any agent invocation. Operating in unclear context is a common failure mode and is the easiest to prevent.

**Step 2 — Initiate Claude Code in Plan Mode.** From the relevant repository, the engineer launches Claude Code and enters Plan Mode via Shift+Tab Shift+Tab. The engineer presents the ticket, the relevant context, and any constraints the ticket does not explicitly capture (e.g., “this service is being refactored next sprint, prefer minimally-invasive changes”).

**Step 3 — Plan iteration with the lead orchestrator.** The lead orchestrator produces a plan. The engineer reviews the plan for substance: does it identify the touched surfaces, the tests, the doctrines, the rollback path, the risks? If the plan is thin, the engineer asks for revision. Iteration on the plan typically takes two to four rounds for a substantive feature. The plan is then committed to the ticket as a comment, becoming part of the project record.

**Step 4 — Architect review of the plan.** The engineer invokes the architect-reviewer sub-agent to review the plan against this document and the doctrines. The architect-reviewer surfaces inconsistencies with the documented architecture, missing doctrine considerations, and patterns that conflict with the rest of the codebase. Findings are addressed in the plan before implementation begins.

**Step 5 — Outcomes rubric.** The engineer authors the outcomes rubric for the feature, attached to the plan. The rubric is specific enough that a grading agent which has not seen the implementation can evaluate it against the rubric. A rubric that is vague (“Code quality is good”) is not useful; specific criteria (“Clippy passes at deny-warnings”, “Integration test scenarios A, B, C pass”) are.

**Step 6 — Exit Plan Mode and implement.** The engineer exits Plan Mode (Shift+Tab), and the lead orchestrator begins implementation. The orchestrator delegates to specialist sub-agents per the plan: test-author for the test scaffolding, security-reviewer for in-flight security validation on sensitive paths, integration-specialist for consumer integrations. The engineer observes the work, intervenes where the orchestrator misreads context, but does not micromanage — the orchestrator’s context-window is more focused than the engineer’s and the engineer’s job is now to validate, not to direct keystroke-by-keystroke.

**Step 7 — Outcomes grading.** Once the implementation is complete, the engineer invokes the grading agent against the rubric authored in step 5. The grading agent has not seen the implementation transcript and evaluates the deliverable on its own terms. Findings are addressed; the cycle iterates until the rubric is at or above its passing thresholds.

**Step 8 — Human code review.** The engineer opens a pull request. Two human reviewers approve per Doctrine 11. Reviewers read the plan, the outcomes grading, the code, the tests, and the documentation. Reviewers are not replicating the grading agent’s work; they are confirming that the grading agent’s evaluation is correct and that the deliverable meets the project’s standard from a human-judgement perspective.

**Step 9 — CI verification.** CI runs the full policy gate suite: lint, unit tests, integration tests, contract tests, security analysis, SBOM generation, signature verification. All gates pass before merge is permitted.

**Step 10 — Merge and deploy through the pipeline.** On merge to main, the deployment pipeline progresses the artefact through environments per V6 P26. Production deployment occurs at the documented release cadence with the documented approvals.

**Workflow: bug fix**

Bug fixes are smaller-scope but follow the same discipline. The lead orchestrator typically handles bug fixes without sub-agent delegation.

- Read the bug report, reproduce the bug locally if possible, examine the relevant code paths and tests.

- If the bug indicates a missing test (the most common case), the test that would have caught the bug is written first, observed to fail, then the fix is applied. This is test-driven repair.

- If the bug indicates a logic error in tested code, the test is updated to capture the intended behaviour before the implementation is corrected.

- If the bug indicates a doctrine violation in the codebase (a workaround that broke, a dangling thread that surfaced), the root-cause fix is preferred over a band-aid even if the band-aid is the smaller change. The expansion of scope is documented in the pull request.

- The fix carries the same review, CI, and deployment discipline as any other change. Bug-fix shortcuts are forbidden.

**Workflow: security review**

Security review is initiated either by the change’s author (when the change touches a security-sensitive path) or by a reviewer who observes a security implication during code review. The security-reviewer sub-agent is invoked explicitly.

The agent applies STRIDE (Spoofing, Tampering, Repudiation, Information disclosure, Denial of service, Elevation of privilege) to the change, the OWASP Top 10 where applicable, and the project’s threat model from V5 P23. The agent’s output is a structured findings list with severity per finding. Critical findings block merge until addressed. Medium and Low findings may be accepted with documented rationale by the security function’s representative. Informational findings are noted for retrospective sampling.

In addition to ad-hoc security review on pull requests, the security function performs a structured quarterly security review across the codebase. The quarterly review uses the architect-reviewer and security-reviewer agents in concert against a stratified sample of recently merged work, producing a quarterly findings report shared with the consortium’s Technical Advisory Function.

**Workflow: architecture decision**

Architecture decisions are documented as Architecture Decision Records (ADRs) per the canonical pattern. The recor-adr-author skill produces the ADR draft; the human authors refine and approve.

The ADR template used on this project follows.

> \# ADR \<NNNN\>: \<Title\>
>
> Date: \<YYYY-MM-DD\>
>
> Status: Proposed / Accepted / Deprecated / Superseded by ADR-\<NNNN\>
>
> Authors: \<names\>
>
> Reviewers: \<names\>
>
> \## Context
>
> \<2-4 paragraphs describing the situation that motivates the decision. What problem
>
> exists? Why is it being raised now? What constraints apply?\>
>
> \## Decision
>
> \<The decision itself, stated in one or two sentences with the technical specifics
>
> needed to be unambiguous.\>
>
> \## Considered alternatives
>
> \<For each alternative considered: name, brief description, why not chosen.
>
> At least two alternatives are documented; "no alternatives considered" is
>
> a defect in the ADR.\>
>
> \## Consequences
>
> \<What follows from this decision? What gets easier, harder, more expensive,
>
> more capable? What new commitments does the team take on? What old commitments
>
> become obsolete?\>
>
> \## Doctrines applied
>
> \<Which doctrines from V1 P2 are relevant to this decision and how are they honoured?\>
>
> \## Document references
>
> \<Which sections of this Architecture Document does this ADR affect? An ADR
>
> that affects the document triggers the change procedure in V1 P1.\>
>
> \## Implementation
>
> \<The plan to implement the decision. May be: "Implemented in PR \<link\>", "To
>
> be implemented in sprint \<N\>", "Permanent ongoing operational change".\>

ADRs are numbered sequentially in the project’s ADR directory. ADRs are never deleted; superseded ADRs are marked as such and reference their replacement. The ADR record is the project’s institutional memory and protects against the recurrent debates that erode engineering time when the decision context has been forgotten.

**Workflow: code review**

Code review in an agent-assisted project differs from traditional code review in the proportion of human attention. The agent produces work that meets the explicit doctrines; the human reviewer’s job is to verify what the agent cannot verify on its own behalf — architectural fit, judgement calls that depend on context outside the codebase, the appropriateness of the work to the broader product direction.

Human code review on this project applies the following emphasis. Verify that the plan was followed; deviations from the plan are flagged as either deliberate (with rationale) or accidental (requiring correction). Verify that the doctrines are honoured; the doctrine-check skill should have caught most violations, the human catches the residual. Verify that the work belongs in the change; scope creep is the most common defect in agent-assisted work and is flagged. Verify that the work is consistent with the rest of the codebase; agents lack the cross-codebase judgement that comes from working on the project over months. Verify that the documentation is honest; “this function does X” documentation is checked against the function actually doing X.

The architect-reviewer and security-reviewer sub-agents are invoked as part of the review process when the human reviewer’s context warrants it; they are not invoked for every pull request because the cost of agent invocation must be proportional to the value. Reviewers are trained to recognise which changes warrant which agent invocations.

**Workflow: refactor**

Refactors are scoped operations that change the structure of code without changing its observable behaviour. They are handled by the refactor-specialist sub-agent with strict constraints.

- The refactor’s scope is documented in advance: which files, which functions, which patterns. Scope creep during the refactor is rejected.

- Tests for the affected code paths are verified to exist and to pass before the refactor begins. If tests are inadequate, tests are added first as a separate change before the refactor is attempted.

- The refactor preserves observable behaviour. Observability is verified by the test suite passing without modification of test assertions.

- Where the refactor reveals a defect in the original code (the refactor exposes a bug that was previously masked), the defect is fixed in a separate change with its own tests.

- Large refactors are decomposed into a sequence of smaller refactors per Doctrine 10. A refactor producing a pull request over five hundred lines requires named approval.

**Workflow: incident investigation**

Production incidents trigger an investigation workflow led by the incident-investigator agent under the Incident Commander’s direction. The workflow operates in three phases.

**Phase 1: containment**

The first phase is to contain the impact: stop the bleeding, preserve evidence, communicate status. The incident-investigator surveys observability surfaces in parallel using sub-agents, producing rapid hypotheses about scope and severity. The Incident Commander uses the hypotheses to direct the immediate operational response — traffic shifting, capacity scaling, integration disabling, or other measures that contain impact without compromising evidence.

**Phase 2: investigation**

With the impact contained, the second phase is to identify the cause. The incident-investigator traverses logs, traces, metrics, ledger anchors, recent deployments, recent configuration changes, and recent dependency updates. The investigation is structured: hypotheses are stated explicitly, evidence is gathered for or against each, and hypotheses are ranked by evidential support. The investigation continues until the cause is identified with sufficient confidence that the remediation can be designed.

**Phase 3: remediation and post-incident review**

Remediation is a feature-development workflow against the identified cause, with the additional discipline that the fix is accompanied by the regression test that would have detected the cause before it surfaced. The post-incident review is held within five business days, producing a structured report against the project’s post-incident template. The review focuses on the systemic conditions that permitted the incident, not on individual blame; action items addressing the systemic conditions are tracked through completion.

Post-incident reports for non-sensitive incidents are published to the project’s engineering surface as part of the transparency commitment. The transparency itself is operationally valuable: it builds team learning, signals to consumers and funders that the platform is honestly operated, and provides the basis for ongoing OPSEC and doctrine improvement.

**Workflow: dependency upgrade**

Dependency upgrades are routine but carry security and stability implications. The workflow is automated through Renovate with human review.

- Renovate produces pull requests for dependency updates on a scheduled cadence (weekly for non-security; daily for security updates with the CVE matching enabled).

- CI runs the full test suite against the updated dependency. Failures are surfaced; passing updates may proceed to review.

- Reviewer evaluates the change against the dependency’s changelog, particularly for breaking changes, deprecations, and behaviour shifts that tests may not catch.

- Updates touching dependencies critical to the cryptographic substrate, the verification engine, or the API gateway require additional reviewer signature from the relevant specialist team.

- Major version updates are not auto-merged regardless of CI status; they always require human review and may trigger a small ADR documenting the upgrade rationale.

**Workflow: release**

Releases occur on a documented cadence: minor releases monthly, patch releases as needed, major releases quarterly. The release workflow is operated by the SRE function with engineering team participation.

- Release branch cut from main on the documented schedule.

- Release notes generated by the docs-author agent from the merged pull requests against the release branch.

- Pre-release security audit performed by the security-reviewer agent against the release’s aggregate diff.

- Pre-release performance regression test against the prior release’s baseline.

- Staged rollout: canary deployment to staging, then pre-production, then production with monitoring at each stage.

- Rollback procedures rehearsed before each release; rollback authorization documented in the runbook for the release.

- Post-release verification: synthetic probes confirm functionality across the consumer integration surfaces; SLO compliance confirmed for the first twenty-four hours.

> **NOTE —** Releases are explicit events, not continuous deployment. The platform serves national institutional consumers with their own change-management cycles; uncoordinated continuous deployment would produce friction with consumer integrations and would erode the platform’s perceived stability. Continuous deployment is appropriate for many SaaS products; it is not appropriate for sovereign infrastructure of this character.

**Sample transcripts**

The project maintains a library of representative session transcripts illustrating the workflows in practice. The transcript library is hosted in the central engineering repository and is updated when new patterns emerge. Engineers consult the library when encountering a workflow type for the first time; the library is also the basis for onboarding exercises.

Transcripts are not artefacts that engineers should slavishly imitate. They are illustrative; they show one path through a workflow that proved successful. The doctrines and the workflows are the binding standards; the transcripts demonstrate the standards in operation but do not extend or modify them.

**Metrics on agent-assisted engineering**

The project measures agent-assisted engineering through specific telemetry. Metrics are reviewed monthly by the engineering lead and quarterly by the Technical Advisory Function. Anomalies trigger investigation.

|  |  |
|----|----|
| **Metric** | **Definition and acceptable range** |
| Plan-to-implementation ratio | Median time spent in Plan Mode vs implementation across substantive changes. Target: planning consumes 15–30% of total change time. Below 15% suggests insufficient planning; above 30% suggests inefficient planning or over-decomposition. |
| Outcomes rubric pass rate on first grading | Share of features that pass the outcomes rubric on the grading agent’s first evaluation. Target: ≥80%. Lower rates suggest rubrics that are unachievable, plans that under-specify the rubric criteria, or implementation that is rushed. |
| Doctrine violation findings per merged PR | Average number of doctrine violations the architect-reviewer and security-reviewer agents find per pull request that proceeds to human review. Target: \<0.5. Higher rates suggest doctrine drift; lower rates may suggest the agents are not catching real violations. |
| Human review rejection rate | Share of pull requests rejected at human review (not merged on first review). Target: 10–20%. Lower rates suggest insufficient critical review; higher rates suggest insufficient pre-review preparation. |
| Agent cost per feature | Aggregate inference cost (tokens consumed across orchestrator and sub-agents) divided by features delivered. Target: tracked over time, not against an absolute threshold. Sharp increases prompt investigation; sustained decreases indicate maturing team practice. |
| Time from ticket to merge | Median elapsed time from ticket-in-progress to merged pull request for non-trivial features. Target: tracked over time. Reductions are positive provided doctrine compliance is maintained. |

> **SUCCESS —** The agent-assisted engineering practice has succeeded when: the platform is being built at the velocity that completes the build envelope; the doctrines are honoured at the level documented in V1 P2; the metrics above are within or trending toward their target ranges; the team’s sense is one of focused, productive, high-standard engineering rather than rushed compromise. Success is observable in the project telemetry and in the team’s engagement, not in subjective testimonial.

**Authoritative Technology Stack**

> *This Part is the single source of truth for the technologies used in RÉCOR. The version pinning is exact, the rationale is documented, the forbidden list is binding, and the upgrade governance is non-discretionary. Every dependency added to the platform is added under this Part’s discipline.*

**Top-level technology choices**

The technologies below constitute the platform’s spine. Each is identified by its specific version at the document’s baseline. Version updates progress through the change procedure documented in V1 P1; the document’s version of record at any point in time governs the platform’s build.

**Cryptographic substrate**

|  |  |  |
|----|----|----|
| **Component** | **Version / specification** | **Rationale** |
| Hardware Security Modules | Thales Luna Network HSM 7 with FIPS 140-3 Level 3 certification; firmware 7.13.x baseline; partition allocation per consortium organisation | Industry-standard sovereign HSM; FIPS 140-3 Level 3 is the procurement floor for national-scale key custody; per-organisation partitioning aligns with the consortium’s decentralised key custody model |
| Permissioned ledger — Phase I–III | Hyperledger Fabric v3.1.x with SmartBFT BFT ordering enabled; channel-level capability V3_0 | Mature CFT/BFT ordering, ten-organisation consortium semantics, well-understood operational profile, current LTS in the Fabric v3 line |
| Permissioned ledger — Phase IV+ target | Hyperledger Fabric-X v1.3 LTS (target Q4 2026 release) with HSM-native attested MSP, zero-trust ACLs, K8s Operator | Production-readiness profile for regulated infrastructure: HSM/FIPS, divergence detection, snapshotting; migration is documented as ADR-002 |
| Fabric Certificate Authority | Fabric CA v1.5.19 baseline (April 2026 release); Go 1.26.2 runtime | Latest stable line; current security backports; documented PostgreSQL 17 backend support |
| Threshold signature scheme | FROST-Ed25519 via the ZF FROST reference implementation (Rust), with the BIP-340 schnorrkel companion for Bitcoin anchoring | Production-mature FROST library; 7-of-10 threshold matches the consortium model; Ed25519 is the platform’s primary signature primitive elsewhere; schnorrkel anchoring binds to Bitcoin for sovereignty-independent anchoring |
| Zero-knowledge proof system | Halo2 via the halo2_proofs Rust crate (Zcash Foundation maintained); KZG commitment baseline with circuit-specific parameter generation | No trusted setup (significant political-resilience property); active maintenance; mature tooling; the proof system selected for selective-disclosure use cases |
| External timestamping | OpenTimestamps with self-hosted calendar plus the public OpenTimestamps calendar network; anchored to the Bitcoin blockchain | Sovereignty-independent anchoring substrate that the platform cannot itself manipulate; verifiable by external parties without privileged access; minimal operational dependency |
| Cryptographic primitive library (general) | RustCrypto family crates pinned to 0.X versions specified in workspace Cargo.lock; Ring 0.17.x where RustCrypto coverage is incomplete | Audited Rust implementations; pure-Rust where possible to maintain build determinism; Ring is the fallback for primitives where RustCrypto coverage is not yet production-grade |

**Persistence and data infrastructure**

|  |  |  |
|----|----|----|
| **Component** | **Version / specification** | **Rationale** |
| Primary relational store | PostgreSQL 17.x with the configured extensions: pgcrypto, pg_partman, pg_stat_statements, pgaudit, postgis (selective), pg_trgm | PostgreSQL 17 is the current major; mature operational tooling; encryption-at-rest via transparent disk encryption plus envelope encryption for restricted columns; pgaudit produces the audit log integrated into the platform’s audit channel |
| Graph store | Neo4j Enterprise 5.x clustered in causal-cluster topology with three primaries plus two secondaries per site; APOC and GDS plugins enabled | Industry-standard graph store for the ownership-graph traversals; mature operational tooling; community detection and shortest-path operations are pre-implemented in GDS; causal-cluster topology provides the read scalability the Investigation Workbench requires |
| Full-text search | OpenSearch 2.18.x with custom analyzers for French (snowball-french) and English plus the transliteration filters for Cameroonian name patterns | Apache 2.0 licence (critical for sovereign deployment); compatible with the Elastic ecosystem tooling; transliteration analyzers are the load-bearing capability for fuzzy entity matching across Mbarga/Mbargha/M'Barga variants |
| Event streaming | Apache Kafka 3.8.x with KRaft mode (Zookeeper-free); infinite retention on the audit and declaration topics; compaction on the entity-state topics | Industry-standard event streaming; KRaft mode eliminates the Zookeeper operational dependency; infinite retention is the platform’s system-of-record property for the audit channel |
| Object storage | MinIO RELEASE.2026-x with erasure-coding at the documented EC:8 profile across the two operational sites; server-side encryption with HSM-rooted KMS | S3-compatible API for ecosystem tooling; deployable on commodity hardware on sovereign infrastructure; erasure-coding provides the durability that the platform’s declaration-document store requires |
| Cache and ephemeral state | Redis 8.x in Sentinel topology for the platform’s cache and rate-limit token store; Dragonfly as the comparator runtime considered but Redis chosen for ecosystem maturity | Mature operational tooling; well-understood failure modes; documented operational ownership |
| Time-series store (metrics) | Prometheus 3.x with long-term storage via Thanos to MinIO; retention 90 days local + 5 years remote | De facto standard for Kubernetes observability; Thanos provides the long-term retention without prohibitive Prometheus operational complexity |
| Log aggregation | Grafana Loki 3.x with multi-tenant separation by service; structured JSON ingestion with the platform’s log schema | Lightweight ingestion compared to Elastic; aligned with Grafana ecosystem already in use for metrics dashboards |
| Distributed tracing | Grafana Tempo 2.x with OpenTelemetry instrumentation across every service | OpenTelemetry as the cross-language standard; Tempo as the storage and query backend |

**Service runtime and orchestration**

|  |  |  |
|----|----|----|
| **Component** | **Version / specification** | **Rationale** |
| Container runtime | containerd 2.0.x as the Kubernetes runtime; docker only as a developer convenience, never as a production runtime | Industry-standard CRI implementation; smaller attack surface than Docker daemon; aligned with Kubernetes operational practice |
| Container orchestration | Kubernetes 1.32.x deployed via kubeadm with the documented sovereign control-plane topology | Industry-standard orchestration; well-understood operational profile; deployable on sovereign infrastructure without dependence on a cloud-managed service |
| Service mesh | Istio 1.24.x with the ambient mesh data plane (per-namespace waypoint proxies; sidecar-free pod model) | Mature mTLS automation; Open Policy Agent integration; ambient mode reduces the operational complexity of the sidecar model while preserving the security properties |
| Workload identity | SPIFFE/SPIRE 1.10.x; one SPIFFE trust domain per consortium organisation federated through the consortium’s root trust authority | Industry-standard workload identity; multi-organisation federation; integrates with Istio and OPA seamlessly |
| Policy engine | Open Policy Agent 0.70.x with bundle service for policy distribution; Rego as the policy language | De facto standard for cloud-native authorisation; Rego provides the expressiveness for the platform’s complex policy requirements |
| Secret management | HashiCorp Vault Enterprise 1.18.x deployed in HA topology with HSM-rooted auto-unseal; CSI driver for Kubernetes pod injection | Production-mature secret management; HSM-rooted unseal eliminates the dependency on shamir-split unseal keys at restart; enterprise edition provides the namespacing the multi-tenant consortium model requires |
| API gateway | Envoy Proxy 1.32.x as the data plane; xDS configured via Istio control plane; the platform’s edge gateway uses Envoy directly with the WASM filter chain documented in V4 P15 | Industry-standard L7 proxy; programmability via the WASM filter chain for the platform’s custom authorisation, audit, and rate-limit logic |

**AI inference**

|  |  |  |
|----|----|----|
| **Component** | **Version / specification** | **Rationale** |
| Primary model — Tier A | Claude Opus 4.7 via Anthropic API at api.anthropic.com with extended thinking enabled at xhigh effort | Empirical capability leader as of the document baseline; the load-bearing reasoning capability for the verification engine |
| Cost-sensitive fallback — Tier A | Claude Sonnet 4.6 via Anthropic API for routine work where Opus capability is not required | Cost-discipline complement to the primary; preserves the Anthropic-primary doctrine while permitting model selection on the cost axis |
| Pseudonymised PII reasoning — Tier B | Claude Opus 4.7 via AWS Bedrock PrivateLink in af-south-1 (Cape Town) | African-region inference for data that must not cross the Atlantic; Bedrock provides the IAM-rooted access discipline; PrivateLink eliminates public-internet exposure |
| Pseudonymised PII fallback — Tier B | Claude Sonnet 4.6 via AWS Bedrock PrivateLink af-south-1 | Same rationale as Tier A cost-sensitive fallback |
| Sovereign on-premises — Tier C primary | Llama 3.3 70B Instruct (Meta) deployed on the in-country GPU cluster via vLLM 0.6.x with int8 quantisation | Highest-capability open model deployable on sovereign infrastructure; vLLM provides the production-grade serving runtime; int8 quantisation halves the GPU memory footprint with minimal quality loss |
| Sovereign on-premises — Tier C secondary | Mistral Large 2 (Mistral AI) deployed similarly | Diversification against single-model failure modes; comparable capability profile to Llama 3.3 70B |
| GPU runtime | NVIDIA H100 80GB SXM5 in 8x-cluster topology per operational site (two sites); NVIDIA driver 550.x baseline; CUDA 12.4 | Production-grade inference GPUs; sized for the projected Tier C workload; SXM5 form factor provides the NVLink bandwidth the model parallelism requires |
| Inference gateway | Custom Rust service implementing the three-tier routing logic; documented in V5 P18 | Routing discipline is enforced at the gateway, not by convention in calling services; the gateway is itself a critical path in the platform |

**Build, test, and supply chain**

|  |  |  |
|----|----|----|
| **Component** | **Version / specification** | **Rationale** |
| Source control | Git on Gitea Enterprise 1.23.x self-hosted; mirror replication to a second sovereign site | Self-hosted Git on sovereign infrastructure; mirror replication provides the survivability properties the project requires; Gitea over GitLab on smaller operational footprint at the project’s scale |
| Monorepo build tool | Bazel 7.x with the documented Bzlmod configuration; just as the human-facing command runner over Bazel | Bazel provides the hermetic build properties required for SLSA Level 4 provenance; just provides the ergonomic command interface that lowers the cost of Bazel’s ergonomic ceiling |
| CI/CD platform | GitHub Actions self-hosted runners on dedicated Kubernetes pools for the platform; Argo CD 2.13.x for GitOps deployment | Industry-standard CI/CD with self-hosted runners providing the supply-chain isolation; Argo CD provides the GitOps discipline for deployments |
| Build provenance | SLSA Level 4 with build provenance signed by Sigstore Cosign 2.x; transparency log at sigstore.dev with mirror at the project’s sovereign rekor instance | SLSA Level 4 is the supply chain integrity target documented in Doctrine 20; Sigstore provides the production-mature attestation toolchain |
| SBOM | CycloneDX 1.6 SBOM generation per build artefact; Syft 1.x as the generator | CycloneDX is the cross-ecosystem SBOM standard; Syft has the broadest language coverage |
| Container scanning | Trivy 0.58.x for CVE matching against SBOM; Snyk Container as the secondary scanner for diversification | Two independent scanners detect different vulnerability classes; Trivy is the primary; Snyk is the secondary |
| Static analysis — Rust | Clippy at deny-warnings baseline; cargo-audit for advisory matching; cargo-deny for licence and dependency policy | Clippy is the standard Rust linter; deny-warnings is the project policy; cargo-audit and cargo-deny operationalise Doctrine 20 |
| Static analysis — Go | golangci-lint 1.62.x with the project’s curated linter set; govulncheck for advisory matching | golangci-lint composes the Go ecosystem’s linters; the curated set is documented in the language doctrine |
| Static analysis — TypeScript | ESLint 9.x with the project’s ruleset (extended from airbnb-typescript with project-specific additions); tsc strict mode; depcheck | ESLint 9 flat-config; strict TypeScript is the project default; depcheck catches unused dependencies |
| Static analysis — cross-language | Semgrep 1.x with custom rules for project-specific patterns; CodeQL via the GitHub-Actions-compatible runner | Semgrep for fast pattern matching; CodeQL for deeper data-flow analysis |

**Observability and operations**

|  |  |  |
|----|----|----|
| **Component** | **Version / specification** | **Rationale** |
| Instrumentation | OpenTelemetry SDKs (Rust, Go, TypeScript, Python) at v1.x baseline per language; OTel Collector at the documented topology | Cross-language instrumentation standard; the platform’s consistent instrumentation surface depends on the cross-cutting OTel discipline |
| Metrics dashboards | Grafana 11.x with the platform’s dashboard library version-controlled in code | Industry-standard dashboarding; version-controlled dashboards prevent drift from production state |
| Alerting | Prometheus Alertmanager 0.28.x with the platform’s alert library version-controlled; routing through PagerDuty for on-call coverage | Standard Prometheus alerting; PagerDuty for the operational on-call rotation |
| Runbooks | Markdown in the docs/runbooks/ directory; one runbook per alert; alert payloads link to runbooks | Co-located with code for reviewability; the doctrine that observability is non-optional includes the runbook |
| Status page | Cachet 2.x self-hosted for the consortium’s public status page | Self-hosted for sovereignty; consumer integrations subscribe to the status page programmatically |

**Forbidden technologies**

The technologies in the table below are forbidden in the RÉCOR codebase. The list is binding; substitution is governed through ADR with named approval. The rationale per technology is documented to enable understanding of the decision; the decision itself is the binding artefact regardless of whether the rationale fully captures every concern.

|  |  |
|----|----|
| **Forbidden technology** | **Rationale** |
| C and C++ for new code | Memory-safety profile is incompatible with the platform’s security posture; Rust is the equivalent-performance alternative without the memory-safety footgun. Exception: third-party C/C++ libraries via FFI where Rust equivalent is not yet production-grade (e.g., HSM SDKs from Thales) |
| Java for new code | Operational complexity (JVM tuning, garbage-collection pause profile), supply chain breadth (Log4Shell taught the lesson), and the absence of a clear advantage over Go for the platform’s service profile. Exception: HSM SDKs and certain government legacy adapters where Java is the only available SDK |
| Python for non-ML production code | Performance profile and the runtime complexity of operational Python deployment make it unsuitable for the platform’s service tier. Exception: ML pipelines (data preparation, model evaluation, batch scoring) where Python’s ecosystem advantage is decisive |
| Ruby, PHP, Perl | No ecosystem coherence with the platform’s primary languages; insufficient cryptographic library maturity for the verification engine; small operational community within Cameroonian engineering talent |
| .NET | Operational complexity for cross-platform deployment; ecosystem alignment with primarily Microsoft-stack environments; insufficient ecosystem advantage over Go for the platform’s service profile |
| MongoDB | Operational characteristics in failure modes are insufficiently transparent for sovereign-grade operations; PostgreSQL with JSONB columns provides the document-store capability the platform requires while preserving relational guarantees |
| Cassandra and ScyllaDB | Distributed system complexity disproportionate to the platform’s scale requirements; PostgreSQL plus the documented partition strategy handles the platform’s throughput profile |
| AWS proprietary services for primary workloads | Sovereign deployment requirement is incompatible with cloud-proprietary primary dependencies; AWS Bedrock for Tier B inference is the named exception under explicit consortium approval |
| Closed-source databases without source-code escrow | Operational risk in vendor failure modes; the platform requires source-code-level transparency on data-tier components |
| Cryptocurrency, smart-contract platforms (Ethereum and forks) | No legitimate operational need; introduces attack surface and political complexity without corresponding benefit |
| Generative AI for production code generation without doctrine compliance | Use of generative AI is permitted only through Claude Code as documented in V2 P5; ad hoc GPT-4 or Gemini for code generation outside that discipline violates Doctrine 22 and is forbidden |

**Dependency policy**

Dependencies are managed under a strict policy that operationalises Doctrines 19 (reproducible everything) and 20 (supply chain integrity).

- Every dependency is pinned to a specific version with cryptographic hash verification in the lockfile.

- Dependencies are consumed only from approved registries: crates.io for Rust, the Go module proxy at proxy.golang.org for Go, the npm registry at registry.npmjs.org for TypeScript, PyPI for Python. The project mirrors each registry through the consortium’s artifact repository (Nexus 3.x or Artifactory) to provide supply-chain isolation and disaster-recovery resilience.

- Each new dependency requires explicit approval through a lightweight review: licence compatibility (MIT, Apache-2.0, BSD, MPL-2.0 acceptable; GPL/AGPL forbidden for the codebase; LGPL admissible for runtime-linked libraries only), maintenance status (last commit within twelve months, active issue triage), security history (no unaddressed advisories above Medium severity in the past twelve months), and operational reputation (used by other sovereign or regulated systems, or by recognised open-source projects).

- Dependencies of dependencies (transitive dependencies) are inspected through SBOM analysis. Where a transitive dependency is itself forbidden by the policy, the direct dependency is rejected until the transitive is resolved.

- Upgrade cadence: security patches within seven days of disclosure for Critical and High advisories, thirty days for Medium, ninety days for Low; minor and patch version updates monthly through automated Renovate pull requests; major version updates ad hoc with ADR documentation.

- Critical dependencies (the verification engine’s direct dependencies, the cryptographic library set, the inference SDKs) carry enhanced scrutiny: source-code review at first adoption, version-pin reviews at every upgrade, contribution monitoring through the dependency’s public repository.

**Version upgrade governance**

Version upgrades to the technologies documented in this Part are governed by the same change procedure that governs this document (V1 P1). The change procedure is the binding mechanism; ad hoc upgrades that bypass the procedure are doctrine violations regardless of the upgrade’s technical merits. Specifically, upgrades to: the cryptographic substrate, the AI inference models, the persistence layer technologies, the orchestration platform, and the build toolchain trigger formal ADR with named approval. Upgrades to libraries within an established technology choice (a new patch version of a Rust crate, for instance) are routine and proceed through the Renovate-mediated workflow without formal ADR.

> **NOTE —** Version pins in this document are baselines, not the floor. The platform may run on a higher patch version of any dependency at any moment provided the security and dependency policies are honoured. The document is updated at minor version boundaries where the change carries architectural significance; patch versions are tracked through the lockfiles and the Renovate record without document revision.

**Security advisory tracking**

The platform tracks security advisories across every dependency through three channels operating in concert: Anthropic’s own security advisory feed for the AI inference paths; CVE feeds matched against the SBOM via Trivy and Snyk; and the language-specific advisory channels (RustSec advisory database for Rust, the Go vulnerability database for Go, npm audit for TypeScript, PyPI advisory database for Python). Advisories above the response thresholds documented in the dependency policy trigger the corresponding upgrade or mitigation.

The security function operates a weekly review of the advisory queue, with escalation to engineering leadership for any advisory affecting the cryptographic substrate or the verification engine’s dependencies. The advisory record is itself audited quarterly for completeness against the public advisory streams.

> **SUCCESS —** The technology stack is operationally well-governed when: every production dependency is pinned and reproducible; every dependency’s licence is in the approved set; no advisory above the response threshold is unaddressed beyond its window; the SBOM for every production artefact is current and valid; and every version upgrade in the recent quarter passed through the documented governance. The success criterion is observable in the project’s supply-chain telemetry and is audited quarterly.

**Programming Languages Doctrine**

> *RÉCOR uses four languages in production: Rust for performance-critical and security-critical paths, Go for control-plane and integration services, TypeScript for the application layer, and Python for ML pipelines only. The language for any new code is determined by the doctrine, not by the engineer’s preference.*

**Language assignment by layer and concern**

The language assignment is documented per architectural layer and per cross-cutting concern. The assignment is not negotiable on stylistic grounds; deviation requires documented rationale through ADR.

|  |  |  |
|----|----|----|
| **Layer / concern** | **Language** | **Rationale** |
| Layer 0 — cryptographic primitives, FROST coordination, ZK circuits, OpenTimestamps client, HSM client wrappers | Rust | Memory safety is non-negotiable in cryptographic code; performance profile required for ZK proof generation; pure-Rust audited primitives are available from the RustCrypto ecosystem |
| Layer 0 — Fabric chaincode | Go | Fabric chaincode is canonically written in Go; the Fabric Gateway client API is most mature in Go; no Rust chaincode SDK at production maturity |
| Layer 1 — data infrastructure operators (custom) | Go | Kubernetes operator framework is most mature in Go; sufficient performance for control-plane operations |
| Layer 2 — Entity, Person, Declaration, Verification State, Document services | Rust | Hot paths in the platform’s critical write and read flows; Rust’s performance and correctness profile is the load-bearing engineering property |
| Layer 2 — Workflow, Notification, Audit Aggregator, Schedule services | Go | Less performance-critical control-plane services where Go’s ergonomics and ecosystem (Temporal SDK, Notification SDKs) make implementation faster without compromising the platform’s requirements |
| Layer 2 — ML evaluation, training pipelines, data preparation | Python | Python is the canonical ML language; the ecosystem advantage (PyTorch, Hugging Face Transformers, scikit-learn) is decisive |
| Layer 3 — verification engine pipeline, signature implementations, Dempster–Shafer fusion, inference client | Rust | Highest-stakes reasoning in the platform; performance and correctness profile of Rust is the appropriate choice |
| Layer 4 — GraphQL gateway, REST endpoints, webhook subscribers, rate limiter, BODS exporter | Rust | API surface is performance-sensitive (rate limiter especially); async-graphql and axum provide production-mature frameworks |
| Layer 5 — consumer integration adapters | Rust for synchronous fail-closed integrations (ARMP, customs); Go for asynchronous and batch integrations (DGI bulk, sectoral cadastres) | Per-integration choice based on the integration’s performance and reliability requirements; documented per integration in V4 P16 |
| Layer 6 — web applications (Declarant Portal, Officer Console, Investigation Workbench, Public Portal, Admin Console) | TypeScript with React 19 | React is the de facto frontend framework; TypeScript provides the type safety; strict mode is the project doctrine |
| Layer 6 — native mobile wrappers | TypeScript with Capacitor 6 wrapping the PWA | Single codebase across web and native; Capacitor provides the platform shims for iOS and Android without React Native’s ecosystem complexity |
| Layer 6 — whistleblower intake (Tor service) | Rust | Operationally isolated; Rust’s minimal runtime is appropriate for a security-isolated service |
| Build tooling, scripts, internal CLIs | Go for non-trivial CLIs; just for shell-equivalent task orchestration; Rust for tools that ship as cargo binaries within the platform | Go is the canonical CLI language with strong ecosystem; just is the human-facing task runner; Rust where the tool is part of the platform’s binary distribution |
| Infrastructure as code | HCL (Terraform) for cloud and IaC primitives; YAML for Kubernetes manifests; Helm for templated Kubernetes deployments; Rego for policy | Each tool’s native language; engineers do not write Go code that generates Terraform |

**Rust doctrine**

**Edition and toolchain**

Rust 2024 edition; rustup-managed toolchain pinned to the version in rust-toolchain.toml at the workspace root. The toolchain version is the document’s baseline at any given moment; upgrades follow the technology stack version upgrade governance (V3 P7).

**Cargo workspace organisation**

Each Rust service is a Cargo workspace at the service directory root. Workspaces follow a uniform structure:

> services/entity/
>
> ├── Cargo.toml \# workspace manifest
>
> ├── Cargo.lock \# checked in; reproducible builds
>
> ├── rust-toolchain.toml \# pinned toolchain
>
> ├── .cargo/config.toml \# workspace cargo configuration
>
> ├── crates/
>
> │ ├── entity-domain/ \# pure domain types and logic, no I/O
>
> │ ├── entity-storage/ \# persistence layer adapters
>
> │ ├── entity-grpc/ \# gRPC server implementation
>
> │ ├── entity-http/ \# HTTP/REST companion server
>
> │ ├── entity-cli/ \# operational CLI for the service
>
> │ └── entity-server/ \# composition root: binary that wires together
>
> ├── proto/ \# protobuf contracts for the service
>
> ├── migrations/ \# SQL migrations
>
> ├── tests/ \# workspace-level integration tests
>
> └── benches/ \# performance benchmarks

**Coding standards**

- Format: rustfmt with the project’s rustfmt.toml; non-conforming code is rejected by CI. Maximum line length 100 characters; tab equivalent four spaces.

- Lints: clippy at deny-warnings with the project’s allowed lint list documented in .cargo/config.toml. Allowed lints are explicit and reviewed quarterly.

- Error handling: thiserror for library errors; anyhow for application-level error propagation; the ? operator for propagation; no unwrap() or expect() in production paths.

- Async runtime: tokio 1.x; service code is async by default; CPU-bound work is dispatched to rayon or to dedicated blocking pools.

- Logging: tracing crate with structured fields; no log macros; no println!. Log records are JSON-formatted at the production binary; pretty-formatted in tests and development.

- Result types: every fallible function returns Result; the project’s error types are documented per service in the service’s CLAUDE.md and adhere to the project’s error taxonomy.

- Memory safety: unsafe blocks are forbidden by default and require named approval per use. Each unsafe block is documented inline with the safety invariants that justify its use.

- Concurrency: shared mutable state is forbidden across async tasks except through documented synchronisation primitives (Arc\<Mutex\>, Arc\<RwLock\>, mpsc channels, watch channels). Data races are caught by tokio’s thread-sanitizer profile in CI.

- Testing: \#\[cfg(test)\] modules within each crate for unit tests; tests/ directory for integration tests; the proptest crate for property tests; criterion for benchmarks.

**Project Rust idioms**

- Domain types implement Clone only where Clone is operationally needed. By default, domain types are move-only to discourage accidental copying of large structures.

- Newtype wrappers around primitive types where the primitive’s value space exceeds the domain’s value space. “struct EntityId(uuid::Uuid)”, not “uuid::Uuid” directly as the entity identifier.

- Constructors that validate domain invariants return Result\<Self, Error\>; constructors that cannot fail are the standard new() pattern. Domain types are constructed only through their documented constructors; struct literal construction is forbidden except in tests.

- Public API surfaces accept and return the platform’s canonical domain types, not transport-specific types (protobuf messages, JSON values). Translation between transport and domain happens at the I/O boundary.

- Async functions in the public API return BoxFuture or impl Future\<Output = Result\<T, E\>\>; the project does not return async fn directly in trait definitions until the trait-async stabilisation matures further than its baseline at the document date.

**Go doctrine**

**Toolchain**

Go 1.26.x baseline (1.26.2 minimum at the document baseline); managed through the project’s mise.toml. Go modules with the central proxy mirror. Go workspaces (go.work) for the monorepo coordination.

**Service organisation**

Go services follow the structure documented below, derived from the standard Go project layout with the project’s additions:

> services/workflow/
>
> ├── go.mod
>
> ├── go.sum
>
> ├── cmd/
>
> │ └── workflow-server/ \# main package
>
> ├── internal/ \# private to this service
>
> │ ├── domain/ \# domain types and logic
>
> │ ├── storage/ \# persistence adapters
>
> │ ├── grpc/ \# gRPC server
>
> │ ├── http/ \# HTTP/REST companion
>
> │ └── schedule/ \# Temporal workflow definitions
>
> ├── pkg/ \# public packages (when needed)
>
> ├── proto/ \# protobuf contracts
>
> ├── migrations/ \# SQL migrations
>
> └── tests/ \# integration tests

**Coding standards**

- Format: gofmt and goimports are applied automatically by the post-edit hook; CI verifies.

- Lints: golangci-lint 1.62.x with the curated linter set documented in .golangci.yml: errcheck, gosec, govet, staticcheck, ineffassign, unused, gocyclo (threshold 15), revive, gosimple, prealloc.

- Error handling: explicit error returns; errors wrapped with fmt.Errorf("%w") for chain preservation; errors.Is and errors.As for checking; sentinel errors defined as package-level vars where appropriate; never panic in production code paths.

- Concurrency: goroutines with documented lifetime management; context.Context first parameter for cancellation propagation; sync.WaitGroup or errgroup.Group for coordination; channels for communication.

- Logging: log/slog with structured fields; JSON output in production; structured key=value in development.

- Configuration: koanf for layered configuration loading from environment, files, and consortium’s config service.

- Testing: standard testing package; gomock-generated mocks; testify/assert for ergonomic assertions; table-driven tests as the standard pattern.

**Project Go idioms**

- Interfaces are defined at the consumer, not at the producer. Small interfaces composed where larger behaviour is needed (“accept interfaces, return structs”).

- Context first parameter on all I/O functions. Context cancellation is respected throughout.

- Database access via sqlc-generated type-safe code, not via the raw database/sql interface.

- Service initialisation in cmd/\<service\>/main.go with explicit dependency wiring; the project does not use dependency-injection frameworks (Wire, fx) at the document’s baseline due to ecosystem ambiguity, but the topic is open to revisit through ADR.

- gRPC services implemented from the generated server interface; not hand-written.

**TypeScript doctrine**

**Toolchain**

TypeScript 5.7.x with strict mode mandatory; Node.js 22.x LTS for tooling; pnpm 9.x for package management; tsx as the TS execution runtime where Node is needed at runtime; Vite 6.x as the build tool for application code.

**Strict configuration**

The project’s tsconfig.json sets every strictness flag. The configuration is shared across applications through a base tsconfig with per-application overrides only for module and target settings.

> {
>
> "compilerOptions": {
>
> "target": "ES2022",
>
> "module": "ESNext",
>
> "moduleResolution": "bundler",
>
> "jsx": "react-jsx",
>
> "strict": true,
>
> "noImplicitAny": true,
>
> "strictNullChecks": true,
>
> "strictFunctionTypes": true,
>
> "strictBindCallApply": true,
>
> "strictPropertyInitialization": true,
>
> "alwaysStrict": true,
>
> "noImplicitThis": true,
>
> "useUnknownInCatchVariables": true,
>
> "noUnusedLocals": true,
>
> "noUnusedParameters": true,
>
> "noImplicitReturns": true,
>
> "noFallthroughCasesInSwitch": true,
>
> "noUncheckedIndexedAccess": true,
>
> "noImplicitOverride": true,
>
> "exactOptionalPropertyTypes": true,
>
> "isolatedModules": true,
>
> "esModuleInterop": true,
>
> "forceConsistentCasingInFileNames": true,
>
> "skipLibCheck": true,
>
> "resolveJsonModule": true
>
> }
>
> }

**Application organisation**

Frontend applications follow a feature-folder organisation with the documented per-feature structure. Cross-feature primitives live in a shared library consumed by every application.

> applications/declarant-portal/
>
> ├── package.json
>
> ├── tsconfig.json
>
> ├── vite.config.ts
>
> ├── src/
>
> │ ├── main.tsx \# composition root
>
> │ ├── App.tsx \# router and providers
>
> │ ├── features/
>
> │ │ ├── declaration/ \# declaration filing feature
>
> │ │ │ ├── components/ \# React components scoped to feature
>
> │ │ │ ├── hooks/ \# feature-specific hooks
>
> │ │ │ ├── services/ \# API client and offline-sync code
>
> │ │ │ ├── types.ts \# feature types
>
> │ │ │ └── index.ts \# feature public API
>
> │ │ └── entity-search/
>
> │ ├── shared/ \# cross-feature primitives within this app
>
> │ └── sw/ \# service worker code
>
> ├── tests/ \# integration tests; unit tests live in features/
>
> └── public/ \# static assets

**Coding standards**

- Lints: ESLint 9 with the flat config; the project’s ruleset extending eslint-config-airbnb-typescript with the project additions for accessibility (eslint-plugin-jsx-a11y) and React hooks (eslint-plugin-react-hooks).

- Formatting: Prettier 3.x; the project’s .prettierrc enforces 100-character line length, single quotes, trailing commas, semicolons.

- React components: functional components only; class components forbidden. Hooks-based state management.

- State management: per-feature local state via useState/useReducer; cross-feature state via TanStack Query for server state and Zustand for client state. Redux is forbidden — the team’s judgement is that Redux’s ergonomic cost exceeds its benefit at the platform’s scale.

- API client: TanStack Query with the project’s typed client generated from the OpenAPI/GraphQL schemas via openapi-typescript or graphql-codegen. No hand-written API clients.

- Forms: react-hook-form with Zod schemas for validation. Schemas are shared between the API client (for response validation) and the form (for input validation).

- Styling: Tailwind CSS v4 with the project’s design tokens; no per-component CSS files; no CSS-in-JS libraries except where Tailwind cannot express the styling (rare).

- Internationalisation: react-i18next with the project’s translation files; French is the primary language; English and Pidgin English are secondary. RTL languages not currently supported (no Cameroonian RTL constituency).

- Routing: React Router 7.x in framework mode; nested routes; loaders for data fetching at the route level.

**Project TypeScript idioms**

- Discriminated unions for state machines and for variant data. “type Result\<T, E\> = { ok: true; value: T } \| { ok: false; error: E }”, accessed by exhaustive switch.

- Branded types for domain identifiers. “type EntityId = string & { readonly \_tag: ‘EntityId’ }” constructed only through validated factories.

- No ‘any’ in production code; ‘unknown’ plus narrowing at the boundary. The eslint rule no-explicit-any is at error level.

- Public functions accept and return readonly types where mutation is not part of the contract. Readonly is the default for arrays and objects in shared types.

- Async functions return Promise\<Result\<T, E\>\> for fallible operations, not throwing. Throwing is reserved for programmer-error conditions (assertions, invariants), not for expected failures.

**Python doctrine (ML only)**

**Toolchain**

Python 3.12.x baseline; uv as the package manager and virtual-environment tool (replacing pip/poetry); ruff for linting and formatting; mypy strict for type checking; pytest for testing. Python is permitted only for ML data pipelines, model evaluation, batch scoring, and the AI inference audit framework; it is not permitted for production services.

**ML pipeline organisation**

ML code organisation follows a structured monorepo pattern with separate directories for data preparation, model training, model evaluation, and batch scoring. Each pipeline is itself runnable in isolation and is integrated into the platform through Temporal workflows that invoke the pipeline as a containerised job.

**Coding standards**

- Format: ruff format (Black-compatible) applied automatically.

- Lints: ruff check with the project’s ruleset; mypy strict for type checking; no untyped functions in production code.

- Dependency management: uv with the project’s pinned uv.lock; no direct pip use.

- Library choice: PyTorch 2.x as the ML framework baseline; Hugging Face Transformers for transformer model interaction; scikit-learn for classical ML; pandas + DuckDB for data preparation; pyarrow for data exchange with the rest of the platform.

- Testing: pytest with the project’s fixtures; hypothesis for property tests; data validation through Great Expectations for the data preparation pipelines.

- Notebook discipline: Jupyter notebooks for exploration; production code lives in .py files; notebooks are not deployed to production. Notebook-to-production-code migration is documented in the ML pipeline migration runbook.

**Cross-language interoperability**

Services communicate across language boundaries through gRPC over mTLS, with the contracts authored in Protocol Buffers v3. Schemas are version-controlled in the proto/ directory of the originating service and consumed through the buf-generated language bindings (Rust via tonic-build, Go via protoc-gen-go-grpc, TypeScript via @bufbuild/protobuf, Python via betterproto). Breaking schema changes are detected by buf breaking and blocked in CI.

Event-driven communication is via Kafka with Apache Avro schemas in the project’s Confluent Schema Registry (self-hosted). Forward and backward compatibility rules are enforced through schema-registry compatibility levels. Schema versioning is mandatory; consumers tolerate forward schema evolution within the documented compatibility window.

REST and GraphQL surfaces from the platform to external consumers are described in OpenAPI 3.1 (REST) and the GraphQL SDL (GraphQL). The schemas are version-controlled and serve as the source of truth from which language-specific client and server code is generated.

**Build tooling and command interface**

The human-facing command interface is just (the command runner). Every repository carries a justfile with a uniform set of commands. The uniformity matters: an engineer entering any repository finds the same commands, lowering the cognitive overhead of working across the polyglot monorepo.

> \# justfile (uniform commands across every service)
>
> default:
>
> @just --list
>
> \# Run all checks: format, lint, test
>
> check: fmt lint test
>
> \# Format the code (no-op if no changes)
>
> fmt:
>
> @just \_fmt-{{ language }}
>
> \# Lint the code
>
> lint:
>
> @just \_lint-{{ language }}
>
> \# Run tests
>
> test:
>
> @just \_test-{{ language }}
>
> \# Build the production artefact
>
> build:
>
> @just \_build-{{ language }}
>
> \# Run the service locally (development mode)
>
> run:
>
> @just \_run-{{ language }}
>
> \# Run integration tests against ephemeral environment
>
> integration-test:
>
> @just \_integration-test-{{ language }}
>
> \# Generate code from contracts (proto, openapi, graphql)
>
> gen:
>
> buf generate
>
> \# Apply database migrations (development environment only)
>
> migrate:
>
> @just \_migrate-{{ language }}
>
> \# Sync dependencies to the lockfile
>
> deps-sync:
>
> @just \_deps-sync-{{ language }}

Bazel underlies just for the hermetic build properties required by SLSA Level 4. Engineers invoke just commands; Bazel runs underneath. The project does not require engineers to be Bazel experts; it requires them to be just users with a documented learning path into Bazel for the cases where the abstraction leaks.

**Language interop with Claude Code**

Each language’s tooling integrates with Claude Code through the project’s skills. The recor-rust-service, recor-go-service, and recor-react-app skills produce service scaffolding aligned with this Part’s standards. The doctrine-check skill knows about each language’s anti-patterns. The test-author skill produces tests in the language idioms documented above. Engineers do not have to instruct the agent on language standards; the standards are encoded in the skills.

> **SUCCESS —** The language doctrine is operationally honoured when: every new service is implemented in the language assigned by this Part; the per-language coding standards are honoured at the file level (measured by linter pass rates); cross-language interoperability is via the documented contract mechanisms; and language-specific anti-patterns are absent from the codebase as confirmed by the architect-reviewer and security-reviewer agents. The success criterion is observable in the project’s code-quality telemetry.

**Architectural Principles and Patterns**

> *The principles in this Part are how the platform is engineered. They are not aspirational. Every service, every integration, every application is built against these principles; deviations are documented as ADRs with explicit rationale.*

**The twelve principles**

Twelve principles govern the platform’s architectural posture. Each is stated, then operationalised in subsequent sections.

- Principle 1 — Domain-driven boundaries. The platform is decomposed into bounded contexts whose boundaries match the domain’s natural seams: entities, persons, declarations, verification, evidence, access, audit, workflow, schemas, notification, identity. Services correspond to bounded contexts; services do not share databases; cross-context communication is through explicit contracts.

- Principle 2 — Separation of read and write. Where read and write workloads diverge significantly (verification queries reading vast graph contexts; declaration writes touching modest state), the read model is materialised separately from the write model. CQRS is applied selectively, not as a doctrinal commitment, where the workload divergence justifies the complexity.

- Principle 3 — Event sourcing for high-stakes domains. Declarations, verification outcomes, lane decisions, and access events are event-sourced: the canonical record is the append-only sequence of events, with current state derived through projection. Other domains (entity attributes, person identity) are state-stored with audit logs.

- Principle 4 — Saga for cross-service workflows. Workflows that span multiple bounded contexts (declaration submission triggering verification triggering lane decision triggering consumer notification) are coordinated through choreographed sagas with explicit compensation. Distributed transactions across services are forbidden.

- Principle 5 — Outbox for reliable event emission. Services that produce events to Kafka write the event to a local outbox table in the same database transaction as the state change. A background dispatcher publishes from the outbox to Kafka at-least-once; consumers handle duplicates through Doctrine 13 idempotency.

- Principle 6 — Circuit breakers and bulkheads at every external boundary. Calls to consumer integrations, to AI inference, to external feeds (sanctions, PEP, adverse media) operate under circuit breakers that fail fast when the dependency is unhealthy. Bulkheads (separate thread pools, separate connection pools) prevent dependency failure from propagating to the platform’s core.

- Principle 7 — Idempotency at every state-changing boundary. Doctrine 13 instantiated: every state-changing operation accepts an idempotency token; the operation’s implementation honours the token; replay of the same operation produces the same outcome. The platform’s at-least-once messaging semantics depend on this principle holding universally.

- Principle 8 — Soft-real-time, never hard-real-time. The platform commits to operational SLOs on the order of milliseconds (KYC lookup), seconds (ARMP webhook), or minutes (DGI bulk export). It does not commit to hard real-time guarantees in the operating-systems sense, and engineers design for soft-real-time profiles.

- Principle 9 — Eventual consistency where strong consistency is unnecessary. Read models lag write models by a documented bound (typically seconds). Operations that require strong consistency (declaration uniqueness, lane decisions) operate against the strongly-consistent write model; operations that tolerate eventual consistency (searches, dashboard aggregations) operate against read models.

- Principle 10 — Backpressure and load shedding. Every service implements backpressure: rejected work returns explicit signals to callers, not implicit queue growth. Load shedding kicks in at documented thresholds: priority queues for declarations, age-based eviction for non-critical work, circuit breakers on dependencies.

- Principle 11 — Observability before optimisation. No performance optimisation is undertaken without observability that confirms the bottleneck. Profiles, traces, and metrics drive optimisation decisions; intuitive optimisation is forbidden.

- Principle 12 — Defence in depth at every layer. No single security mechanism is the sole protection for any class of attack. Network policies, mTLS, OPA policies, application-level authorisation, audit logging, and anomaly detection all run in concert; the failure of any single layer does not by itself permit successful attack.

**Domain-driven design at RÉCOR**

The platform applies DDD at the strategic level (bounded contexts, context maps, ubiquitous language) and at the tactical level (aggregates, entities, value objects, domain events). The strategic decomposition is documented in V4 P13 (services); the tactical patterns are documented per service in their CLAUDE.md files.

**Bounded contexts**

The platform’s twelve bounded contexts are documented below with their primary aggregates and their cross-context relationships.

|  |  |  |
|----|----|----|
| **Bounded context** | **Primary aggregates** | **Upstream / downstream relationships** |
| Entity | LegalEntity, EntityAttribute, OwnershipChain | Source for: Person, Declaration, Verification. Consumes from: CFCE source-of-record, OHADA registry. |
| Person | NaturalPerson, PersonIdentifier, NaturalPersonAlias | Source for: Entity (as ultimate BO), Declaration, Verification. Consumes from: BUNEC, NIU, immigration database. |
| Declaration | Declaration, DeclarationAmendment, DeclarationHistory | Triggers: Verification. Consumes: Entity, Person. |
| Verification | VerificationCase, VerificationStage, VerificationOutcome, EvidencePackage | Triggers: LaneDecision, ConsumerNotification. Consumes: Declaration, Entity, Person, Evidence, plus external feeds. |
| Evidence | EvidenceArtefact, EvidenceProvenance | Source for: Verification. Provenance from: cryptographic substrate. |
| LaneDecision | LaneOutcome, LaneRationale, AppealRecord | Triggers: ConsumerNotification, AuditTrail. Consumes: Verification. |
| Access | AccessGrant, AccessRequest, AccessPolicy, AccessJustification | Cross-cuts every other context for authorisation. Consumes from: Identity. Policy-engine target. |
| Audit | AuditEvent, AuditLog, AuditAnchor | Sink for cryptographically-anchored consequential events. Anchors to: Cryptographic substrate. |
| Workflow | WorkflowDefinition, WorkflowInstance, WorkflowStep | Orchestrates cross-context sagas. Consumes from: all event streams. |
| Schema | SchemaDefinition, SchemaVersion, SchemaMigration | Cross-cuts every persistent context. Governs schema evolution. |
| Notification | NotificationChannel, NotificationDispatch, NotificationDelivery | Triggers external delivery to consumer integrations and to declarants. Consumes from: all triggering event streams. |
| Identity | Principal, AuthSession, Credential, GroupMembership | Source for: Access, Audit. Consumes from: Keycloak as the IdP. |

**Ubiquitous language**

The project maintains a glossary of the ubiquitous language, version-controlled in docs/glossary.md. Terms have a single meaning across code, documentation, contracts, and human conversation. Drift in terminology is treated as a defect; the doctrine on completeness includes correct terminology. Representative terms with their defined meanings:

- **Declarant.** The natural or legal person responsible for submitting a beneficial ownership declaration. May or may not be the beneficial owner.

- **Beneficial owner.** The natural person who ultimately controls a legal entity, per the legal definition in the implementing legislation. Often abbreviated BO.

- **Front person.** A natural person who is recorded as a beneficial owner but whose actual control over the entity is exercised by a different natural person. Detection of fronts is the dominant adversarial reasoning challenge.

- **Lane decision.** The outcome of the verification engine for a given declaration: green (accepted), yellow (requires analyst review), red (rejected or flagged for investigation).

- **Evidence package.** The structured collection of artefacts — documents, screening results, AI reasoning outputs, anchored events — that support a verification outcome. Evidence packages are reviewable by analysts, appealable by declarants, and disclosable on judicial demand.

- **Consumer integration.** An interface through which an external institution (ARMP, ANIF, DGI, BEAC, customs, sectoral cadastres, CONAC, INTERPOL/StAR) consumes platform data.

- **Threshold-signed operation.** A consequential operation that requires the FROST 7-of-10 quorum with at least one non-state signature.

- **Public tier.** The publicly-accessible portion of the BO register, statutorily grounded and not subject to administrative discretion.

- **Restricted tier.** BO data accessible only to authorised consumers under role-based and justification-based access policies.

- **Encrypted tier.** BO data that requires threshold-signed quorum approval for any access.

**CQRS application**

CQRS (Command Query Responsibility Segregation) is applied where read and write workloads diverge enough to warrant the complexity. The decision is per-context.

- Entity context: applies CQRS. Write model is in PostgreSQL with the canonical entity state. Read models are: (a) the Neo4j ownership-graph projection for traversal queries, (b) the OpenSearch index for fuzzy matching and full-text queries, (c) the Redis cache for high-frequency identifier-to-summary lookups.

- Declaration context: applies CQRS. Write model is event-sourced in PostgreSQL with the declaration event log. Read models are: (a) the current-declaration projection for KYC lookup, (b) the historical-declaration projection for amendment tracking.

- Verification context: applies CQRS. Write model is the verification case state in PostgreSQL with stage outcomes event-sourced. Read models are: (a) the open-cases queue for analyst routing, (b) the evidence-package read model for analyst review, (c) the lane-decision projection for consumer integration.

- Person context: state-stored, not CQRS. Read and write workloads are comparable in profile; the simpler model is preferred.

- Access context: state-stored, not CQRS. Authorisation lookups are high-frequency but operate against a small in-memory cache populated from the state.

**Event sourcing application**

Event sourcing is applied to high-stakes domains where the audit trail of state changes is itself part of the system’s value, not merely a side effect.

**Event-sourced contexts**

- Declaration: every declaration event (submission, amendment, withdrawal, correction) is appended to the declaration event log. Current declaration state is derived through projection. The event log is the source of truth; the projection is regenerable from the log.

- Verification: every verification stage outcome, every analyst action, every lane decision is appended to the verification event log.

- Lane decision: every lane decision and every appeal action is appended to the lane decision event log.

- Access: every access request, every access grant, every access exercise is appended to the access event log.

**Event schema discipline**

Events are first-class citizens in the platform. Each event type has: a named event type identifier; a versioned schema in the project’s schema registry; a documented semantic meaning; an example payload; and a forward-compatibility policy. Events are immutable once written; corrections are themselves new events that supersede or compensate prior events.

Representative event schema for the canonical declaration-submitted event:

> \# DeclarationSubmitted v3
>
> \# Emitted when a beneficial ownership declaration is successfully accepted
>
> event_type: "recor.declaration.declaration_submitted"
>
> event_version: 3
>
> event_id: \<UUID v7\> \# time-sortable for ordering
>
> event_time: \<ISO 8601 UTC\>
>
> emitter: "declaration-service@\<service-instance-id\>"
>
> correlation_id: \<UUID\>
>
> causation_id: \<UUID\> \# the request or event that caused this
>
> payload:
>
> declaration_id: \<UUID v7\>
>
> entity_id: \<UUID v7\>
>
> declarant_principal: \<SPIFFE ID\>
>
> declarant_role: "self" \| "authorised_agent" \| "operator_assisted"
>
> declaration_kind: "incorporation" \| "annual_renewal" \| "change_of_control"
>
> \| "correction" \| "amendment"
>
> ubo_persons: \[\<PersonId\>\]
>
> ubo_chain_summary:
>
> direct_owners: \[{entity_id\|person_id, percent}\]
>
> chain_depth_max: \<int\>
>
> chain_count: \<int\>
>
> documents_attached: \[\<DocumentId\>\]
>
> cryptographic_attestation:
>
> signed_by: \<PrincipalId\>
>
> signature_algorithm: "ed25519"
>
> signature: \<hex\>
>
> nonce: \<hex\>
>
> ledger_anchor:
>
> fabric_channel: "declaration-events"
>
> fabric_block: \<int\> \# populated after ledger commit
>
> ledger_tx_id: \<hex\> \# populated after ledger commit
>
> ots_calendar_status: "submitted" \| "anchored"
>
> provenance:
>
> schema_uri: "recor://schemas/declaration_submitted/v3"
>
> schema_hash: \<hex\> \# binds event to its schema version
>
> source_truststore: "recor-prod"

**Saga pattern**

Cross-service workflows are implemented as choreographed sagas: each service reacts to events emitted by other services, performs its local action, and emits a new event. The Workflow service tracks saga state for visibility and operates as the orchestrator for sagas that require explicit coordination.

The canonical saga is the declaration-lifecycle saga: declaration submission triggers verification, verification stages produce per-stage outcomes, the fusion stage produces the lane decision, the lane decision triggers consumer notifications. Each step is an explicit event; failures at any step produce compensation events that roll back the prior steps. Sagas are tested end-to-end with explicit failure-injection scenarios.

Distributed transactions — two-phase commit, XA — are forbidden across services. The Saga pattern with compensating actions is the substitute. The doctrine that no two services share a database (Principle 1) makes the saga approach the only available coordination mechanism, which is the intended consequence.

**Outbox pattern**

Reliable event emission to Kafka uses the outbox pattern. The service writes the event to a local outbox table in the same database transaction as the state change. A background dispatcher service polls the outbox and publishes to Kafka, marking the row as published. The dispatcher honours at-least-once semantics; duplicates are handled by consumer idempotency.

Representative outbox-table schema:

> CREATE TABLE outbox (
>
> id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
>
> event_id UUID NOT NULL UNIQUE,
>
> event_type TEXT NOT NULL,
>
> event_version INTEGER NOT NULL,
>
> aggregate_type TEXT NOT NULL,
>
> aggregate_id UUID NOT NULL,
>
> partition_key TEXT NOT NULL, -- for Kafka partitioning
>
> payload JSONB NOT NULL,
>
> headers JSONB NOT NULL DEFAULT '{}',
>
> created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
>
> dispatched_at TIMESTAMPTZ,
>
> dispatch_attempts INT NOT NULL DEFAULT 0,
>
> last_error TEXT
>
> );
>
> CREATE INDEX outbox_undispatched
>
> ON outbox (created_at)
>
> WHERE dispatched_at IS NULL;
>
> CREATE INDEX outbox_aggregate
>
> ON outbox (aggregate_type, aggregate_id, created_at);

**Circuit breakers and bulkheads**

Every call to an external dependency operates under a circuit breaker. The circuit breaker tracks failure rate, opens when the rate exceeds the configured threshold, and rejects subsequent calls during the open period to allow the dependency to recover. Half-open probes test recovery before resuming full traffic.

Bulkheads partition resources by dependency. The connection pool for the sanctions feed is separate from the connection pool for the PEP feed, which is separate from the connection pool for the Anthropic API. A dependency failure does not exhaust resources used by other dependencies. The implementation is a documented pattern using the standard async runtime’s primitives; the project does not depend on a specific resilience-engineering library across languages.

Configuration thresholds per dependency are documented in the dependency’s integration runbook. Representative thresholds:

|  |  |  |  |
|----|----|----|----|
| **Dependency** | **Failure threshold** | **Open duration** | **Bulkhead size** |
| Anthropic API (Tier A) | 5% error rate over 60s | 30s with half-open probes | 100 concurrent requests |
| AWS Bedrock (Tier B) | 5% error rate over 60s | 30s | 50 concurrent requests |
| LSEG sanctions feed | 20% error rate over 5 min | 5 min | 20 concurrent requests |
| ARMP webhook callback | n/a (we serve, not call) | n/a | 200 concurrent requests in |
| BUNEC identity verification | 10% error rate over 5 min | 2 min | 30 concurrent requests |

**Idempotency**

Idempotency is implemented through explicit idempotency keys carried in request headers. The key is generated by the client (UUID v7 is the recommended scheme; time-sortable identifiers improve cache behaviour) and is honoured by the server through a dedicated idempotency store.

Idempotency store implementation:

- On request receipt, the server checks the idempotency store for the key. If found, the stored response is returned without re-executing the operation.

- If not found, the server executes the operation and stores the response under the key with a TTL of 24 hours.

- Concurrent requests with the same key are serialised through a database-level advisory lock; the first request executes the operation, subsequent requests receive the stored response.

- Idempotency-store entries are cleaned up after their TTL through a Temporal-scheduled job.

**Backpressure and load shedding**

Backpressure is propagated upstream through explicit error signals rather than through implicit queue growth. When a service is overloaded, it returns HTTP 503 with Retry-After headers (or the gRPC RESOURCE_EXHAUSTED equivalent) to its callers. Callers respect backpressure by exponential-backoff retry with jitter.

Load shedding kicks in at documented per-service thresholds. Representative thresholds for the Declaration service: at 80% CPU utilisation across the pod replicas for 60 seconds, low-priority operations (historical-declaration queries) begin returning 503; at 90% utilisation, all queries except priority-1 declaration submissions begin returning 503; at 95% utilisation, the service rejects all incoming traffic and the upstream load balancer routes to the secondary site’s replicas.

**Eventual consistency budget**

Read models lag write models. The lag is bounded by documented per-projection commitments. Engineers and consumers can rely on the documented bounds; behaviour beyond the bound is a defect.

|  |  |  |
|----|----|----|
| **Read model** | **p99 lag bound** | **Implication** |
| Entity-graph projection (Neo4j) | 30 seconds | Investigation Workbench may surface state up to 30 seconds stale; analysts know this through UI affordances |
| Entity full-text search (OpenSearch) | 5 seconds | Fuzzy entity searches reflect declarations up to 5 seconds old |
| Entity summary cache (Redis) | 1 second | KYC lookups reflect within 1 second of declaration acceptance |
| Open-cases queue (PostgreSQL projection) | Strongly consistent | Built from the verification event log within the write transaction |
| BODS export (MinIO) | Up to 1 hour from event to export visibility | Daily diff cycle plus dispatcher latency |

> **NOTE —** The eventual-consistency budgets are explicit commitments to consumers. The platform measures actual lag in production and alerts on any sustained breach. The doctrine of observability covers the consistency budgets: if the lag is not measured, it is not known to be honoured.

**Defence in depth**

Defence in depth means no single security mechanism is the sole protection for any class of attack. The platform’s security layers operate in concert.

|  |  |
|----|----|
| **Layer** | **Protection mechanism** |
| Network | Kubernetes NetworkPolicies restrict ingress and egress per pod; Istio enforces mTLS for every connection; the perimeter firewall blocks unknown destinations from the cluster egress |
| Identity | SPIFFE/SPIRE workload identity per pod; Keycloak as the human-user identity provider with hardware-token MFA; identity is verifiable at every receipt of every call |
| Authorisation | Open Policy Agent evaluates every consequential operation against the project’s Rego policies; policy decisions are themselves logged |
| Application | Per-service authorisation logic that complements OPA, ensuring the service itself enforces its domain’s policies |
| Data | Envelope encryption with HSM-rooted master keys; data classification enforced at the storage layer; column-level encryption for restricted fields |
| Audit | Every consequential operation produces a cryptographically-signed audit event anchored in the ledger; audit logs are themselves immutable |
| Detection | SIEM consuming structured logs and audit events; anomaly detection on access patterns; alerts to the security on-call |

A single compromise (a stolen credential, a vulnerable container image, a misconfigured policy) does not by itself permit successful attack. The attacker must compromise multiple layers in concert, which is a substantially harder profile than compromising any single layer.

> **SUCCESS —** The architectural principles are operationally honoured when: every service maps to a bounded context as documented in V4 P13; CQRS, event sourcing, sagas, outbox, idempotency, and circuit breakers are present where the principles assign them; the eventual-consistency budgets are measured and honoured in production; the defence-in-depth layers are all present and operational. The success criterion is observable in the project’s architecture-compliance audit performed quarterly.

**Repository and Code Organization**

**Monorepo strategy**

RÉCOR is organised as a polyglot monorepo. The single repository contains every service, every application, every shared library, every contract definition, every infrastructure-as-code module, and the canonical documentation including this Architecture Document. The monorepo choice is deliberate: it eliminates the version-skew problems that plague polyrepo architectures, it surfaces cross-service implications of changes at review time, it concentrates the security and supply-chain controls at a single point, and it supports the Claude Code multi-agent orchestration model (sub-agents can navigate the whole codebase, not just one service’s slice).

The monorepo discipline depends on Bazel for hermetic, incremental builds. Without Bazel, the monorepo would force every build to rebuild the world; with Bazel, each build runs the minimal incremental work derived from the dependency graph. The just-over-Bazel pattern (V3 P8) gives engineers the ergonomic command surface while preserving Bazel’s build properties.

**Top-level repository structure**

The monorepo’s top-level structure is documented below. Every engineer learns this structure during onboarding; the structure is stable across the project’s lifetime.

> recor/
>
> ├── README.md \# repository orientation
>
> ├── CLAUDE.md \# top-level agent orientation
>
> ├── ARCHITECTURE.md \# short overview, references this Document
>
> ├── CONTRIBUTING.md \# contribution guide
>
> ├── SECURITY.md \# vulnerability disclosure
>
> ├── LICENSE \# MIT for open-source-distributable parts
>
> ├── .gitignore
>
> ├── .gitattributes
>
> ├── .github/
>
> │ ├── workflows/ \# CI workflows
>
> │ ├── CODEOWNERS \# per-path ownership
>
> │ └── pull_request_template.md
>
> ├── .claude/ \# Claude Code project config (root)
>
> │ ├── settings.json
>
> │ ├── agents/ \# specialist agent definitions
>
> │ ├── skills/ \# project skills (or as submodule)
>
> │ └── hooks/ \# pre/post tool-use scripts
>
> ├── docs/ \# project documentation
>
> │ ├── architecture/ \# this Document’s source files
>
> │ ├── build-spec/ \# Build Specification source
>
> │ ├── concept-note/ \# Concept Note source
>
> │ ├── adr/ \# Architecture Decision Records
>
> │ ├── runbooks/ \# operational runbooks
>
> │ ├── security/ \# threat models, security analyses
>
> │ ├── glossary.md \# ubiquitous language
>
> │ └── onboarding/ \# onboarding guides
>
> ├── services/ \# bounded-context services
>
> │ ├── entity/ \# Entity service (Rust)
>
> │ ├── person/ \# Person service (Rust)
>
> │ ├── declaration/ \# Declaration service (Rust)
>
> │ ├── verification/ \# Verification service (Rust)
>
> │ ├── verification-engine/ \# Verification engine (Rust)
>
> │ ├── evidence/ \# Evidence service (Rust)
>
> │ ├── lane-decision/ \# Lane Decision service (Rust)
>
> │ ├── access/ \# Access service (Rust)
>
> │ ├── audit/ \# Audit Aggregator (Go)
>
> │ ├── workflow/ \# Workflow service (Go, Temporal)
>
> │ ├── schema/ \# Schema registry (Go)
>
> │ ├── notification/ \# Notification service (Go)
>
> │ ├── identity/ \# Identity adapter (Go, fronts Keycloak)
>
> │ ├── api-gateway/ \# API gateway (Rust, Envoy WASM filters)
>
> │ ├── integrations/ \# consumer integrations
>
> │ │ ├── armp/
>
> │ │ ├── anif-goaml/
>
> │ │ ├── dgi/
>
> │ │ ├── beac-banking/
>
> │ │ ├── customs-asycuda/
>
> │ │ ├── sectoral-cadastres/
>
> │ │ ├── conac/
>
> │ │ └── interpol-star/
>
> │ ├── inference-gateway/ \# AI inference gateway (Rust)
>
> │ ├── exports/ \# batch exports (Rust + Go mixed)
>
> │ ├── chaincode/ \# Fabric chaincode (Go)
>
> │ └── frost-coordinator/ \# FROST coordination (Rust)
>
> ├── applications/ \# user-facing applications
>
> │ ├── declarant-portal/ \# TypeScript + React
>
> │ ├── officer-console/
>
> │ ├── investigation-workbench/
>
> │ ├── public-portal/
>
> │ ├── whistleblower-intake/ \# Rust (Tor service)
>
> │ └── admin-console/
>
> ├── libraries/ \# shared libraries
>
> │ ├── rust/ \# Rust shared crates
>
> │ │ ├── recor-types/ \# shared domain types
>
> │ │ ├── recor-crypto/ \# cryptographic primitives
>
> │ │ ├── recor-observability/ \# tracing, metrics, logging helpers
>
> │ │ ├── recor-fabric-client/ \# Fabric Gateway client wrapper
>
> │ │ ├── recor-test-utils/ \# test fixtures and assertions
>
> │ │ └── recor-error/ \# canonical error types
>
> │ ├── go/ \# Go shared modules
>
> │ │ ├── recor-types/
>
> │ │ ├── recor-observability/
>
> │ │ └── recor-test-utils/
>
> │ ├── ts/ \# TypeScript shared packages
>
> │ │ ├── recor-ui-kit/ \# design-token-bound component library
>
> │ │ ├── recor-i18n/ \# translations
>
> │ │ ├── recor-api-client/ \# generated API client
>
> │ │ └── recor-test-utils/
>
> │ └── protos/ \# protobuf source of truth
>
> ├── contracts/ \# external contract definitions
>
> │ ├── grpc/ \# internal gRPC contracts (protos)
>
> │ ├── rest/ \# external REST contracts (OpenAPI 3.1)
>
> │ ├── graphql/ \# GraphQL schemas
>
> │ ├── events/ \# Avro event schemas
>
> │ └── bods/ \# BODS export schema
>
> ├── infrastructure/ \# infrastructure-as-code
>
> │ ├── terraform/ \# cloud and IaC primitives
>
> │ ├── kubernetes/ \# base manifests
>
> │ ├── helm/ \# Helm charts
>
> │ ├── argocd/ \# Argo CD application definitions
>
> │ ├── ansible/ \# ansible for HSM and bare-metal provisioning
>
> │ └── networks/ \# network policies and configurations
>
> ├── policies/ \# OPA Rego policies
>
> ├── dashboards/ \# Grafana dashboards (JSON)
>
> ├── alerts/ \# Prometheus alerts (YAML)
>
> ├── tools/ \# development tooling
>
> │ ├── cli/ \# internal CLIs
>
> │ ├── codegen/ \# code generation helpers
>
> │ └── ci/ \# CI helper scripts
>
> ├── tests/ \# cross-cutting tests
>
> │ ├── e2e/ \# end-to-end tests
>
> │ ├── contract/ \# cross-service contract tests
>
> │ ├── chaos/ \# chaos engineering scenarios
>
> │ └── performance/ \# load and regression tests
>
> ├── WORKSPACE \# Bazel workspace root
>
> ├── BUILD.bazel \# top-level Bazel targets
>
> ├── MODULE.bazel \# Bzlmod configuration
>
> ├── justfile \# top-level command interface
>
> ├── mise.toml \# toolchain pinning
>
> ├── .pre-commit-config.yaml \# pre-commit hooks
>
> └── renovate.json \# dependency update configuration

**CODEOWNERS**

Per-path ownership is enforced through GitHub’s CODEOWNERS mechanism. Every path has at least one owner team; sensitive paths have multiple owner teams whose approval is jointly required. The CODEOWNERS file is itself a high-value artefact and is reviewed quarterly.

> \# CODEOWNERS — owners enforce review approval on path-matching changes
>
> \# Top-level governance
>
> /docs/architecture/ @recor/architect-team @recor/security-team
>
> /docs/adr/ @recor/architect-team
>
> /.claude/ @recor/architect-team @recor/security-team
>
> \# Cryptographic substrate
>
> /services/frost-coordinator/ @recor/crypto-team @recor/security-team
>
> /services/chaincode/ @recor/crypto-team @recor/integration-team
>
> /libraries/rust/recor-crypto/ @recor/crypto-team @recor/security-team
>
> \# Verification engine
>
> /services/verification-engine/ @recor/verification-team @recor/architect-team
>
> /services/inference-gateway/ @recor/verification-team @recor/security-team
>
> \# Consumer integrations (per-integration teams)
>
> /services/integrations/armp/ @recor/integration-team @recor/armp-liaison
>
> /services/integrations/anif-goaml/ @recor/integration-team @recor/anif-liaison
>
> /services/integrations/dgi/ @recor/integration-team @recor/dgi-liaison
>
> \# ... etc per integration
>
> \# Applications
>
> /applications/declarant-portal/ @recor/frontend-team @recor/declarant-experience
>
> /applications/investigation-workbench/ @recor/frontend-team @recor/verification-team
>
> \# Infrastructure
>
> /infrastructure/terraform/ @recor/sre-team
>
> /infrastructure/kubernetes/ @recor/sre-team
>
> /policies/ @recor/security-team
>
> \# Schemas and contracts
>
> /contracts/grpc/ @recor/architect-team
>
> /contracts/rest/ @recor/architect-team @recor/integration-team
>
> /contracts/events/ @recor/architect-team

**Service template**

Every service in services/ conforms to a uniform internal structure adapted to its language. The recor-rust-service and recor-go-service skills produce the scaffolding. The skills materialise the patterns documented in V3 P8 (Rust and Go organisation).

A new service is created through the skill, not by hand-copying an existing service. Hand-copying introduces drift; the skill produces canonical output.

**Application template**

Every application in applications/ conforms to a uniform structure (V3 P8 TypeScript organisation). The recor-react-app skill produces the scaffolding. The shared design tokens, the i18n setup, the API client integration, and the service-worker baseline are present from the first commit.

**Shared library policy**

Code that is genuinely shared across services or applications lives in libraries/. The policy on what belongs in a shared library is conservative: a library exists when at least three services use it and when the abstraction is stable enough that changes to it are rare. Premature library extraction — “I might use this elsewhere” — is forbidden under Doctrine 3 (search before building) and Doctrine 8 (no dangling threads).

When code is promoted from a service to a shared library, the promotion is documented as an ADR. The library is then versioned within the workspace; consuming services depend on the library through workspace references. Cross-workspace versioning (where libraries have separate version numbers) is reserved for a small set of libraries with external consumers (recor-types when exposed as a public SDK).

**Code generation pipeline**

Several artefacts are generated from authoritative source files. The generation runs in CI and produces deterministic output; generated files are checked into the repository for review visibility and for the ability to inspect generated code without running the generation locally.

|  |  |  |
|----|----|----|
| **Source** | **Generator** | **Outputs** |
| contracts/grpc/\*.proto | buf generate (project buf.gen.yaml) | Rust (tonic-build), Go (protoc-gen-go-grpc), TypeScript (@bufbuild/protobuf), Python (betterproto) bindings |
| contracts/rest/\*.openapi.yaml | openapi-typescript, oapi-codegen | TypeScript API client types, Go server stubs |
| contracts/graphql/\*.graphql | graphql-codegen, async-graphql-derive | TypeScript types, Rust schema bindings |
| contracts/events/\*.avsc | avro-rs codegen, avro-tools (Java) for cross-language schemas | Rust serde-compatible structs, Go structs, TypeScript types |
| PostgreSQL migrations | sqlc, sqlx-cli | Go type-safe query code, Rust compile-time-checked queries |
| Design tokens | Custom Style Dictionary configuration | Tailwind theme, CSS variables, design-token reference documentation |

**Branching, tagging, and versioning**

The repository uses a trunk-based branching model. Features are developed on short-lived feature branches and merged to main through pull requests. Main is always deployable to staging without further work. Long-lived feature branches are forbidden; if a feature is genuinely too large for a single short-lived branch, it is decomposed and the pieces are integrated behind feature flags.

Releases are tagged on main with semantic-versioned tags (vMAJOR.MINOR.PATCH). The CI produces signed release artefacts at every tag. Hotfix releases are made from the prior release tag’s commit, not from main, and are then merged forward to main.

**Submodules and vendored content**

Git submodules are used sparingly. The .claude/skills/ directory is the principal submodule, pointing to the central engineering skills repository. Vendored content (forked dependencies, internalised libraries from external sources) lives in vendor/ with documented provenance and the project’s patches applied on top. Vendored content carries a documented review cadence for upstream changes.

> **NOTE —** The monorepo grows over time. Within the project’s lifetime the repository is expected to reach approximately 1.5 million lines of code across the polyglot stack. Bazel’s incremental build properties keep build and test times tractable at that scale; the just command surface keeps the engineer experience tractable. Engineers do not need to be Bazel experts; the abstraction holds for routine work and the lead architect plus dedicated tooling engineers maintain it.

**Layer 0 — Cryptographic Substrate Implementation**

> *Layer 0 is the load-bearing trust substrate. Every cryptographic operation in the platform routes through this layer; correctness here is not a quality goal but a survival condition for the platform.*

**HSM client architecture**

The platform interacts with Thales Luna Network HSMs through the libcryptoki C library wrapped by a Rust crate. The Rust wrapper exposes a strongly-typed safe API that prevents misuse at compile time. The wrapper is the only path through which platform code may invoke HSM operations; direct PKCS#11 calls are forbidden.

**HSM client crate structure**

> libraries/rust/recor-hsm/
>
> ├── Cargo.toml
>
> ├── src/
>
> │ ├── lib.rs \# public API
>
> │ ├── session.rs \# session lifecycle
>
> │ ├── partition.rs \# per-organisation partition handling
>
> │ ├── key.rs \# key handle types (opaque, non-Clone)
>
> │ ├── sign.rs \# signing operations
>
> │ ├── wrap.rs \# envelope-encryption key wrapping
>
> │ ├── attestation.rs \# HSM attestation operations
>
> │ ├── error.rs \# error taxonomy
>
> │ └── ffi/ \# raw libcryptoki bindings (sealed)
>
> ├── build.rs \# link against libcryptoki
>
> └── tests/
>
> ├── mock_hsm/ \# software-emulated HSM for tests
>
> └── integration/

**Public API design principles**

- KeyHandle types are opaque newtypes. A handle is not the key material; it is a reference the HSM honours. Handle types do not implement Clone or Debug-with-content; cloning a handle produces a duplicate reference, not a duplicate key.

- Operations that require quorum approval return Pending types that the caller cannot consume without presenting the quorum-signed authorisation. The type system enforces the policy.

- Errors are exhaustive: every PKCS#11 error code is mapped to a typed error variant; unknown error codes are explicit failures, not silent passthroughs.

- All HSM calls are async; the underlying calls are made on a dedicated blocking thread pool to avoid blocking the runtime.

- Sessions are pooled per partition; the pool is sized to the partition’s configured concurrent-operation limit.

**Representative API**

> // libraries/rust/recor-hsm/src/lib.rs
>
> pub struct HsmClient { /\* opaque internals \*/ }
>
> impl HsmClient {
>
> pub async fn connect(
>
> config: HsmConfig,
>
> partition: PartitionId,
>
> ) -\> Result\<Self, HsmError\> { ... }
>
> pub async fn sign(
>
> &self,
>
> key: &SigningKeyHandle,
>
> message: &\[u8\],
>
> algorithm: SignatureAlgorithm,
>
> ) -\> Result\<Signature, HsmError\> { ... }
>
> pub async fn wrap_data_key(
>
> &self,
>
> wrapping_key: &WrappingKeyHandle,
>
> data_key: &PlaintextDataKey,
>
> ) -\> Result\<WrappedDataKey, HsmError\> { ... }
>
> pub async fn unwrap_data_key(
>
> &self,
>
> wrapping_key: &WrappingKeyHandle,
>
> wrapped: &WrappedDataKey,
>
> quorum: Option\<QuorumAuthorization\>,
>
> ) -\> Result\<PlaintextDataKey, HsmError\> { ... }
>
> }
>
> // Opaque newtypes — cannot be constructed outside this crate
>
> pub struct SigningKeyHandle(KeyHandleId);
>
> pub struct WrappingKeyHandle(KeyHandleId);
>
> // Operations requiring quorum return Pending types
>
> pub enum WrappingKeyOperation {
>
> Unrestricted, // signing daily-export wrap keys
>
> QuorumRequired(QuorumPolicy), // encrypted-tier wrap keys
>
> }

**Fabric chaincode templates**

Hyperledger Fabric chaincode is written in Go (Fabric’s canonical chaincode language) and follows a uniform template. The template encodes the project’s patterns for argument validation, authorisation, structured logging, and event emission.

**Chaincode structure**

Each chaincode is in services/chaincode/\<name\>/ with the following structure:

> services/chaincode/declaration-anchor/
>
> ├── go.mod
>
> ├── main.go \# chaincode entry point
>
> ├── internal/
>
> │ ├── contract/ \# contract definition
>
> │ │ ├── contract.go \# SmartContract type
>
> │ │ ├── submit_declaration.go \# one file per transaction function
>
> │ │ ├── get_anchor.go
>
> │ │ └── list_anchors.go
>
> │ ├── auth/ \# authorisation helpers
>
> │ ├── events/ \# event emission
>
> │ └── storage/ \# state keys and queries
>
> ├── collections.json \# private-data collection definitions
>
> ├── policy.yaml \# endorsement policy
>
> └── tests/

**Representative transaction function**

> // services/chaincode/declaration-anchor/internal/contract/submit_declaration.go
>
> func (c \*SmartContract) SubmitDeclaration(
>
> ctx contractapi.TransactionContextInterface,
>
> declarationJSON string,
>
> ) error {
>
> // 1. Authorisation: must be the Declaration service’s SPIFFE identity
>
> if err := auth.RequireService(ctx, "declaration-service"); err != nil {
>
> return fmt.Errorf("authorisation: %w", err)
>
> }
>
> // 2. Parse and validate
>
> var decl Declaration
>
> if err := json.Unmarshal(\[\]byte(declarationJSON), &decl); err != nil {
>
> return fmt.Errorf("invalid declaration JSON: %w", err)
>
> }
>
> if err := decl.Validate(); err != nil {
>
> return fmt.Errorf("declaration validation: %w", err)
>
> }
>
> // 3. Idempotency: check whether this declaration_id is already anchored
>
> key := storage.DeclarationAnchorKey(decl.DeclarationID)
>
> existing, err := ctx.GetStub().GetState(key)
>
> if err != nil {
>
> return fmt.Errorf("state read: %w", err)
>
> }
>
> if existing != nil {
>
> // Idempotent: same content is acceptable; different is a conflict
>
> if !bytes.Equal(existing, \[\]byte(declarationJSON)) {
>
> return fmt.Errorf("conflict: declaration_id already anchored with different content")
>
> }
>
> // Same content; emit no event and return success
>
> return nil
>
> }
>
> // 4. Anchor the declaration
>
> if err := ctx.GetStub().PutState(key, \[\]byte(declarationJSON)); err != nil {
>
> return fmt.Errorf("state write: %w", err)
>
> }
>
> // 5. Emit the declaration-anchored event
>
> eventPayload := events.DeclarationAnchored{
>
> DeclarationID: decl.DeclarationID,
>
> EntityID: decl.EntityID,
>
> TxID: ctx.GetStub().GetTxID(),
>
> Timestamp: time.Now().UTC().Format(time.RFC3339),
>
> }
>
> payloadJSON, \_ := json.Marshal(eventPayload)
>
> if err := ctx.GetStub().SetEvent("declaration-anchored", payloadJSON); err != nil {
>
> return fmt.Errorf("event emission: %w", err)
>
> }
>
> return nil
>
> }

**Endorsement policies**

Endorsement policies for the declaration-anchor chaincode require endorsements from at least two of the consortium’s organisations: MINFI plus one other. Policies for the audit channel are stricter, requiring endorsements from at least three organisations including at least one non-state seat. Policy YAML is version-controlled with the chaincode.

**FROST coordinator service**

The FROST coordinator orchestrates threshold-signed operations. The service is written in Rust using the ZF FROST reference library. The coordinator is one of the platform’s most security-critical services and operates under enhanced review and audit discipline.

**Coordinator responsibilities**

- Receive signing requests from authorised services (the platform’s identity services for principal authentication, the chaincode for ledger operations, the Access service for encrypted-tier access grants).

- Verify the policy applies: the requested operation matches a policy that permits threshold signing, the requestor has the policy-required role, the operation context is valid (not replayed).

- Initiate the FROST signing protocol with the consortium’s key-holders. The protocol involves two rounds: commitment exchange and signature share submission.

- Collect the threshold quorum (7 of 10 with at least one non-state) of signature shares.

- Aggregate the shares into the final signature and return it to the requestor.

- Anchor the operation (request, quorum, signature) in the audit channel of the Fabric ledger.

**Coordinator state machine**

> // services/frost-coordinator/src/state_machine.rs
>
> \#\[derive(Debug, Clone)\]
>
> pub enum SigningState {
>
> /// Request received; policy evaluation in progress
>
> PolicyEvaluation { request_id: RequestId, request: SigningRequest },
>
> /// Policy approved; sending commitment requests to key-holders
>
> CollectingCommitments {
>
> request_id: RequestId,
>
> request: SigningRequest,
>
> commitments: HashMap\<KeyHolderId, Commitment\>,
>
> deadline: Instant,
>
> },
>
> /// Quorum of commitments received; computing the binding nonce
>
> BindingNonce {
>
> request_id: RequestId,
>
> request: SigningRequest,
>
> commitments: HashMap\<KeyHolderId, Commitment\>,
>
> },
>
> /// Sent share requests to committed key-holders; awaiting shares
>
> CollectingShares {
>
> request_id: RequestId,
>
> request: SigningRequest,
>
> commitments: HashMap\<KeyHolderId, Commitment\>,
>
> shares: HashMap\<KeyHolderId, SignatureShare\>,
>
> deadline: Instant,
>
> },
>
> /// Aggregating shares into final signature
>
> Aggregating {
>
> request_id: RequestId,
>
> request: SigningRequest,
>
> signature: Signature,
>
> },
>
> /// Signature anchored in ledger; returning to caller
>
> AnchoringAudit {
>
> request_id: RequestId,
>
> signature: Signature,
>
> anchor_tx: TxId,
>
> },
>
> /// Completed successfully
>
> Completed { request_id: RequestId, signature: Signature },
>
> /// Failed; reason recorded
>
> Failed { request_id: RequestId, reason: FrostError },
>
> }

**Key-holder client**

Each consortium organisation operates a key-holder client. The client is a separate small service that holds the organisation’s FROST share within the local HSM partition and participates in the protocol when the coordinator initiates. The key-holder client is deployed per organisation; the consortium operates the coordinator centrally.

**Halo2 circuits**

Halo2 circuits implement the platform’s zero-knowledge proof use cases. Three classes of circuit are documented: ownership-percentage proof (proving a beneficial-ownership share exceeds a threshold without revealing the exact share), entity-existence proof (proving an entity exists in the registry without revealing the entity’s identity), and chain-traversal proof (proving an ownership chain reaches a specific natural person without revealing the intermediate links).

**Circuit organisation**

> libraries/rust/recor-zk/
>
> ├── Cargo.toml
>
> ├── src/
>
> │ ├── lib.rs
>
> │ ├── circuits/
>
> │ │ ├── ownership_percentage.rs
>
> │ │ ├── entity_existence.rs
>
> │ │ └── chain_traversal.rs
>
> │ ├── proving_key.rs \# KZG setup management
>
> │ ├── verifier.rs \# verifier API
>
> │ └── prover.rs \# prover API
>
> ├── params/ \# KZG ceremony output
>
> └── tests/

**Circuit example — ownership-percentage proof**

The circuit proves: there exists an ownership chain from entity E to natural person P with cumulative share ≥ threshold T, without revealing the chain or the exact share. The proof is used in the Public Portal to answer “does person P beneficially own ≥ 25% of entity E?” without exposing the underlying chain to public scrutiny.

Implementation strategy: the circuit consumes a Merkle inclusion proof of the chain edges within the platform’s ownership-graph commitment, and proves that the product of the edge shares is ≥ T. The Merkle root is published periodically by the platform with a threshold signature; the verifier checks the proof against the published root.

**OpenTimestamps integration**

OpenTimestamps anchors the platform’s audit channel to Bitcoin via the OpenTimestamps protocol. The integration runs a self-hosted calendar that aggregates the platform’s commitments and submits them in batches to the OpenTimestamps public calendar network and to the Bitcoin blockchain.

**OpenTimestamps client design**

- The platform produces a Merkle tree of audit events per hour. The root hash is the input to OpenTimestamps.

- The Merkle root is submitted to the self-hosted calendar and to two independent public calendars (the canonical OpenTimestamps calendar and one operated by a regional partner).

- Bitcoin anchoring occurs hourly via the standard OTS protocol. The anchor proof is stored in the audit channel.

- Verification of any audit event consumes the event, its Merkle inclusion proof, the Merkle root, and the OTS anchor proof. The verification can be performed by any party that can access the Bitcoin blockchain.

**Integration with the rest of the platform**

Layer 0 exposes its capabilities to the rest of the platform through specific service contracts. The contracts are documented in /contracts/grpc/cryptographic-substrate.proto. Higher-layer services consume Layer 0 capabilities only through these contracts; direct dependencies on the HSM SDK, the Fabric Gateway, or the FROST library are confined to Layer 0 itself.

The boundary is non-negotiable. A breach of the boundary — a higher-layer service calling HSM directly, or implementing its own FROST coordinator — is detected by the architect-reviewer agent during code review and corrected before merge.

> **SUCCESS —** Layer 0 is operationally complete when: the HSM client is in production with documented quarterly attestation; Fabric is in production with all ten organisations’ peers committing to all channels; the FROST coordinator has performed at least 1000 threshold-signed operations without protocol failure; OpenTimestamps anchoring is producing proofs verifiable from external Bitcoin nodes; and the Halo2 circuits are deployed with verified parameter generation. The success criterion is observable in the platform’s cryptographic operations telemetry.

**Layer 1 — Storage Substrate Implementation**

**PostgreSQL — canonical relational store**

**Database topology**

PostgreSQL operates in a high-availability primary-replica topology per site, with logical replication between sites for disaster recovery. Each bounded context that requires a relational store has its own logical database within the cluster; services do not share databases per Principle 1. Cross-database references are forbidden; references travel through events and APIs.

Per-site cluster: one primary plus two streaming replicas in synchronous mode for the audit-critical contexts (Declaration, Verification, Audit) and asynchronous mode for the rest. Connection pooling through PgBouncer in transaction mode. Backup via pg_basebackup nightly plus continuous WAL archiving to MinIO.

**Schema and migration governance**

Database migrations are version-controlled in migrations/ under each service. Migrations are forward-and-reverse: every migration has an explicit rollback path. Migrations are tested in CI against a fresh database and against the prior schema version with representative data. Production migrations are reviewed and applied through the documented deployment pipeline (V6 P26).

**Representative DDL — Declaration service**

> -- migrations/20260601_001_initial_declaration_schema.up.sql
>
> CREATE EXTENSION IF NOT EXISTS pgcrypto;
>
> CREATE EXTENSION IF NOT EXISTS pgaudit;
>
> CREATE EXTENSION IF NOT EXISTS pg_partman;
>
> CREATE EXTENSION IF NOT EXISTS pg_trgm;
>
> -- Declaration event log (event-sourced; append-only)
>
> CREATE TABLE declaration_events (
>
> event_id UUID PRIMARY KEY,
>
> event_type TEXT NOT NULL,
>
> event_version INTEGER NOT NULL,
>
> aggregate_id UUID NOT NULL, -- declaration_id
>
> aggregate_version INTEGER NOT NULL,
>
> occurred_at TIMESTAMPTZ NOT NULL,
>
> correlation_id UUID NOT NULL,
>
> causation_id UUID,
>
> payload JSONB NOT NULL,
>
> payload_schema_uri TEXT NOT NULL,
>
> emitter_principal TEXT NOT NULL,
>
> emitter_signature BYTEA NOT NULL,
>
> ledger_anchor_tx TEXT, -- set after Fabric commit
>
> ledger_anchor_block BIGINT,
>
> created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
>
> );
>
> CREATE UNIQUE INDEX declaration_events_aggregate_version
>
> ON declaration_events (aggregate_id, aggregate_version);
>
> CREATE INDEX declaration_events_aggregate_time
>
> ON declaration_events (aggregate_id, occurred_at);
>
> CREATE INDEX declaration_events_correlation
>
> ON declaration_events (correlation_id);
>
> -- Partition by month
>
> SELECT partman.create_parent(
>
> p_parent_table =\> 'public.declaration_events',
>
> p_control =\> 'occurred_at',
>
> p_type =\> 'native',
>
> p_interval =\> 'monthly',
>
> p_premake =\> 12
>
> );
>
> -- Current-declaration projection (read model)
>
> CREATE TABLE declaration_current (
>
> declaration_id UUID PRIMARY KEY,
>
> entity_id UUID NOT NULL,
>
> declarant_principal TEXT NOT NULL,
>
> declaration_kind TEXT NOT NULL,
>
> status TEXT NOT NULL, -- 'submitted','accepted','withdrawn','superseded'
>
> submitted_at TIMESTAMPTZ NOT NULL,
>
> accepted_at TIMESTAMPTZ,
>
> superseded_by UUID,
>
> ubo_persons UUID\[\] NOT NULL,
>
> ubo_chain_depth_max INTEGER NOT NULL,
>
> ubo_chain_count INTEGER NOT NULL,
>
> current_event_id UUID NOT NULL REFERENCES declaration_events(event_id),
>
> current_event_version INTEGER NOT NULL,
>
> projection_built_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
>
> );
>
> CREATE INDEX declaration_current_entity
>
> ON declaration_current (entity_id, status);
>
> CREATE INDEX declaration_current_submitted
>
> ON declaration_current (submitted_at DESC);
>
> -- Outbox for reliable event publication to Kafka
>
> CREATE TABLE outbox (
>
> id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
>
> event_id UUID NOT NULL UNIQUE,
>
> event_type TEXT NOT NULL,
>
> event_version INTEGER NOT NULL,
>
> aggregate_type TEXT NOT NULL,
>
> aggregate_id UUID NOT NULL,
>
> partition_key TEXT NOT NULL,
>
> payload JSONB NOT NULL,
>
> headers JSONB NOT NULL DEFAULT '{}',
>
> created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
>
> dispatched_at TIMESTAMPTZ,
>
> dispatch_attempts INT NOT NULL DEFAULT 0,
>
> last_error TEXT
>
> );
>
> CREATE INDEX outbox_undispatched
>
> ON outbox (created_at)
>
> WHERE dispatched_at IS NULL;
>
> -- Idempotency store for declaration submission
>
> CREATE TABLE idempotency_keys (
>
> key TEXT PRIMARY KEY,
>
> operation TEXT NOT NULL,
>
> request_hash BYTEA NOT NULL,
>
> response_payload JSONB,
>
> response_status INTEGER,
>
> created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
>
> expires_at TIMESTAMPTZ NOT NULL DEFAULT NOW() + INTERVAL '24 hours'
>
> );
>
> CREATE INDEX idempotency_keys_expiry
>
> ON idempotency_keys (expires_at);
>
> -- Row-level security for restricted-tier columns
>
> ALTER TABLE declaration_current ENABLE ROW LEVEL SECURITY;
>
> CREATE POLICY declaration_read_authorised ON declaration_current
>
> FOR SELECT
>
> USING (current_setting('app.principal_authorised', true) = 'true');

**Encrypted columns**

Columns containing restricted-tier data (declarant personal contact information, sensitive evidence-package contents) are encrypted at the application layer with envelope encryption rooted in the HSM. The encryption is transparent to PostgreSQL; the database sees ciphertext bytes.

**Neo4j — ownership graph**

**Graph schema**

The ownership graph is the platform’s primary representation of who owns whom. Nodes are entities and natural persons; edges are ownership relationships with attributes for share percentage, capacity (direct, indirect, beneficial), and provenance.

> // Neo4j Cypher schema declarations
>
> // Run via the schema-management service at deployment
>
> // Indexes
>
> CREATE INDEX entity_id IF NOT EXISTS
>
> FOR (e:Entity) ON (e.entity_id);
>
> CREATE INDEX person_id IF NOT EXISTS
>
> FOR (p:Person) ON (p.person_id);
>
> CREATE INDEX entity_name_text IF NOT EXISTS
>
> FOR (e:Entity) ON (e.canonical_name);
>
> // Constraints
>
> CREATE CONSTRAINT entity_id_unique IF NOT EXISTS
>
> FOR (e:Entity) REQUIRE e.entity_id IS UNIQUE;
>
> CREATE CONSTRAINT person_id_unique IF NOT EXISTS
>
> FOR (p:Person) REQUIRE p.person_id IS UNIQUE;
>
> CREATE CONSTRAINT entity_required_props IF NOT EXISTS
>
> FOR (e:Entity) REQUIRE (e.entity_id, e.canonical_name, e.legal_form) IS NOT NULL;
>
> // Edges:
>
> // (:Entity)-\[:OWNS {percent, capacity, since, declaration_id}\]-\>(:Entity)
>
> // (:Person)-\[:OWNS {percent, capacity, since, declaration_id}\]-\>(:Entity)
>
> // (:Person)-\[:CONTROLS {via, since, declaration_id}\]-\>(:Entity)
>
> // (:Person)-\[:RELATED {kind, since}\]-\>(:Person)
>
> // (:Entity)-\[:SUPERSEDES\]-\>(:Entity)
>
> // (:Person)-\[:ALIAS_OF\]-\>(:Person)

**Graph operations**

Common query patterns are documented in libraries/rust/recor-neo4j-queries/ as named query templates. Engineers do not write ad-hoc Cypher in service code; they invoke named queries. The query catalogue is versioned and reviewed.

Representative named queries: get-ultimate-bo (returns the natural persons at the end of the chain with cumulative shares); detect-circular-ownership (returns chains where the same entity appears twice); shared-owner-pairs (returns entity pairs that share at least one beneficial owner with cumulative share above threshold); community-detection (returns clusters of mutually-owning entities via the Louvain algorithm). The detect-circular-ownership and shared-owner-pairs queries underpin the verification engine’s pattern detection signatures.

**OpenSearch — full-text and fuzzy matching**

OpenSearch indexes entity and person records for full-text search and for the fuzzy matching that the verification engine’s entity-resolution stage requires. The index mappings are tuned for Cameroonian name patterns including transliteration variants (Mbarga / Mbargha / M’Barga / Mbargha) and for the French / English / Pidgin English text content that appears in entity descriptions.

> // services/entity/opensearch-mappings/entity-index.json
>
> {
>
> "settings": {
>
> "number_of_shards": 4,
>
> "number_of_replicas": 2,
>
> "refresh_interval": "5s",
>
> "analysis": {
>
> "analyzer": {
>
> "cameroonian_name": {
>
> "type": "custom",
>
> "tokenizer": "standard",
>
> "filter": \[
>
> "lowercase",
>
> "asciifolding",
>
> "cameroonian_transliteration",
>
> "cameroonian_phonetic"
>
> \]
>
> },
>
> "french_english_mixed": {
>
> "type": "custom",
>
> "tokenizer": "standard",
>
> "filter": \[
>
> "lowercase",
>
> "asciifolding",
>
> "snowball_french",
>
> "snowball_english"
>
> \]
>
> }
>
> },
>
> "filter": {
>
> "cameroonian_transliteration": {
>
> "type": "synonym",
>
> "synonyms_path": "synonyms/cameroonian_name_transliteration.txt"
>
> },
>
> "cameroonian_phonetic": {
>
> "type": "phonetic",
>
> "encoder": "double_metaphone",
>
> "replace": false
>
> }
>
> }
>
> }
>
> },
>
> "mappings": {
>
> "properties": {
>
> "entity_id": { "type": "keyword" },
>
> "canonical_name": {
>
> "type": "text",
>
> "analyzer": "cameroonian_name",
>
> "fields": {
>
> "exact": { "type": "keyword" },
>
> "mixed": { "type": "text", "analyzer": "french_english_mixed" }
>
> }
>
> },
>
> "aliases": {
>
> "type": "text",
>
> "analyzer": "cameroonian_name"
>
> },
>
> "legal_form": { "type": "keyword" },
>
> "registration_number": { "type": "keyword" },
>
> "tax_id": { "type": "keyword" },
>
> "registered_address": {
>
> "type": "text",
>
> "analyzer": "french_english_mixed"
>
> },
>
> "active": { "type": "boolean" },
>
> "registered_at": { "type": "date" },
>
> "last_event_at": { "type": "date" }
>
> }
>
> }
>
> }

**Apache Kafka — event streaming**

**Topic conventions**

Topic names follow the convention {bounded-context}.{event-class}.{version}, e.g., declaration.submitted.v3, verification.outcome.v2, audit.consequential.v1. Partitions are sized by expected event volume; partition keys are documented per topic (typically the aggregate identifier). Retention is documented per topic; the audit topic uses infinite retention via the audit log compaction strategy.

|  |  |  |  |
|----|----|----|----|
| **Topic** | **Partitions** | **Retention** | **Partition key** |
| declaration.submitted.v3 | 32 | Infinite (compacted by aggregate_id) | entity_id |
| declaration.amended.v3 | 32 | Infinite (compacted) | entity_id |
| verification.stage_completed.v2 | 32 | 90 days | declaration_id |
| verification.outcome.v2 | 32 | Infinite | declaration_id |
| lane.decided.v2 | 32 | Infinite | declaration_id |
| audit.consequential.v1 | 16 | Infinite | principal_id |
| notification.dispatched.v1 | 8 | 30 days | delivery_id |
| evidence.added.v1 | 16 | Infinite (compacted) | evidence_id |

All topics use Avro schemas registered in the project’s self-hosted Schema Registry. Producer and consumer compatibility is enforced through schema-registry compatibility levels (BACKWARD for most topics; FORWARD_TRANSITIVE for the audit topic to permit producer evolution faster than consumer evolution).

**MinIO — object storage**

MinIO stores declaration documents, evidence artefacts, exported files, and database backups. Buckets are organised by classification level and by content type. Server-side encryption uses HSM-rooted data encryption keys; bucket policies enforce access at the IAM level; MinIO’s audit log feeds the platform’s audit aggregator.

|  |  |
|----|----|
| **Bucket** | **Contents and access policy** |
| recor-declarations-restricted | Declaration-attached documents; encrypted at rest; read access by Declaration service and Verification service only |
| recor-evidence-restricted | Evidence-package binary content; encrypted; read access by Evidence service and authorised analysts only |
| recor-exports-public | BODS exports; cleartext (public-tier data); read access through public CDN |
| recor-exports-restricted | Consumer-integration exports (DGI, ANIF); encrypted; per-consumer read access |
| recor-backups | Database WAL archives and basebackups; encrypted; read access only by SRE-team principals; immutable retention policy (write-once for 90 days) |
| recor-audit-archives | Long-term audit log archives; encrypted; immutable retention policy (write-once for 7 years) |
| recor-models | AI model checkpoints and adapter weights for the on-premises Tier C deployment; encrypted; read access only by inference gateway |

**Redis — cache and ephemeral state**

Redis provides caching and ephemeral state for the platform. Caches are not the source of truth; cache invalidation is event-driven from the originating service’s outbox. The Redis Sentinel topology provides failover; the data is not durably committed at Redis level.

Namespace conventions: each service has its own logical Redis database within the cluster. Keys follow the pattern {service}:{purpose}:{identifier}, e.g., entity:summary:550e8400-e29b-41d4-a716-446655440000, ratelimit:declarant-portal:client_ip_hash. TTLs are documented per cache; never-expiring keys are explicitly justified.

**Cross-store consistency**

State that exists in multiple stores (Postgres for the canonical event log, Neo4j for the graph projection, OpenSearch for the search index, Redis for the cache) is kept consistent through the outbox pattern. The canonical write commits to Postgres in a single transaction with the outbox entry; the projection services consume the outbox-published events from Kafka and update their respective stores. Inconsistencies are detected by a reconciliation job that runs nightly and emits alerts on any drift beyond the documented bounds.

> **SUCCESS —** Layer 1 is operationally healthy when: every store passes its weekly reconciliation against the canonical event log; backup-restore drills succeed within the documented RPO/RTO; cross-store eventual-consistency bounds (V4 P9) are honoured in production telemetry; encrypted-tier data never appears in cleartext in non-HSM-attested storage. The success criterion is observable in the storage-layer telemetry and is audited quarterly.

**Layer 2 — Domain Services**

> *Twelve bounded-context services implement the platform’s domain logic. Each service owns its data, exposes contracts at its boundary, and operates under the doctrines and principles documented earlier. The service-by-service summaries below give the engineering team the operational reference for each service’s shape.*

**Service summary table**

|  |  |  |
|----|----|----|
| **Service** | **Language** | **Purpose** |
| entity | Rust | Canonical legal entity records; manages entity attributes, aliases, registration data |
| person | Rust | Canonical natural-person records; manages PII with sovereign-tier protections |
| declaration | Rust | Beneficial-ownership declaration intake, validation, lifecycle, amendment |
| verification | Rust | Verification case orchestration; tracks case state through nine-stage pipeline |
| verification-engine | Rust | Nine-stage verification pipeline implementation; pattern detection; Dempster–Shafer fusion |
| evidence | Rust | Evidence package management; provenance tracking; analyst review interface |
| lane-decision | Rust | Final lane decisions (green/yellow/red); appeal handling; consumer notification trigger |
| access | Rust | Access requests, grants, policy evaluation; OPA integration; threshold-signed quorum for encrypted-tier |
| audit | Go | Audit-log aggregation; cryptographic anchoring coordination; immutable archival |
| workflow | Go | Temporal-based saga orchestration; scheduled jobs; cross-service coordination |
| schema | Go | Schema registry; cross-store schema evolution governance |
| notification | Go | Notification dispatch to consumers, declarants, analysts; retry policy; delivery confirmation |

**Per-service detail**

**entity service**

- Persistence: PostgreSQL (canonical), Neo4j (graph projection), OpenSearch (search projection), Redis (summary cache).

- Contracts: gRPC for internal callers; REST for the API gateway; events: entity.created, entity.updated, entity.merged, entity.dissolved.

- SLOs: p99 \< 50ms for entity-summary lookup by ID; p99 \< 200ms for fuzzy entity search.

- Dependencies: CFCE source-of-record reconciler (read-only consumer), OHADA registry adapter.

- Concerns: entity merging when discovered duplicates; alias management; the legal-form vocabulary aligned with OHADA Uniform Act forms.

**person service**

- Persistence: PostgreSQL with encrypted columns for PII; restricted-tier classification.

- Contracts: gRPC for authorised internal callers; not exposed at API gateway directly. Access goes through other services.

- SLOs: p99 \< 30ms for identifier lookup.

- Dependencies: BUNEC, NIU, immigration databases (read-only, through dedicated identity-authentication adapters).

- Concerns: name canonicalisation (Cameroonian patterns); date-of-birth-only-when-needed minimisation; alias-of relationships.

**declaration service**

- Persistence: PostgreSQL event-sourced with current-declaration projection.

- Contracts: REST for the Declarant Portal; gRPC for internal callers; events: declaration.submitted, declaration.accepted, declaration.amended, declaration.withdrawn.

- SLOs: p99 \< 500ms for declaration submission acceptance (returns receipt, verification runs async).

- Dependencies: entity service, person service, evidence service for document attachment, FROST coordinator for declarant-attested signatures.

- Concerns: amendment lifecycle; correction vs amendment vs withdrawal semantics; legal-form-specific validation.

**verification service**

- Persistence: PostgreSQL event-sourced (verification case state).

- Contracts: gRPC; events: verification.case_opened, verification.stage_completed, verification.case_closed.

- SLOs: end-to-end verification (all nine stages) target p99 \< 5 minutes for green-lane cases.

- Dependencies: verification-engine service (does the work), evidence service (stores evidence), inference-gateway (AI reasoning).

- Concerns: case-state durability across stage transitions; resumption after operational disruption; analyst routing for yellow-lane outcomes.

**verification-engine service**

- Persistence: PostgreSQL for stage-outcome events; Redis for in-flight stage state.

- Contracts: gRPC.

- SLOs: documented per stage in V4 P14.

- Dependencies: every external feed (sanctions, PEP, adverse media), inference-gateway, entity service, person service, evidence service.

- Concerns: the most architecturally complex service; documented in detail in V4 P14.

**evidence service**

- Persistence: PostgreSQL for evidence metadata, MinIO for binary content, all encrypted.

- Contracts: gRPC; REST for analyst review interface; events: evidence.added, evidence.reviewed.

- SLOs: p99 \< 1s for evidence package retrieval.

- Dependencies: object storage, cryptographic substrate for provenance.

- Concerns: chain of custody; analyst-facing presentation of evidence packages.

**lane-decision service**

- Persistence: PostgreSQL event-sourced.

- Contracts: gRPC; events: lane.decided, lane.appealed, lane.appeal_resolved.

- SLOs: p99 \< 100ms for lane-decision query.

- Dependencies: verification service, notification service, audit service.

- Concerns: appeal lifecycle; superseding decisions on appeal; the immutable record of all decisions and appeals.

**access service**

- Persistence: PostgreSQL for access grants, events for access exercise; Redis for in-memory policy cache.

- Contracts: gRPC; integrates with OPA for policy evaluation.

- SLOs: p99 \< 10ms for authorisation decision.

- Dependencies: identity service, FROST coordinator for encrypted-tier access.

- Concerns: justification capture; need-to-know enforcement; threshold-signed quorum integration.

**audit service**

- Persistence: Kafka (audit topic, infinite retention), PostgreSQL (aggregator state), MinIO (archives), Fabric audit channel (anchoring).

- Contracts: gRPC; consumes events from every service.

- SLOs: p99 \< 30s for audit-event anchoring.

- Dependencies: cryptographic substrate, OpenTimestamps client.

- Concerns: capture every consequential event; cryptographic signature verification; archival integrity over decades.

**workflow service**

- Persistence: Temporal cluster with PostgreSQL persistence.

- Contracts: gRPC for workflow triggering and query.

- SLOs: workflow scheduling latency p99 \< 1s.

- Dependencies: every service that participates in sagas.

- Concerns: workflow versioning across schema changes; long-running workflow recovery.

**schema service**

- Persistence: PostgreSQL for schema definitions; backed by the Confluent Schema Registry for Avro schemas.

- Contracts: gRPC for schema retrieval and validation.

- SLOs: p99 \< 50ms for schema retrieval.

- Dependencies: independent of other services.

- Concerns: schema-evolution governance; compatibility-level enforcement.

**notification service**

- Persistence: PostgreSQL for dispatch state; Kafka for incoming events; outbox pattern for outgoing.

- Contracts: gRPC; events: notification.dispatched, notification.delivered, notification.failed.

- SLOs: p99 \< 5s for notification dispatch initiation; per-channel SLOs for delivery confirmation.

- Dependencies: every consumer-integration adapter; email and SMS gateways.

- Concerns: per-consumer delivery semantics; retry policy; dead-letter handling.

**Service skeleton (canonical)**

Every service’s composition root wires together the same set of cross-cutting concerns in the same order. The skeleton is materialised by the recor-rust-service and recor-go-service skills. The canonical wiring sequence is documented below; deviations require named approval.

> // services/\<service\>/crates/\<service\>-server/src/main.rs (Rust skeleton)
>
> \#\[tokio::main\]
>
> async fn main() -\> Result\<(), Box\<dyn std::error::Error\>\> {
>
> // 1. Load configuration (env, files, consortium config service)
>
> let config = Config::load()?;
>
> // 2. Initialise observability (tracing, metrics, logs)
>
> let \_otel = recor_observability::init(&config.observability)?;
>
> // 3. Connect to persistence and cache
>
> let pg = PgPool::connect(&config.postgres.url).await?;
>
> let redis = redis::Client::open(config.redis.url.as_str())?;
>
> // 4. Initialise the schema registry client
>
> let schema_registry = SchemaRegistryClient::new(&config.schema_registry).await?;
>
> // 5. Initialise the FROST coordinator client (for services that sign)
>
> let frost = FrostClient::connect(&config.frost).await?;
>
> // 6. Initialise the audit-event emitter (writes to outbox)
>
> let audit = AuditEmitter::new(&pg, &schema_registry);
>
> // 7. Build the domain services with their dependencies
>
> let entity_service = entity::Service::new(pg.clone(), redis.clone(), audit.clone());
>
> // 8. Build the gRPC server with interceptors
>
> let grpc = GrpcServer::builder()
>
> .interceptor(AuthInterceptor::new(&config.auth))
>
> .interceptor(TracingInterceptor::new())
>
> .interceptor(RateLimitInterceptor::new(redis.clone()))
>
> .service(EntityServiceServer::new(entity_service.clone()))
>
> .build();
>
> // 9. Build the HTTP companion (health, metrics, admin)
>
> let http = HttpServer::builder()
>
> .health(entity_service.clone())
>
> .metrics()
>
> .admin(&config.admin)
>
> .build();
>
> // 10. Spawn background workers (outbox dispatcher, projection rebuilders)
>
> let outbox_dispatcher = OutboxDispatcher::spawn(pg.clone(), &config.kafka);
>
> let projection_rebuilder = ProjectionRebuilder::spawn(/\* ... \*/);
>
> // 11. Wait for shutdown signal
>
> let shutdown = recor_observability::shutdown_signal();
>
> tokio::select! {
>
> \_ = grpc.serve(config.grpc.address) =\> {}
>
> \_ = http.serve(config.http.address) =\> {}
>
> \_ = shutdown =\> {
>
> tracing::info!("shutdown signal received");
>
> }
>
> }
>
> // 12. Graceful shutdown
>
> outbox_dispatcher.shutdown().await;
>
> projection_rebuilder.shutdown().await;
>
> Ok(())
>
> }

The Go service skeleton follows the equivalent sequence with idiomatic Go patterns. The two skeletons are kept in alignment so that engineers crossing between Rust and Go services find the same composition shape.

> **SUCCESS —** A Layer 2 service is complete when: it conforms to its language’s service template; it implements the documented contracts; its persistence is in place with migrations; its observability surfaces are live; its SLOs are measured in production; its CLAUDE.md is current; its tests cover the layer at the documented pyramid ratio; its runbooks exist for each defined alert.

**Layer 3 — Verification Engine Implementation**

> *The verification engine is the platform’s load-bearing analytical capability. Its correctness is the platform’s credibility. This Part documents the engine’s nine-stage pipeline, the pattern detection signatures, the Dempster–Shafer fusion that integrates evidence, and the architectural patterns that make the engine inspectable, testable, and continuously improvable.*

**Pipeline architecture**

The verification engine is implemented as a pipeline orchestrator in Rust that drives a declaration through nine sequential stages. Each stage is a separate Rust crate implementing a trait the orchestrator defines. New stages can be added; existing stages can be improved; the architecture supports incremental evolution without rewriting the engine.

> // services/verification-engine/src/pipeline.rs
>
> \#\[async_trait\]
>
> pub trait Stage: Send + Sync {
>
> fn name(&self) -\> &'static str;
>
> fn version(&self) -\> u32;
>
> fn classification(&self) -\> Classification;
>
> async fn run(&self,
>
> declaration: &Declaration,
>
> context: &mut PipelineContext)
>
> -\> Result\<StageOutcome, StageError\>;
>
> }
>
> pub struct Pipeline {
>
> stages: Vec\<Arc\<dyn Stage\>\>,
>
> fusion: Arc\<DempsterShaferFusion\>,
>
> audit: Arc\<AuditEmitter\>,
>
> inference: Arc\<InferenceGatewayClient\>,
>
> }
>
> impl Pipeline {
>
> pub async fn process(&self, declaration: Declaration)
>
> -\> Result\<VerificationOutcome, PipelineError\>
>
> {
>
> let mut context = PipelineContext::new(&declaration);
>
> let mut stage_outcomes = Vec::with_capacity(self.stages.len());
>
> for stage in &self.stages {
>
> let start = Instant::now();
>
> let outcome = stage.run(&declaration, &mut context).await?;
>
> let elapsed = start.elapsed();
>
> self.audit.emit_stage_completed(
>
> &declaration, stage.as_ref(), &outcome, elapsed,
>
> ).await?;
>
> stage_outcomes.push(outcome);
>
> }
>
> let fused = self.fusion.fuse(&stage_outcomes)?;
>
> let lane = self.lane_from_fused(&fused);
>
> Ok(VerificationOutcome {
>
> declaration_id: declaration.id,
>
> stage_outcomes,
>
> fused,
>
> lane,
>
> })
>
> }
>
> }

**The nine stages**

|  |  |  |
|----|----|----|
| **\#** | **Stage** | **What it does** |
| 1 | Schema validation | Structural validation against the declaration schema; field-level invariants; legal-form-specific required fields |
| 2 | Identity authentication | Authenticates declared natural persons and entities against BUNEC, NIU, immigration; produces identity-confidence evidence |
| 3 | Sanctions screening | Daily-refreshed UN, EU, US OFAC, UK HMT sanctions list cross-reference; international and national PEP lists |
| 4 | Adverse-media screening | Multilingual adverse-media search (French, English) plus the ICIJ leaked-data archives (Panama, Paradise, Pandora, Suisse Secrets) |
| 5 | Entity resolution | Fuzzy matching against the existing entity universe to detect duplicates and aliases |
| 6 | Pattern detection | Eight pattern-detection signatures (described below) operating in parallel |
| 7 | AI-reasoning enrichment | Claude Opus 4.7 (Tier B) reasoning over the declaration plus the prior stages’ outcomes; identifies concerns the rules-based stages may miss |
| 8 | Cross-source triangulation | ARMP procurement records, DGI taxpayer records, real-estate records, sectoral cadastres; identifies inconsistencies |
| 9 | Dempster–Shafer fusion + lane decision | Combines the per-stage evidence; produces the final lane decision with calibrated confidence |

**Pattern detection signatures**

The pattern detection stage runs eight signatures in parallel against the declaration plus the ownership graph. Each signature is a separate component with documented logic, documented basic-probability assignment for the fusion stage, and documented test fixtures.

- **Signature 1 — Circular ownership.** Detects ownership cycles in the chain. Declarant entity owns A, A owns B, B owns the declarant entity. Cycles are anomalous and either indicate concealment or genuine complex structures requiring scrutiny.

- **Signature 2 — Excessive chain depth.** Detects chains where the natural person at the end is reached only through more than six intermediate legal entities. Excessive depth is itself a concealment indicator.

- **Signature 3 — Offshore-jurisdiction concentration.** Detects chains routing through jurisdictions with high prevalence in concealment patterns. The jurisdiction list is curated and reviewed quarterly.

- **Signature 4 — Front-person indicators.** The dominant adversarial pattern: a declared beneficial owner whose financial profile is inconsistent with their declared ownership share. Implementation cross-references age, declared income, declared assets, residential pattern against the value of the declared ownership.

- **Signature 5 — Shared-owner patterns.** Detects entities sharing beneficial owners with anomalous frequency. Two entities sharing the same beneficial owner is normal; ten entities under common beneficial ownership where the owner has limited business history is anomalous.

- **Signature 6 — Timing patterns.** Detects declarations timed to evade scrutiny: declarations submitted immediately before tender deadlines, before regulatory cutoffs, or in the night-hours of high-stakes business cycles.

- **Signature 7 — Supervised classifier.** A gradient-boosted classifier trained on the platform’s accumulated case outcomes. Produces a probability that the current declaration matches the historical concealment pattern. Trained quarterly with the platform’s growing dataset; deployed only after quarterly inference audit approval.

- **Signature 8 — Community detection.** Graph-clustering analysis identifying communities of entities with anomalous mutual ownership patterns. Operates on the Neo4j graph projection via the GDS Louvain implementation.

**Dempster–Shafer fusion**

The fusion stage combines per-stage evidence into a unified belief assignment. Each stage produces basic probability assignments over the frame of discernment {accept, reject, uncertain}. The fusion combines the assignments using Dempster’s rule, producing a final belief over the same frame.

The choice of Dempster–Shafer over a naive Bayesian approach is documented in ADR-014. The principal reasons: Bayesian fusion requires independence assumptions that the stages do not satisfy (sanctions and PEP screening are correlated); DS handles ignorance explicitly (a stage may decline to produce evidence rather than producing a misleading prior); DS produces both belief and plausibility, which permits the lane decision to incorporate uncertainty calibration.

**Fusion implementation**

> // libraries/rust/recor-ds-fusion/src/lib.rs
>
> pub struct BeliefAssignment {
>
> accept: f64,
>
> reject: f64,
>
> uncertain: f64,
>
> other: f64, // “none of the above” — captures stage refusal
>
> }
>
> impl BeliefAssignment {
>
> pub fn validate(&self) -\> Result\<(), FusionError\> {
>
> let sum = self.accept + self.reject + self.uncertain + self.other;
>
> if (sum - 1.0).abs() \> 1e-9 {
>
> return Err(FusionError::ProbabilitiesDontSum);
>
> }
>
> for v in \[self.accept, self.reject, self.uncertain, self.other\] {
>
> if !(0.0..=1.0).contains(&v) {
>
> return Err(FusionError::ProbabilityOutOfRange);
>
> }
>
> }
>
> Ok(())
>
> }
>
> }
>
> pub struct DempsterShaferFusion {
>
> config: FusionConfig,
>
> }
>
> impl DempsterShaferFusion {
>
> pub fn fuse(&self, stage_outcomes: &\[StageOutcome\])
>
> -\> Result\<FusedOutcome, FusionError\>
>
> {
>
> let assignments: Vec\<\_\> = stage_outcomes.iter()
>
> .filter_map(\|s\| s.belief.as_ref())
>
> .collect();
>
> if assignments.is_empty() {
>
> return Err(FusionError::NoEvidence);
>
> }
>
> let mut combined = assignments\[0\].clone();
>
> for a in &assignments\[1..\] {
>
> combined = self.combine(&combined, a)?;
>
> }
>
> Ok(FusedOutcome {
>
> belief: combined,
>
> // belief + plausibility per Dempster-Shafer
>
> plausibility_accept: combined.accept + combined.uncertain,
>
> plausibility_reject: combined.reject + combined.uncertain,
>
> })
>
> }
>
> fn combine(&self, a: &BeliefAssignment, b: &BeliefAssignment)
>
> -\> Result\<BeliefAssignment, FusionError\>
>
> {
>
> // Dempster's rule of combination with normalisation
>
> // K = sum of products over disjoint sets
>
> let k = a.accept \* b.reject + a.reject \* b.accept;
>
> if (1.0 - k).abs() \< 1e-9 {
>
> return Err(FusionError::TotalConflict);
>
> }
>
> let factor = 1.0 / (1.0 - k);
>
> Ok(BeliefAssignment {
>
> accept: factor \* (a.accept \* b.accept
>
> \+ a.accept \* b.uncertain
>
> \+ a.uncertain \* b.accept),
>
> reject: factor \* (a.reject \* b.reject
>
> \+ a.reject \* b.uncertain
>
> \+ a.uncertain \* b.reject),
>
> uncertain: factor \* (a.uncertain \* b.uncertain),
>
> other: 0.0,
>
> })
>
> }
>
> }

**AI inference integration**

Stage 7 (AI-reasoning enrichment) uses the inference gateway documented in V5 P18. The gateway selects the tier based on the data classification: pseudonymised declaration content goes to Tier B (Bedrock Cape Town); when the declaration cannot be pseudonymised (rare but possible for entities with publicly-known UBOs), Tier C (sovereign on-premises) is used.

The AI prompts are versioned in libraries/rust/recor-prompts/. Prompt evolution is governed: changes require parallel-testing against a held-out evaluation set, the quarterly inference audit examines the production prompts in use. Engineers do not modify prompts on production paths without the documented review.

**Lane routing**

The lane decision is computed from the fused belief. Thresholds are documented and reviewed quarterly. The thresholds at the document’s baseline:

- Green lane: belief in {accept} ≥ 0.85 AND belief in {reject} ≤ 0.05. The declaration is accepted automatically; consumer notifications proceed.

- Yellow lane: 0.40 ≤ belief in {accept} \< 0.85 OR plausibility in {reject} \> 0.20. Routed to analyst review. Most declarations land here on first pass; analyst review brings additional evidence and produces a final outcome.

- Red lane: belief in {reject} ≥ 0.50 OR specific high-signal patterns matched (sanctions hit, identity-document fraud). The declaration is rejected; the declarant receives a notification with appeal rights; the case may be referred to ANIF, CONAC, or TCS.

Lane thresholds are themselves part of the platform’s governance surface. Adjustments require ADR with named approval. The quarterly inference audit evaluates whether the thresholds are producing the intended distribution of outcomes; recalibration is performed when warranted.

**Testing the engine**

The verification engine carries the platform’s most extensive test suite. Test categories include: unit tests per stage (target 90% coverage); integration tests against the full pipeline using synthetic declarations; property tests asserting invariants (monotonicity of belief under additional evidence, idempotency of pipeline runs on the same declaration); adversarial tests against a corpus of known concealment patterns curated from international cases and pilot findings; performance tests against projected production load.

The adversarial test corpus is itself a governed artefact. Cases added to the corpus must satisfy: the case is documented from a published or platform-internal source; the case represents a real concealment pattern, not a theoretical one; the expected lane outcome for the case is documented; the case’s outcome is reviewed quarterly to detect drift.

**Performance**

End-to-end verification target SLOs:

|  |  |  |
|----|----|----|
| **Outcome** | **p50** | **p99** |
| Schema validation only (rejection at stage 1) | 100ms | 500ms |
| Sanctions hit (early-termination at stage 3) | 2s | 5s |
| Green-lane end-to-end (all 9 stages pass) | 30s | 5 minutes |
| Yellow-lane (all stages plus analyst routing) | 1 minute (engine) plus analyst-review time | 5 minutes (engine) |
| Red-lane with AI-reasoning concurrence | 1 minute | 5 minutes |

> **SUCCESS —** The verification engine has succeeded when: every stage operates at its documented SLO; the Dempster–Shafer fusion produces calibrated outcomes verified through quarterly audit; the eight pattern detection signatures operate at their documented detection rates against the adversarial corpus; the supervised classifier (Signature 7) maintains its accuracy on holdout test sets across quarterly retraining cycles; the inference audit framework consistently approves the engine’s operation for the next quarter.

**Layer 4 — API Surface**

> *Layer 4 is the platform’s public face for every programmatic consumer. The discipline here governs how the platform looks to ARMP, ANIF, DGI, civil society researchers, and every other party that integrates with the platform programmatically.*

**API gateway**

The platform exposes its APIs through a dedicated gateway built on Envoy Proxy with custom WASM filters in Rust. The gateway terminates external TLS, performs the first-line authentication, applies the global rate limits, routes to internal services through mTLS-protected mesh paths, and emits the audit events that capture every external interaction with the platform.

The gateway is itself a service in services/api-gateway/ with the WASM filter source in Rust. The filter compiles to wasm32-wasi and loads into Envoy at start-up. The filter responsibilities: JWT validation against Keycloak’s JWKS; client-certificate validation for institutional consumers; rate-limit token consumption against the Redis token-bucket; audit-event emission to Kafka.

**GraphQL federation**

The primary internal-and-administrative API surface is GraphQL. The platform operates a federated graph composed from per-service subgraphs implemented with async-graphql in Rust. The federation is via Apollo Federation v2 semantics with the gateway implemented as Apollo Router (the Rust-native gateway) configured to consume the per-service subgraphs.

Subgraph composition: the entity service publishes the Entity, EntityOwnership, EntityAttribute subgraph; the person service publishes Person, PersonAlias (restricted-tier, federated only to authorised callers); the declaration service publishes Declaration, DeclarationAmendment, DeclarationHistory; the verification service publishes VerificationCase, EvidencePackage; the lane decision service publishes LaneDecision, Appeal; the access service publishes AccessGrant, AccessRequest.

**Schema design discipline**

- Nullable by default; required fields are explicit. The pattern reflects the reality that declared data is often partially complete.

- Connections (Relay-style pagination) for any list type. Offset pagination is forbidden; cursor pagination is the standard pattern.

- Enums for closed value sets; strings only where the value space is genuinely open. New enum values are added through schema versioning, not by emitting unknown values.

- DataLoader batching across resolvers; n+1 queries detected by the schema-quality CI check and rejected.

- Persisted queries for external consumers; arbitrary GraphQL operations are not permitted from external callers. Persisted query registration is itself an authorised operation that the consumer requests through their onboarding workflow.

**Query depth and complexity limits**

Arbitrary GraphQL depth is a denial-of-service vector. The gateway enforces per-consumer limits on query depth (default 7 levels) and query complexity (computed per-field cost). Limits are documented per consumer tier; higher tiers (institutional consumers) receive higher limits negotiated through their integration contract.

**REST endpoints**

REST endpoints serve the use cases that benefit from REST’s ergonomics: webhook subscriptions, file uploads, BODS exports, health checks for external monitoring. REST is documented in OpenAPI 3.1 in contracts/rest/. The server implementations use axum 0.7.x for the Rust REST endpoints.

REST design discipline:

- Resource-oriented URLs (/entities/{id}, /declarations/{id}/amendments). RPC-style URLs (/createEntity) are forbidden except in genuinely procedural endpoints (e.g., /v1/verification/{id}/recompute).

- Standard HTTP status semantics; never invent custom statuses. 200, 201, 204, 400, 401, 403, 404, 409, 422, 429, 500, 503 cover the platform’s needs.

- Idempotency headers (Idempotency-Key) on every POST and PATCH; honoured per Doctrine 13.

- Versioning via URL prefix (/v1/, /v2/). Header-based versioning is forbidden; URL versioning is reviewable and routable at the gateway.

- Hypertext links in responses (HAL or JSON:API; project standardised on JSON:API).

- Pagination through link relations (next, prev, first, last) with cursor tokens; offset pagination is forbidden.

**Webhook subscriptions**

Consumers subscribe to platform events through the webhook system. Subscriptions are configured per consumer; events matching the subscription are delivered with HMAC-signed payloads. Delivery semantics: at-least-once, retry-with-exponential-backoff on consumer failure, dead-letter queue for permanently-failing deliveries.

**Webhook delivery contract**

> POST \<consumer-webhook-url\> HTTP/1.1
>
> Host: consumer.example.cm
>
> Content-Type: application/json
>
> X-RECOR-Webhook-Id: \<UUID v7\>
>
> X-RECOR-Event-Type: declaration.accepted
>
> X-RECOR-Event-Version: 3
>
> X-RECOR-Delivery-Attempt: 1
>
> X-RECOR-Timestamp: 2026-06-15T09:32:14.123456Z
>
> X-RECOR-Signature-Ed25519: \<hex\>
>
> X-RECOR-Idempotency-Key: \<UUID v7\>
>
> {
>
> "event_id": "01J9Y...",
>
> "event_type": "declaration.accepted",
>
> "event_version": 3,
>
> "event_time": "2026-06-15T09:32:14.123456Z",
>
> "payload": { ... }
>
> }
>
> \# Expected responses:
>
> \# 2xx — acknowledged; no retry
>
> \# 4xx — client error; no retry (logged as delivery failure)
>
> \# 5xx, network errors, timeouts — retry with backoff
>
> \# After 12 failed attempts over 7 days, moved to dead-letter queue

Consumers verify the Ed25519 signature against the platform’s publicly-published webhook signing key. Key rotation is announced 30 days in advance through the consumer notification channel; consumers verify against the current key during the rotation window.

**Rate limiting**

Rate limiting operates at three scopes: per-consumer global, per-endpoint per-consumer, and global protection for the platform. The implementation is a Redis-backed token-bucket evaluated by the API gateway’s WASM filter.

**Rate-limit tiers**

|  |  |  |  |
|----|----|----|----|
| **Tier** | **Requests/sec** | **Burst** | **Notes** |
| Public anonymous | 10 | 30 | Public Portal end-users without authentication |
| Public authenticated | 50 | 200 | Researchers and civil society with API key |
| Institutional consumer (standard) | 500 | 2000 | ARMP, DGI, sectoral cadastres |
| Institutional consumer (high-volume) | 2000 | 10000 | ANIF goAML bidirectional, BEAC banking; negotiated per consumer |
| Internal services (within the mesh) | Unlimited | Unlimited | Internal mesh traffic is shaped by per-service deployment capacity, not gateway limits |

**BODS exporter**

The Beneficial Ownership Data Standard (BODS) exporter produces the public-tier export in BODS v0.4 format consumable by Open Ownership and other beneficial-ownership data consumers globally. The exporter runs on a documented schedule (daily delta export plus monthly full export), produces signed export files, and publishes to the recor-exports-public MinIO bucket with public CDN access.

Implementation: a Rust binary in services/exports/bods-export/ that reads from the entity-graph projection (Neo4j) and the public-declaration projection (PostgreSQL), filters to public-tier-only fields, formats per BODS v0.4 schema, signs the output with a dedicated BODS signing key, and publishes.

**OpenAPI and SDL governance**

The OpenAPI 3.1 specifications and the GraphQL SDLs are part of the platform’s contract surface. Changes follow the platform’s contract-evolution policy: backward-compatible additions are routine; backward-incompatible changes require a new version with documented migration paths for consumers. Contract changes pass through buf-equivalent compatibility checks in CI (Spectral for OpenAPI, GraphQL Inspector for GraphQL).

> **SUCCESS —** Layer 4 is operationally successful when: every consumer integration operates through the gateway under its documented rate-limit tier; the GraphQL federation supports the platform’s administrative and investigative workflows with sub-second p99 for typical queries; webhook delivery rates exceed 99.5% within seven days for all subscribed consumers; BODS exports publish daily on schedule and validate against the BODS v0.4 reference; no contract-incompatible change reaches production without the documented consumer migration.

**Layer 5 — Consumer Integration Implementations**

> *The eight consumer integrations are where the platform produces value. Each integration is a separate adapter implementing a specific consumer’s contract, with its own SLO commitments, its own failure-mode discipline, and its own consumer-side liaison. The fail-closed posture at consequential boundaries (Doctrine 14) is implemented here.*

**Integration architecture pattern**

Each consumer integration is a separate service in services/integrations/\<consumer\>/. The service implements the consumer’s contract on one side and the platform’s internal interfaces on the other. The integration is the single point of consumer-specific knowledge in the platform; other platform services interact with the consumer only through the integration.

Common patterns across integrations: contract test against the consumer’s mock surface (run in CI on every change); SLO definitions documented in V4 P16 and measured continuously; circuit breakers with consumer-specific thresholds; per-consumer credentials handled through Vault; per-consumer audit channels.

**Integration: ARMP (procurement)**

Agence de Régulation des Marchés Publics (ARMP) is the procurement regulator. The integration provides synchronous KYC lookups during the tender adjudication process. ARMP submits a tender-candidate entity identifier; the platform returns within the negotiated SLO the entity’s BO record plus conflict-of-interest flags computed from the bidder pool. Failed lookups produce explicit hold signals; ARMP procurement procedures require the hold to be respected (the fail-closed boundary).

- Implementation: Rust service exposing a synchronous gRPC endpoint to ARMP’s procurement-management system.

- SLO: p99 \< 800ms for KYC lookup; p99 \< 2s for conflict-of-interest analysis across the bidder pool.

- Fail-closed mechanism: when the platform cannot respond within 5s for a KYC lookup or 10s for a conflict-of-interest analysis, ARMP’s system records an explicit hold; the tender step cannot proceed until the platform recovers.

- Audit: every ARMP query and every response is logged with the requesting principal, the tender identifier, and the response payload signature.

**Integration: ANIF goAML (financial intelligence)**

Agence Nationale d’Investigation Financière (ANIF) operates Cameroon’s deployment of the goAML system from UNODC. The integration is bidirectional: outgoing direction pushes BO enrichment to ANIF’s analyst-review queue for STRs that name registered entities; incoming direction receives ANIF’s analyst-confirmed risk indicators that feed back into the verification engine.

- Implementation: Go service implementing the goAML XML schemas for bidirectional STR enrichment.

- SLO: outgoing enrichment p99 \< 30s; incoming consumption p99 \< 30s.

- Operational integration: BO enrichment annotated on at least 90% of STRs naming registered entities by end of pilot.

- Audit: every enrichment delivery and every incoming risk indicator is anchored in the platform’s audit channel; the cryptographic provenance is verifiable by an external auditor.

**Integration: DGI (tax administration)**

Direction Générale des Impôts (DGI) consumes BO data for taxpayer-cross-reference, transfer-pricing audit support, and beneficial-ownership disclosure verification in tax filings. The integration is dual-mode: bulk export (daily) for large-taxpayer-audit workflows and on-demand lookup for case-specific queries.

- Implementation: Go service for bulk export pipeline (Temporal-scheduled daily job producing the DGI export); Rust service for synchronous on-demand lookups.

- Bulk export SLO: complete daily by 06:00 Africa/Douala; documented schema versioned in /contracts/dgi/.

- On-demand lookup SLO: p99 \< 500ms.

- Specific signal: transfer-pricing risk indicator (entities sharing BO that trade with each other) is part of the bulk export per ADR-127.

**Integration: BEAC banking (KYC for account opening)**

Banque des États de l’Afrique Centrale (BEAC) is the regional central bank; commercial banks in Cameroon use the platform’s KYC API during account opening to verify declared beneficial ownership against the registry. The integration is the platform’s highest-traffic synchronous interface.

- Implementation: Rust service with synchronous gRPC and REST endpoints.

- SLO: p50 \< 100ms, p99 \< 500ms for KYC lookup.

- Capacity: designed for 100 requests/sec sustained per consortium bank; 30 banks supported, providing 3000 req/sec headroom.

- Fail-closed boundary: when the platform cannot respond, the bank’s account-opening workflow holds the account creation; institutional liaison ensures bank operational SOPs are aligned with this expectation.

**Integration: customs ASYCUDA**

Cameroonian customs operates ASYCUDA World as its automated declarations system. The integration enriches customs declarations with BO information for the importing/exporting entities, flagging concealment patterns relevant to customs (under-invoicing schemes, shell-importer patterns).

- Implementation: Go service with batch and on-demand interfaces.

- Batch: hourly enrichment of new customs declarations.

- On-demand SLO: p99 \< 1s for individual entity enrichment query.

**Integration: sectoral cadastres**

Three sectoral cadastres consume BO data: mining (MINMIDT cadastre minier), forestry (MINFOF cadastre forestier), and hydrocarbons (SNH/MINMIDT cadastre pétrolier). The integration provides BO enrichment to each cadastre’s licence-management workflow.

- Implementation: separate small Go services per cadastre due to different consumer-system shapes (REST, SOAP, file-based exchange depending on cadastre).

- Common SLO: p99 \< 2s for enrichment query.

**Integration: CONAC (anti-corruption commission)**

Commission Nationale Anti-Corruption (CONAC) consumes asset-declaration cross-references: the platform cross-references CONAC’s asset-declaration filings (for officials required to declare) against the BO register, surfacing entities the official declares as well as undisclosed entities where the official appears as BO.

- Implementation: Rust service operating on a structured query interface.

- SLO: asynchronous workflow; results within 24 hours of CONAC submission for full asset-declaration cross-reference.

**Integration: INTERPOL / StAR (international cooperation)**

INTERPOL and the Stolen Asset Recovery Initiative (StAR) consume BO data to support international asset-recovery operations. The integration is constrained: information sharing operates only under the legal-framework provisions and only through documented request channels. The integration is documented here for completeness; operational details are governed by the cooperation frameworks documented in V1 P3.

- Implementation: a Rust service handling structured information-sharing requests with explicit consortium approval per request.

- No standing SLO; requests are processed on a case-by-case basis under the cooperation framework.

**Cross-integration concerns**

**Credential management**

Each consumer integration has its own credentials — mTLS certificates for the platform’s outgoing connections; signing keys for outgoing webhooks; consumer-side credentials for the platform’s incoming connections from the consumer. All credentials are stored in Vault with consumer-specific paths; rotation is on the documented cadence (annually for certificates, quarterly for signing keys).

**Mock surfaces**

Each consumer has a mock surface in the project’s development environment. The mock surface implements the consumer’s contract sufficient for the platform’s contract tests to pass. The mocks are maintained in alignment with the real consumer’s evolution; consumer-side changes that affect the contract are reflected in the mock as part of the consumer-liaison process.

**Liaison and onboarding**

Each consumer institution has a designated liaison on the consortium’s engineering team. The liaison maintains the relationship with the consumer’s technical lead, coordinates contract evolution, manages the consumer’s onboarding (training, acceptance testing, first-production-load support), and is the named point of contact for production incidents affecting the consumer.

> **SUCCESS —** Layer 5 is operationally successful when: every consumer integration meets its negotiated SLO over rolling thirty-day windows; the fail-closed boundaries operate as documented (consumer-side hold semantics respected); the bidirectional integrations (ANIF goAML in particular) exchange data at the documented coverage rates; consumer liaisons report active operational relationships with the consumer-side teams; consumer-side acceptance tests pass at every quarterly regression cycle.

**Layer 6 — Applications**

> *Six user-facing applications constitute the platform’s human interface. Each application is built on a uniform React 19 + TypeScript stack, with offline-first behaviour where the use case warrants it, and with strict accessibility commitments. The applications are how the platform is experienced; their quality determines whether the platform is used.*

**Frontend doctrine**

Every application conforms to a uniform stack documented in V3 P8 (TypeScript section). The doctrine in this Part adds the application-specific architecture, the offline-first patterns, the design system, and the per-application detail.

- React 19 with the new compiler enabled; functional components only; concurrent features (Suspense, transitions, deferred values) used where the user experience benefits.

- TypeScript strict mode mandatory; the project’s tsconfig as shipped.

- Vite 6.x as the build tool with the project’s shared configuration.

- Tailwind CSS v4 with the project’s design tokens.

- TanStack Query for server state; Zustand for client state; Redux forbidden.

- react-hook-form + Zod for forms with shared schemas validating both input and API responses.

- React Router 7.x in framework mode with route-level data loaders.

- Internationalisation via react-i18next; French primary, English secondary, Pidgin English where the user base requires.

**Design system**

The platform’s design system is materialised in libraries/ts/recor-ui-kit/. Design tokens (colours, typography, spacing, radii, shadows) are defined in a single source and consumed across all applications. Components in the kit cover the platform’s recurring patterns: data tables with cursor pagination; entity-card summaries; principal-presence indicators; lane-decision badges (green/yellow/red); document-upload widgets; signature-attestation widgets.

Components are accessible (WCAG 2.1 AA). Keyboard navigation works for every interactive surface; screen readers announce state changes; colour contrasts meet the AA threshold; focus indicators are visible and styled per the design system.

**PWA architecture**

Each application is a Progressive Web App. The Vite-generated build produces the service worker, the manifest, and the precache of the application shell. Workbox provides the service-worker library; the project’s configuration is consistent across applications with per-application customisations for cache strategies.

Service-worker strategies by content type:

- Application shell (HTML, JS, CSS): precached at install; updated through the service-worker update flow. Stale-while-revalidate during the update window.

- API responses for read operations: network-first with cache fallback for resilience. Cache TTL per endpoint.

- API responses for write operations: not cached; require online. Submissions queue in IndexedDB during offline and replay on connection restore.

- User-uploaded documents in flight: persisted in IndexedDB until the declaration containing them is successfully submitted; cleared after server acknowledgement.

- Static assets (icons, illustrations): cache-first, long-lived.

**Offline-first design for the Declarant Portal**

The Declarant Portal carries the platform’s most demanding offline requirement. Declarants in regions with intermittent connectivity must be able to complete declaration workflows offline and submit when connectivity is available. The portal’s offline capability is designed in from the outset.

**Local data persistence**

IndexedDB via Dexie 4.x provides the local persistence. The schema mirrors the declaration form’s structure with versioned migrations. Schema:

> // applications/declarant-portal/src/db.ts
>
> import Dexie from "dexie";
>
> export interface DraftDeclaration {
>
> id: string; // UUID v7
>
> entity_id: string \| null;
>
> status: "drafting" \| "ready_to_submit" \| "submitting" \| "submitted" \| "failed";
>
> payload: DeclarationPayload;
>
> attachments: AttachmentRef\[\];
>
> created_at: number; // epoch millis
>
> updated_at: number;
>
> submission_attempts: number;
>
> last_error?: string;
>
> idempotency_key: string;
>
> }
>
> export interface Attachment {
>
> id: string;
>
> draft_id: string;
>
> filename: string;
>
> mime_type: string;
>
> size: number;
>
> bytes: Blob;
>
> uploaded: boolean;
>
> upload_token?: string;
>
> }
>
> export class DeclarantDB extends Dexie {
>
> drafts!: Dexie.Table\<DraftDeclaration, string\>;
>
> attachments!: Dexie.Table\<Attachment, string\>;
>
> constructor() {
>
> super("recor-declarant-portal");
>
> this.version(1).stores({
>
> drafts: "id, entity_id, status, updated_at",
>
> attachments: "id, draft_id, uploaded",
>
> });
>
> }
>
> }
>
> export const db = new DeclarantDB();

**Sync protocol**

On connection restore, the service worker triggers a background sync. The sync flow: read drafts in “ready_to_submit” status; upload any unloaded attachments; submit the declaration via the synchronous submit endpoint with the stored idempotency key; on success, transition the draft to “submitted” and clear attachments; on failure, increment submission_attempts, store the error, and surface to the user. The idempotency key ensures that a draft submitted offline and then re-submitted after connection restore produces exactly one declaration.

**Conflict resolution**

Drafts in IndexedDB are owned by the declarant on the device; multi-device editing of the same draft is not supported. A declarant who creates a draft on one device and continues on another starts a new draft on the second device. The simplicity prevents the conflict-resolution complexity that multi-device editing would introduce and reflects the operational reality that declarants typically file from a single workstation.

**Multi-device strategy via Capacitor**

Each PWA is also wrappable as a native iOS and Android application via Capacitor 6. The wrapper produces native installers (IPA, AAB) from the PWA codebase with platform-specific shims for file upload, push notifications, and platform integrations. The wrapped apps are distributed through the Apple App Store and Google Play under the consortium’s developer accounts.

Native wrapping is provided for the Declarant Portal and Public Portal where the mobile-app form factor adds value over the mobile-web experience (offline functionality is more reliable in native apps; push notifications are first-class). The Investigation Workbench, Officer Console, and Admin Console are web-only; their use case (desktop investigative work) doesn’t benefit from native packaging.

**The six applications**

**Declarant Portal**

Used by entities filing their beneficial ownership declarations. The most usability-critical application; declarants are non-technical, often filing under time pressure (tender deadlines, regulatory cutoffs), often working in intermittent connectivity. Design priorities: clear progressive disclosure of the declaration workflow; one screen at a time on mobile; explicit save indicators; offline-first; minimal friction for amendment.

- Screens (high level): landing, entity selection, declaration kind selection, declarant role selection, ownership wizard (multi-step), evidence upload, attestation, submission, receipt.

- Offline support: full draft creation and editing offline; submission requires online.

- Devices: mobile-first design; tested on representative low-end Android devices used in Cameroonian regions.

- Languages: French (primary), English, Pidgin English with simplified flow.

**Officer Console**

Used by analysts at consumer institutions (ARMP, ANIF, DGI, CONAC, TCS, BEAC) for entity lookup and case work. Design priorities: efficient batch lookup; rich entity detail view; integration with the institution’s case-management workflow; role-appropriate display of restricted vs encrypted tier data.

- Screens: search, entity detail, batch lookup, case list, case detail, evidence review.

- Offline support: limited; read access to recently-viewed entities cached for short-term offline reference.

- Devices: desktop-primary; mobile-responsive for limited field use.

**Investigation Workbench**

Used by ANIF, CONAC, and TCS investigators for complex investigations involving graph traversal and AI-assisted query. Most architecturally complex application; the only one with heavy client-side computation (graph visualisation).

- Screens: case list, case detail, graph explorer (cytoscape.js with the platform’s custom layout algorithms), evidence package builder, AI-assisted query interface, export.

- Offline support: none; the investigation context requires online graph access.

- Devices: desktop only; high-resolution displays recommended.

- Special: the AI-assisted query interface uses the inference gateway (Tier B) with a natural-language query interface that translates to Cypher against Neo4j with the investigator’s confirmation before execution.

**Public Portal**

Used by the public, civil society, researchers, journalists. Statutorily-open data presented in a discoverable, queryable, exportable form. The platform’s public face; quality here determines public trust in the platform.

- Screens: landing with explanation, search, entity detail with public-tier data, BODS download, transparency reports.

- Offline support: aggressive caching; viewable entity pages cached for offline reference.

- Devices: every device; mobile-first; tested across the spectrum of devices observed in Cameroonian usage.

- Statically renderable: where possible, pages are pre-rendered to enable CDN edge delivery and minimum-server-load consumption.

**Whistleblower Intake**

Anonymous and protected channel for whistleblowers reporting concerns about declared BO. Operates as a Tor hidden service in addition to clearnet; end-to-end encryption with a dedicated Halo2-based proof of submission integrity. The Whistleblower Intake is the most security-sensitive Layer 6 application.

- Implementation: Rust application with embedded web server; not a SPA. Server-rendered HTML for the highest assurance baseline against client-side compromise.

- Tor: deployed as a Tor onion service; clearnet access also supported with Tor-routing recommended.

- Submission: encrypted on the submitter’s device; decryptable only by the protected-investigator team via threshold-signed quorum approval.

- Operational isolation: deployed in a dedicated namespace; no shared persistence with the main platform; communication only through the audit channel for submission records.

**Administrative Console**

Used by consortium administrators and the lead engineering team for platform governance: schema reviews, threshold parameter adjustments (with quorum), policy changes, system-health overview, audit log queries. The most powerful application; access strictly controlled.

- Screens: governance dashboard, schema versions, policy editor, threshold parameters, audit query, key ceremony management.

- Offline support: none.

- Devices: desktop only.

- Access: requires hardware-token MFA and named role; consequential operations require threshold-signed quorum.

**Accessibility audit**

Every application passes a WCAG 2.1 AA accessibility audit before its operational launch. Audits are conducted by an independent accessibility firm with documented findings remediated before the launch. Quarterly regression audits verify continued compliance as the applications evolve.

**Performance budgets**

Per-application performance budgets:

|  |  |  |  |  |
|----|----|----|----|----|
| **Application** | **FCP target** | **LCP target** | **TTI target** | **Notes** |
| Declarant Portal | 1.5s | 2.5s | 3.5s | Tested on representative low-end Android device on 3G |
| Officer Console | 1s | 1.5s | 2s | Desktop wired connection assumed |
| Investigation Workbench | 1s | 2s | 3s | Graph rendering is excluded; on-demand |
| Public Portal | 1s | 1.5s | 2s | Edge-cached; budgets verified across regions |
| Whistleblower Intake | 1s | 2s | 2s | Server-rendered; lighter client; Tor latency excluded |
| Admin Console | 1s | 1.5s | 2s | Internal users; wired connection assumed |

Performance budgets are measured continuously through real-user monitoring; regressions beyond the budget block the next release until investigated and resolved.

> **SUCCESS —** Layer 6 is operationally successful when: every application passes its accessibility audit and maintains compliance across releases; the Declarant Portal’s offline functionality is verified by user-testing in representative connectivity conditions; the Investigation Workbench supports the analyst workflows documented in the consumer-integration acceptance tests; the Public Portal is consumed by civil society and journalists at the levels documented in the project’s transparency reports; the Whistleblower Intake operates without operational compromise events.

**AI Inference Engineering**

> *Every model call routes through the inference gateway under strict policy: data classification determines tier, tier determines provider, every call is logged, every prompt is versioned, every model decision is auditable. The gateway is the operational instantiation of Doctrine 22.*

**Three-tier routing**

The inference gateway is a Rust service that all platform code calls instead of speaking to model providers directly. It tags each request by data classification, dispatches to the appropriate tier, captures the inference audit record, applies the fallback cascade on errors, and returns the structured response.

|  |  |  |  |
|----|----|----|----|
| **Tier** | **Data classification accepted** | **Routes to** | **Use cases** |
| A | Public; pseudonymised public-tier | Anthropic API (Opus 4.7 primary; Sonnet 4.6 fallback) | Public-document reasoning; BODS export-quality checks; user-facing explanations |
| B | Pseudonymised Restricted (PII removed) | AWS Bedrock PrivateLink af-south-1 (Opus 4.7 primary; Sonnet 4.6 fallback) | Verification engine stage 7; entity-resolution reasoning; case-note enrichment; analyst-assist queries from Investigation Workbench |
| C | Raw PII; Encrypted-tier reasoning | Sovereign on-premises GPU cluster (Llama 3.3 70B Instruct primary; Mistral Large 2 secondary) | Identity-authentication reasoning that requires the cleartext identifier; sensitive entity disambiguation; encrypted-tier analyst queries |

**Routing enforcement**

Routing is enforced at the gateway, not by convention. Every request carries a data-classification tag in its header; the gateway rejects requests where the calling service’s declared tag is inconsistent with the payload’s actual content (a content scanner runs on every request; mismatched tags are SEV-3 incidents).

Calling services cannot bypass the gateway. Egress network policies block direct connections from any platform pod to Anthropic API hostnames or to Bedrock endpoints; the only egress allowed is from the inference gateway pod’s namespace. The mesh policy is the structural enforcement of the doctrine.

**Prompt management**

Every prompt is version-controlled in libraries/rust/recor-prompts/. A prompt is a structured artefact: the system instruction, the user-message template, the expected output schema, the model parameters (temperature, max_tokens, stop sequences), the evaluation set against which the prompt is regression-tested. Prompts have semantic versions; production deployments reference specific prompt versions.

> // libraries/rust/recor-prompts/src/verification_engine/stage_7_reasoning.rs
>
> pub const STAGE_7_REASONING_V12: PromptDefinition = PromptDefinition {
>
> id: "verification_engine.stage_7.reasoning",
>
> version: 12,
>
> classification: Classification::PseudonymisedRestricted,
>
> model_target: ModelTarget::Tier_B,
>
> system_instruction: include_str!("v12/system.md"),
>
> user_template: include_str!("v12/user.md"),
>
> parameters: ModelParameters {
>
> temperature: 0.2,
>
> max_tokens: 8000,
>
> stop_sequences: &\["\</analysis\>"\],
>
> extended_thinking: ExtendedThinking::Xhigh,
>
> },
>
> output_schema: include_str!("v12/output_schema.json"),
>
> evaluation_set_path: "evals/stage_7_v12.jsonl",
>
> approval: PromptApproval {
>
> ratified_by: &\["lead-architect", "verification-engineering-lead", "security-lead"\],
>
> ratified_at: "2026-04-15",
>
> next_review: "2026-07-15",
>
> },
>
> };

**Token accounting and budgets**

Every inference call consumes tokens. The gateway accounts for token consumption per calling service, per data classification, per model, per prompt version, per outcome. Token spend is reported to the engineering finance dashboard and reviewed monthly. Budgets per service are documented in the platform’s cost model; sustained budget overruns trigger investigation.

Token-spend metrics: per-service input tokens, per-service output tokens, per-service cached input tokens (with prompt caching active for stable system prompts; cached input is 90% cheaper), per-service extended-thinking tokens. The Anthropic prompt caching feature is enabled aggressively for stable system prompts; the project’s system-instruction stability is one of the design properties that makes the caching economic.

**Fallback cascade**

On model errors, the gateway runs a documented fallback cascade. The cascade is per-tier:

- Tier A primary failure: retry once with exponential backoff (network-level retry); if persistent, fall back to Sonnet 4.6 on Anthropic API; if Sonnet also fails, fail the call with explicit error. The verification engine handles inference failure as evidence absence (the stage marks itself unable-to-evaluate; the fusion stage incorporates the absence).

- Tier B primary failure: retry once; fall back to Sonnet 4.6 on Bedrock; if Bedrock is entirely unavailable, the call fails. The platform does not fall back from Tier B to Tier A because that would cross the data-residency boundary.

- Tier C primary failure: retry once; fall back to Mistral Large 2 on the same in-country GPU cluster; if both are unavailable, fail the call.

The cascade is implemented in the gateway as a state machine with documented transitions. The cascade’s behaviour is itself audited; every fallback invocation is logged with the reason for the primary’s failure.

**Inference audit logging**

Every inference call produces an audit record: the calling service, the prompt id and version, the tier and model used, the input token count, the output token count, the response classification (success, fallback, failure), the timestamp, the correlation identifier for the broader request that triggered the inference.

Audit records do not include the input or output text by default — the inputs and outputs may contain restricted-tier data and persisting them broadly violates the data classification doctrine. A sampled subset (configurable, default 1%) of records preserves the input and output text for the quarterly inference audit, encrypted at rest with HSM-rooted keys, accessible only by the inference audit team through documented procedure.

**Quarterly inference audit**

Every quarter the platform’s inference operation is audited. The audit examines: the production prompts in use against the prompts approved at the prior quarter’s audit; the model selection rate per tier against the projected mix; the fallback rate per tier against the expected rate; the sampled inputs and outputs for anomalies or drift; the token spend against budget; the quality of inference outputs against the evaluation sets.

The audit produces a report shared with the Technical Advisory Function. The report’s recommendations drive prompt updates, evaluation-set expansions, threshold adjustments to the verification engine, and (rarely) model substitution decisions. The audit framework itself is the meta-discipline that prevents inference quality from degrading silently.

**Caching strategy**

Two layers of caching:

- Prompt caching at the model provider. Stable system prompts are cached at Anthropic and at Bedrock; the platform’s system prompts are designed to be stable (the user-message bears the variable content). Token costs for cached input are 90% lower; latencies are also lower. Caching is configured per prompt; the gateway adds the cache_control header per Anthropic’s API spec.

- Response caching for deterministic prompts. Where the prompt is deterministic in its output (low-temperature reasoning on stable input), the response is cached in Redis with a TTL appropriate to the input’s staleness profile. The verification engine’s sanctions-screening doesn’t cache (data refreshes daily); entity-resolution reasoning caches with short TTLs (entity changes are tracked through events; cache invalidation on event consumption).

**Performance**

Inference latency budgets:

|  |  |  |  |
|----|----|----|----|
| **Use case** | **p50** | **p99** | **Notes** |
| Stage 7 reasoning (verification) | 8s | 20s | Extended thinking enabled; long output for evidence narrative |
| Analyst-assist query (Investigation Workbench) | 5s | 15s | Streamed output to user; perceived latency lower |
| Entity-resolution reasoning | 2s | 5s | Short prompts; small output |
| Public-portal explanation | 3s | 8s | Cached aggressively; cache-hit latency near zero |

> **SUCCESS —** AI inference is operationally sound when: every call routes through the gateway under classification discipline; routing-policy violations are at zero in production; the fallback cascade exercises monthly under controlled chaos; the quarterly inference audit approves the production prompts; token spend remains within budget; latencies meet the documented SLOs; the verification engine’s stage 7 outcomes correlate with analyst-confirmed outcomes at the documented calibration threshold.

**Identity and Access Engineering**

> *Identity is the platform’s primary control surface. Every operation answers the question: who is doing this, and are they permitted? The discipline in this Part is how that question is answered authoritatively, consistently, and auditably.*

**Identity providers**

Two identity providers operate in concert. Keycloak is the platform’s sovereign IdP for human users — declarants, officers, analysts, investigators, administrators. SPIFFE/SPIRE is the workload identity system for service-to-service authentication. The two systems are integrated through the identity service in Layer 2.

**Keycloak deployment**

- Self-hosted Keycloak 25.x in the consortium’s sovereign infrastructure; HA topology with three nodes per site; PostgreSQL-backed.

- Per-consortium-organisation realms; users belong to their organisation’s realm; cross-realm trust through brokering for the consumer-institution federations.

- Authentication flows: declarants use email plus password plus TOTP; officers and analysts use OIDC federation from their institutional IdPs plus YubiKey FIDO2 second factor; administrators use YubiKey-only with hardware-attested authentication.

- Session management: short-lived access tokens (5 minutes), longer-lived refresh tokens (8 hours with idle timeout 30 minutes), session invalidation on policy events.

**SPIFFE/SPIRE deployment**

- One SPIFFE trust domain per consortium organisation. Trust domains: spiffe://recor.cm/minfi, spiffe://recor.cm/anif, etc.

- Federated trust between domains through SPIRE federation. Each domain’s root authority is the consortium’s root certificate authority for that organisation.

- Workload identities follow the pattern spiffe://\<domain\>/ns/\<namespace\>/sa/\<service-account\>/svc/\<service-name\>.

- Attestation: kubernetes node-attestor plus kubernetes workload-attestor; HSM-attested workloads use hsm node-attestor for the highest-assurance services (FROST coordinator, inference gateway).

- SVIDs are short-lived (1 hour) with automatic rotation; expired SVIDs are rejected by Istio at the mesh boundary.

**Authorisation through OPA**

Open Policy Agent evaluates every consequential authorisation decision. Policies are written in Rego in the /policies/ directory, version-controlled, reviewed quarterly, and distributed to OPA sidecars via the bundle service. The platform’s services call out to a co-located OPA sidecar for every authorisation decision; the sidecar evaluates locally with sub-millisecond latency.

Representative Rego policy for declaration access:

> \# policies/declaration_access.rego
>
> package recor.declaration.access
>
> import future.keywords.if
>
> import future.keywords.in
>
> default allow := false
>
> \# Anyone with a valid principal can read public-tier declarations of their entity
>
> allow if {
>
> input.action == "read"
>
> input.resource.classification == "public"
>
> input.principal.entity_id == input.resource.entity_id
>
> }
>
> \# Analysts at named consumer institutions can read restricted-tier
>
> allow if {
>
> input.action == "read"
>
> input.resource.classification == "restricted"
>
> input.principal.role in {
>
> "armp.analyst", "anif.analyst", "dgi.analyst",
>
> "conac.analyst", "tcs.analyst", "beac.kyc-officer"
>
> }
>
> \# Must include structured justification
>
> input.context.justification != ""
>
> input.context.justification_kind in {
>
> "procurement", "str_investigation", "tax_audit",
>
> "corruption_inquiry", "kyc_account_opening"
>
> }
>
> }
>
> \# Encrypted-tier requires threshold-signed quorum
>
> allow if {
>
> input.action == "read"
>
> input.resource.classification == "encrypted"
>
> input.context.quorum_authorisation.threshold_signed == true
>
> count(input.context.quorum_authorisation.signers) \>= 7
>
> has_non_state_signer(input.context.quorum_authorisation.signers)
>
> }
>
> has_non_state_signer(signers) if {
>
> some s in signers
>
> s.organisation in {"civil_society_seat", "international_observer_seat"}
>
> }

**Hardware token enrolment**

Officers, analysts, and administrators authenticate with hardware-backed credentials. YubiKey 5C NFC is the project’s standard token; FIDO2/WebAuthn is the authentication protocol. Token enrolment is performed in person at the consortium’s offices with documented identity verification; tokens are linked to the user’s Keycloak identity at enrolment.

Token lifecycle: enrolment at onboarding; replacement at loss with documented re-verification; revocation at offboarding within the 24-hour window per V1 P4.

**Justification capture**

Access to restricted-tier and encrypted-tier data requires structured justification at the moment of access. The Access service captures the justification: the kind (procurement, investigation, audit, KYC), the operational context (the tender, the STR identifier, the audit case), the requesting principal, the time, the specific resources accessed. The justification is itself anchored in the audit channel; misuse is detectable through retrospective sampling.

**Session governance**

Sessions are explicit objects with explicit lifecycle. The Identity service tracks: session creation, session use (which resources are accessed during the session), session invalidation. Session invalidation occurs on: logout, policy change affecting the session’s principal, anomalous activity detection by the security monitoring, scheduled expiry.

**Cross-organisational federation**

Consumer institutions authenticate to the platform with their own institutional identities through SAML or OIDC federation. The federation is established case-by-case; each institution’s liaison engineer negotiates the federation parameters with the consortium’s identity team. The federation does not extend the institution’s access beyond what is documented in the consumer integration contract; identity claims from the institution are validated and constrained against the contract.

> **SUCCESS —** Identity and access are operationally sound when: every authorisation decision in production passes through OPA with sub-millisecond latency; every consequential access carries the documented justification; hardware-token coverage is 100% for officers, analysts, and administrators; SPIFFE workload identity is in place for every service in the mesh; federation with consumer institutions is operational for all eight consumer integrations.

**Performance and Latency Engineering**

> *Performance is engineered, not wished. The platform commits to specific latency budgets for specific operations; those budgets are decomposed across the stack; each tier in the stack carries its share of the budget; budgets are measured continuously. Performance regressions block releases.*

**Latency budgets**

The platform’s end-to-end latency budgets are documented per operation. Each budget is decomposed across the platform’s layers; engineers know the per-layer share of the budget for which their service is responsible.

|  |  |  |  |
|----|----|----|----|
| **Operation** | **p50 budget** | **p99 budget** | **Comments** |
| BEAC KYC lookup (single entity) | 100ms | 500ms | Read against canonical projection; high frequency |
| ARMP conflict-of-interest analysis (10 bidders) | 1s | 2s | Graph traversal across 10 entities |
| DGI on-demand lookup (single entity) | 200ms | 500ms | Cached aggressively |
| Declaration submission acceptance | 300ms | 500ms | Returns receipt; verification runs async |
| Verification (green-lane end-to-end) | 30s | 5 min | Pipeline target; documented in V4 P14 |
| Public Portal entity page (cached) | \<100ms | 200ms | CDN-served; cache invalidation event-driven |
| Public Portal entity page (uncached) | 500ms | 1.5s | Database round trip required |
| Investigation Workbench graph query | 1s | 3s | Cytoscape rendering excluded; on-demand |
| AI-assisted analyst query | 5s | 15s | Inference latency dominates; streamed to user |

**Budget decomposition**

The 500ms p99 budget for BEAC KYC lookup decomposes as follows. Engineers in each layer optimise to their share; the sum is the platform’s commitment.

|  |  |  |
|----|----|----|
| **Layer** | **Budget share** | **Detail** |
| TLS termination at gateway | 20ms | Connection reuse; session resumption |
| JWT validation | 5ms | Cached JWKS; minimal CPU |
| Rate-limit check at gateway | 5ms | Redis token-bucket; pipelined |
| Mesh hop to entity service | 10ms | mTLS within cluster; intra-zone |
| Authorisation (OPA) | 5ms | Local OPA sidecar |
| Cache lookup (Redis) | 10ms | p99; mostly p50 \< 1ms |
| Database query (if cache miss) | 100ms | Indexed; primary-key lookup |
| Serialisation and return path | 15ms | Protobuf encode + mesh hop back |
| Network egress to consumer | 330ms | Wide network; the dominant fraction; depends on consumer location |

**Caching strategy**

Caching is applied at multiple layers per principle 11 (observability before optimisation) and only where measurements support the caching’s benefit.

- Edge cache: Public Portal entity pages, BODS exports, static assets. Cache invalidation on entity events via the CDN purge API.

- API gateway cache: read-heavy GraphQL queries with short TTLs; never cached for restricted-tier or higher.

- Service-level cache (Redis): entity summary cache for KYC; person identifier cache for verification; ownership graph snapshot for the verification engine’s pattern detection signatures.

- Database query cache: PostgreSQL’s built-in plan caching; OpenSearch query result caching for fuzzy searches with stable inputs.

- AI inference cache: prompt caching at the provider; response caching at the gateway for deterministic prompts.

**Database query optimisation**

PostgreSQL queries are reviewed before production deployment. Every query that runs more than 10 times per minute carries: an EXPLAIN ANALYZE output captured in the service’s test fixtures; an index that supports the query; a documented p99 latency commitment. Queries that fail to meet their commitment in production trigger automatic investigation through pg_stat_statements alerting.

Neo4j queries follow the same discipline. Cypher queries are reviewed; PROFILE output is captured; indexes are explicit; LIMIT is applied where the result set is unbounded.

**Connection pooling**

Database connection pools are sized per service against measured concurrency. Default PgBouncer transaction-mode pool size: 25 connections per service instance, sized down through measurement. Pool exhaustion is a SEV-3 incident; the metric pgbouncer_cl_waiting alerts when waiting clients exceed threshold.

**Synthetic load testing in CI**

Performance regression tests run on every Program Increment boundary. The test harness simulates production load against the staging environment using k6 with the project’s scenarios in tests/performance/. Regression beyond the documented thresholds blocks the next PI from commencing until investigated and addressed.

**Profiling discipline**

Production profiling is enabled continuously through the platform’s observability stack: CPU profiling via pyroscope or pprof for Go and Rust services; memory profiling through the same; allocation tracing through tokio-console for Rust services. Profile-guided optimisation decisions are documented per ADR; no optimisation runs without observability supporting the decision.

> **SUCCESS —** Performance is sound when: every operation’s p99 latency is within budget over the rolling thirty-day window; regression tests at PI boundaries pass without exception; production profiling is operational across services; the budget decompositions match measured per-layer latencies within the documented tolerances; no production incident in the prior quarter was caused by an unforeseen performance characteristic.

**Offline-First and Multi-Device Engineering**

> *The Declarant Portal must work when the declarant’s connection does not. Public Portal users must be able to consult entity pages they have seen before, without connectivity. This is not a feature gate; it is a baseline expectation for a national platform serving regions with intermittent connectivity.*

**Service worker lifecycle**

Each PWA application registers a service worker on first visit. The service worker controls subsequent navigations within the application’s scope. The Workbox library provides the service worker primitives; the project’s configuration in each application’s vite.config.ts customises the strategy per asset class.

**Service worker registration**

> // applications/declarant-portal/src/main.tsx (excerpt)
>
> import { registerSW } from "virtual:pwa-register";
>
> const updateSW = registerSW({
>
> onNeedRefresh() {
>
> // Notify user that an update is available; prompt for refresh
>
> notifyUpdateAvailable(() =\> updateSW(true));
>
> },
>
> onOfflineReady() {
>
> // Notify user that the app is ready for offline use
>
> notifyOfflineReady();
>
> },
>
> onRegisterError(error) {
>
> // Service worker registration failed; log for diagnostics
>
> reportServiceWorkerError(error);
>
> },
>
> });

**Service worker strategies per asset class**

|  |  |  |
|----|----|----|
| **Asset class** | **Strategy** | **Notes** |
| Application shell (HTML, JS, CSS) | Precache + cache-first | Precached at install; updated through service-worker lifecycle |
| API GET responses for entity reads | Network-first with cache fallback | Cache TTL 1 hour; cache key includes auth context |
| API GET responses for declaration drafts | IndexedDB (Dexie), not cache | The Dexie store is the source of truth for offline drafts |
| API POST submissions | Network-only with offline queue | Failed submissions queue in IndexedDB; replayed by background sync |
| Static assets (icons, illustrations) | Cache-first, long TTL | Versioned URLs; cache invalidation by URL change |
| Image documents (user-uploaded attachments) | IndexedDB Blob storage | Persisted until submission acknowledged |

**IndexedDB schema design**

IndexedDB is the platform’s offline-first source of truth for client-owned data. The schema is versioned through Dexie’s version-migration mechanism; migrations are tested in CI.

Schema design principles:

- One database per application; multiple object stores within.

- Primary keys are UUID v7 (time-sortable; useful for index ordering).

- Compound indexes on the access patterns the application performs offline.

- Migration paths are forward-only; downgrade is not supported (the app refuses to operate on a newer schema if a user reverts to an older app version).

- Storage quotas are respected; the application warns the user at 70% utilisation and prompts for cleanup of submitted drafts.

**Background sync**

Background sync registers a tag with the service worker on submission failure; when connectivity returns, the service worker receives a sync event and processes the queued operations. The implementation is the standard Workbox BackgroundSyncPlugin with the project’s replay logic.

Sync semantics:

- Each queued operation has an idempotency key. Replay produces the same outcome as the original attempt.

- Operations are processed in submission order. Out-of-order replay is forbidden (a draft’s amendment cannot be applied before the original).

- Operations that fail with 4xx server errors are not retried; the user is notified and prompted to resolve the issue. Operations that fail with 5xx or network errors are retried with exponential backoff.

- Permanent failures move to a user-visible failure queue with the diagnostic information for support engagement.

**Conflict resolution**

For the Declarant Portal, drafts are owned by the device; multi-device editing of the same draft is not supported (V4 P17). The simplicity prevents conflict-resolution complexity.

For the Public Portal’s cached entity pages: the cache is read-only; conflicts cannot arise. Cache invalidation on the server side (entity state change) is propagated through the standard cache-control headers and the platform’s cache-versioning scheme; stale caches are tolerated within the documented eventual-consistency budget.

For the Officer Console’s limited offline cache: similarly read-only; users see the cached state with a clear staleness indicator and refresh on connection restore.

**Multi-device through Capacitor**

Native iOS and Android wrapping is provided for the Declarant Portal and Public Portal. The Capacitor build produces signed application bundles for the Apple App Store and Google Play; the consortium’s developer accounts publish them. Native wrapping adds: file-system access for document upload; push notifications for declaration status updates; better offline reliability (native apps survive aggressive iOS/Android cache eviction better than web PWAs).

Capacitor configuration includes:

- App identifier: cm.recor.declarant; cm.recor.public for each app.

- Permissions: minimum set needed (file access for upload, notifications for status, network for sync). No location, no camera (the camera flow uses the file picker).

- Universal links / app links: claimed for the consortium’s domain to permit the deep linking from email notifications to specific declaration drafts.

- Signing: production builds signed with the consortium’s mobile-app signing keys; signing keys are held in HSM-attested mobile-app provisioning processes.

**Low-bandwidth optimisations**

The platform is operated in regions with diverse network conditions. Specific optimisations:

- Image lazy-loading via the native loading="lazy" attribute and Intersection Observer for finer control.

- Brotli compression on every text response; mid-tier Brotli level for the dynamic responses, maximum level for the static assets at build time.

- Progressive image loading with low-quality image placeholders for high-resolution document previews.

- Code splitting per route; only the route’s code is loaded on navigation.

- Service worker preloading of likely-next routes based on user navigation patterns.

- API response pagination with cursor pagination; large list responses are not requested in single calls.

**Connectivity detection**

Applications detect connectivity through navigator.onLine and through actual API probe pings. Pure reliance on navigator.onLine is insufficient (it reports connected even when the connection is to a captive portal that has not authenticated). The platform’s connectivity probe pings a lightweight endpoint and trusts a successful round-trip more than the browser’s connectivity flag.

UI indication of connectivity state is explicit. Offline mode is shown clearly; queued operations are visible; sync progress is shown when sync is running. The user is not surprised by the application’s state; the state is always visible.

> **SUCCESS —** Offline-first and multi-device behaviour is operationally sound when: declarants in regions with intermittent connectivity successfully file declarations end-to-end at the documented success rates; Public Portal users experience meaningful cached read access during connectivity outages; the IndexedDB schema migrations apply without user-visible disruption across application versions; the Capacitor-wrapped native apps maintain feature parity with the web PWAs.

**Observability Engineering**

> *Doctrine 16 is non-negotiable: observability is a delivery requirement, not an enhancement. A service without observability cannot be deployed. This Part documents how the discipline operates.*

**Instrumentation standard**

OpenTelemetry is the cross-language instrumentation standard. Every service is instrumented for: metrics (Prometheus-compatible output), structured logs (JSON), traces (W3C trace-context propagation), and health probes (liveness, readiness, startup).

Language-specific SDKs are pinned per V3 P7. The instrumentation code lives in libraries/\<language\>/recor-observability/ and is consumed by every service through the language’s standard dependency mechanism. Engineers do not write instrumentation from scratch per service; they consume the library.

**Metric naming conventions**

Metric names follow the Prometheus best practices with the project’s prefix. The convention: recor\_\<service\>\_\<subsystem\>\_\<metric\>\_\<unit\>. Units are explicit. Counters end with \_total. Histograms have \_seconds, \_bytes, \_count suffixes as appropriate. Labels are stable; high-cardinality labels (per-user, per-entity) are forbidden.

Representative metrics per service:

> \# Counter: total operations performed
>
> recor_declaration_submissions_total{outcome="accepted\|rejected"}
>
> \# Histogram: operation latency
>
> recor_declaration_submission_duration_seconds{outcome="..."}
>
> buckets: 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1, 2.5, 5, 10
>
> \# Gauge: current state size
>
> recor_declaration_outbox_pending_count
>
> \# Counter: errors by class
>
> recor_declaration_errors_total{error_class="validation\|persistence\|...\|"}
>
> \# Distribution: payload sizes
>
> recor_declaration_payload_bytes

**Log structure standard**

Every log record is JSON with the project’s canonical fields. The fields are produced by the instrumentation library; engineers do not construct log records by hand.

> {
>
> "ts": "2026-06-15T09:32:14.123456Z", \# ISO 8601 UTC
>
> "level": "info", \# trace\|debug\|info\|warn\|error
>
> "service": "declaration", \# service name
>
> "service_instance": "declaration-7f8d2", \# pod identity
>
> "trace_id": "0af7651916cd43dd8448eb211c80319c",
>
> "span_id": "b7ad6b7169203331",
>
> "principal": "spiffe://recor.cm/minfi/declaration-service",
>
> "request_id": "01J9Y...",
>
> "msg": "declaration submitted",
>
> "declaration_id": "01J9X...",
>
> "entity_id": "01J9W...",
>
> "outcome": "accepted",
>
> "duration_ms": 287
>
> }

Logs are aggregated via Loki with multi-tenant separation by service. Retention 30 days at full fidelity; long-term archival to MinIO for compliance with the audit retention requirements (7 years for consequential events; the audit channel’s retention is separately managed in the audit service).

**Trace context propagation**

Every cross-service call carries the W3C traceparent header. The instrumentation library adds the header automatically on gRPC, HTTP, and Kafka messages. Async work scheduled by Temporal carries the trace context through the workflow execution. The platform achieves end-to-end traceability for any request that enters the platform; the trace surfaces every service the request touches with timing per hop.

**Alert routing**

Prometheus Alertmanager routes alerts to PagerDuty for on-call coverage. Alerts are organised by team: the SRE team holds the primary rotation; specialist teams (security, verification engineering, integration) hold escalation rotations for alerts in their domain.

Alert severity classification (separate from incident severity but related):

- P1: page on-call immediately; response time \< 15 minutes.

- P2: notify on-call; response time \< 1 hour business; \< 4 hours off-hours.

- P3: ticket; response time \< 1 business day.

- P4: informational; reviewed weekly.

**Dashboards**

Grafana dashboards are version-controlled in /dashboards/ and provisioned through Argo CD. Per-service dashboards present the four golden signals (latency, traffic, errors, saturation) at minimum. Per-flow dashboards present cross-service flows (declaration submission, verification end-to-end, consumer integration delivery).

Dashboards are not free-form. The project maintains a dashboard style guide; new dashboards conform to the style; deviations require review by the SRE lead. The discipline produces dashboards that on-call engineers can read fluently under incident pressure.

**Runbook discipline**

Every alert links to a runbook. The runbook explains: what the alert means; the immediate triage steps; the diagnosis steps; the remediation steps; the post-incident actions. Runbooks live in /docs/runbooks/ alongside the code. Alerts that do not link to a runbook are not permitted in production.

**Synthetic monitoring**

In addition to passive observability, the platform operates synthetic probes against critical paths: declaration submission end-to-end against the staging environment every 5 minutes; ARMP KYC lookup probe; ANIF goAML delivery probe; BODS export availability probe. Probe results feed alerts at the same severity as production-fault alerts.

> **SUCCESS —** Observability is operationally sound when: every service in production emits the four signal classes; every alert links to a runbook; trace context propagates end-to-end on representative request samples; dashboards are consumed by on-call engineers fluently as evidenced by incident response times; the alert noise rate is below the documented threshold (typically \<10% of alerts are false-positives).

**Security Engineering**

> *Security is engineered into the platform from the first commit, not added at a pre-launch gate. The discipline in this Part operationalises that posture: how threats are modelled, how the code is tested for security, how the platform is probed by adversaries on a documented schedule, and how the cryptographic substrate is kept ahead of the post-quantum transition.*

**Threat modelling**

Every service carries a STRIDE threat model in docs/security/threat-models/\<service\>.md. The model is authored at service inception, updated when the service’s architecture changes materially, and reviewed quarterly by the security function. STRIDE — Spoofing, Tampering, Repudiation, Information disclosure, Denial of service, Elevation of privilege — frames the threats; the threat model documents the threats observed for the service and the mitigations in place.

Representative threat-model entry for the declaration service:

> \# docs/security/threat-models/declaration.md (excerpt)
>
> \## Threats and mitigations
>
> \### T-001: Spoofing of declarant identity (Spoofing)
>
> Threat: An attacker submits a declaration as a different declarant.
>
> Mitigations:
>
> \- Keycloak authentication with TOTP for declarants; hardware-token for officers
>
> \- SPIFFE workload identity at every service boundary
>
> \- Cryptographic attestation of declarant intent (Ed25519 signature on the
>
> declaration payload by the declarant’s key derived from their identity)
>
> Residual risk: Low. Authentication compromise plus signing-key compromise
>
> required; both are protected through Keycloak’s strong-authentication discipline.
>
> \### T-002: Tampering with declaration content in transit (Tampering)
>
> Threat: An on-path attacker modifies the declaration between client and server.
>
> Mitigations:
>
> \- TLS 1.3 at every connection
>
> \- Application-level integrity through the declarant’s signature
>
> \- Replay protection through the nonce captured in the signature
>
> Residual risk: Negligible.
>
> \### T-003: Repudiation of submission by declarant (Repudiation)
>
> Threat: A declarant claims they did not submit a declaration that is in the registry.
>
> Mitigations:
>
> \- Cryptographic signature on every declaration; non-repudiation property of Ed25519
>
> \- Anchoring of the declaration event in the Fabric ledger
>
> \- OpenTimestamps anchoring of the audit channel to Bitcoin
>
> Residual risk: Negligible.
>
> \### T-004: Information disclosure through error messages (Information disclosure)
>
> Threat: An attacker probes the API to extract information through error messages.
>
> Mitigations:
>
> \- Generic error messages externally; detailed errors only in internal logs
>
> \- Rate limiting at the gateway
>
> \- Anomaly detection on error-rate patterns per principal
>
> Residual risk: Low.
>
> \### T-005: Denial of service through resource exhaustion (Denial of service)
>
> Threat: An attacker submits high-volume declarations to exhaust capacity.
>
> Mitigations:
>
> \- Rate limiting per-principal at the gateway
>
> \- Backpressure propagation from the declaration service
>
> \- Auto-scaling within configured ceilings
>
> \- DDoS protection at the network edge
>
> Residual risk: Moderate. Resource exhaustion is fundamentally a capacity question;
>
> the documented capacity ceilings define the platform’s commitment.
>
> \### T-006: Elevation of privilege via injection (Elevation of privilege)
>
> Threat: An attacker exploits an injection vulnerability to escalate privileges.
>
> Mitigations:
>
> \- Type-safe database access through sqlx/sqlc (no string-concatenated SQL)
>
> \- Input validation at every API boundary
>
> \- Output encoding for any user-supplied content rendered in UIs
>
> \- Semgrep and CodeQL static analysis in CI
>
> \- Sandboxed execution where untrusted content is processed
>
> Residual risk: Low.

**Security testing in CI**

Security testing runs on every pull request, not at a pre-release gate. The CI security stage includes:

- Static analysis — Semgrep with the project’s rules and the community OWASP ruleset; CodeQL with the security-extended query suite for each language.

- Dependency scanning — Trivy and Snyk Container against the SBOM produced at build; CVE matching against the published advisory feeds; failure threshold at High severity.

- Secrets scanning — detect-secrets and gitleaks scan every diff for credential patterns; baseline secrets are pre-approved through a documented exception list.

- Container image scanning — Trivy scans the built container image for OS-level vulnerabilities; base-image updates are tracked through Renovate.

- Infrastructure-as-code scanning — Checkov against the Terraform modules; tfsec as the secondary scanner; OPA Conftest for Kubernetes manifests.

- License scanning — license-scanner against the dependency tree; forbidden licenses (GPL, AGPL) fail the build.

**Dynamic application security testing**

DAST runs nightly against the staging environment using OWASP ZAP with the project’s authenticated-scan configuration. Findings above the configured severity threshold are filed as security tickets and triaged by the security function within the SLA documented for the finding’s severity.

**Penetration testing schedule**

External penetration testing is performed on a documented cadence. The cadence reflects the platform’s lifecycle: build phases see one pentest at each phase gate; operational phases see two pentests per year by independent firms (alternating between firms to diversify perspective).

|  |  |  |
|----|----|----|
| **Phase** | **Pentest scope** | **Notes** |
| Pre-Phase III gate | Cryptographic substrate, identity, foundation Layer 2 services | First pentest; baseline finding triage |
| Pre-Phase IV gate | Verification engine, full Layer 2 | Focused on the verification engine’s adversarial robustness |
| Pre-launch (Phase V) | Full platform; red-team adversarial simulation | Largest engagement; covers consumer integrations, applications, full data flows |
| Operational year 1 | Two pentests; first six months and end of year | Focus on changes since the prior pentest |
| Subsequent operational years | Twice yearly | Ongoing posture validation |

Pentest findings are remediated under documented SLAs: Critical findings within 7 days; High within 30 days; Medium within 90 days; Low at the next operational quarter. SLA exceptions require named approval and are tracked on the security risk register.

**Bug bounty**

Post-launch the platform operates a bug bounty programme through a recognised platform provider (HackerOne or Bugcrowd at the consortium’s selection). Scope is documented: Public Portal, public APIs, the BODS export channel; out-of-scope: social engineering, physical attacks, denial of service. Bounty levels follow industry-standard tiers; the consortium’s legal framework provides safe-harbour protection for good-faith researchers.

**Vulnerability disclosure**

The platform publishes a vulnerability disclosure policy at /docs/security/vdp.md and at the canonical URL https://recor.cm/.well-known/security.txt. The policy commits to: acknowledgement of reports within 72 hours; triage within 7 days; resolution per the SLAs above; named credit at the reporter’s discretion.

**Cryptographic agility roadmap**

Post-quantum cryptography is engineered into the platform’s substrate per Doctrine 21. The platform is not running PQ primitives today; it is engineered so that the migration is a matter of configuration, not rebuild.

- Algorithm-identifier negotiation at every cryptographic protocol layer. Current operations declare their algorithm; consumers honour the declaration; switching algorithms is metadata-driven.

- Key rotation procedures are documented as ceremonies; the same procedures handle algorithm migration as a special case of rotation.

- Hybrid signatures are planned: signatures will be doubled (classical Ed25519 + PQ Dilithium) during the migration window, with verifiers honouring either; once the PQ algorithm is sufficient on its own, classical can be retired.

- Monitoring of NIST PQC progression and FIPS 203/204/205 production-readiness through the security function’s standing agenda.

- Hybrid TLS via the OpenSSL-1.1.1 + OQS-OpenSSL providers (or successor) when production-ready for the platform’s TLS endpoints.

The migration timing is conditional on ecosystem maturity. The platform’s commitment is to be among the first national-scale platforms to migrate when the ecosystem is ready; the engineering preparation is complete in advance.

**Security risk register**

The security function maintains a risk register documenting: identified threats not fully mitigated; accepted risks with documented rationale; tracked mitigations in progress; quarterly review status. The register is itself classified Restricted; the public-shareable summary is published in the project’s transparency report.

> **SUCCESS —** Security engineering is operationally sound when: every service has a current threat model reviewed in the prior quarter; CI security gates pass on every merged pull request; pentest findings are remediated within SLA; the bug bounty programme post-launch produces a steady flow of low-severity findings and zero high-severity findings reaching production; the cryptographic agility roadmap is on schedule against the public PQ ecosystem milestones.

**Development Environment**

> *A reproducible, secure, productive development environment is itself a precondition for the platform’s standard. The discipline in this Part covers what every engineer is issued on day one and how they bootstrap to productive work within hours, not weeks.*

**Approved development hardware**

Engineering personnel are issued consortium-managed laptops. Personal devices are not permitted to access platform code repositories or restricted-tier development resources. The hardware baseline:

- Apple MacBook Pro M3/M4 (14" or 16") or Lenovo ThinkPad X1 / P-series with Intel Core Ultra or AMD Ryzen 7000-series.

- Minimum specifications: 32 GB RAM, 1 TB NVMe SSD, hardware-attested TPM 2.0 or Apple Secure Enclave.

- Full-disk encryption mandatory (FileVault on macOS, LUKS on Linux).

- Managed by the consortium’s MDM (Jamf for macOS, Microsoft Intune for Windows-on-Lenovo, MeshCentral or open-source MDM for Linux laptops at the consortium’s preference).

**Operating system baseline**

Approved OS choices: macOS 15.x or later; Ubuntu 24.04 LTS; Fedora Workstation 41 or later. Windows is not approved for primary engineering work; engineers requiring Windows use it inside a managed Hyper-V VM on a Linux host. The baseline image includes the project’s pre-installed tooling, the security baselines (EDR, host firewall configuration, screen lock policy), and the consortium’s certificate authority for mTLS to internal services.

**Toolchain bootstrap**

Engineers run a single script that installs every toolchain to the project’s pinned versions. The script is idempotent and re-runnable; it does not modify the engineer’s personal preferences (shell, editor configurations are out of scope).

> \# Bootstrap script (excerpt)
>
> \# Run after cloning the repository:
>
> just bootstrap
>
> \# Behind the scenes:
>
> \# 1. Install mise (toolchain version manager) if not present
>
> curl https://mise.run \| sh
>
> \# 2. Install all toolchains pinned in mise.toml at the repo root
>
> mise install
>
> \# 3. Install OS-level dependencies (libcryptoki for HSM, pkg-config, etc.)
>
> just \_install-system-deps
>
> \# 4. Install internal CLIs (just, recor-cli, dev-helpers)
>
> just \_install-internal-cli
>
> \# 5. Configure direnv for per-directory environment activation
>
> just \_configure-direnv
>
> \# 6. Verify by running the test suite for a representative service
>
> cd services/entity && just check
>
> \# Expected: passes within minutes; failure indicates bootstrap incomplete

The mise.toml at the repository root pins every toolchain version:

> \# mise.toml
>
> \[tools\]
>
> rust = "1.84.0"
>
> go = "1.26.2"
>
> node = "22.11.0"
>
> pnpm = "9.12.3"
>
> python = "3.12.7"
>
> uv = "0.5.4"
>
> java = "21.0.4" \# for the few cases requiring JVM (HSM SDK adapters)
>
> buf = "1.47.2"
>
> just = "1.36.0"
>
> terraform = "1.10.0"
>
> kubectl = "1.32.0"
>
> helm = "3.16.3"
>
> sops = "3.9.1"
>
> age = "1.2.0"
>
> \[env\]
>
> RECOR_DEV = "1"

**Local development environment**

Local development uses kind (Kubernetes-in-Docker) for the platform’s service mesh and docker-compose for the heavier dependencies (PostgreSQL, Neo4j, OpenSearch, Kafka, Redis). The setup is automated; a fresh checkout to running platform takes under 30 minutes.

- kind cluster with 4 nodes (1 control plane, 3 worker) provisioned with the platform’s base manifests.

- Istio installed via the project’s Helm chart with mTLS in permissive mode for local development.

- PostgreSQL 17 in docker-compose with pre-loaded test data for each service’s schema.

- Neo4j Enterprise (single-node) with starter graph data.

- OpenSearch single-node with index templates pre-applied.

- Kafka in KRaft mode with the project’s topics pre-created.

- Redis single-node.

- Vault in dev mode with the project’s policies pre-loaded.

- Software-emulated HSM (SoftHSM2) for cryptographic operations during development; production HSM access is not granted to development environments.

**Test data fixtures**

Realistic synthetic test data is produced by the recor-test-data-generator (Python) into the project’s fixtures. The generator produces entities, persons, declarations, and ownership chains with declared statistical distributions matching the production data’s shape. Engineers consume the fixtures through the recor-test-utils library; tests construct ad-hoc data where needed but reach for fixtures first.

**Secrets handling in development**

Development secrets are managed via sops with age encryption keys held in each engineer’s yubikey. Encrypted secrets are committed to the repository; decryption requires the engineer’s age key. Production secrets are never available in development; the discipline is enforced by the secrets-scanner CI gate and by manual review.

**Editor and IDE policy**

Engineers use the editor of their choice. The project supports first-class integration with VS Code, JetBrains IDEs (IntelliJ, GoLand, RustRover, WebStorm), and Neovim. Recommended extensions and configurations are documented per editor in docs/onboarding/editor-setup/. Editor-specific configuration files in the repository are minimal and editor-neutral; engineers customise their own setup.

**Claude Code installation**

Claude Code is installed via Anthropic’s documented installation path. The CLI is configured with the engineer’s identity through the project’s onboarding script; team-level configuration (settings.json, agent definitions, skills) is pulled from the repository’s .claude/ directory automatically at first invocation. The team’s subscription is administered centrally; engineers do not configure billing or API keys personally.

**Onboarding checklist**

The onboarding checklist is a structured first-week activity for new engineers.

- Day 1: HR onboarding; identity issuance; hardware setup; mise install; first bootstrap run.

- Day 2: doctrine onboarding (V1 P2); OPSEC training (V1 P4); reading paths through this document; environment validation.

- Day 3: Claude Code onboarding (V2 P5); first agent-assisted exercise with senior engineer pairing.

- Day 4-5: representative ticket assigned; full workflow exercised end-to-end with mentor review.

- Week 2-4: graduated independence; routine commit access; first sprint participation.

> **SUCCESS —** The development environment is operationally sound when: a new engineer completes bootstrap to first passing test within their first day; the discipline of secrets, identity, and supply-chain is enforced at the engineer’s workstation by configuration rather than convention; local development reproduces production behaviour faithfully enough that test passes correlate with production reliability; engineers report the environment as enabling rather than obstructing their work.

**Continuous Integration**

> *CI is the platform’s automated standard-enforcement. Every doctrine that can be expressed as a machine check is expressed; every pull request runs the full check suite; failures block merge. CI is non-negotiable; CI runs on every change; CI is fast enough that engineers respect rather than circumvent it.*

**Pipeline stages**

Every pull request runs through a uniform pipeline composed of nine stages. Stages run in parallel where possible; the pipeline’s wall-clock duration target is under 15 minutes for a representative service’s change.

|  |  |  |
|----|----|----|
| **\#** | **Stage** | **What it does** |
| 1 | Pre-flight | Linting (per-language formatters and linters), branch policy verification, commit-message verification, conflict detection |
| 2 | Build | Bazel build of affected targets; hermetic build verification |
| 3 | Unit tests | Per-package unit tests; coverage measurement against the documented threshold |
| 4 | Integration tests | Tests against ephemeral PostgreSQL, Redis, OpenSearch; testcontainers-based |
| 5 | Contract tests | Pact contract verification; consumer-driven contract tests against the consumer’s mock |
| 6 | Security analysis | Semgrep, CodeQL, Trivy, secrets scanning, license scanning |
| 7 | Supply chain | SBOM generation (CycloneDX), dependency-hash verification, SLSA Level 4 provenance generation, Sigstore signing of artefacts |
| 8 | Performance smoke | Targeted performance regression against the prior baseline; full performance regression at PI boundaries only |
| 9 | Doctrine check | Architect-reviewer agent runs against the diff with the doctrines as the rubric; surfaces any doctrine concerns |

**Per-language CI templates**

Per-language details are factored into reusable GitHub Actions workflows that each service’s pipeline references. The templates encode the per-language coding standards documented in V3 P8.

**Rust CI template**

> \# .github/workflows/\_rust-ci.yaml (referenced from service workflows)
>
> name: Rust CI
>
> on:
>
> workflow_call:
>
> inputs:
>
> service-path:
>
> required: true
>
> type: string
>
> jobs:
>
> rust-checks:
>
> runs-on: \[self-hosted, linux, recor-runner\]
>
> steps:
>
> \- uses: actions/checkout@v4
>
> \- uses: ./.github/actions/setup-rust
>
> \- name: Format check
>
> run: cargo fmt --all -- --check
>
> \- name: Clippy at deny-warnings
>
> run: cargo clippy --all-targets --all-features -- -D warnings
>
> \- name: Test
>
> run: cargo nextest run --all-features
>
> \- name: Test coverage
>
> run: cargo llvm-cov --fail-under-lines 85
>
> \- name: Cargo deny (dependency policy)
>
> run: cargo deny check
>
> \- name: Cargo audit (advisory matching)
>
> run: cargo audit --deny warnings

**Go CI template**

> \# .github/workflows/\_go-ci.yaml
>
> name: Go CI
>
> on:
>
> workflow_call: {}
>
> jobs:
>
> go-checks:
>
> runs-on: \[self-hosted, linux, recor-runner\]
>
> steps:
>
> \- uses: actions/checkout@v4
>
> \- uses: ./.github/actions/setup-go
>
> \- name: Format check
>
> run: test -z "\$(gofmt -l .)"
>
> \- name: golangci-lint
>
> run: golangci-lint run --config .golangci.yml
>
> \- name: Test with race detector
>
> run: go test -race -coverprofile=cover.out ./...
>
> \- name: Coverage threshold
>
> run: \|
>
> go tool cover -func=cover.out
>
> go run tools/coverage-gate.go --threshold 80
>
> \- name: govulncheck
>
> run: govulncheck ./...

**TypeScript CI template**

> \# .github/workflows/\_ts-ci.yaml
>
> name: TypeScript CI
>
> on:
>
> workflow_call: {}
>
> jobs:
>
> ts-checks:
>
> runs-on: \[self-hosted, linux, recor-runner\]
>
> steps:
>
> \- uses: actions/checkout@v4
>
> \- uses: ./.github/actions/setup-node
>
> \- name: Type check
>
> run: pnpm tsc --noEmit
>
> \- name: ESLint
>
> run: pnpm eslint --max-warnings 0 .
>
> \- name: Prettier check
>
> run: pnpm prettier --check .
>
> \- name: Test
>
> run: pnpm vitest run --coverage
>
> \- name: Coverage threshold
>
> run: pnpm coverage-gate --threshold 80
>
> \- name: Bundle size check
>
> run: pnpm build && pnpm bundle-size-check

**Quality gates with exact thresholds**

Quality gates are explicit; the project does not maintain implicit standards that surface only at review time. The thresholds:

|  |  |  |
|----|----|----|
| **Gate** | **Threshold** | **Notes** |
| Unit test coverage — Rust | ≥85% lines per crate | Higher for cryptographic substrate (90%) |
| Unit test coverage — Go | ≥80% lines per package | Higher for verification engine (90%) |
| Unit test coverage — TypeScript | ≥80% lines per package | Higher for offline-sync logic (90%) |
| Static analysis findings (Critical) | 0 | Hard block |
| Static analysis findings (High) | 0 net new | Pre-existing findings tracked separately; new findings block |
| Vulnerable dependencies (High+) | 0 unaddressed beyond SLA window | SLA defined in V3 P7 |
| Bundle size (frontend) | Per-app budget; tracked | Increases \>10% over baseline require justification |
| Build provenance signature | Required (SLSA L4) | Hard block |
| SBOM presence | Required | Hard block |

**CI infrastructure**

Self-hosted GitHub Actions runners on dedicated Kubernetes pools. The runners are hermetic: each job runs in a fresh ephemeral container with no carry-over from prior jobs. Runner pools are sized to keep queue times under 30 seconds during normal workload. Runners are isolated from the production network; they have egress only to the approved package mirrors and to the Anthropic API for the architect-reviewer agent.

**SBOM and signing**

Every build artefact carries a CycloneDX SBOM generated by Syft. The SBOM includes every dependency at the exact version with cryptographic hash. The artefact is signed by Sigstore Cosign at build time; the signature is anchored in the project’s rekor instance and mirrored to the public Sigstore transparency log.

Deployment verifies the signature before admitting the artefact to the target environment. Unsigned artefacts cannot reach any environment above local development; the admission controller in Kubernetes enforces this through the signed-image policy.

> **SUCCESS —** CI is operationally sound when: every merged pull request has passed the full pipeline; the pipeline median duration is below the target; the doctrine-check stage surfaces violations effectively without producing intolerable noise; signed artefacts are produced at every build; the supply-chain audit can trace any production artefact back through its signed provenance to the specific source commit and the specific reviewer signatures.

**Continuous Delivery**

> *Delivery is the discipline of moving artefacts from “merged” to “operating” safely, predictably, and reversibly. The platform’s delivery is GitOps-driven: every environment’s state is declared in Git; Argo CD reconciles the cluster to the declared state; no engineer kubectl-applies to production.*

**Environment progression**

Four environments operate the platform across its lifecycle. Each environment has documented purpose, documented approval requirements for promotion, and documented data lineage.

|  |  |  |
|----|----|----|
| **Environment** | **Purpose** | **Data and approvals** |
| dev | Engineer’s local cluster (kind) plus a shared cloud dev cluster for cross-team integration testing | Synthetic data; auto-deployment on merge to main |
| staging | Pre-production environment with production-like topology | Synthetic data with production-like scale; auto-promotion from dev after CI success; engineer-initiated deployments for feature branches |
| pre-production | Production-equivalent environment used for release rehearsal and consumer-integration acceptance testing | Anonymised production data refreshed monthly; promotion requires named release engineer approval |
| production | Live operational environment | Live data; promotion requires release manager + on-call SRE + security-on-call approval |

**GitOps with Argo CD**

Argo CD 2.13.x reconciles each cluster to the state declared in /infrastructure/argocd/ in the repository. Each environment has its own Application definitions; promotion is a pull request that updates the environment’s Application to point at a newer artefact version.

> \# infrastructure/argocd/production/declaration-service.yaml
>
> apiVersion: argoproj.io/v1alpha1
>
> kind: Application
>
> metadata:
>
> name: declaration-service-production
>
> namespace: argocd
>
> spec:
>
> project: recor-production
>
> source:
>
> repoURL: https://gitea.recor.cm/recor/recor.git
>
> targetRevision: main
>
> path: infrastructure/helm/declaration
>
> helm:
>
> releaseName: declaration
>
> valueFiles:
>
> \- values-production.yaml
>
> parameters:
>
> \- name: image.tag
>
> value: "v1.42.7" \# the version being deployed
>
> destination:
>
> server: https://prod-cluster.recor.cm:6443
>
> namespace: recor-services
>
> syncPolicy:
>
> automated:
>
> prune: true
>
> selfHeal: true
>
> syncOptions:
>
> \- CreateNamespace=false \# namespaces are pre-provisioned
>
> \- ServerSideApply=true
>
> revisionHistoryLimit: 30 \# for rollback

**Canary deployments**

Production deployments are canary-style by default. Argo Rollouts (sidecar to Argo CD) orchestrates the canary progression.

- Canary stage 1 — 5% of traffic for 10 minutes; observed metrics within thresholds; auto-progression.

- Canary stage 2 — 25% of traffic for 30 minutes; metrics within thresholds.

- Canary stage 3 — 50% of traffic for 60 minutes.

- Full rollout — 100% of traffic; canary retired.

- Any stage failure (metric outside threshold, on-call manual abort) triggers automatic rollback to the prior version.

Canary metrics evaluated automatically: error rate (p99 \< 0.1% above baseline); latency (p99 \< 10% above baseline); saturation (CPU \< 70%, memory \< 80%); business metric (declaration success rate, KYC lookup success rate, varies per service).

**Rollback procedures**

Rollback is a first-class operation, not an emergency procedure. The release engineer initiates rollback through Argo CD’s rollback to a prior revision; the rollback completes within the same SLO as a forward deployment. Database migrations are designed to be reversible (V4 P12 migration governance); rollback of a release that included a non-reversible migration is itself documented and requires named approval.

**Database migration strategy**

Migrations are applied in a structured sequence to preserve compatibility across in-flight deployments.

- Stage 1: Expand. The new schema is applied; the schema accommodates both old and new code (additive only).

- Stage 2: Deploy new code. The service deploys against the expanded schema; both old and new behaviours are operational.

- Stage 3: Migrate data. Data is migrated to the new schema where the migration requires data movement.

- Stage 4: Contract. The old schema elements are removed once the new code has fully replaced the old behaviour.

Each migration stage is a separate release. The expand-migrate-contract pattern eliminates the class of deployment failures caused by simultaneous schema and code change.

**Feature flags**

Feature flags decouple deployment from release. The platform uses a self-hosted feature flag service (Flipt 1.x) with the project’s flag-management discipline.

- Every feature behind a flag has an owner team.

- Flags have a lifecycle: created, enabled-for-X-percent, enabled-globally, removed. Flags older than two PIs that have not transitioned through the lifecycle are flagged in the engineering retrospective.

- Flags evaluate locally in each service through SDK with periodic fetch of the flag state from the flag service. Network failure to the flag service falls back to the cached state.

- Flag changes are auditable: who changed which flag when.

**Release coordination**

Releases are coordinated through the release calendar. Releases occur on documented cadence (V2 P6 release workflow); ad hoc releases require named approval. Consumer institutions are notified of upcoming releases through the consumer notification channel; the notification includes the release content summary and any consumer-facing changes.

> **SUCCESS —** Continuous delivery is operationally sound when: every production deployment proceeds through canary stages without manual override; rollback when needed completes within SLO; database migrations across the expand-contract pattern apply without service disruption; feature flags are used to decouple deployment from release as the project’s default discipline; the consumer notification surface is reliably engaged for every consumer-affecting change.

**Infrastructure as Code**

> *Every piece of infrastructure on the platform is declared in code, reviewed in pull requests, and applied through documented procedures. Manual cluster changes are forbidden; the discipline that infrastructure is code is the load-bearing reproducibility property.*

**Tooling**

- Terraform 1.10.x for cloud and IaC primitives — cluster provisioning, networking, DNS, certificate issuance, identity bindings.

- Kubernetes manifests directly authored for the platform’s namespaces, base policies, and operators.

- Helm charts for the platform’s services, packaged centrally in /infrastructure/helm/.

- Ansible 9.x for bare-metal provisioning and HSM ceremonies (operations not amenable to declarative reconciliation).

- Argo CD for ongoing reconciliation of declared state.

- Argo Rollouts for advanced deployment strategies (canary, blue/green).

- Open Policy Agent for cluster admission policies.

**Repository layout**

Infrastructure code lives under /infrastructure/ as documented in V4 P10. The sub-directories:

> infrastructure/
>
> ├── terraform/
>
> │ ├── modules/ \# reusable modules
>
> │ │ ├── k8s-cluster/
>
> │ │ ├── postgres-ha/
>
> │ │ ├── minio-cluster/
>
> │ │ ├── keycloak/
>
> │ │ └── vault/
>
> │ ├── environments/
>
> │ │ ├── dev/
>
> │ │ ├── staging/
>
> │ │ ├── pre-production/
>
> │ │ └── production/
>
> │ └── backend.tf \# state storage configuration
>
> ├── kubernetes/
>
> │ ├── base/ \# base manifests applied to every cluster
>
> │ ├── environments/ \# per-environment overlays
>
> │ └── operators/ \# operator deployments (Istio, Argo, Vault, etc)
>
> ├── helm/ \# per-service Helm charts
>
> │ ├── declaration/
>
> │ ├── verification/
>
> │ ├── entity/
>
> │ └── ... \# one chart per service
>
> ├── argocd/ \# Application definitions
>
> │ ├── dev/
>
> │ ├── staging/
>
> │ ├── pre-production/
>
> │ └── production/
>
> ├── ansible/
>
> │ ├── hsm-bootstrap/
>
> │ ├── hsm-ceremony/
>
> │ └── gpu-cluster-bootstrap/
>
> └── networks/ \# NetworkPolicy manifests

**Cluster bootstrap**

A new Kubernetes cluster is brought up through a documented sequence. The sequence is rehearsed at every fresh environment build; the runbook lives at /docs/runbooks/cluster-bootstrap.md.

- Terraform applies the cluster provisioning module: control-plane nodes, worker nodes, load balancer, DNS, certificate authority.

- Ansible configures the bare-metal hosts where applicable (HSM-attested nodes, GPU nodes for sovereign inference).

- kubeadm initialises the Kubernetes control plane on the provisioned nodes.

- Argo CD is installed via Helm; the cluster is added to the consortium’s GitOps fleet.

- Argo CD reconciles the cluster to its declared state: namespaces, RBAC, NetworkPolicies, Istio, Vault, Keycloak, observability stack.

- Validation: the cluster’s smoke tests run; readiness is gated on test pass.

**HSM provisioning**

HSM provisioning is the most operationally sensitive infrastructure activity. The Thales Luna Network HSMs are procured per consortium organisation; each HSM serves the originating organisation’s partition. The provisioning sequence:

- Physical installation in the secure data centre with documented chain-of-custody.

- Initial cryptographic officer authentication using factory-provided credentials, immediately rotated.

- Partition creation per the consortium’s policy; partition policies set per the project’s HSM template.

- Generation of the operator-card sets per partition with the consortium’s cryptographic officers attending.

- Connection to the platform’s key-management infrastructure via the HSM client wrapper.

- Audit-record creation in the consortium’s ceremony log with all participants’ signatures.

**Network policies**

Every namespace has Kubernetes NetworkPolicies that explicitly enumerate allowed ingress and egress. The default-deny posture: a pod cannot send or receive traffic unless a policy permits it. Policies are reviewed quarterly; new services’ policies are part of the service’s deployment artefact.

> \# infrastructure/kubernetes/base/networkpolicies/declaration-service.yaml
>
> apiVersion: networking.k8s.io/v1
>
> kind: NetworkPolicy
>
> metadata:
>
> name: declaration-service-policy
>
> namespace: recor-services
>
> spec:
>
> podSelector:
>
> matchLabels:
>
> app: declaration
>
> policyTypes: \[Ingress, Egress\]
>
> ingress:
>
> \- from:
>
> \- namespaceSelector:
>
> matchLabels:
>
> recor-mesh: "true"
>
> \- podSelector:
>
> matchLabels:
>
> app: api-gateway
>
> ports:
>
> \- protocol: TCP
>
> port: 8443 \# gRPC mTLS
>
> egress:
>
> \- to:
>
> \- podSelector:
>
> matchLabels:
>
> app: postgres-declaration
>
> ports:
>
> \- protocol: TCP
>
> port: 5432
>
> \- to:
>
> \- podSelector:
>
> matchLabels:
>
> app: redis
>
> ports:
>
> \- protocol: TCP
>
> port: 6379
>
> \- to:
>
> \- podSelector:
>
> matchLabels:
>
> app: kafka
>
> ports:
>
> \- protocol: TCP
>
> port: 9092
>
> \- to:
>
> \- namespaceSelector:
>
> matchLabels:
>
> name: vault
>
> podSelector:
>
> matchLabels:
>
> app: vault
>
> ports:
>
> \- protocol: TCP
>
> port: 8200

**State and secret management**

Terraform state is stored in encrypted S3-compatible storage (MinIO) with state locking via the project’s DynamoDB-compatible service or PostgreSQL backend. State access is restricted to the SRE team; production state requires named approval to read.

Secrets in Kubernetes are managed through Vault’s Kubernetes auth method plus the Vault CSI driver. Pods request secrets at startup; secrets are injected into in-memory files that the pod consumes. Kubernetes Secret objects are minimal and reserved for non-sensitive configuration.

> **SUCCESS —** Infrastructure as code is operationally sound when: every cluster’s state can be reconstructed from the IaC repository to a working environment within the documented RTO; no manual changes have been applied to any production cluster in the prior quarter; HSM ceremonies have been performed under the documented procedure with audited outputs; NetworkPolicies are present on every namespace with documented allowed flows.

**Operations Runbooks**

> *Runbooks are the operational instantiation of the doctrine that observability is non-optional. Every alert links to a runbook; every runbook explains what the alert means and how to respond. The discipline produces incident response that is fast, accurate, and consistent regardless of which engineer is on call.*

**Runbook structure**

Every runbook follows the project’s template. The structure ensures runbooks are usable under incident pressure when the on-call engineer cannot read prose at leisure.

> \# Runbook: \<Alert name\>
>
> \## What this alert means
>
> \<One sentence; the meaning of the alert in domain terms\>
>
> \## Severity
>
> \<P1 \| P2 \| P3 \| P4\>
>
> \## Immediate triage (first 5 minutes)
>
> 1\. Check the linked dashboard: \<URL\>
>
> 2\. Determine whether the alert is real or a false positive:
>
> \<specific verification steps\>
>
> 3\. If real, classify severity:
>
> \<criteria for escalation\>
>
> \## Diagnosis (next 15 minutes)
>
> 1\. \<Specific diagnostic step\>
>
> 2\. \<Specific diagnostic step\>
>
> 3\. \<Specific diagnostic step\>
>
> \## Common causes and remediations
>
> \- Cause A: \<description\> → Remediation: \<action\>
>
> \- Cause B: \<description\> → Remediation: \<action\>
>
> \- Cause C: \<description\> → Remediation: \<action\>
>
> \## Escalation
>
> \- If unable to resolve within \<X minutes\>, escalate to \<named role/rotation\>
>
> \- For SEV-1/SEV-2, also notify \<named role\>
>
> \## Post-incident
>
> \- File a post-incident report at \<path\>
>
> \- Update this runbook if the incident exposed a gap in it

**Incident severity classification**

Operational incident severity is documented separately from security incident severity (V1 P4). The operational severities:

|  |  |
|----|----|
| **Severity** | **Definition and response** |
| SEV-1 | Platform-wide outage; major consumer integration failing; production data integrity at risk. Response: full incident response activated; Incident Commander appointed; updates to consortium leadership within 1 hour; root cause and corrective action documented within 7 days. |
| SEV-2 | Significant service degradation; one consumer integration failing with workaround; performance degraded but not failed. Response: on-call engineering activated; updates to engineering leadership within 4 hours; corrective action documented within 14 days. |
| SEV-3 | Limited service degradation; single feature failing; non-critical service slow. Response: regular triage queue; corrective action in the next sprint. |
| SEV-4 | Cosmetic issues; documented latency variations within budget; informational alerts. Response: tracked in the backlog; reviewed weekly. |

**On-call rotation**

On-call rotation is staffed across the engineering team with documented coverage. The rotation pattern:

- Primary on-call: rotates weekly; takes the first page on any alert.

- Secondary on-call: rotates weekly offset from primary; backs up if primary is unavailable; provides reasoning support during incident response.

- Specialist on-call: per-team rotations for security, verification engineering, cryptographic substrate; engaged for incidents in their domain.

- SRE lead on-call: management coverage; engaged for SEV-1 and SEV-2 escalations.

- Consortium escalation: documented contact for the lead-architect and security-lead roles; engaged for SEV-1 incidents and for incidents with public-communications implications.

On-call expectations: response within 15 minutes for SEV-1/SEV-2; presence in the incident channel within the same window. Compensation for on-call time is included in the project’s personnel arrangements. Burnout-protection: no engineer is on call more than one week in four; consecutive on-call weeks are forbidden.

**Game days**

Game days are scheduled chaos exercises run quarterly in pre-production. The exercise injects failure scenarios that the team responds to as if real, exercising the full incident response process including communication and post-incident review.

Representative scenarios:

- Postgres primary failure with replica failover.

- Kafka broker loss with consumer group rebalance.

- Inference gateway failure with cascading service degradation.

- HSM partition failure on one site with failover to the other.

- Argo CD reconciliation drift on a production-equivalent cluster.

- Coordinated multi-component failure simulating a regional outage.

Game days produce findings: gaps in runbooks, untested failure modes, communication-coordination weaknesses. Findings are tracked through completion in the project’s engineering backlog.

**Post-incident review**

Every SEV-1 and SEV-2 incident produces a post-incident review (PIR). The PIR is held within five business days; produces a structured report against the project’s template; focuses on systemic conditions rather than individual error; produces action items that are tracked through completion.

The PIR template covers: incident summary; timeline (events, observations, decisions, communications); root cause (technical and process); contributing factors; what went well; what could have gone better; action items (each with owner and target date); broader lessons. The completed PIR is published to the engineering surface; sensitive content is redacted before publication.

**Standing runbooks**

Beyond the per-alert runbooks, the project maintains a small set of standing runbooks that cover scenarios not tied to a specific alert. These include:

- /docs/runbooks/release.md — the full release procedure.

- /docs/runbooks/rollback.md — emergency rollback procedure.

- /docs/runbooks/cluster-bootstrap.md — bringing up a new cluster.

- /docs/runbooks/hsm-ceremony.md — the HSM ceremony procedures.

- /docs/runbooks/dr-failover.md — disaster recovery procedures (V6 P29).

- /docs/runbooks/onboard-engineer.md — engineer onboarding sequence.

- /docs/runbooks/offboard-engineer.md — engineer offboarding within 24 hours.

- /docs/runbooks/security-incident.md — security incident response.

> **SUCCESS —** Operations runbooks are operationally effective when: every alert links to a runbook that on-call engineers report as useful; mean time to acknowledge alerts is within the documented target; mean time to resolve incidents is within target by severity class; game day findings are addressed before the next quarterly exercise; PIRs produce action items that demonstrably reduce recurrence of the addressed failure mode.

**Disaster Recovery**

> *The platform serves national infrastructure with multi-decade operational horizons. Disaster recovery is engineered to a standard that contemplates the loss of an entire site, the loss of a substantial fraction of cryptographic officers, and the loss of confidence in a vendor relationship. The discipline in this Part documents the procedures that recover from those scenarios.*

**Recovery objectives**

Per service category the platform commits to documented Recovery Time Objectives (RTO) and Recovery Point Objectives (RPO). Categories reflect the criticality of the underlying capability.

|  |  |  |  |
|----|----|----|----|
| **Service category** | **RTO** | **RPO** | **Notes** |
| Critical synchronous (ARMP KYC, BEAC banking) | 15 minutes | 0 | Active-active across both sites; cross-site replication synchronous |
| Standard synchronous (DGI on-demand, sectoral cadastres) | 1 hour | 5 minutes | Active-active or fast failover; replication asynchronous bounded |
| Asynchronous (DGI bulk, ANIF goAML, BODS exports) | 4 hours | 1 hour | Failover with backup-based recovery |
| Verification engine | 1 hour | 5 minutes | State replayable from Kafka |
| Audit channel | 4 hours | 0 | Synchronous Fabric replication; ledger is the canonical store |
| Cryptographic substrate | Site failover within 30 minutes; full recovery from ceremony-site within 24 hours | 0 | HSM partitions replicated across sites; ceremonial site provides last-resort recovery |

**Backup discipline per data store**

**PostgreSQL**

- Nightly pg_basebackup to MinIO with full encryption.

- Continuous WAL archiving to MinIO with five-minute granularity.

- Cross-site replication via streaming replication (synchronous for critical contexts, asynchronous for the rest).

- Quarterly restore test: a fresh database is restored from backup in pre-production and validated.

**Neo4j**

- Daily backup via neo4j-admin backup to MinIO.

- Causal cluster topology with secondary replicas at the other site.

- Quarterly restore test.

**OpenSearch**

- Snapshot to MinIO daily.

- Cross-cluster replication for the search indices.

- Index rebuilding from canonical Postgres on full loss is the documented worst-case recovery.

**Kafka**

- Topics with infinite retention (audit, declarations) are the canonical store and themselves require no separate backup.

- Topics with limited retention are reconstructable from PostgreSQL where they are derivatives.

- Cross-site replication via MirrorMaker 2 with offset translation.

**MinIO**

- Erasure coding within each site (EC:8 profile) provides per-site durability.

- Cross-site replication for the critical buckets (recor-evidence-restricted, recor-audit-archives, recor-backups).

- Quarterly restore test for representative buckets.

**HSM keys**

- HSM partitions replicated to the ceremonial site’s air-gapped HSM through documented key-replication ceremonies (semi-annual).

- Cryptographic officer escrows held in secured personal escrows for the quorum-reconstruction recovery scenario.

- Annual recovery rehearsal validates the ceremonial-site recovery procedure end-to-end.

**Recovery procedures**

**Single-site failure**

Loss of the Yaoundé site or the Douala site. The surviving site continues operation; cross-site replication provides the data continuity. The recovery sequence:

- Detect the failure through health probes; alerts trigger the standing SEV-1 response.

- Argo CD reconciles the surviving cluster to serve traffic at full capacity if it was previously serving at partial.

- DNS failover routes external traffic to the surviving site.

- Consumer integrations are notified of the failover through the status channel.

- The failed site is recovered through documented site-rebuild procedures; once recovered, replication resyncs and active-active resumes.

**Catastrophic data corruption**

A data corruption event affecting a primary store. The recovery uses the event log as the source of truth:

- PostgreSQL: restore from the most recent backup plus WAL replay to a point just before the corruption.

- Projections (Neo4j, OpenSearch, Redis): rebuild from the event log in Kafka.

- MinIO objects: restore from cross-site replication or from backup.

- Validation: reconciliation across stores must converge before resuming write traffic.

**Cryptographic substrate compromise**

A compromise of an HSM partition or of a cryptographic officer is the most severe scenario. The response:

- Immediate revocation of the compromised partition’s operational signing capability through the FROST coordinator’s reconfiguration.

- Re-ceremony to redistribute key shares across new officers and reconstituted partitions.

- Audit log review to identify any operations performed under the compromised key during the exposure window.

- Public communication coordinated by the consortium’s communications function.

**Ledger recovery from Kafka**

In the worst-case scenario of full Fabric ledger loss across both sites (highly unlikely given the replication topology), the ledger can be reconstructed from the Kafka audit topic. The audit topic’s infinite retention combined with each event’s cryptographic signature and Merkle anchoring provides the basis: replay the events into a fresh Fabric channel; the replayed channel’s state matches the lost channel’s state by construction; the Bitcoin anchors continue to attest to the events’ original timestamps.

The procedure has been rehearsed in pre-production at small scale; the full-scale rehearsal is part of the annual cryptographic-substrate recovery drill.

**Quarterly DR drill**

Each quarter the operations team executes one DR drill. The drill rotates across scenarios; over a year the major scenarios are all exercised. Findings from drills are addressed; the drill itself documents the recovery time achieved against the RTO.

Drill schedule:

- Q1: Site failover drill (Yaoundé site simulated failure).

- Q2: Database recovery drill (Postgres restore from backup with WAL replay).

- Q3: Cross-store reconciliation drill (corruption recovery scenario).

- Q4: Cryptographic substrate drill (HSM partition replacement under simulated compromise).

**Annual full-platform recovery drill**

Once a year the platform executes a full-platform recovery drill: rebuild the platform from backup into an isolated environment, validate end-to-end functionality, document any gaps. The annual drill is the platform’s deepest assurance that recovery actually works at the scale and complexity it would face in a real catastrophic event.

> **SUCCESS —** Disaster recovery is operationally sound when: every quarterly drill completes within the documented RTO for its scenario; the annual full-platform recovery succeeds end-to-end; backup-restore tests run continuously without intervention; the cryptographic substrate recovery is rehearsed and documented; the consortium has named-recovery roles in place with documented authorities to act in catastrophic scenarios.

**Sprint-by-Sprint Build Plan**

> *The build plan operationalises the SDLC phases (V1 P3) into eight-week Program Increments and two-week sprints. The plan is not a Gantt chart; it is a critical-path-with-dependencies map that the team executes against. The plan is updated at every PI boundary based on the prior PI’s actual outcomes.*

**Plan structure**

The build runs through twelve Program Increments (PI-1 through PI-12) over twenty-four months. Each PI is eight weeks. Each PI delivers a coherent set of capabilities; PI boundaries align with the SDLC phase gates documented in V1 P3.

**PI-1 through PI-2: Foundation (months 1–4)**

**PI-1 (sprints 1–4)**

- Sprint 1: Consortium establishment confirmed; engineering team’s key roles staffed; toolchain ratified; HSM order placed; GPU cluster order placed; sovereign data centre capacity confirmed.

- Sprint 2: Monorepo initialised with the documented top-level structure; CI/CD operational at minimal scope; first service skeleton (entity) committed and deployable to dev cluster.

- Sprint 3: Doctrine onboarding completed for the founding team; OPSEC training completed; Claude Code onboarding completed; first service (entity) under agent-assisted development with end-to-end workflow exercised.

- Sprint 4: Foundation observability operational (Prometheus, Grafana, Loki, Tempo, Alertmanager); first runbook produced; first ADR landed; identity and access services in initial deployment with Keycloak in dev environment.

**PI-2 (sprints 5–8)**

- Sprint 5: HSMs received and installed; initial key ceremony performed at the ceremonial site with documented ceremony report; FROST coordinator service in initial form.

- Sprint 6: Hyperledger Fabric 3.1.x network operational with all ten organisations’ peers; first chaincode deployed (declaration-anchor) with endorsement policies.

- Sprint 7: PostgreSQL HA topology operational; Neo4j cluster operational; OpenSearch operational; Kafka operational; MinIO operational. Foundation Layer 2 services (entity, person, declaration) in pre-production.

- Sprint 8: Phase II gate review: cryptographic substrate operational; data tier operational; CI/CD operational at full scope; SLSA L4 build provenance verified by independent rebuilder; first tabletop security exercise. Gate passed; project enters Phase III.

**PI-3 through PI-5: Verification Engine (months 5–11)**

**PI-3 (sprints 9–12)**

- Sprint 9: Verification service skeleton; verification-engine service skeleton; pipeline orchestrator with first two stages (schema validation, identity authentication).

- Sprint 10: Stage 3 (sanctions screening) with daily-refreshed feeds operational; Stage 4 (adverse-media screening) operational; Evidence service in pre-production.

- Sprint 11: Stage 5 (entity resolution) with the fuzzy-matching pipeline operational; OpenSearch transliteration analyzers tuned and validated against Cameroonian name corpus.

- Sprint 12: Stage 6 (pattern detection signatures 1–6) operational; tested against synthetic adversarial corpus.

**PI-4 (sprints 13–16)**

- Sprint 13: Stage 7 (AI-reasoning enrichment) operational with inference gateway routing to Tier B; prompts versioned and registered.

- Sprint 14: Stage 8 (cross-source triangulation) operational; first triangulation against ARMP procurement records and DGI taxpayer records.

- Sprint 15: Stage 9 (Dempster-Shafer fusion + lane decision) operational; lane thresholds initial calibration.

- Sprint 16: End-to-end verification pipeline integration testing; performance benchmarking; first quarterly inference audit framework operational.

**PI-5 (sprints 17–20)**

- Sprint 17: Declarant Portal v1 operational; ARMP-registered bidder cohort identified and onboarded for pilot.

- Sprint 18: Pilot operation begins; 200 ARMP-registered bidders file declarations under voluntary scheme.

- Sprint 19: Pilot operation continues; verification engine calibrated against pilot outcomes; lane thresholds adjusted with documented ADR.

- Sprint 20: Pilot evaluation completed; Phase III gate review. Verification engine operational with documented accuracy; pilot improvements incorporated. Gate passed; project enters Phase IV.

**PI-6 through PI-8: Consumer Integrations (months 11–16)**

**PI-6 (sprints 21–24)**

- Sprint 21: ARMP integration in pre-production with synchronous KYC and conflict-of-interest analysis; consumer-side acceptance testing begins.

- Sprint 22: ANIF goAML integration with bidirectional STR enrichment; ANIF consumer-side acceptance testing.

- Sprint 23: DGI integration (bulk and on-demand) operational; DGI acceptance testing.

- Sprint 24: BEAC banking KYC integration operational with initial bank consumers; bank acceptance testing.

**PI-7 (sprints 25–28)**

- Sprint 25: Customs ASYCUDA integration operational.

- Sprint 26: Sectoral cadastres integrations (mining, forestry, hydrocarbons) operational.

- Sprint 27: CONAC integration operational.

- Sprint 28: INTERPOL/StAR integration framework operational; first cross-border information-sharing exercise.

**PI-8 (sprints 29–32)**

- Sprint 29: Officer Console v1 operational; consumer-institution analysts trained.

- Sprint 30: All eight consumer integrations operating at negotiated SLOs.

- Sprint 31: Consumer SLO compliance measured across thirty consecutive days.

- Sprint 32: Phase IV gate review. All consumer integrations operational; SLO compliance demonstrated; legal-framework progression at second reading. Gate passed; project enters Phase V.

**PI-9 through PI-10: Applications and ML maturity (months 17–20)**

**PI-9 (sprints 33–36)**

- Sprint 33: Investigation Workbench v1 operational; ANIF, CONAC, TCS investigators trained.

- Sprint 34: Public Portal v1 operational; communications strategy approved; civil society partner engagements scheduled.

- Sprint 35: Whistleblower Intake operational as Tor service; protected investigator team trained.

- Sprint 36: Administrative Console v1 operational; governance workflows tested.

**PI-10 (sprints 37–40)**

- Sprint 37: Supervised pattern detection classifier (Signature 7) trained on accumulated Phase III/IV data.

- Sprint 38: Classifier deployed to production after quarterly inference audit approval; community detection (Signature 8) deployed.

- Sprint 39: Pre-launch security audit by independent firm; findings remediated.

- Sprint 40: Pre-launch accessibility audit; findings remediated. Phase V gate review. Gate passed; project enters Phase VI.

**PI-11 through PI-12: Launch and operations (months 21–24)**

**PI-11 (sprints 41–44)**

- Sprint 41: Mandatory rollout sequence begins per the documented entity-class schedule; large extractive entities first.

- Sprint 42: BODS export consumed by Open Ownership; first international consumption demonstrated.

- Sprint 43: ISO 27001 certification engagement initiated.

- Sprint 44: First post-launch security audit; full disaster recovery drill executed in production-equivalent environment.

**PI-12 (sprints 45–48)**

- Sprint 45: Mandatory rollout extended to medium-sized entities per schedule.

- Sprint 46: Steady-state operations model engaged; operations function takes over from build teams with documented handover.

- Sprint 47: First quarterly inference audit under operational conditions completed.

- Sprint 48: Phase VI exit gate review. Mandatory declaration in effect for all targeted entity classes; build complete; operations operating; consortium’s evolutionary function takes over.

**Critical path dependencies**

The critical path through the build plan runs through the cryptographic substrate (Phase II) into the verification engine (Phase III), with consumer integrations and applications gated on the verification engine being operational. Specific dependencies:

- Verification engine cannot proceed past Stage 7 without inference gateway operational; inference gateway depends on Tier B Bedrock PrivateLink procurement and Tier C sovereign GPU deployment.

- Consumer integrations cannot begin acceptance testing without the verification engine producing lane decisions; the pilot in PI-5 is the gate.

- Mandatory rollout in Phase VI is gated on legal-framework promulgation; the framework progresses on its own legislative timeline; engineering delivery is paced to match.

- Supervised classifier (Signature 7) cannot be trained until Phase III/IV data is sufficient; the training is gated on data accumulation, not on engineering capability.

**Plan adaptation**

The plan above is the baseline. At every PI boundary the team reviews actual outcomes against planned outcomes and adjusts. The discipline is rolling-wave: PI-N+1’s plan is firm; PI-N+2 through PI-N+4 are indicative; PI-N+5 and beyond are direction. The plan does not survive contact with operational reality unchanged; the documented baseline preserves the strategic intent against which adaptations are evaluated.

> **SUCCESS —** The build plan is operationally effective when: every PI boundary closes with the planned outcomes substantially delivered; deviations are documented and approved through the change procedure; the critical path proceeds without compromise; the team’s velocity is sustainable across the twenty-four-month horizon; the project lands on schedule at the Phase VI exit gate with full launch capability.

**Test Strategy**

> *Tests are how the platform proves to itself that it works. The test strategy is layered: many fast tests at the unit level, fewer integration tests against ephemeral environments, sparing end-to-end tests that exercise critical user journeys. Beyond the standard pyramid, the platform invests in property tests, fuzz tests, mutation tests, and chaos tests where the stakes warrant the investment.*

**Test taxonomy**

Eight test categories are practised across the platform. Each category has a defined purpose, location, runtime, and CI integration.

|  |  |
|----|----|
| **Category** | **Purpose, location, and CI integration** |
| Unit tests | Verify pure logic in isolation. Located alongside the code in each service. Run on every commit; under 5 minutes for the full unit suite per service. Coverage threshold per V6 P25. |
| Integration tests | Verify cross-component behaviour against real dependencies (real Postgres, real Redis, real Kafka) via testcontainers. Located in tests/ per service. Run on every pull request; under 15 minutes per service’s suite. |
| Contract tests | Verify producer-consumer contract conformance via Pact. Producer side runs the contract against the service’s implementation; consumer side runs against the producer’s mock. Located in tests/contract/. Run on every pull request to either side. |
| End-to-end tests | Verify critical user journeys end-to-end across the deployed platform. Located in tests/e2e/. Run nightly against staging; selected smoke subset runs on every pre-production promotion. |
| Property tests | Verify invariants hold across the space of inputs. Located alongside the related unit/integration tests; marked with the property framework’s annotation. Run on every commit; expanded iteration counts run nightly. |
| Fuzz tests | Verify robustness of parsers and protocol implementations against adversarial inputs. Located in fuzz/ per service. Run continuously on dedicated fuzz infrastructure; findings surface as security tickets. |
| Mutation tests | Verify that the test suite’s assertions are meaningful (mutated code fails tests). Located in tests/mutation/. Run weekly; mutation score targets per service documented. |
| Chaos tests | Verify the platform’s resilience to infrastructure faults. Located in tests/chaos/. Run weekly in staging; quarterly game days in pre-production (V6 P28). |

**Per-layer test ratios**

The platform’s test pyramid is layered with explicit ratios per service category. Ratios are guidance, not strict mandate; the QA function reviews adherence quarterly.

|  |  |  |  |  |
|----|----|----|----|----|
| **Service category** | **Unit** | **Integration** | **E2E** | **Special emphasis** |
| Layer 0 (cryptographic substrate) | 70% | 20% | 10% | Heavy property testing; mutation score target 80% |
| Layer 2 (domain services) | 60% | 30% | 10% | Contract tests with downstream consumers |
| Layer 3 (verification engine) | 50% | 30% | 10% | Adversarial corpus tests; weekly mutation runs |
| Layer 4 (APIs) | 50% | 30% | 20% | Contract tests with consumer integrations; fuzz tests on parsers |
| Layer 5 (consumer integrations) | 40% | 40% | 20% | Contract tests against consumer mocks dominate |
| Layer 6 (applications) | 50% | 30% | 20% | Accessibility and visual regression tests |

**Property testing emphasis**

Property tests carry disproportionate weight in the platform’s test strategy because they catch the class of bugs that example-based tests miss. Property tests are explicitly required for:

- Cryptographic primitives — sign/verify roundtrip, encrypt/decrypt roundtrip, hash determinism, FROST signing protocol invariants.

- Dempster–Shafer fusion — monotonicity of belief under additional concurring evidence, commutativity of evidence combination, idempotency under reordering.

- Pipeline orchestration — idempotency of pipeline runs on the same declaration, monotonicity of stage outcomes (a stage cannot reduce belief in itself across re-runs).

- Outbox dispatcher — at-least-once delivery, no duplicate publication beyond consumer-side deduplication window.

- Conflict-resolution semantics — last-write-wins under version vectors produces the same outcome regardless of network ordering.

**Adversarial corpus**

The verification engine carries an adversarial test corpus: real or anonymised concealment patterns documented from international cases (Panama Papers, Paradise Papers, Suisse Secrets), from regional cases produced by ANIF and CONAC, and from the platform’s pilot operation. Each case in the corpus is documented with its expected lane outcome and is included in the verification engine’s regression test suite.

Cases are added through a documented procedure: the case is documented from a published or platform-internal source; the case represents a real concealment pattern, not a theoretical one; the case’s expected lane outcome is documented; the case is reviewed quarterly to detect drift; the case is anonymised where the underlying data is restricted.

**Test data governance**

Test data is synthetic, not derived from production. The recor-test-data-generator (Python) produces declarations, entities, persons, ownership chains with declared statistical distributions matching the production data’s shape. Anonymisation of production data is reserved for pre-production environments under documented approval; full PII never appears in dev or staging.

Synthetic data generation is itself versioned. Changes to the generator that affect the distributions trigger re-baselining of tests that depend on those distributions. Engineers do not modify the generator without QA review.

**Performance regression testing**

Performance regression tests run at PI boundaries using k6 with the project’s scenarios. Scenarios cover: declaration submission load, KYC lookup load, verification engine throughput, BODS export generation time, Investigation Workbench graph query latency. Regression beyond documented thresholds blocks the next PI from commencing.

**Test infrastructure**

Test infrastructure is itself infrastructure code. The testcontainers-managed dependencies run on the CI runners with appropriate resource sizing; the staging environment for end-to-end and performance tests is a production-equivalent topology in the staging cluster. Test fixtures and mock surfaces are version-controlled alongside the code they test.

**Testing the agents themselves**

The Claude Code specialist agents are tested through their Outcomes rubrics (V2 P5). The grading agent’s evaluation of each agent’s output across representative test scenarios is the agent’s test suite. Test scenarios for each agent are version-controlled in .claude/agents/\<name\>.tests.md; the recor-doctrine-check skill triggers agent regression tests when an agent definition changes.

> **SUCCESS —** The test strategy is operationally sound when: every service meets its category’s test-ratio targets; the property tests prevent the class of bugs that example-based tests would miss; the adversarial corpus catches at least 95% of its documented patterns at the verification engine’s current threshold settings; the mutation score across the cryptographic substrate exceeds 80%; performance regression tests run cleanly at every PI boundary; the test infrastructure is reproducible and self-healing.
