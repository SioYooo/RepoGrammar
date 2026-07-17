# UNKNOWN Governance Specification

RepoGrammar treats `UNKNOWN` as a first-class typed analysis output. It is not a
panic, not an empty result, and not something to guess away.

## Purpose

The purpose of `UNKNOWN` is to prevent weak, stale, conflicting, unsupported, or
dynamic evidence from becoming false certainty. A family classification may
still be useful when some supporting facts are unknown, but the unknown facts
must remain visible and must block only the claims they actually affect.

## Unknown Classes

- `blocking_unknown`: prevents a specific claim or classification from being
  emitted because required evidence is unavailable or contradictory.
- `non_blocking_unknown`: records missing or weak evidence that does not affect
  the emitted claim under the current query.
- `recoverable_unknown`: may be resolved by additional evidence such as a
  semantic worker, user hint, optional provider, project config, dependency
  install, or bounded runtime trace.
- `irreducible_unknown`: cannot be resolved safely by static analysis in the
  current configuration, usually because behavior is runtime-defined or
  intentionally dynamic.

These four public tokens are a legacy protocol projection and remain unchanged
in CLI, MCP, storage, metrics, and JSON. Internally, policy keeps two orthogonal
axes: `ClaimImpact` (`Blocking` or `NonBlocking`) decides whether an affected
claim may proceed, while `ResolutionClass` (`Recoverable` or `Irreducible`)
decides whether registered analysis or recovery can discharge the UNKNOWN.
Family-effect serialization projects `ClaimImpact` back to
`blocking_unknown`/`non_blocking_unknown`; provider-supplied and legacy default
classes retain their existing public projection. Consequently the public four
class counters remain legacy counters, not an internal two-axis cross-tab.
The public Rust `ClaimUnknown` shape likewise retains its
`pub class: UnknownClass` field for field access and struct-literal
compatibility. Internal family policy converts that field to `ClaimImpact`
only for `Blocking` and `NonBlocking`; `Recoverable` and `Irreducible` are
rejected by that conversion and must never become family block decisions.

## Reason Codes

Reason codes are stable protocol-facing values. They may apply to Python v0.1,
transitional TypeScript/JavaScript substrate, future C/C++, provider facts, or
storage freshness:

- `DynamicImport`
- `MonkeyPatch`
- `PytestFixtureInjection`
- `RuntimeDependencyInjection`
- `UnresolvedImport`
- `MissingProjectConfig`
- `MissingDependency`
- `FrameworkMagic`
- `MacroOrPreprocessor`
- `BuildVariantAmbiguity`
- `ConflictingFacts`
- `StaleEvidence`
- `InsufficientSupport`

Future reason codes must be documented here before they become public CLI, MCP,
storage, or metrics output.

## Claim Rules

Structural evidence may generate candidates but cannot independently prove
semantic family membership. Compiler-native semantic facts, framework roles,
CFG/dataflow/effect summaries, API usage, and repository context may strengthen
claims only when their provenance and freshness checks pass.

A blocking `UNKNOWN` may suppress a family only when it originates from that
language's own authoritative anchor engine. An `UNKNOWN` fact from any other
(for example, provider or stored) engine that merely falls within a unit must
not clear the unit's support, so a foreign-provenance `UNKNOWN` cannot block an
otherwise-valid family. This anchor-engine gate applies uniformly to the
Python, TS/JS, Rust, and Java per-unit blocking detectors, public
family-effect classification, and compatibility features that suppress
candidate clusters (the same gate governs the C# and C/C++ preview per-unit
detectors). A
stored family with no evidence rows also cannot be proven
fresh; the freshness check abstains rather than serving an evidence-less row as
a confident match.

The `families` listing applies the same evidence-freshness discipline across the
whole active generation, but reports it as a per-family verdict rather than
hiding the family. It reads one bounded projection of `(family_id, path,
content_hash)` for the active generation, hash-verifies each distinct evidence
path at most once, and assigns each family one of three states: `fresh` (every
evidence path verified with a matching hash), `stale` (at least one evidence
path is missing or its content hash changed), or `cannot_verify` (no stale path,
but at least one path failed verification for a non-content reason — too large,
non-UTF-8, or unavailable). A family with zero evidence rows abstains as
`cannot_verify`, matching the single-family evidence-less rule above. Stale takes
precedence over `cannot_verify`, which takes precedence over `fresh`. Stale and
`cannot_verify` families stay listed with their verdict and are counted in the
report's `fresh_count`/`stale_count`/`cannot_verify_count`; a stale listing also
carries one low-cardinality report-level `StaleEvidence` unknown recovering via
`run repogrammar resync`, and is never collapsed into a whole-listing `UNKNOWN`.

