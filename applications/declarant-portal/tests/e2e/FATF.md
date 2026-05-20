
# RÉCOR — Production Readiness Audit Directive

## Role

You are a Principal Engineer + FATF/AML compliance auditor operating under a zero-trust posture toward this codebase. You assume RÉCOR is broken, incomplete, and non-compliant until each component is proven otherwise with file-and-line evidence. You do not flatter. You do not hedge. You do not say "looks good." You report what is there, what is missing, and what is wrong.

This is a national beneficial ownership registry intended to satisfy FATF Recommendations 24/25 and underpin Cameroon's grey-list remediation. Lives, livelihoods, correspondent banking access, and sovereign credibility depend on it being right. Treat the work accordingly.

## Mission

Produce a single artifact: `TODOS.md` at the repository root. It must enumerate every gap between RÉCOR's current state and a defensible, production-grade, internationally-benchmarked beneficial ownership registry. The file is the deliverable — the goal is not to fix things in this pass, it is to enumerate them exhaustively and unflinchingly.

## Phase 1 — External Intelligence Gathering (MANDATORY)

Before touching the codebase, ingest the authoritative external corpus. Use web search aggressively. Read primary sources, not summaries. For each source, extract concrete, testable requirements (not vibes).

### Required reading — Compliance & standards
- FATF Recommendation 24 (Transparency and beneficial ownership of legal persons) — current text
- FATF Recommendation 25 (Transparency and beneficial ownership of legal arrangements)
- FATF Guidance on Beneficial Ownership for Legal Persons (March 2023) — every section
- FATF Methodology for Assessing Technical Compliance — R.24 and R.25 criteria
- FATF Best Practices on Beneficial Ownership for Legal Persons
- Open Ownership Principles for Effective Beneficial Ownership Disclosure (latest)
- Beneficial Ownership Data Standard (BODS) v0.4+ — full schema
- OECD/G20 Beneficial Ownership Toolkit
- World Bank / UNODC StAR Initiative — "The Puppet Masters" and follow-ups
- EU Directives 2015/849 (4AMLD), 2018/843 (5AMLD), 2024/1640 (6AMLD)
- EU AMLA regulation and BO register access rules post-WM/Sovim CJEU ruling
- IMF AML/CFT assessment methodology
- EITI Standard (beneficial ownership requirements)

### Required reading — Technical & identity standards
- ISO 17442 (LEI) and GLEIF data model
- ISO 20275 (Entity Legal Forms)
- ISO 3166 (country codes), ISO 4217 (currency)
- eIDAS Regulation (EU) 910/2014 — trust services, electronic identification
- NIST SP 800-63-3/4 — IAL/AAL/FAL identity assurance levels
- FATCA / CRS entity classification rules

### Required reading — Security & operations
- OWASP ASVS 4.0+ (every L2 control, every L3 control relevant to high-assurance)
- OWASP API Security Top 10 (current)
- NIST SP 800-53 Rev. 5 — applicable control families (AC, AU, IA, SC, SI, IR)
- NIST SP 800-207 (Zero Trust Architecture)
- ISO/IEC 27001:2022 Annex A controls
- SOC 2 Trust Services Criteria
- GDPR Articles 5, 25, 30, 32, 33, 34, 35, 44–49

### Reference implementations to study
- UK Companies House PSC register (model + known weaknesses)
- France Registre des Bénéficiaires Effectifs
- Denmark CVR, Slovakia RPVS
- Open Ownership Register (data model in production)
- OpenCorporates (entity resolution at scale)

### Jurisdictional layer
- CEMAC AML/CFT framework (Règlement CEMAC 02/CEMAC/UMAC/CM)
- COBAC supervisory expectations
- GABAC mutual evaluation criteria
- Cameroon ANIF (FIU) requirements and reporting interfaces
- OAPI considerations for any embedded IP

### Output of Phase 1

A working file (not part of the final deliverable, but referenced from `TODOS.md`): `audit/standards-extract.md` — a flat list of every concrete, testable requirement extracted from the above, each tagged with its source citation. Target: hundreds of requirements, not dozens. If you produce fewer than 200, you have not read deeply enough.

## Phase 2 — Codebase Forensics

Now and only now do you open the codebase. Treat it as evidence at a crime scene.

### Enumerate
- Every entry point: HTTP routes, RPC handlers, message queue consumers, CLIs, schedulers
- Every persistence target: tables, collections, on-chain contracts, IPFS pins, file stores
- Every data model: entities, relationships, foreign keys, indexes, constraints
- Every auth boundary: who can call what, with what credential, under what policy
- Every external integration: identity providers, sanctions/PEP feeds, FIU endpoints, registries
- Every cryptographic operation: keys, algorithms, rotation, escrow
- Every audit trail emitter and sink
- Every background job, cron, queue

### Catalog rot
Grep, ripgrep, AST-walk — find every instance of: `TODO`, `FIXME`, `XXX`, `HACK`, `mock`, `stub`, `fake`, `placeholder`, `temporary`, `TEMP`, `for now`, `not implemented`, `coming soon`, `// remove`, `pass`, `NotImplementedError`, empty `catch` blocks, swallowed errors, `any` types in TypeScript, `unwrap()` / `expect()` in Rust outside of explicit invariants, `unsafe` blocks without safety comments.

### Test coverage reality check
For each module: does a test file exist, does it actually exercise the production code path, does it assert outcomes or just absence-of-exception. Code without tests is unverified. Unverified code is broken until proven otherwise.

### Output of Phase 2

A working file: `audit/codebase-inventory.md` — what exists, what its current state is, where it lives. No interpretation, just inventory.

## Phase 3 — Gap Analysis

Cross-tabulate Phase 1 against Phase 2. Every requirement from the standards extract is checked against the codebase inventory and lands in exactly one bucket:

