# Changelog

## Unreleased

### Added

- Repository bootstrap for the RepoGrammar Rust-core package layout.
- Layered architecture skeleton for core, ports, application, interfaces, and
  adapters.
- Language-native semantic worker boundary and TypeScript worker protocol
  placeholder.
- Semantic worker v1 protocol tokens, message schemas, and NDJSON fixtures for
  TypeScript semantic facts and unsupported-version fallback.
- Semantic worker v1 request schema and TypeScript request fixture for the
  Rust-to-worker stdin contract.
- Rust-side TypeScript semantic-worker process adapter that writes request JSON
  over stdin, enforces a timeout, validates bounded NDJSON v1 stdout, converts
  fact messages into RepoGrammar-owned semantic facts, and sanitizes
  unavailable, unsupported-version, crash, timeout, and protocol-violation
  failures.
- Dependency-free TypeScript worker executable stub that validates the v1 stdin
  request contract and emits sanitized NDJSON `worker_error` plus
  `end_of_stream` fallback output when compiler-backed semantic analysis is
  unavailable.
- CPython AST Python worker with private parse-document JSON output for the
  Rust parser adapter and semantic-worker-compatible NDJSON framework-role
  heuristic smoke output, without running repository code or provider tools.
- Python worker structural fact output for import bindings, decorator anchors,
  class bases, simple call targets, same-file pytest fixture edges, and typed
  dynamic/unresolved `UNKNOWN` cases, now including path-derived module-name
  anchors, CPython `symtable` structural scope anchors, and a private
  `tomllib` project-config summary mode. Its semantic-worker-compatible
  project mode now resolves only unique repo-local module imports as
  `STRUCTURAL` facts and reports ambiguous/missing repo-local imports or
  `sys.path` mutation as typed `UNKNOWN`. Default parser-mode indexing now
  passes discovered repo-relative `.py` inventory into private parse-document
  requests so source-tied repo-local import facts can be persisted without
  launching a Python semantic worker; oversized context payloads fall back to
  contextless parsing. Default indexing validates and persists parser-origin
  `STRUCTURAL`/`UNKNOWN` facts while keeping them out of family construction
  and CLI/MCP family evidence.
- Default Python indexing discovers root `pyproject.toml` as `python-config`,
  reads it through the Rust source-store path/hash boundary, and persists a
  `project_config` unit with sanitized `PROJECT_CONFIG`/`STRUCTURAL` metadata
  or typed config `UNKNOWN` facts; these records stay out of family construction
  and claim-input readiness.
- Bounded Python exact-anchor support derivation: validated CPython structural
  anchors can now produce separate `DATAFLOW_DERIVED` support facts when their
  target exact-matches the Python framework compatibility table for a unit with
  one framework role. Raw parser facts and framework heuristics remain
  insufficient, project-config facts stay blocked, and Python still requires
  three compatible support members before the EC-MVFI-lite family builder writes
  a family.
- Compact/evidence/deep family output modes for CLI and MCP family detail.
  Compact is now the default and omits evidence records; evidence/deep return
  selected repo-relative evidence metadata under an optional token budget and
  explicitly report that source snippets are not included.
- Greedy family evidence selection metadata for CLI and MCP. Evidence/deep
  output now reports the selector strategy, rough budget satisfaction, covered
  claim labels, and missing requested variation/exception coverage instead of
  preserving raw storage order or inferring unsupported coverage from notes.
- Python v0.1 release fixture smoke coverage for FastAPI, pytest, Pydantic,
  SQLAlchemy, mixed, dynamic-unknown, and low-support examples, plus a test-only
  strong FastAPI semantic-support fixture that validates family reads, stale
  evidence fallback, leakage guards, and a no-worker exact-anchor FastAPI
  positive path without claiming production Python semantic-provider support.
- Metadata-only algorithm paper archive for syntax, semantics, retrieval,
  graph fingerprints, alignment, anti-unification, clustering, evidence
  selection, evaluation, and installer supply-chain references.
- Parallel-agent implementation and post-implementation logic-review
  requirements in the mirrored agent contract.
- Historical TypeScript/JavaScript-first MVP language policy, now superseded by
  ADR-0011's Python-first v0.1 target.
- Pattern-family-first CLI command surface, with CodeGraph-style graph commands
  rejected as top-level v0.1 commands.
- Stable deferred `stats --json` output that exposes metric-kind vocabulary
  without reporting token savings or repository-derived metrics.
- Safe contracts for agent installation, initialization progress, metrics, and
  telemetry consent.
- Repo-local lifecycle implementation for `init`, `uninit`, `status`,
  `doctor`, `unlock`, and `logs`, with parser/mining behavior kept deferred.
