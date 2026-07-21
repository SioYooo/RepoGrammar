# Module Map

This map links `src/` paths to responsibilities and canonical documentation.

| Path | Responsibility | Canonical docs to update |
|---|---|---|
| `src/rust/core/model/` | Domain identifiers, IR, evidence, semantic facts, family classification, provenance, and internal orthogonal UNKNOWN claim-impact/resolution axes behind the stable public `UnknownClass` projection | `docs/specifications/domain-model.md`, `docs/specifications/semantic-workers.md`, `docs/specifications/unknowns.md` |
| `src/rust/core/mining/` | Normalization, fingerprinting, candidate discovery, alignment, anti-unification, clustering, representative selection boundaries | `docs/specifications/indexing-pipeline.md` |
| `src/rust/core/policy/` | Compatibility, abstention, and freshness policy | `docs/specifications/domain-model.md`, `docs/specifications/product.md` |
| `src/rust/ports/` | Traits for parser, semantic worker, index store, family store, source store, and telemetry, including bounded read-model query contracts for stats, family summaries, and family candidates, plus contract types for future Python, Rust, and TS/JS semantic providers | `docs/architecture/dependency-rules.md`, related specifications |
| `src/rust/application/` | Indexing, query, conformance, install planning, zero-friction setup orchestration, progress, repository lifecycle, auto-sync lifecycle state, storage, telemetry, metrics, proof-lattice support-promotion, and authoritative readiness/UNKNOWN recovery-policy use-case boundaries | `docs/architecture/overview.md`, this file, relevant specifications |
| `src/rust/application/query.rs`, `query_terms.rs`, `query_resolution.rs` | Exact-first family query routing, deterministic vocabulary-versioned term normalization/ranking, calibrated abstention, bounded freshness hydration, and static-alignment orchestration. `query_resolution.rs` is the single deterministic, source-free authority that parses a raw target into orthogonal hard/scope/ranking constraints (`TargetConstraints`) reused across the resolution call sites | `docs/specifications/query-resolution.md`, `docs/specifications/domain-model.md` |
| `src/rust/application/setup.rs` | Pure setup plan/execution state machine that composes existing machine and repository ownership boundaries with one authorization, current/outdated owned-agent authority state, status-derived freshness, three-state family inventory, typed preservation, and rollback limited to newly configured integrations | `docs/decisions/ADR-0026-zero-friction-onboarding-orchestration.md`, `docs/specifications/cli.md`, `docs/specifications/installation.md` |
| `src/rust/application/recovery.rs` | Single authoritative recovery classifier and command formatter consumed by setup, repository readiness, query preflight, CLI, and MCP presentation boundaries | `docs/specifications/cli.md`, `docs/specifications/unknowns.md` |
| `src/rust/application/family.rs` | Conservative EC-MVFI-lite family claim construction and the authoritative cross-language family-`UNKNOWN` classifier reused by support derivation, compatibility, and query paths | `docs/specifications/indexing-pipeline.md`, `docs/specifications/product.md`, `docs/specifications/unknowns.md` |
| `src/rust/application/process_liveness.rs` | Shared process and lock-owner liveness policy for repository locks and autosync daemon locks; the composition root combines it with PID-plus-nonce startup ownership before reporting a spawned daemon ready | `docs/specifications/storage.md`, `docs/development/testing.md` |
| `src/rust/adapters/filesystem/` | Filesystem and Git-backed discovery boundaries, including shared aggregate resource admission and bounded autosync metadata fingerprint traversal; ADR-0023 reserves the future no-follow handle-relative root/child authority here, without exposing capability types across ports | `docs/specifications/indexing-pipeline.md`, `docs/specifications/storage.md`, `docs/architecture/dependency-rules.md`, `docs/decisions/ADR-0023-handle-relative-filesystem-confinement-preflight.md` |
| `src/rust/adapters/parsing/` | Parser boundaries, including transitional TS/JS syntax extraction, bounded TS/JS project-config inventory for path aliases/rootDirs, conservative TS/JS exact-anchor structural/UNKNOWN facts (`tsjs/`), Tree-sitter Java/Spring structural extraction (`java/`) with bounded same-class JUnit/TestNG test-data-link resolution in `java/test_data.rs`, C# structural extraction (`csharp.rs`) with pure same-class xUnit data-link analysis in `csharp/test_data.rs`, C/C++ scanner/fact orchestration (`cpp/mod.rs`) with bounded preprocessor analysis in `cpp/preprocessor.rs`, pure test-macro contract and linear Boost suite-state analysis in `cpp/test_framework.rs`, and project-config inventory in `cpp/project_config.rs`, Tree-sitter Rust self-dogfood structural extraction (`rust/`), and the CPython AST-backed Python extractor | `docs/specifications/indexing-pipeline.md`, `docs/architecture/dependency-rules.md`, `docs/decisions/ADR-0020-top-20-language-expansion-gate.md` |
| `src/rust/adapters/languages/` | Language-specific parsing and discovery configuration, including pure Go, PHP, Ruby, and Swift path/configuration classifiers that do not select a runtime environment or support a family | `docs/specifications/indexing-pipeline.md`, `docs/roadmap.md` |
| `src/rust/adapters/semantic_workers/` | Rust-side process boundary for language-native semantic workers | `docs/specifications/semantic-workers.md`, `docs/decisions/ADR-0004-rust-core-language-native-workers.md` |
| `src/rust/adapters/frameworks/` | Framework recognition boundaries for current TS/JS, Python, Rust, Java, C#, and C/C++ exact-anchor roles; new Top-20 languages receive a language-specific registry only in the corresponding atomic family submodule | `docs/specifications/indexing-pipeline.md`, `docs/roadmap.md`, `docs/specifications/python-analysis.md`, `docs/plans/top-20-language-expansion-plan.md` |
| `src/rust/adapters/persistence/` | SQLite storage boundary, including schema migrations and read-path indexes | `docs/specifications/storage.md`, `docs/decisions/ADR-0002-local-sqlite-index.md` |
| `src/rust/adapters/telemetry/` | Local diagnostic event sink boundary | `docs/architecture/overview.md` |
| `src/rust/interfaces/cli/` | Pattern-family-first CLI argument and progressive human-output boundary, including setup parsing, one confirmation, distinct readiness facts/stage labels, complete limitations, and sanitized rendering | `README.md`, `docs/specifications/cli.md` |
| `src/rust/interfaces/mcp/` | Transport-neutral MCP tool boundary | `docs/specifications/mcp-api.md` |
| `src/rust/bin/repogrammar.rs` | Product composition root | `README.md`, CLI documentation |
| `src/rust/bin/repo_guard.rs` | Repository governance CLI | `docs/development/repository-guard.md` |
| `src/install/` | End-user and source-checkout installer wrapper scripts that install release binaries and call the product binary without duplicating native agent logic | `README.md`, `docs/specifications/installation.md`, `docs/development/testing.md` |
| `src/npm/` | Thin npm/npx launcher and tests that download release binaries and exec the product binary without reimplementing RepoGrammar | `README.md`, `docs/specifications/installation.md`, `docs/development/testing.md` |
| `src/rust/test_support/` | Shared deterministic Rust test helpers | `docs/development/testing.md` |
| `src/rust/integration_tests/` | Crate-level Rust integration-style tests | `docs/development/testing.md` |
| `src/workers/typescript/` | Transitional TypeScript semantic worker executable with bounded module/export/package operations, optional compiler-API module resolution, compiler-cross-checked export identity and shared-client binding, and dependency-free structural fallback | `docs/specifications/semantic-workers.md` |
| `src/workers/python/` | CPython `ast`/`symtable`-backed Python worker for private parse-document extraction; bounded `pyproject.toml`/`setup.cfg`/static `setup.py` project-config summaries; and conservative framework-role NDJSON smoke coverage | `docs/specifications/semantic-workers.md`, `docs/specifications/python-analysis.md` |
| `src/protocol/` | Semantic worker protocol notes and schema | `docs/specifications/semantic-workers.md` |
| `src/fixtures/typescript/` | TypeScript/JavaScript source fixtures for Express, Jest/Vitest, Next.js, Fastify, Prisma, Drizzle, and negative TS/JS adapter cases | `docs/development/testing.md` |
| `src/fixtures/python/` | Python v0.1 source fixtures | `docs/development/testing.md`, `docs/specifications/python-analysis.md` |
| `src/fixtures/rust/` | Rust v0.2 self-dogfood source fixtures | `docs/development/testing.md`, `docs/specifications/indexing-pipeline.md` |
| `src/fixtures/java/` | Java v0.2 structural-preview fixtures | `docs/development/testing.md`, `docs/plans/multi-language-expansion-plan.md` |
| `src/fixtures/csharp/` | C# v0.2 structural-preview fixtures | `docs/development/testing.md`, `docs/plans/multi-language-expansion-plan.md` |
| `src/fixtures/cpp/` | C/C++ v0.2 structural-preview fixtures | `docs/development/testing.md`, `docs/plans/multi-language-expansion-plan.md` |

ADR-0020 defines planned ownership, not pre-created runtime modules. Each new
Top-20 language must add its discovery, parsing/frontend, framework or language-
internal pattern registry, persistence/readiness routing, fixtures, and module-
map rows through the atomic submodule commits defined by the active plan. The
final completion audit links and verifies those commits. Until the relevant
implementation lands, the language has no `src/` ownership and must not be
inferred as supported from this map.

Every `src/` change must include a relevant documentation update in the same
commit unless the documentation already precisely describes the resulting
state. The agent must explain that judgment in the final report.
