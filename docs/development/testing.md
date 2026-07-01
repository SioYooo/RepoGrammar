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
- Helper code used only by tests must be gated with `#[cfg(test)]` or an
  equivalent test-only cfg so `cargo clippy --all-targets` does not compile it
  as dead production code.
- Dependency updates that require Rust API adaptation must run compile,
  clippy, and full test gates against the updated lockfile. Parser, hash, or
  workflow-action major-version updates must preserve the existing fixture and
  source-free-output test coverage rather than relying on the dependency bump
  alone.
- Symlink safety tests must assert rejection on hosts that can create symlinks.
  On Windows sessions that lack the symlink creation privilege, tests may exit
  early only after confirming the failure is the platform privilege or
  unsupported-symlink error; unrelated I/O errors must still fail the test.
- CLI not-implemented behavior must be stable and asserted.
- CLI missing-index fallback tests must cover both human-readable output and
  `--json` output for the query command surface.
- Repo-local lifecycle tests must use temporary workspaces and cover init
  layout, idempotent repair, Git exclude hygiene, optional root `.gitignore`
  marker writes, `REPOGRAMMAR_DIR` override validation, symlink/file conflicts,
  human and JSON status/doctor output, explicit status
  `manifest_schema_version` and `storage_schema_version` fields, explicit
  doctor `checks.manifest_schema_version` and `checks.storage_schema_version`
  fields, status/doctor storage layout, mutable-database presence,
  legacy-generation-layout presence, and WAL/SHM sidecar fields, no ambiguous
  status or doctor `schema_version` fields, JSON-parsed manifest validation
  with reordered valid fields and invalid required fields, corrupted manifests,
  missing subdirs, diagnostic-only doctor findings for missing or invalid
  `.repogrammar/.gitignore`, `.git/info/exclude`, root `.gitignore` markers,
  and `receipts/init.json`,
  `uninit --yes`, unlock inspection without `--force --yes`, confirmed stale
  `index.lock` removal with `--force --yes`, active/unknown/invalid lock
  refusal, daemon/SQLite lock preservation, repo-local autosync
  enable/status/disable config behavior, autosync daemon-lock inspection, and
  redacted logs metadata.
- File discovery tests must use temporary workspaces and cover TS/JS inclusion,
  Python `.py` inclusion, unsupported module extensions,
  default dependency/build/generated/state-dir
  exclusions, Git-ignored files when Git is available, safe Git-unavailable
  warnings, parent Git worktree ignore rules for subdirectory projects, the
  inclusive 1 MB size boundary, oversized skips, strict SHA-256 hash
  generation, bounded max-plus-one content reads for hashing,
  deterministic ordering, symlink escape skips, invalid roots, strict gitignore
  failure when Git ignore checks are unavailable, and absence of source snippets
  or absolute paths in reports. Python discovery coverage must
  include common virtualenv/cache/dependency/build directories such as
  `.venv`, `venv`, `env`, `.tox`, `.nox`, `__pycache__`, `.pytest_cache`,
  `.mypy_cache`, `.ruff_cache`, `build`, `dist`, and `site-packages`,
  including nested path segments where applicable, and Git-ignored `.py` files
  in root and parent-worktree subdirectory projects.
