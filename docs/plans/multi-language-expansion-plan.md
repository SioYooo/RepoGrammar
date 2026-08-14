# Multi-Language Structural Expansion Plan

- Status: Active implementation plan
- Last updated: 2026-07-16
- Scope: Execution plan for ADR-0019 — C# and C/C++ bounded preview slices,
  Java framework deepening, and framework-adapter widening for Rust, Python,
  and TS/JS.
- Related docs: `docs/decisions/ADR-0019-bounded-multi-language-structural-expansion.md`,
  `docs/reports/unknown-resolution-sota-analysis.md`,
  `docs/specifications/unknowns.md`, `docs/roadmap.md`,
  `docs/experiments/unknown-regression-benchmark.md`

If this file conflicts with an accepted ADR or a specification, update the
lower-priority text or write a superseding ADR.

## Goal

Give agents working in Java, C/C++, and C# repositories (and richer Rust,
Python, TS/JS repositories) bounded pattern-family context with the same
sound-by-abstention contract as the existing slices: exact anchors form
families; everything dynamic, generated, variant-dependent, or unresolved
stays typed, claim-scoped `UNKNOWN` with a named recovery mechanism.

## Current baseline (before this plan)

- Java: Spring-only preview (`spring_boot_application`, `spring_component`,
  `spring_mvc_route`, `spring_data_jpa_repository`,
  `spring_data_repository_definition`).
- C/C++, C#: not discovered, not parsed, not surfaced.
- Rust: self-dogfood roles only; no general framework anchors.
- Python: FastAPI/pytest/SQLAlchemy/Pydantic only.
- TS/JS: Express/Jest-Vitest/Next/Fastify/Prisma/Drizzle only.

## Engineering template (all waves)

Every language slice follows the Java preview touch-point checklist:

1. `Cargo.toml` grammar dependency (new languages only).
2. Core model: `Language` variant + token, `CodeUnitKind` variants + tokens,
   IR-kind mapping.
3. Discovery: `DiscoveredLanguage` variant, extension adapter under
   `adapters/languages/`, `language_for_path` arm, build-output skip dirs,
   config-file discovery.
4. Parsing adapter under `adapters/parsing/` with anchor engine/method
   constants (`repogrammar-<lang>-syntax` /
   `tree_sitter_<lang>_structural_anchors_v1`), structural units, exact
   anchors, typed UNKNOWNs.
5. Framework role registry under `adapters/frameworks/`.
6. Application indexing: `derive_<lang>_framework_support_facts` +
   blocked-unit and blocking-claim policy + both index and resync insertion
   points with the order-sensitive fact-offset arithmetic.
7. Application family: engine constants, min-support 3, eligible kinds,
   feature extraction, unknown domain (engine+method gated), blocking and
   non-blocking claim lists, evidence-pair compatibility with
   required-equal profiles, cluster signature, variation slots, safe-origin
   support routing.
8. Query/read model: readiness scope (`bounded_v0_2_preview`), inventory
   scope, claim-prefix maps, required mechanisms, recovery mapping,
   expected anchor engine per unit.
9. Persistence read model: `REPO_SHAPE_LANGUAGE_SCOPES` token + the four
   `repo_shape_*_where` whitelists + `family:<lang>:*` glob.
10. Fixtures under `src/fixtures/<lang>/release/v0_2/` + product smoke tests
    (positive exact-anchor family, negative/lookalike, dynamic-unknown,
    low-support, stale-evidence) + leakage-assert registration.
11. `src/fixtures/unknown_reduction/<lang>_<mechanism>_{unresolved,resolved}`
    benchmark pair + pinned bucket baselines.
12. Docs cascade + memories in the same atomic commit.

Provenance strings must match byte-for-byte across parser constants,
application derivation constants, and query/test expectations.

## Wave CS1 — C# initial slice

Language tokens `csharp`, `csharp-config`; family prefix `family:csharp:`;
skip dirs must include `bin/`, `obj/`.

| Framework | Unit kinds | Anchors (exact `using`/FQN gated) | Support targets |
|---|---|---|---|
| ASP.NET Core controllers | `AspNetControllerAction`, `AspNetController` | `Microsoft.AspNetCore.Mvc.{ApiController,Route,HttpGet,HttpPost,HttpPut,HttpDelete,HttpPatch,HttpHead,HttpOptions}` attributes; base `ControllerBase`/`Controller` | `aspnetcore.mvc.{ApiController,HttpGet,...}` |
| Minimal APIs | `AspNetMinimalApiRoute` | literal-first-argument `MapGet/MapPost/MapPut/MapDelete/MapPatch` invocations on builder receivers | `aspnetcore.minimal.map_{get,...}` |
| EF Core | `EfCoreDbContext`, `EfCoreEntitySet` | base `Microsoft.EntityFrameworkCore.DbContext`; `DbSet<T>` get/set properties | `efcore.{db_context,db_set}` |
| xUnit | `CSharpTestMethod` (framework role xunit) | `Xunit.{Fact,Theory,InlineData,MemberData}` | `xunit.{fact,theory}` |
| NUnit | `CSharpTestMethod` (role nunit) | `NUnit.Framework.{Test,TestFixture,TestCase,SetUp,TearDown}` | `nunit.{test,test_case}` |
| MSTest | `CSharpTestMethod` (role mstest) | `Microsoft.VisualStudio.TestTools.UnitTesting.{TestClass,TestMethod,DataRow}` | `mstest.{test_method,data_row}` |