Implementation paths that consume family-affecting `UNKNOWN`s must use one
authoritative classifier for blocking, non-blocking, public family-effect, and
compatibility-feature decisions. Callers may filter that classifier's result by
class or serialize it for query output, but they must not rederive provenance or
claim-impact rules from raw fact fields. Parser-origin structural support
derivation for Python, TS/JS, Java, C#, C/C++, and Rust must pass the unit
language, parser fact, and exact single framework role through that classifier
before minting derived support; language-specific derivation code must not keep
shadow reason/claim tables. Exact provider-resolved promotion remains governed
by its provider proof and operation-specific resolution contract.

Inventory and query policy must enter through one internal disposition
classifier. `blocks_support` and family suppression read only `ClaimImpact`;
recovery codes read only `ResolutionClass` plus the registered mechanism.
Callers must not infer either axis from free-text recovery guidance or from the
public `UnknownClass` token. A missing/unregistered mechanism defaults to
`Irreducible`, never to an invented provider capability. Exact assumptions may
refine that result conservatively: data-dependent import/eval/exec/reflection,
Spring proxy/Mockito runtime generation, C# runtime binding, Rust procedural
macro expansion, and Cargo build-script output are execution/runtime boundaries.
Ordinary Rust declarative macros, C/C++ preprocessing under a fixed compile
command, and Rust target/feature/cfg selection remain recoverable through their
registered analyzer or variability mechanisms; the policy must not mark the
whole macro, call-target, or build-variant claim family irreducible by name.

The Rust domain currently exposes stable typed `UNKNOWN` classes and reason
codes and uses them in the internal semantic-fact claim-input readiness gate.
Fresh supported fact kinds with `SEMANTIC` or `DATAFLOW_DERIVED` certainty may
be eligible inputs for future claim builders, but that eligibility is not a
pattern-family claim. Stale or missing source evidence blocks the affected
semantic-fact claim input with `StaleEvidence`; `UNKNOWN` fact kind plus
`STRUCTURAL`, `FRAMEWORK_HEURISTIC`, and `UNKNOWN` certainty block with
`InsufficientSupport`; `CONFLICTING` certainty blocks with `ConflictingFacts`.

If views disagree, RepoGrammar must preserve the conflict as
`ConflictingFacts`, produce `CONFLICTING` internally where appropriate, and
abstain or emit `UNKNOWN` for affected claims. Do not average or vote away
conflicts when the losing fact would change behavior, security, persistence,
authorization, transactionality, idempotency, async lifecycle, error mapping, or
external side effects.

Future provider agreement or runtime observation states, such as Python
cross-checks or RightTyper-style observed traces, must not become public
certainty tokens until the Rust domain, protocol schemas, storage, CLI, MCP,
and tests define those tokens together. Until then, agreement and observation
details remain provenance or assumptions, while disagreements still surface as
`ConflictingFacts` or typed `UNKNOWN`.

Some unknowns block only specific claims:

- unresolved auth middleware may block a conformance claim about authorization
  while still allowing a syntax-only route-shape classification;
- a stale dependency graph may block call-context evidence while preserving
  source-range evidence from the current file hash;
- Python fixture injection may block test-behavior equivalence
  while still allowing structural pytest test discovery;
- dynamic Python Pydantic model factories may block static model-family
  identity without blocking unrelated model classes in the same file;
- dynamic or unsafe Python pytest fixture `name=` aliases may block fixture
  binding without changing the structural fact that the function is decorated
  as a pytest fixture;
- duplicate applicable Python `conftest.py` fixture names may block fixture
  binding with `ConflictingFacts`, while known pytest built-in fixtures may
  remain non-supporting external context;
- dynamic FastAPI dependency target expressions may block the dependency-target
  sub-claim while still allowing a route family when route membership has enough
  independent exact-anchor support;
- dynamic TS/JS route methods, unsafe or unresolved Express/Fastify/Hono
  receivers, unsafe or unresolved Jest/Vitest/Mocha/`node:test` runner bindings,
  unsupported Next.js route-convention magic, Prisma injected/raw/bulk/dynamic
  clients, Drizzle unresolved/raw/dynamic builders, NestJS decorators not bound
  to `@nestjs/common` or routes outside an exact controller, and Hono routes on
  an untraced receiver may block the affected `tsjs_receiver_binding`,
  `tsjs_runner_binding`, `tsjs_support_target`, `tsjs_nest_controller_identity`,
  `tsjs_hono_receiver`, or adapter-specific claim while other exact-anchor units
  in the same repository can still form a family when they have enough
  independent compatible support.
- NestJS DI/token resolution (`tsjs_nest_di_resolution`,
  `RuntimeDependencyInjection`), NestJS dynamic modules
  (`tsjs_nest_dynamic_module`, `FrameworkMagic`), and Zod runtime
  refinement/transform (`tsjs_zod_runtime_refinement`, `FrameworkMagic`) are
  non-blocking subclaims recorded on accepted anchors; they never block family
  membership and are recovered through `nestjs_di_model` or existing framework
  buckets, never guessed away. Zod lookalikes without an exact `zod` import and
  Nest `@Controller`-lookalikes are simply not anchored (no family, no false
  certainty).