- SQLite storage tests must use temporary workspaces and cover idempotent
  migrations, required-table validation, WAL and foreign-key PRAGMAs,
  foreign-key enforcement, mutable top-level database creation, active
  `index_generations` row selection, legacy active-pointer fallback only when no
  mutable database exists, preservation of the previous active generation after
  failed validation, repository-relative
  indexed-file paths, semantic-fact/evidence storage with same-generation
  code-unit path/hash/range validation, IR node/edge storage with
  same-generation code-unit/node validation, malformed semantic evidence and IR
  graph rejection before activation, derived-record dependency persistence for
  semantic and family evidence, idempotent unchanged indexed-file rewrites,
  changed-path replacement that cascades stale path-scoped rows and marks
  derived dependents dirty, removed-path deletion that is idempotent for absent
  paths and marks existing derived dependents dirty before cascading stale rows,
  post-commit `PRAGMA optimize` plus passive WAL checkpoint maintenance without
  automatic `VACUUM`,
  active dirty-record refusal, active dependency/hash mismatch refusal, atomic
  rollback of failed fact writes,
  active reads ignoring building-generation rows until activation,
  building-generation write gates for indexed files, code units, IR nodes/edges,
  and semantic facts, validation/activation transition guards that do not
  downgrade active generations, read-only active `files`/`units` listing order
  and tamper rejection, read-only active IR and semantic-fact listing with
  validation and tamper rejection, internal active claim-input snapshot reads
  from one validated generation, snapshot tamper rejection across files, units,
  IR, and semantic facts, prune retention that preserves active generations,
  deletes only old inactive generation rows from the mutable database while
  keeping dry-runs mutation-free, and still covers the legacy directory fallback
  refusal cases for symlinked or non-directory generation entries, missing or
  corrupt active-generation pointers, and symlinked or malformed
  active-generation pointers. Status/doctor coverage must include empty,
  mutable, legacy-only, and mutable-plus-legacy storage layout diagnostics.
  Explicit compact coverage must include
  `compact --dry-run --json` size reporting without writes, `compact --yes`
  active-generation preservation and before/after size reporting, and refusal
  of unsafe active database states such as dirty records.
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
  Progress renderer tests must also cover exact integer percentages and
  interactive TTY progress as single-line carriage-return updates with one
  final newline rather than one terminal line per event.
- Auto-sync CLI tests must cover `autosync` defaulting to `status`,
  `enable/start/status/stop/disable/run` routing, `--poll-ms` and
  `--debounce-ms` validation, `--progress` compatibility, strict-gitignore
  propagation, semantic worker environment inheritance, human and JSON output,
  and no source snippets or absolute paths. Default tests must not start or kill
  real user background services;
  product-runtime background behavior may be covered through
  temporary-repository smoke tests or ignored/manual tests.
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
  find/explain/check lookup, deterministic local `PARTIAL_CONTEXT` for a
  uniquely resolved indexed target without family evidence, ambiguity abstention
  before partial context, path-plus-symbol targets, root-file targets,
  `path:line` and `path:start-end` target forms, advisory partial `check`
  output without proof-like fields, short-substring false-match rejection, stale
  family-evidence refusal with `StaleEvidence`, compact/evidence/deep output
  mode behavior, target and token-budget validation, greedy evidence coverage
  metadata, default read plans with repo-relative paths, content hashes, byte
  ranges, explicit source-span opt-in, line-numbered rendered spans, stale or
  hash-mismatched span omission with Read/Grep fallback guidance, missing
  variation/exception coverage reporting, JSON/human CLI output, advisory
  `check` behavior, and absence of source snippets unless explicitly requested.
- Rust self-dogfood tests must cover `.rs` and `Cargo.toml` discovery,
  Tree-sitter Rust code-unit extraction, structural Rust anchors, typed
  `MacroOrPreprocessor`, `BuildVariantAmbiguity`, `FrameworkMagic`, and
  `UnresolvedImport` UNKNOWN boundaries, support>=3 for internal Rust families,
  low-support abstention, default source-free CLI/MCP output, explicit
  source-span opt-in, safe and unsafe module resolution, target-specific Cargo
  dependency inventory, Cargo build-script non-execution with a sentinel file,
  repository-level build-variant blocking, and fixtures under
  `src/fixtures/rust/release/v0_2/` including `internal_family_gates`,
  `parser_adapters`, `installer_actions`, `product_tests`, `low_support_family`,
  `macro_cfg_unknowns`, `trait_dispatch_unknowns`, `module_resolution`, and
  `cargo_build_blocked_family`.
