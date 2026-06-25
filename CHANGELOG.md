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
- Metadata-only algorithm paper archive for syntax, semantics, retrieval,
  graph fingerprints, alignment, anti-unification, clustering, evidence
  selection, evaluation, and installer supply-chain references.
- Parallel-agent implementation and post-implementation logic-review
  requirements in the mirrored agent contract.
- TypeScript/JavaScript-first MVP language policy with Python deferred to a
  focused second-language phase.
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
- Application-level query preflight contract that keeps pattern-family query
  commands in fallback until family evidence exists while treating `files` and
  `units` as implemented inventory commands whose missing-index fallback is an
  active-index precondition failure.
- Storage-aware `status` and `doctor` reporting for active generation id, schema
  version, WAL journal mode, integrity check, and unhealthy active-generation
  pointer cases.
- Regression coverage for semantic-fact/evidence storage, including fact-token
  validation, sanitized text fields, same-generation code-unit path/hash/range
  evidence, building-only writes, malformed evidence rejection before
  activation, and atomic rollback of failed fact writes.
- v0.1 parallel development planning artifacts for repo-local lifecycle,
  adapter/provider abstraction, experimental Python dogfooding, optional
  CodeGraph provider boundaries, typed UNKNOWN governance, family compression,
  query/MCP, installer, and release-smoke phases.
- Experimental Python dogfooding plan and ADR that keep Python outside official
  v0.1 production support while targeting FastAPI, pytest, SQLAlchemy, and
  Pydantic validation.
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
- Hardened semantic-worker protocol validation so worker errors must still close
  with `end_of_stream`, evidence paths are schema-constrained to repo-relative
  forms, fixture validation rejects unsafe evidence paths and source-like text,
  and the worker stub rejects Windows drive-prefix changed-file paths without
  echoing request data.
- Bumped the pre-release storage schema to version 3 for IR node code-unit
  linkage and semantic-fact/evidence constraints; stale schema 1 and 2
  generation databases must be rebuilt rather than silently treated as
  compatible.
- Updated roadmap, product, CLI, MCP, indexing, semantic-worker, storage, and
  domain-model docs to align Python dogfooding, optional provider, and UNKNOWN
  boundaries with the current syntax-only indexing baseline.
- Hardened generation-scoped storage writes so indexed files, code units, IR
  nodes/edges, and semantic facts can only be recorded while a generation is
  still building, and active generations cannot be downgraded by stale
  validation or activation handles.
- Expanded `repo-guard` required-document coverage to include v0.1 planning
  artifacts, the substrate hardening checkpoint, ADR-0009/ADR-0010, typed
  UNKNOWN governance, and the matching durable memory mirrors.
- Hardened `doctor` lifecycle diagnostics so missing or invalid generated state
  `.gitignore`, Git exclude patterns, init receipts, and root `.gitignore`
  markers are reported without mutating repository state.