- TS/JS dynamic imports, conditional or non-literal `require`, unresolved
  repo-local imports, unresolved or conflicting path aliases, unresolved or
  conflicting rootDirs candidates, ambiguous star re-exports, and missing
  ambient test-runner project context must remain typed `UNKNOWN`. These
  unknowns may be blocking only when they affect framework identity,
  runner/receiver binding, support target, or another emitted family claim;
  otherwise they remain context/read-plan guard evidence and must not be guessed
  away.
- The TS/JS parser maps granular v0.2 cases onto the stable reason-code set:
  dynamic `import(...)` is `DynamicImport`; non-literal or conditional
  `require`, dynamic route/test calls, Next server-client/middleware/server
  action/re-export magic, Fastify dynamic route options or full routes without
  literal `url`/`path` or handler fields, Prisma callback, raw, bulk, or
  dynamic operations, and Drizzle raw/dynamic builders are
  `FrameworkMagic` or `BuildVariantAmbiguity`; exact local Next dynamic
  segments, route groups, and parallel routes are stored as context assumptions
  on accepted anchors rather than UNKNOWNs by themselves; unresolved relative
  imports, unresolved path aliases, unresolved rootDirs candidates,
  unresolved Express/Fastify/Hono receivers, unresolved Prisma clients,
  unresolved Drizzle db/table bindings, NestJS decorators not imported from
  `@nestjs/common` (`nest_unresolved_controller_import`,
  `nest_unresolved_route_import`), NestJS routes outside an exact controller
  (`nest_route_outside_controller`), and missing ambient runner or Next package
  context are `UnresolvedImport`, `FrameworkMagic`, or `MissingProjectConfig` as
  applicable; reassigned or
  shadowed receivers, unsafe test runner bindings, conflicting path aliases,
  conflicting rootDirs candidates, and ambiguous star re-exports are
  `ConflictingFacts`. These mappings are
  intentionally conservative and do not create new public reason codes for every
  syntax shape.
- Python project-config parsing also uses `ConflictingFacts` when a static
  `setup.py` contains multiple independently authoritative module-body setup
  calls; their project names and source roots are never merged.
- A unique recognized `setuptools` setup call uses `MissingProjectConfig` when
  a relevant field is dynamic, partial, duplicate, unpacked, overridable, or
  unreachable after a definite top-level `raise`. No roots are emitted from the
  incomplete field. A complete empty `setup()` call is not an `UNKNOWN`.
- Rust self-dogfood maps unresolved external modules and complex repo-local
  `use crate::...` / `use super::...` / `use self::...` paths to
  `UnresolvedImport`, `#[cfg]` / `#[cfg_attr]`, target-specific Cargo sections,
  and Cargo build scripts to `BuildVariantAmbiguity`, macro/proc-macro syntax
  to `MacroOrPreprocessor`, trait-object dispatch to `FrameworkMagic`, and
  stale Rust source evidence to `StaleEvidence`. External dependency `use`
  paths are not treated as repo-local module proof; checked-in `Cargo.toml`
  dependency metadata remains structural context. Source-level cfg UNKNOWNs may
  carry bounded nearest-`Cargo.toml` feature-predicate assumptions, including
  whether a simple feature predicate is declared, but they still do not evaluate
  the selected cfg/profile/target. Query/family recovery guidance may summarize
  that declared/undeclared feature state, but the result remains a blocking
  `UNKNOWN` until the cfg is resolved.
- Rust UNKNOWNs block only the affected claim. `rust_build_variant`,
  `rust_macro_expansion`, `rust_trait_dispatch`, `rust_module_resolution`, and
  `rust_family_membership` block the relevant internal RepoGrammar family
  claim; a root `Cargo.toml` build-script or target-specific
  `rust_build_variant` UNKNOWN blocks Rust self-dogfood family emission for the
  repository until the build variant is resolved. Nested, fixture, or package
  manifests must not globally block unrelated root Rust family claims; they
  remain scoped to the affected package/crate/claim until provider-backed
  package scope is implemented. Unrelated optional call-shape details may
  remain non-blocking metadata. Rust UNKNOWNs must not be guessed away by
  naming convention or path similarity.
- Rust general framework preview analysis (serde/thiserror/tokio/clap derive and
  attribute shapes plus axum literal `Router::new().route(...)` segments) is
  gated by same-file `use`-path evidence or an inline fully-qualified path. It
  maps framework derive tokens (`Serialize`/`Deserialize`, `Error`,
  `Parser`/`Subcommand`/`Args`) without that evidence to `UnresolvedImport`
  blocking `rust_framework_attribute_binding`, and a non-literal axum route path,
  an unresolved route helper, or a receiver that does not trace to
  `Router::new()` to `UnresolvedImport` blocking `rust_axum_route_identity`. The
  derive/attribute macro *expansion* stays a non-blocking `MacroOrPreprocessor`
  subclaim `rust_derive_expansion` on the new framework units (expansion
  honesty), and axum tower middleware ordering and handler extractor trait
  resolution stay non-blocking `FrameworkMagic` subclaims
  `rust_axum_middleware_semantics` and `rust_axum_extractor_semantics` on route
  units. A `#[cfg]` on a general framework unit keeps its existing
  `BuildVariantAmbiguity` blocking behavior. RepoGrammar never expands
  derive/attribute macros, resolves traits, or performs points-to analysis; the
  written attribute shapes plus use-path evidence are the only anchors.
