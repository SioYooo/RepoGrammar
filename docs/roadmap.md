# Roadmap

## Bootstrap complete

- Repository governance and mirrored agent contract.
- Rust package skeleton.
- Semantic worker boundary plus v1 protocol tokens, schemas, NDJSON fixtures,
  and a Rust-side TypeScript process adapter that validates worker stdout into
  owned semantic facts without treating those facts as family evidence.
- Dependency-free TypeScript worker executable stub that validates stdin and
  reports compiler-backed semantic analysis as unavailable without inspecting
  source files.
- Metadata-only algorithm paper archive under `algorithms/paper/`.
- Pattern-family-first CLI command surface and safe command-contract parsing.
- Repo-local lifecycle for `.repogrammar/`, including init/uninit,
  status/doctor, conservative unlock, redacted log metadata, and Git ignore
  hygiene.
- Git-aware TypeScript and JavaScript file discovery substrate with strict
  hashes, default exclusions, size-limit handling, symlink safety, skip reasons,
  and deterministic ordering.
- Python `.py` discovery and CPython AST-backed structural code-unit extraction
  for modules, functions, async functions, classes, methods, FastAPI
  route-shaped functions, pytest tests/fixtures, Pydantic model-shaped classes,
  SQLAlchemy model-shaped classes, and SQLAlchemy repository method-shaped
  functions, with Python virtualenv/cache/dependency directories skipped.
- CPython `symtable` structural scope anchors, path-derived Python module-name
  anchors, and a private `tomllib` parser mode for sanitized `pyproject.toml`
  summaries; these are structural/context facts and do not prove family claims.
- SQLite storage substrate with generation-scoped migrations, WAL settings,
  foreign-key enforcement, semantic-fact/evidence and family-evidence write
  validation for building generations, validation before activation, and
  rollback preservation for failed generations.
- Syntax-only `index`/`sync` wiring that stores TS/JS discovery metadata and
  structural code units in active SQLite generations without source snippets,
  absolute paths, or family evidence.
- Lightweight TS/JS framework-role fact storage for syntax-origin Express,
  React, and Jest/Vitest code-unit shapes, using `FRAMEWORK_HEURISTIC`
  certainty and unresolved-binding assumptions without enabling family claims.
- Lightweight Python framework-role fact storage for CPython AST-origin
  FastAPI, pytest, Pydantic, and SQLAlchemy code-unit shapes, using
  `FRAMEWORK_HEURISTIC` certainty and unresolved-binding assumptions without
  enabling family claims by themselves.
- Bounded exact-anchor Python support derivation that creates separate
  `DATAFLOW_DERIVED` facts only when validated CPython anchors exact-match the
  Python framework compatibility table for a unit with one framework role.
- Optional command-level semantic-worker ingestion for `index`/`sync` when
  `REPOGRAMMAR_TYPESCRIPT_WORKER` names an explicit executable, with optional
  argv supplied by `REPOGRAMMAR_TYPESCRIPT_WORKER_ARGS_JSON`. Accepted facts must
  match the building generation's indexed code-unit path, hash, and range;
  worker fallback keeps syntax-only indexing, and mismatched evidence aborts the
  new generation.
- Active `files`/`units` inventory reads for repo-relative indexed-file metadata
  and file-manifest-only or syntax-only code units from the validated active
  generation.
- Internal active-generation claim-input snapshot plus semantic-fact freshness
  and readiness gate that compares active fact evidence with current source
  hashes and blocks stale facts, weak certainty, conflicting certainty, and
  `UNKNOWN` fact kinds with typed `UNKNOWN` before any future family claim
  builder can consume them.
- FamilyStore substrate for generation-scoped family records, members,
  variation slots, and family-bound evidence, plus a conservative EC-MVFI-lite
  builder that only populates family rows from repeated compatible candidates
  with strong semantic/dataflow support.
- FamilyStore-backed `families`, `family`, `member`, `find`, `explain`, and
  `check` CLI read paths that return stored family detail or typed
  `UNKNOWN`; missing active indexes still use fallback guidance.
