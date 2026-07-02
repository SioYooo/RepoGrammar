# Changelog

## Unreleased

### Changed

- Storage now uses the top-level mutable `.repogrammar/repogrammar.sqlite`
  database as the normal index store, with active generation state held in
  `index_generations` rows and legacy `current-generation`/`generations/`
  support retained only as fallback. Query commands now return
  `PARTIAL_CONTEXT` for a uniquely resolved indexed target that lacks family
  evidence, preserving typed `InsufficientSupport` instead of inflating it into
  a family claim. The partial-context resolver now preserves embedded paths,
  `path:line`, `path:start-end`, symbol hints, residue terms, candidates, and
  advisory `check` metadata without proof-like conformance fields. The mutable
  storage schema now records derived-record dependencies and dirty-record
  markers, reports their active counts through status/doctor, and refuses active
  family or semantic reads when dirty rows or dependency/hash mismatches are
  present. Re-recording an unchanged indexed file is now idempotent, while
  replacing a changed path removes stale path-scoped records and marks derived
  dependents dirty in the same SQLite transaction. Removed indexed paths now
  use the same fail-closed path-scoped cascade and dirty-marker behavior.
  Successful mutable index activation and mutating mutable prune operations now
  apply bounded SQLite maintenance with `PRAGMA optimize` and a passive WAL
  checkpoint without running automatic `VACUUM`. Explicit
  `repogrammar compact --dry-run --json` and `repogrammar compact --yes`
  commands now report mutable SQLite database/WAL/SHM size metadata, require the
  index lock, validate the active generation, and reserve full `VACUUM` for
  confirmation-gated compaction with a truncating WAL checkpoint only. Status
  and doctor now also report storage layout (`empty`, `mutable`, `legacy`, or
  `mutable_with_legacy`), mutable-database presence, legacy layout presence, and
  WAL/SHM sidecar byte counts when a mutable database exists.
- `repogrammar sync` now performs path-level incremental updates when the active
  mutable generation is readable, schema-compatible, and dirty-free. It
  copy-forwards unchanged file/code-unit/IR/non-derived semantic records into a
  new generation, reparses added or modified paths, drops removed paths,
  recomputes local derived support and families, and reports delta counters in
  `sync --json`. Project-context changes, configured semantic workers, unsafe
  storage layouts, or dirty active records now report `sync_mode:
  full_rebuild_fallback`; `index` and `resync` remain full rebuilds.
- Public-preview hardening now blocks React-shaped TypeScript semantic-worker
  facts from forming unsupported JS/TS family claims, applies safe
  `tsconfig.json` / `jsconfig.json` `baseUrl` prefixes to JSON path aliases,
  documents that Jest/Vitest script configs are metadata/typed `UNKNOWN` only,
  and recommends explicit preview tags instead of GitHub's `latest` redirect for
  prerelease installer usage.
- Installer launchers now validate release archive entry names before
  extraction and reject unsafe or unexpected paths even when the checksum
  matches. The npm launcher also validates release tags/cache containment and
  stages binary+worker cache updates before swapping them into place.
- Installer wrapper stale-PATH cleanup now exits nonzero when a requested prune
  cannot remove an outdated `repogrammar` copy, instead of reporting a
  successful install or prune with the stale executable still ahead on PATH.
- Python indexing now builds a bounded static repo-local import and pytest
  fixture graph from CPython `ast` context. Unique local module imports, direct
  imported top-level symbols, static package re-exports, literal `__all__` star
  imports, same-file/conftest fixture edges, and literal
  `request.getfixturevalue("name")` lookups are persisted as source-tied
  `DATAFLOW_DERIVED` graph facts with `provider_resolved=false`; ambiguous,
  dynamic, plugin, unsafe star, external, or unresolved cases remain typed
  `UNKNOWN`.
- SQLAlchemy repository-method exact anchors now include typed
  `Session.get(...)` and `AsyncSession.get(...)` receiver calls. Plain `.get`
  calls without a typed SQLAlchemy session receiver remain non-SQLAlchemy
  context.
- TS/JS project context now records safe JSON `rootDirs` from root
  `tsconfig.json` / `jsconfig.json` and uses them as a bounded fallback for
  repo-local relative import resolution. Unique rootDirs targets become
  `STRUCTURAL` `RESOLVED_IMPORT` context facts, while unresolved or conflicting
  rootDirs candidates remain typed `UNKNOWN`.