Typed UNKNOWNs (mapped per ADR-0019 D4): attribute lookalikes without exact
`using`/FQN (`UnresolvedImport`, blocking `csharp_attribute_binding`);
non-literal route templates (`FrameworkMagic`, non-blocking
`csharp_aspnet_route_template`); convention routing / DI registration /
assembly scanning (`RuntimeDependencyInjection`, non-blocking
`csharp_di_registration` / `csharp_aspnet_convention_routing`); source
generators + external partial halves (`MacroOrPreprocessor`, blocking
`csharp_generated_source` when the affected declaration is the anchor,
non-blocking otherwise); `dynamic` member binding (`FrameworkMagic`,
blocking `csharp_dynamic_binding` for call-target claims); MSBuild
`Condition` (`BuildVariantAmbiguity`, config scope); Razor files not parsed
(`csharp_razor_compilation` non-blocking context).

Project config: SDK-style `.csproj` + `Directory.Build.props` +
`Directory.Packages.props` → `PROJECT_CONFIG` facts (target framework,
package references, implicit-usings flag) or typed config UNKNOWN.

Mechanisms: `csharp_project_model`, `csharp_source_generator_boundary`,
`csharp_di_model`, `aspnet_route_literal_model`.

Benchmark pair: `csharp_aspnet_{unresolved,resolved}` — unresolved side uses
a lookalike `[HttpGet]` without `using Microsoft.AspNetCore.Mvc;` (no
family); resolved side has exact usings and forms replacement support facts.

## Wave C1 — C/C++ initial slice

Language tokens `c`, `cpp`, `cpp-config`; family prefix `family:cpp:`;
extensions `.c .h .cc .cpp .cxx .hh .hpp .hxx`.

| Framework | Unit kinds | Anchors (include-evidence gated) | Support targets |
|---|---|---|---|
| GoogleTest | `CppTestCase`, `CppTestFixture` | `TEST(S,N)`, `TEST_F(F,N)`, `TEST_P(F,N)`, `TYPED_TEST` macro shapes + `#include <gtest/gtest.h>` (or `gmock`) evidence; fixture `: public ::testing::Test` | `gtest.{test,test_f,test_p,typed_test}` |
| Catch2 | `CppTestCase` | `TEST_CASE("...")`, `SCENARIO("...")` + `#include <catch2/...>` evidence | `catch2.{test_case,scenario}` |
| doctest | `CppTestCase` | `TEST_CASE("...")` + `#include <doctest/doctest.h>` evidence | `doctest.test_case` |
| Boost.Test | `CppTestCase`, `CppTestSuite` | `BOOST_AUTO_TEST_CASE(n)`, `BOOST_AUTO_TEST_SUITE(n)`/`_END()` + boost/test include evidence | `boost_test.{auto_test_case,auto_test_suite}` |
| Qt (context only) | `QtObjectClass` (structural) | `Q_OBJECT` in class body; PMF-form `QObject::connect(a, &A::sig, b, &B::slot)` | context metadata only, no family support in C1 |

Typed UNKNOWNs: `TEST_CASE` with both/neither Catch2 and doctest include
evidence → `ConflictingFacts`/`UnresolvedImport` blocking
`cpp_test_framework_identity`; test macro under `#if`/`#ifdef` →
`BuildVariantAmbiguity` blocking that unit's membership with the guard
condition recorded; user macros wrapping registration macros, token
pasting, computed includes, absent moc/protoc outputs →
`MacroOrPreprocessor`; unresolvable angle includes → `MissingDependency`
(non-blocking context unless the claim depends on it); string-form
`SIGNAL()/SLOT()` connects and function-pointer callback registration →
`FrameworkMagic` scoped to dispatch claims.

Project config: `compile_commands.json` (per-TU flags inventory; staleness
→ typed UNKNOWN), `vcpkg.json`, `conanfile.txt` → `PROJECT_CONFIG` facts.
CMake/Meson/Makefile parsing deferred (absence keeps affected claims
UNKNOWN).

Mechanisms: `cpp_build_variant_model`, `cpp_macro_boundary`,
`cpp_compile_commands_model`, `cpp_test_framework_model`.

Benchmark pair: `cpp_gtest_{unresolved,resolved}` — unresolved side defines
tests behind `#ifdef ENABLE_TESTS` without include evidence; resolved side
has plain `TEST` + gtest include and forms replacement support facts.

