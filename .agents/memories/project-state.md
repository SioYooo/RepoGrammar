# Project State

- Status: Bootstrap plus conservative v0.2 TS/JS exact-anchor family substrate
  for Express, Jest/Vitest (with Mocha/`node:test` `runner_kind` aliasing), Zod,
  NestJS, Hono, Next.js, Fastify, Prisma, and Drizzle, internal v0.2
  Rust structural self-dogfood indexing for RepoGrammar-owned implementation
  families, Python `.py` discovery, CPython AST structural
  indexing slice, persisted internal Python
  structural anchors, path-derived module-name anchors, CPython `symtable`
  structural scope anchors, FastAPI dependency/error/request-shape anchors,
  pytest parametrize argument anchors, Pydantic field/config/member anchors, typed
  dynamic Pydantic model factory, dynamic import, `sys.path` mutation, dynamic
  call, dynamic/unresolved decorator, monkey-patch, and unresolved import
  `UNKNOWN` facts, private
  bounded private project-config summaries, semantic-worker-compatible project-
  mode module-level repo-local import resolution, default parser-mode repo-local
  import context from discovered `.py` inventory and sanitized root
  root Python project-config source roots from `pyproject.toml`, `setup.cfg`,
  and static-literal `setup.py`, default-indexed `python-config` structural
  project-config records, structural IR storage,
  opt-in syntax-origin framework-role fact storage, semantic fact ingestion,
  bounded exact-anchor Python `DATAFLOW_DERIVED` support derivation,
  internal active claim-input snapshot reads, semantic-fact
  freshness/readiness gating, FamilyStore-backed query reads, read-only MCP
  serving, and global Codex/Claude Code installer writes with an interactive
  multi-select wizard and all-or-rollback `--target all` transaction. ADR-0011 makes
  Python-first analysis the official v0.1 implementation target, and ADR-0012
  defines the claim-driven selective Python analysis cascade. The current
  Python slice persists parser-origin Python facts and structural project-config
  records but blocks them from direct family construction and claim-input
  readiness. A separate
  `repogrammar-python-derived` step can synthesize support from exact canonical
  anchors for units with one framework role, so narrow Python family rows can be
  produced without claiming provider-backed semantics. Product smoke now covers
  those exact-anchor families across CLI `member`/`find`/`explain`/advisory
  `check`, token-budget auto evidence, explicit compact/evidence/deep metadata
  modes, supported MCP operations, and stale source mutation/deletion returning
  blocking `StaleEvidence` `UNKNOWN`. Family detail output now supports
  compact/evidence/deep modes shared by CLI and MCP; compact is the default,
  evidence/deep use greedy metadata coverage selection, all matched modes
  include read plans with hash-checked line ranges when source hashes are
  fresh, and source snippets remain disabled unless callers explicitly request
  bounded source spans. Supported Python members can
  preserve non-blocking subclaim
  `UNKNOWN`s, such as unresolved FastAPI dependency targets, as metadata-only
  family detail entries keyed by the concrete family id and subclaim without
  turning those subclaims into route-membership support. Read plans use
  repo-relative paths, strict content hashes, byte ranges, line ranges when
  fresh, purpose labels, and estimated token costs. CLI
  `--include-source-spans` and MCP
  `include_source_spans: true` render only selected hash-checked spans with
  line numbers and omit stale or unsupported spans with Read/Grep fallback
  guidance. Target source remains required before edits outside rendered
  ranges. `families --json` now uses source-free family summaries, and
  `repogrammar stats --json` without `--unknowns` uses the read-model aggregate
  path without hydrating family evidence, semantic facts, IR graphs, full
  claim snapshots, or family detail. MCP exact family/member lookups hydrate
  only bounded candidates, and fuzzy lookup abstains with typed `UNKNOWN` when
  candidate discovery is truncated. `repogrammar stats --json` now reports
  repo-shape diagnostics for local pattern density, family support coverage,
  abstention rate, external dependency signal, and thin-wrapper/token-saving
  risk, readiness/blocking reasons, and estimated potential read displacement,
  and reports measured token savings only when local paired baseline/treatment
  experiment records are comparable. Stats also reports
  `official_family_scope: python_v0_1`, source-free indexed file/code-unit/
  semantic-fact counts, by-language indexed inventory, and explicit TS/JS
  unsupported-scope guidance when indexed context exists without supported
  families. React/RN remains unsupported; stats should route exact TSX/RN-like
  paths to `find`/`check` for `PARTIAL_CONTEXT` instead of implying family or
  conformance support. `index`, `sync`, and `resync` emit
  typed stage progress to stderr while running, with exact counts when known
  and NDJSON progress available through `--json --progress always`.
  Rust self-dogfood uses Tree-sitter Rust only for structural `.rs` unit
  extraction and typed UNKNOWNs; it does not run Cargo, rustc, build scripts,
  procedural macros, or general Rust semantic analysis. The v0.2 Rust fixture
  suite now includes family gates, parser adapters, installer actions, product
  tests, low-support abstention, macro/cfg blockers, trait-dispatch blockers,
  Cargo build-script non-execution/blocking, and bounded module-resolution/Cargo
  target-dependency inventory. Root `Cargo.toml` build-variant UNKNOWN can block
  repository-wide Rust self-dogfood family emission, but nested fixture/package
  manifests must not globally block unrelated root Rust family support.
  Source-level Rust cfg UNKNOWNs can carry nearest Cargo feature context,
  including simple feature predicates and declared/undeclared feature state,
  without evaluating cfgs or changing family support eligibility.
  A conservative v0.2 Java/Spring preview slice now discovers `.java` files,
  parses Java with Tree-sitter Java, recognizes exact imported/FQN Spring MVC,
  stereotype, Spring Boot, and Spring Data anchors, derives
  `repogrammar-java-derived` support only under exact target and safe-origin
  gates, and keeps simple lookalikes, route constants, DI/proxy/component-scan,
  Maven/Gradle/javac/annotation-processor, classpath, and generated repository
  behavior as typed `UNKNOWN` or non-supporting context.
  Java framework deepening (Wave J1, 2026-07-11) extends that slice, under the
  same exact-import/FQN gate, with JUnit 5/4 and TestNG test methods, JPA/Jakarta
  Persistence entities under dual `jakarta.persistence`/`javax.persistence` roots
  (jakarta and javax entities never cluster together via a `jpa_namespace_root`
  assumption), JAX-RS/Jakarta REST resource classes/methods under dual
  `jakarta.ws.rs`/`javax.ws.rs` roots (verb-outside-`@Path` is blocking), Mockito
  test-context metadata, Lombok-as-`MacroOrPreprocessor`-`UNKNOWN`, and Spring
  Data derived-query metadata (non-support). The parser now lives in a
  `parsing/java/` module (`mod.rs` core plus `spring.rs`/`junit.rs`/`jpa.rs`/
  `jaxrs.rs`/`test_data.rs`), and the blocking-claim and copied-assumption policy
  tables are hoisted into one authoritative registry in
  `adapters/frameworks/java.rs`. New
  mechanisms: `java_test_annotation_model`, `jpa_entity_model`,
  `jaxrs_resource_model`, `java_annotation_processor_boundary`,
  `java_mockito_runtime_mock_model`. It never runs Maven/Gradle/javac, annotation
  processors, or Lombok, never generates Mockito mocks, and never parses
  `testng.xml`/`orm.xml`.
  The Java test-data linker now builds one registry per class-like body and
  replaces only complete direct-repeatable JUnit `@MethodSource` scalar/array
  literal sets (blank/omitted same-name convention, unique static targets) and
  TestNG literal `dataProvider` / exact `@DataProvider` links with structural
  facts. Exact link identity requires an FQN or one unambiguous explicit import;
  wildcard/colliding imports, local type shadows, malformed imports, nested
  annotations, and parse-open inventories abstain. External/signature/provider-
  class, type-level, inherited, explicit-container/meta, `PER_CLASS` non-static,
  overloaded/duplicate, dynamic, unknown-identity, partial-positive, invalid
  non-parameterized, mixed-framework, missing-target, and nested-boundary links
  remain typed
  `UNKNOWN` or conflict. The linker executes no compiler, build tool, test
  engine, annotation processor, or repository code and does not claim Java C0
  completion or add provider-link family support.
  A conservative v0.2 C# preview slice (Wave CS1) now discovers `.cs` files
  (skipping the MSBuild `obj/` directory), parses C# with Tree-sitter C#,
  recognizes exact lexical-scope using/FQN-gated ASP.NET Core controller/action, minimal-API
  route (in-file `WebApplication` builder receiver), EF Core `DbContext`/`DbSet`,
  and xUnit/NUnit/MSTest anchors, derives `repogrammar-csharp-derived` /
  `bounded_tree_sitter_csharp_anchor_v1` support only under exact-target and
  safe-origin gates with support>=3 and HTTP-method clustering, and keeps
  lookalike attributes, actions outside controllers, unresolvable minimal-API
  receivers, MSTest methods without `[TestClass]`, `#if` build variants, and
  DI/filter-pipeline/convention-routing/source-generator/dynamic behavior as
  typed `UNKNOWN`. A 2026-07-15 CS1 follow-up resolves only direct-literal xUnit
  `MemberData` links to a unique unconditional `public static` field, property,
  or zero-argument method in the same non-partial, non-generic, base-free class.
  It records the source kind without evaluating provider rows; external types,
  runtime arguments, dynamic names, partial/inherited/generic/non-class scopes,
  overloads, missing/ineligible/conditional sources, and unresolved attribute
  identities remain `csharp_test_member_data` UNKNOWN. C# using evidence now
  follows Tree-sitter compilation-unit/namespace lexical scopes, and immutable
  member inventories are shared through `Arc` rather than copied per AST edge.
  It never runs MSBuild, Roslyn, source generators, or the
  ASP.NET Core runtime, and never evaluates preprocessor conditions. `stats`/
  `doctor` `by_language`, `unknowns` readiness detail, repo-shape scopes, and
  `repo-guard` `.cs` guarding all include the bounded `csharp` scope.
  A conservative v0.2 C/C++ preview slice (Wave C1) now discovers `.c`/`.h`
  (C grammar) and `.cc`/`.cpp`/`.cxx`/`.hh`/`.hpp`/`.hxx` (C++ grammar) files
  (skipping the CLion `cmake-build-debug`/`cmake-build-release` directories),
  recognizes include-evidence-gated GoogleTest (`TEST`/`TEST_F`/`TEST_P`/
  `TYPED_TEST`, fixture base `::testing::Test`), Catch2 `TEST_CASE`/`SCENARIO`,
  doctest `TEST_CASE`, and Boost.Test `BOOST_AUTO_TEST_CASE`/`BOOST_FIXTURE_TEST_CASE`/
  `BOOST_AUTO_TEST_SUITE` registration-macro shapes (both function-definition and
  call-expression parse forms), derives `repogrammar-cpp-derived` /
  `bounded_tree_sitter_c_cpp_anchor_v1` support only under exact-target and
  safe-origin gates with support>=3. Framework include evidence now comes only
  from exact, normalized, unconditional Tree-sitter `preproc_include` nodes;
  commented/string/pseudo/conditional includes do not count. It keeps macro
  lookalikes without include evidence, Catch2-vs-doctest ambiguity, complex or
  unclosed `#if`/`#ifdef` build variants (with an exact whole-file standard-guard
  exception), ERROR-node macro regions, Qt `Q_OBJECT`/moc and
  string SIGNAL/SLOT, and function-pointer dispatch as typed `UNKNOWN`. It parses
  `compile_commands.json`, `vcpkg.json`, and `conanfile.txt` as structural
  `PROJECT_CONFIG` inventory only. It never runs a build, compiler, preprocessor,
  or moc/protoc, and never expands macros. The 2026-07-15 identity-hardening
  review fixed three correctness/security edges: fixture bases had bypassed the
  include gate, recognized macro names had accepted unbounded arguments, and
  Boost suite pairing had not actually maintained scope state. The 2026-07-16
  follow-up aligned the bounded contracts with primary framework evidence:
  `TEST`/`TEST_F`/`TEST_P` names reject underscores, Catch2's optional second
  argument must be an explicit adjacent square-bracket tag list, and Boost's
  ordinary-call decorator whitelist enforces documented per-name signatures.
  Macro contracts use bounded Tree-sitter argument shapes, and a pure
  `test_framework.rs` pass validates nested/orphan/unclosed Boost markers with a
  source-ordered explicit stack. Completeness remains deliberately bounded:
  doctest decorator chains, namespace-aliased or multi-expression Boost
  decorators, Boost template-only forms, malformed/nonliteral Catch2 tags, and
  other version-specific signatures stay typed `UNKNOWN`; suite names are not
  case family inputs. The additional framework pass is linear in macro
  occurrences with `O(depth)` stack memory and a monotonically consumed issue
  vector.
  `stats`/`doctor` `by_language`,
  `unknowns` readiness detail, repo-shape scopes, and `repo-guard`
  `.cxx`/`.hh`/`.hxx` guarding include the bounded `c/cpp` scope.
  Wave E1 widens Rust beyond self-dogfood with general framework anchors:
  use-path/FQN-gated serde derive models, thiserror error enums,
  `#[tokio::main]`/`#[tokio::test]` entrypoints, clap derive parsers, and axum
  literal `Router::new().route(...)` segments (kinds `serde_model`,
  `thiserror_error_enum`, `tokio_entry`, `tokio_test`, `clap_parser`,
  `axum_route`) promote through the existing `repogrammar-rust-derived` support
  path under support>=3 (serde requires an equal trait/target profile; axum an
  equal HTTP method and literal path shape). General-framework tables live in
  `src/rust/adapters/frameworks/rust_general.rs` (self-dogfood policy untouched),
  chained through `frameworks/mod.rs` and the combined
  `rust_support_target_is_role_compatible`/`rust_role_is_known`/
  `rust_support_family` routers. Derive/attribute expansion
  (`rust_derive_expansion`) and axum middleware/extractor semantics stay
  non-blocking; derive-without-use (`rust_framework_attribute_binding`),
  non-literal/untraceable axum routes (`rust_axum_route_identity`), and `#[cfg]`
  stay blocking. The Rust readiness scope is now `bounded_v0_2_preview`, the
  `axum_route_model` mechanism is registered, and fixtures `serde_exact_models`,
  `thiserror_exact_errors`, `axum_exact_routes`, `derive_lookalikes`, plus the
  `rust_serde_unresolved`/`rust_serde_resolved` benchmark pair cover it. No macro
  expansion, trait resolution, or points-to is claimed.
  A bounded v0.2 Python framework-widening preview (Wave E1) extends the CPython
  AST worker beyond FastAPI/pytest/SQLAlchemy/Pydantic to Django
  (`django.db.models.Model` bases with field-count/`class Meta` variation
  context, literal `urlpatterns` `path()`/`re_path()` routes, and
  `django.test.TestCase`), Flask (`Flask(__name__)`/`Blueprint` receiver routes
  incl. Flask 2 method shortcuts), stdlib `unittest.TestCase` `test_*` methods,
  click/typer commands, and Celery `@shared_task`/`@app.task`. Each maps 1:1 to
  a `framework:{django,flask,unittest,click,typer,celery}.*` role, reuses the
  existing `repogrammar-python-derived` / `bounded_ast_anchor_v1` support>=3
  path, and keeps name lookalikes, non-literal routes, unresolvable receivers,
  settings-driven behavior, string dispatch, patch targets, and task-queue
  routing as typed `UNKNOWN`. New mechanisms `django_project_model`,
  `django_settings_model`, and `flask_app_model` join the telemetry vocabulary.
  This is preview scope and does not change the ADR-0011 v0.1 focus statement.
  `repogrammar setup` is now the primary zero-decision onboarding entrypoint.
  It creates one application-layer plan over the existing agent-install,
  repository-init/index, optional auto-sync, and product MCP self-test
  boundaries; interactive execution confirms once, `--yes` is agent-safe,
  `--dry-run` is zero-write, and telemetry remains disabled. Missing live agents
  preserve repository-only setup and do not receive a suggested coding-agent
  question. Agent ownership now distinguishes current and obsolete but
  internally consistent RepoGrammar authority: current entries are skipped,
  obsolete entries refresh through the install service, and refreshed
  pre-existing integrations remain outside setup's newly-created rollback set.
  Foreign or malformed agent configuration is
  never overwritten. A successful native probe with unrecognized configuration
  is an explicit malformed state: agent writes are blocked, repository-only
  setup continues, and recovery recommends doctor. Fresh active-index reruns
  inspect the actual family inventory, so zero-family repositories remain
  `ready_with_limitations` with source fallback instead of inheriting the
  repository-level `query_ready` flag as family evidence, while family query
  failure stays `Unknown` rather than becoming zero. Setup JSON separately
  reports ready/blocked agent targets, product self-test, agent-query,
  repository-index, auto-sync, and family-evidence readiness plus every
  limitation. Rollback removes only machine writes/receipts created by the
  current run while preserving pre-existing integrations, repo state, and valid
  active generations.
  The compact default help now centers `setup`, `find`, and `doctor`, with
  `help --all` retaining full discovery. Compact `families`, `find`, `explain`,
  and `check` human output hides internal cluster/query routing fields while
  JSON contracts retain canonical ids and `UNKNOWN` tokens. Repository and
  query consumers now reuse one authoritative recovery classifier so status,
  doctor, query preflight, setup, and MCP-facing recommendations do not derive
  conflicting next actions.
  `repogrammar init --yes` remains the agent-safe one-command repository
  bootstrap: it initializes repo-local state and builds or refreshes the active
  index by default, while `--autosync` starts auto-sync only after that readable
  active generation exists. `--state-only` preserves lifecycle-only repair
  without indexing. `repogrammar status --json` and `repogrammar doctor --json`
  now expose a source-free repository readiness object that separates
  not-initialized, state-only/no-active-index, ready active index,
  unhealthy/stale active index, autosync, and storage-hygiene states from
  pattern-family support. The readiness hygiene report covers `.repogrammar/`
  presence, ignore policy, and tracked-risk, and reports `.codegraph/` only as
  foreign unmanaged provider state with no RepoGrammar create/repair/delete
  behavior. `repogrammar logs` now reads bounded redacted tails from
  repo-local component logs and reports clean unavailable states for missing or
  unreadable logs. `repogrammar autosync` now provides optional repo-local automatic sync:
  `autosync start` enables and launches a background worker that polls the
  existing discovery fingerprint, debounces saves, and reuses the current full
  `sync` path, while `status`, `stop`, `disable`, and foreground `run` manage
  the worker. The foreground worker inherits strict-gitignore and semantic
  worker environment settings. It is explicit per repository and is not started
  by MCP serving, queries, or agent installation. Anonymous telemetry
  upload is explicit opt-in, enabled `stats --json` writes only an allowlisted
  local rollup without queue creation or network upload, does not include a
  repository instance id, reports rollup counts / upload-open-network status,
  carries only coarse experiment aggregate categories, and experiment export is
  redacted by default.
  Experiment recording prompts default-no `[y/N]` when `--yes` is absent, with
  controlled-pair warning about token/time/provider cost. Live install keeps
  telemetry independent from agent setup: `--yes` alone is not consent,
  `install --yes` without telemetry flags does not prompt and keeps telemetry
  disabled, env/CI opt-outs force disabled, and dry-run output names the planned
  native Codex/Claude Code MCP command shape. Interactive `repogrammar install`
  can select Codex, Claude Code, or both in one run, skips already managed
  RepoGrammar receipts, installs or repairs a stable `repogrammar` command when
  possible, and does not touch `.repogrammar/`. A reversible, idempotent managed
  instruction-file writer (exact `<!-- BEGIN/END REPOGRAMMAR MANAGED SECTION -->`
  markers, create/append/replace/idempotent/remove, atomic temp+rename with
  re-read verification, malformed-marker refusal) is implemented and tested, and
  receipts now record `instruction_file_path` and `instruction_action`; live
  instruction writing stays deferred unless
  `REPOGRAMMAR_INSTRUCTION_FILE_<TARGET>` resolves to an absolute path, so the
  installer never guesses real Codex/Claude instruction-file locations. The
  installer target registry also recognizes CodeGraph-style target ids for
  Cursor, opencode, Hermes, Gemini, Antigravity, and Kiro in dry-run and
  `--print-config` planning modes; live writes remain implemented only for
  global Codex and Claude Code until each additional adapter has an ownership
  receipt, uninstall inverse, and tests. Source
  checkouts also include `src/install/repogrammar-install.sh` as the
  public-facing TUI wrapper for release-binary install/repair/uninstall choices,
  with Cargo kept as an explicit contributor source-build path only and release
  artifacts bundling the current Python worker asset. The source-checkout
  wrapper can also dogfood before release assets exist by explicitly installing
  from `target/release/repogrammar` into the same RepoGrammar-managed command
  layout used by the Rust installer, then delegating agent wiring to
  `repogrammar install`. The managed layout places bundled Python workers under
  install state discoverable from the managed executable, and refresh rollback
  restores the previous managed executable/command copy if self-test or native
  agent configuration fails. It backs up older unmanaged command files before
  replacing them from the source-checkout installer, while native agent
  configuration still refuses missing or foreign managed receipts. The optional
  npm package
  `@sioyooo/repogrammar` is a thin npx launcher over those same release
  artifacts, not a JavaScript implementation; local npm dogfood can use
  `REPOGRAMMAR_BINARY` to execute an already built binary without release
  downloads. Uninstall refuses
  missing or foreign managed receipts, while `uninstall --target all` removes
  owned first-class agent receipts it finds. Ready Python exact-anchor families
  can also record metadata-only variation evidence when exact-compatible
  framework-anchor support targets differ. FastAPI static `response_model=...`, static
  `Depends(get_db)` dependency-target, `Depends`, `HTTPException`, and literal
  HTTPException status-code structural anchors, plus static FastAPI
  body/path/query/header/cookie request-shape anchors, remain auxiliary
  schema/context/effect metadata and are not membership support targets; product
  runtime tests now verify these auxiliary anchors are persisted, blocked from
  claim-input readiness, and absent from derived support facts.
  Pytest fixture decorators are now alias-aware in same-file and `conftest.py`
  contexts. Direct parametrize arguments take precedence over same-name fixtures,
  indirect parametrize arguments stay typed `PytestFixtureInjection` `UNKNOWN`,
  and fixture-edge/parametrize-argument anchors remain context metadata rather
  than family support.
  Pydantic field, field-type, `model_config`, nested `Config`,
  computed-field, and model-validator anchors likewise remain schema/config/member
  metadata and are not membership support targets; runtime validator body calls
  remain non-blocking `pydantic_validator_side_effects` UNKNOWNs. Imported
  external Pydantic/SQLAlchemy bases remain framework-identity UNKNOWNs unless
  they resolve to exact supported framework bases.
  FastAPI same-function service-call anchors remain handler/service context
  metadata and are not membership support targets.
  SQLAlchemy `relationship` and
  `Session.add`/`AsyncSession.add` anchors are also structural context/effect
  metadata, not family membership support; product runtime tests verify
  `relationship` and `Session.add` stay blocked from claim-input readiness and
  absent from derived support facts. SQLAlchemy session call anchors now
  include direct `Session.commit`, `Session.rollback`, `Session.scalar`,
  `Session.scalars`, and async equivalents plus bounded propagation from
  `__init__`-assigned `self.session`/`self.db` attributes, with same-method
  receiver reassignment and custom query wrappers blocking
  canonicalization. Rust `ports::python_provider`, `ports::rust_provider`, and
  `ports::tsjs_provider` contracts now exist for future candidate-scoped
  provider requests, provenance assumptions, cache-key dimensions, and
  recoverable provider-unavailable `UNKNOWN`s. The application layer can plan
  validated Pyrefly framework-identity request scopes for plausible Python
  candidate groups and skip parser-origin blocking `UNKNOWN`s for the planned
  claim, but Pyrefly/Pyright/RightTyper execution, provider fact storage, and
  provider-backed canonical evidence remain deferred. Except for the default
  safe Rust Cargo metadata project-model stage, Rust/TSJS provider execution is
  also deferred and tracked in
  `docs/plans/rust-tsjs-semantic-analysis-plan.md`. The planner can run over
  active-generation snapshots without mutating semantic facts, family rows, or
  CLI/MCP output.
  The Rust Cargo metadata provider adapter can parse
  `cargo metadata --format-version=1 --no-deps` output into owned
  `PROJECT_CONFIG` facts and recoverable provider `UNKNOWN`s during
  `index`/`sync`/`resync` when same-generation `Cargo.toml` code units exist;
  rust-analyzer/rustc/rustdoc JSON semantic providers remain deferred.
  The `dynamic-unknown` release fixture now exercises dynamic Pydantic model
  factories, dynamic import, `sys.path` mutation, dynamic call target,
  dynamic/unresolved decorator, and monkey-patch boundaries through product indexing/query paths;
  each boundary is persisted as typed parser-origin `UNKNOWN`, blocked from
  claim-input readiness, and kept out of derived support.
  Exact-anchor derivation now treats only top-level imports as file-level
  framework aliases and resolves those aliases by source position: units before
  a top-level shadowing definition or assignment may still use the framework
  import, while later units cannot. Module-level dynamic import or `sys.path`
  mutation is copied into unit-scoped blocking `UNKNOWN`s only for later
  family-shaped units in the same file. Final Python family construction also
  consumes parser-origin context features for complete-link compatibility and
  repeats claim-scoped blocking `UNKNOWN` checks before emitting a confident
  family. Python worker dynamic call aliases now keep assigned or chained
  `importlib.import_module`, `__import__`, `eval`, `exec`, `compile`,
  `locals`/`globals`/`vars`, `getattr`, and `setattr` dynamic use as typed
  `UNKNOWN` unless the literal import can be uniquely resolved as repo-local
  structural context.
  Product-path regression coverage now also indexes local framework lookalikes
  such as `@client.get(...)`, user-defined `BaseModel`, and user-defined
  SQLAlchemy-shaped `Base` classes and verifies they remain non-family
  evidence through query output.
  Release smoke coverage now exercises the committed `stale-evidence` fixture
  for mutation/deletion freshness failures and the FastAPI/APIRouter route
  method variation fixture across the full `delete`/`get`/`head`/`options`/
  `patch`/`post`/`put` matrix. Discovery regression coverage explicitly covers
  the Python v0.1 virtualenv/cache/build/dependency skip directory matrix,
  Python gitignore behavior in root and parent-worktree layouts, and explicit
  strict gitignore failure when Git ignore checks are unavailable. CLI/MCP
  query inputs share target and token-budget bounds.