- TS/JS exact-anchor binding now accepts CommonJS destructuring aliases from
  exact supported framework packages, covering Express routers, Fastify
  factories, Prisma clients, and Drizzle table/db factories without treating
  custom wrappers or injected clients as support.
- Rust cfg/cfg_attr build-variant UNKNOWNs now carry bounded Cargo feature
  context, including the nearest discovered `Cargo.toml`, feature predicate
  names, and whether each feature is declared there. These assumptions improve
  `cargo_feature_cfg_model` triage without evaluating cfgs or converting the
  UNKNOWN into family support.

### Added

- UNKNOWN regression benchmark coverage now pins release-fixture
  `unknowns --json` language, reason-code, and required-mechanism buckets for
  Python dynamic behavior, TS/JS framework negative cases, and Rust macro/cfg
  boundaries, while also asserting those negative fixtures do not silently form
  families. The protocol is documented under `docs/experiments/`.
- Conservative Java/Spring structural-preview support. RepoGrammar now discovers
  `.java` files, uses Tree-sitter Java for structural Java/Spring code units,
  emits exact imported/FQN Spring MVC/stereotype/Spring Boot/Spring Data
  anchors, derives bounded `repogrammar-java-derived` support under exact target
  and safe-origin gates, and preserves typed `UNKNOWN`s for lookalike
  annotations, nonliteral route paths, DI/proxy/component-scan/runtime/classpath
  behavior, Maven/Gradle/javac/annotation-processor semantics, and low support.
- Conservative TS/JS structural-preview adapters for Next.js, Fastify, Prisma,
  and Drizzle. The new adapter registry adds role-compatible exact-anchor
  promotion with framework-specific `derived_from=tsjs_<framework>_structural_anchors`
  provenance, bounded Next App/Pages conventions including async const route
  handlers with dynamic segment/route-group context assumptions, Fastify
  shorthand and `fastify.route.route` full declarations, Prisma allowlisted
  model operations and array transactions, and Drizzle table/query anchors
  including `db.query.<table>.findMany/findFirst`. Package-only evidence,
  framework-role heuristics, React components/hooks, Next middleware/server
  actions/re-exports, Fastify plugin prefixes, Prisma bulk/injected/raw clients,
  and Drizzle raw/dynamic builders remain `UNKNOWN` or non-supporting context.
  New v0.2 fixtures cover positive Next/Fastify/Prisma/Drizzle families and
  negative package/dynamic/raw/bulk/shadowed cases.
- Rust structural self-dogfood indexing for RepoGrammar's own implementation
  families. The new Tree-sitter Rust adapter discovers `.rs` files and
  `Cargo.toml`, extracts structural Rust units, emits typed UNKNOWNs for
  cfg/build variants, macro/proc-macro syntax, unresolved modules, and
  trait-object dispatch, and derives bounded internal `DATAFLOW_DERIVED`
  support only for RepoGrammar-owned roles. Product fixtures under
  `src/fixtures/rust/release/v0_2` prove support>=3 positive families across
  family gates, parser adapters, installer actions, and product tests;
  low-support, macro/cfg, trait-dispatch, conflicting-module, Cargo build-script,
  and unsafe-path abstention; bounded Cargo target-dependency inventory;
  metadata-only default output; stale source refusal; build-script non-execution;
  and explicit source-span opt-in. This is not general Rust semantic analysis.
- CLI family query output, MCP `repogrammar_context` family responses, and
  `repogrammar stats` now surface `estimated_potential_token_savings` as an
  `ESTIMATED` local potential-read-displacement diagnostic. Successful family
  context responses update a repo-local aggregate under
  `.repogrammar/telemetry/local-metrics/` without adding anonymous upload queue
  entries, source text, paths, hashes, prompts, query text, or evidence text.
- `repogrammar autosync` now provides optional repository-local automatic sync
  management. `autosync start` enables and starts a background worker that
  polls the existing discovery fingerprint, debounces file saves, and runs the
  normal delta-aware `sync` path; `status`, `stop`, `disable`, and foreground
  `run` manage the worker. The feature is explicit per repository and is not
  started by MCP serving, agent installation, or queries.
