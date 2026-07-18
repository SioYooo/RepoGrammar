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
  upload or imply a Windows release artifact. The wrapper must explicitly
  return success after asserting an intentionally failing child invocation so
  the expected child status cannot become the native test process status.
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
  live process start time, including the one-second precision boundary of the
  Unix elapsed-time probe, shared process-liveness policy coverage,
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
  accepted-byte, visited-entry, and depth gates, Git-ignore parity with manual
  discovery (Git-ignored supported files are excluded from the accepted-file and
  byte budgets and from the digest, and a lowered accepted ceiling still accepts
  a repository whose only over-ceiling files are Git-ignored), the safe
  no-ignore fallback when Git is absent or errors, determinism across repeated
  passes, and no-path/source-leak errors. The batched `git check-ignore --stdin`
  helper must be covered against its per-file, index-aware equivalent.
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
  fingerprint tests must prove its Git-ignore parity with manual discovery.
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
- Generation write-session tests must cover the single-connection,
  bounded-batch write lifecycle with deterministic, test-only fault injection.
  Required cases: a clean build reports one measured connection open, far fewer
  committed transactions than rows, and the expected phase-checkpoint count; a
  standalone checkpoint commits the open batch and increments the checkpoint
  counter; a field- or referential-validation rejection that persisted nothing
  leaves a reusable `building` generation (never a false `failed`); an abandon or
  drop after at least one committed batch stamps `failed` and leaves the previous
  active generation readable and unchanged; a fault injected mid-record (after an
  evidence insert, before its fact insert) rolls the torn batch back atomically
  while committed batches survive; a commit-time fault leaves the batch open and
  discards it on rollback; a reader resolves the previous active generation
  throughout the build and only flips after activation; a generation status flip
  landing between batches is rejected at the next batch open; and finishing after
  an abandon, or finishing twice, is a typed error rather than a silent success.
  Faults are injected only through a `#[cfg(test)]` seam on the session;
  production builds neither compile the seam nor construct a fault.
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
  child liveness, a persisted `starting` phase before initialization, and
  `ready` only after repository validation, worker preflight, initial
  fingerprinting, log initialization, and the first successful heartbeat.
  Deterministic coverage must include immediate child exit, lock refusal,
  bounded timeout, first-heartbeat failure, and successful readiness, plus the
  exact low-cardinality startup codes `worker_environment_invalid`,
  `repository_fingerprint_failed`, `repository_state_unavailable`,
  `daemon_lock_refused`, `child_exited_before_ready`, `startup_timeout`, and
  `first_heartbeat_failed`. Human and JSON assertions must distinguish current
  `daemon_state`, `startup_state`, `startup_failure_code`, and
  `repository_ready` from the prior sync result exposed as
  `previous_autosync_attempt`; historical failure must not redefine current
  process readiness. Tests must also cover serialized lifecycle ownership plus
  exact-record stop/guard cleanup under a concurrently replaced lock and no
  nonce, environment, credential, source, or absolute-path leakage. Tests must
  assert typed startup semantic classes rather than incidental lower-level
  error strings. Default
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
- Write-path performance architecture must keep an ignored reproducible
  benchmark fixture under `src/rust/integration_tests/`. The
  `write_path_benchmark_fixture_measures_session_vs_per_record` test builds the
  same fixture corpus twice — once through one generation write session and once
  through the granular per-record store methods (each a one-shot session) — and
  reports elapsed wall-clock plus the adapter-measured connection-open and
  committed-transaction counts for both arms (read from the store's real write
  instrumentation, never asserted by construction). It asserts the session path
  opens exactly one connection and commits far fewer transactions than the
  record count, and that the granular path opens and commits once per record.
  The per-record arm is today's granular API, not the deleted historical code
  (which was bare autocommit inserts); the wall-clock ratio is a
  same-implementation comparison of one session versus per-record opens, not a
  before/after of the change. It is intentionally ignored so default gates stay
  deterministic; run it explicitly with
  `cargo test --lib write_path_benchmark_fixture_measures_session_vs_per_record -- --ignored --nocapture`
  when validating write-path performance work. Fixture-scale numbers are
  hardware-dependent; report them with a machine caveat.
- Write-session phase checkpointing must have a pipeline-level test that indexes
  a real repository through both the full and incremental pipelines and asserts,
  through the store's write instrumentation, that each pipeline opens exactly one
  write connection and checkpoints at its phase boundaries.
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
  no-active-generation fallback, active-generation typed `UNKNOWN`,
  static-alignment `check_conformance` certificates (alignment-status tokens
  with `runtime_equivalence: "UNKNOWN"`; typed abstention when conformance
  evidence is insufficient), exact `show_family` target handling, compact/evidence/deep output
  mode serialization, target and token-budget validation, metadata-only greedy
  evidence selection, metadata-only default read plans for all supported
  operations, explicit `include_source_spans` validation and rendering, stale
  or omitted span fallback guidance, missing variation/exception coverage
  reporting, JSON-RPC initialize/exact-one-tool tools/list/tools/call/shutdown
  handling, and absence of source snippets unless explicitly requested.
