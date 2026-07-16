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
- Release-wrapper tests must classify glibc family and architecture-specific
  minimum versions before download, cover musl/old/unknown rejection offline,
  and prove a concurrent npm cache activation loser never deletes the winning
  install. Npm package tests must create the real tarball under a temporary
  directory, inspect its files/metadata, install it into an isolated prefix
  offline, execute its wrapper against local fake release assets, and remove
  the tarball with the temporary directory.
- Native CI must run the PowerShell source-only installer contract on Windows.
  That job is platform evidence for the contributor path only and must not
  upload or imply a Windows release artifact.
- Tests must not modify real repository files unless the test is explicitly
  exercising a temporary copy.
- Process-boundary tests that rely on inherited child pipes must make child
  lifetime and signal handling explicit instead of depending on
  platform-specific wrapper behavior.
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
- ADR-0023 filesystem-confinement work requires deterministic barriers or
  test-only hooks, never sleeps, for file/dir/root replacement before relative
  opens. Native Unix suites must cover symlink swaps; native Windows suites
  must cover junction/reparse swaps even when symlink privilege is unavailable.
  Source-store tests must swap an intermediate parent and prove validated
  `Component::Normal` names are opened one at a time. Unix FIFO replacement and
  relevant Windows device/reparse cases must return within an explicit bound
  without hanging before metadata. An outside sentinel must prove rejected
  targets were never opened.
- Dependency admission for that work additionally requires exact resolved-tree
  provenance/advisory/build-script review, compile proof on both Linux and
  Apple architectures plus Windows MSVC, and native Linux/macOS/Windows
  no-follow, special-file-safe open, and confinement artifacts.
  Cross-compilation or one-host tests alone do not close the limitation.
- CLI not-implemented behavior must be stable and asserted.
- Repository-guard required-document changes must have a focused temporary-
  repository test that proves every newly registered path produces an exact
  `RequiredDocumentMissing` violation when absent.
- CLI missing-index fallback tests must cover both human-readable output and
  `--json` output for the query command surface.
- Repo-local lifecycle tests must use temporary workspaces and cover init
  layout, idempotent repair, Git exclude hygiene, optional root `.gitignore`
  marker writes, `REPOGRAMMAR_DIR` override validation, symlink/file conflicts,
  human and JSON status/doctor output, explicit status
  `manifest_schema_version` and `storage_schema_version` fields, explicit
  doctor `checks.manifest_schema_version` and `checks.storage_schema_version`
  fields, source-free status/doctor `readiness` states and local-state hygiene,
  `.repogrammar/` ignored/tracked-risk reporting, `.codegraph/` foreign
  provider reporting with `managed_by_repogrammar: false`, no absolute path or
  source-text leakage from readiness output, status/doctor storage layout,
  mutable-database presence,
  legacy-generation-layout presence, and WAL/SHM sidecar fields, no ambiguous
  status or doctor `schema_version` fields, JSON-parsed manifest validation
  with reordered valid fields and invalid required fields, corrupted manifests,
  missing subdirs, diagnostic-only doctor findings for missing or invalid
  `.repogrammar/.gitignore`, `.git/info/exclude`, root `.gitignore` markers,
  and `receipts/init.json`,
  `uninit --yes`, unlock inspection without `--force --yes`, confirmed stale
  `index.lock` removal with `--force --yes`, active/unknown/invalid lock
  refusal, PID-reuse-aware stale lock classification when the platform exposes
  live process start time, shared process-liveness policy coverage,
  daemon/SQLite lock preservation, repo-local autosync
  enable/status/disable config behavior, autosync daemon-lock inspection,
  repository readiness rejecting daemon locks whose PID is live but whose Unix
  command line is not `repogrammar autosync run`, and redacted logs metadata.
- File discovery tests must use temporary workspaces and cover TS/JS inclusion,
  Python `.py` inclusion, unsupported module extensions,
  default dependency/build/generated/state-dir
  exclusions, Git-ignored files when Git is available, safe Git-unavailable
  warnings, parent Git worktree ignore rules for subdirectory projects, the
  inclusive 1 MB size boundary, oversized skips, strict SHA-256 hash
  generation, bounded max-plus-one content reads for hashing,
  deterministic ordering, symlink escape skips, invalid roots, strict gitignore
  failure when Git ignore checks are unavailable, and absence of source snippets
  or absolute paths in reports. Aggregate resource coverage must use low private
  test limits to prove exact and plus-one behavior for accepted files, accepted
  bytes, reported skipped paths, visited entries, and directory depth; zero-byte
  file counting; oversized/unsupported/symlink/Git-ignored non-consumption;
  deterministic broken-symlink `Unreadable` classification;
  bounded single-directory buffering; path/source-free errors; and Git-warning
  deduplication. Application coverage must prove typed invalid-input mapping and
  that failure precedes generation preparation/activation. Autosync fingerprint
  tests must independently prove its exact/plus-one accepted-file,
  accepted-byte, visited-entry, and depth gates, intentional Git-ignored
  supported-file charging, and no-path/source-leak errors.
  Python discovery coverage must
  include common virtualenv/cache/dependency/build directories such as
  `.venv`, `venv`, `env`, `.tox`, `.nox`, `__pycache__`, `.pytest_cache`,
  `.mypy_cache`, `.ruff_cache`, `build`, `dist`, and `site-packages`,
  including nested path segments where applicable, and Git-ignored `.py` files
  in root and parent-worktree subdirectory projects.