- Java/Spring preview tests must cover `.java` discovery, Tree-sitter Java
  parser extraction, exact imported/FQN Spring MVC/stereotype/Spring Boot/Spring
  Data anchors, `UnresolvedImport` for Spring-lookalike simple annotations
  without exact imports, no route-family support outside exact controllers,
  nonliteral route-path UNKNOWN subclaims, `repogrammar-java-derived`
  safe-origin promotion, support>=3 family gates, and rejection of structural
  parser anchors, substring targets, and wrong-origin facts as direct family
  support.
- MCP serve tests must cover the single default `repogrammar_context` tool
  schema, accepted operation enum, unknown tool and operation rejection,
  missing-state fallback without implicit repo-local state creation,
  no-active-generation fallback, active-generation typed `UNKNOWN`, advisory
  `check_conformance` with `CONTEXT_ONLY` context success when conformance is
  unproven, exact `show_family` target handling, compact/evidence/deep output
  mode serialization, target and token-budget validation, metadata-only greedy
  evidence selection, metadata-only default read plans for all supported
  operations, explicit `include_source_spans` validation and rendering, stale
  or omitted span fallback guidance, missing variation/exception coverage
  reporting, JSON-RPC initialize/exact-one-tool tools/list/tools/call/shutdown
  handling, and absence of source snippets unless explicitly requested.
- Installer tests must cover dry-run no `.repogrammar/` mutation, no receipt
  creation, no native configuration delegation, and native Codex/Claude Code
  MCP command-shape reporting for dry-run global installs, plus live-write
  `--yes` gating, MCP self-test before native configuration, hanging MCP
  self-test timeout/kill behavior, interactive TUI wizard routing, multi-select
  Codex/Claude Code parsing and deterministic normalization, existing
  RepoGrammar-owned receipt detection, already-managed target skipping,
  safe `--target all --scope global --yes`, all-or-rollback multi-target
  install, unsupported native scopes, receipt writing, receipt-write rollback,
  receipt-owned uninstall, all-target uninstall of owned receipts,
  missing/foreign receipt refusal, install `--yes` not enabling telemetry,
  `install --yes` not prompting for telemetry, install `--telemetry` persisting
  consent only after successful live install, environment/CI telemetry
  disablement overriding install consent, CodeGraph-style target parsing for
  `auto`, `all`, `none`, comma-separated concrete targets, aliases, duplicate
  normalization, and invalid empty CSV entries, no-write
  `--print-config <target>` behavior for deferred registry targets, and no
  `.repogrammar/` mutation.
  Managed-binary refresh tests must cover staging the new file, removing the
  previous RepoGrammar-managed executable or managed command copy before
  activation, and actionable failure guidance when that previous file cannot be
  removed because a coding agent or MCP process may still hold it.
  Default tests must not invoke real `codex` or `claude` binaries; validate
  native integration through dry-run output, command-vector construction, fake
  configurators, fake prompts, and receipt behavior. Any real native-agent CLI
  integration test must be explicitly ignored or feature-gated outside default
  CI.
- Installer wrapper tests must run without network access and without real
  native-agent CLIs. They must cover shell syntax validation and a local fake
  release-artifact path for `src/install/repogrammar-install.sh`, including
  checksum verification, CLI command installation, bundled worker asset
  installation, delegated
  `repogrammar install` / `repogrammar uninstall` invocation through a fake
  binary, source-checkout `--from-source` install/configure dogfood without
  network access, actionable no-release failure text, backup/replacement of
  older unmanaged command files, missing-worker artifact rejection,
  release-workflow artifact and installer-script checksum contract checks,
  target/scope pass-through for comma-separated, `none`, and local-scope
  install requests, stale PATH prune failure propagation, and command removal.
  Default tests must not use wrapper scripts to call real `codex` or `claude`
  binaries.
  Windows PowerShell wrapper coverage must include `src/install/install.ps1`
  source-checkout `-FromSource` installation with an already built local binary,
  bundled worker asset installation, unmanaged command backup, no
  `.repogrammar/` mutation, locked stale PATH prune failure propagation, and
  nonzero propagation when delegated `repogrammar install` fails.