- Last updated: 2026-07-16
- Scope: Current implemented capability snapshot.
- Evidence: Rust code, README, roadmap, CLI/storage/indexing specs, and
  `repo-guard` checks.
- Related canonical docs: `README.md`, `docs/roadmap.md`,
  `docs/specifications/cli.md`, `docs/specifications/mcp-api.md`,
  `docs/specifications/storage.md`, `docs/specifications/indexing-pipeline.md`,
  `docs/specifications/unknowns.md`, `docs/specifications/product.md`,
  `docs/specifications/installation.md`, `docs/development/testing.md`,
  `docs/plans/v0.2-agent-adoption-read-displacement-plan.md`,
  `docs/specifications/python-analysis.md`,
  `docs/decisions/ADR-0011-python-first-v0-1.md`,
  `docs/decisions/ADR-0012-python-selective-analysis-cascade.md`,
  `docs/decisions/ADR-0020-top-20-language-expansion-gate.md`,
  `docs/decisions/ADR-0021-go-standard-library-semantic-worker-preflight.md`,
  `docs/decisions/ADR-0024-php-sandboxed-frontend-phpunit-preflight.md`,
  `docs/decisions/ADR-0025-swift-syntax-sourcekit-xctest-preflight.md`,
  `docs/plans/top-20-language-expansion-plan.md`,
  `docs/reports/language-support/go-completion-review.md`,
  `docs/reports/language-support/php-completion-review.md`,
  `docs/reports/language-support/swift-completion-review.md`
