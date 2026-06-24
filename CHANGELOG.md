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
- Metadata-only algorithm paper archive for syntax, semantics, retrieval,
  graph fingerprints, alignment, anti-unification, clustering, evidence
  selection, evaluation, and installer supply-chain references.
- Parallel-agent implementation and post-implementation logic-review
  requirements in the mirrored agent contract.
- TypeScript/JavaScript-first MVP language policy with Python deferred to a
  focused second-language phase.
- Pattern-family-first CLI command surface, with CodeGraph-style graph commands
  rejected as top-level v0.1 commands.
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
- Syntax-only `index` and `sync` integration that runs TS/JS discovery, reads
  source through a repo-relative hash-checked boundary, stores repo-relative file
  metadata and structural code-unit records in a new SQLite generation, validates
  it, and atomically activates `.repogrammar/current-generation` without claiming
  semantic-worker, mining, query, or family evidence.
- Storage-aware `status` and `doctor` reporting for active generation id, schema
  version, WAL journal mode, integrity check, and unhealthy active-generation
  pointer cases.
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
- Aligned semantic-worker schemas with fixture validation by rejecting blank
  string `target` values.
- Documented `repo-guard` required-document coverage for ADR-0008.
- Documented progress `WorkUnits` constructor validation and CLI missing-index
  fallback/deferred implementation status, including structured `--json`
  fallback output for query commands.
- Documented safe repo-local lifecycle behavior, including state directory
  override validation, Git ignore hygiene, bootstrap manifest status, and
  conservative lock/log handling.
- Documented that discovery-to-storage syntax-only code-unit generations are
  implemented while semantic-worker execution, query execution, and family
  evidence remain deferred.
- Documented `rusqlite` as the first production dependency, constrained to the
  persistence adapter for repository-local SQLite storage.
- Updated roadmap, product, CLI, MCP, indexing, semantic-worker, storage, and
  domain-model docs to align Python dogfooding, optional provider, and UNKNOWN
  boundaries with the current syntax-only indexing baseline.