- Public-preview readiness documentation now includes an explicit support matrix
  for Python v0.1, conservative JS/TS v0.2 exact-anchor support, unsupported
  React/broad TS/JS semantics, source-span opt-in, token-saving claim limits, and
  installer platform boundaries. A readiness report and real-repo dogfood
  protocol were added under `docs/reports/` and `docs/experiments/`.
- Release/install readiness tests now verify npm platform-to-artifact mappings
  for macOS, Linux, and Windows preview targets, unsupported npm platform/arch
  rejection, required bundled Python worker assets, Bash installer state-boundary
  behavior, release workflow artifact names, and installer script checksum
  publication.
- Additional v0.2 JS/TS fixtures cover JavaScript Jest/Vitest family support,
  exact Next/Fastify/Prisma/Drizzle positives, and React/package-only/dynamic/raw
  lookalikes that must not form public family rows.
- Conservative TS/JS exact-anchor family support for Express route handlers,
  Jest/Vitest suites/tests, and structural-preview Next.js, Fastify, Prisma, and
  Drizzle adapters. The syntax parser emits `STRUCTURAL` anchors only for exact
  local framework bindings and file conventions; reassigned, shadowed,
  dynamic-receiver or dynamic-method, custom-wrapper, conditional-import, raw,
  object-literal, and framework-magic lookalikes stay `UNKNOWN`. The application
  layer promotes those anchors to `DATAFLOW_DERIVED` support facts (engine
  `repogrammar-tsjs-derived`, method `bounded_exact_anchor_v1`), and the loose
  substring compatibility gate is replaced by an exact target whitelist plus a
  safe-origin check. TS/JS family construction now requires at least three
  compatible support facts, uses complete-link clustering over conservative
  route/test/component/query feature profiles, records variation slots, and
  keeps project-config inventory
  (`package.json`, `tsconfig.json`, `jsconfig.json`, Jest/Vitest config files)
  as structural context or typed config `UNKNOWN` only. React components/hooks
  remain `UNKNOWN`. CLI/MCP `find`/`check`/`family` and the source-span renderer
  work for JS/TS fixtures; default output stays source-free and
  `--include-source-spans` / `include_source_spans=true` returns bounded
  hash-checked line-numbered spans. New fixtures live under
  `src/fixtures/typescript/release/v0_2`. This is a token-saving foundation, not
  full TS/JS semantic analysis or an official v0.1 target change.
- Bounded TS/JS project context now feeds parser-mode indexing: discovered
  TS/JS module paths, JSON `tsconfig.json`/`jsconfig.json` path aliases, and
  package/config Jest/Vitest context are passed into the syntax parser. Unique
  literal relative imports and unique path-alias imports can be persisted as
  `STRUCTURAL` `RESOLVED_IMPORT` context facts, while dynamic imports,
  non-literal or conditional `require`, unresolved or conflicting aliases, and
  star re-exports persist typed `UNKNOWN`s. Ambient Jest/Vitest globals now
  require package/config test-runner context; test-file location alone is not
  enough to form support.
- The installer target registry is now exposed through a per-target adapter
  contract (`TargetAdapter`) that consolidates scope support, live-writer
  status, the no-write config preview, and `describe_paths` planning. Dry-run
  output now reports a per-target instruction-file plan line that names the
  `REPOGRAMMAR_INSTRUCTION_FILE_<TARGET>` override and its deferred default.
  Live writes still cover only global Codex and Claude Code, and `--target all`
  still installs only live-supported targets all-or-rollback.
- The installer now has a reversible, idempotent managed instruction-file
  writer using the exact markers `<!-- BEGIN REPOGRAMMAR MANAGED SECTION -->`
  and `<!-- END REPOGRAMMAR MANAGED SECTION -->`. It creates, appends, replaces,
  or leaves unchanged the managed section, refuses malformed or partial markers,
  writes atomically with re-read verification, and on uninstall or rollback
  removes that section, deleting a file RepoGrammar created when removal leaves
  it empty while preserving any pre-existing or user-added content. Receipts now
  record `instruction_file_path` and
  `instruction_action`. Live instruction writing stays deferred unless
  `REPOGRAMMAR_INSTRUCTION_FILE_<TARGET>` resolves to an absolute path, because
  real Codex/Claude instruction-file locations are not yet verified.
