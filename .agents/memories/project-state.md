# Project State

- Status: Bootstrap plus syntax-only indexing and opt-in semantic fact ingestion
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
TypeScript semantic-worker request/output protocol validation and process
validation, a dependency-free TypeScript worker stub that reports compiler
analysis as unavailable, a validated semantic-fact storage writer, and opt-in
command-level semantic-worker fact ingestion through the same-generation storage
gate.

## Durable knowledge

Implemented capabilities include module boundaries, minimal domain types,
pattern-family-first CLI command parsing, safe installer dry-run planning, typed
progress and telemetry policy types, stable not-implemented behavior,
transport-neutral MCP single-tool operation boundary, repository guard checks,
documentation, skills, memories, CI configuration, repo-local
`init`/`uninit`/`status`/`doctor`/`unlock`/`logs`, TS/JS file discovery,
hash-checked source reads, dependency-free syntax-only code-unit extraction,
generation-scoped SQLite migrations/storage/validation/activation, product
runtime wiring for `index` and `sync`, and the dependency-free
`src/workers/typescript/worker.js` unavailable fallback stub, plus limited
`files`/`units` reads from the active syntax-only generation. Those reads
revalidate active-generation health plus stored paths, hashes, languages, unit
ids, and byte ranges before returning repo-relative metadata. The storage port
and SQLite adapter can persist already-validated semantic facts and
repo-relative evidence for building generations when they match an indexed
same-generation code unit's path, content hash, and byte range. By default
`index` and `sync` still report `semantic_worker: deferred`; when
`REPOGRAMMAR_TYPESCRIPT_WORKER` names an explicit worker executable, optional
`REPOGRAMMAR_TYPESCRIPT_WORKER_ARGS_JSON` supplies its argv vector, and accepted
worker facts may be recorded before generation validation and activation.
Worker fallback keeps indexing syntax-only, while mismatched semantic evidence
aborts the new generation.

Tree-sitter integration, TypeScript compiler API integration, command-level
semantic-fact freshness/claim gates, unified IR population, semantic/framework
facts, family mining, query-ready family evidence, pattern-family query
execution, MCP serving, installer writes, and telemetry network transport are
not implemented.

Pattern-family query commands still use stable fallback behavior. `files` and
`units` can return active syntax-only index metadata, but stored syntax-only
units must not be described as query-ready family evidence.

## Implications

Future agents must not claim TypeScript analysis, Python production support,
pattern-family mining, freshness-validated semantic claims, query execution, or
stable MCP API support until those capabilities are implemented and tested.
Agents also must not restart repo-local lifecycle, SQLite generation, opt-in
semantic-worker ingestion, or Rust-side worker process validation work from
scratch; extend the existing lifecycle, storage, worker stub, and worker
boundary substrates through the canonical specs.

## Revalidation conditions

Update this memory after Tree-sitter integration, TypeScript compiler API
integration, semantic-fact freshness/claim gates, family-query read paths, MCP
serving, installer writes, or production family evidence lands.