- Go discovery-only coverage must include stable `go`/`go-config` tokens,
  `.go` source and `_test.go` inventory, root/nested `go.mod` and `go.work`,
  normalized-path rejection, dot/underscore, `vendor`, and `testdata` path
  classification, the dated GOOS/GOARCH suffix-shape snapshot without ambient
  selection, exact/plus-one 1 MiB behavior, symlink refusal, deterministic and
  source-free persistence, and incremental zero-parse metadata deltas for Go
  source/config additions, removals, and modifications while those tokens are
  absent from `ParserProjectContext`. Mixed repositories must retain their
  supported-language facts/families and `syntax_only_code_units` mode even on
  an unchanged round with zero parser attempts; tampered legacy Go claim rows
  must be omitted by generation-replacement copy-forward without being counted
  as cleared dirty markers. The default all-Go product path must prove that its
  source store is never called, warnings are aggregated by language token
  without paths, and no units, facts, IR, or families are produced. Marker
  scanning and Go project-context invalidation remain frontend/IR obligations.
- PHP discovery-only coverage must include stable `php`/`php-config` tokens;
  exact case-sensitive `.php` and literal `.php` handling; exact root/nested
  `composer.json`, `composer.lock`, `phpunit.xml`, and `phpunit.xml.dist`
  basenames with configuration precedence; normalized-path rejection; deferred
  `.inc`/`.phtml`/`.phpt`/`.php.dist`/`artisan`/`composer.phar`/`auth.json`
  shapes; and PHP-only `.composer`/`.phpunit.cache` exclusions without globally
  pruning other languages. Exact `vendor` must remain globally excluded.
  Coverage must include binary bytes, exact/plus-one size/resource limits,
  symlink refusal, deterministic path/raw-byte-hash/size/token persistence, and
  Git-aware discovery without source text or paths in warnings. PHP-only
  indexing must bypass the source store and parser, emit at most one truthful
  warning per accepted token, report `file_manifest_only`, and produce no unit,
  IR, fact, typed `UNKNOWN`, family, or project model. Mixed repositories retain
  `syntax_only_code_units`; incremental tests must prove token-based add/modify/
  remove deltas and legacy-claim purge while preserving metadata. Autosync
  retains its intentional Git-independent charging. Custom `vendor-dir`,
  project-profile invalidation, and semantic admission remain later-stage tests.
- Ruby discovery-only coverage must include stable `ruby`/`ruby-config` tokens;
  configuration-before-source precedence for `gems.rb`; literal `.rb` and
  `.gemspec` basename handling; normalized-path and invalid-input rejection;
  Ruby-only `.bundle`/`.ruby-lsp` exclusions with the stable
  `language_specific_exclusion` skip token; and proof that those components
  remain eligible to other languages. It must cover binary content, exact and
  plus-one size/resource limits, symlink refusal, deterministic path/hash/size/
  token persistence, and Git-aware discovery without source text or paths in
  warnings. Ruby-only indexing must bypass the source store and parser, emit at
  most one truthful warning per accepted token, report `file_manifest_only`
  with a deferred parser, and produce no units, IR, facts, typed `UNKNOWN`s, or
  families. Mixed repositories retain `syntax_only_code_units`. Incremental
  tests must prove token-based add/modify/remove metadata deltas and purge
  seeded legacy Ruby claims while preserving file metadata; autosync
  fingerprint tests must preserve its intentional Git-independent charging.
  Ruby project-context invalidation remains a later frontend obligation.
- Swift discovery-only coverage must include stable `swift`/`swift-config`
  tokens; exact case-sensitive `.swift` including basename `.swift`; exact
  root/nested `Package.swift`, `Package.resolved`, `.swift-version`, and
  complete ASCII `Package@swift-M[.m[.p]].swift` grammar with configuration
  precedence; malformed version-manifest lookalikes that remain `.swift`
  source; normalized-path rejection; and Swift-only `.build`/`.swiftpm`
  exclusions without globally pruning other languages. It must cover binary
  bytes, exact/plus-one file and aggregate resource limits, source/config
  symlink refusal, deterministic path/raw-byte-hash/size/token persistence, and
  Git-aware discovery without source/config leakage. Swift-only indexing must
  bypass the source store and parser, emit at most one warning per token, report
  `file_manifest_only`, and produce no units, IR, facts, typed `UNKNOWN`s,
  project records, or families. Mixed repositories retain syntax mode;
  incremental tests must prove add/modify/remove and unchanged metadata deltas,
  whole-manifest warning retention, and seeded legacy source/config claim purge.
  Autosync must track accepted Swift source/config and cross-language files below
  Swift-specific exclusions while ignoring excluded Swift candidates. Project-
  context invalidation remains a later frontend obligation.
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
  snippets or absolute paths in CLI output and stored metadata. Incremental
  `sync` coverage must include project-context fallback when TS/JS, Python, or
  Rust source inventories change and must assert stale unresolved-import
  UNKNOWNs are not copied forward after a new repo-local import target appears.
  Go, PHP, Ruby, and Swift inventory coverage must instead prove token-based incremental
  add/modify/remove deltas for their source/configuration tokens, zero
  source-store/parser calls for those paths, whole-manifest warning retention,
  honest `file_manifest_only`/deferred-parser output for inventory-only and
  empty generations, syntax mode for unchanged mixed generations, and purge of
  seeded legacy Go/PHP/Ruby/Swift units, IR, facts, derived support, and families while
  file metadata survives.
  Progress renderer tests must also cover exact integer percentages and
  interactive TTY progress as single-line carriage-return updates with one
  final newline rather than one terminal line per event.