## Wave J1 — Java deepening

Reuses `repogrammar-java-syntax` engine; adds unit kinds and roles; splits
`parsing/java.rs` into a `parsing/java/` module (framework-agnostic core +
`spring.rs` + new per-framework files) and hoists the duplicated
blocking-claim and assumption-prefix tables into one shared registry before
adding frameworks.

| Framework | Unit kinds | Anchors (exact import/FQN, dual `jakarta.*`/`javax.*` roots where noted) | Support targets |
|---|---|---|---|
| JUnit 5 | `JavaTestMethod` (role junit5) | `org.junit.jupiter.api.{Test,ParameterizedTest,BeforeEach,AfterEach,BeforeAll,AfterAll,Nested,Disabled}`; `org.junit.jupiter.params.provider.{ValueSource,CsvSource,MethodSource}` | `junit.jupiter.{test,parameterized_test}` |
| JUnit 4 | `JavaTestMethod` (role junit4) | `org.junit.{Test,Before,After,BeforeClass,AfterClass,Ignore,Rule}` | `junit4.test` |
| TestNG | `JavaTestMethod` (role testng) | `org.testng.annotations.{Test,BeforeMethod,AfterMethod,DataProvider}` | `testng.test` |
| Mockito | context anchors on test classes | `org.mockito.{Mock,Spy,InjectMocks,Captor}`; `org.mockito.junit.jupiter.MockitoExtension` via `@ExtendWith` | context metadata (mock semantics are bytecode-generated → UNKNOWN) |
| JPA / Jakarta Persistence | `JpaEntity`, `JpaMappedSuperclass`, `JpaEmbeddable` | `jakarta.persistence.{Entity,Table,Id,GeneratedValue,Column,OneToMany,ManyToOne,ManyToMany,OneToOne,MappedSuperclass,Embeddable,Version,Transient}` + `javax.persistence.*` twins | `jpa.{entity,mapped_superclass,embeddable}` |
| JAX-RS / Jakarta REST | `JaxRsResourceMethod`, `JaxRsResourceClass` | `jakarta.ws.rs.{Path,GET,POST,PUT,DELETE,PATCH,HEAD,OPTIONS,Produces,Consumes,PathParam,QueryParam}` + `javax.ws.rs.*` twins | `jaxrs.{resource,resource_method}` |
| Spring Data derived queries | metadata on `SpringDataRepository` members | method-name grammar `(find|read|get|query|count|exists|delete)(First|Top\d*)?(Distinct)?By...` on recognized repository interfaces | structural metadata + variation slots, not standalone support |
| Lombok | none | `lombok.{Data,Getter,Setter,Builder,Value,NoArgsConstructor,AllArgsConstructor,RequiredArgsConstructor,Slf4j,...}` recognized only to emit typed UNKNOWN | `MacroOrPreprocessor`, claim `java_generated_members`, non-blocking for class identity, blocking for synthesized-member claims |

New blocking claims: `java_test_annotation_binding`,
`java_jpa_entity_identity`, `java_jaxrs_resource_identity` (same
exact-import gate as Spring). New non-blocking claims:
`java_generated_members`, `java_mockito_runtime_mocks`,
`java_spring_data_query_derivation`.

Mechanisms: `java_test_annotation_model`, `jpa_entity_model`,
`jaxrs_resource_model`, extending the existing
`spring_data_repository_model`.

Benchmark pair: `java_junit_{unresolved,resolved}` — lookalike `@Test`
without import vs exact `org.junit.jupiter.api.Test`.

