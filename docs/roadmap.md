# Roadmap

## Bootstrap complete

- Repository governance and mirrored agent contract.
- Rust package skeleton.
- Semantic worker boundary and TypeScript worker protocol placeholder.
- Pattern-family-first CLI command surface and safe command-contract parsing.
- Core domain type placeholders.
- Parser, storage, telemetry, CLI, and MCP boundaries.
- Repository guard and CI quality gates.

## Next phase: parser, semantic worker, and IR design

- Add Tree-sitter dependency only after parser behavior is specified.
- Validate TypeScript worker tooling and package manager before adding
  executable worker code.
- Implement TypeScript and JavaScript code-unit extraction.
- Define the TypeScript semantic worker protocol tests and version policy.
- Convert parser AST into RepoGrammar-owned unified IR.
- Add deterministic fixture coverage under `src/fixtures/`.

## Command implementation path

- Implement repository-local initialization generations and locking.
- Implement `find`, `family`, `explain`, and `check` against real stored
  pattern-family evidence.
- Implement read-only `serve` for MCP.
- Implement safe installer writes only after native agent detection, backups,
  receipts, and MCP self-tests are validated.

## v0.1 language scope

- Official: TypeScript and JavaScript.
- Framework focus: Express, NestJS, React, Jest, and Vitest.
- Goal: validate pattern-family representation with high evidence density.

## Python path

- Pre-v0.2 Python work is experimental only.
- v0.2 target subset: FastAPI, pytest, SQLAlchemy, and Pydantic.
- Django is deferred until after the focused Python backend subset validates the
  language-adapter abstraction.

## Later phases

- Structural normalization and fingerprinting.
- Candidate discovery and representative selection.
- Structural alignment, anti-unification, and clustering.
- SQLite schema and FTS5-backed evidence storage.
- MCP transport implementation for planned tools.
- Framework adapters for Express, NestJS, React, Jest, and Vitest.

## Open design areas

- Unified IR shape and loss model.
- Family support thresholds.
- Counterexample and companion-family representation.
- Benchmark methodology and validation datasets.
