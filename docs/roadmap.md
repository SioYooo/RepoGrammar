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
- Core domain type placeholders.
- Parser, storage, telemetry, CLI, and MCP boundaries.
- Repository guard and CI quality gates.

## Next phase: file discovery and storage substrate

- Implement Git-aware TypeScript and JavaScript file discovery.
- Skip `.repogrammar/`, `.repogrammar-*`, dependency, build, coverage, cache,
  virtual environment, and generated-output directories.
- Respect Git ignore rules where Git is available and report a safe warning
  when it is not.
- Enforce the default 1 MB file-size limit and strict SHA-256 content hashes.
- Canonicalize paths, reject symlink escapes, record skip reasons, and return
  deterministic ordering.
- Design SQLite migrations and generation activation before storing indexed
  facts.

## Command implementation path

- Implement SQLite-backed generations, migrations, WAL settings, rollback, and
  freshness validation after discovery is safe.
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

- Add Tree-sitter dependency only after discovery and storage boundaries are
  tested.
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