- Npm launcher tests must run without network access, without Rust/Cargo, and
  without real native-agent CLIs. They must use local fake release artifacts to
  cover the full public-preview platform/artifact matrix, unsupported
  platform/arch rejection, checksum rejection, binary/worker cache
  installation, missing-worker artifact rejection, `REPOGRAMMAR_BINARY` local
  dogfood bypass, argument forwarding including target lists, local scope, and
  `--print-config`, and npm package shape via `npm pack --dry-run`.
- Telemetry and metrics tests must cover default anonymous telemetry disabled,
  anonymous telemetry and research trace consent as separate state,
  `REPOGRAMMAR_TELEMETRY=0` and `DO_NOT_TRACK=1` forcing effective telemetry
  off, disabled telemetry making zero upload transport calls, upload dry-runs
  making zero network calls, HTTPS-or-localhost endpoint validation, allowlisted
  telemetry payload validation, absence of source snippets/prompts/query
  text/paths/repository names/symbols/raw errors/env values from exported
  payloads, explicit upload receipt behavior with fake transports, inspect-only
  telemetry export without queue/rollup creation, enabled `stats --json`
  writing only a bucketed local rollup and disabled stats writing no telemetry
  state, local-only `estimated_potential_token_savings` aggregate recording
  without upload queue entries or source/path/hash/query fields, stats reporting
  that aggregate as `ESTIMATED` while leaving measured `token_savings` null
  without paired measurements, redacted research export,
  redacted experiment export without raw names/session ids/token counts, paired
  baseline/treatment token experiment recording, default-no experiment prompts,
  record-existing prompt no-extra-session wording, controlled-pair
  token/time/provider-cost prompt warnings,
  missing pairs yielding `token_savings: null`, comparable pairs computing
  token savings and ratio, required measurement source, and `stats --json`
  reporting measured savings only when a valid paired measurement exists.
  Anonymous telemetry schema tests must cover bucketed experiment aggregate
  fields without raw token counts or user-provided experiment names.
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
  bases/calls/pytest test anchors/test and fixture dependency edges, bounded
  parse-document `conftest.py` fixture hierarchy context, FastAPI route/response-model/static
  dependency-target/dependency-call/error/status-code anchors, static FastAPI
  request body/path/query/header/cookie marker anchors, pytest fixture
  decorator aliases, literal pytest fixture `name=` aliases, dynamic fixture
  names remaining typed `PytestFixtureInjection` `UNKNOWN`, pytest parametrize
  decorator and literal argument anchors, direct-parametrize-over-fixture
  precedence, indirect parametrize remaining typed `PytestFixtureInjection`
  `UNKNOWN`, duplicate applicable conftest fixtures becoming fixture-binding
  `ConflictingFacts`, known pytest built-in fixtures becoming external context
  rather than missing-fixture UNKNOWNs, plugin-style fixture names remaining
  typed `PytestFixtureInjection` `UNKNOWN`, Pydantic field, field-type,
  model-config, nested Config, computed-field, validator, and model-validator
  anchors, dynamic Pydantic model factories remaining typed `FrameworkMagic`
  `UNKNOWN`,
  semantic-worker-compatible NDJSON structural facts plus framework-role output,
  requested-project `conftest.py` fixture hierarchy edges, file-local FastAPI
  router/app alias propagation with same-name reassignment invalidation, typed
  same-function FastAPI service-call context anchors with reassignment
  invalidation, typed `UNKNOWN` output for dynamic decorators, unresolved bare
  decorators, monkey patches, dynamic calls, unsafe or nonliteral
  `importlib.import_module(...)` calls,
  `sys.path.append`/`sys.path.insert` import-environment mutation, and
  unresolved cases, plus safe literal dynamic-import anchors and plain
  `getattr(...)` assignments that do not become dynamic call-target UNKNOWNs,
  oversized request
  rejection, unsafe path and symlink-escape rejection, bounded semantic-mode
  source reads, and absence of source snippets, absolute paths, or unsafe
  dynamic-import literal targets.
