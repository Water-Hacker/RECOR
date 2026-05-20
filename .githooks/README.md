# RÉCOR git hooks

Project-tracked git hooks that mirror what CI runs. Install once
per clone:

```bash
git config core.hooksPath .githooks
```

## Hooks

- `pre-commit` — runs gitleaks on staged changes (FIND-012-tier
  audit closure: gitleaks was already in CI but not at the
  developer's workstation, so credentials could be detected only
  AFTER landing in the remote).

## Why not pre-commit framework / husky

Husky / pre-commit need extra dependencies. A vanilla `.githooks/`
directory works with stock git and doesn't add to the toolchain
surface.
