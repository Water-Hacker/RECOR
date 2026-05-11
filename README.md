# RÉCOR

**Registre de l'Effective Contrôle et Origine Réelle**
National Beneficial Ownership Registry of Cameroon

This is the canonical source repository for RÉCOR — the sovereign-grade beneficial
ownership registry of the Republic of Cameroon, operated by a consortium of ten
institutions and built under the Software Architecture Document referenced below.

## Authoritative documents

Three documents govern this codebase. Read in order:

1. **Concept Note** — the project's strategic rationale; for funder and political audiences
   → `docs/concept-note/RECOR-Concept-Note.docx`

2. **Software Architecture Document** — what the system is, how it is engineered
   → `docs/architecture/RECOR-Software-Architecture-Document.docx`

3. **Implementation Companion** — paste-and-go artefacts the team actually uses
   → `docs/companion/RECOR-Implementation-Companion.docx`

## Quick start

```bash
# 1. Clone with submodules
git clone --recurse-submodules https://gitea.recor.cm/recor/recor.git
cd recor

# 2. Install toolchains via mise
curl https://mise.run | sh
mise install

# 3. Bootstrap the development environment
just bootstrap

# 4. Verify
cd services/entity && just check
```

If `just check` passes you have a working environment.

## Repository layout

See the Architecture Document V4 P10 for the canonical layout.
Key directories: `services/`, `applications/`, `libraries/`, `contracts/`,
`infrastructure/`, `policies/`, `docs/`.

## Engineering doctrines

The 24 strict doctrines in Architecture V1 P2 are binding on every contribution.
The first reading for any new engineer is the doctrines.

## Claude Code

This repository is built primarily through Claude Code agents on Opus 4.7.
Read `.claude/README.md` and the Companion V2 sections before initiating
agent-assisted work.

## Contributing

See `CONTRIBUTING.md`. Note: this is a sovereign infrastructure project. External
contributions are accepted only through the consortium's documented contribution
process.

## Security

Vulnerability disclosure: `SECURITY.md` or https://recor.cm/.well-known/security.txt

## Licence

The source code in this repository is the property of the RÉCOR Consortium.
Portions distributed under Apache-2.0 are marked accordingly; the default is
Restricted distribution under consortium licence terms.
