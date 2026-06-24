# Project State

- Status: Bootstrap plus syntax-only indexing substrate
- Last updated: 2026-06-25
- Scope: Current implemented capability snapshot.
- Evidence: Rust code, README, roadmap, CLI/storage/indexing specs, and
  `repo-guard` checks.
- Related canonical docs: `README.md`, `docs/roadmap.md`,
  `docs/specifications/cli.md`, `docs/specifications/storage.md`,
  `docs/specifications/indexing-pipeline.md`
- Supersedes: None
- Superseded by: None

## Context

RepoGrammar is still pre-alpha, but it is past pure skeleton bootstrap. The
current branch has repository-local lifecycle, TypeScript/JavaScript discovery,
generation-scoped SQLite storage, syntax-only code-unit indexing, Rust-side
TypeScript semantic-worker process validation, and a validated semantic-fact
storage writer that is not yet wired into indexing.

## Durable knowledge

Implemented capabilities include module boundaries, minimal domain types,
pattern-family-first CLI command parsing, safe installer dry-run planning, typed
progress and telemetry policy types, stable not-implemented behavior,
transport-neutral MCP single-tool operation boundary, repository guard checks,
documentation, skills, memories, CI configuration, repo-local
`init`/`uninit`/`status`/`doctor`/`unlock`/`logs`, TS/JS file discovery,
hash-checked source reads, dependency-free syntax-only code-unit extraction,
generation-scoped SQLite migrations/storage/validation/activation, and product
runtime wiring for `index` and `sync`. The storage port and SQLite adapter can
persist already-validated semantic facts and repo-relative evidence for building
generations when they match an indexed same-generation code unit's path, content
hash, and byte range, but no current command produces those rows.

Tree-sitter integration, executable TypeScript compiler worker source,
command-level semantic-fact indexing, unified IR population,
semantic/framework facts, family mining, query-ready family evidence, query read
paths, MCP serving, installer writes, and telemetry network transport are not
implemented.

Current query commands still use stable fallback behavior. Stored syntax-only
units must not be described as query-ready family evidence.

## Implications

Future agents must not claim TypeScript analysis, Python production support,
pattern-family mining, semantic-fact indexing, query execution, or stable MCP API
support until those capabilities are implemented and tested. Agents also must
not restart repo-local lifecycle, SQLite generation, or Rust-side worker process
validation work from scratch; extend the existing lifecycle, storage, and worker
boundary substrates through the canonical specs.

## Revalidation conditions

Update this memory after Tree-sitter integration, TypeScript compiler worker
source, command-level semantic-fact indexing, query read paths, MCP serving,
installer writes, or production family evidence lands.