- Supersedes: None
- Superseded by: None

## Context

RepoGrammar is still pre-alpha, but it is past pure skeleton bootstrap. The
current branch has repository-local lifecycle, transitional
TypeScript/JavaScript discovery, Python `.py` discovery with Python
virtualenv/cache/dependency skips, generation-scoped SQLite storage, syntax-only
TS/JS code-unit indexing, CPython AST-backed Python structural code-unit
indexing, worker-local Python structural facts for imports, decorators, class
bases, simple calls, `pytest.test` test-function anchors, alias-aware pytest
fixture decorators, same-file pytest test/fixture dependency edges, and typed
dynamic Pydantic model factory, dynamic/unresolved decorator, dynamic call
including `locals()[...]`, `eval`, `exec`, and `compile`, monkey-patch,
dynamic import including `__import__`, `sys.path` mutation, or unresolved
import `UNKNOWN` cases persisted as internal parser-origin semantic facts. It
also labels FastAPI route
`response_model`, static dependency targets, `Depends`/`HTTPException`, literal
HTTPException status codes, static FastAPI body/path/query/header/cookie
request-shape markers, literal pytest parametrize arguments, Pydantic
field/config/member declarations, and bounded FastAPI same-function service
calls as structural parser-origin anchors without upgrading them to
provider-backed semantics. Dynamic Pydantic model factories remain typed
framework-identity `UNKNOWN` rather than static model-family support.
Default parser-mode indexing now also carries sanitized root project-config
source roots from `pyproject.toml`/`tomllib`, `setup.cfg`/`configparser`, and
static `setup.py`/CPython-AST facts plus bounded discovered
`conftest.py` context into the CPython parse-document request so source-rooted
repo-local import facts, same-file fixture dependency facts, and
parent-directory pytest fixture-edge facts can be persisted structurally; those
facts are still not default family evidence. The
semantic-worker-compatible Python project mode can also output requested
`conftest.py` fixture hierarchy edges. Exact root `pyproject.toml`, `setup.cfg`,
and `setup.py` are persisted as `python-config` files and `project_config` units
with sanitized structural config metadata or typed/conservative config results;
`setup.py` is parsed with CPython `ast` and never executed, and setup/package-
finder calls contribute only from a direct unconditional setup expression when
a direct, aliased, or qualified `setuptools` import remains lexically unshadowed,
undeleted, and free of recognized relevant attribute/namespace mutation at the
call. Accepted setup calls have zero positional arguments, no unpacking, unique
relevant keywords, complete unique string mappings, and unambiguous literal
finder roots; incomplete or top-level-unreachable recognized config becomes
typed `MissingProjectConfig`, while `setup()` remains valid empty config,
and multiple authoritative calls become `ConflictingFacts` rather than merged
project metadata,
syntax-origin TS/JS and Python framework-role fact storage, bounded exact-anchor
Python support derivation that excludes units with claim-relevant parser-origin
blocking `UNKNOWN`s,
bounded Python complete-link clustering over support-family and parser-context
features so bridge members cannot single-link incompatible Python support
families, plus metadata-only variation slots when context profiles differ
inside an already-supported Python family. pytest non-builtin fixture context
must match for family compatibility, while known builtin fixture context may
remain metadata-only variation/context,
CodeUnit-derived structural IR node/containment-edge storage, Rust-side
TypeScript semantic-worker
request/output protocol validation and process validation, a dependency-free
TypeScript worker stub that reports compiler analysis as unavailable, a
validated semantic-fact storage writer, opt-in command-level semantic-worker
fact ingestion through the same-generation storage gate, conservative
FamilyStore-backed query reads, and a read-only MCP `repogrammar_context` stdio
boundary. It also has live installer/uninstaller writes for global Codex and
Claude Code MCP targets through native agent CLIs, gated by `--yes` or the
interactive wizard, MCP self-test, all-or-rollback multi-target handling, and
RepoGrammar-owned receipts. It also has an internal
active-generation claim-input snapshot read path for future claim builders and
an internal file-hash freshness/readiness gate that blocks stale facts,
unsupported fact kinds, weak certainty, or conflicting certainty with typed
`UNKNOWN`. It also has committed TS/JS release fixtures and a product CLI JSON
smoke gate that exercises `init`, `index`, `files`, `units`, pattern-family
queries, and `doctor` without treating syntax-only evidence as a family claim.
ADR-0011 pivots the official v0.1 implementation target to Python-first
analysis for FastAPI, pytest, SQLAlchemy, and Pydantic; ADR-0012 defines that
the implementation should use a claim-driven selective cascade rather than
running every analyzer over every file. The current Python implementation is
the first CPython AST structural slice with worker-local structural anchors and
typed dynamic/unresolved `UNKNOWN` output plus narrow exact-anchor derived
support for canonical framework targets. The current TS/JS substrate remains
useful scaffolding but must not be described as the official v0.1 target.