- `repogrammar index` and `repogrammar sync` now emit progress while they run.
  Human progress uses stderr and exact completed/total counts when known;
  `--json --progress always` emits progress NDJSON on stderr while preserving
  the final JSON result on stdout.
- CLI and MCP family queries now support explicit bounded source-span rendering
  (`--include-source-spans` / `include_source_spans: true`) over hash-checked
  read-plan spans. Default output remains metadata-only, and stale or omitted
  spans carry Read/Grep fallback guidance.
- Source-checkout installer dogfood now works before GitHub prerelease assets
  or npm publication exist. The Bash wrapper can install the built contributor
  binary through explicit `--from-source` flows, writes the command through the
  same managed install layout used by the Rust installer, delegates agent
  wiring to `repogrammar install`, backs up and replaces older unmanaged
  `repogrammar` command files during explicit CLI installation, and reports
  actionable missing-release guidance. The npm launcher now has a tested
  `REPOGRAMMAR_BINARY` local dogfood bypass while keeping release artifacts as
  the published default.
- Interactive `repogrammar install` now provides a dependency-light text wizard
  for machine-level Codex and Claude Code MCP wiring. The wizard supports
  multi-select in one run, skips already managed RepoGrammar receipts by
  default, keeps telemetry default-off, and does not initialize or index
  repositories. Noninteractive `--target all --scope global --yes` uses the
  same all-or-rollback multi-agent transaction, and `uninstall --target all`
  removes only RepoGrammar-owned first-class agent receipts.
- Re-running `repogrammar install` now still installs or repairs the
  user-writable `repogrammar` command when selected agents are already managed.
- Managed installs now place bundled Python worker assets under install state
  that the managed executable can discover, and refresh rollback restores the
  previous managed executable and command copy if self-test or native agent
  configuration fails.
- The installer now has a CodeGraph-style target registry for planning and
  configuration previews. `--target` accepts `auto`, `all`, `none`, aliases,
  and comma-separated concrete target lists; `--location` aliases `--scope`;
  and `--print-config <target>` prints no-write MCP snippets for known targets
  such as Cursor, opencode, Hermes, Gemini, Antigravity, and Kiro while live
  writes remain gated to global Codex and Claude Code.
- Source checkouts now include `src/install/repogrammar-install.sh`, a
  dependency-light TUI wrapper for downloading and verifying a prebuilt release
  binary, installing or repairing the command, configuring or uninstalling
  Codex/Claude Code integrations, removing the local command path after
  confirmation, or explicitly choosing a contributor source build.
- Release automation now builds prebuilt `repogrammar` artifacts with checksum
  assets for macOS arm64/x86_64, Linux arm64/x86_64, and Windows x86_64
  preview, bundles the current Python worker asset, and publishes `install.sh`
  / `install.ps1` installer assets for tagged preview releases. Real
  downloadable prerelease artifacts are available only after a preview tag is
  published.
