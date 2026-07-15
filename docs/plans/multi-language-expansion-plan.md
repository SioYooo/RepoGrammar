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
