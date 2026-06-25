# Architecture Overview

RepoGrammar uses a Rust primary core with explicit room for language-native
semantic workers. The architecture is layered with ports and adapters so parser,
storage, telemetry, CLI, MCP, and worker-runtime concerns do not leak into the
core domain model.

## Layers

```text
core
  ^
ports
  ^
application
  ^
interfaces

adapters --implements--> ports

bin --wires together--> interfaces + application + adapters
```

## Responsibilities

- `src/rust/core`: domain model, mining primitives, and policies. It has no dependency
  on Tree-sitter, SQLite, MCP, CLI, filesystem, network, or process concerns.
- `src/rust/ports`: traits for external capabilities. Ports can depend on `core` but
  cannot expose third-party parser, database, or transport types.
- `src/rust/application`: use-case orchestration for indexing, query,
  conformance, installation planning, progress, repository lifecycle, metrics,
  and telemetry policy.
  It depends on `core` and `ports`.
- `src/rust/interfaces`: CLI and MCP input/output boundaries. It delegates to
  application use cases and does not own pattern-family logic.
- `src/rust/adapters`: concrete implementations for parser, language,
  framework, semantic-worker, persistence, and telemetry ports.
- `src/rust/bin`: composition roots and process exit behavior.
- `src/workers`: future language-native semantic workers such as TypeScript.
- `src/protocol`: versioned protocol notes and schemas shared across workers.

## Data flow

The intended indexing flow is:

```text
repository files -> discovery/exclusion policy -> parser adapter -> code units -> language-native semantic worker -> core IR -> application pipeline -> store port -> repo-local SQLite adapter
```

The current product path implements discovery, a dependency-free syntax-only
parser adapter, code-unit metadata storage, CodeUnit-derived IR node and
containment-edge storage, and SQLite generation activation. The Rust-side
TypeScript semantic-worker process boundary can validate NDJSON worker output
into owned facts. `index` and `sync` can optionally run an explicit worker
executable through `REPOGRAMMAR_TYPESCRIPT_WORKER`, pass a JSON configured argv
vector from `REPOGRAMMAR_TYPESCRIPT_WORKER_ARGS_JSON`, and record only facts
that match the building generation's indexed code-unit evidence.
Tree-sitter, TypeScript compiler worker code, freshness-validated semantic
claims, typed IR attributes beyond the structural bootstrap graph, family
mining, and stronger query evidence remain later boundaries.

Query and conformance flows reverse that direction by reading stored family and
source evidence through ports before returning interface-specific output. The
SQLite adapter writes repository-derived state only under `.repogrammar/` or the
directory named by `REPOGRAMMAR_DIR`.

Installer flows stay separate from repository indexing flows because they modify
machine-level agent integration rather than repository-local index state.
`install` and `uninstall` must not create or remove `.repogrammar/`; `init` and
`uninit` own that lifecycle.

## Composition root

`src/rust/bin/repogrammar.rs` is the product composition root. It currently
wires the CLI boundary, repository-lifecycle surface, TS/JS discovery,
syntax-only parser adapter, filesystem source reader, SQLite generation store
for `index` and `sync`, optional semantic-worker ingestion when an explicit
worker executable and optional argv vector are configured, FamilyStore-backed
query reads, and read-only MCP serving through the same query layer. Full
family mining, TypeScript compiler analysis, broad installer writes, and stable
production family-evidence claims remain later boundaries.
`src/rust/bin/repo_guard.rs` is a separate governance tool and must not be
coupled to product runtime logic.

## External dependency boundaries

Tree-sitter belongs only in parsing and language adapters and is treated as
syntax-first, not semantics-only. Language-native
compiler, type-checker, or LSP types belong only in semantic-worker adapters and
workers. SQLite and SQL migration logic belong only in persistence adapters. MCP
schemas and transport errors belong only in `interfaces/mcp`.

Optional providers such as a future CodeGraph provider are lower-layer
auxiliary evidence sources. They must be isolated behind ports/adapters,
translated into RepoGrammar-owned facts, and treated as optional. Core and
application logic must not require provider-owned local state or APIs.

## Why ports and adapters

Pattern-family conclusions must be auditable and conservative. Keeping
third-party parser, compiler, storage, and transport types outside the core makes it
possible to test domain behavior deterministically and mark uncertain facts as
`UNKNOWN` rather than treating adapter quirks as domain truth.