- Auto-sync CLI tests must cover `autosync` defaulting to `status`,
  `enable/start/status/stop/disable/run` routing, `--poll-ms` and
  `--debounce-ms` validation, `--progress` compatibility, strict-gitignore
  propagation, semantic worker environment inheritance, `start` rejecting
  invalid semantic worker argv environment before launching a background worker,
  bounded startup readiness from matching child PID plus startup nonce and
  child liveness, deterministic immediate-child-exit, lock-refusal, timeout,
  and successful-readiness paths, serialized lifecycle ownership plus exact-
  record stop/guard cleanup under a concurrently replaced lock, human and JSON output, and no nonce, environment,
  credential, source, or absolute-path leakage. Tests must assert typed startup
  semantic classes rather than incidental lower-level error strings. Default
  tests must not start or kill real user background services;
  product-runtime background behavior may be covered through
  temporary-repository smoke tests or ignored/manual tests.
- Family storage tests must cover generation-scoped family records, members,
  variation slots, family-bound evidence, building-only writes, non-`UNKNOWN`
  family validation requiring evidence, active-generation list/show reads,
  summary and candidate read-model APIs, query plans for read-path indexes,
  tampered family/evidence row rejection on detail reads, and no source snippet
  or absolute path leakage.
- Family builder and query tests must cover framework-heuristic-only groups
  staying `UNKNOWN`, semantic/dataflow-supported repeated candidates becoming
  eligible family records, role-incompatible semantic facts staying
  insufficient, future Python provider-origin facts staying subject to exact
  canonical target compatibility and same-code-unit path/hash/range evidence,
  foreign-provenance `UNKNOWN`s not producing family effects or compatibility
  blockers that suppress otherwise supported families, the single family
  `UNKNOWN` classifier feeding blocking, non-blocking, query-visible, and
  compatibility-feature decisions, all six implemented language structural
  support-derivation paths matching that classifier without copied reason/claim
  tables, TS/JS blocking units producing no derived support, and Rust
  `rust_framework_attribute_binding` / `rust_axum_route_identity` blockers
  preventing support promotion,
  no-family active generations returning typed
  `InsufficientSupport`, exact family/member lookup versus fuzzy
  find/explain/check lookup, deterministic local `PARTIAL_CONTEXT` for a
  uniquely resolved indexed target without family evidence, ambiguity abstention
  before partial context, fuzzy candidate cap abstention before broad hydration,
  path-plus-symbol targets, root-file targets,
  `path:line` and `path:start-end` target forms, advisory partial `check`
  output without proof-like fields, short-substring false-match rejection, stale
  family-evidence refusal with `StaleEvidence`, compact/evidence/deep output
  mode behavior, target and token-budget validation, greedy evidence coverage
  metadata, default read plans with repo-relative paths, content hashes, byte
  ranges, explicit source-span opt-in, line-numbered rendered spans, stale or
  hash-mismatched span omission with Read/Grep fallback guidance, missing
  variation/exception coverage reporting, JSON/human CLI output, advisory
  `check` behavior, and absence of source snippets unless explicitly requested.
- Read-path performance architecture must keep an ignored reproducible
  benchmark fixture under `src/rust/integration_tests/`. The
  `read_path_benchmark_fixture_measures_bounded_query_paths` test generates
  12,000 code units, 300 families, 25,000 family-evidence rows, and 18,000
  semantic facts, then measures stats, stats-with-unknowns inventory, family
  summaries, exact family lookup, exact member lookup, fuzzy path lookup, and
  the corresponding MCP `show_family`/`find_analogues` paths. It is
  intentionally ignored so default quality gates stay deterministic; run it
  explicitly with
  `cargo test --lib read_path_benchmark_fixture_measures_bounded_query_paths -- --ignored --nocapture`
  when validating read-path performance work.