- Added the `@sioyooo/repogrammar` npm package manifest and thin `npx`
  launcher. The launcher downloads and verifies the same prebuilt release
  artifacts, caches the binary and bundled worker asset, and delegates all
  behavior to the Rust CLI without requiring Cargo.
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
  class bases, simple call targets, `pytest.test` test-function anchors,
  same-file pytest test and fixture dependency edges, and typed
  dynamic/unresolved `UNKNOWN` cases, now including path-derived module-name
  anchors, CPython `symtable`
  structural scope anchors, and a private
  `tomllib` project-config summary mode. Its semantic-worker-compatible
  project mode now resolves only unique repo-local module imports as
  `STRUCTURAL` facts, resolves requested-project `conftest.py` fixture names
  through pytest's directory hierarchy as structural fixture-edge facts, and
  reports ambiguous/missing repo-local imports, unsafe/unresolved literal
  dynamic imports, `__import__`, `locals()[...]`, `eval`, `exec`, `compile`, or
  `sys.path` mutation as typed `UNKNOWN`. Default parser-mode indexing now
  passes discovered repo-relative `.py` inventory and bounded discovered
  `conftest.py` contents into private parse-document requests so source-tied
  repo-local import facts, same-file fixture dependency facts, and
  parent-directory pytest fixture-edge facts can be persisted without launching
  a Python semantic worker; oversized context
  payloads fall back to contextless parsing. The worker now performs file-local
  simple FastAPI router/app alias propagation with same-name reassignment
  invalidation, emits structural FastAPI `response_model`, `Depends`, and
  static `Depends(get_db)` dependency-target anchors plus `HTTPException`
  call and literal status-code anchors, treats literal
  `pytest.mark.parametrize` arguments as parametrize facts rather than
  fixture-injection UNKNOWNs, and labels Pydantic field, field-type,
  model-config, nested Config, computed-field, validator, and model-validator
  anchors as structural model metadata.
  SQLAlchemy parser anchors now include `Mapped[...]`, `mapped_column(...)`,
  typed `Session`/`AsyncSession` call targets, and bounded propagation from
  `__init__`-assigned `self.session`/`self.db` attributes into repository
  methods with same-method receiver reassignment invalidation.
  FastAPI route parser anchors now include bounded same-function service-call
  context for import-resolved static local forms such as `service = UserService();
  service.list_users()` and `runner = run_query; runner()`, with reassignment
  invalidation and dynamic `getattr(...)` calls preserved as typed `UNKNOWN`.
  Static FastAPI `Body`, `Path`, `Query`, `Header`, and `Cookie` route
  parameter markers now produce structural request-shape anchors; those anchors
  remain context metadata and do not become family support.
  Dynamic decorator factories now produce typed `FrameworkMagic` UNKNOWNs for
  `python_framework_identity`, and `setattr(...)` monkey-patching produces typed
  `MonkeyPatch` UNKNOWNs for `python_call_target`; neither path becomes family
  evidence. Assigned aliases of dynamic import/execution/namespace/lookup and
  monkey-patch functions now remain typed `UNKNOWN` as well, while uniquely
  repo-local literal `importlib.import_module(...)` can still become structural
  import context. Dynamic FastAPI dependency target expressions such as
  `Depends(make_dependency())` now produce typed `RuntimeDependencyInjection`
  UNKNOWNs for the dependency-target sub-claim instead of silently disappearing.
  Literal pytest fixture `name=` aliases now define the fixture binding name,
  while dynamic or unsafe `name=` values remain typed `PytestFixtureInjection`
  UNKNOWNs instead of falling back to the implementation function name.
  Dynamic Pydantic `create_model(...)` factories now remain typed
  `FrameworkMagic` UNKNOWNs instead of becoming static model-family support.
  Bare unresolved decorators now also produce framework-identity
  `FrameworkMagic` UNKNOWNs while local decorators and native `property`,
  `classmethod`, and `staticmethod` remain structural metadata.
  Duplicate applicable pytest `conftest.py` fixture names now produce typed
  `ConflictingFacts` UNKNOWNs for fixture binding, known pytest built-in
  fixtures such as `tmp_path` and `capsys` become metadata-only
  `pytest.builtin_fixture.*` context, and plugin-style fixture names remain
  `PytestFixtureInjection` UNKNOWN without an allowlist or provider.
  The `dynamic-unknown` release fixture now covers dynamic import, `sys.path`
  mutation, dynamic FastAPI dependency targets, pytest fixture-binding
  ambiguity/plugin UNKNOWNs, dynamic call target, dynamic decorator, and
  monkey-patch boundaries through the product indexing/query path.
  A dedicated `pytest-dynamic-fixture-name` release fixture verifies dynamic
  pytest fixture `name=` values stay UNKNOWN through the product path without
  producing fixture-binding support.
  No-worker release smoke now covers direct FastAPI, FastAPI alias, pytest,
  Pydantic model/settings, SQLAlchemy model-field, and SQLAlchemy
  session/repository exact-anchor derived-support family paths without claiming
  provider-backed Python semantics. It also covers exact-anchor `member`,
  `find`, `explain`, advisory `check`, token-budget auto evidence, explicit
  compact/evidence/deep metadata modes, MCP parity for supported operations,
  and stale source mutation/deletion returning `StaleEvidence` `UNKNOWN`.
  Matched CLI and MCP family reads now also include metadata-only read plans
  with repo-relative paths, strict content hashes, byte ranges, purpose labels,
  estimated token costs, and `source_snippets_included: false`. Read plans mark
  target source as required before edits, keep line ranges `null` until safe
  source-span rendering exists, and are suppressed when stale or insufficient
  evidence returns typed `UNKNOWN`. Non-blocking supported-member subclaim
  UNKNOWNs, such as unresolved FastAPI dependency targets, are preserved in
  family detail/query metadata with the concrete family id instead of being
  silently dropped from confident route-family reads.
  `repogrammar stats --json` now reports repo-shape diagnostics for local
  pattern density, family support coverage, abstention rate, and
  thin-wrapper/token-saving risk without reporting measured token savings or
  context compression ratios.