- Release fixture smoke tests currently copy committed TS/JS source fixtures
  from both the legacy transitional `src/fixtures/typescript/release/v0_1/`
  corpus and the conservative exact-anchor `src/fixtures/typescript/release/v0_2/`
  corpus, plus Python source fixtures from `src/fixtures/python/release/v0_1/`,
  into temporary workspaces and run the product CLI through `init`, `index`,
  `files`, `units`, `families`, `family`, `member`, `find`, `explain`, `check`,
  and `doctor` JSON paths.
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
- Conservative TS/JS exact-anchor tests live alongside the parser
  (`src/rust/adapters/parsing/tsjs/`), the family gate
  (`src/rust/application/family.rs`), the derivation pass
  (`src/rust/application/indexing.rs`), and the product smoke
  (`src/rust/bin/repogrammar.rs`). They must cover Express positive routes,
  Next.js App/Pages file-convention positives including async const route
  handlers, Fastify shorthand/full route positives, Prisma client/query/
  transaction positives, Drizzle schema/query positives including
  `db.query.<table>.findMany/findFirst`, object-literal/dynamic-receiver/
  dynamic-method/reassigned/shadowed negatives, raw and unsupported bulk query
  negatives, typed TS/JS `UNKNOWN` facts for unsafe/unresolved receiver, runner,
  route, client, and query boundaries, bounded
  `package.json`/`tsconfig.json`/`jsconfig.json` project-config context, bounded
  static relative/path-alias import resolution, typed `UNKNOWN` for dynamic
  import, non-literal or conditional `require`, unresolved/conflicting aliases,
  ambiguous star re-exports, Jest/Vitest imported positives,
  ambient-in-test-file positives only with package/config test-runner context,
  custom-wrapper and foreign-import negatives, that
  `FRAMEWORK_HEURISTIC` facts never derive support, that only
  `repogrammar-tsjs-derived` `DATAFLOW_DERIVED` facts with exact whitelisted
  targets form families, that TS/JS families require at least three compatible
  support facts, that complete-link clustering rejects single-link bridge
  members, that route/test/component/query variation slots are recorded from
  context profiles, and that default JS/TS query output stays source-free while
  `--include-source-spans` / `include_source_spans=true` returns bounded
  hash-checked line-numbered spans. Positive TS/JS family fixtures live under
  `src/fixtures/typescript/release/v0_2/express_exact_routes`,
  `jest_vitest_exact_tests`, `next_exact_routes`, `fastify_exact_routes`,
  `prisma_exact_repositories`, and `drizzle_exact_repositories`; package-only,
  raw, dynamic, and unsupported lookalikes live under
  `framework_adapter_negative_cases` and `unsupported_framework_lookalikes`.
