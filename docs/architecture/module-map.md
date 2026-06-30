# Module Map

This map links `src/` paths to responsibilities and canonical documentation.

| Path | Responsibility | Canonical docs to update |
|---|---|---|
| `src/rust/core/model/` | Domain identifiers, IR, evidence, semantic facts, family classification, provenance | `docs/specifications/domain-model.md`, `docs/specifications/semantic-workers.md` |
| `src/rust/core/mining/` | Normalization, fingerprinting, candidate discovery, alignment, anti-unification, clustering, representative selection boundaries | `docs/specifications/indexing-pipeline.md` |
| `src/rust/core/policy/` | Compatibility, abstention, and freshness policy | `docs/specifications/domain-model.md`, `docs/specifications/product.md` |
| `src/rust/ports/` | Traits for parser, semantic worker, index store, family store, source store, and telemetry, plus contract types for future Python, Rust, and TS/JS semantic providers | `docs/architecture/dependency-rules.md`, related specifications |
| `src/rust/application/` | Indexing, query, conformance, install planning, progress, repository lifecycle, auto-sync lifecycle state, storage, telemetry, metrics, and proof-lattice support-promotion use-case boundaries | `docs/architecture/overview.md`, this file, relevant specifications |
| `src/rust/application/family.rs` | Conservative EC-MVFI-lite family claim construction from validated application inputs | `docs/specifications/indexing-pipeline.md`, `docs/specifications/product.md`, `docs/specifications/unknowns.md` |
| `src/rust/adapters/filesystem/` | Filesystem and Git-backed discovery boundaries | `docs/specifications/indexing-pipeline.md`, `docs/specifications/storage.md`, `docs/architecture/dependency-rules.md` |
| `src/rust/adapters/parsing/` | Parser boundaries, including transitional TS/JS syntax extraction, bounded TS/JS project-config inventory, conservative TS/JS exact-anchor structural/UNKNOWN facts (`tsjs/`), Tree-sitter Rust self-dogfood structural extraction (`rust/`), and the CPython AST-backed Python extractor | `docs/specifications/indexing-pipeline.md`, `docs/architecture/dependency-rules.md` |
| `src/rust/adapters/languages/` | Language-specific parsing configuration | `docs/specifications/indexing-pipeline.md`, `docs/roadmap.md` |
| `src/rust/adapters/semantic_workers/` | Rust-side process boundary for language-native semantic workers | `docs/specifications/semantic-workers.md`, `docs/decisions/ADR-0004-rust-core-language-native-workers.md` |
| `src/rust/adapters/frameworks/` | Framework recognition boundaries; TS/JS adapter registry for Express, Jest/Vitest, React, Next.js, Fastify, Prisma, and Drizzle roles plus Python FastAPI, pytest, SQLAlchemy, and Pydantic roles | `docs/specifications/indexing-pipeline.md`, `docs/roadmap.md`, `docs/specifications/python-analysis.md` |
| `src/rust/adapters/persistence/` | SQLite storage boundary | `docs/specifications/storage.md`, `docs/decisions/ADR-0002-local-sqlite-index.md` |
| `src/rust/adapters/telemetry/` | Local diagnostic event sink boundary | `docs/architecture/overview.md` |
| `src/rust/interfaces/cli/` | Pattern-family-first CLI argument and output boundary | `README.md`, `docs/specifications/cli.md` |
| `src/rust/interfaces/mcp/` | Transport-neutral MCP tool boundary | `docs/specifications/mcp-api.md` |
| `src/rust/bin/repogrammar.rs` | Product composition root | `README.md`, CLI documentation |
| `src/rust/bin/repo_guard.rs` | Repository governance CLI | `docs/development/repository-guard.md` |
| `src/install/` | End-user and source-checkout installer wrapper scripts that install release binaries and call the product binary without duplicating native agent logic | `README.md`, `docs/specifications/installation.md`, `docs/development/testing.md` |
| `src/npm/` | Thin npm/npx launcher and tests that download release binaries and exec the product binary without reimplementing RepoGrammar | `README.md`, `docs/specifications/installation.md`, `docs/development/testing.md` |
| `src/rust/test_support/` | Shared deterministic Rust test helpers | `docs/development/testing.md` |
| `src/rust/integration_tests/` | Crate-level Rust integration-style tests | `docs/development/testing.md` |
| `src/workers/typescript/` | Transitional TypeScript semantic worker executable stub and future compiler-backed worker | `docs/specifications/semantic-workers.md` |
| `src/workers/python/` | CPython `ast`/`symtable`-backed Python worker for private parse-document extraction, private `tomllib` project-config summaries, and conservative framework-role NDJSON smoke coverage | `docs/specifications/semantic-workers.md`, `docs/specifications/python-analysis.md` |
| `src/protocol/` | Semantic worker protocol notes and schema | `docs/specifications/semantic-workers.md` |
| `src/fixtures/typescript/` | TypeScript/JavaScript source fixtures for Express, Jest/Vitest, Next.js, Fastify, Prisma, Drizzle, and negative TS/JS adapter cases | `docs/development/testing.md` |
| `src/fixtures/python/` | Python v0.1 source fixtures | `docs/development/testing.md`, `docs/specifications/python-analysis.md` |
| `src/fixtures/rust/` | Rust v0.2 self-dogfood source fixtures | `docs/development/testing.md`, `docs/specifications/indexing-pipeline.md` |

Every `src/` change must include a relevant documentation update in the same
commit unless the documentation already precisely describes the resulting
state. The agent must explain that judgment in the final report.