- Java/Spring preview analysis maps simple Spring-lookalike annotations without
  exact imports/FQNs and unsupported repository/controller identity to
  `UnresolvedImport` or `FrameworkMagic`. Those unknowns block only affected
  claims such as `java_spring_annotation_binding`,
  `java_spring_controller_identity`, `java_spring_framework_identity`,
  `java_spring_repository_identity`, or `java_family_membership`. Nonliteral
  Spring MVC route paths, Spring Boot component scans, dependency injection,
  AOP/proxy behavior, and generated Spring Data implementations remain explicit
  non-blocking `UNKNOWN` subclaims when exact anchor identity is otherwise
  supported: `java_spring_route_path`, `java_spring_component_scan`,
  `java_spring_dependency_injection`, `java_spring_proxy_semantics`, and
  `java_spring_generated_repository`. Classpath or Maven/Gradle-sensitive facts
  remain unsupported context unless a later project/module-level representation
  is introduced. These subclaims must not be used as proof of runtime
  equivalence. Java/Spring support must not be guessed from annotation simple
  names, custom composed annotations, directory names, dependency filenames, or
  fact text substrings.
- Java framework deepening (JUnit 5/4, TestNG, Mockito, JPA/Jakarta Persistence,
  JAX-RS/Jakarta REST, Lombok, Spring Data derived queries) reuses the same
  exact-import/FQN gate. Test, JPA-entity, and JAX-RS annotation simple names
  without an exact import/FQN map to `UnresolvedImport` blocking
  `java_test_annotation_binding`, `java_jpa_entity_identity`, or
  `java_jaxrs_resource_identity`; a JAX-RS verb annotation outside an exact
  `@Path` class maps to `FrameworkMagic` blocking `java_jaxrs_resource_identity`;
  an `@Test` that resolves to both JUnit 4 and JUnit 5 maps to `ConflictingFacts`
  blocking `java_test_annotation_binding`. A complete, direct repeatable JUnit
  `@MethodSource` literal set (scalar or array, including blank/omitted same-name
  convention) whose entries each identify one source-visible static method, or
  a same-class TestNG literal `dataProvider` identifying one source-visible
  exact `@DataProvider`, may replace the corresponding link `UNKNOWN` with a
  separate structural fact. Exact link identity rejects wildcard/colliding
  imports, local type shadows, malformed imports, nested annotation values, and
  parse-open inventories. External/signature-selected class/provider
  references, type-level or inherited sources, explicit containers or
  meta-annotations, JUnit `PER_CLASS` non-static factories,
  duplicate/overloaded names, dynamic names, use without `@ParameterizedTest`,
  missing targets, partial-positive sets, mixed framework/source identity, and
  nested-boundary crossings remain non-blocking
  `FrameworkMagic` or `ConflictingFacts` subclaims
  (`java_test_method_source` / `java_testng_data_provider`). These structural
  replacements do not prove test-family membership or runtime invocation.
  Other non-blocking subclaims cover Mockito bytecode-generated mocks
  (`java_mockito_runtime_mocks`), JPA lazy proxies/naming strategies/orm.xml
  (`java_jpa_runtime_mapping`), Spring Data derived-query property paths
  (`java_spring_data_query_derivation`), and Lombok generated members
  (`java_generated_members`, mapped to `MacroOrPreprocessor` per ADR-0019 D4).
  jakarta and javax entities share identical targets but never cluster together
  (a `jpa_namespace_root` assumption keeps them apart). Annotation-processing and
  generated members are never simulated; `testng.xml` and `orm.xml` are not
  parsed; derived-query property-path validity is not claimed.