- MCP initialize guidance unit tests assert that the one authoritative guidance
  string shared with the installer-managed body contains the required positive
  triggers, narrow exceptions, and authority-docs -> one
  `find_analogues`/concrete-target/compact call -> returned `read_plan` -> typed
  fallback or CodeGraph-detail ordering clauses. These are prose-contract tests,
  not an executable agent classifier. Stable-release acceptance evidence must
  separately use a fresh session to prove a schema/prompt/Meaning Contract task
  follows that order and a deterministic `UNKNOWN`/fallback task reports the
  reason before CodeGraph; it must also show that the identical call is not
  repeated.
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
- Managed-instruction tests must cover exact current content version `2`, the
  exact unmarked legacy body classified as logical version `1`, exact
  unversioned global legacy `0`, single-byte/body
  drift becoming `foreign`, partial/duplicated
  markers becoming `malformed`, foreign/malformed preservation, exact legacy
  refresh, idempotent current content, atomic create/append/replace/remove, and
  unrelated-content preservation. CLI coverage for `instructions
  status|sync|remove --file` must prove path-free JSON, dry-run zero writes,
  non-dry-run `--yes`, sanitized refusal/error codes, reversible removal, no
  `.repogrammar/` creation, and no implicit AGENTS/CLAUDE mirroring. Atomic-write
  coverage must preoccupy sibling candidates with both regular files and
  symlinks, prove neither is followed or removed, prove operation-owned cleanup
  after failure while the creating handle remains open, preserve a foreign
  pathname replacement even when size and timestamps offer no distinction,
  prove unlinking changes the creating handle's link-count identity, and
  preserve an existing Unix mode such as `0600`, owner uid, and group gid
  through sync and remove. The same-directory hostile concurrent
  pathname-replacement boundary remains an explicit unsupported limitation,
  not a claimed compare-and-swap guarantee.
  Legacy classification inputs must come from independent fixed fixtures under
  `src/fixtures/instructions/`, not from the classifier's own body constructors.
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
  installer-script checksum contract checks, Cargo/npm/lockfile and exact
  tag-at-current-`origin/main` publication authority, explicit build-only
  rehearsal dispatch, tag-only candidate creation, the exact runner-compatible
  paginated collision query, rejection of `--slurp` combined with `--jq`,
  refusal to replace an existing draft, stage-only OIDC publication, the exact
  11-asset draft
  inventory, and the exact packaged-artifact lifecycle gate. That gate
  must unpack the candidate binary with its worker, use the committed Pydantic
  release fixture in an isolated HOME, and cover exact `version`, setup
  dry-run/live product-MCP self-test, explicit `resync`, unchanged incremental
  copy-forward, `find`/advisory-`check`, autosync readiness across at least three
  poll intervals, changed-file generation activation, stop, and daemon-lock
  removal on native Linux and macOS. Lifecycle failures must report only
  low-cardinality readiness fields and must not claim that a process exited
  when liveness was merely unverifiable. Windows remains a source-only boundary,
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
  cover the full supported stable/preview platform and artifact matrix,
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
  without paired measurements, all-scope savings accounting (see below),
  redacted research export,
  redacted experiment export without raw names/session ids/token counts, paired
  baseline/treatment token experiment recording, default-no experiment prompts,
  record-existing prompt no-extra-session wording, controlled-pair
  token/time/provider-cost prompt warnings,
  missing pairs yielding `token_savings: null`, comparable pairs computing
  token savings and ratio, required measurement source, and `stats --json`
  reporting measured savings only when a valid paired measurement exists.
  Anonymous telemetry schema tests must cover bucketed experiment aggregate
  fields without raw token counts or user-provided experiment names.