- TS/JS file discovery substrate with repo-relative metadata, strict SHA-256
  content hashes, default generated-directory skips, Git ignore checks,
  symlink-escape rejection, size-limit handling, and deterministic skip
  reasons.
- Python `.py` discovery with repo-relative metadata and default skips for
  common Python virtualenv, cache, and dependency directories.
- SQLite storage substrate behind a port, including generation-scoped
  migrations, WAL and foreign-key PRAGMAs, required-table validation,
  repository-relative indexed-file records, active-generation pointer
  activation, and rollback preservation when validation fails.
- Semantic-fact/evidence storage substrate that records validated facts only for
  building generations when evidence matches an indexed code unit, content hash,
  repository-relative path, and byte range.
- Syntax-only `index` and `sync` integration that runs TS/JS discovery, reads
  source through a repo-relative hash-checked boundary, stores repo-relative file
  metadata and structural code-unit records in a new SQLite generation, validates
  it, and atomically activates `.repogrammar/current-generation` without claiming
  semantic-worker-derived facts, mining, query, or family evidence.
- CodeUnit-derived structural IR storage for syntax-only indexing, with one IR
  node per code unit, conservative containment edges, empty IR payloads, and
  same-generation SQLite validation without introducing family claims.
- CPython AST-backed Python structural code-unit extraction for modules,
  functions, async functions, classes, methods, FastAPI route-shaped functions,
  pytest tests/fixtures, Pydantic model-shaped classes, SQLAlchemy model-shaped
  classes, and SQLAlchemy repository method-shaped functions.
- Lightweight TS/JS framework-role fact storage for syntax-origin Express,
  React, and Jest/Vitest code-unit shapes. Stored facts use
  `FRAMEWORK_HEURISTIC` certainty and unresolved-binding assumptions, and do not
  enable pattern-family query commands.
- Lightweight Python framework-role fact storage for CPython AST-origin
  FastAPI, pytest, Pydantic, and SQLAlchemy code-unit shapes. Stored facts use
  `FRAMEWORK_HEURISTIC` certainty and unresolved-binding assumptions, and do not
  enable pattern-family query commands.
- Internal CPython AST parser fact storage for Python import, decorator,
  class-base, call, fixture-edge, and typed dynamic/unresolved `UNKNOWN`
  anchors. Stored facts are source-snippet-free, same-generation validated, and
  blocked from claim-input readiness as insufficient support.
- Opt-in semantic-worker fact ingestion for `index` and `sync` when
  `REPOGRAMMAR_TYPESCRIPT_WORKER` names an explicit worker executable, with
  optional argv supplied by `REPOGRAMMAR_TYPESCRIPT_WORKER_ARGS_JSON`. Accepted
  facts are recorded only through the same-generation code-unit path/hash/range
  storage gate; worker fallback remains syntax-only, and stale or mismatched
  semantic evidence aborts the new generation.
- Active `files` and `units` read paths that return repo-relative
  file-manifest-only or syntax-only indexed-file metadata and code-unit records
  from the validated active generation without source snippets, absolute paths,
  semantic facts, mining, or family evidence claims; the read path opens the
  active generation read-only and revalidates stored paths, hashes, languages,
  unit ids, and byte ranges before returning records.
- Active semantic-fact/evidence read path for future claim builders, with
  read-only active-generation access and validation of stored fact
  kind/certainty tokens, assumptions JSON, repo-relative evidence paths,
  content hashes, code-unit ids, and byte ranges. This remains internal and does
  not expose semantic facts through CLI/MCP query commands or make them
  freshness-validated family evidence.
- Internal active-generation claim-input snapshot over files, code units, IR
  nodes/edges, and semantic facts for future claim builders. It uses the same
  read-only active generation and validation rules, remains unavailable through
  CLI/MCP, and does not create family evidence.
- Internal semantic-fact freshness and claim-input readiness gate that checks
  active fact evidence against current source content hashes, blocks stale or
  missing evidence with typed `StaleEvidence` `UNKNOWN`, and keeps structural,
  framework-heuristic, conflicting, or unknown certainty and `UNKNOWN` fact kind
  out of future family claim inputs.
- Conservative EC-MVFI-lite family builder that groups compatible
  framework-role candidates but writes `DOMINANT_PATTERN` family records only
  when each supporting member has strong same-generation `SEMANTIC` or
  `DATAFLOW_DERIVED` non-framework support.
- FamilyStore-backed query read path for `families`, `family`, `member`,
  `find`, `explain`, and `check`, including stable typed `UNKNOWN` output when
  a readable active generation lacks sufficient family evidence.
