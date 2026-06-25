# Testing Policy

All test source lives under `src/`.

## Locations

- Module-level tests use `#[cfg(test)] mod tests` beside implementation.
- Crate-level Rust integration-style tests live in `src/rust/integration_tests/`.
- Shared deterministic Rust helpers live in `src/rust/test_support/`.
- Source fixtures live in `src/fixtures/`.

Root `tests/`, `benches/`, `examples/`, and `scripts/` directories are not
allowed.

## Test properties

- Tests must be deterministic and independent of execution order.
- Tests must not access the network by default.
- Temporary directories must be unique and cleaned up.
- Tests must not modify real repository files unless the test is explicitly
  exercising a temporary copy.
- CLI not-implemented behavior must be stable and asserted.
- CLI missing-index fallback tests must cover both human-readable output and
  `--json` output for the query command surface.
- Repo-local lifecycle tests must use temporary workspaces and cover init
  layout, idempotent repair, Git exclude hygiene, optional root `.gitignore`
  marker writes, `REPOGRAMMAR_DIR` override validation, symlink/file conflicts,
  human and JSON status/doctor output, explicit status
  `manifest_schema_version` and `storage_schema_version` fields, explicit
  doctor `checks.manifest_schema_version` and `checks.storage_schema_version`
  fields, no ambiguous status or doctor `schema_version` fields, JSON-parsed
  manifest validation with reordered valid fields and invalid required fields,
  corrupted manifests,
  missing subdirs, diagnostic-only doctor findings for missing or invalid
  `.repogrammar/.gitignore`, `.git/info/exclude`, root `.gitignore` markers,
  and `receipts/init.json`,
  `uninit --yes`, unlock inspection without `--force --yes`, confirmed stale
  `index.lock` removal with `--force --yes`, active/unknown/invalid lock
  refusal, daemon/SQLite lock preservation, and redacted logs metadata.
- File discovery tests must use temporary workspaces and cover TS/JS inclusion,
  Python `.py` inclusion, unsupported module extensions,
  default dependency/build/generated/state-dir
  exclusions, Git-ignored files when Git is available, safe Git-unavailable
  warnings, parent Git worktree ignore rules for subdirectory projects, the
  inclusive 1 MB size boundary, oversized skips, strict SHA-256 hash
  generation, bounded max-plus-one content reads for hashing,
  deterministic ordering, symlink escape skips, invalid roots, and absence of
  source snippets or absolute paths in reports. Python discovery coverage must
  include common virtualenv/cache/dependency directories such as `venv`,
  `__pycache__`, `.pytest_cache`, `.mypy_cache`, `.ruff_cache`, and
  `site-packages`.
- SQLite storage tests must use temporary workspaces and cover idempotent
  migrations, required-table validation, WAL and foreign-key PRAGMAs,
  foreign-key enforcement, activation pointer validation, preservation of the
  previous active generation after failed validation, repository-relative
  indexed-file paths, semantic-fact/evidence storage with same-generation
  code-unit path/hash/range validation, IR node/edge storage with
  same-generation code-unit/node validation, malformed semantic evidence and IR
  graph rejection before activation, atomic rollback of failed fact writes,
  building-generation write gates for indexed files, code units, IR nodes/edges,
  and semantic facts, validation/activation transition guards that do not
  downgrade active generations, read-only active `files`/`units` listing order
  and tamper rejection, read-only active IR and semantic-fact listing with
  validation and tamper rejection, internal active claim-input snapshot reads
  from one validated generation, snapshot tamper rejection across files, units,
  IR, and semantic facts, and rejection of symlinked or malformed
  active-generation pointers.
- Syntax-only `index` and `sync` tests must cover initialized-state
  requirements, human and JSON output, generation activation, positive code-unit
  extraction and storage, source ranges, language/kind/content-hash metadata,
  malformed syntax returning partial units plus diagnostics, unsupported or
  invalid source behavior, generation preservation after source/parser/storage
  failure, `index.lock` acquisition before discovery and generation
  preparation, no discovery when lock acquisition fails, active-lock refusal for
  both `index` and `sync`, confirmed stale-lock replacement, failed lock
  metadata write cleanup, successful lock cleanup, status/doctor storage and
  lock health, corrupt
  manifests, missing state subdirectories without implicit repair, active
  `files`/`units` human and JSON output, no-active-generation fallback, broken
  active-generation pointers, product runtime wiring, and absence of source
  snippets or absolute paths in CLI output and stored metadata.
- Family storage tests must cover generation-scoped family records, members,
  variation slots, family-bound evidence, building-only writes, non-`UNKNOWN`
  family validation requiring evidence, active-generation list/show reads,
  tampered family/evidence row rejection, and no source snippet or absolute path
  leakage.