- C# (ASP.NET Core / EF Core / xUnit / NUnit / MSTest) preview analysis maps
  attribute short names without an exact Tree-sitter lexical-scope `using` or
  inline FQN to
  `UnresolvedImport`, HTTP route attributes outside an exact controller and
  MSTest methods outside a `[TestClass]` to `FrameworkMagic`, an unresolvable
  minimal-API `Map*` receiver to `UnresolvedImport`, and `#if`/`#endif`
  conditional regions overlapping a unit to `BuildVariantAmbiguity`. Those
  unknowns block only affected claims `csharp_attribute_binding`,
  `csharp_controller_identity`, `csharp_minimal_api_receiver`,
  `csharp_test_class_identity`, `csharp_build_variant`, or
  `csharp_family_membership`. Runtime dependency injection, the MVC filter
  pipeline, nonliteral route templates, convention routing (`MapControllerRoute`
  / `MapHub` / `MapGrpcService`), partial-class and source-generator boundaries,
  and `dynamic` binding remain explicit non-blocking `UNKNOWN` subclaims when
  exact anchor identity is otherwise supported:
  `csharp_di_registration`, `csharp_aspnet_filter_pipeline`,
  `csharp_aspnet_route_template`, `csharp_aspnet_convention_routing`,
  `csharp_partial_external`, `csharp_generated_source`, `csharp_dynamic_binding`,
  and `csharp_test_member_data`. The `csharp_test_member_data` link UNKNOWN is
  discharged only when every exact xUnit `MemberData` attribute names, by one
  plain identifier string, a unique unconditional `public static` field,
  property, or zero-argument method in the same closed non-generic class. An
  exact link adds `xunit_member_data_link=exact_same_class_public_static` and a
  deterministic source-kind inventory to the structural anchor; it does not
  prove provider return rows, parameter compatibility, discovery enumeration,
  or runtime equivalence. Explicit `MemberType`, additional attribute
  properties, source arguments, dynamic
  names, partial/inherited/generic/non-class scope, overloads, missing or
  ineligible members, conditional compilation, and unresolved attribute
  identity remain `csharp_test_member_data` `UNKNOWN`; parse-degraded
  attributes, providers, and class scopes never discharge it. RepoGrammar never
  executes MSBuild, Roslyn,
  source generators, or the ASP.NET Core runtime, and never evaluates
  preprocessor conditions; generated, MSBuild-conditional, and Razor facts remain
  unsupported context. C# support must not be guessed from attribute simple
  names, base-type names, directory names, or fact text substrings.
- C/C++ (GoogleTest / Catch2 / doctest / Boost.Test) preview analysis maps
  registration-macro shapes without a corroborating unconditional, exact-path
  Tree-sitter `preproc_include` to
  `UnresolvedImport`, a `TEST_CASE` whose include evidence matches both Catch2
  and doctest to `ConflictingFacts`, `#if`/`#ifdef`/`#ifndef` conditional regions
  overlapping a unit to `BuildVariantAmbiguity`, and Tree-sitter ERROR-node
  parse-degraded macro regions to `MacroOrPreprocessor`. Exact
  `::testing::Test`/`testing::Test` base syntax without the same unconditional
  gtest/gmock include evidence is also `UnresolvedImport`, not a fixture anchor.
  A recognized registration macro outside the audited bounded arity/argument
  shape, a Boost.Test orphan/invalid suite end, a registration after corrupted
  suite state, or an EOF-unclosed suite is `MacroOrPreprocessor` for
  `cpp_test_framework_identity`. Root-level Boost cases remain valid because
  Boost.Test provides an implicit master suite; exact nested suite markers are
  tracked with a source-ordered explicit stack. Those unknowns block
  only affected claims `cpp_test_framework_identity`, `cpp_build_variant`,
  `cpp_macro_boundary`, or `cpp_family_membership`. Commented, string-contained,
  pseudo-, computed, non-normalized, or conditional-branch includes never
  corroborate framework identity. Standard include guards (`#ifndef X` or
  `#if !defined(X)`) avoid `cpp_build_variant` only when the conditional is the
  sole non-comment top-level construct, has no alternative branch, and its
  first non-comment body directive is the empty object-like `#define X`.
  Partial guards, value-defining `#define X ...`, nested conditions, and complex
  or unclosed conditions remain build variants. `#pragma once` is not a
  conditional. Qt `Q_OBJECT`/moc
  output (`cpp_generated_code`), string-based SIGNAL/SLOT connect
  (`cpp_signal_slot_string_dispatch`), function-pointer or virtual dispatch
  (`cpp_indirect_dispatch`), and absent or unreadable
  `compile_commands.json`/`vcpkg.json`/`conanfile.txt` project configuration
  (`cpp_project_config`) remain explicit non-blocking `UNKNOWN` subclaims or
  non-supporting context. RepoGrammar never runs a build, compiler, preprocessor,
  or moc/protoc, and never expands macros; generated and build-variant facts
  remain unsupported context. The accepted contract snapshot is intentionally
  narrower than all framework versions: underscore-bearing `TEST`/`TEST_F`/
  `TEST_P` names, doctest decorator expressions, namespace-aliased,
  multi-expression, template-only, or wrong-signature Boost decorators, and
  nonliteral, free-form, unbalanced, empty, separated, trailing-garbage,
  escaped/raw/prefixed Catch2 tag strings remain `UNKNOWN`. C/C++ support must
  not be guessed from macro names, base-type names, directory names, or fact
  text substrings.