- All-scope estimated-potential-token-savings tests must cover the single
  estimator authority per context-delivering outcome shape: found families
  (unchanged), PARTIAL_CONTEXT read plans (whole-file baseline from the stored
  inventory size, no event when the size is unavailable), committed/partial
  alignment certificates (target file plus family evidence baseline, no event
  when abstaining), and the `max(0, baseline - returned)` floor. They must cover
  the additive local rollup: `by_outcome_shape` and `by_language` breakdown
  accumulation, tolerated-when-absent parsing of a legacy rollup file written
  before the breakdowns existed (keeping the `estimated-potential-token-savings.v1`
  schema token), and an out-of-vocabulary language coercing to `unknown` rather
  than dropping the event or writing an out-of-vocabulary key. A same-source
  vocabulary test must pin the query-layer language producers
  (`inventory_language_scope` plus the found-family `mixed` marker) to the single
  authoritative `SAVINGS_LANGUAGE_KEYS` allowlist so the two cannot diverge.
  Surface tests must assert the `estimated_potential_token_savings` block (with
  `outcome_shape`, `language`, `ESTIMATED` kind, and the not-measured caveat) is
  present on PARTIAL_CONTEXT and alignment responses on both CLI and MCP, and
  that `stats --json` reports the additive `all_scope_token_savings` block
  (`savings_events`/`total_queries` denominator plus `by_outcome_shape` and
  `by_language`) while the concise human `stats` leads with the summary and moves
  full detail behind `--json`. An end-to-end binary test must prove a
  TypeScript-only repository records nonzero PARTIAL_CONTEXT savings where the
  Python-scoped panel previously reported zero. Run these through
  `cargo test --workspace --all-features` (unit estimator and telemetry cases,
  `interfaces::cli::tests`, `interfaces::mcp::tests`, and the
  `product_runtime_partial_context_records_nonzero_all_scope_token_savings`
  binary test).
- Found-family payload member lists must be bounded outside `--mode deep`: tests
  must assert the inline `members` array is capped at `MAX_RENDERED_FAMILY_MEMBERS`
  in unchanged deterministic order while `member_count` reports the true total and
  `members_truncated` flags the cut, on both CLI JSON and MCP, with `--mode deep`
  restoring the full list.
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
  through `python3`, validate private parse-document JSON output with the exact
  request/response tuple `protocol_version=1, contract_revision=1`, and prove
  that a missing or different revision returns only the low-cardinality
  `PYTHON_FRONTEND_CONTRACT_MISMATCH` envelope without paths, source, or raw
  payload. They must also cover syntax-error
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
  oversized request, a 40,000-import module completing within an isolated
  subprocess bound without quadratic binding-map snapshots, a sleeping fake
  frontend producing a typed timeout within a deterministic upper bound while
  stdout is concurrently drained,
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
  Python frontend size/error boundaries. The Rust adapter must serialize the
  exact private parse-document contract tuple, reject missing or different
  response revisions as typed `PythonFrontendContractMismatch`, and map an old
  worker's bounded rejection of the revision-bearing request to the same typed,
  path-free result while retaining ordinary missing-worker classification.
  Application coverage must assert the sanitized rebuild/reinstall recovery
  and prove no candidate generation is activated after a mismatch. The exact
  committed `src/fixtures/python/release/v0_1/pydantic-basic/schemas.py` must
  run through direct-worker and Rust-adapter tests plus full indexing and an
  unchanged incremental copy-forward; its `pydantic.field_validator` structural
  fact and non-blocking `pydantic_validator_side_effects` UNKNOWN must remain
  fresh and source-free in both active generations. Tests must also assert
  `setup.cfg` provenance uses `configparser`, not `tomllib`.
  Semantic-worker-compatible project-mode repo-local import resolution for
  unique module-level matches, ambiguous or missing
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

## Product evaluation harness

`repo-guard product-eval` is the committed, deterministic product-core
evaluation harness. It measures what the product runtime actually returns for
the pattern-family query surface (`find`, `family`, `member`, `explain`,
`check`) against a fixed committed corpus. It is report-only measurement
infrastructure and changes no production behavior.

```text
cargo run --quiet --bin repo-guard -- product-eval \
  --corpus src/fixtures/evaluation/query-corpus-v1.json \
  --out <output-dir> [--repetitions <n>] [--bin <path-to-repogrammar>] \
  [--condition <token>] [--baseline token-overlap]
```

For each corpus fixture the harness copies the committed fixture root into an
isolated temporary workspace with an isolated `HOME`/XDG/`CODEX_HOME` and a
tool-only `PATH`, runs `init` then `resync`, applies any per-query source
mutation to that copy, and drives the product binary through the query. It
never modifies the real repository and never enables auto-sync. Workspaces are
removed on success and retained (path printed to stderr) on a harness error.
When `--bin` is omitted the harness resolves the sibling `repogrammar` binary
next to `repo-guard`; both build into the same target directory.

