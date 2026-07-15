# Agent Workflow

Agents use this workflow for repository changes:

```text
Inspect -> Read guidance -> Establish scope -> Decide branch -> Design smallest change -> Implement -> Test -> Update docs -> Run verification -> Review diff -> Commit -> Merge when required -> Delete merged branches -> Report evidence
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

For nontrivial implementation work, use parallel agent teams when independent
work slices or review lanes exist. Give each agent clear ownership, avoid
overlapping writes, and preserve user or agent changes already present in the
worktree.

Before accepting agent-team output into the main session, inspect the changed
logic, run the relevant checks, and resolve conflicts by understanding the code
instead of mechanically choosing one side.

Current v0.1 implementation planning is tracked in
`docs/plans/v0.1-parallel-development-plan.md` and
`docs/plans/python-v0.1-implementation-plan.md`. Use those plans to choose
phase scope, ownership lanes, validation gates, and commit boundaries. Update
the plans and matching `.agents/memories/` files whenever phase scope, Python
v0.1 analysis, optional CodeGraph provider integration, or `UNKNOWN` policy
changes.

## Verification

Run formatting, clippy with warnings denied, tests, repository guard, guide
equality, and whitespace checks. Do not weaken a failing check.

## Commit and report

Stage explicit paths, review `git diff --cached --stat` and `git diff --cached`,
then commit with a Conventional Commit message. After a branch is merged into
`main`, delete the merged or superseded branch only after verifying the work is
contained by `main`, as described in `branching-and-commits.md`. The final
report must include branch, commit hash, branch cleanup performed or still
pending, documentation changed, commands run, and remaining risks.