- Go is `discovered_only` and remains unsupported after ADR-0021 plus the
  source-free discovery/config module. Inventory emits no Go semantic facts or
  `UNKNOWN` reason codes; the unsupported-parser notice is a bounded path-free
  warning per language token, not a claim fact. A future
  Go test-function slice must route `go_file_selection`,
  `go_test_declaration_identity`, and `go_generated_origin` through the same
  authoritative family-`UNKNOWN` classifier used by other languages; those
  obligations block only the affected `go.testing.test_function` claim.
  Package buildability, module/workspace/vendor context, external types,
  dispatch, cgo, and `go:generate` are separate subclaims and are non-blocking
  for the narrow source-local declaration unless that classifier records a
  direct impact. Foreign-provenance `UNKNOWN`s never block a Go family. Until
  the semantic/family modules land, these names are normative future obligations rather
  than implemented reason codes, facts, families, or public support.
- PHP is `discovered_only` and remains unsupported after ADR-0024. Discovery
  persists only bounded file metadata for `php` and `php-config` tokens; it
  emits no PHP semantic fact, typed `UNKNOWN`, code unit, IR, or family, and its
  inventory-only warning is not semantic evidence. The ADR's frontend-
  availability, resource/protocol,
  syntax/profile, project/lock/PHPUnit-version, namespace/ancestry/attribute-
  identity-and-shape, provider/dependency, trait-origin, dynamic-include/
  autoload, runtime-mutation, generated-source, and suite-selection mechanisms
  have explicit initial claim impacts and resolution evidence. They are
  normative future obligations for the exact `php.phpunit.test_method` claim,
  not implemented reason codes. No isolated differential/oracle or syntax
  fallback result may silently discharge those obligations or form a PHP
  family.
- Swift is `discovered_only` and remains unsupported after ADR-0025. Discovery
  persists only bounded file metadata for `swift` and `swift-config` tokens; it
  emits no Swift semantic fact, typed `UNKNOWN`, code unit, IR, project record,
  or family, and its inventory-only warning is not semantic evidence. The ADR's
  artifact/frontend availability, syntax/profile degradation, package root and
  target selection, toolchain/SDK/XCTest identity, conditional compilation,
  attribute, macro/plugin, ancestry, protocol dispatch, generated-source, and
  resource/protocol mechanisms are normative future obligations for exact
  `swift.xctest.test_method`, not implemented reason codes. No SwiftSyntax,
  compiler, or SourceKit result may silently discharge those obligations or
  form a Swift family before the authoritative claim-impact registry lands.
- Ruby is `discovered_only` and remains unsupported after ADR-0022. Discovery
  persists only bounded file metadata for `ruby` and `ruby-config` tokens; it
  emits no Ruby facts, typed `UNKNOWN`s, code units, IR, or families, and its
  inventory-only warning is not semantic evidence. The future exact Minitest
  slice must route `ruby_parse_degraded`, `ruby_syntax_version`,
  `ruby_minitest_require_identity`, `ruby_constant_identity`,
  `ruby_minitest_test_definition`, `ruby_runtime_mutation`, and
  `ruby_generated_source` through one authoritative Ruby claim-impact
  classifier. The first syntax profile resolves only from a sole repository-
  root `.ruby-version` with exact `4.0.6` plus optional LF. Parse/profile,
  loading/constant identity, lexical require order, direct public test-definition,
  and mutation obligations block only affected `ruby.minitest.test_method`
  claims. `ruby_generated_source` blocks only when bounded positive evidence
  identifies generated origin or conflicting origin evidence; absence of a
  marker is neither provenance nor a blocker. Foreign-provenance `UNKNOWN`s
  never block Ruby support. These mechanisms must become typed `UNKNOWN` after
  the registry lands; until then they are unavailable and no Ruby semantic
  capability, reason codes, facts, families, or public support exists.
- Python bounded preview analysis (Django, Flask, stdlib unittest, click/typer,
  Celery; ADR-0019 wave E1) reuses the FastAPI/SQLAlchemy exact-import gate. A
  base or decorator receiver that matches a known framework simple name but does
  not resolve to the exact framework import binding stays `UNKNOWN` by
  abstention, and an imported-external model base with Django field members maps
  to `FrameworkMagic` blocking `python_django_model_identity` (the Pydantic /
  SQLAlchemy external-base precedent). A non-literal Django `urlpatterns`
  `path()` / `re_path()` first argument blocks `python_django_url_identity`, an
  unresolvable Flask/typer/Celery decorator receiver or non-literal Flask rule
  blocks `python_flask_route_identity` / `python_cli_command_identity` /
  `python_celery_task_identity`; all are `FrameworkMagic`. Settings-driven
  behavior, string URL dispatch / `include()`, `unittest.mock.patch` targets, and
  Celery `.delay()` / `.apply_async()` / `send_task()` runtime routing remain
  explicit non-blocking subclaims when exact anchor identity is otherwise
  supported: `python_django_settings_behavior`, `python_django_string_dispatch`
  (`FrameworkMagic`), `python_unittest_patch_target` (`MonkeyPatch`), and
  `python_celery_runtime_routing` (`FrameworkMagic`). RepoGrammar never evaluates
  `settings.py`, reverses URLs, models middleware order, or resolves task-queue
  routing; those remain unsupported context. These previews do not change the
  ADR-0011 v0.1 FastAPI/pytest/SQLAlchemy/Pydantic focus statement.