- v0.1 TS/JS release fixture smoke gate that exercises product CLI JSON paths
  across committed Express, React, Jest/Vitest, mixed JS/TS, low-support, and
  mixed-language fixture shapes while preserving syntax-only `UNKNOWN` query
  behavior by default.
- Python v0.1 release fixture smoke gate that exercises committed FastAPI,
  pytest, Pydantic, SQLAlchemy, mixed, dynamic-unknown, and low-support fixture
  shapes through product CLI JSON paths while preserving low-support/dynamic
  `UNKNOWN` query behavior.
- No-worker exact-anchor FastAPI family smoke that proves derived support can
  reach the EC-MVFI-lite family read path without claiming provider-backed
  Python semantics.
- Test-only strong FastAPI fixture support that injects compatible `SEMANTIC`
  facts through the existing worker boundary to prove family reads, stale
  evidence fallback, and leakage guards without claiming production Python
  semantic-provider support.
- Storage-aware `status`/`doctor` reporting for active generation health,
  schema version, journal mode, integrity checks, and invalid active-generation
  pointers.
- Core domain type placeholders.
- Parser, storage, telemetry, CLI, and MCP boundaries.
- Repository guard and CI quality gates.

## Next implementation plan

RepoGrammar v0.1 now follows a product-first Python analysis phase plan.
The detailed coordination artifacts are
`docs/plans/v0.1-parallel-development-plan.md`, the Python implementation plan
in `docs/plans/python-v0.1-implementation-plan.md`, and the immediate
hardening checkpoint in `docs/plans/v0.1-substrate-hardening-checkpoint.md`.

The substrate hardening checkpoint now covers generation immutability,
lifecycle doctor hygiene, repo-guard coverage, bounded reads, parent Git ignore
behavior, manifest/status/doctor schema clarity, semantic-worker request
limits, index-lock cleanup/unlock gates, conservative family query reads, and
read-only MCP serving over `repogrammar_context`. With ADR-0011 and ADR-0012
accepted, the next analysis implementation slice should pivot to Python v0.1
through a claim-driven selective cascade while preserving the family claim input
contract and EC-MVFI-lite readiness gates rather than exposing substrate records
as overclaimed user-visible family claims.

1. Phase 1: repo-local lifecycle.
2. Phase 1.5: language and provider abstraction.
3. Phase 1.6: Python v0.1 analysis boundary.
4. Phase 1.7: optional CodeGraph provider boundary.
5. Phase 1.8: UNKNOWN governance.
6. Phase 2: file discovery for Python v0.1 plus transitional TS/JS substrate.
7. Phase 3: storage and generation.
8. Phase 4: parsers.
9. Phase 5: semantic and framework facts.
10. Phase 6: pattern-family compression / EC-MVFI-lite.
11. Phase 7: query CLI and MCP.
12. Phase 8: install and uninstall.
13. Phase 9: release fixtures and smoke gate.

The current codebase has completed the repo-local lifecycle substrate,
TS/JS and Python `.py` discovery, generation-scoped SQLite storage, syntax-only
TS/JS and CPython AST-backed Python code-unit indexing, syntax-origin
framework-role fact storage, CodeUnit-derived IR node/containment-edge storage,
active
file-manifest-only or syntax-only files/units inventory reads, FamilyStore-backed
pattern-family query read paths with typed `UNKNOWN`, internal active
claim-input snapshot reads,
semantic-fact/evidence storage substrate, the Rust-side semantic-worker
process/NDJSON validation boundary, and opt-in command-level semantic-fact
ingestion through the storage gate, bounded exact-anchor Python support
derivation, plus an internal semantic-fact file-hash freshness and claim-input
readiness gate, and read-only MCP serving through the
same query layer, narrow global Codex/Claude MCP installer writes, and the
v0.1 TS/JS and Python release fixture smoke gates. Continue one boundary at a time:
Python repo-local module/import graph, safe project configuration, provider
provenance/cache keys, bounded framework-role propagation, or richer
family-claim gates. Keep
syntax-only code units, structural IR, syntax-origin framework-role facts, and
weak stored semantic facts out of family claims unless the conservative builder
has stronger compatible support.