## Durable knowledge

Implemented capabilities include module boundaries, minimal domain types,
pattern-family-first CLI command parsing, safe installer dry-run planning, typed
progress and telemetry policy types, stable not-implemented behavior,
transport-neutral MCP single-tool operation boundary, read-only MCP serving,
repository guard checks,
documentation, skills, memories, CI configuration, repo-local
`init`/`uninit`/`status`/`doctor`/`unlock`/`logs`, TS/JS and Python file
discovery, hash-checked source reads, dependency-free TS/JS syntax-only
code-unit extraction, CPython AST-backed Python structural code-unit
extraction, worker-local Python structural fact payloads for import bindings,
decorator anchors, class bases, simple call targets, `pytest.test`
test-function anchors, alias-aware pytest fixture decorators, same-file pytest
test/fixture dependency edges, parent-directory `conftest.py` fixture hierarchy edges, FastAPI
dependency/error/request-shape anchors, pytest parametrize argument anchors that
are not treated as fixture injection UNKNOWNs,
Pydantic field/config/member anchors, typed dynamic Pydantic model factory
framework-identity `UNKNOWN`, typed dynamic/unresolved decorator
framework-identity `UNKNOWN`, monkey-patch call-target `UNKNOWN`, and typed
  dynamic/unresolved import `UNKNOWN` cases including `__import__`, plus bare
  or indexed `locals`/`globals`, `eval`, `exec`, and `compile` call-target
  `UNKNOWN`s, plus bounded same-function FastAPI
