# Repository Agent Contract

## Authority

- This file and its mirror are the repository's compact, mandatory agent contract. They must remain byte-for-byte identical.
- Read `docs/README.md`, then the relevant files under `.agents/skills/`, `.agents/memories/`, and `docs/` before changing code.
- After editing either mirrored guide, immediately run `cargo run --quiet --bin repo-guard -- sync-agent-guides --from <edited-file>`.
- Do not create nested `AGENTS.md`, `CLAUDE.md`, override files, or competing instruction files without explicit maintainer approval.

## Repository Boundaries

- Put all source, executable, test, benchmark, migration-tool, fixture-source, and automation-tool code under `src/`.
- Outside `src/`, only manifests, lockfiles, configuration, documentation, non-code assets, and generated build output are allowed.
- Rust tests live beside modules under `#[cfg(test)]`, in `src/rust/integration_tests/`, or in another documented path under `src/`.
- Keep nontrivial CI and repository automation logic in `src/rust/bin/repo_guard.rs`; declarative workflow files may only invoke it.
- Respect the module boundaries and dependency direction defined in `docs/architecture/`.

## Change Discipline

- Inspect repository status and existing instructions before editing. Preserve unrelated user changes.
- Make the smallest coherent change. Do not perform unrelated refactors, broad formatting, dependency upgrades, or speculative rewrites.
- For nontrivial implementation work, use parallel agent teams where independent slices exist. Assign disjoint ownership, preserve other agents' and users' edits, and integrate results only through the main session after review.
- After implementation, inspect the changed code logic before accepting or merging agent-team output into the main session. Verify behavior with the required checks and resolve conflicts semantically, not by blindly choosing one side.
- Every code change must include corresponding tests and documentation changes in the same atomic commit.
- Update normative requirements in `docs/`, reusable workflows in `.agents/skills/`, durable learned context in `.agents/memories/`, and only cross-cutting mandatory rules in this mirrored contract.
- Never leave duplicated requirements inconsistent. Update the canonical document and every affected reference.

## Verification

- Run `cargo fmt --all -- --check`.
- Run `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- Run `cargo test --workspace --all-features`.
- Run `cargo run --quiet --bin repo-guard -- check`.
- Do not disable, weaken, skip, or bypass a failing check. Report any check that cannot be run.

## Git Workflow

- Every completed agent assignment ends with one or more atomic commits. Each commit must be independently coherent and include its tests and documentation.
- Use Conventional Commits, stage explicit paths, and review the staged diff before committing.
- Implement a major feature on a dedicated branch. A major feature changes public behavior, module boundaries, storage or protocol contracts, or multiple subsystems.
- Merge a major-feature branch into `main` only after all required checks pass. Use a non-fast-forward merge unless repository policy explicitly requires another strategy.
- Do not rewrite shared history, force-push, discard unrelated changes, commit secrets, or push unless explicitly authorized.

## Engineering Standards

- Prefer explicit types, deterministic behavior, small modules, typed errors, and dependency inversion at external boundaries.
- Treat inputs, paths, repository contents, database values, and MCP payloads as untrusted.
- Avoid hidden global state, silent fallback, swallowed errors, speculative abstractions, and unsupported claims.
- Mark unresolved static-analysis facts as `UNKNOWN`; do not convert heuristics into certainty.
- Treat Tree-sitter as a syntax and candidate-generation layer, not as the sole semantic oracle. Structural similarity alone must not prove semantic family membership.
- RepoGrammar v0.1 official language scope is TypeScript/JavaScript only. Python is planned second and must be marked experimental until a focused v0.2 adapter is accepted.
- Keep the CLI pattern-family-first. Do not add `callers`, `callees`, `impact`, `affected`, `node`, or `explore` as top-level v0.1 commands.
- Do not impose RepoGrammar's mirrored `AGENTS.md`/`CLAUDE.md` policy on repositories that consume RepoGrammar.
- Do not add a production dependency without demonstrated need and an accompanying architecture or decision update.

## Completion

- Work is complete only when implementation, tests, relevant documentation, mirrored-guide equality, verification, and atomic commits are complete.
- Final reports must include the branch, commit hash, changed documentation, verification commands and results, and remaining risks or `UNKNOWN`s.