- Rust self-dogfood tests must cover `.rs` and `Cargo.toml` discovery,
  Tree-sitter Rust code-unit extraction, structural Rust anchors, typed
  `MacroOrPreprocessor`, `BuildVariantAmbiguity`, `FrameworkMagic`, and
  `UnresolvedImport` UNKNOWN boundaries, support>=3 for internal Rust families,
  low-support abstention, default source-free CLI/MCP output, explicit
  source-span opt-in, safe and unsafe module resolution, target-specific Cargo
  dependency inventory, Cargo feature context on source-level cfg UNKNOWNs,
  family UNKNOWN recovery that preserves that cfg feature context,
  Cargo build-script non-execution with a sentinel file, repository-level
  build-variant blocking, and fixtures under
  `src/fixtures/rust/release/v0_2/` including `internal_family_gates`,
  `parser_adapters`, `installer_actions`, `product_tests`, `low_support_family`,
  `macro_cfg_unknowns`, `trait_dispatch_unknowns`, `module_resolution`, and
  `cargo_build_blocked_family`.
- Rust general framework preview tests must cover use-path-gated serde/thiserror/
  tokio/clap derive and attribute anchors and axum literal
  `Router::new().route(...)` receiver tracing, derive-without-use blocking
  `rust_framework_attribute_binding`, non-literal/untraceable axum
  `rust_axum_route_identity`, non-blocking `rust_derive_expansion` and axum
  middleware/extractor subclaims, Serialize-only vs both-trait serde non-merge,
  `#[cfg]` still blocking a framework unit, unchanged self-dogfood behavior,
  `repogrammar-rust-derived` support>=3 family gates, and fixtures under
  `src/fixtures/rust/release/v0_2/` including `serde_exact_models`,
  `thiserror_exact_errors`, `axum_exact_routes`, and `derive_lookalikes`, plus
  the `rust_serde_unresolved`/`rust_serde_resolved` unknown-reduction pair.
- Java/Spring preview tests must cover `.java` discovery, Tree-sitter Java
  parser extraction, exact imported/FQN Spring MVC/stereotype/Spring Boot/Spring
  Data anchors, `UnresolvedImport` for Spring-lookalike simple annotations
  without exact imports, no route-family support outside exact controllers,
  nonliteral route-path UNKNOWN subclaims, `repogrammar-java-derived`
  safe-origin promotion, support>=3 family gates, and rejection of structural
  parser anchors, substring targets, and wrong-origin facts as direct family
  support.
- Java framework-deepening (Wave J1) tests must additionally cover exact
  imported/FQN JUnit 5/4 and TestNG test methods, JPA/Jakarta Persistence
  entities under dual `jakarta`/`javax` roots with jakarta-vs-javax
  non-clustering, JAX-RS/Jakarta REST `@Path` resource classes and verb methods
  with a verb-outside-`@Path` block, mixed JUnit 4/5 `@Test` conflict blocking,
  Lombok non-blocking generated-members `UNKNOWN`, Spring Data derived-query
  metadata as non-support, the `test_annotation_lookalikes` negative smoke, and
  the `java_junit_unresolved`/`java_junit_resolved` benchmark pair
  (`java_test_annotation_model`).
- Java test-data-link tests must cover exact same-class/class-like JUnit
  `@MethodSource` scalar, array, direct-repeatable, blank/omitted same-name, and
  complete-set resolution; coexistence with `ValueSource`/`CsvSource`; exact
  TestNG named/default `@DataProvider` resolution; and structural replacement
  facts that never become family support. Negative coverage must preserve typed
  `UNKNOWN` or conflict for external/signature/provider-class, inherited,
  type-level/`@ParameterizedClass`, explicit-container/meta, duplicate or
  overloaded, `PER_CLASS` non-static, dynamic, partial-positive,
  wildcard/colliding/local-shadow/malformed imports, unknown identity, invalid
  non-parameterized use, mixed framework identity, parse-degraded inventories,
  missing targets, lookalikes, nested annotations, enum-constant/anonymous
  boundaries, and comments/text blocks that resemble annotations or imports.
  TestNG assignment-name trivia must preserve the runtime provider name without
  joining identifier fragments. Resource regressions include 2,048 decoy
  methods plus 512 lookups, a 65-annotation abstention cap, and nested/text-block
  lexical cases. Product coverage uses `java_test_data_{unresolved,resolved}`
  and a six-member release fixture to prove UNKNOWN reduction without changing
  the original JUnit/TestNG family targets.