The run writes `<output-dir>/product-eval-results.json`
(`schema_version: product-eval-results.v2`) with top-level `condition` and
`baseline` provenance tags
(see [Run conditions and the token-overlap baseline](#run-conditions-and-the-token-overlap-baseline)),
per-fixture `resync` latency
and discovered/stored counts, per-query expected/actual/`match`/mismatch-field
detail with all repetition latencies, and a summary of matches, per-kind and
per-intent counts, p50/p95 latency, the `false_family_selections` and
`selected_on_abstention_gold` safety counters, and a `metrics` object. Each result also carries `intent`, `reciprocal_rank` (retrieval queries
only), and the actual's null-tolerant `hydrated_family_count` and
`retrieval_stage_count` placeholders. Mismatches are baseline data, so the
command exits `0` when the run completes; it exits nonzero only on a genuine
harness error (missing binary, unparseable corpus, subprocess failure, or
non-JSON query output). Corpus gold expectations encode product intent, not
current output, so retrieval-intent natural-language and synonym questions over
families that exist are recorded as mismatches rather than softened. Latency
figures are machine-dependent; verdicts, per-kind/per-intent counts,
`false_family_selections`, and the integer metric numerators/denominators are
stable for a pinned corpus and product commit. The current baseline reading is
recorded in [`../experiments/product-core-baseline.md`](../experiments/product-core-baseline.md).
Harness parsing, matching, hashing, metric math, and result-serialization logic
are covered by unit tests in `src/rust/bin/repo_guard.rs` that do not depend on
the product binary or the network.

### Query intent taxonomy

Every corpus query declares a measurement `intent` (a new optional field, so the
corpus schema stays backward-compatible `product-eval-corpus.v1`):

- `retrieval` — a specific family should be resolved. Gold carries an `ok`
  outcome and a `family`/`family_prefix`/`family_any_of` target. Exact ids,
  members, paths, roles, and `path:line`/`path:start-end` locators over a
  single-family path resolve today; bare framework-name-as-concept, synonyms, and
  natural-language questions abstain and are recorded as the measured retrieval
  gap, not softened.
- `abstention` — the correct behavior is a typed `UNKNOWN`. Covers ambiguous
  targets, unsupported-language questions, unsafe typo inputs, bare framework
  tokens (the deterministic resolver must not guess a family from a short
  substring), stale evidence, and byte-range locators spanning multiple families.
- `context` — metadata-only local context (`PARTIAL_CONTEXT`) or a zero-family
  repository, where no family claim is safe but a read plan is.

An optional `expected.candidates_include` lists family-id prefixes that should
appear in the actual candidate set; it is the Recall@K/MRR gold and is
independent of which single family (if any) is selected.

### Retrieval metrics

The `summary.metrics` object reports, each as a rate plus its integer
numerator/denominator (a rate over an empty denominator serializes as `null`):

- `hit_at_1` — over retrieval-intent queries, the fraction whose selected family
  satisfies the family gold.
- `candidate_recall` — over queries with `candidates_include`, the fraction where
  every listed prefix is matched by some candidate family within the first
  `K = 5` candidates. `candidate_recall` measures list construction and is scored
  whether or not the run commits a single family.
- `mrr` — over retrieval-intent queries, mean reciprocal rank of the *committed*
  answer. Only a run that commits (an `ok`/`partial_context` outcome) scores: its
  selected family, when it satisfies gold, is rank 1, otherwise the first
  gold-satisfying id within the first `K = 5` candidates contributes `1/rank`. A
  run that abstains (`unknown`/`fallback`) scores `0` regardless of what its
  diagnostic candidate list held — MRR is the committed-answer metric, distinct
  from `candidate_recall`. The candidate depth `K = 5` is applied identically for
  every condition (the product's list is truncated to five; the baseline already
  reports at most five).
- `correct_abstention_rate` — over abstention-intent queries, the fraction whose
  actual outcome is `unknown`.
- `false_family_rate` — `false_family_selections` divided by the number of
  queries that declare a family constraint; the absolute count is kept. A query
  whose gold is an abstention carries no family constraint, so a confident wrong
  selection there is invisible to this metric (see `selected_on_abstention_gold`).
- `selected_on_abstention_gold` — a safety counter, reported both in `metrics` and
  at `summary` top level: the number of queries whose gold outcome is `unknown`
  (no family should be committed) where the run nonetheless selected a family. It
  is the abstention-side complement of `false_family_selections`; together they
  cover confident wrong selection on both retrieval and abstention gold. It is not
  a rate.
- `unsupported_rejection_rate` — over `unsupported_concept` queries, the fraction
  that abstain.
- `ambiguity_precision` — over abstention-intent `ambiguous`/`nl_pattern_question`
  queries, the fraction that abstain.

`summary.by_intent` reports per-intent `{total, matches}` totals alongside the
existing `summary.by_kind`. `summary.false_family_selections` and
`summary.selected_on_abstention_gold` are surfaced at the summary top level as
the two confident-wrong-selection safety counters.

### Run conditions and the token-overlap baseline

Every results document carries two top-level provenance fields — a `condition`
string that names what was measured, and a `baseline` field (`"token-overlap"` or
`null`) that names the control independently of the condition label — so product,
ablation, and baseline runs over the same corpus are stored distinctly under one
schema:

- The default condition is `product` (the product runtime drives every query),
  with `baseline: null`.
- `--condition <token>` records an explicit condition verbatim. The token is
  low-cardinality and validated as `[a-z0-9_-]+` up to 40 characters, and must not
  start with `-` (so a forgotten flag value such as `--condition --baseline` is a
  hard error, not a silently accepted token). Use it to tag an ablation run (the
  product built with ablation env/flags); the harness records the tag but does not
  itself change product behavior.
- `--baseline token-overlap` runs the naive control described below, sets
  `baseline: "token-overlap"`, and defaults the condition to
  `baseline_token_overlap`. An explicit `--condition` still wins, so a labeled
  baseline ablation is possible — but `--condition product` with a baseline is
  rejected with a typed error, because a baseline is not the product.

The token-overlap baseline is an honest naive lower bound evaluated on the same
corpus gold and emitted in the same `product-eval-results.v2` schema. It indexes
each fixture through the same isolated `init`+`resync` flow, then fetches the
product's `families --json` listing once per fixture. For each query it does not
drive the product; instead it:

1. lowercases the query target, splits it on non-ASCII-alphanumeric characters,
   drops tokens shorter than three characters, and deduplicates them;
2. scores each family by the count of distinct query tokens that are substrings of
   its `family_id` (the id embeds language/kind/role tokens);
3. selects the unique argmax when its score is at least two, abstaining on a strict
   tie at the maximum or a sub-threshold maximum; and
4. reports its own candidate ranking (families with a positive score, ordered by
   score then id, capped at the shared `K = 5`) so the same `hit@1`, `mrr`,
   `candidate_recall`, abstention, `false_family`, and `selected_on_abstention_gold`
   metrics are computed against the shared gold.

The baseline has no aliases, concepts, margin calibration, route, or typed unknown
reason. It also never receives the per-query source mutations: a stale-evidence
query is graded against gold the baseline cannot observe, which penalizes the
baseline only — a recorded asymmetry, not a defect. It exists only to contrast the
product against a deterministic lower bound and must never be tuned to flatter or
diminish either side; its metric line is recorded exactly as produced.

Tie-abstention does **not** make the baseline safe from confident wrong selection:
a query whose distinct tokens uniquely clear the threshold is selected even when the
gold is an abstention — for example the unsafe-typo target `fastapi_rout` scores two
(`fastapi`, `rout`) against the FastAPI family alone and is selected, which the
product correctly abstains on. The baseline's weakness therefore surfaces as lower
`hit_at_1`, `candidate_recall`, and context coverage, as a lower
`correct_abstention_rate`, and as a nonzero `selected_on_abstention_gold`. A
`false_family_selections` of `0` is corpus-contingent — the abstention-intent
queries carry no family constraint, so wrong selections on them land in
`selected_on_abstention_gold`, not `false_family_selections` — and is not a design
guarantee of the baseline.

The `matches`/`by_kind`/`by_intent` verdict counts and the latency figures are
**not** comparable across conditions: verdict counts include route and
unknown-reason fields the baseline never produces (so its `matches` is
mechanically lower), and the baseline's per-query latency measures in-process
scoring rather than a product subprocess (near `0 ms`). Compare conditions on the
retrieval metrics and the two safety counters, not on `matches` or latency.

## Sync-equivalence oracle

`repo-guard sync-equivalence` is the committed incremental/full-build
equivalence oracle. It is the mandatory guard for every incremental-`sync`
project-context gate rule: an incremental sync must produce a semantically
identical active generation to a clean full rebuild over the same worktree, or
explicitly fall back to a full rebuild. It changes no production behavior.

```text
cargo run --quiet --bin repo-guard -- sync-equivalence \
  --fixture src/fixtures/incremental_equivalence/v1 \
  [--scenario <id> | --all] [--bin <path-to-repogrammar>] \
  --out <output-dir>
```

For each scenario the harness copies the committed fixture root into an
isolated temporary workspace (isolated `HOME`/XDG/`CODEX_HOME`, tool-only
`PATH`, telemetry disabled — identical to the product-eval harness). It builds
state A with `init` then `resync`, applies the scenario's scripted patch, and
runs `sync` to produce state B (incremental). It then builds a separate clean
workspace C by applying the same patch first and running `init`+`resync`
(clean full build). It compares canonical dumps of B and C across the product's
own read surfaces — `files`, `units`, `families`, `family <id> --mode deep`
(deep mode is required; compact mode returns an empty selected-evidence array so
the family-evidence ledger would never be compared), `unknowns` — plus the
store-port ledgers not exposed by any CLI surface: the semantic-fact multiset,
the IR graph (nodes and edges, which have bespoke incremental copy-forward
logic), and the repo-shape stats. When `--bin` is omitted the harness resolves
the sibling `repogrammar` binary next to `repo-guard`. Workspaces are removed
after each scenario. (The claim-input snapshot is intentionally not dumped
separately: it is the union of the already-compared indexed-files, code-units,
IR-graph, and semantic-fact surfaces.)

Canonicalization strips only the sanctioned non-semantic fields: generation
ids/timestamps (never surfaced into the compared dumps — the top-level
`active_generation` of every response and the `unknown_inventory`'s
`active_generation` are dropped) and the order/history-assigned
`fact_id`/`evidence_id` sequence numbers (excluded from the fact and family-
evidence tuples; the family-evidence `estimated_tokens` presentation field is
also dropped). The fact tuple deliberately keeps `content_hash` (it is
provenance-bearing — the field that distinguishes a stale retained fact from a
correctly re-parsed one), encodes `target` Option-ness explicitly, and joins
assumptions with the unit separator in emitted order. The single sanctioned
semantic divergence — external TypeScript worker facts (`typescript`) retained
by a worker-less incremental sync for unchanged files — is checked against a
two-sided retention rule (retained facts' path unchanged and content hash
matching the current indexed file; and no clean-only provider fact left
unmatched) rather than by equality with the clean rebuild. `cargo_metadata`
facts are compared by equality, since the in-binary Rust provider is
reproducible in the clean rebuild. Every v1 scenario is worker-less, so that
provider bucket is empty by construction; the rule is applied rather than
blanket-ignored.

Each scenario declares an `expected_outcome` (`EQUAL` or `FELL_BACK`), for a
fallback an `expected_fallback_reason`, and optionally an
`expected_reparsed_files` count; a scenario `pass` requires the observed outcome,
reason, and (when declared) reparsed count to match. This makes the exit-0 gate
non-trivial: an unexpected `EQUAL` (a gate that silently regressed to the
incremental path), an unexpected fallback (a misfiring preflight), a wrong
fallback reason, a file-local path that reparsed more files than the single
edited one, or any `INEQUAL` all fail. The run writes
`<output-dir>/sync-equivalence.json` (`schema: sync-equivalence.v1`): per
scenario the observed `sync_mode`, `fallback_reason`, `reparsed_files`,
`expected_reparsed_files`, `equal`, `outcome`, `expected_outcome`,
`expected_fallback_reason`, `pass`, and per-surface bounded diff samples. The
exit status is `0` only when every requested scenario passes.

The committed v1 scenarios are `java_edit`, `csharp_edit`, `docs_noop`,
`java_add`, `java_delete`, `rs_content_edit`, `tsjs_content_edit`, and
`python_body_edit` (incremental paths, expected `EQUAL`); `tsjs_add`, `rs_add`,
`mocharc_remove`, and `python_conftest_edit` (expected `FELL_BACK` via
`project_context_changed`); and `python_interface_edit` (expected `FELL_BACK` via
`python_interface_changed`). The `rs_content_edit`, `tsjs_content_edit`, and
`python_body_edit` scenarios are the end-to-end proof of the content-only
file-local fast paths: each edits one function body (a Rust test fn under
`service/rust/`, a TS ambient test under `web/`, a Python function under
`analytics/`), and the incremental generation must be canonically equal to a
clean rebuild while reparsing exactly one file (`expected_reparsed_files: 1`).
`python_body_edit` is specifically the proof of the Python interface-hash gate:
the body edit leaves `analytics/app.py`'s interface projection unchanged, so only
that module reparses while its sibling `analytics/conftest.py` copies forward.
`python_interface_edit` adds a top-level function to the same module, changing its
interface hash, and must fall back with `python_interface_changed`;
`python_conftest_edit` edits `analytics/conftest.py`'s body and must fall back
with `project_context_changed` regardless of interface hash, proving the conftest
carve-out. The interface-hash gate's third condition — the Python context-payload
regime must stay safely under the worker's ~1 MiB per-request cap on both
manifests, else it falls back with `python_context_budget` — is covered by
`application::indexing` unit tests with an injected small cap rather than an
oracle scenario, since a near-cap committed fixture would need ~1 MiB of Python
source. The `tsjs_add`/`rs_add` counterparts confirm the gate still falls back
when a source file is *added* (the path set grows, which can change how other
files resolve). The fixture carries ambient TS tests under `web/` that form runner
families only while the root `.mocharc.json` is present, so the `mocharc_remove`
scenario is the end-to-end regression for the Mocha-runner-config gate fix: if
that gate regressed, the removal would run incrementally, copy forward the stale
flag-on TS families, and diverge from the clean rebuild — a real inequality on top
of the expected-outcome check.

## Response payload byte measurement (payload-measure)

`repo-guard payload-measure` is the deterministic byte-measurement instrument for
the response-precision policy (S10). It indexes the committed fixture
`src/fixtures/evaluation/payload-measure` in an isolated temporary workspace and
serializes a fixed query corpus, recording the exact response byte count and
top-level field-group attribution per operation x category x tier (mode x
verbosity). It writes `payload-bytes.summary.json` (stable, sorted,
timestamp-free) and `payload-bytes.md` under `--out`. The subcommand reference is
in `docs/development/repository-guard.md`.

The fixture is a small deterministic Python/TypeScript repository: `api/routes.py`
plus `lonely.py` form one FastAPI route family of 31 members (rendered as 20 under
the member cap, with `member_count` reporting the true 31), alongside small
SQLAlchemy, Pydantic, pytest, and Express families and a below-support Flask file
that drives the `PARTIAL_CONTEXT` shape. The corpus covers Found
(big/small/NL/TypeScript), abstention `UNKNOWN`, `PARTIAL_CONTEXT`, exact family
hydration, and static-alignment conformance, plus one MCP `inspect_readiness` row.

The big Found family and conformance are additionally measured at `--mode deep
--include-source-spans` (one extra row per verbosity, tagged `source_spans: on`),
so the `read_plan` <-> `source_spans` overlap — the S6 dedup target and the plan's
largest single per-response item — is measurable; it is invisible unless source
spans are explicitly requested. Every row carries `source_spans: on|off`; the
summary also records a `fixture_shape` block (big-family `member_count`,
`members_rendered`, `members_truncated`) so fixture drift is detectable from the
artifact alone.

### Before/after protocol

Byte savings are declarable only from a before/after comparison, never from a
single run:

1. Run `payload-measure --out <before>` at the baseline commit (before a
   precision slice lands).
2. Run `payload-measure --out <after>` after the slice lands, over the same
   fixture.
3. Diff `<before>/payload-bytes.summary.json` against
   `<after>/payload-bytes.summary.json`. The per-row `total_bytes` and
   `field_bytes`, and the aggregate `field_group_totals`, are the byte table a
   savings claim must cite.

The member-cap lane's byte reduction is credited to that lane, not to a precision
slice: regenerate the baseline after the cap lands so precision-slice deltas are
measured on post-cap payloads. `verbosity=full` is expected to reproduce the
pre-change bytes exactly (v1 additivity), while `verbosity=minimal` carries any
opt-in lean shape; a before/after diff reads both tiers.

### Determinism guarantee

`payload-bytes.summary.json` is a pure function of the fixture content and the
product binary — no timestamps, latencies, or workspace paths enter it, and every
map and row list is sorted. Two runs against the same fixture and binary therefore
produce byte-identical summaries. The end-to-end smoke test
(`payload_measure_is_deterministic_and_schema_stable_end_to_end` in
`src/rust/bin/repo_guard.rs`) runs the harness twice against the committed fixture,
asserts the two summaries are byte-identical, and asserts the schema, row count
(including the source-spans variants), required report-variant coverage, the
`fixture_shape` (`member_count == 31`, `members_rendered == 20`), and that
readiness is measured on the MCP surface. It locates the product `repogrammar`
binary that `cargo test --workspace` builds alongside the test harness; if that
binary is absent the test fails loudly (it does not silently no-op, so a green CI
never hides an unmeasured harness). The pure attribution and row-construction logic
is covered by separate unit tests that need no live index.

### Scope

The harness uses a purpose-built corpus rather than the product-eval corpus
`src/fixtures/evaluation/query-corpus-v1.json`. That corpus is a retrieval-accuracy
corpus: it has no family with >= 25 members (so it cannot exercise the cap or the
`members[]`-dominance finding) and no byte-tuned abstention/`PARTIAL_CONTEXT`
targets, so it cannot attribute field-group bytes per report variant. The dedicated
corpus is the correct instrument for that measurement.

The measured surface is the shared query serializers (`find`/`family`/`check`) —
the exact functions the Wave-1 precision slices edit. The CLI `--json` output and
the MCP `repogrammar_context` result are serialized through the same query path, so
measuring the CLI surface covers the MCP query payloads too. The one exception is
readiness, which has no query-path serializer: it is measured directly through the
MCP `inspect_readiness` surface (`serve` stdio), the bounded, source-free readiness
report.

### Uncovered shapes

A genuine `CompetingFamilies` above-floor margin tie is not reachable on this
fixture (the family names are too separable, so ambiguous natural-language queries
abstain via `below_min_score` into `UNKNOWN`), matching the audit's uncovered-shape
note. Two CLI-only lifecycle/stats slices are out of scope for this harness and
are owned by later lanes, not excluded on principle: S12 is the CLI `stats`
`by_language` payload (empty-language-row suppression), and S13 is the CLI
`status`/`doctor` lifecycle dual-readiness and DB-internals cleanup. The MCP query
payloads and readiness are fully covered above.

## Agent-study pilot harness (RQ5)

The Phase 7 RQ5 agent-impact study has a standalone pilot harness under
`src/experiments/agent_study/` (Python 3 stdlib only, no new dependencies). It
is automation tooling, not product code, and is exercised independently of the
Rust gate. See `docs/experiments/agent-study-pilot.md` for the protocol,
pilot results, and honest caveats (N=2 proves mechanics only — no effect
claims).

- Unit tests (tree-hash equivalence to the Rust `fixture_version` hash,
  transcript parsers, safety detectors, mechanical grader, record schema +
  privacy guard):
  `python3 src/experiments/agent_study/selftest.py`
  The seeded fixture transcripts are gitignored (the `transcript*.jsonl`
  privacy backstop covers them too); on a fresh checkout the selftest
  regenerates them deterministically from the committed
  `fixtures/build_fixtures.py` before running.
- Zero-spend end-to-end pipeline check (parse → detect → grade → record → cost
  accounting over scripted fixture transcripts, including the four seeded
  detector runs; launches no agent and needs no network):
  `python3 src/experiments/agent_study/driver.py --dry-run`

Committed records (`agent-study-run.v1` JSONL) hold only hashes, counts, and
repo-relative paths; raw transcripts and patches stay in a local untracked work
base outside the repo tree (the driver refuses a `--work-base` inside the repo).
`regrade.py` re-derives verdicts/metrics from saved transcripts with zero spend
and writes the committed `docs/experiments/data/agent-study-regrade.v1.json`.

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

Before cutting a stable or preview tag or opening release-readiness changes,
contributors should run the normal local gate plus a source-checkout smoke
matrix that exercises installation boundaries without live machine writes.

Release-policy tests must cover both npm channels. Preview requires the exact
manifest prerelease under `preview`; before any stable exists, npm's required
`latest` may point to that same exact prerelease as a bounded preview-only
state. Stable requires exact `latest=0.3.2`, exact
`preview=0.2.0-preview.0`, both immutable versions in the registry inventory,
the explicit absence of the failed `0.2.0` and `0.2.1` candidates, and a
retained-candidate SRI match. Any other prerelease under `preview`, any
prerelease-valued `latest` after stable, either failed candidate appearing as
published, malformed inventories, or unpublished tag targets fail closed. The
workflow contract tests require the exact `./npm-candidate/...` local package
spec and reject the bare relative form that npm can interpret as GitHub
shorthand. Manual reconciliation remains read-only. Stable
completion additionally requires immutable GitHub release verification, the
exact 11-asset inventory including the public npm candidate manifest, every
asset attestation and checksum, npm signature/provenance verification, and the
exact tag-run id and successful run attempt. The finalizer must verify public
npm pack metadata against the public manifest before executing a package or
launcher, then run the downloaded public Linux archive, downloaded public shell
installer, pinned live repository-only setup, unversioned live
repository-only setup, and preview version smoke. Version/tag checks or a
setup dry run alone are not a finalizer. Native-agent integration and fresh
coding-agent instruction behavior remain separate isolated pre-release/manual
evidence rather than automatic finalizer evidence.

Public npm launcher smoke must create separate external `pinned`, `latest`, and
`preview` work directories and enter the selected lane's work directory inside
a child shell before invoking `npx`. Separate HOME/cache/PATH values alone are
insufficient if the command still runs from the checked-out RepoGrammar root:
the same-name root `package.json` can cause npm to skip injecting the requested
public package's bin. The installer shell contract test locks the three work
directories, the dynamic lane `cd`, and the child-shell boundary.
It also locks the `${RUNNER_TEMP}` root and the workflow's visible rejection of
verifier definitions dispatched from a ref other than `main`. The contract
requires `git` in the tool-only PATH so the setup smoke can initialize its
isolated fixture repository.

The first post-public finalizer run, `29587973589`, was bound to candidate run
`29586694524`, attempt 1. It passed immutable GitHub release, public npm
metadata/provenance, packaged-native, and public-installer checks, then failed
the launcher smoke with the root-working-directory behavior above and did not
emit `STABLE_RELEASE_READY`. A corrected finalizer is dispatched from `main`
while checking out immutable `v0.2.2`; this changes verifier orchestration only,
not the release source, artifacts, or candidate-run identity.
Follow-up run `29589865164` again failed in the public launcher step. Because
the workflow redirected command output and uploaded no failure evidence, that
run alone does not identify the exact invocation. An exact local reproduction
from the same external-work-directory and tool-only-PATH shape showed that the
version command passes, setup returns typed `repository_initialization_failed`,
and adding only `git` makes the same public package complete setup. This is a
verifier-environment correction, not a product or publication change.

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
