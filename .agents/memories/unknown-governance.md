# UNKNOWN Governance

- Status: Active
- Last updated: 2026-07-16
- Scope: Durable reminders for uncertainty handling.
- Evidence: `docs/specifications/unknowns.md`
- Related canonical docs: `docs/specifications/domain-model.md`, `docs/specifications/semantic-workers.md`
- Supersedes: None
- Superseded by: None

## Durable knowledge

- `UNKNOWN` is a first-class typed result, not a temporary error string.
- Structural evidence can generate candidates but cannot prove semantic family
  membership.
- Compiler-native semantic facts outrank structural and framework-heuristic
  facts.
- Conflicting, stale, unavailable, unsupported, low-confidence, or dynamic facts
  should become `UNKNOWN` or abstention for affected claims.
- Syntax-only code units are structural candidates only; they are not semantic
  facts, framework-equivalence claims, or family evidence.
- Syntax-origin framework-role facts are semantic fact records, but their
  `FRAMEWORK_HEURISTIC` certainty remains insufficient support for family-claim
  input.
- The Rust domain now has stable typed UNKNOWN class/reason tokens, and the
  query application layer uses them for internal semantic-fact claim-input
  readiness. Stale or missing source blocks with `StaleEvidence`, weak certainty
  and `UNKNOWN` fact kind block with `InsufficientSupport`, and conflicting
  certainty blocks with `ConflictingFacts`.
- Root `Cargo.toml` `rust_build_variant` UNKNOWNs may block repository-wide Rust
  self-dogfood families, but nested fixture/package manifests must not globally
  block unrelated root Rust family claims; keep those unknowns scoped to the
  affected package/crate/claim until provider-backed package scope exists.
- Source-level Rust `#[cfg]` / `#[cfg_attr]` UNKNOWNs may carry bounded nearest
  `Cargo.toml` feature context, including simple feature predicates and whether
  each feature is declared. This is `cargo_feature_cfg_model` triage only; it
  does not evaluate cfgs or make the unit eligible family support. Family/query
  recovery may summarize the feature state for agents while keeping the claim
  blocking.
- Rust repo-local `use crate::...`, `use super::...`, and `use self::...` paths
  may be recorded as structural import context when the path is exact. Complex
  repo-local use trees remain `UnresolvedImport`; external dependency use paths
  remain Cargo dependency context or future provider work, not module proof.
- TS/JS Zod/NestJS/Hono/Mocha preview UNKNOWNs are claim-scoped. NestJS
  decorators not bound to `@nestjs/common`, and `@Get`-style routes outside an
  exact-import `@Controller` class, block with `tsjs_nest_controller_identity`
  (recovery `nestjs_di_model`); Hono routes on an untraced receiver block with
  `tsjs_hono_receiver` (recovery `hono_receiver_model`). NestJS DI/token
  resolution (`tsjs_nest_di_resolution`), dynamic modules
  (`tsjs_nest_dynamic_module`), and Zod runtime refinement
  (`tsjs_zod_runtime_refinement`) are non-blocking subclaims that never block
  families. Zod schemas without an exact `zod`/`zod/v4` import are simply not
  anchored. Mocha and `node:test` runners carry a required-equal `runner_kind`
  so their families never merge with jest/vitest. `nestjs_di_model` and
  `hono_receiver_model` are registered required-mechanism tokens.
- Java/Spring preview UNKNOWNs are claim-scoped: unresolved Spring annotation
  imports/FQNs and unsupported controller/repository identity can block Java
  family support, while nonliteral route paths, component scan, DI/proxy,
  classpath, Maven/Gradle, javac, annotation-processor, and generated
  repository behavior remain explicit subclaims or non-supporting context.