- Fuzzy path or path-suffix query targets that match evidence in more than one
  stored family must become a blocking `InsufficientSupport` `UNKNOWN` for
  `query target ambiguity`. The recovery guidance should name the candidate
  family ids and ask the caller to narrow the target to one exact family id or
  member id rather than silently treating one matching family as the whole-file
  answer. Query responses should also surface candidate family ids through
  structured route metadata so agents can use them as follow-up handles without
  parsing recovery text. These ids do not make an ambiguous or partial result a
  selected family claim.

When a family is emitted with a non-blocking unknown, the affected claim should
name the concrete family and claim whenever possible, such as
`<family_id>:runtime_equivalence`, so downstream agents do not treat the
unknown as repository-global. Current Python family detail/query output also
preserves supported-member non-blocking subclaim unknowns, such as
`<family_id>:fastapi_dependency_target`, through metadata-only family output;
the route family can remain confident while the dependency-target subclaim
stays `UNKNOWN`.

## Recovery Actions

Recovery suggestions may include:

- run or configure a language-native semantic worker;
- add missing project configuration;
- install or point to missing dependencies;
- enable an optional provider such as a future CodeGraph provider;
- provide explicit user hints;
- run an optional bounded runtime trace when that feature exists and is enabled;
- re-run `repogrammar sync` when evidence is stale.

Recovery actions are suggestions, not automatic mutation permission. They must
not create indexes, install dependencies, execute runtime traces, or initialize
provider state without an explicit command and documented consent boundary.

## CLI, MCP, Storage, and Metrics

Human and JSON CLI output must distinguish:

- missing-index fallback;
- stale-index refusal;
- not-yet-implemented query execution;
- deterministic local `PARTIAL_CONTEXT` that resolves a target but has no
  family evidence;
- typed `UNKNOWN` from a real analysis result.

`PARTIAL_CONTEXT` is not a weakened success state. It may provide a
metadata-only read plan for a fresh indexed path or code unit, but it must carry
typed `InsufficientSupport` for the missing pattern-family claim and must not
include family evidence, member evidence, conformance status, or source text
unless the normal explicit source-span opt-in succeeds. When the operation is
`check_conformance`, the partial result may include only advisory conformance
metadata with `UNKNOWN` status and must not include proof-like `pass`,
`conforms`, or `fail_on` fields.

When public family output is blocked by `StaleEvidence`,
`InsufficientSupport`, or another claim-relevant `UNKNOWN`, CLI and MCP output
must not present stale or unsupported read-plan spans as authoritative. Matched
fresh families may include read plans and, under explicit opt-in, bounded
source spans. Stale, missing, hash-mismatched, unsupported, dynamic,
insufficient, or conflicting evidence must omit source spans and keep the
recovery guidance explicit instead of implying that source ranges are safe to
trust.

MCP serialization must preserve unknown class, reason code, affected claim,
evidence attempted, freshness status, and suggested recovery action.

Storage records for families, facts, and evidence must retain enough provenance
to explain why a fact is unknown or why it was non-blocking for a claim.

Metrics may count persisted semantic unknowns by language, framework role,
framework-role state, adapter, reason, stage, required mechanism,
support-blocking status, and stable recovery code. Recovery buckets must use
low-cardinality codes such as `run_sync`, `add_project_config`,
`enable_provider`, `not_implemented_in_current_version`, `resolve_import_graph`,
`resolve_fixture_graph`, `resolve_dependency_metadata`,
`runtime_trace_required`, `manual_review_required`, or reserved `unknown`; they
must not use free-text recovery guidance, repository paths, code snippets,
code-unit ids, or fact ids. Provider-related codes must match the optional
semantic provider registry so guidance never names a capability an agent cannot
act on: `enable_provider` is emitted only for a mechanism an *integrated*
provider slot resolves (today only the TypeScript compiler slot), because that
provider exists and `doctor` shows how to configure it; a mechanism a
*registered-but-not-integrated* slot would resolve (the python and rust slots),
or a framework/dependency-injection/build model only a future provider could
resolve, recovers via `not_implemented_in_current_version` rather than promising
a provider. A single cross-check authority against the registry decides this;
callers must not re-derive it from hard-coded mechanism lists.
`resolve_dependency_metadata` is retained as a reserved historical code but no
live mechanism emits it, because its mechanism is a python-provider-slot bucket
that now recovers via `not_implemented_in_current_version`. The `run_sync`
recovery code keeps that spelling for metric-bucket continuity even though its
operator action is `repogrammar resync`.
Mechanism buckets should be actionable analyzer/provider codes, for example
`python_import_graph`, `pytest_fixture_graph`, `fastapi_dependency_graph`,
`python_package_reexport_model`, `python_star_import_model`,
`pytest_plugin_fixture_model`,
`typescript_module_resolver`, `typescript_paths_resolver`,
`typescript_rootdirs_model`, `typescript_package_entry_model`,
`typescript_commonjs_alias_model`, `typescript_export_graph`,
`fastify_receiver_model`, `prisma_client_model`, `drizzle_db_model`,
`nestjs_di_model`, `hono_receiver_model`,
`rust_module_graph`, `cargo_feature_cfg_model`, `rust_macro_boundary`,
`rust_trait_dispatch_model`, `axum_route_model`,
`java_spring_route_literal_model`,
`spring_component_scan_model`, `spring_di_model`, `spring_proxy_model`,
`spring_data_repository_model`, `java_test_annotation_model`, `jpa_entity_model`,
`jaxrs_resource_model`, `java_annotation_processor_boundary`,
`java_mockito_runtime_mock_model`, `csharp_project_model`, `csharp_di_model`,
`csharp_build_variant_model`, `csharp_source_generator_boundary`,
`aspnet_route_literal_model`, `cpp_build_variant_model`, `cpp_macro_boundary`,
`cpp_test_framework_model`, `cpp_compile_commands_model`,
`django_project_model`, `django_settings_model`, and `flask_app_model`. The
Rust general framework preview routes `rust_framework_attribute_binding` to
`rust_module_graph`, `rust_derive_expansion` to `rust_macro_boundary`, and the
axum claims (`rust_axum_route_identity`, `rust_axum_middleware_semantics`,
`rust_axum_extractor_semantics`) to `axum_route_model`.