- C# preview tests must cover `.cs` discovery skipping the MSBuild `obj/`
  directory, Tree-sitter C# parser extraction, exact using/FQN-gated ASP.NET
  Core controller/action, minimal-API route, EF Core `DbContext`/`DbSet`, and
  xUnit/NUnit/MSTest anchors, `UnresolvedImport` for lookalike attributes without
  usings, `FrameworkMagic` for actions outside a controller and MSTest methods
  without a `[TestClass]`, `BuildVariantAmbiguity` for `#if` regions,
  `repogrammar-csharp-derived` safe-origin promotion, support>=3 family gates
  with HTTP-method clustering, and release-fixture product smokes for exact
  controllers, exact xUnit tests, exact same-class xUnit `MemberData`, framework
  lookalikes, preprocessor variants, low support, and the unknown-reduction
  resolved/unresolved pair. `MemberData` tests must prove unique public-static
  field/property/zero-argument-method links and retain typed UNKNOWNs for
  partial, inherited, generic, non-class, overloaded, private/instance,
  conditional, external-type, extra-property, parameterized, dynamic-name,
  mixed exact/lookalike, nested-scope, and non-`Theory` forms.
  ERROR/MISSING attribute/provider/class shapes must remain UNKNOWN. A dense
  conditional-region fixture must exercise the merged-interval binary lookup
  under the default one-mebibyte source bound.
  Using-context tests must prove that comments, strings, and sibling namespaces
  cannot corroborate an attribute while a namespace-local directive applies
  only inside that namespace.
- C/C++ preview tests must cover `.c`/`.h` (C grammar) and
  `.cc`/`.cpp`/`.cxx`/`.hh`/`.hpp`/`.hxx` (C++ grammar) discovery skipping the
  CLion `cmake-build-debug`/`cmake-build-release` directories, both the
  function-definition and call-expression macro parse shapes, include-evidence
  gating for GoogleTest/Catch2/doctest/Boost.Test, Catch2-vs-doctest
  disambiguation (`ConflictingFacts`/`UnresolvedImport`), bounded arity and
  GoogleTest's official no-underscore name rule for `TEST`/`TEST_F`/`TEST_P`,
  Catch2's quoted adjacent square-bracket tag-list grammar (including
  free-form, unbalanced, and trailing-garbage negatives), and exact
  identifier/string/decorator-shape validation for every supported macro,
  including the Boost ordinary-call whitelist and per-decorator arity and
  literal-kind shapes while template-only decorator forms remain non-claims,
  typed `MacroOrPreprocessor` rejection for excess/lookalike arguments and the
  explicit doctest/Boost decorator non-claims, nested Boost suite pairing,
  valid root/master-suite cases, orphan end plus following-case corruption, and
  EOF-unclosed suite blocking, `::testing::Test` fixture anchors only behind the
  same exact unconditional gtest/gmock include gate (including missing,
  lookalike, and conditional negative cases), exact unconditional include recognition,
  rejection of commented/string/pseudo/non-exact and conditional-branch include
  corroboration, nested-conditional coverage, true whole-file include-guard
  exclusion, false partial/value-defining guard rejection, complex and unclosed
  conditional build-variant blocking, ERROR-node `cpp_macro_boundary`, Qt
  `Q_OBJECT`/string SIGNAL/SLOT non-blocking context,
  `compile_commands.json`/`vcpkg.json`/`conanfile.txt` inventory and
  malformed-config `UNKNOWN`, `repogrammar-cpp-derived` safe-origin promotion,
  support>=3 family gates, and release-fixture product smokes for exact
  GoogleTest and Catch2 families, macro lookalikes and exact-include contract
  violations, preprocessor variants, low support, and the unknown-reduction
  resolved/unresolved pair.
- Python bounded preview tests (Wave E1) must cover, in
  `src/workers/python/worker.test.py`, every new unit kind (`django_model`,
  `django_url_pattern`, `django_test`, `flask_route`, `unittest_test_method`,
  `click_command`, `typer_command`, `celery_task`), the exact-import
  abstention for Django/Flask lookalikes, the shape anchors (field/method/
  param count, `class Meta`, Flask `http_method`), and the typed UNKNOWN
  recipes (`python_django_url_identity`, `python_django_model_identity`,
  `python_flask_route_identity`, `python_django_settings_behavior`,
  `python_django_string_dispatch`, `python_unittest_patch_target`,
  `python_celery_runtime_routing`). Product smokes under
  `src/fixtures/python/release/v0_2/` must prove django/flask/unittest/
  django-urls families with `repogrammar-python-derived` support>=3, that
  framework lookalikes and low-support fixtures form no family, and the
  `python_django_unresolved`/`python_django_resolved` unknown-reduction pair
  reduces the `django_project_model` bucket with a source-backed derived
  support fact.
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
  configurators, fake prompts, and receipt behavior. Native probe tests must
  cover exact target-specific not-found classification, successful config
  parsing, unexpected failed output fail-closed behavior, successful
  unrecognized output becoming a preserved malformed state without raw output
  or path leakage, no-receipt foreign entries, receipt/native missing or command
  drift, native+receipt agreement at the current authority (`OwnedCurrent`),
  native+receipt agreement at an obsolete authority (`OwnedOutdated`) followed
  by safe refresh, refresh snapshot restoration, post-add exact presence, and
  rollback on the final product `tools/list` self-test. Execution assertions
  must keep newly configured targets separate from reconfigured pre-existing
  targets. Any real native-agent CLI integration test must be explicitly
  ignored or feature-gated outside default CI.