- Python v0.1 tests must cover the implemented CPython `ast` frontend output,
  FastAPI, pytest, SQLAlchemy, and Pydantic structural positives, Python
  language/kind token stability, product `index`/`units` smoke coverage,
  path-derived module-name anchors, CPython `symtable` scope anchors, private
  `tomllib` project-config summaries, default-index persistence of root
  `pyproject.toml` as `python-config`/`project_config` structural context or
  typed config `UNKNOWN`, semantic-worker-compatible project-mode repo-local
  import resolution for unique module-level matches, ambiguous or missing
  repo-local import `UNKNOWN`, `sys.path` mutation
  `RuntimeDependencyInjection` `UNKNOWN`, dynamic FastAPI dependency-target
  `RuntimeDependencyInjection` `UNKNOWN`, dynamic import, dynamic call-target,
  dynamic/unresolved decorator, and monkey-patch `UNKNOWN` facts through product indexing,
  persisted parser-origin structural
  facts including FastAPI response-model/static dependency-target/
  dependency-call/error/status-code anchors, static FastAPI request
  body/parameter anchors, pytest parametrize, and Pydantic validator anchors,
  typed `UNKNOWN`,
  persisted project-config facts staying out of claim-input readiness,
  heuristic framework-role facts staying out of family claims, raw parser-origin
  facts staying out of support derivation while parser-origin context and
  `UNKNOWN` facts reach the family builder only for compatibility, variation,
  and abstention,
  exact-anchor derived support facts producing no-worker direct FastAPI,
  the complete FastAPI/APIRouter `delete`, `get`, `head`, `options`, `patch`,
  `post`, and `put` route-method matrix, FastAPI alias, pytest test,
  pytest `mark.parametrize` decorator support, pytest fixture, Pydantic model/settings,
  SQLAlchemy model-field, and SQLAlchemy session/repository families only when
  Python support reaches three members, Python complete-link clustering refusing
  single-link bridge members and splitting distinct ready support-family
  clusters with stable sanitized ids, their CLI/MCP metadata-only
  compact/evidence/deep query paths,
  explicit metadata-only variation slots when parser-context profiles differ
  inside an already-supported Python family,
  pytest non-builtin fixture-context differences remaining incompatible while
  known builtin fixture-context differences are metadata-only variation/context,
  default-index parser context receiving discovered `.py` inventory, sanitized
  root `pyproject.toml` source roots from parser/tomllib project-config facts,
  and bounded discovered `conftest.py` contents,
  exact-anchor target variation coverage only for already-ready Python families,
  FastAPI response-model/static dependency-target/dependency-call/error/status-code
  context anchors and static request body/parameter anchors staying out of
  support derivation, claim-input readiness, and support-target variation metadata,
  pytest test/fixture dependency-edge, builtin-fixture context, and
  parametrize-argument anchors staying out of family support, SQLAlchemy relationship and Session.add
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
- Stats CLI tests must cover parseable `--json` output, missing-index fallback,
  allowed metric-kind vocabulary, local-pattern-density/family-coverage/
  abstention diagnostics, thin-wrapper/token-saving risk, readiness/blocking
  reasons, null measured token-savings fields, unknown option rejection, and
  absence of source/path leakage.
- Progress tests must cover invalid known-work counts through the `WorkUnits`
  constructor rather than constructing impossible progress states directly, and
  must assert known-work percentages while preserving indeterminate output for
  unknown work.

## Current coverage

Bootstrap tests cover core model validation, classification vocabulary,
measurement taxonomy, semantic certainty behavior, protocol token mappings,
strict content-hash validation, TypeScript worker version fallback, progress
rendering and `WorkUnits` validation, schema coverage, JSON-parsed semantic
worker request and NDJSON fixture coverage, Rust-side TypeScript semantic-worker
process and NDJSON validation behavior, telemetry consent, transport-neutral MCP
tool names, CLI command surface, missing-index fallback human/JSON output,
repo-local lifecycle init/status/doctor/uninit/unlock/logs safety behavior,
explicit `init --yes --resync --autosync` bootstrap sequencing and failure
preservation, bounded redacted repo-local log tails,
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
metadata-only read-plan line-range enrichment and omission guidance,
read-only MCP `repogrammar_context` schema/JSON-RPC serving, schema-backed
family-evidence `covered_claims` write/read validation and query selection,
installer live-write gating through native MCP CLIs and managed receipts,
transitional TS/JS release fixture smoke coverage for product CLI JSON paths,
dependency-free TypeScript worker unavailable-stub behavior,
CPython AST Python worker structural parse and NDJSON smoke behavior,
installer dry-run parsing, deferred `stats --json` metrics contract behavior,
bounded filesystem source reads for discovery hashing and source-store
hash-checked reads, parent Git worktree ignore handling for subdirectory
projects, index/sync/resync lock acquisition and doctor lock-state reporting, and
`repo-guard` sync/path/diff/ADR-0008 required document logic.

## Required local gate

Use the full gate before committing implementation changes:

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
python3 src/workers/python/worker.test.py
node src/workers/typescript/worker.test.js
cargo run --quiet --bin repo-guard -- check
cargo run --quiet --bin repo-guard -- check-diff --base origin/main --head HEAD
git diff --check origin/main...HEAD
cmp -s AGENTS.md CLAUDE.md
```
