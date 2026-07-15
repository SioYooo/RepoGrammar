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

## Post-merge branch cleanup

After `main` contains the merged work, delete the merged branch pointer so stale
stacked branches do not look like remaining work. In a protected-main or
PR-only workflow, this means deleting the pull request head branch after the PR
merge is visible on `origin/main`. For integration stacks, also delete
superseded intermediate branches once patch-equivalence or containment proves
that their work is already in `main`.

Do not delete a branch solely because its name looks old or because
`git branch --no-merged` reports it. Squash merges, rewritten integration
commits, and stacked branches can leave misleading ancestry. Verify with the PR
merge state plus commands such as `git cherry -v`, `git diff`, `git merge-base`,
or equivalent GitHub branch-containment evidence before deleting local or remote
branches.

## Multi-agent work

Parallel agents use separate branches or worktrees and avoid overlapping file
ownership. Conflicts must be resolved by understanding semantics, not by
mechanically choosing one side.