J1 bounded follow-up (2026-07-16): `parsing/java/test_data.rs` resolves only
unique source-visible JUnit/TestNG test-data links within one class-like body.
Accepted JUnit shapes are a complete set of direct repeatable, exact imported/
FQN `@MethodSource` annotations whose scalar/array literal entries (including
blank/omitted same-name convention) each target exactly one static method.
Accepted TestNG shapes are an exact imported/FQN
`@Test(dataProvider = "...")` targeting exactly one exact `@DataProvider`
(whose omitted `name` defaults to the provider method name). The output is
structural replacement evidence only. Strict link identity excludes wildcard/
colliding imports, local shadows, malformed imports, nested annotations, and
parse-open inventories. External/signature/provider-class references,
type-level or inherited sources, explicit containers/meta-annotations,
`PER_CLASS` non-static factories, overloads/duplicates, dynamic names, unknown
identity, partial-positive sets, missing targets, invalid test kind, and
nested-boundary crossings remain typed `UNKNOWN` or conflict. Primary
contracts: [JUnit 6.1.1 MethodSource](https://docs.junit.org/6.1.1/api/org.junit.jupiter.params/org/junit/jupiter/params/provider/MethodSource.html)
and [TestNG annotations](https://testng.org/annotations.html).
The positive regression pair is
`java_test_data_{unresolved,resolved}`; this checkpoint does not execute a test
engine or satisfy the Java completion gate in ADR-0020.

## Wave E1 — existing-language widening

- Rust (general anchors, no longer self-dogfood-only for these roles):
  `#[derive(Serialize)]`/`#[derive(Deserialize)]` + `#[serde(...)]`
  (`serde.derive_model`), `#[derive(Error)]` + `#[error("...")]`
  (`thiserror.error_enum`), `#[tokio::main]`/`#[tokio::test]`
  (`tokio.{entry,test}`), `#[derive(Parser)]` + `#[command]`/`#[arg]`
  (`clap.parser`), axum literal `Router::new().route("/x", get(h))` chains
  (`axum.route`). Derive-macro expansion stays `MacroOrPreprocessor`;
  anchors are the written attribute shapes plus use-path evidence.
- Python: Django (`django.db.models.Model` bases + field declarations,
  `urls.py` `path()`/`re_path()` literal routes, `django.test.TestCase`),
  Flask (`Flask(__name__)`, `@app.route`/`@bp.route` literal rules,
  `Blueprint`), stdlib `unittest.TestCase` + `test_*` methods, click/typer
  command decorators, Celery `@app.task`/`@shared_task`. Settings-driven
  and string-dispatch behavior stays UNKNOWN.
- TS/JS: Zod `z.object(...)` schema builders with exact `zod` import;
  NestJS `@Module/@Controller/@Injectable/@Get/@Post` decorators; Mocha and
  `node:test` `describe/it/test` aliasing onto the existing suite/test
  surface (require package/config runner context like Jest/Vitest); Hono
  literal `app.get('/x', h)` routes. React exclusion unchanged.

## Wave F1 — one common framework per existing frontend language

### Why this wave exists

ADR-0019 already directs RepoGrammar to "cover the mainstream framework and
third-party-library landscape for the new languages and for the already-supported
languages", and the priority-ordered backlog below is the researched list of what
that means. This wave does not choose new frameworks from recollection: every
entry is drawn from that backlog, which was compiled against the developer-survey
and registry sources recorded under "Research sources".

The wave exists because the backlog had no delivery unit. "Later waves" is a
queue, not a scope, so nothing in it could be finished or refused. Wave F1 takes
one entry per language and closes it.

### What "all languages" can mean here

A framework family needs exact, source-visible anchors in files the frontend
already parses. Eleven Top-20 languages have no source frontend at all, so they
can host no framework family of any kind: Go, PHP, Ruby, Swift, Visual Basic
.NET, Delphi/Object Pascal, Ada, Fortran, R, MATLAB, and Scratch. That is not a
scoping choice made here; it is the recorded per-language blocker partition in
`docs/reports/language-support/top-20-program-summary.json`, which this wave
cites rather than restates.

Assembly is excluded separately: ADR-0038 caps it at non-authoritative lexical
candidates that may never support a family. SQL already carries its one
language-internal family from ADR-0040 and needs no framework.

Six lanes remain and each gets exactly one entry: Python, TypeScript/JavaScript,
Java, C#, C/C++, and Rust.

### Feasibility filter

A backlog entry is admitted into this wave only if all four hold. Any entry that
fails one is deferred with the failing condition named, which is a result, not an
omission.

1. **Exact source-visible anchor.** The shape is written in the source the
   existing frontend already reads. Runtime registration, DI resolution, code
   generation, and macro expansion are not anchors; where a framework's meaning
   depends on them, the dependence routes through an existing typed `UNKNOWN`
   mechanism, as Lombok already does.
2. **No new artifact.** No Rust crate, no grammar, no downloaded or bundled
   tool. The zero-external-dependency constraint is not relaxed for frameworks.
3. **Discovery already admits the file.** A framework whose primary artifact is
   an extension discovery does not recognize is excluded *by discovery*, not by
   preference. `.vue`, `.svelte`, and `.razor` are the live cases, which is also
   why the backlog's Vue/Angular/Blazor entries stay deferred.
4. **No silent collision.** RepoGrammar derives family support only when a code
   unit carries exactly one framework role (`single_framework_role`,
   `src/rust/application/indexing.rs`). A second detector firing on a unit an
   existing detector already claims does not error — it drops the unit from the
   support path *and* from the blocked-unit path, silently deleting a family
   that used to form. Every entry therefore declares its overlap surface and
   ships a regression assertion that existing fixtures still form the families
   they formed before.

### Lane assignments

| Lane | Backlog entry | Exact anchor | Role / kind | Support target(s) |
|---|---|---|---|---|
| Rust | `tracing` `#[instrument]` | `#[instrument]` or `#[tracing::instrument]` on a function, gated by same-file `use tracing::instrument` or an inline fully-qualified path | `framework:tracing.instrument` / `tracing_instrument` | `tracing.instrument` |
| C# | FluentValidation `AbstractValidator<T>` | class whose base is using/FQN-gated `AbstractValidator<T>` | `framework:fluentvalidation.validator` / `fluentvalidation_validator` | `fluentvalidation.AbstractValidator` |
| Java | Jakarta Servlet (`HttpServlet` + `@WebServlet`) | class extending imported/FQN `HttpServlet` under dual `jakarta.servlet`/`javax.servlet` roots | `framework:servlet.http_servlet` / `servlet_http_servlet` | `jakarta.servlet.http.HttpServlet`, `javax.servlet.http.HttpServlet` |
| Python | marshmallow schemas | class with exact canonical base `marshmallow.Schema` | `framework:marshmallow.schema` / `marshmallow_schema` | `marshmallow.Schema` |
| TS/JS | Playwright test fixtures | call bound to an exact `@playwright/test` import | existing `framework:jest_vitest.test` role | `playwright.test` |
| C/C++ | CppUnit | `CPPUNIT_TEST_SUITE_REGISTRATION(Identifier);` call-expression macro under `cppunit/` include evidence | `framework:cppunit.suite_registration` / `cppunit_suite_registration` | `cppunit.CPPUNIT_TEST_SUITE_REGISTRATION` |

Two lane decisions are made here rather than at implementation time, because
each changes what the code must be.

**TS/JS adds no new role.** `jest_vitest.suite`/`.test` is already a multi-runner
surface: `mocha.describe`, `mocha.it`, `node_test.describe`, and `node_test.test`
are existing targets on it. Because `support_family` falls through to the exact
target for these roles, each runner still forms its **own** family rather than
clustering with the others. Playwright therefore joins as a new target, gets its
own family, needs no new role or code-unit kind, and adds no new collision
surface.

Implementation narrowed this to the test case only. Playwright exports `test`
but no bare `describe`: its suites are written `test.describe(...)`, a member
call the detector does not anchor because it requires the identifier to be
followed directly by `(`. That is the conservative outcome — a Playwright suite
stays `UNKNOWN` rather than being mistaken for a test case — and it is recorded
here rather than left as a silent gap between plan and code.

**Rust must order its attribute chain.** One function can carry both
`#[tokio::main]` and `#[instrument]`. The shipped tokio detector claims such a
function today, so the tracing detector must not also claim it. Tokio keeps
precedence and the regression assertion covers the both-attributes case.

### Falsifiable acceptance

This wave is complete when all of the following hold. Partial completion is
reported per lane, never averaged.

1. Each of the six lanes has either a landed bounded-preview family or a
   source-backed refusal naming the failing filter condition. A refusal is an
   acceptable completion state; a silent omission is not. Every entry outside
   the six lanes carries a `framework_support` state in the program summary, so
   no language is silently absent from the answer.
2. Every landed lane ships the four-part fixture set through product paths: at
   least three compatible positive members (each language's minimum support is
   three), a lookalike/negative fixture that must not form a family, a
   low-support fixture below the threshold, and a collision-regression assertion
   that pre-existing fixtures still form exactly the families they formed before
   this wave.
3. No public surface exposes framework source text, repository identifiers, or
   absolute paths for the new anchors.
4. `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets
   --all-features -- -D warnings`, `cargo test --workspace --all-features`,
   `cargo run --quiet --bin repo-guard -- check`, and the `check-diff` gate all
   pass, and `AGENTS.md`/`CLAUDE.md` remain byte-identical.
5. The official v0.1 scope sentence in the mirrored agent contract is unchanged.
   Every Wave F1 family is a bounded preview and none of them widens the
   FastAPI/pytest/SQLAlchemy/Pydantic v0.1 target.
6. No language's ADR-0020 gate count changes. Gate 5 needs one exact family per
   language and every lane here already had one; this wave widens framework
   coverage, and recording it as gate movement would be a false claim.

### Outcome and audit findings

All six lanes landed a bounded-preview family, so the wave's first acceptance
condition is met per lane with no refusals. An adversarial audit then refuted
every one of the six on first pass, which is the result worth recording: the
slices were correct about what they anchored and wrong about what they excluded.

Four real defects were found and fixed. Three of them were mine and one was
pre-existing and newly exposed:

1. **Rust attribute detection read the function body.** The attribute needle was
   matched against `unit_slice_with_attributes`, which spans the leading
   attributes *through the closing brace*. A function whose body merely
   mentioned `#[instrument]` or `#[tokio::main]` -- in a string, a comment, or a
   macro template -- was claimed as that framework. Because a unit gets exactly
   one kind, this both invented a role and stole the unit from the role it had.
   Detection now uses an attribute-only slice. The tokio half of this was
   pre-existing; adding tracing is what made it fire, including inside this
   wave's own test file.
2. **`is_class_like`/`is_method_like` were never extended.** This wave's own
   touch-point list names them as a silent-failure point, and all six lanes
   missed them anyway, dropping IR containment edges for four of the five new
   class-like and method-like kinds. Fixed for every new kind. A pre-existing
   gap remains for `django_model` and `django_test`, which is recorded here
   rather than fixed, because closing it changes edges this wave did not create.
3. **A member call was read as a bare runner call.** The TS/JS runner scanner is
   line-based, and `call_offset` accepted `describe(` inside `test.describe(`.
   Playwright suites are written exactly that way, so a real Playwright file
   could mint an ambient `jest_vitest.describe` anchor. `call_offset` now
   rejects member access, and the Playwright fixture uses a real
   `test.describe(...)` so the guard is pinned by product evidence.
4. **`tracing.instrument` fell through to the self-dogfood variation
   dimensions**, and the `playwright` query token was registered as producible
   when no role produces it. Both are documented-invariant violations rather
   than wrong answers; both are corrected.

Bounded limitations the audit surfaced that are *not* defects, and are non-claims
rather than todos. Each is the same fidelity boundary the shipped gate beside it
already has, and narrowing them means resolving bindings this product does not
resolve:

- C# `base_is_exact`, Java's annotation import gate, and the C/C++ include gate
  are all lexical. A file with the right `using`/`import`/`#include` plus a
  locally declared type of the same name is claimed, and a wildcard import can
  blur the jakarta/javax split. The shipped ASP.NET, EF Core, JPA, Catch2, and
  Boost gates have this property today; it is the price of not running a
  compiler.
- Only direct bases anchor. A project's own `BaseSchema(marshmallow.Schema)`
  with N children yields one member, not N. That matches the shipped Pydantic
  and SQLAlchemy behaviour and is deliberately conservative.
- CppUnit registration is anchored; the suite's `CPPUNIT_TEST` entries are not,
  because enumerating them is registry construction at runtime.

### The other fourteen entries, archived

Wave F1 landed a framework family for seven of the twenty-one tracked entries.
"Add common framework support for all languages" cannot be satisfied further
than that today, and the reason differs per language, so each is recorded in
`top-20-program-summary.json` under `framework_support` rather than left to be
re-derived. Four states, and only one of them is a queue:

**Landed (7).** Python, C++, Java, C#, JavaScript, TypeScript, Rust.

**Closed by decision (3).** These will not gain a framework family without a
superseding ADR, so they are answers, not backlog:

- Assembly — ADR-0038 caps the lane at non-authoritative lexical candidates that
  may never support a family.
- Scratch — ADR-0039 records product integration as a NO-GO over the deflate
  dependency, so no source is read at all.
- SQL — SQL's framework surface is migration tooling, and ADR-0035 and ADR-0040
  both refuse to infer a migration tool or an ordering from filenames. The
  bounded DDL frontend reads statement shapes; a `.sql` file contains no other
  framework surface. SQL already carries its language-internal family instead.

**No qualifying candidate yet (1).** C has a frontend, but every C/C++ backlog
entry that passed the Wave F1 filter is a C++ framework. The C-native candidate,
GLib/GObject `G_DEFINE_TYPE`, was assessed and deferred because its GNOME-style
macro spelling has no proving test here. CppUnit's detector is reachable from
the C language token because the frontend is shared — a property of that
frontend, not a claim that C code uses CppUnit.

**Blocked on a frontend (10).** Visual Basic .NET, Delphi/Object Pascal, Ada,
Fortran, R, MATLAB, Go, PHP, Swift, Ruby. These have no source frontend, so no
framework anchor of any kind is visible. Framework support is *downstream* of a
bounded frontend, which is its own multi-module effort under ADR-0020 gate 2 —
sequencing, not infeasibility. Four already name their first framework target in
their completion review: `go.testing.test_function`,
`php.phpunit.test_method`, `ruby.minitest.test_method`,
`swift.xctest.test_method`. The other six name none, so choosing one is part of
the frontend effort rather than a separate framework decision.

The practical consequence: a future "all languages" request is bounded by ten
frontends, not by framework work. Building one bounded frontend and then adding
its first family is the unit of progress, and Wave F1's per-slice checklist is
what the second half of that unit costs.

### Per-slice touch points

The engineering template above applies with items 1 and 3 omitted (no new
language, no new discovery extension). Four of the remaining touch points fail
*silently* when missed, so each slice asserts them rather than relying on review:

- `family_eligible_kind` in `src/rust/application/family.rs` — a kind absent
  here never enters family feature extraction, so the family simply never
  appears.
- the derived-support assumption-prefix filter in
  `src/rust/application/indexing.rs` — a new framework's variation assumptions
  are dropped without a diagnostic if its prefix is missing.
- the `repo_shape_*_where` kind whitelists in
  `src/rust/adapters/persistence/sqlite.rs` — untyped SQL string literals, so a
  missing kind silently undercounts eligible units.
- `is_class_like` / `is_method_like` in `src/rust/adapters/parsing/mod.rs` — a
  missing kind silently drops IR containment edges.

Each slice lands as one atomic Conventional Commit carrying its parser anchor,
role registry, family wiring, fixtures, and documentation together. Lanes are
implemented serially because all six edit the same core-model, family, indexing,
query-vocabulary, and persistence files; parallel worktrees would conflict in
every one of them.

## Wave F2 — frontend-enabled framework lanes

Wave F1 covered every language that already had a source frontend. The ten
languages it could not reach are blocked on a frontend, not on framework work,
and Wave F2 is where that prerequisite gets built one language at a time.

The unit of work is a pair: a bounded frontend scoped to exactly what one
framework anchor needs, then that framework's family. Scoping the frontend to
the anchor is the whole discipline — SQL's frontend reads dialect-invariant DDL
and nothing else, and Go's reads one declaration shape in one filename class.
A frontend that tries to cover a language is a different, much larger project
and is not what this wave does.

Wave F2 lanes may change ADR-0020 gate counts, which is why they are not part of
Wave F1: that wave's acceptance explicitly forbids gate movement because every
language in it already had its exact family. A language crossing from
`discovered_only` to a frontend genuinely closes gates, and the completion
review reports which ones against delivered code.

The Go lane found the boundary the hard way, and it reshapes this wave.

**Four preflight ADRs forbid the scanner route for the claim.** ADR-0021 (Go),
ADR-0022 (Ruby), ADR-0024 (PHP), and ADR-0025 (Swift) each carry an evidence
ladder whose forbidden item names "text or regex matching", and each names a
pinned real parser as primary evidence — a Go standard-library worker, Prism,
a sandboxed PHP frontend, SwiftSyntax. The zero-external-dependency constraint
puts every one of those out of reach. So for these four languages the family is
**closed under the current constraints**, not waiting on effort.

The Go scanner still shipped, demoted to auxiliary evidence per ADR-0041's
correction. That is worth keeping — it recognizes the exact declaration and
emits typed `UNKNOWN`s — but it is not a family and must never be reported as
one.

| Lane | Scanner route | Reason | State |
|---|---|---|---|
| Go | closed for the claim | ADR-0021 evidence ladder item 4 | scanner landed as auxiliary evidence; no family |
| Ruby | closed for the claim | ADR-0022 D5 item 4 | not started; Ruby's grammar is also hostile to scanning |
| PHP | closed for the claim | ADR-0024 D6 item 4 | not started |
| Swift | closed for the claim | ADR-0025 D7 item 5 | not started |

**Six languages have no such clause.** The inventory ADRs for Visual Basic .NET
(0031), Delphi/Object Pascal (0032), Ada (0033), Fortran (0034), R (0036), and
MATLAB (0037) contain no evidence-ladder prohibition on text matching, so the
bounded-scanner route is open to them in principle. None of them names a first
framework target, so scoping one is part of the work rather than a lookup.

Whether the scanner route is *sound* for a given one of those six is a separate
question from whether it is permitted, and it must be answered per language: Go
was a good fit because braces, mandatory parentheses, and an unambiguous `func`
keyword make a bounded scan exact. A language whose grammar defeats scanning
should get a source-backed refusal rather than a scanner that abstains on most
real files.

| Open lane | First framework | Scanner soundness | Authority | State |
|---|---|---|---|---|
| R | testthat `test_that` | sound: line comments, quoted and raw strings, unambiguous nesting; no heredocs, regex literals, or transpose ambiguity | ADR-0042 | landed; review reports 5/9 |
| MATLAB | `matlab.unittest` | doubtful: `'` is both transpose and string delimiter, which is the same class of hazard that rules Ruby out | needs an ADR | not started |
| Visual Basic .NET | MSTest attributes | sound: `'`/`REM` comments and double-quoted-only strings mean the comment character is never a delimiter | ADR-0043 | landed; review reports 5/9 |
| Delphi/Object Pascal | DUnitX `[TestFixture]` | sound: `//`, `{ }`, `(* *)` comments and single-quoted strings are disjoint delimiter sets, so no character serves two purposes | ADR-0044 | landed; review reports 5/9 |
| Ada | AUnit | plausible: verbose but highly regular grammar | needs an ADR | not started |
| Fortran | none dominant | blocked on target selection, not on route | needs an ADR | not started |

R is taken first because testthat is the only entry among the six that is
genuinely dominant in its own ecosystem, and because its runner convention makes
the file path identity evidence rather than a style guess.

Each lane follows the ADR-first order ADR-0040 established: decide the admitted
subset and the exact anchor in the ADR, because both decide what the code is —
and read the preflight's evidence ladder before writing it, which is the
specific mistake ADR-0041 records.

## Later waves (priority-ordered backlog)

- C#: SignalR `Hub` bases + `MapHub<T>`, FluentValidation
  `AbstractValidator<T>`, MediatR `IRequestHandler<TReq,TRes>` closed
  generics, Razor `PageModel` + `OnGet/OnPost` grammar, Refit attribute
  interfaces, Hangfire expression-tree jobs, MassTransit `IConsumer<T>`,
  Serilog/Polly call shapes, Moq/NSubstitute/FluentAssertions test-library
  context, Blazor `ComponentBase` (needs `.razor` scope decision).
- Java: Jakarta Servlet (`HttpServlet` + `@WebServlet`), CDI scopes,
  Bean Validation constraints, Jackson annotation metadata (dual
  `com.fasterxml`/`tools.jackson` roots), Micronaut/Quarkus
  (`@QuarkusTest`, Panache bases, MicroProfile config), MyBatis
  interface↔XML join, Spring `@Configuration/@Bean/@Transactional`
  context anchors, Retrofit/Feign interfaces, Kafka/AMQP listener
  annotations with literal-topic candidates.
- C/C++: CppUnit/Unity(embedded), Drogon `METHOD_LIST` + Crow
  `CROW_ROUTE` + oatpp `ENDPOINT` route macros, wxWidgets event tables,
  GLib/GObject `G_DEFINE_TYPE` + literal `g_signal_connect`, protobuf/gRPC
  `.proto` schema parsing with derivable-but-absent generated-code
  UNKNOWNs, FreeRTOS/Zephyr task/thread macros, spdlog/{fmt} call shapes,
  bounded CMakeLists literal-argument subset.
- Rust: sqlx `query!` shapes + `#[derive(FromRow)]`, tonic service impls,
  tracing `#[instrument]`, criterion/proptest, actix-web attribute routes,
  diesel/sea-orm derives, tauri commands.
- Python: DRF serializers/viewsets, marshmallow schemas, aiohttp server
  routes, pytest plugin markers (`pytest.mark.asyncio`), attrs, Airflow
  DAG/task decorators, Starlette-direct, Litestar.
- TS/JS: Playwright test fixtures, Mongoose schemas, TypeORM decorators,
  tRPC routers, GraphQL SDL/resolver shapes, Cypress, Koa/@koa-router,
  Sequelize/Knex. Meta-frameworks beyond Next.js (Nuxt/SvelteKit/React
  Router) need a dedicated scope decision because of the React exclusion.
- Non-targets documented as deliberate: boto3 (runtime-generated client
  methods are UNKNOWN-dominated), Spock (Groovy front-end required), Vue and
  Angular (front-end scope excluded with React), Makefile semantics, Gradle
  script evaluation.

## Fixture matrix (per new language)

`src/fixtures/<lang>/release/v0_2/`: one positive exact-anchor fixture per
initial framework (>= 3 compatible members), one lookalike/negative fixture,
one dynamic/variant-unknown fixture, one low-support fixture. Plus the
`src/fixtures/unknown_reduction/` pair(s) named above. Java reuses the
existing root and adds junit/jpa/jaxrs positive fixtures plus a lookalike
fixture.

## Research sources (reviewed 2026-07-11)

Usage rankings and static-recognizability evidence: JetBrains State of
Java 2025 and Developer Ecosystem 2025; JRebel Java Productivity Reports
2023/2025; New Relic State of the Java Ecosystem 2024; Snyk JVM 2021;
Jakarta EE Developer Survey 2025; InfoQ Java Trends 2025; Maven Central
rankings; ISO C++ Developer Surveys 2024/2025; JetBrains C++ ecosystem
reports; vcpkg/ConanCenter indexes; Stack Overflow Developer Survey
2024/2025; JetBrains State of .NET 2025; NuGet download statistics;
State of JS 2025; npm registry download API; PyPI top-packages dataset;
JetBrains Python Developers Survey 2024; crates.io download/reverse-dep
API; Rust Survey 2025. Sound no-execution analysis precedent: Clang JSON
compilation-database spec; clangd compile-commands design notes; CodeQL
build-mode-none GA notes (C/C++ 2025, C#/Java 2024); TypeChef (OOPSLA
2011) and SuperC (PLDI 2012) variability-aware parsing; SVF points-to
(CC 2016; requires LLVM IR — documented as out of scope); Roslyn
no-MSBuild source-level compilation constraints.

## Explicit unsupported claims

- No compiler/analyzer execution, no build execution, no macro or source
  generator expansion, no preprocessor evaluation, no Razor compilation,
  no MSBuild/Gradle/CMake evaluation.
- No points-to, no class-hierarchy dispatch resolution, no cross-TU C/C++
  semantic linking, no C# partial-class synthesis beyond checked-in files.
- Structural anchors are candidates plus bounded family evidence under the
  exact-anchor gates; they are not full language semantics, and preview
  scopes are not official v0.1 support.
- UNKNOWN counts may go down only through source-backed replacement facts
  proven by benchmark pairs, never through reclassification.