- Application-level query preflight contract that keeps pattern-family query
  commands in fallback until a readable active generation exists, then lets the
  query layer return family evidence or typed `UNKNOWN`; `files` and `units`
  remain implemented inventory commands whose missing-index fallback is an
  active-index precondition failure.
- Read-only MCP `repogrammar_context` stdio boundary for `initialize`,
  `tools/list`, `tools/call`, and `shutdown`, reusing the same pattern-family
  query preflight and FamilyStore-backed lookup path without enabling installer
  writes.
- Narrow live `install`/`uninstall` execution for explicit Codex and Claude Code
  MCP targets through native agent CLIs, gated by `--yes`, MCP self-test, and
  RepoGrammar-owned receipts while keeping broad target and unsupported scope
  writes deferred.
- v0.1 TS/JS release fixture corpus and product CLI smoke gate that runs
  `init`, `index`, `files`, `units`, pattern-family query commands, and
  `doctor` JSON paths without upgrading syntax-only evidence into family
  claims.
- Storage-aware `status` and `doctor` reporting for active generation id, schema
  version, WAL journal mode, integrity check, and unhealthy active-generation
  pointer cases.
- Regression coverage for semantic-fact/evidence storage, including fact-token
  validation, sanitized text fields, same-generation code-unit path/hash/range
  evidence, building-only writes, malformed evidence rejection before
  activation, and atomic rollback of failed fact writes.
- v0.1 parallel development planning artifacts for repo-local lifecycle,
  adapter/provider abstraction, Python-first analysis, optional
  CodeGraph provider boundaries, typed UNKNOWN governance, family compression,
  query/MCP, installer, and release-smoke phases.
- Historical experimental Python dogfooding plan and ADR, now superseded by
  ADR-0011.
- Python-first v0.1 analysis specification, implementation plan, ADR, and
  durable memory for FastAPI, pytest, SQLAlchemy, and Pydantic family evidence.
- Python selective analysis cascade ADR and documentation, defining CPython
  `ast`/`symtable`/`tomllib` as the primary frontend, Pyrefly as the future
  primary static provider, Pyright as a selective claim-upgrading cross-check,
  typed canonical framework identities, RightTyper-style observed evidence as
  explicit opt-in only, and precision-first evidence compression.
- Optional CodeGraph provider plan and ADR that allow future auxiliary provider
  evidence without making CodeGraph a dependency or product wrapper.
- UNKNOWN governance specification with typed unknown classes, reason codes,
  claim-blocking semantics, and recovery-action guidance.
- Mirrored `AGENTS.md` and `CLAUDE.md` governance contract.
- Documentation system covering architecture, specifications, development
  workflow, ADRs, roadmap, skills, and memories.
- `repo-guard` repository governance binary with guide sync, source-location,
  skill front matter, required-document, and diff-documentation checks.
- CI workflow for formatting, clippy, tests, repository guard checks, and pull
  request diff documentation gating.

### Changed

- Tightened provenance and semantic-worker evidence docs around strict
  `sha256:<64 hex>` content hashes.
- Replaced the bootstrap SHA-256 implementation with the standard `sha2` crate
  while preserving strict `sha256:<64 hex>` content-hash output and vector
  tests.
- Replaced CLI and progress JSON string assembly with `serde_json` builders for
  parseable machine output without changing human output.
- Centralized lexical repo-relative path validation across source reads,
  storage, SQLite validation, semantic-worker boundaries, schemas, and protocol
  tests.
- Centralized native Git context resolution for discovery ignore checks and
  repo-local `.git/info/exclude` hygiene instead of manually parsing `.git`
  files.
- Bound discovery hashing and source-store reads to `max_file_bytes + 1` bytes
  so oversized files are classified without allocating the full file.
- Changed TS/JS discovery to resolve parent Git worktrees before running
  `check-ignore`, so subdirectory project roots honor parent `.gitignore`
  rules while reports stay project-relative.
- Changed bootstrap manifest validation to parse JSON fields instead of relying
  on literal string layout.
- Documented JSON-parsed semantic-worker protocol fixture tests without
  claiming a running TypeScript worker or runtime indexing integration.
- Documented request-side semantic-worker fixture validation without claiming a
  bundled Node or TypeScript compiler worker.
- Documented that the checked-in TypeScript worker is an unavailable fallback
  stub, not compiler-backed TypeScript analysis, and added its Node smoke test
  to CI.
- Aligned semantic-worker schemas with fixture validation by rejecting blank
  string `target` values.
- Documented `repo-guard` required-document coverage for ADR-0008.
- Documented progress `WorkUnits` constructor validation and CLI missing-index
  fallback/deferred implementation status, including structured `--json`
  fallback output for query commands.
- Documented safe repo-local lifecycle behavior, including state directory
  override validation, Git ignore hygiene, bootstrap manifest status, and
  conservative lock/log handling.