service-call anchors with reassignment invalidation,
syntax-origin
framework-role facts for recognized Express, React, Jest/Vitest, FastAPI,
pytest, Pydantic, and SQLAlchemy code-unit shapes,
exact root `pyproject.toml`, `setup.cfg`, and `setup.py` discovery and sanitized
structural project-config records, sanitized project-config source roots reused
as default parser context, with unbound, standalone, conditional,
shadowed/deleted/mutated calls abstaining and recognized incomplete/overridable/
top-level-unreachable/computed config becoming typed `UNKNOWN`, with roots
from coexisting formats unioned only as structural candidate context,
bounded `DATAFLOW_DERIVED` support facts derived only from exact canonical
framework anchors when the unit has one Python framework role and no
claim-relevant parser-origin blocking `UNKNOWN`,
bounded complete-link Python family clustering over internal support-family
features,
CodeUnit-derived structural IR nodes and
conservative containment edges, generation-scoped SQLite
migrations/storage/validation/activation, product runtime wiring for `index`
and `sync`, and the dependency-free
`src/workers/typescript/worker.js` unavailable fallback stub, plus limited
`files`/`units` reads from active file-manifest-only or syntax-only generations.
Those reads revalidate active-generation health plus stored paths, hashes,
languages, unit ids, and byte ranges before returning repo-relative metadata.
Release fixture smoke coverage copies committed TS/JS and Python fixtures into
temporary workspaces and checks product CLI JSON paths, no absolute-path
leakage, no source-snippet or parser/provider-internal leakage, and
conservative `UNKNOWN` query results by default. Python release fixtures cover
direct FastAPI, FastAPI alias, pytest, alias-aware pytest fixtures,
Pydantic model/settings, SQLAlchemy,
mixed, dynamic-unknown, and low-support examples. The dynamic fixture covers
dynamic Pydantic model factories, dynamic import, `sys.path` mutation, dynamic
call target, dynamic/unresolved decorator, and monkey patching without producing family
claims. Positive direct FastAPI,
FastAPI alias, pytest tests, pytest fixtures, Pydantic model/settings,
SQLAlchemy model-field, and SQLAlchemy session/repository including
commit/rollback and scalar/scalars fixtures now validate the
no-worker exact-anchor derived-support family path, exact-anchor target variation metadata,
metadata-only evidence modes, MCP parity, and stale-evidence query refusal. A
separate
test-only strong FastAPI semantic-support fixture injects compatible `SEMANTIC`
facts through the
existing worker boundary to validate family reads and stale-evidence fallback
without claiming production Python semantic-provider support.
Family detail reads now use compact/evidence/deep output modes. Compact omits
evidence records; evidence/deep run deterministic greedy metadata selection
under an optional token budget and report `source_snippets_included: false`.
Family evidence records carry schema-backed `covered_claims` labels. The
current builder emits `canonical` and `support`, plus one narrow Python
`variation` label when an already-ready family has multiple exact-compatible
framework-anchor support targets. Requested exception coverage and broader
variation coverage are returned in `missing_claims` until later builders link
evidence to those roles.
`index`, `sync`, and `resync` acquire `.repogrammar/locks/index.lock` before
discovery and hold it through validation and activation. Lock metadata is
published as a complete record when the filesystem supports atomic publication,
partial metadata write failures must remove the partial lock file, and
same-host stale detection uses native process liveness on Windows and Unix so a
dead nonzero PID can be replaced safely while impossible current-OS PID values
are rejected before probing. `unlock --force --yes` removes only confirmed
stale `index.lock`; active, unknown, invalid, daemon, and SQLite locks remain
in place. Status and doctor JSON use explicit manifest/storage schema-version
fields and do not expose ambiguous `schema_version` fields.
Auto-sync daemon acquisition writes complete `.repogrammar/locks/daemon.lock`
metadata through a temporary file plus atomic publish when supported, falls
back to create-new semantics when needed, and removes stale daemon locks only
when the inspected bytes still match. Repository readiness and autosync status
now use the same process-liveness policy; on Unix, a daemon lock is running
only when the PID is live and its command line confirms `repogrammar autosync
run`. Startup additionally uses a bounded PID-plus-startup-nonce daemon-lock
handshake and verifies the spawned child is still alive before reporting ready;
immediate exit, lock refusal, and timeout fail typed. Stop and guard cleanup use
compare-and-remove so a concurrently replaced daemon lock is preserved.
The storage port and SQLite adapter can persist already-validated semantic facts
and repo-relative evidence for building generations when they match an indexed
same-generation code unit's path, content hash, and byte range. By default
`index`, `sync`, and `resync` still report `semantic_worker: deferred`; when
`REPOGRAMMAR_TYPESCRIPT_WORKER` names an explicit worker executable, optional
`REPOGRAMMAR_TYPESCRIPT_WORKER_ARGS_JSON` supplies its argv vector, and accepted
worker facts may be recorded before generation validation and activation.
Worker fallback keeps indexing syntax-only, while mismatched semantic evidence
aborts the new generation.
The application query/storage boundary can load an internal active-generation
claim-input snapshot containing files, code units, IR nodes/edges, and semantic
facts after revalidating stored fact kind/certainty tokens, assumptions JSON,
repo-relative evidence, content hashes, code-unit ids, and byte ranges. This is
an internal substrate only; CLI/MCP query commands do not render semantic facts.
The query application layer can check snapshot semantic facts against current
source hashes and classify fresh supported facts as eligible inputs for future
claim builders or typed `UNKNOWN` blockers (`StaleEvidence`,
`InsufficientSupport`, or `ConflictingFacts`). Fresh eligible facts are still not
family evidence by themselves. The current EC-MVFI-lite builder can persist a
family only when repeated framework-role candidates also have fresh
same-generation `SEMANTIC` or `DATAFLOW_DERIVED` support that is compatible
with the framework role; arbitrary unrelated semantic facts remain
`InsufficientSupport`. Public CLI/MCP family reads now exact-match `family` and
`member` targets, keep fuzzy matching limited to find/explain/check style
queries, gate rendered family evidence against current source hashes, and
report stale evidence as typed `StaleEvidence` `UNKNOWN`. Syntax-origin
framework-role facts use `FRAMEWORK_HEURISTIC` certainty and remain blocked
from family-claim input as insufficient support without stronger compatible
evidence. A conservative TS/JS exact-anchor path now exists alongside the Python
one: the syntax parser emits `STRUCTURAL` anchors (engine
`repogrammar-tsjs-syntax`, method `exact_anchor_v1`) for exact Express,
Jest/Vitest (with Mocha/`node:test` `runner_kind` aliasing that never merges
distinct runners), Zod schema builders, NestJS
controller/route/injectable/module decorators bound to `@nestjs/common` (routes
require containment in an exact controller; a bounded decorator-prefix scan
extends class/method unit starts over their `@Decorator(...)` stack), Hono
`new Hono()` receiver routes, Next.js, Fastify, Prisma, and Drizzle shapes, and
the application layer promotes them to `DATAFLOW_DERIVED` support (engine
`repogrammar-tsjs-derived`, method `bounded_exact_anchor_v1`, assumptions
`provider_resolved=false`, `derived_from=tsjs_structural_anchors`,
framework-specific `derived_from=tsjs_<framework>_structural_anchors`,
`framework_role=<role>`, `tsjs_anchor_kind=<kind>`). The family gate matches
exact TS/JS targets plus a safe origin instead of substring text, requires at
least three compatible TS/JS support facts, and applies complete-link
compatibility over conservative route/test/component/query feature profiles so
bridge members cannot single-link incompatible families. Bounded `package.json`,
`tsconfig.json`, `jsconfig.json`, and Jest/Vitest config inventory is stored only
as structural context or typed config `UNKNOWN`. Parser-mode TS/JS indexing also
passes discovered TS/JS module paths, JSON path aliases, package dependencies,
and bounded test-runner package/config context into the syntax parser. Unique
literal relative and path-alias imports can be persisted as structural
repo-local import context; dynamic imports, non-literal/conditional `require`,
unresolved or conflicting aliases, and star re-exports stay typed `UNKNOWN`.
Ambient Jest/Vitest globals require package/config test-runner context rather
than test-file path alone (mocha dependency or a `.mocharc.*` config now also
provides ambient context). Reassigned/shadowed/dynamic/lookalike Express,
Fastify, or Hono receivers, dynamic route methods, custom Jest/Vitest/Mocha
wrappers, ambiguous non-test-file globals, Next middleware/server
actions/re-exports, Prisma bulk/raw/injected/dynamic clients, Drizzle
raw/dynamic builders, Zod schemas without an exact `zod` import, NestJS
decorators not bound to `@nestjs/common` or routes outside an exact controller
(`tsjs_nest_controller_identity`), Hono routes on untraced receivers
(`tsjs_hono_receiver`), and all React components/hooks stay `UNKNOWN` or
non-supporting context. NestJS DI/token resolution and dynamic modules, and Zod
runtime refinement, are non-blocking subclaims recovered via `nestjs_di_model`
or existing framework buckets and never block families. Exact local
Next dynamic segments, route groups, and parallel routes are retained as
context assumptions on page/layout/route anchors. React-shaped TypeScript
semantic-worker support facts are also blocked from public-preview family
claims; React remains unsupported until a later ADR changes the support matrix.
Jest/Vitest script
configs (`jest.config.ts`, `vitest.config.js`, and similar) are metadata/typed
`UNKNOWN` only; package dependencies and JSON config files provide the current
ambient runner context.
`src/fixtures/typescript/release/v0_2/express_exact_routes`,
`jest_vitest_exact_tests`, `next_exact_routes`, `fastify_exact_routes`,
`prisma_exact_repositories`, `drizzle_exact_repositories`, `zod_exact_schemas`,
`nest_exact_controllers`, `hono_exact_routes`, `mocha_exact_tests`,
`framework_adapter_negative_cases`, `unsupported_framework_lookalikes`,
`tsjs_new_framework_lookalikes`, and the `tsjs_nest_unresolved`/`tsjs_nest_resolved`
UNKNOWN-reduction pair exercise the positive and negative product paths; the
v0.1 `jest-vitest-basic`
fixture remains below the conservative TS/JS support threshold and returns
`UNKNOWN` rather than forming ambient suite/test families. TS/JS remains a
transitional substrate, not the official Python-first v0.1 target, and this is
not full TS/JS semantic analysis.

