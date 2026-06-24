# Dependency Rules

## Allowed dependencies

- `core` may depend only on Rust standard library and explicitly accepted domain
  dependencies.
- `ports` may depend on `core`.
- `application` may depend on `core` and `ports`.
- `interfaces` may depend on `application`, `core`, and stable transport-neutral
  types.
- `adapters` may depend on `core`, `ports`, and their concrete external
  libraries.
- `bin` may wire all layers together.
- language-native workers under `src/workers/` may use their native compiler or
  language-service ecosystem behind a versioned protocol.
- optional providers, including any future CodeGraph provider, belong behind
  ports and adapters. Their SDK, CLI, MCP, or file formats must not enter
  `core`, `ports`, or `application` as concrete third-party types.

## Forbidden dependencies

- `core` must not depend on `interfaces`, `adapters`, `ports`, Tree-sitter,
  SQLite, MCP SDKs, filesystem, process execution, or network access.
- `ports` must not expose Tree-sitter nodes, SQLite rows, MCP payloads, or other
  concrete third-party transport types.
- `application` must not run SQL, inspect Tree-sitter nodes, or parse MCP
  transport payloads directly.
- `interfaces` must not implement pattern-family mining rules.
- `repo_guard` must not call product runtime code.

## Filesystem and Git discovery boundary

Repository traversal, filesystem metadata, symlink inspection, content-hash
calculation, and Git ignore probing belong in filesystem adapters. The
application layer may orchestrate discovery through ports and may classify
deferred indexing outcomes, but it must not expose absolute paths, source text,
or concrete process payloads across the port boundary.

## Tree-sitter boundary

Tree-sitter is the intended universal syntax technology, but parser AST nodes stay in
`src/rust/adapters/parsing/` and language-specific adapter modules. Adapters convert
parser output into `core::model` types before returning through `ports::parser`.

Tree-sitter is not a complete semantic analyzer. It can generate syntax
features, changed ranges, code-unit candidates, decorator/call shapes, and
structural fingerprints, but compiler-native semantic facts take precedence over
structural heuristics.

## Semantic worker boundary

Language-native semantic workers belong under `src/workers/` or
`src/rust/adapters/semantic_workers/`. They may use official compiler,
type-checker, or LSP APIs for their language. All compiler-native facts must be
translated into RepoGrammar-owned semantic facts before entering `core`.
Rust-side semantic-worker adapters may depend on transport-neutral JSON parsing
for versioned NDJSON validation, but parser/compiler SDK payloads must still be
translated at the adapter boundary.

When a semantic worker is unavailable, version-incompatible, conflicting with
another analyzer, or unable to decide a dynamic behavior, the result must be
`UNKNOWN` or abstention.

## SQLite boundary

SQLite and migration execution logic belong in `src/rust/adapters/persistence/`.
The `rusqlite` production dependency is allowed only at that adapter boundary.
Storage use cases depend on RepoGrammar-owned ports such as
`ports::index_store`, `ports::family_store`, and `ports::source_store`, not
direct SQL calls or SQLite row types.

## MCP boundary

MCP tool names, schemas, transport errors, and serialization rules belong in
`src/rust/interfaces/mcp/`. Domain classifications are expressed in core types before
they are serialized for MCP.

## Optional provider boundary

Optional providers such as a future CodeGraph provider may supply auxiliary
candidate, call/dependency, or graph-neighborhood facts only through
RepoGrammar-owned port types. Missing, stale, unavailable, or conflicting
provider facts must not fail default indexing and must become typed `UNKNOWN` or
auxiliary diagnostics for affected claims.

RepoGrammar must not create, initialize, modify, delete, or require
provider-owned local state such as `.codegraph/`.

## Test code boundary

All test source lives under `src/`, either next to modules with `#[cfg(test)]`,
in `src/rust/integration_tests/`, or in documented test-support modules. Root
`tests/`, `benches/`, `examples/`, and `scripts/` directories are not allowed.

## src-only rule

All source, executable, test, benchmark, migration-tool, fixture-source, worker,
and automation-tool code must live under `src/`, regardless of implementation
language. `repo-guard check` enforces this by detecting common source extensions
outside `src/`.