- Family builder and query tests must cover framework-heuristic-only groups
  staying `UNKNOWN`, semantic/dataflow-supported repeated candidates becoming
  eligible family records, role-incompatible semantic facts staying
  insufficient, future Python provider-origin facts staying subject to exact
  canonical target compatibility and same-code-unit path/hash/range evidence,
  no-family active generations returning typed
  `InsufficientSupport`, exact family/member lookup versus fuzzy
  find/explain/check lookup, short-substring false-match rejection, stale
  family-evidence refusal with `StaleEvidence`, compact/evidence/deep output
  mode behavior, token-budget validation, greedy evidence coverage metadata,
  missing variation/exception coverage reporting, JSON/human CLI output,
  advisory `check` behavior, and absence of source snippets or absolute paths.
- MCP serve tests must cover the single default `repogrammar_context` tool
  schema, accepted operation enum, unknown tool and operation rejection,
  missing-state fallback without implicit repo-local state creation,
  no-active-generation fallback, active-generation typed `UNKNOWN`, advisory
  `check_conformance` with `CONTEXT_ONLY` context success when conformance is
  unproven, exact `show_family` target handling, compact/evidence/deep output
  mode serialization, token-budget validation, metadata-only greedy evidence
  selection, missing variation/exception coverage reporting, JSON-RPC
  initialize/tools/list/tools/call/shutdown handling, and absence of source
  snippets or absolute paths.
- Installer live-write tests must cover `--yes` gating, MCP self-test before
  native configuration, hanging MCP self-test timeout/kill behavior,
  unsupported broad `--target all`, unsupported native scopes, receipt writing,
  receipt-write rollback, receipt-owned uninstall, foreign receipt refusal, and
  no `.repogrammar/` mutation.
- Optional semantic-worker indexing tests must cover explicit opt-in wiring,
  non-empty discovered-file request scope, deterministic fact recording through
  the same-generation storage gate, syntax-only fallback for unavailable,
  unsupported-version, timeout, crash, and protocol-violation worker results,
  sanitized fallback warnings, and preservation of the previous active
  generation when accepted worker output conflicts with indexed
  path/hash/range evidence.
- Protocol fixture tests must parse fixture lines as JSON before checking
  message types, fallback payloads, repository-relative evidence paths,
  sanitized target/note text, evidence fields, and strict content-hash formats.
  Semantic fact target tests must cover invalid blank targets, accepted `null`
  targets, and accepted non-blank targets.
- Semantic-worker request fixture tests must parse the stdin request as JSON and
  reject wrong protocol versions, missing required fields, non-object payloads,
  non-absolute project roots, duplicate changed files, absolute paths,
  traversal, Windows absolute paths, URI-like paths, and backslash paths.
- Runtime semantic-worker adapter tests must cover valid fact/progress/EOS
  output, malformed JSON, missing EOS, invalid hashes, blank targets,
  impossible work counts, absolute or URI evidence paths, unsupported snippet
  fields, sanitized worker-error mapping, worker crashes, timeouts, oversized
  output, invalid request paths, unrequested fact paths, and relative executable
  rejection. They must also cover inherited-pipe timeout handling, unsupported
  field-name redaction, invalid/symlink project roots, the shared 1 MiB stdin
  request envelope with limit-plus-one rejection, empty changed-file requests
  that return facts, worker-error output that omits `end_of_stream`,
  unsupported TypeScript versions with semantic certainty, sorted/deduplicated
  request files, and rejected absolute-path or source-like free text.
- TypeScript worker executable tests must run the dependency-free worker stub
  through Node, validate parseable NDJSON `worker_error` plus `end_of_stream`
  output for valid requests, accept large changed-file requests below the
  shared 1 MiB stdin envelope, reject malformed requests, and prove request
  paths are not echoed in errors.
- Python worker executable tests must run the checked-in CPython AST worker
  through `python3`, validate private parse-document JSON output, syntax-error
  diagnostics, generic `module`/`function`/`async_function`/`class`/`method`
  code-unit output, parse-document structural facts for imports/decorators/class
  bases/calls/pytest test anchors/fixture edges, bounded parse-document
  `conftest.py` fixture hierarchy context, FastAPI route/response-model/static
  dependency-target/dependency-call/error/status-code anchors, static FastAPI
  request body/path/query/header/cookie marker anchors, pytest fixture
  decorator aliases, pytest parametrize decorator and literal argument anchors,
  direct-parametrize-over-fixture precedence, indirect parametrize remaining
  typed `PytestFixtureInjection` `UNKNOWN`, Pydantic field, field-type,
  model-config, nested Config, computed-field, validator, and model-validator
  anchors,
  semantic-worker-compatible NDJSON structural facts plus framework-role output,
  requested-project `conftest.py` fixture hierarchy edges, file-local FastAPI
  router/app alias propagation with same-name reassignment invalidation, typed
  same-function FastAPI service-call context anchors with reassignment
  invalidation, typed `UNKNOWN` output for dynamic decorators, monkey patches,
  dynamic calls, unsafe or nonliteral `importlib.import_module(...)` calls,
  `sys.path.append`/`sys.path.insert` import-environment mutation, and
  unresolved cases, plus safe literal dynamic-import anchors and plain
  `getattr(...)` assignments that do not become dynamic call-target UNKNOWNs,
  oversized request
  rejection, unsafe path and symlink-escape rejection, bounded semantic-mode
  source reads, and absence of source snippets, absolute paths, or unsafe
  dynamic-import literal targets.