Family query output, MCP `repogrammar_context` family responses, and
`repogrammar stats` now expose `estimated_potential_token_savings` as an
`ESTIMATED` local potential-read-displacement diagnostic. Query responses also
expose source-free `query_route` metadata: fuzzy find/explain/check operations
start from the path, symbol/member id, framework role, or pattern question the
agent already has, discover/hydrate bounded candidate families internally, and
return family ids as follow-up handles rather than required initial inputs.
Successful family context responses update only a repo-local aggregate under
`.repogrammar/telemetry/local-metrics/`; this is not measured token savings, not
causal evidence, and not part of anonymous telemetry upload payloads. Stats
readiness and blocking reasons explain whether estimated displacement is
plausible, but measured savings remain absent unless a comparable paired
experiment exists.

Tree-sitter integration, TypeScript compiler API integration,
provider-backed Python project-configuration semantics, Pyrefly/Pyright
provider execution, provider-backed canonical framework evidence,
command-level full repository/worktree freshness metadata, typed IR attributes
beyond the structural bootstrap graph, resolved framework semantics, full
family mining, project-local installer writes, live instruction-file writes by
default (the managed marker-fenced writer is implemented and tested, but live
writing stays deferred unless `REPOGRAMMAR_INSTRUCTION_FILE_<TARGET>` resolves to
an absolute path), additional coding-agent integrations, and telemetry network
transport are not implemented.