- Setup tests must cover option parsing, exactly one live confirmation,
  noninteractive `--yes`, dry-run zero writes under a fresh temporary HOME,
  missing/no-live agents with repository-only success, existing repository and
  agent state, obsolete owned integration refresh, preservation of a
  pre-existing owned integration after a downstream failure, rejection of a
  partial/empty configured-target outcome, foreign/malformed receipts with all
  limitations reported, native/receipt/index/auto-sync/MCP failure classes,
  family inventory `Available(0)`/`Available(N)`/`Unknown` without mapping a
  failure to zero, machine-only rollback, active-generation preservation,
  idempotent reruns, distinct repository-initialization and repository-index
  stage labels, and sanitized human/JSON output. JSON assertions must cover
  ready/blocked agent targets, product self-test state, agent-query readiness,
  repository-index readiness, auto-sync readiness, family-evidence state, all
  limitations, and `suggested_question: null` for repository-only success. A
  real product-binary MCP self-test remains required. Default tests must use
  fake native configurators and must never
  mutate a developer's agent configuration.
- Installer wrapper tests must run without network access and without real
  native-agent CLIs. They must cover shell syntax validation and a local fake
  release-artifact path for `src/install/repogrammar-install.sh`, including
  checksum verification, CLI command installation, bundled worker asset
  installation, delegated
  `repogrammar install` / `repogrammar uninstall` invocation through a fake
  binary, source-checkout `--from-source` install/configure dogfood without
  network access, actionable no-release failure text, refusal to replace an
  unmanaged command without the `--replace-unmanaged-command` opt-in plus
  backup/replacement with it, a directory command path still failing, missing-worker
  artifact rejection, non-regular-file (symlink/hardlink) release-member
  rejection, release-workflow tag/version-guard and artifact and
  installer-script checksum contract checks, tag publication credential
  preflight, explicit build-only workflow dispatch, staged GitHub-assets-before-
  npm publication, exact packaged-artifact `version`/setup-dry-run/product-MCP
  `find`/advisory-`check` smoke, plus macOS product CI and the Windows
  source-only boundary contracts,
  target/scope pass-through for comma-separated, `none`, and
  local-scope install requests, stale PATH prune failure propagation, and
  command removal.
  Default tests must not use wrapper scripts to call real `codex` or `claude`
  binaries.
  Windows PowerShell wrapper coverage must include `src/install/install.ps1`
  refusing a default/non-`-FromSource` install before network or filesystem
  writes even when a fake release directory is supplied; absence of a release
  download implementation; source-checkout `-FromSource` installation with an
  already built local binary; bundled worker asset installation; unmanaged
  command refusal without `-ReplaceUnmanagedCommand` plus opt-in backup with
  it; no `.repogrammar/` mutation; locked stale PATH prune failure propagation;
  and nonzero propagation when delegated `repogrammar install` fails.
- Npm launcher tests must run without network access, without Rust/Cargo, and
  without real native-agent CLIs. They must use local fake release artifacts to
  cover the full supported public-preview platform/artifact matrix,
  unsupported platform/arch rejection, explicit Windows rejection, checksum
  rejection, binary/worker cache installation, missing-worker artifact
  rejection, unexpected-entry rejection,
  non-regular-file (symlink/hardlink) release-member rejection with no partial
  cached binary left behind, `REPOGRAMMAR_BINARY` local dogfood bypass, argument
  forwarding including target lists, local scope, `--print-config`, and the full
  `setup` option vector, plus npm package shape via `npm pack --dry-run`.
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
- TypeScript worker executable tests must run the checked-in Node worker,
  validate bounded operation requests for module/export/package resolution,
  cover dependency-free structural fallback and TypeScript compiler-API
  provider-resolved output when a compiler module is available, accept large
  changed-file requests below the shared 1 MiB stdin envelope, reject malformed
  requests, and prove request paths and source snippets are not echoed in
  errors.
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
  repo-local `include_router` context crossing the Rust boundary with its exact
  seven-assumption envelope and a low-cardinality, source-free prefix shape,
  malformed/raw prefix assumptions remaining rejected, and dynamic prefix or
  unresolved binding outcomes crossing as typed `fastapi_router_prefix` /
  `fastapi_router_binding` `UNKNOWN`s rather than parser failures,
  same-function FastAPI service-call context anchors with reassignment
  invalidation, typed `UNKNOWN` output for dynamic decorators, unresolved bare
  decorators, monkey patches, dynamic calls, unsafe or nonliteral
  `importlib.import_module(...)` calls,
  `sys.path.append`/`sys.path.insert` import-environment mutation, and
  unresolved cases, plus safe literal dynamic-import anchors and plain
  `getattr(...)` assignments that do not become dynamic call-target UNKNOWNs,
  oversized request
  rejection, unsafe path and symlink-escape rejection, bounded semantic-mode
  source reads, checked-in-worker self-analysis within a bounded subprocess
  timeout and through the Rust parser boundary, responses above the former 1
  MiB ceiling but below the explicit 2 MiB response limit, and absence of
  source snippets, absolute paths, or unsafe dynamic-import literal targets.
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
- Product runtime stats tests must include a temporary TSX/React-Native-like
  project with repeated component-shaped code units and no Python files or
  supported TS/JS preview frameworks. The fixture must prove that indexing
  succeeds, top-level `python_family_eligible_units` stays `0`, stats reports
  source-free TS/JS indexed inventory, React/RN family support remains
  `unsupported`, `families --json` does not emit React/RN family rows, and an
  exact-path `check --json` can return source-free `PARTIAL_CONTEXT` instead of
  a family or conformance claim.