- Transitional release fixture smoke tests currently copy committed TS/JS source
  fixtures from `src/fixtures/typescript/release/v0_1/` and Python source
  fixtures from `src/fixtures/python/release/v0_1/` into temporary workspaces and
  run the product CLI through `init`, `index`, `files`, `units`, `families`,
  `family`, `member`, `find`, `explain`, `check`, and `doctor` JSON paths.
  Default smoke expectations must remain conservative: syntax-only indexing
  succeeds, machine output is parseable and does not leak source snippets,
  parser/provider internals, or absolute paths, low-support and dynamic cases
  return typed `UNKNOWN`/`InsufficientSupport`, and positive family cases require
  either exact-anchor derived `DATAFLOW_DERIVED` support or an explicitly
  injected compatible semantic/dataflow support fixture. Positive family smoke
  tests must cover compact default output without evidence records, `member`,
  `find`, `explain`, and advisory `check` read paths, token-budget auto
  evidence mode, explicit compact override, explicit evidence and deep modes
  with repo-relative metadata only, MCP parity for supported operations, and
  stale-evidence `UNKNOWN` after source mutation or deletion. Python
  exact-anchor variation smoke must prove that `--include-variations` selects
  explicit `variation` evidence metadata only after the family is already
  ready, while exception coverage remains missing.
- Python v0.1 tests must cover the implemented CPython `ast` frontend output,
  FastAPI, pytest, SQLAlchemy, and Pydantic structural positives, Python
  language/kind token stability, product `index`/`units` smoke coverage,
  path-derived module-name anchors, CPython `symtable` scope anchors, private
  `tomllib` project-config summaries, default-index persistence of root
  `pyproject.toml` as `python-config`/`project_config` structural context or
  typed config `UNKNOWN`, semantic-worker-compatible project-mode repo-local
  import resolution for unique module-level matches, ambiguous or missing
  repo-local import `UNKNOWN`, `sys.path` mutation
  `RuntimeDependencyInjection` `UNKNOWN`, dynamic import, dynamic call-target,
  dynamic decorator, and monkey-patch `UNKNOWN` facts through product indexing,
  persisted parser-origin structural
  facts including FastAPI response-model/static dependency-target/
  dependency-call/error/status-code anchors, static FastAPI request
  body/parameter anchors, pytest parametrize, and Pydantic validator anchors,
  typed `UNKNOWN`,
  persisted project-config facts staying out of claim-input readiness,
  heuristic framework-role facts staying out of family claims, raw parser-origin
  facts staying out of family construction,
  exact-anchor derived support facts producing no-worker direct FastAPI,
  FastAPI alias, pytest test, pytest fixture, Pydantic model/settings,
  SQLAlchemy model-field, and SQLAlchemy session/repository families only when
  Python support reaches three members, their CLI/MCP metadata-only
  compact/evidence/deep query paths,
  default-index parser context receiving discovered `.py` inventory, sanitized
  root `pyproject.toml` source roots from parser/tomllib project-config facts,
  and bounded discovered `conftest.py` contents,
  exact-anchor target variation coverage only for already-ready Python families,
  FastAPI response-model/static dependency-target/dependency-call/error/status-code
  context anchors and static request body/parameter anchors staying out of
  support derivation, claim-input readiness, and support-target variation metadata,
  pytest fixture-edge and parametrize-argument anchors
  staying out of family support, SQLAlchemy relationship and Session.add
  structural anchors staying out of family support and claim-input readiness, SQLAlchemy
  `Session.commit`/`Session.rollback`/`Session.scalar`/`Session.scalars` and
  async equivalents becoming exact repository-method anchors, bounded
  SQLAlchemy `self.session`/`self.db` role propagation from `__init__`, and
  reassigned receivers not becoming exact session-call anchors, Pydantic field,
  field-type, model-config, nested Config, computed-field, field-validator,
  legacy validator, and model-validator anchors staying out of support derivation,
  FastAPI service-call context anchors staying out of support derivation,
  low-support and dynamic Python release fixtures preserving `UNKNOWN`, test-only strong
  support facts proving explicit worker family read paths only when compatible
  `SEMANTIC` evidence is injected, stale evidence fallback, and typed canonical
  framework identities rather than framework-name substring matching. Human
  family query output and MCP JSON-RPC serve output must preserve typed stale
  `UNKNOWN` reason, affected claim, and recovery text rather than replacing it
  with generic insufficient support. Future
  Python slices must add coverage for
  Tree-sitter
  fallback not creating family claims, Pyrefly/Pyright disagreement becoming
  `ConflictingFacts`, provider provenance/freshness cache keys, and typed
  `UNKNOWN` for pytest fixture injection, missing dependencies, stale evidence,
  and dynamic forms beyond the current import/path/decorator/call/monkey-patch
  slice. Tests must not expect future cross-checked or observed
  certainty tokens until Rust domain, protocol, storage, CLI, MCP, and schema
  support are added.
