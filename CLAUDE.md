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
- During active implementation, batch validation at coherent feature or workstream checkpoints instead of running tests after every small helper or subchange. Run early targeted tests only when they materially unblock diagnosis or protect high-risk behavior; final required checks still gate completion, commit, and merge.
- Every code change must include corresponding tests and documentation changes in the same atomic commit.
- Current v0.1 implementation planning is tracked in `docs/plans/v0.1-parallel-development-plan.md`, `docs/plans/python-v0.1-implementation-plan.md`, and durable memories under `.agents/memories/`. Update those plan/memory files whenever phase scope, Python v0.1 analysis, CodeGraph provider integration, or UNKNOWN policy changes.
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
- Automated agents must never add themselves, model or provider identities, tool accounts, or AI vendors as authors, committers, co-authors, signed-off-by identities, or any other contributor attribution. Agent-made commits must use only the maintainer-configured author and committer identity, with no agent attribution trailers.
- Implement a major feature on a dedicated branch. A major feature changes public behavior, module boundaries, storage or protocol contracts, or multiple subsystems.
- Merge a major-feature branch into `main` only after all required checks pass. Use a non-fast-forward merge unless repository policy explicitly requires another strategy.
- After `main` contains the merged work, delete merged or superseded feature and integration branches once their containment or patch-equivalence is verified.
- Do not rewrite shared history, force-push, discard unrelated changes, commit secrets, or push unless explicitly authorized.

## Engineering Standards

- Prefer explicit types, deterministic behavior, small modules, typed errors, and dependency inversion at external boundaries.
- Before writing custom logic, first reuse existing public APIs, repo-local helpers, native platform features, or installed dependency functionality when they already solve the problem.
- Keep new code minimal and necessary. Do not duplicate behavior already present in this repository or dependencies, and do not add logic unless the requirement genuinely needs it.
- Cross-path decisions must have one authoritative classifier or policy entrypoint; callers may route, format, persist, or test that result, but must not reimplement the decision from raw fields.
- Treat inputs, paths, repository contents, database values, and MCP payloads as untrusted.
- Avoid hidden global state, silent fallback, swallowed errors, speculative abstractions, and unsupported claims.
- Mark unresolved static-analysis facts as `UNKNOWN`; do not convert heuristics into certainty.
- Treat Tree-sitter as a syntax and candidate-generation layer, not as the sole semantic oracle. Structural similarity alone must not prove semantic family membership.
- RepoGrammar v0.1 official language scope is Python-first, focused on FastAPI, pytest, SQLAlchemy, and Pydantic. Existing TypeScript/JavaScript substrate is transitional and must not be described as the official v0.1 target unless a later ADR changes scope.
- Keep the CLI pattern-family-first. Do not add `callers`, `callees`, `impact`, `affected`, `node`, or `explore` as top-level v0.1 commands.
- Do not impose RepoGrammar's mirrored `AGENTS.md`/`CLAUDE.md` policy on repositories that consume RepoGrammar.
- Do not add a production dependency without demonstrated need and an accompanying architecture or decision update.

## Completion

- Work is complete only when implementation, tests, relevant documentation, mirrored-guide equality, verification, and atomic commits are complete.
- Final reports must include the branch, commit hash, changed documentation, verification commands and results, and remaining risks or `UNKNOWN`s.

<!-- BEGIN REPOGRAMMAR MANAGED SECTION -->
<!-- REPOGRAMMAR MANAGED CONTENT VERSION: 3 -->
## RepoGrammar pre-flight gate

RepoGrammar MCP is read-only. Read mandatory repository authority and instruction documents first. Then, when `.repogrammar/` exists, use this gate when planning or executing an implementation, fix, refactor, test, or diagnosis that requires a repository-local contract or convention, repeated implementation, framework role, or analogue comparison. For a covered task, run it before any non-trivial code location is sought and before any code-search or source-reading tool.

Covered cases include root-cause diagnosis or repair and schema, protocol, API, prompt-output, or Meaning Contract qualification, conformance, or drift. A YAML prompt or qualification output checked against a repeated Meaning Contract is covered; neither its file type nor an exact file target exempts mixed work.

1. Build one precise target in this order: exact repo-relative path or locator; exact `unit:`/member/symbol; exact framework role; only then a concise pattern question. Never replace a concrete locus with broad task or governance prose. A prose target retains any language/framework named by the task plus a supported concept such as route, fixture, validation model, data access, or test.
2. Call `repogrammar_context` once for that target with `operation: "find_analogues"` and `mode: "compact"`.
3. Consume the returned `read_plan`; line-numbered `source_spans` included in the result are already read.
4. If the tool is unavailable or explicitly reports `FALLBACK`, stale, omitted, or insufficient evidence, state that reason before CodeGraph. On `UNKNOWN`, exactly one returned candidate family id may be inspected once with `show_family` as candidate context only, never as selected-family or conformance proof; with multiple candidates, narrow using a stronger target or state the fallback reason. Use ordinary search/read only when CodeGraph is unavailable or still insufficient.

Never use CodeGraph first for covered work merely because exact source or call-path detail will be needed later. Call a given target only once; a materially narrower locator is a new target. Treat returned family ids as follow-up handles, not proof. Do not request `include_source_spans` by default and do not run `repogrammar stats` in normal agent loops.

Skip this gate for pure documentation or prose; operational release, git, environment, or credential inspection; syntax-only YAML or configuration validation; and an exact one-symbol, file, or call-path lookup, but only when no repository contract, convention, repeated implementation, framework role, analogue comparison, code-behavior diagnosis, or implementation decision is involved. Never initialize, resync, or start autosync silently; those writes require user or project-policy permission. If `.repogrammar/` does not exist, skip RepoGrammar for that repository.
<!-- END REPOGRAMMAR MANAGED SECTION -->
