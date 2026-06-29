# Branching and Commits

## Branch names

Use descriptive branches:

- `feat/<short-slug>`
- `fix/<short-slug>`
- `refactor/<short-slug>`
- `chore/<short-slug>`

## Major feature definition

A major feature includes a new user-visible capability, public API change, MCP
contract change, database schema change, module-boundary change, new language or
framework adapter, structural pattern-mining change, important production
dependency, data migration, or cross-subsystem change.

## Atomic commits

Each commit must express one logical purpose and include code, tests, and
relevant docs. Do not mix unrelated formatting, dependency upgrades, temporary
logs, or generated caches.

## Conventional Commits

Use messages such as:

```text
chore(repo): initialize architecture and agent governance
feat(index): add normalized code-unit extraction
fix(store): preserve index revision on rollback
docs(mcp): define conformance response contract
```

## Attribution

Automated agents must not add themselves, model or provider identities, tool
accounts, or AI vendors as authors, committers, co-authors, signed-off-by
identities, or any other contributor attribution. Agent-made commits must use
only the maintainer-configured author and committer identity, with no agent
attribution trailers.

## Merge conditions

A major-feature branch can merge into `main` only after all required checks
pass, guide equality is verified, and the branch diff is reviewed. Use a
non-fast-forward merge unless a maintainer explicitly chooses another policy.
Do not push unless explicitly authorized.

## Multi-agent work

Parallel agents use separate branches or worktrees and avoid overlapping file
ownership. Conflicts must be resolved by understanding semantics, not by
mechanically choosing one side.