- Optional provider tests, once added, must cover provider absent, present,
  stale, and conflicting states without making CodeGraph or any other provider
  required for default tests.
- Python provider port tests must cover candidate repo-relative path validation,
  deterministic candidate ordering, duplicate candidate rejection, required
  provider provenance/cache-key dimensions, sanitized metadata, and recoverable
  provider-unavailable `UNKNOWN` output without executing Pyrefly, Pyright,
  RightTyper, or repository code.
- Python provider-planner tests must cover grouping only plausible candidate
  sets by supported code-unit kind and exact framework role, Python support
  threshold enforcement, ambiguous-role and low-support skips, deterministic
  request ordering, claim-specific blocking `UNKNOWN` skips for import
  resolution, framework identity, and pytest fixture binding, non-blocking
  `UNKNOWN` preservation, unsafe path rejection, invalid metadata rejection,
  active-generation snapshot planning without mutation, and no family claim or
  CLI/MCP behavior change from planning alone.
- UNKNOWN governance tests must cover blocking, non-blocking, recoverable, and
  irreducible unknowns when those classes enter Rust, CLI, MCP, storage, or
  metrics code.
- Stats CLI tests must cover the human deferred message, parseable `--json`
  output, allowed metric-kind vocabulary, null token-savings fields, unknown
  option rejection, and absence of source/path leakage.
- Progress tests must cover invalid known-work counts through the `WorkUnits`
  constructor rather than constructing impossible progress states directly.

## Current coverage

Bootstrap tests cover core model validation, classification vocabulary,
measurement taxonomy, semantic certainty behavior, protocol token mappings,
strict content-hash validation, TypeScript worker version fallback, progress
rendering and `WorkUnits` validation, schema coverage, JSON-parsed semantic
worker request and NDJSON fixture coverage, Rust-side TypeScript semantic-worker
process and NDJSON validation behavior, telemetry consent, transport-neutral MCP
tool names, CLI command surface, missing-index fallback human/JSON output,
repo-local lifecycle init/status/doctor/uninit/unlock/logs safety behavior,
JSON-parsed bootstrap manifest validation,
TS/JS and Python file discovery filtering/hash/path-safety behavior, SQLite storage
migration and generation-activation safety behavior, validated
semantic-fact/evidence storage substrate behavior, syntax-only code-unit
extraction and storage bridging, source-read hash/path safety, storage-aware
status/doctor reporting, active file-manifest-only or syntax-only
readback, shared repo-relative path policy, native Git context resolution,
`files`/`units` read paths, product runtime wiring, optional semantic-worker
fact ingestion through the
same-generation storage gate, sanitized worker fallback during indexing,
structural IR node/containment-edge storage for syntax-only code units,
active semantic-fact/evidence read-path validation plus internal active
claim-input snapshot validation for future claim builders, typed UNKNOWN
class/reason token validation, internal semantic-fact freshness/readiness gating
for fresh supported facts, stale evidence, missing source, weak certainty,
conflicting facts, and `UNKNOWN` fact kind, conservative EC-MVFI-lite family
builder gating, FamilyStore-backed query `UNKNOWN`/detail rendering,
read-only MCP `repogrammar_context` schema/JSON-RPC serving, schema-backed
family-evidence `covered_claims` write/read validation and query selection,
installer live-write gating through native MCP CLIs and managed receipts,
transitional TS/JS release fixture smoke coverage for product CLI JSON paths,
dependency-free TypeScript worker unavailable-stub behavior,
CPython AST Python worker structural parse and NDJSON smoke behavior,
installer dry-run parsing, deferred `stats --json` metrics contract behavior,
bounded filesystem source reads for discovery hashing and source-store
hash-checked reads, parent Git worktree ignore handling for subdirectory
projects, index/sync lock acquisition and doctor lock-state reporting, and
`repo-guard` sync/path/diff/ADR-0008 required document logic.