- Conservative TS/JS exact-anchor tests live alongside the parser
  (`src/rust/adapters/parsing/tsjs/`), the family gate
  (`src/rust/application/family.rs`), the derivation pass
  (`src/rust/application/indexing.rs`), and the product smoke
  (`src/rust/bin/repogrammar.rs`). They must cover Express positive routes,
  Next.js App/Pages file-convention positives including async const route
  handlers, exact ES import/CommonJS require/CommonJS destructuring-alias
  framework bindings, Fastify shorthand/full route positives, Prisma
  client/query/transaction positives, Drizzle schema/query positives including
  `db.query.<table>.findMany/findFirst`, object-literal/dynamic-receiver/
  dynamic-method/reassigned/shadowed negatives, raw and unsupported bulk query
  negatives, typed TS/JS `UNKNOWN` facts for unsafe/unresolved receiver, runner,
  route, client, and query boundaries, bounded
  `package.json`/`tsconfig.json`/`jsconfig.json` project-config context, bounded
  static relative/path-alias/rootDirs import resolution, typed `UNKNOWN` for
  dynamic import, non-literal or conditional `require`,
  unresolved/conflicting aliases or rootDirs candidates, ambiguous star
  re-exports, Jest/Vitest imported positives,
  ambient-in-test-file positives only with package/config test-runner context,
  custom-wrapper and foreign-import negatives, that
  `FRAMEWORK_HEURISTIC` facts never derive support, that only
  `repogrammar-tsjs-derived` `DATAFLOW_DERIVED` facts with exact whitelisted
  targets form families, that TS/JS families require at least three compatible
  support facts, that complete-link clustering rejects single-link bridge
  members, that route/test/component/query variation slots are recorded from
  context profiles, and that default JS/TS query output stays source-free while
  `--include-source-spans` / `include_source_spans=true` returns bounded
  hash-checked line-numbered spans. They must also cover Zod exact `z.object`
  schema positives, NestJS `@Controller`/`@Get` exact-import route families and
  the DI/dynamic-module non-blocking subclaims, Hono `new Hono()` literal-route
  positives, Mocha/`node:test` runner aliasing with a required-equal
  `runner_kind` that never merges mocha with vitest families, and blocking
  `tsjs_nest_controller_identity`/`tsjs_hono_receiver` negatives. Positive TS/JS
  family fixtures live under
  `src/fixtures/typescript/release/v0_2/express_exact_routes`,
  `jest_vitest_exact_tests`, `next_exact_routes`, `fastify_exact_routes`,
  `prisma_exact_repositories`, `drizzle_exact_repositories`, `zod_exact_schemas`,
  `nest_exact_controllers`, `hono_exact_routes`, and `mocha_exact_tests`;
  package-only, raw, dynamic, and unsupported lookalikes live under
  `framework_adapter_negative_cases`, `unsupported_framework_lookalikes`, and
  `tsjs_new_framework_lookalikes`. The NestJS UNKNOWN-reduction pair lives under
  `src/fixtures/unknown_reduction/tsjs_nest_unresolved` /
  `tsjs_nest_resolved`.
- Python v0.1 tests must cover the implemented CPython `ast` frontend output,
  FastAPI, pytest, SQLAlchemy, and Pydantic structural positives, Python
  language/kind token stability, product `index`/`units` smoke coverage,
  path-derived module-name anchors, CPython `symtable` scope anchors, private
  bounded project-config summaries, default-index persistence of root
  `pyproject.toml`, `setup.cfg`, and `setup.py` as `python-config`/
  `project_config` structural context or typed/conservative config results.
  Real `RepoGrammarSourceParser` regressions must prove exact root `setup.py`
  routing, CPython-AST provenance and non-execution, literal source-root
  extraction, malformed/incomplete config as `MissingProjectConfig`, strict
  zero-positional/no-unpack setup and finder argument shapes, complete unique
  string-to-string `package_dir` mappings, builtins-qualified mutation
  invalidation, definite top-level-`raise` reachability, empty `setup()`, and
  rejection of similar or nested config paths. Application tests must cover the
  three-format structural root union without claiming packaging precedence;
  RecordingParser coverage alone is insufficient for the product route. Tests
  must also preserve the source-store and
  Python frontend size/error boundaries and assert `setup.cfg` provenance uses
  `configparser`, not `tomllib`. Semantic-worker-compatible project-mode repo-local
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
  `Session.get`/`Session.commit`/`Session.rollback`/`Session.scalar`/`Session.scalars`
  and async equivalents becoming exact repository-method anchors, bounded
  SQLAlchemy `self.session`/`self.db` role propagation from `__init__`, and
  reassigned receivers or custom query wrappers not becoming exact session-call
  anchors, Pydantic field,
  field-type, model-config, nested Config, computed-field, field-validator,
  legacy validator, and model-validator anchors staying out of support derivation,
  Pydantic validator body calls remaining non-blocking side-effect UNKNOWNs,
  imported external Pydantic/SQLAlchemy bases remaining framework-identity UNKNOWNs,
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
  metrics code. Internal-policy tests must also prove `ClaimImpact` parity with
  the authoritative family classifier, `ResolutionClass` static-vs-runtime
  boundary cases, unregistered-mechanism conservative defaults, and unchanged
  legacy public class/count/JSON projections. An external-crate compile test
  must preserve `ClaimUnknown { class: ... }` struct literals and public class
  field access. `blocks_support` assertions must depend only on claim impact;
  recovery-code assertions must depend only on resolution class and a
  registered mechanism.