- Documented that discovery-to-storage syntax-only code-unit generations and
  Rust-side semantic-worker process validation are implemented while TypeScript
  compiler worker execution, pattern-family query execution, and family
  evidence remain deferred.
- Documented default `semantic_worker: deferred` index/sync behavior plus
  explicit-worker fallback statuses and `semantic_facts` reporting.
- Documented and tested optional semantic-worker argv configuration through
  `REPOGRAMMAR_TYPESCRIPT_WORKER_ARGS_JSON`, keeping worker startup free of
  shell parsing and PATH-dependent shebang assumptions.
- Documented `rusqlite` as the first production dependency, constrained to the
  persistence adapter for repository-local SQLite storage.
- Documented `serde_json` as a production dependency for runtime
  semantic-worker NDJSON validation in adapter code.
- Hardened Rust-side TypeScript semantic-worker process handling around
  canonical project roots, request size limits, inherited-pipe timeout handling,
  unsupported semantic TypeScript versions, sorted/deduplicated changed-file
  requests, field-name redaction, and source/path-like text rejection.
- Aligned the Rust-side TypeScript semantic-worker request guard with the
  checked-in worker stub's 1 MiB stdin envelope, including the terminating
  newline written after the JSON request object.
- Hardened the Rust-side TypeScript semantic-worker process boundary so timeout
  handling does not wait on descendant-held pipes after killing the worker, and
  empty changed-file requests cannot accept worker facts as repository-wide
  scope.
- Hardened semantic-worker protocol validation so worker errors must still close
  with `end_of_stream`, evidence paths are schema-constrained to repo-relative
  forms, fixture validation rejects unsafe evidence paths and source-like text,
  and the worker stub rejects Windows drive-prefix changed-file paths without
  echoing request data.
- Bumped the pre-release storage schema to version 4 for IR node code-unit
  linkage, semantic-fact/evidence constraints, and family-bound evidence
  constraints; stale schema 1, 2, and 3
  generation databases must be rebuilt rather than silently treated as
  compatible.
- Added the FamilyStore storage substrate for generation-scoped family records,
  members, variation slots, and family-bound evidence without enabling
  pattern-family query commands yet.
- Updated roadmap, product, CLI, MCP, indexing, semantic-worker, storage, and
  domain-model docs to align Python-first v0.1 analysis, optional provider, and
  UNKNOWN boundaries with the current transitional TS/JS indexing baseline.
- Hardened generation-scoped storage writes so indexed files, code units, IR
  nodes/edges, and semantic facts can only be recorded while a generation is
  still building, and active generations cannot be downgraded by stale
  validation or activation handles.
- Hardened `index` and `sync` generation updates with
  `.repogrammar/locks/index.lock`, including active-lock refusal, confirmed
  stale-lock replacement during acquisition, lock acquisition before discovery,
  cleanup of partial metadata writes, cleanup of successful runs, and doctor
  lock-state reporting.
- Implemented `unlock --force --yes` for confirmed stale `index.lock` removal
  while preserving active, unknown, invalid, daemon, and SQLite locks.
- Expanded `repo-guard` required-document coverage to include v0.1 planning
  artifacts, the Python analysis specification and ADR-0011, the substrate
  hardening checkpoint, ADR-0009/ADR-0010, typed UNKNOWN governance, and the
  matching durable memory mirrors.
- Hardened `doctor` lifecycle diagnostics so missing or invalid generated state
  `.gitignore`, Git exclude patterns, init receipts, and root `.gitignore`
  markers are reported without mutating repository state.
- Split `status` schema reporting into explicit `manifest_schema_version` and
  `storage_schema_version` human/JSON fields, removing the ambiguous status
  JSON `schema_version` field.
- Split `doctor` JSON schema reporting into explicit
  `checks.manifest_schema_version` and `checks.storage_schema_version`, removing
  the ambiguous `checks.schema_version` field.
- Tightened EC-MVFI-lite support compatibility so arbitrary semantic facts do
  not prove Express, React, Jest, or Vitest framework-role families.
- Tightened pattern-family query matching so `family` and `member` use exact
  ids, while fuzzy matching remains limited to `find`, `explain`, and `check`
  and rejects short-substring false positives.
- Changed advisory `check` and MCP `check_conformance` success contexts to
  report top-level `CONTEXT_ONLY` with nested advisory `UNKNOWN` instead of
  implying conformance is proven.
- Added public family-evidence freshness checks so stale source hashes block or
  omit CLI/MCP family detail with typed `StaleEvidence` `UNKNOWN`.
- Hardened MCP install self-tests with a bounded timeout that kills and reaps
  hanging self-test processes before native agent configuration.