Pattern-family query commands and MCP tool calls still use stable fallback
behavior before an active index and typed `UNKNOWN` when active evidence is
insufficient. Advisory `check`/`check_conformance` responses may return matched
family context as `CONTEXT_ONLY`, but conformance remains nested `UNKNOWN`
because runtime equivalence is unproven. `files` and `units` can return active
file-manifest-only or syntax-only index metadata, but stored syntax-only units
must not be described as query-ready family evidence.

## Implications

ADR-0020 now freezes the TIOBE July 2026 Top-20 list as a planning snapshot and
defines the evidence gate for future language completion. Seven ranked
languages have current structural paths (Python, C, C++, Java, C#, JavaScript,
Rust); TypeScript is a separate current extra; thirteen ranked languages remain
new expansion work. This is normative future scope, not implemented support.
Extension discovery or a structural grammar alone must never advance a
language to supported status. Use
`docs/plans/top-20-language-expansion-plan.md` for the disjoint convergence and
new-language waves, and require a source-free four-part completion review for
each language.

Filesystem discovery and the autosync metadata fingerprint now share fixed
aggregate admission: 100,000 accepted supported files, 512 MiB accepted bytes,
250,000 visited entries, and depth 256; discovery also caps its reported skip
collection at 100,000. Exact limits succeed and plus one fails with a typed,
path/source-free invalid-input error before any generation is prepared. The
autosync fingerprint charges accepted metadata file sizes even though it hashes
only metadata, keeping watcher admission conservative with the next index.
It does not evaluate Git ignore during polling, so supported Git-ignored files
count and may make autosync stricter than manual discovery. A durable security
follow-up remains open: aggregate bounds limit work but do not close the
pre-existing canonicalize-then-reopen tree-swap race. Discovery, source reads,
and fingerprinting eventually need one cross-platform no-follow,
handle-relative traversal before claiming concurrent filesystem confinement.
ADR-0023 now accepts that design and a qualification-first six-stage sequence:
pin the root, reopen children relative to retained directory handles, and use
one validated component per no-follow open plus the same special-file-safe
regular-file handle for metadata and bounded content. The exact
`cap-std`/`cap-fs-ext` 4.0.2 candidate is not admitted; transitive, advisory,
build-script, five-target compile, native Linux/macOS/Windows runtime, Unix
FIFO, Windows junction/reparse, simultaneous consumer migration, and final
audit evidence remain missing. Mount topology and physical device/inode origin
remain outside the narrow pathname-redirection claim. The P2 gap and incomplete
completion review remain open.

Go's N1 preflight is accepted by ADR-0021, and its discovery/config module now
provides bounded `.go` plus root/nested `go.mod`/`go.work` inventory with
distinct `go`/`go-config` tokens. Go is `discovered_only` and unsupported.
Full and incremental indexing skip parser-facing source-store reads and parsing for these tokens,
aggregate path-free unsupported warnings by token, persist only source-free
file metadata, and emit no units, facts, IR, or families. Go-only and empty
generations report `file_manifest_only` with a deferred parser; mixed parsed
generations remain syntax-only even when an incremental round has zero parser
attempts. Warnings come from the whole current manifest. Inventory-only
add/modify/remove deltas stay incremental, and copy-forward purges any legacy
Go units, IR, facts, derived support, or families while retaining metadata.
The frontend must restore token-based project-context invalidation when Go
enters `ParserProjectContext`. A pure normalized-
path classifier records Go-tool exclusions, `_test.go`, and the dated Go 1.26.5
known GOOS/GOARCH suffix shape without selecting an environment or proving
support. Build-tag/generated/cgo/`go:generate` marker scanning is assigned to
frontend/IR rather than approximated by text matching.

A future version-pinned Tree-sitter Go grammar may generate syntax candidates
only. The authoritative semantic path is an explicit, opt-in,
sandboxed Go standard-library worker over supplied inputs using `go/parser`,
`go/token`, candidate-scoped `go/types`, and `go/build/constraint`; the safe
default must not invoke `go/packages`, `go list`, gopls, cgo, or repository
build/test/generate commands or read repository/home/credential/cache files.
The current generic process adapter is not the required Go OS sandbox. Unadjusted
repo-relative byte ranges, an in-memory controlled importer, ordered path/hash
cache manifests, and the worker artifact digest are mandatory. Build
constraints, GOOS/GOARCH suffixes, generated files, module/workspace resolution,
external types, interface/dynamic dispatch, cgo,
and `go:generate` remain structural or typed `UNKNOWN` until an authorized
mechanism resolves the exact obligation.

PHP's N1 preflight and discovery/configuration slice are recorded by ADR-0024.
PHP is `discovered_only` and remains unsupported. Stable `php`/`php-config`
tokens inventory exact `.php` and exact root/nested `composer.json`,
`composer.lock`, `phpunit.xml`, and `phpunit.xml.dist` basenames. Configuration
classification precedes source suffix matching; literal `.php` is eligible.
Exact `.composer`/`.phpunit.cache` are PHP-only exclusions with the stable
`language_specific_exclusion` token and must not globally prune unrelated
languages; exact `vendor` remains globally excluded. Accepted PHP files persist
only repo-relative path, raw-byte SHA-256, size, and token. PHP-only generations
report `file_manifest_only`; mixed parser-capable generations remain
`syntax_only_code_units`. Warnings are deterministic and aggregated once per
accepted token. Incremental PHP deltas add, modify, remove, or copy only file
metadata and purge legacy PHP units, IR, facts, evidence, support, and families.
Manual discovery honors Git ignore; autosync keeps its generic Git-independent
conservative charging. PHP paths bypass the source store and parser and produce
zero code units, IR, facts, typed `UNKNOWN`s, families, project-model records,
or readiness/support claims.