Do not advance full mining or broad installer writes until parser output,
family-evidence read paths, full public freshness checks, MCP self-tests, and
evidence contracts remain validated together.

## Command implementation path

- Add full repository/worktree freshness metadata for stored family evidence.
- Persist safe `pyproject.toml`/pytest configuration facts before
  provider-backed claim upgrades; default indexing already passes discovered
  `.py` inventory to the parser for source-tied repo-local import facts.
- Add typed IR attributes only after CodeUnit-derived IR nodes and containment
  edges remain stable.
- Extend `find`, `family`, `explain`, and `check` beyond the current
  EC-MVFI-lite/typed-UNKNOWN slice as stronger semantic evidence becomes
  available.
- Harden read-only MCP self-tests for the default `repogrammar_context` tool and
  missing/stale-index fallback semantics.
- Broaden safe installer writes beyond explicit global Codex/Claude MCP
  registration only after native agent detection, backups, receipts, and MCP
  self-tests are validated for the expanded scope.

## v0.1 language scope

- Official target: Python.
- Framework focus: FastAPI, pytest, SQLAlchemy, and Pydantic.
- Goal: validate pattern-family representation with high evidence density in
  Python backend and test repositories.

## Python path

- Python is the v0.1 implementation target.
- Use the method stack in `docs/specifications/python-analysis.md` and
  `docs/decisions/ADR-0012-python-selective-analysis-cascade.md`.
- The first CPython structural slice is implemented, including
  worker-local structural anchors for imports, decorators, class bases, simple
  calls, same-file pytest fixture edges, and typed dynamic/unresolved
  `UNKNOWN`, plus path-derived module names, CPython `symtable` scope anchors,
  and private `tomllib` project-config summaries. The semantic-worker-compatible
  project mode now builds a bounded module graph for requested `.py` files and
  emits structural repo-local import facts only for unique module-level matches,
  resolves requested-project `conftest.py` fixture names through pytest's
  directory hierarchy, and emits typed `UNKNOWN` for ambiguous/missing
  repo-local imports and `sys.path` mutation. Default indexing now passes
  discovered `.py` inventory plus bounded discovered `conftest.py` contents to
  the private parser request so source-tied repo-local import facts and
  parent-directory pytest fixture-edge facts can be persisted through the Rust
  storage/readiness gate. Project-config summaries are default indexing
  structural context but not claim evidence. The current
  application layer can derive `DATAFLOW_DERIVED` support from exact canonical
  anchors under the Python support >= 3 gate. Next Python slices should escalate
  only plausible family candidates to Pyrefly and use Pyright only for
  claim-upgrading cross-checks.
- First target subset: FastAPI, pytest, SQLAlchemy, and Pydantic.
- Django is deferred until after the focused Python backend subset validates the
  language-adapter abstraction.
- Whole-program call graphs, sound full Python semantics, default runtime
  tracing, and LLM-derived family evidence are rejected for v0.1.

## TypeScript/JavaScript path

- Existing TS/JS discovery, syntax extraction, framework-role facts, worker
  protocol scaffolding, and fixtures remain transitional substrate.
- Production-quality TS/JS family evidence is deferred until after Python v0.1
  unless a later ADR changes scope again.

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
- Implement Python structural-anchor persistence, repo-local import resolution,
  safe project configuration, pytest fixture graph recovery, usage propagation,
  provider-backed canonical target evidence, and target-centered call recovery
  in scoped phases.
- Validate TypeScript worker tooling and package manager before adding
  TypeScript compiler API dependencies or semantic fact emission.
- Expand TypeScript and JavaScript code-unit extraction beyond the bootstrap
  syntax-only extractor where the extra precision is justified.
- Extend the TypeScript semantic worker version policy, generation matching, and
  storage tests before consuming worker facts in indexing.
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
- Framework adapters for FastAPI, pytest, SQLAlchemy, and Pydantic first;
  Express, NestJS, React, Jest, and Vitest move to the TS/JS follow-up path.

## Open design areas

- Unified IR shape and loss model.
- Family support thresholds.
- Counterexample and companion-family representation.
- Benchmark methodology and validation datasets.
