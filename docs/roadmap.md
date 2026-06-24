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
- Syntax-only `index`/`sync` wiring that stores TS/JS discovery metadata and
  structural code units in active SQLite generations without source snippets,
  absolute paths, semantic facts, or family evidence.
- Storage-aware `status`/`doctor` reporting for active generation health,
  schema version, journal mode, integrity checks, and invalid active-generation
  pointers.
- Core domain type placeholders.
- Parser, storage, telemetry, CLI, and MCP boundaries.
- Repository guard and CI quality gates.

## Next implementation plan

RepoGrammar v0.1 now follows a product-first and dogfooding-aware phase plan.
The detailed coordination artifact is
`docs/plans/v0.1-parallel-development-plan.md`.

1. Phase 1: repo-local lifecycle.
2. Phase 1.5: language and provider abstraction.
3. Phase 1.6: experimental Python dogfooding boundary.
4. Phase 1.7: optional CodeGraph provider boundary.
5. Phase 1.8: UNKNOWN governance.
6. Phase 2: file discovery for official TS/JS and experimental Python.
7. Phase 3: storage and generation.
8. Phase 4: parsers.
9. Phase 5: semantic and framework facts.
10. Phase 6: pattern-family compression / EC-MVFI-lite.
11. Phase 7: query CLI and MCP.
12. Phase 8: install and uninstall.
13. Phase 9: release fixtures and smoke gate.

The current codebase has completed the repo-local lifecycle substrate,
TS/JS discovery, generation-scoped SQLite storage, and syntax-only code-unit
indexing. The next implementation slice should refine one boundary at a time:
storage read paths and freshness, parser-to-IR, or semantic-worker execution.
Keep syntax-only code units structural and non-semantic while validating the next
boundary.

Do not advance mining, query execution, or MCP serving until parser output,
storage read paths, freshness checks, and evidence contracts are validated
together.

## Command implementation path

- Add semantic-worker execution or parser-to-IR storage after syntax-only
  generations remain stable.
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
- Python dogfooding before v0.2 is for internal adapter and `UNKNOWN`
  validation only. It must not change the official v0.1 TS/JS support claim.

## CodeGraph provider path

- CodeGraph is a possible optional lower-layer graph/navigation provider.
- RepoGrammar must work without CodeGraph and must not become a CodeGraph
  wrapper.
- CodeGraph-derived facts, if accepted later, can support candidate retrieval,
  dependency context, and graph-neighborhood evidence. They cannot independently
  prove pattern-family membership.
- RepoGrammar must not create, initialize, modify, or delete `.codegraph/`.

## UNKNOWN governance path

- `UNKNOWN` is a first-class typed analysis result.
- Dynamic behavior, missing project configuration, missing dependencies,
  conflicting facts, stale evidence, and insufficient support must not be
  guessed away.
- Some unknowns block only specific claims, not necessarily every family
  classification.
- See `docs/specifications/unknowns.md` for the current taxonomy and recovery
  guidance.

## Later phases

- Add Tree-sitter dependency only when the parser adapter scope, fixture set, and
  dependency policy are reviewed.
- Validate TypeScript worker tooling and package manager before adding
  executable worker code.
- Expand TypeScript and JavaScript code-unit extraction beyond the bootstrap
  syntax-only extractor where the extra precision is justified.
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