- Java framework-deepening UNKNOWNs (Wave J1) stay claim-scoped: test/JPA-entity/
  JAX-RS annotation lookalikes (`java_test_annotation_binding`,
  `java_jpa_entity_identity`, `java_jaxrs_resource_identity`), a JAX-RS verb
  outside a `@Path` class, and mixed JUnit 4/5 `@Test` bindings
  (`ConflictingFacts`) block family support. Exact imported/FQN JUnit
  `@MethodSource` direct-repeatable scalar/array literal sets (including the
  blank/omitted same-name convention) may discharge `java_test_method_source`
  only when every entry resolves to one static method in the same class-like
  body; exact TestNG literal data-provider names may
  discharge `java_testng_data_provider` only when one exact `@DataProvider` is
  visible there. Both replacements are structural and never family support.
  Link identity requires an FQN or one unambiguous explicit import and rejects
  wildcard/colliding imports, local shadows, nested annotations, and parse-open
  inventories. External/signature/provider-class, type-level/inherited,
  explicit-container/meta, `PER_CLASS` non-static, overloaded/duplicate,
  dynamic, partial-positive, unknown-identity, invalid non-parameterized,
  mixed-framework, missing-target, and nested-boundary
  links remain typed `FrameworkMagic` or `ConflictingFacts` subclaims. Mockito
  bytecode mocks (`java_mockito_runtime_mocks`), JPA runtime mapping
  (`java_jpa_runtime_mapping`), Spring Data derived queries
  (`java_spring_data_query_derivation`), and Lombok generated members
  (`java_generated_members`, `MacroOrPreprocessor`) remain non-blocking subclaims.
  jakarta and javax entities share identical targets but never cluster together.
  The blocking-claim and copied-assumption policy tables are authoritative in
  `adapters/frameworks/java.rs`; new mechanisms `java_test_annotation_model`,
  `jpa_entity_model`, `jaxrs_resource_model`, `java_annotation_processor_boundary`,
  and `java_mockito_runtime_mock_model` are registered in the telemetry vocabulary.
- C# preview UNKNOWNs (Wave CS1) are claim-scoped: lookalike attributes without
  exact lexical-scope usings (`csharp_attribute_binding`), route attributes outside a
  controller (`csharp_controller_identity`), unresolvable minimal-API receivers
  (`csharp_minimal_api_receiver`), MSTest methods without a `[TestClass]`
  (`csharp_test_class_identity`), and `#if` build variants
  (`csharp_build_variant`) can block C# family support, while runtime DI, the
  filter pipeline, nonliteral route templates, convention routing, partial-class
  and source-generator boundaries and `dynamic` binding remain non-blocking
  subclaims. xUnit `MemberData` remains a non-blocking subclaim except when a
  direct identifier string resolves to one unique unconditional `public static`
  field/property/zero-argument method in the same closed non-generic class; that
  bounded link is recorded structurally without claiming row compatibility.
  Attribute binding uses Tree-sitter lexical-scope usings, never comment text or
  a sibling namespace's directives.
  FamilyUnknownDomain::CSharp is recognized only
  on an exact `repogrammar-csharp-syntax` /
  `tree_sitter_csharp_structural_anchors_v1` origin, and the C# mechanism
  vocabulary is `csharp_project_model`, `csharp_di_model`,
  `csharp_build_variant_model`, `csharp_source_generator_boundary`, and
  `aspnet_route_literal_model`. No MSBuild/Roslyn/source-generator/ASP.NET Core
  runtime or preprocessor evaluation is performed.