- Anonymous telemetry now has state-backed `status`, `on`, `off`, `export`,
  `upload`, and `purge` commands, remains disabled by default, honors
  environment opt-outs, validates a versioned allowlist payload schema, and
  keeps research trace consent separate. The anonymous payload no longer carries
  a repository instance id, includes only coarse external-dependency risk, and
  treats export as inspect-only rather than queue creation. Enabled
  `stats --json` now writes only an allowlisted local rollup and still performs
  no network upload or queue creation. Telemetry status now reports rollup
  counts, CI disablement, and whether explicit upload would open a network
  connection. Live install now keeps telemetry consent independent from agent
  configuration: `--yes` alone does not enable telemetry, `install --yes`
  without telemetry flags does not prompt and keeps telemetry disabled, and
  dry-run output names the planned native Codex/Claude Code MCP command shape.
  Local paired token
  experiments can now record baseline/treatment token counts through explicit
  `record-existing` or `controlled-pair` confirmation so `stats --json` reports
  measured token savings only when comparable measurements exist. The product
  binary prompts for experiment recording with default-no `[y/N]` when `--yes`
  is absent; controlled-pair prompts warn about possible extra
  token/time/provider cost. Anonymous telemetry payloads include only coarse
  experiment aggregate categories, experiment export is redacted by default,
  and failed treatment correctness invalidates product token-saving claims even
  when a raw token delta is available. Uninstall now refuses missing managed
  receipts instead of silently succeeding.
  FastAPI exact-anchor regression coverage now spans all supported
  FastAPI/APIRouter HTTP route methods (`delete`, `get`, `head`, `options`,
  `patch`, `post`, and `put`) and keeps `api_route`/WebSocket decorators
  deferred. pytest regression coverage now distinguishes the support-eligible
  `pytest.mark.parametrize` decorator from context-only parametrize argument
  anchors.
  Default indexing validates and persists
  parser-origin
  `STRUCTURAL`/`UNKNOWN` facts while keeping them out of support derivation
  and CLI/MCP family evidence. The family builder may consume them only as
  context features or claim-scoped abstention inputs.
- Default Python indexing discovers root `pyproject.toml` as `python-config`,
  reads it through the Rust source-store path/hash boundary, and persists a
  `project_config` unit with sanitized `PROJECT_CONFIG`/`STRUCTURAL` metadata
  or typed config `UNKNOWN` facts; these records stay out of family support and
  claim-input readiness.
- Default Python parser context now reuses sanitized root `pyproject.toml`
  source roots from the existing parser/tomllib project-config facts, alongside
  discovered `.py` inventory and bounded `conftest.py` context, so repo-local
  import facts can reflect configured source roots without adding a second TOML
  parser or provider-backed semantics.
- Bounded Python exact-anchor support derivation: validated CPython structural
  anchors can now produce separate `DATAFLOW_DERIVED` support facts when their
  target exact-matches the Python framework compatibility table for a unit with
  one framework role. Raw parser facts and framework heuristics remain
  insufficient, project-config facts stay blocked, and Python still requires
  three compatible support members before the EC-MVFI-lite family builder writes
  a family.
- Python EC-MVFI-lite family construction now applies bounded complete-link
  clustering over support-family feature groups. Bridge members can no longer
  single-link incompatible Python support families into one confident claim, and
  multiple ready clusters inside one coarse bucket receive stable sanitized
  cluster ids without exposing source snippets or absolute paths.
- Python family construction now also consumes parser-origin context facts in
  its complete-link compatibility check and applies claim-scoped blocking
  `UNKNOWN`s at the final family-builder boundary. Dynamic import/import
  resolution UNKNOWNs block affected family membership, pytest fixture-binding
  UNKNOWNs block pytest family support, and FastAPI dependency-target UNKNOWNs
  remain scoped to the dependency subclaim rather than route-family membership.
