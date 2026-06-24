---
name: implement-change
description: Use for every code change in RepoGrammar; do not use for documentation-only edits that do not touch behavior or repository automation.
---

# Purpose

Provide the default implementation workflow for scoped code changes.

# Trigger conditions

Use when editing `src/`, changing CLI behavior, adding tests, changing
repository automation, or touching product runtime boundaries.

# Required reading

- `AGENTS.md`
- `docs/README.md`
- `docs/architecture/module-map.md`
- `docs/development/documentation-policy.md`
- Canonical specification for the touched module

# Preconditions

- Run `git status --short --branch`.
- Preserve unrelated user changes.
- Decide whether the change is a major feature requiring a branch.

# Step-by-step procedure

1. Establish the smallest coherent scope.
2. Read current implementation and related docs.
3. Add or update tests before claiming behavior.
4. Implement within the documented module boundary.
5. Update relevant docs in the same commit.
6. Run full verification.
7. Stage explicit paths and review the staged diff.
8. Commit with a Conventional Commit message.

# Required verification

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo run --quiet --bin repo-guard -- check
git diff --check
```

# Documentation updates

Use the update matrix in `docs/development/documentation-policy.md`. Do not use
`CHANGELOG.md` as a substitute for the canonical document.

# Commit requirements

Use explicit path staging. Review `git diff --cached --stat` and
`git diff --cached` before committing.

# Completion report

Report branch, commit hash, changed docs, commands run, and remaining risks.

# Failure and rollback handling

Do not disable checks. If a change cannot pass, leave it uncommitted and report
the exact blocker.