- C/C++ preview UNKNOWNs (Wave C1) are claim-scoped: registration macros without
  include corroboration or with Catch2-vs-doctest ambiguity
  (`cpp_test_framework_identity` via `UnresolvedImport`/`ConflictingFacts`),
  exact `testing::Test` fixture-base shapes without that same include evidence
  (`cpp_test_framework_identity` via `UnresolvedImport`), unsupported bounded
  macro arity/argument shapes (including underscore-bearing official GoogleTest
  names, malformed Catch2 tag lists, and wrong-signature/template-only Boost
  decorators), and orphan/corrupted/unclosed Boost.Test suite
  state (`cpp_test_framework_identity` via `MacroOrPreprocessor`),
  `#if`/`#ifdef` build variants overlapping a unit (`cpp_build_variant`), and
  ERROR-node parse-degraded macro regions (`cpp_macro_boundary`) can block C/C++
  family support, while Qt `Q_OBJECT`/moc output (`cpp_generated_code`), string
  SIGNAL/SLOT dispatch (`cpp_signal_slot_string_dispatch`), function-pointer
  dispatch (`cpp_indirect_dispatch`), and `compile_commands.json`/manifest gaps
  (`cpp_project_config`) remain non-blocking subclaims or non-supporting context.
  Framework includes corroborate identity only when an exact normalized header
  path comes from an unconditional Tree-sitter `preproc_include`; comments,
  strings, pseudo directives, conditional branches, and computed includes do
  not count. Standard include guards are excluded only when the guard is the
  sole non-comment top-level construct, has no alternative, and begins with a
  matching empty object-like `#define`; partial/value-defining guards, nested
  conditions, and complex or unclosed conditions remain build variants.
  Root Boost cases use the implicit master suite; explicit suite markers are
  validated by a linear source-order stack without treating suite names as case
  family evidence. Doctest decorator expressions, namespace-aliased or chained
  Boost decorators, and unsupported/version-specific signatures intentionally
  remain `UNKNOWN`.
  FamilyUnknownDomain::Cpp is recognized only for unit
  languages `c` or `cpp` on an exact `repogrammar-cpp-syntax` /
  `tree_sitter_c_cpp_structural_anchors_v1` origin, and the C/C++ mechanism
  vocabulary is `cpp_build_variant_model`, `cpp_macro_boundary`,
  `cpp_test_framework_model`, and `cpp_compile_commands_model`. No build,
  compiler, preprocessor, macro-expansion, or moc/protoc execution is performed.
- Rust general framework UNKNOWNs (Wave E1) are claim-scoped: serde/thiserror/
  clap derive tokens without same-file `use`-path evidence
  (`rust_framework_attribute_binding` → `rust_module_graph`) and non-literal,
  unresolved-helper, or untraceable-`Router::new()` axum routes
  (`rust_axum_route_identity` → `axum_route_model`) block only their unit, as
  does `#[cfg]` (`rust_build_variant`). Derive/attribute macro expansion
  (`rust_derive_expansion` → `rust_macro_boundary`) and axum tower middleware and
  handler extractor semantics (`rust_axum_middleware_semantics`,
  `rust_axum_extractor_semantics` → `axum_route_model`) stay non-blocking honesty
  subclaims on the new framework units. FamilyUnknownDomain::Rust is recognized
  only on a `repogrammar-rust-syntax` origin; the general-framework compatibility
  and support-family tables live in
  `src/rust/adapters/frameworks/rust_general.rs` and reuse the existing
  `repogrammar-rust-derived` safe origin, leaving the self-dogfood policy in
  `core/policy/rust_self_dogfood.rs` untouched. No derive/attribute macro
  expansion, trait resolution, or points-to analysis is performed.
- The ADR-0019 wave E1 Python preview (Django/Flask/unittest/click/typer/Celery)
  reuses the FamilyUnknownDomain::Python domain and the CPython `python` /
  `cpython_ast` parser origin. Blocking claims are `python_django_model_identity`,
  `python_django_url_identity`, `python_flask_route_identity`,
  `python_cli_command_identity`, and `python_celery_task_identity`; non-blocking
  subclaims are `python_django_settings_behavior` (mechanism
  `django_settings_model`), `python_django_string_dispatch`,
  `python_unittest_patch_target` (`MonkeyPatch`), and
  `python_celery_runtime_routing`. New mechanism vocabulary is
  `django_project_model`, `django_settings_model`, and `flask_app_model`; cli and
  celery identity reuse `python_import_graph`. Framework identity requires an
  exact framework import binding; settings.py, URL reversing, middleware order,
  and task-queue routing are never evaluated.
- A fresh supported semantic fact kind is only eligible input for future claim
  builders. It is not a pattern-family classification or conformance result.
- Tests for new analyzers should include uncertain, conflicting, stale,
  unsupported, and dynamic cases.
- Python dynamic Pydantic model factories such as `pydantic.create_model(...)`
  remain typed `FrameworkMagic` `UNKNOWN` for framework identity unless a later
  provider-backed design proves a narrower claim.
