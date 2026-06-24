---
name: major-feature-workflow
description: Use when a change adds user-visible capability, changes public API, MCP contract, storage schema, module boundaries, indexing pipeline, or important dependencies.
---

# Purpose

Keep large changes reviewable and mergeable.

# Trigger conditions

Use for new CLI commands, MCP tools, database schema changes, language or
framework adapters, major indexing changes, public API changes, or
cross-subsystem features.

# Required reading

- `docs/development/branching-and-commits.md`
- `docs/development/agent-workflow.md`
- Relevant ADRs and specifications

# Preconditions

- `main` is clean or unrelated changes are explicitly protected.
- The feature has a clear scope and success criteria.

# Step-by-step procedure

```text
main
-> create dedicated branch
-> implement atomic commits
-> run full verification
-> update docs and ADRs
-> review diff
-> merge --no-ff into main when checks pass
```

# Required verification

Run all standard checks before merge and again on `main` after merge.

# Documentation updates

Update specifications, architecture docs, ADRs, README, CHANGELOG, skills, and
memories as appropriate to the feature.

# Commit requirements

Use one or more independently coherent Conventional Commits.

# Completion report

Report feature branch, commit hashes, merge commit, verification results, and
remaining risks.

# Failure and rollback handling

Do not merge a failing branch. Do not force-push or rewrite shared history
without explicit maintainer approval.
