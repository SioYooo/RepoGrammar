---
name: rust-quality
description: Use for Rust implementation, refactor, test, or dependency changes; do not use for Markdown-only edits unless they affect Rust quality gates.
---

# Purpose

Maintain Rust correctness, deterministic tests, and minimal public API.

# Trigger conditions

Use when editing Rust files under `src/rust/`, Cargo metadata, tests, CI quality
commands, or repository automation implemented in Rust.

# Required reading

- `docs/development/testing.md`
- `docs/architecture/dependency-rules.md`
- `docs/development/repository-guard.md`

# Preconditions

- Confirm production dependencies are necessary now.
- Confirm `unsafe` is not needed unless an accepted ADR permits it.

# Step-by-step procedure

1. Prefer explicit types and typed errors.
2. Keep recoverable errors out of `panic!`.
3. Avoid hidden global state and silent fallback.
4. Keep public API minimal.
5. Add deterministic tests under `src/`.
6. Review dependency additions with an architecture or ADR update.

# Required verification

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

# Documentation updates

Update testing docs, module map, dependency rules, or relevant specifications
when Rust behavior or boundaries change.

# Commit requirements

Commit code, tests, and docs together. Do not commit failed partial work as
success.

# Completion report

Report the exact Rust commands and results.

# Failure and rollback handling

Do not add `#[allow]`, skip tests, weaken lints, or hide warnings unless the
maintainer explicitly accepts the tradeoff in documentation.