- Ready Python families now record metadata-only variation slots when
  parser-context profiles such as FastAPI effect markers or service-call shapes
  differ inside an already-supported family. These slots do not expose source
  snippets or promote parser context into support evidence.
- pytest family compatibility now requires non-builtin fixture dependency
  profiles to match; known builtin fixture-context differences can remain as
  metadata-only variation/context.
- The CPython AST worker now resolves import aliases and module-level dynamic
  UNKNOWN propagation by source position. A route or model before a top-level
  shadowing assignment can still use exact framework imports, later units
  cannot, local `@client.get(...)` no longer becomes a FastAPI route, local
  `BaseModel`/`Base` classes no longer become Pydantic/SQLAlchemy support, and
  bare `locals()`/`globals()` calls now produce typed call-target `UNKNOWN`s.
- Product-path regression coverage now indexes local framework lookalikes and
  proves `@client.get(...)`, user-defined `BaseModel`, and user-defined
  SQLAlchemy-shaped `Base` classes stay out of family support and query claims.
- Query input hardening now shares target and token-budget validation between
  CLI and MCP. MCP schema exposes `target` max length and `token_budget`
  maximum, and both interfaces reject oversized or control-character targets.
- File discovery now supports explicit strict gitignore mode via
  `REPOGRAMMAR_STRICT_GITIGNORE=true`, which treats unavailable Git ignore
  checks as an error. Non-strict discovery keeps the previous warning fallback.
  Gitignore regression coverage now includes Python files in root and parent
  worktree project layouts.
- Narrow Python exact-anchor variation metadata: when an already-ready Python
  family has multiple exact-compatible framework-anchor support targets, the
  family builder records a dedicated variation slot and one metadata-only
  `variation` evidence label. This does not add provider-backed semantics,
  source snippets, exception mining, or runtime-equivalence claims.
- Python family claim-boundary regression coverage now proves FastAPI
  `response_model`, static dependency-target, `Depends`, `HTTPException`, and
  literal HTTPException status-code structural anchors do not create membership
  support or alter exact-anchor support targets. Family detail UNKNOWN output
  now scopes runtime-equivalence gaps to the concrete family id, and human
  `families` output preserves typed stale-evidence UNKNOWN details.
- Product-path Python auxiliary-context regression coverage now proves FastAPI
  request body/path/query/header/cookie anchors and SQLAlchemy
  `relationship`/`Session.add` anchors are persisted as CPython structural
  facts, blocked from claim-input readiness with `InsufficientSupport`, and
  absent from derived family-support facts.
- CPython AST worker SQLAlchemy structural anchors now include
  `sqlalchemy.orm.relationship` and `Session.add`/`AsyncSession.add` effect
  calls. These anchors remain structural context and are explicitly excluded
  from family membership support.
- SQLAlchemy repository-method exact anchors now include direct
  `Session.scalar`/`Session.scalars` and async session equivalents, with a
  release smoke fixture proving derived family support without source snippets.
- SQLAlchemy transaction-boundary exact anchors now have derivation and product
  release-smoke coverage for sync/async `Session.commit` and
  `Session.rollback`, while keeping transaction equivalence unclaimed and
  `Session.add` as context/effect metadata only.
- CPython AST worker pytest fixture detection is now alias-aware for same-file
  and `conftest.py` contexts. Direct parametrize arguments take precedence over
  same-name fixtures, indirect parametrize arguments stay typed
  `PytestFixtureInjection` `UNKNOWN`, and fixture-edge/parametrize-argument
  anchors remain excluded from family support.
- CPython AST worker Pydantic model-member structural anchors now include
  fields, field annotation targets, `model_config`, nested `Config`,
  `computed_field`, validators, and `model_validator`. These anchors remain
  schema/config/member context and are explicitly excluded from family
  membership support. Pydantic validator anchors are no longer accepted as
  exact-anchor family support; v0.1 Pydantic family support is limited to
  compatible model/settings base targets.
- CPython AST worker FastAPI service-call structural anchors now recover only
  bounded same-function static call targets and remain handler/service context,
  explicitly excluded from route-family membership support.
- Rust `ports::python_provider` contract for future candidate-scoped
  Pyrefly/Pyright/RightTyper provider requests, provenance assumptions,
  cache-key dimensions, and recoverable provider-unavailable `UNKNOWN`s. This
  does not execute provider tools or add production provider-backed Python
  semantics.