The production candidate is `mago-syntax` 1.43.0 only in a separately reviewed
OS-sandboxed worker. Official PHP 8.5.8 `php -n -l` is the isolated syntax-
validity oracle; `nikic/PHP-Parser` 5.8.0 is the isolated AST/location
differential and separately qualification-gated fallback. Tree-sitter PHP
0.24.2 is syntax fallback only. The future first exact family is
`php.phpunit.test_method`, and Composer JSON/lock plus PHPUnit XML remain
bounded non-executing data for a separate project-model parser pinned to
Composer 2.10.2 lock-content-hash semantics. No PHP source may enter a frontend
before that model applies project selection and custom exclusions. Custom
`vendor-dir`, PHPUnit cache-directory selection, PHP project-context
invalidation, and every parser/family gate remain unimplemented. All
dependency, sandbox, protocol, resource, target, `UNKNOWN`, family, product,
and completion-review gates remain open.

Swift's N1 preflight and discovery/configuration slice are recorded by
ADR-0025. Swift is `discovered_only` and remains unsupported. Stable `swift`
and `swift-config` tokens inventory exact case-sensitive `.swift` paths and
exact root/nested `Package.swift`, `Package.resolved`, `.swift-version`, and
complete ASCII `Package@swift-M[.m[.p]].swift` basenames. Configuration
classification precedes source classification; invalid version-manifest
lookalikes with exact `.swift` remain ordinary source inventory. Exact
`.build`/`.swiftpm` components are Swift-only exclusions with the stable
`language_specific_exclusion` token and must not globally prune unrelated
language files.

Accepted Swift files persist only bounded repo-relative path, raw-byte SHA-256,
size, and token. Swift-only generations report `file_manifest_only`; mixed
parser-capable generations retain `syntax_only_code_units`. Warnings are
deterministic and aggregated once per accepted token. Incremental Swift deltas
add, modify, remove, or copy only metadata and purge legacy units, IR, facts,
evidence, support, and families. Manual discovery honors Git ignore; autosync
keeps its generic Git-independent conservative charging. Swift paths bypass
the source store and parser and produce zero code units, IR, facts, typed
`UNKNOWN`s, families, project records, or readiness/support claims.

The future syntax candidate is exact SwiftSyntax 603.0.2 `SwiftParser` in a
separately reviewed OS-sandboxed worker, differentially qualified against the
exact Swift 6.3.3 compiler. Exact 6.3.3 SourceKit-LSP/sourcekitd/compiler is
only a separately qualified semantic identity candidate and must not open or
build the repository, evaluate manifests, resolve dependencies, load target
modules/macros/plugins, use ambient SDK/toolchain/cache state, spawn children,
or use network access. A bounded static SwiftPM project model may parse supplied
bytes but never execute Swift. The first family is the narrow direct
`swift.xctest.test_method`; it requires selected test-target, clean syntax,
exact platform XCTest module/immediate-superclass identity, direct zero-
parameter no-value-return instance method, freshness, compatibility, and
support >= 3. Swift Testing remains deferred because `@Test` is a compiler
macro. The next permitted module is documentation/evidence-only artifact,
compiler-differential, dependency, supply-chain, five-target, and native
sandbox qualification. Every later project, IR, obligation, family, semantic-
product, cross-module-review, and completion-audit gate remains open.

The durable Swift pause baseline is preflight commit
`d293238723c0b943d9665f05a4db948fba0f0e35` plus reviewed discovery commit
`9bc1960db21e62216f2c9b85e88e32e9733390b0` on
`feature/framework-wave-2`. Correctness, security, and design/performance
reviews were clean; the full required gates and post-commit documentation gate
passed, the worktree was clean, and nothing was pushed or merged. Resume from
`docs/plans/swift-n1-qualification-handoff.md`; it limits the next session to a
documentation/evidence-only `QUALIFIED`, `NO_GO`, `BLOCKED`, or `INCONCLUSIVE`
stage-3 verdict and forbids production dependency/worker admission in the same
commit. At this checkpoint the strict ADR-0020 terminal count is `0/20`; Go,
PHP, Swift, and Ruby are the four `discovered_only` additions, not supported
languages.

Ruby's N1 preflight and discovery/configuration slice are recorded by ADR-0022;
Ruby is `discovered_only` and remains unsupported. Stable `ruby`/`ruby-config`
tokens inventory bounded `.rb`, `Gemfile`, `Gemfile.lock`, `gems.rb`,
`gems.locked`, `.ruby-version`, and `*.gemspec` paths. Configuration
classification precedes source classification, so `gems.rb` is always
`ruby-config`; literal `.rb` and `.gemspec` basenames are eligible.
`.bundle`/`.ruby-lsp` are Ruby-only exclusions with the stable
`language_specific_exclusion` skip token and must not globally prune another
language's files. Accepted Ruby files persist only repo-relative path, content
hash, size, and token. Ruby-only/empty generations report `file_manifest_only`
with parsing deferred; mixed generations with parser-capable languages remain
`syntax_only_code_units`. Warnings are deterministic and aggregated once per
accepted token. Incremental Ruby deltas add, modify, remove, or copy only file
metadata and purge seeded legacy Ruby units, IR, facts, support, and families.
Manual discovery honors Git ignore; autosync fingerprinting intentionally keeps
its generic Git-independent conservative charging. Ruby paths bypass the source
store and parser and produce zero code units, IR, facts, typed `UNKNOWN`s,
families, project-model records, or readiness/support claims. Restore Ruby
project-context invalidation before any future frontend admits these tokens.

The native `ruby-prism` candidate is not an authorized dependency and must not
be linked into the primary process for untrusted input. ADR-0022 requires a
documentation/evidence-only dependency and sandbox qualification before a
separate production worker/artifact admission stage, then frontend/IR and the
authoritative obligation registry. Until that registry lands, future Ruby
obligations are unavailable rather than emitted `UNKNOWN`s; generated-source
blocking will require bounded positive generated/conflicting-origin evidence.
Exact provenance, version-profile, resource, target, worker-input, first-family,
and ten-stage contracts are canonical in ADR-0022 and its completion review.
Never evaluate Gemfiles/gemspecs or run Ruby, Bundler, RubyGems, Rake, Rails,
tests, generators, installed gems, repository tooling, or request-time network
for default Ruby analysis. No dependency, Ruby/Bundler command, tool probe, or
network access is part of the discovery slice; every later gate remains open.

Future agents must not claim compiler-backed TypeScript analysis,
provider-backed Python semantic analysis, full pattern-family mining,
freshness-validated semantic claims, installer writes beyond global
Codex/Claude MCP registration, or stable MCP API support until
those capabilities are implemented and tested.
Agents also must not restart repo-local lifecycle, SQLite generation, opt-in
semantic-worker ingestion, or Rust-side worker process validation work from
scratch. Do not restart structural IR storage or active semantic-fact/evidence
read-path work from scratch either; extend the existing lifecycle, storage,
worker stub, query read path, and worker boundary substrates through the
canonical specs.

## Revalidation conditions

Update this memory after provider-backed project-configuration semantics,
Pyrefly/Pyright provider integration, Tree-sitter fallback, TypeScript compiler
API integration, full family-claim gates, broader installer writes, production
family evidence, or stable MCP API support lands.
