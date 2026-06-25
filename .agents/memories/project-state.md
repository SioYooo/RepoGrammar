# Project State

- Status: Bootstrap plus syntax-only indexing, structural IR storage, opt-in
  syntax-origin framework-role fact storage, semantic fact ingestion, internal
  active claim-input snapshot reads, semantic-fact freshness/readiness gating,
  FamilyStore-backed query reads, read-only MCP serving, and narrow global
  explicit-target installer writes
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
syntax-origin TS/JS framework-role fact storage, CodeUnit-derived structural IR
node/containment-edge storage, Rust-side TypeScript semantic-worker
request/output protocol validation and process validation, a dependency-free
TypeScript worker stub that reports compiler analysis as unavailable, a
validated semantic-fact storage writer, opt-in command-level semantic-worker
fact ingestion through the same-generation storage gate, conservative
FamilyStore-backed query reads, and a read-only MCP `repogrammar_context` stdio
boundary. It also has narrow live installer/uninstaller writes for explicit
global Codex and Claude Code MCP targets through native agent CLIs, gated by
`--yes`, MCP self-test, and RepoGrammar-owned receipts. It also has an internal
active-generation claim-input snapshot read path for future claim builders and
an internal file-hash freshness/readiness gate that blocks stale facts,
unsupported fact kinds, weak certainty, or conflicting certainty with typed
`UNKNOWN`.

## Durable knowledge

Implemented capabilities include module boundaries, minimal domain types,
pattern-family-first CLI command parsing, safe installer dry-run planning, typed
progress and telemetry policy types, stable not-implemented behavior,
transport-neutral MCP single-tool operation boundary, read-only MCP serving,
repository guard checks,
documentation, skills, memories, CI configuration, repo-local
`init`/`uninit`/`status`/`doctor`/`unlock`/`logs`, TS/JS file discovery,
hash-checked source reads, dependency-free syntax-only code-unit extraction,
syntax-origin framework-role facts for recognized Express, React, and
Jest/Vitest code-unit shapes, CodeUnit-derived structural IR nodes and
conservative containment edges, generation-scoped SQLite
migrations/storage/validation/activation, product runtime wiring for `index`
and `sync`, and the dependency-free
`src/workers/typescript/worker.js` unavailable fallback stub, plus limited
`files`/`units` reads from active file-manifest-only or syntax-only generations.
Those reads revalidate active-generation health plus stored paths, hashes,
languages, unit ids, and byte ranges before returning repo-relative metadata.
`index` and `sync` acquire `.repogrammar/locks/index.lock` before discovery and
hold it through validation and activation. Partial lock metadata write failures
must remove the partial lock file. `unlock --force --yes` removes only confirmed
stale `index.lock`; active, unknown, invalid, daemon, and SQLite locks remain
in place. Status and doctor JSON use explicit manifest/storage schema-version
fields and do not expose ambiguous `schema_version` fields.
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
family evidence. Syntax-origin framework-role facts use
`FRAMEWORK_HEURISTIC` certainty and remain blocked from family-claim input as
insufficient support until stronger evidence and claim builders exist.

Tree-sitter integration, TypeScript compiler API integration, command-level
full repository/worktree freshness metadata, typed IR attributes beyond the
structural bootstrap graph, resolved framework semantics, full family mining,
broad installer writes, project-local installer writes, instruction-file
integration, and telemetry network transport are not implemented.

Pattern-family query commands and MCP tool calls still use stable fallback
behavior before an active index and typed `UNKNOWN` when active evidence is
insufficient. `files` and `units` can return active file-manifest-only or
syntax-only index metadata, but stored syntax-only units must not be described
as query-ready family evidence.

## Implications

Future agents must not claim TypeScript analysis, Python production support,
full pattern-family mining, freshness-validated semantic claims, installer
writes beyond explicit Codex/Claude MCP registration, or stable MCP API support
until those capabilities are implemented and tested.
Agents also must not restart repo-local lifecycle, SQLite generation, opt-in
semantic-worker ingestion, or Rust-side worker process validation work from
scratch. Do not restart structural IR storage or active semantic-fact/evidence
read-path work from scratch either; extend the existing lifecycle, storage,
worker stub, query read path, and worker boundary substrates through the
canonical specs.

## Revalidation conditions

Update this memory after Tree-sitter integration, TypeScript compiler API
integration, full family-claim gates, broader installer writes, production
family evidence, or stable MCP API support lands.