- Application-layer Pyrefly framework-identity request planning for plausible
  Python candidate groups. The planner validates future-provider request scopes
  from in-memory facts or active-generation snapshots and skips
  claim-blocking parser `UNKNOWN`s without executing Pyrefly, storing provider
  facts, changing CLI/MCP output, or upgrading family claims. Planner tests now
  cover import-resolution, framework-identity, and pytest fixture-binding
  blockers.
- Exact-anchor Python support derivation is now sound-by-abstention for bounded
  framework-family claims: a parser-origin blocking `UNKNOWN` prevents the
  affected unit from contributing `DATAFLOW_DERIVED` family support, while
  FastAPI dependency-target UNKNOWNs stay scoped to dependency-target subclaims.
- CPython exact-anchor derivation now ignores framework imports shadowed by
  later same-name top-level definitions or assignments, treats only top-level
  imports as file-level framework aliases, and copies module-level dynamic
  import or `sys.path` mutation into unit-scoped blocking `UNKNOWN`s for later
  family-shaped units instead of letting those units contribute support.
- Compact/evidence/deep family output modes for CLI and MCP family detail.
  Compact is now the default and omits evidence records; evidence/deep return
  selected repo-relative evidence metadata under an optional token budget and
  explicitly report that source snippets are not included.
- Greedy family evidence selection metadata for CLI and MCP. Evidence/deep
  output now reports the selector strategy, rough budget satisfaction, covered
  claim labels, and missing requested variation/exception coverage instead of
  preserving raw storage order or inferring unsupported coverage from notes.
- Schema-backed family evidence coverage labels. The pre-release storage schema
  v5 now persists validated `covered_claims` labels for family evidence, and
  query selection consumes those labels rather than inferring claim coverage
  from notes or storage order.
- Python v0.1 release fixture smoke coverage for FastAPI, pytest, Pydantic,
  SQLAlchemy, mixed, dynamic-unknown, and low-support examples, plus a test-only
  strong FastAPI semantic-support fixture that validates family reads, stale
  evidence fallback, leakage guards, and a no-worker exact-anchor FastAPI
  positive path without claiming production Python semantic-provider support.
  The no-worker release path now also exercises the committed `stale-evidence`
  fixture for source mutation/deletion and the FastAPI/APIRouter route-method
  variation fixture across `delete`, `get`, `head`, `options`, `patch`, `post`,
  and `put`.
- Python discovery regression coverage now explicitly covers `.venv`, `venv`,
  `env`, `.tox`, `.nox`, `__pycache__`, `.pytest_cache`, `.mypy_cache`,
  `.ruff_cache`, `build`, `dist`, `site-packages`, and nested Python runtime
  cache/dependency directory segments as default exclusions.
  Dynamic release smoke now asserts each dynamic boundary is persisted as typed
  `UNKNOWN`, blocked from claim-input readiness, and absent from derived
  support. Worker and parser regression tests now also distinguish safe literal
  `importlib.import_module(...)` anchors from unsafe/nonliteral dynamic imports,
  cover `sys.path.insert`, and prove plain `getattr(...)` assignments do not
  become dynamic call-target evidence. They also pin generic Python
  `module`/`function`/`async_function`/`class`/`method` code-unit output apart
  from framework-specialized units.
- Metadata-only algorithm paper archive for syntax, semantics, retrieval,
  graph fingerprints, alignment, anti-unification, clustering, evidence
  selection, evaluation, and installer supply-chain references.
- Parallel-agent implementation and post-implementation logic-review
  requirements in the mirrored agent contract.
- Historical TypeScript/JavaScript-first MVP language policy, now superseded by
  ADR-0011's Python-first v0.1 target.
- Pattern-family-first CLI command surface, with CodeGraph-style graph commands
  rejected as top-level v0.1 commands.
- Stable `stats --json` output that exposes metric-kind vocabulary and
  repo-shape diagnostics without reporting measured token savings or source
  snippets.
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

### Fixed

- `repogrammar index`/`sync` `semantic_facts` totals (surfaced by `index --json`
  and `status`) now include TS/JS-derived support facts. The reported count had
  omitted `repogrammar-tsjs-derived` facts even though they were recorded and
  fed family construction, undercounting the total for Express/Jest/Vitest
  repositories; the reported total now equals the facts actually stored in the
  active generation.
