---
name: recor-doctrine-check
description: First line of doctrine enforcement. Loads the doctrines relevant to the current work and reminds the operator of the doctrine that applies. Fires automatically when the work being undertaken matches doctrine-relevant criteria (new code, new test, new endpoint, new migration, new event, new prompt, new policy, refactor, dependency change). Effectively always-on during substantive work.
---

# RÉCOR doctrine check

You are working in the RÉCOR repository, which is governed by 24 strict
engineering doctrines (Architecture V1 P2). The doctrines are not aspirational
guidelines; they are binding on every contribution.

## When this skill fires

This skill fires whenever the work intent indicates substantive code,
infrastructure, or policy change. The doctrines below are loaded into context
for reference during planning and implementation.

## The 24 doctrines (brief summary)

1. **Completeness over partial delivery** — The deliverable includes
   implementation, tests, documentation, observability surfaces, and any
   operational artefacts. Partial delivery is not delivery.

2. **Plan before writing code** — Substantive work begins with Plan Mode
   (Shift+Tab × 2). The plan surfaces decisions the human reviewer must
   confirm.

3. **Search before building** — Check whether the capability already exists
   in /libraries/ or as a shared platform service. Duplication is rejected.

4. **Tests are part of the feature** — Same PR. Test ratios per layer in
   recor-test-pyramid skill.

5. **Documentation is part of the feature** — Same PR. Inline docs for
   public APIs, README updates if the surface changes, CLAUDE.md updates
   if the service's operational behaviour changes.

6. **The complete answer, not the plan to build it** — Once approved to
   execute, the work is done end-to-end.

7. **No workarounds where the real fix exists** — If the right fix is more
   work, do the right fix.

8. **No dangling threads** — Close TODOs, delete commented-out code, complete
   in-progress refactors.

9. **Holy shit, that's done** — The delivery is impressive, not adequate.

10. **Reviewability over speed of merge** — PRs under 500 lines; larger PRs
    are decomposed.

11. **Two reviewers, at least one cross-team** — Approval requires reading,
    not rubber-stamping.

12. **Production-grade from the first commit** — There is no "we'll harden
    this later" phase.

13. **Idempotency on every state-changing operation** — Idempotency key on
    every mutation; replay-safe behaviour.

14. **Fail closed at integration boundaries** — Refuse rather than guess.

15. **Cryptographic provenance on every consequential event** — Audit channel
    integration is non-optional.

16. **Observability is non-optional** — Metrics, traces, logs, dashboards
    are part of the feature.

17. **Zero trust at every network boundary** — mTLS everywhere; SPIFFE
    workload identities.

18. **No secrets in code, tickets, chat, logs** — Secrets go through Vault
    and are surfaced to workloads via projected service account tokens.

19. **Reproducible everything** — Bytewise-identical builds from sources.

20. **Supply chain integrity, SLSA Level 4** — Provenance attestation for
    every artefact.

21. **Post-quantum agility** — Cryptographic substrate supports ML-KEM-1024
    migration when triggered.

22. **Anthropic-primary AI inference** — Routing per V5 P18.

23. **Plan Mode is the default** — Implementation Mode is exited deliberately
    after plan approval.

24. **The standard is non-negotiable; the path is negotiable** — Time,
    fatigue, complexity are not excuses to violate the standard.

## Quick reference for the current work

Look at the work being done. If it is:

- **New code**: doctrines 1, 4, 5, 12, 16, 23 always apply
- **State-changing endpoint**: add doctrine 13 (idempotency)
- **Integration with another service**: add doctrine 14 (fail-closed)
- **Consequential event**: add doctrine 15 (provenance)
- **Network communication**: add doctrine 17 (zero trust)
- **Anything touching secrets**: add doctrine 18
- **CI / build / deployment**: add doctrines 19, 20
- **AI inference**: add doctrine 22
- **Refactor**: add doctrine 10 (PR size); often doctrine 7 (no workarounds)

Read the full doctrine text in Architecture V1 P2 for substantive work.
