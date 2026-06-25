# Project State

- Status: Bootstrap plus syntax-only indexing, structural IR storage, opt-in
  semantic fact ingestion, internal active claim-input snapshot reads, and
  semantic-fact freshness/readiness gating
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
generation-scoped SQLite storage, syntax-only code-unit indexing,
CodeUnit-derived structural IR node/containment-edge storage, Rust-side
TypeScript semantic-worker request/output protocol validation and process
validation, a dependency-free TypeScript worker stub that reports compiler
analysis as unavailable, a validated semantic-fact storage writer, and opt-in
command-level semantic-worker fact ingestion through the same-generation storage
gate. It also has an internal active-generation claim-input snapshot read path
for future claim builders and an internal file-hash freshness/readiness gate
that blocks stale facts, unsupported fact kinds, weak certainty, or conflicting
certainty with typed `UNKNOWN`.

## Durable knowledge

Implemented capabilities include module boundaries, minimal domain types,
pattern-family-first CLI command parsing, safe installer dry-run planning, typed
progress and telemetry policy types, stable not-implemented behavior,
transport-neutral MCP single-tool operation boundary, repository guard checks,
documentation, skills, memories, CI configuration, repo-local
`init`/`uninit`/`status`/`doctor`/`unlock`/`logs`, TS/JS file discovery,
hash-checked source reads, dependency-free syntax-only code-unit extraction,
CodeUnit-derived structural IR nodes and conservative containment edges,
generation-scoped SQLite migrations/storage/validation/activation, product
runtime wiring for `index` and `sync`, and the dependency-free
`src/workers/typescript/worker.js` unavailable fallback stub, plus limited
`files`/`units` reads from active file-manifest-only or syntax-only generations.
Those reads revalidate active-generation health plus stored paths, hashes,
languages, unit ids, and byte ranges before returning repo-relative metadata.
The storage port and SQLite adapter can persist already-validated semantic facts
and repo-relative evidence for building generations when they match an indexed
same-generation code unit's path, content hash, and byte range. By default
`index` and `sync` still report `semantic_worker: deferred`; when
`REPOGRAMMAR_TYPESCRIPT_WORKER` names an explicit worker executable, optional
`REPOGRAMMAR_TYPESCRIPT_WORKER_ARGS_JSON` supplies its argv vector, and accepted
worker facts may be recorded before generation validation and activation.
Worker fallback keeps indexing syntax-only, while mismatched semantic evidence
aborts the new generation.
The application query/storage boundary can load an internal active-generation
claim-input snapshot containing files, code units, IR nodes/edges, and semantic
facts after revalidating stored fact kind/certainty tokens, assumptions JSON,
repo-relative evidence, content hashes, code-unit ids, and byte ranges. This is
an internal substrate only; CLI/MCP query commands do not render semantic facts.
The query application layer can check snapshot semantic facts against current
source hashes and classify fresh supported facts as eligible inputs for future
claim builders or typed `UNKNOWN` blockers (`StaleEvidence`,
`InsufficientSupport`, or `ConflictingFacts`). Fresh eligible facts are still not
family evidence.

Tree-sitter integration, TypeScript compiler API integration, command-level
full repository/worktree freshness metadata, family-claim gates, typed IR
attributes beyond the structural bootstrap graph, semantic/framework facts,
family mining, query-ready family evidence, pattern-family query execution, MCP
serving, installer writes, and telemetry network transport are not implemented.

Pattern-family query commands still use stable fallback behavior. `files` and
`units` can return active file-manifest-only or syntax-only index metadata, but
stored syntax-only units must not be described as query-ready family evidence.

## Implications

Future agents must not claim TypeScript analysis, Python production support,
pattern-family mining, freshness-validated semantic claims, query execution, or
stable MCP API support until those capabilities are implemented and tested.
Agents also must not restart repo-local lifecycle, SQLite generation, opt-in
semantic-worker ingestion, or Rust-side worker process validation work from
scratch. Do not restart structural IR storage or active semantic-fact/evidence
read-path work from scratch either; extend the existing lifecycle, storage,
worker stub, query read path, and worker boundary substrates through the
canonical specs.

## Revalidation conditions

Update this memory after Tree-sitter integration, TypeScript compiler API
integration, full family-claim gates, family-query claim paths, MCP serving,
installer writes, or production family evidence lands.
