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
  and telemetry policy. Its recovery boundary owns one transport-neutral action
  classifier consumed by setup, repository readiness, query preflight, and MCP
  recommendations; one recovery formatter keeps the setup/resync/autosync
  command path identical across those consumers. Setup carries repository
  freshness into its plan, distinguishes current and obsolete owned agent
  authority, accepts auto-sync readiness only after the composition root proves
  PID-plus-nonce lock ownership and child liveness, and keeps telemetry
  preference unchanged. Setup mutation results distinguish newly configured
  targets from reconfigured pre-existing targets so outer rollback cannot
  delete prior ownership. Consumers do not rederive these decisions from raw
  lifecycle or evidence fields. Its family boundary owns the authoritative
  cross-language family-`UNKNOWN` classifier; structural support derivation,
  family construction, compatibility, and query reporting reuse that decision
  instead of interpreting raw reason or claim fields independently.
  It depends on `core` and `ports`.
- `src/rust/interfaces`: CLI and MCP input/output boundaries. It delegates to
  application use cases and does not own pattern-family logic.
- `src/rust/adapters`: concrete implementations for parser, language,
  framework, semantic-worker, persistence, and telemetry ports.
- `src/rust/bin`: composition roots and process exit behavior.
- `src/workers`: language-native semantic workers such as the Python worker and
  the operation-scoped TypeScript worker.
- `src/protocol`: versioned protocol notes and schemas shared across workers.

## Data flow

The intended indexing flow is:

```text
repository files -> discovery/exclusion policy -> parser adapter -> code units -> language-native semantic worker -> core IR -> application pipeline -> store port -> repo-local SQLite adapter
```

The discovery/exclusion boundary applies fixed aggregate file, byte, skipped-
path, visited-entry, and depth admission before the application can prepare an
index generation. The opt-in autosync metadata fingerprint reuses the relevant
file/byte/visited/depth policy inside the same filesystem adapter; the binary
composition root does not implement a second recursive walker.

Those aggregate bounds do not close the current canonicalize-then-reopen race.
ADR-0023 accepts a future private handle-relative filesystem authority shared
by discovery, source reads, and fingerprinting, with no-follow child opens and
same-handle metadata/content reads. The dependency/platform qualification and
implementation remain incomplete; the architecture must not claim concurrent
filesystem confinement yet.

The current product path implements discovery, a dependency-free syntax-only
parser adapter, code-unit metadata storage, CodeUnit-derived IR node and
containment-edge storage, and SQLite generation activation. The Rust-side
TypeScript semantic-worker process boundary can validate NDJSON worker output
into owned facts. `index` and `sync` can optionally run an explicit worker
executable through `REPOGRAMMAR_TYPESCRIPT_WORKER`, pass a JSON configured argv
vector from `REPOGRAMMAR_TYPESCRIPT_WORKER_ARGS_JSON`, and record only facts
that match the building generation's indexed code-unit evidence. The
application layer can currently promote only matching TypeScript-provider
`resolve_export` facts for exact Next.js file-convention anchors and
`resolve_reexport` facts for relative repo-local Express/Fastify named handler
imports, Prisma shared-client imports, or Drizzle db/table imports into
TS/JS-derived provider-resolved support; fallback worker facts remain context.
Tree-sitter, full TypeScript compiler `Program`/`TypeChecker` worker support,
freshness-validated semantic claims beyond bounded worker operations, typed IR
attributes beyond the structural bootstrap graph, family mining, and stronger
query evidence remain later boundaries.

Query and conformance flows reverse that direction by reading stored family and
source evidence through ports before returning interface-specific output.
The application query boundary keeps exact identifiers authoritative, then uses
the committed dependency-free query vocabulary for bounded normalization,
ranking, calibrated abstention, and freshness hydration. Vocabulary v2 can
qualify three exact two-term test/fixture phrases without lowering the absolute
selection or competing-family margin gates; ambiguous families still abstain.
Inventory-style queries such as `stats --json`, `families --json`, and MCP
candidate discovery use bounded read-model ports so agent loops do not hydrate
full family evidence, semantic facts, IR graphs, or all family details. The
SQLite adapter writes repository-derived state only under `.repogrammar/` or the
directory named by `REPOGRAMMAR_DIR`.

Installer flows stay separate from repository indexing flows because they modify
machine-level agent integration rather than repository-local index state.
`install` and `uninstall` must not create or remove `.repogrammar/`; `init` and
`uninit` own that lifecycle. The install service is also the sole authority for
reconciling an internally consistent owned integration from an obsolete managed
executable to the current authority; setup delegates that refresh and rolls
back only targets it newly created.
The install application owns exact versioned, marker-fenced instruction
profiles. The full repository gate and MCP initialize guidance share the
existing v3 contract; install/setup use a shorter conditional global v4 profile
under ADR-0054. Both preserve evidence/abstention boundaries. Known profile
paths, opt-out, instruction-only backfill and receipt preservation remain in
the install authority; native MCP is not rewritten for instruction-only work.

## Composition root

`src/rust/bin/repogrammar.rs` is the product composition root. It currently
wires the CLI boundary, repository-lifecycle surface, multi-language discovery
and bounded parser adapters including the current CPython frontend, filesystem
source reader, SQLite generation store
for `index` and `sync`, optional semantic-worker ingestion when an explicit
worker executable and optional argv vector are configured, FamilyStore-backed
query reads, repository-local storage maintenance (`prune`, `compact`, and
`storage clean`), autosync daemon process management, and read-only MCP serving
through the same query layer. Auto-sync start is a bounded process handshake at
this boundary: the child acquires the daemon lock and records a startup nonce,
while the parent verifies the expected PID/nonce and child liveness before it
returns a running result. The current Python frontend and bounded exact-anchor
family induction are implemented. Python type-provider execution, general
Python semantics, and broad TypeScript Program/TypeChecker analysis remain
later boundaries; current family claims retain their documented exact-anchor
and typed-UNKNOWN limits.
`src/rust/bin/repo_guard.rs` is a separate governance/qualification tool.
Product runtime must not depend on its checks or report schemas. The isolated
host-stage harness may compose existing owned application/adapter APIs for
diagnostics, with independent public-CLI canonical parity; it must not duplicate
family, support or freshness decisions. Normal runtime never starts the profiler.

The query application loads local-context units only when the existing path
matcher or an admitted unit-path prefix can resolve a locus. Impossible targets
retain the original UNKNOWN and directory fallback. All generation, integrity,
foreign-key and source-hash freshness checks remain authoritative; no final
readiness verdict is cached by this optimization.

## External dependency boundaries

The Python parser adapter owns same-process admission of the published Python
3.10+ host requirement before loading its private worker. Only the typed
`PythonFrontendInterpreterUnsupported` crosses the port; the indexing
application uses one source-free recovery formatter across full, incremental,
and project-config parsing. Admission failure cannot activate a replacement
generation; interface extraction remains `Unverified`, preserving conservative
sync fallback. This does not add a semantic provider or execute analyzed sources.

Under ADR-0055, current source builds use one owned Python frontend session
when at least two Python documents are planned. The parser port exposes only
owned parse results and `finish`; the adapter owns isolated stdlib startup,
bounded frames, compact project projections and deterministic worker shutdown.
The indexing application requires EOS/success/EOF before checkpoint/activation.
One-file incremental parsing remains on the existing request path. This
changes transport cost, not family/support semantics or the public MCP schema.

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
