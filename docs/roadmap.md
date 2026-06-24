# Roadmap

## Bootstrap complete

- Repository governance and mirrored agent contract.
- Rust package skeleton.
- Semantic worker boundary plus v1 protocol tokens, schemas, and NDJSON
  fixtures for a TypeScript fact and unsupported-version fallback.
- Metadata-only algorithm paper archive under `algorithms/paper/`.
- Pattern-family-first CLI command surface and safe command-contract parsing.
- Repo-local lifecycle for `.repogrammar/`, including init/uninit,
  status/doctor, conservative unlock, redacted log metadata, and Git ignore
  hygiene.
- Git-aware TypeScript and JavaScript file discovery substrate with strict
  hashes, default exclusions, size-limit handling, symlink safety, skip reasons,
  and deterministic ordering.
- SQLite storage substrate with generation-scoped migrations, WAL settings,
  foreign-key enforcement, validation before activation, and rollback
  preservation for failed generations.
- File-manifest-only `index`/`sync` wiring that stores TS/JS discovery metadata
  in active SQLite generations without source snippets, absolute paths, parser
  facts, code units, or family evidence.
- Storage-aware `status`/`doctor` reporting for active generation health,
  schema version, journal mode, integrity checks, and invalid active-generation
  pointers.
- Core domain type placeholders.
- Parser, storage, telemetry, CLI, and MCP boundaries.
- Repository guard and CI quality gates.

## Next phase: parser-backed code-unit extraction prerequisites

- Keep the file-manifest-only generation contract stable while adding parser
  dependencies and fixtures.
- Introduce syntax parsing and code-unit extraction behind ports without using
  Tree-sitter as semantic proof.
- Keep semantic-worker execution, mining, query execution, and MCP transport
  deferred until parser output and storage boundaries are validated together.

## Command implementation path

- Add parser-backed code-unit persistence after metadata-only generations remain
  stable.
- Implement `find`, `family`, `explain`, and `check` against real stored
  pattern-family evidence.
- Implement read-only `serve` for MCP with the default `repogrammar_context`
  tool and missing/stale-index fallback semantics.
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

- Add Tree-sitter dependency only after discovery, storage, and lifecycle
  integration boundaries are tested.
- Validate TypeScript worker tooling and package manager before adding
  executable worker code.
- Implement TypeScript and JavaScript code-unit extraction.
- Define the TypeScript semantic worker protocol tests and version policy.
- Verify archive metadata, licenses, and SHA-256 values before committing any
  downloaded paper or HTML artifact.
- Convert parser AST into RepoGrammar-owned unified IR.
- Add deterministic fixture coverage under `src/fixtures/`.
- Structural normalization and fingerprinting.
- Candidate discovery and representative selection.
- Structural alignment, anti-unification, and clustering.
- Optional watcher/daemon support that marks affected families stale and lazily
  recomputes instead of eagerly rebuilding the whole repository.
- MCP transport implementation for planned tools.
- Framework adapters for Express, NestJS, React, Jest, and Vitest.

## Open design areas

- Unified IR shape and loss model.
- Family support thresholds.
- Counterexample and companion-family representation.
- Benchmark methodology and validation datasets.