- The UNKNOWN regression benchmark test
  `product_runtime_unknown_regression_benchmark_tracks_mechanisms_without_false_certainty`
  must be updated whenever an analyzer intentionally reduces or reclassifies
  persisted semantic `UNKNOWN`s for its release fixtures. It runs real
  `init`/`resync`/`unknowns --json`/`families --json` product paths over
  Python, TS/JS, and Rust fixtures, pins language/reason/mechanism buckets, and
  guards against false certainty by requiring negative fixtures to remain
  family-free unless the fixture is explicitly promoted with separate
  positive/negative coverage.
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
default `init` active-index bootstrap, `--state-only` lifecycle repair,
auto-sync-after-index sequencing, and bootstrap failure preservation, bounded
redacted repo-local log tails,
JSON-parsed bootstrap manifest validation,
TS/JS, Python, Go, PHP, Ruby, and Swift discovery filtering/hash/path-safety behavior,
SQLite storage migration and generation-activation safety behavior, validated
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
operation-scoped TypeScript worker fallback and compiler-API behavior,
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

## Release readiness smoke matrix

Before cutting a public-preview tag or opening release-readiness changes,
contributors should run the normal local gate plus a source-checkout smoke
matrix that exercises installation boundaries without live machine writes.

- Fresh checkout smoke: clone the current checkout into `/tmp` and run a small
  Cargo product smoke such as `cargo test --workspace --all-features
  version_succeeds`. Use `git clone --no-hardlinks` when the local filesystem
  blocks hardlink creation.
- npm wrapper smoke: run `npm_config_cache=/tmp/repogrammar-npm-cache npm pack
  --dry-run`, then run
  `REPOGRAMMAR_BINARY=/absolute/path/to/repogrammar node src/npm/repogrammar.js
  version`. The direct binary override is contributor dogfood only; published
  npm use still downloads release artifacts.
- Repository lifecycle smoke: copy a committed release fixture to `/tmp`, run
  `repogrammar setup --project <tmp-fixture> --yes --no-autosync --json
  --progress never` with a fresh temporary HOME and no live agent target, then
  check `status --json`, `doctor --json`, `unknowns --json`, and
  `stats --unknowns --json` against that temporary project.
  Setup regression tests additionally require stale and unverifiable active
  indexes to resync, reject a false auto-sync start result, preserve a
  pre-enabled telemetry preference, downgrade zero supported pattern groups,
  keep that downgrade on a fresh active-index rerun by inspecting the real
  family inventory, keep inventory failures `unknown`, refresh consistent
  obsolete owned agent authority without later deleting that pre-existing
  integration, preserve successfully probed but unrecognized native agent
  configuration while continuing repository-only setup, omit the coding-agent
  question when no verified agent integration is ready,
  and render sanitized human failure ledgers for index, auto-sync, MCP
  self-test, and rollback failures.
- MCP serving smoke: send JSON-RPC `initialize`, `tools/list`, and `shutdown`
  messages to `repogrammar serve --project <tmp-fixture>` and verify that
  `repogrammar_context` is advertised.
- Installer planner smoke: run `repogrammar install --target all --scope global
  --dry-run` and `repogrammar uninstall --target all --scope global --dry-run`.
  These commands are human-plan dry runs today; do not add `--json` unless the
  installer contract grows JSON output.

Optional security scans are source-tree checks, not substitutes for the normal
Rust, worker, installer, and repo-guard gates:

- `gitleaks detect --source . --no-banner --redact`;
- `trufflehog filesystem --no-update --no-verification --force-skip-binaries
  --force-skip-archives --exclude-paths <exclude-file> --fail .`, with build
  outputs such as `.git/`, `.repogrammar/`, `target/`, and `node_modules/`
  excluded;
- `cargo audit` when `cargo-audit` is installed;
- `npm audit` only when a lockfile exists.

Unavailable optional scanners, missing lockfiles, and environment warnings such
as multiple `repogrammar` executables on PATH must be reported in the release
readiness summary. They are not silent passes.