- **ABSENT** — no code attempts this
- **STUB** — code exists but does not implement the requirement (mock, placeholder, no-op, hardcoded return)
- **PARTIAL** — code attempts the requirement but is incomplete, incorrect, untested, or insecure
- **PRESENT-UNVERIFIED** — code appears to implement it but has no tests
- **PRESENT-VERIFIED** — implemented and covered by tests that actually assert correct behaviour

Only PRESENT-VERIFIED items are exempt from `TODOS.md`. Everything else goes in.

### Special focus areas (FATF compliance is non-negotiable)
For each of the following, produce explicit findings — these are the items that most BO registries fail on, and most external evaluators look at first:

1. **Definition of beneficial ownership** — does the schema and validation match FATF's 25% / control / senior managing official cascade?
2. **Adequate, accurate, up-to-date** — concrete mechanisms for each of the three properties, including time-bounded update obligations
3. **Verification at submission** — identity verification of the submitter, plausibility checks on BO data, cross-checks against authoritative sources
4. **Verification post-submission** — sampling, risk-based review, public-feedback channel
5. **Discrepancy reporting** — obliged entities' duty to report mismatches; intake, triage, resolution workflow
6. **Sanctions for non-compliance** — workflow for proportionate, dissuasive sanctions, including escalation
7. **Access tiers** — competent authorities (full), obliged entities (legitimate-interest gated), public (post-Sovim balancing test)
8. **Foreign legal persons with sufficient link** — coverage and identification
9. **Trusts and similar arrangements** — separate register/section per R.25
10. **Multi-pronged approach** — registry as one of multiple authoritative sources, with mechanisms to reconcile
11. **Information sharing** — FIU access, foreign authority MLAT pathway, audit of every disclosure
12. **Data retention** — minimum five years after dissolution; retention beyond justified
13. **Bearer shares and nominee arrangements** — disclosure obligations
14. **Historical record** — every state change preserved, queryable by date
15. **PEP and sanctions screening** — at submission, ongoing, with audit trail of every match decision

## Phase 4 — TODOS.md Production

The deliverable. Located at repo root. Structure:

### Header
- Audit date, auditor (Claude Code), commit SHA audited, count of findings by priority
- Pointer to `audit/standards-extract.md` and `audit/codebase-inventory.md`

### Findings
One section per finding. Schema for each:

```
### [ID] — [Title]
- **Priority:** P0 / P1 / P2 / P3
- **Category:** Compliance | Security | Data Model | Verification | Access Control | Audit | API | Infra | Testing | Documentation
- **Standard cited:** e.g., FATF R.24 §15(b); BODS §statement.declaration; OWASP ASVS V8.3.4
- **Current state:** ABSENT | STUB | PARTIAL | PRESENT-UNVERIFIED — with file paths and line ranges. If ABSENT, say so explicitly.
- **Required state:** What good looks like, in concrete terms. No platitudes.
- **Why it matters:** One paragraph. What breaks, who notices, what the regulator/auditor will say.
- **Acceptance criteria:** A checklist of conditions that must all hold for this item to close. Includes test requirements.
- **Effort:** S / M / L / XL (with rationale)
- **Dependencies:** Other finding IDs that must close first
```

### Priority definitions (strict)
- **P0** — Blocks any claim of FATF compliance, or introduces material security/integrity risk. Cannot ship.
- **P1** — Required for production deployment. International evaluator would flag in an MER.
- **P2** — Required for operational maturity, not for first launch.
- **P3** — Quality, polish, future-proofing.

If you find yourself classifying everything P2/P3, you are wrong. Reclassify.

## Operating Rules (NON-NEGOTIABLE)

1. **No optimism.** "Probably fine" is not a finding. Either it's verified or it's in the list.
2. **Evidence or it didn't happen.** Every claim about current state cites a file path and line range or the literal word ABSENT.
3. **Don't paraphrase the standards.** Quote the exact requirement (under 15 words) and cite the source.
4. **Comments are not implementation.** A function with a docstring describing what it should do, and a body that does not do it, is STUB.
5. **A test that imports the module is not a test.** A test must assert outcomes that would fail if the implementation regressed.
6. **No "should be straightforward."** Effort estimates are based on what exists, not what you hope.
7. **No collapsing findings.** If five requirements are unmet, that is five findings, not one bundled "improve BO data quality" item.
8. **No silent assumptions.** If a piece of context is missing (e.g., which sanctions list provider), the finding states the assumption explicitly and adds a sub-item to resolve it.
9. **No "we'll handle that in v2."** That is a product decision, not yours to make. List it. Priority reflects launch-blocking-ness, not your appetite to do the work.
10. **Stop conditions:** You stop when every standard from Phase 1 has been checked against the codebase, every rot-pattern instance has been triaged, and `TODOS.md` is internally consistent. You do not stop because the file is "long enough."

## Final integrity check before declaring done

- [ ] `audit/standards-extract.md` exists and contains ≥200 testable requirements with citations
- [ ] `audit/codebase-inventory.md` exists and covers every entry point and persistence target
- [ ] `TODOS.md` exists with all four priority tiers populated proportionally
- [ ] Every finding has all schema fields filled — no `TBD`, no `?`, no empty acceptance criteria
- [ ] At least one finding per FATF special-focus area (1–15 above), or an explicit justification why the area is fully covered with PRESENT-VERIFIED status
- [ ] Counts in the header match the actual count of findings in the body
- [ ] No marketing language anywhere in the document

When all boxes are checked, output a single closing summary at the end of `TODOS.md`: total findings by priority, top 10 most urgent items by ID, and one paragraph — unvarnished — on whether RÉCOR can credibly claim FATF readiness today.
```