The inventory also exposes a source-free `by_obligation` bucket that names the
kind of semantic question each typed `UNKNOWN` poses — its *obligation* — as a
first-class refinement layered on top of the typed `UNKNOWN`. This never
resolves an `UNKNOWN`, changes whether it blocks, or weakens any gate; it only
classifies the question so an agent can see what would have to be proven. The
obligation vocabulary is a fixed, low-cardinality, source-free enum:
`type_identity`, `symbol_binding`, `dispatch_target`, `framework_identity`,
`build_variant`, `macro_expansion`, `external_dependency`, `runtime_irreducible`
(runtime-defined residuals that stay `UNKNOWN` by design, ADR-0015 class c), and
`governance` (stale, conflicting, or insufficient-support quality states, which
are not semantic obligations). Obligation is orthogonal to the internal
`ResolutionClass` axis (whose legacy public projection uses
`recoverable_unknown`/`irreducible_unknown`) and is derived deterministically
from the typed reason plus the same language/claim/role context used for the
required mechanism.
`repogrammar unknowns --json` and `repogrammar stats --unknowns --json` expose a
source-free aggregate inventory for persisted semantic `UNKNOWN` facts. They do
not claim to count every query-time, family-store, preflight, or storage
fallback `UNKNOWN`. Unknown-rate reduction must not be reported as quality
improvement unless false certainty is also measured or controlled.
`repogrammar stats --json` separately exposes `query_outcome_rollup` with
`rollup_scope: local_query_outcomes`. That local rollup counts recorded
family-query outcomes, including typed query-time `UNKNOWN`,
`PARTIAL_CONTEXT`, and fallback responses, by low-cardinality status,
entrypoint, command or MCP operation category, lookup mode, reason code,
required mechanism, semantic obligation, and recovery code. It is
request-outcome observability, not
persisted semantic analyzer inventory, and it must not store affected-claim
free text, raw recovery guidance, targets, paths, source snippets, family ids,
member ids, code-unit ids, fact ids, content hashes, or byte ranges.
The inventory also exposes source-free readiness-scoped language detail for
`python`, `typescript/javascript`, `rust`, `java`, `csharp`, and `c/cpp`, with
top reason and
required-mechanism buckets only; these details must not include paths, code-unit
ids, fact ids, snippets, repository names, or free-text recovery guidance.
For TS/JS import/export unknowns, the inventory may suppress a parser-origin
UNKNOWN only when a same-generation TypeScript compiler fact with
`provider=typescript`, `provider_resolved=true`, the matching
`query_operation`, and the same repo-relative path, strict hash, code-unit id,
and byte range proves the operation. Static worker fallback facts with
`provider_resolved=false` remain context and do not suppress UNKNOWNs.

The repository-level UNKNOWN regression benchmark is documented in
`docs/experiments/unknown-regression-benchmark.md`. It pins release-fixture
inventory buckets by language, reason code, and required mechanism, then checks
that negative fixtures still produce no family rows. Intentional analyzer
improvements may update those exact buckets only when the replacement evidence
has focused positive/negative coverage and preserves the same source-free,
fail-closed output rules.

## Test Expectations

New analyzers, providers, query paths, and serializers should include positive,
negative, stale, conflicting, unsupported, and dynamic cases. Tests must prove
that unknowns are not silently collapsed into dominant patterns, variations, or
exceptions.
