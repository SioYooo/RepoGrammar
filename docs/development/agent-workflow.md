# Agent Workflow

Agents use this workflow for repository changes:

```text
Inspect -> Read guidance -> Establish scope -> Decide branch -> Design smallest change -> Implement -> Test -> Update docs -> Run verification -> Review diff -> Commit -> Merge when required -> Report evidence
```

## Start a task

Run `git status --short --branch` and inspect relevant history. Preserve
unrelated user changes and generated caches. Read `docs/README.md`, the relevant
skill, memories, and canonical docs for the touched area.

## Branch decision

Use `main` for small bootstrap or maintenance changes in an empty repository.
Use a dedicated branch for major features as defined in
`branching-and-commits.md`.

## Implementation

Make the smallest coherent change. Keep source code under `src/`, keep
third-party parser/storage/transport types at adapter boundaries, and update
tests with code changes.

## Verification

Run formatting, clippy with warnings denied, tests, repository guard, guide
equality, and whitespace checks. Do not weaken a failing check.

## Commit and report

Stage explicit paths, review `git diff --cached --stat` and `git diff --cached`,
then commit with a Conventional Commit message. The final report must include
branch, commit hash, documentation changed, commands run, and remaining risks.