- Python Pydantic runtime validator body calls remain typed `FrameworkMagic`
  `UNKNOWN` for `pydantic_validator_side_effects`; this is a non-blocking
  subclaim and does not by itself disprove model identity.
- Python Pydantic/SQLAlchemy-shaped classes with imported external bases remain
  typed `FrameworkMagic` `UNKNOWN` for framework identity unless the base
  resolves to an exact supported framework base.
- Python SQLAlchemy custom query wrapper calls remain typed `FrameworkMagic`
  `UNKNOWN` for framework identity even when the wrapper body contains typed
  session receiver calls.
- Python unresolved bare decorators remain typed `FrameworkMagic` `UNKNOWN` for
  framework identity. Locally defined decorators and native `property`,
  `classmethod`, and `staticmethod` remain structural metadata.
- Python exact-anchor support derivation is sound-by-abstention for bounded
  framework-family claims: claim-relevant parser-origin blocking `UNKNOWN`s
  such as `DynamicImport`/`UnresolvedImport` for `python_import_resolution`,
  `FrameworkMagic`/`MonkeyPatch` for call or framework identity, and
  `PytestFixtureInjection`/`ConflictingFacts` for `pytest_fixture_binding`
  prevent the affected unit from contributing family support. FastAPI
  `fastapi_dependency_target` UNKNOWNs remain scoped to that subclaim and do
  not block route-family support.
- Supported Python family members preserve non-blocking subclaim `UNKNOWN`s in
  family detail/query metadata with concrete affected claims such as
  `<family_id>:fastapi_dependency_target`; preserving the UNKNOWN does not make
  the subclaim true.
- The family builder repeats the same claim-scoped blocking check after support
  derivation. Parser-origin context facts can split complete-link Python
  clusters or create metadata-only variation slots, but parser-origin blocking
  `UNKNOWN`s remove the affected unit from confident family support unless the
  UNKNOWN is scoped to a non-membership subclaim. pytest non-builtin fixture
  context remains a compatibility constraint rather than a free variation.
- Family-affecting `UNKNOWN` consumption now routes through one classifier for
  blocking, non-blocking, query-visible family effects, and compatibility
  feature blockers. Callers filter or serialize the classifier result instead
  of rechecking origin/provenance and claim-impact rules from raw fact fields.
  Parser-origin structural support derivation for Python, TS/JS, Java, C#,
  C/C++, and Rust also routes its unit-language/parser-fact/exact-role tuple
  through that classifier before minting derived support; do not restore
  language-local shadow reason/claim tables in `indexing.rs`. Exact
  provider-resolved promotion keeps its operation-specific proof contract.
- Keep public `UnknownClass` tokens and counters as the legacy serialization
  projection. Internal family/query policy uses orthogonal `ClaimImpact` and
  `ResolutionClass`: only claim impact may suppress support, and only resolution
  plus a registered mechanism may choose recovery readiness/code. Unknown or
  unregistered mechanisms default irreducible. Preserve the public
  `ClaimUnknown.pub class` field and struct-literal API; internal family-impact
  conversion accepts only blocking/non-blocking and rejects
  recoverable/irreducible. Do not classify an entire call,
  macro, or cfg claim family from its name: exact assumptions distinguish
  data-dependent runtime/proxy/reflection/binder behavior and Rust proc-macro or
  build-script execution from statically recoverable Python call resolution,
  declarative Rust macros, fixed-command C/C++ preprocessing, and cfg selection.
- Python provider agreement, provider disagreement, and runtime observation are
  not current certainty tokens. Until the Rust domain, protocol, storage, CLI,
  MCP, and schemas change together, cross-check and observed-runtime details
  remain provenance/assumptions and conflicts still surface as
  `ConflictingFacts` or typed `UNKNOWN`.
- `unknowns --json` and `stats --unknowns --json` may expose readiness-scoped
  `by_language_detail` summaries, and `stats --json` may expose readiness
  `by_language` buckets, but those outputs must stay source-free and avoid
  paths, code-unit ids, fact ids, snippets, repository names, and free-text
  recovery guidance.

## Revalidation conditions

Update after classification, compatibility, freshness, semantic-worker,
UNKNOWN-token, or family-mining behavior changes.
