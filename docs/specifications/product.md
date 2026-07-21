# Product Specification

RepoGrammar is a local tool for helping coding agents understand recurring
implementation patterns inside a repository.

## Product goal

RepoGrammar should return pattern-family evidence rather than only call graphs
or similarity search results. A result should be able to describe:

- common implementation skeletons;
- high-support repository conventions;
- legitimate variation slots;
- exceptions and counterexamples;
- closest matching implementations;
- contrastive examples that cover key differences;
- source evidence for every conclusion;
- `UNKNOWN` when static analysis cannot support a claim.

The v0.1 technical narrative is Evidence-Constrained Multi-View Family
Induction (EC-MVFI): syntax, compiler semantics, framework role, CFG/dataflow
and effect, API usage, and repository-context views may propose and validate
families, but claims are emitted only when evidence and compatibility gates
support them. Otherwise the result must remain `UNKNOWN`.

The CLI and MCP surfaces must preserve this identity. Human-facing commands are
organized around implementation-pattern families, not generic symbol graph
navigation.

## Intended users

- Local coding agents preparing implementation changes.
- Maintainers reviewing whether a proposed change matches repository norms.
- Developers seeking representative examples inside a large codebase.

## MVP scope

RepoGrammar v0.1 is Python-first. The official v0.1 implementation target is
local Python analysis for recurring repository pattern families in:

- FastAPI;
- pytest;
- SQLAlchemy;
- Pydantic.

The v0.1 product claim is: RepoGrammar provides evidence-gated,
abstention-first,
metadata-only, repo-local Python implementation/integration-family evidence and
read planning for FastAPI, pytest, Pydantic, and SQLAlchemy. It can reduce
coding-agent context acquisition cost when local repeated patterns exist, and it
returns typed `UNKNOWN` when evidence is insufficient. This is not a claim of
sound Python semantic analysis.

The first Python implementation phase follows the claim-driven selective
cascade in `docs/decisions/ADR-0012-python-selective-analysis-cascade.md`.
The implemented slice covers CPython `ast` structural candidates, path-derived
module anchors, CPython `symtable` structural scope anchors, private
project-config summaries from `tomllib` (`pyproject.toml`), `configparser`
(`setup.cfg`), and CPython `ast` (`setup.py`, never executed), semantic-worker-
compatible project-mode module-level repo-local import resolution, default
parser-mode repo-local import context from discovered `.py` inventory and
sanitized root project-config source roots, and framework-role heuristics only,
plus
file-local simple FastAPI router/app alias propagation and a narrow bounded
exact-anchor derivation step that synthesizes separate
`DATAFLOW_DERIVED` support facts when validated parser anchors exact-match the
canonical Python framework compatibility table for a unit with one framework
role. Product smoke tests now prove low-support and dynamic cases remain
`UNKNOWN`, while three-member direct FastAPI, FastAPI alias, pytest, Pydantic
model/settings, SQLAlchemy model-field, and SQLAlchemy session/repository
fixtures can produce families without a semantic worker through those derived
facts. Before storage, Python families now pass bounded complete-link
clustering over support-family features, preventing bridge members from
single-linking incompatible support into one confident claim. Ready Python
families can also record narrow variation metadata when their
already-compatible exact framework-anchor support targets differ within the same
family; this does not imply provider-backed semantics or runtime equivalence.
The current Python worker can also emit bounded same-function
FastAPI route service-call anchors as structural handler/service context and
literal `include_router(..., prefix="...")` router-prefix anchors as module
context. Stored prefix metadata contains only low-cardinality segment shapes,
not the route literal; those anchors are not membership support. It can also emit static
FastAPI request body and request-parameter anchors for `Body`, `Path`, `Query`,
`Header`, and `Cookie` marker shapes; those are route-shape context only and
are not membership support. Dynamic decorator factories and `setattr(...)`
monkey-patching become typed `UNKNOWN`s rather than inferred framework identity
or call-target evidence. It also persists root
`pyproject.toml`, `setup.cfg`, and `setup.py` only as structural project-config
context or typed/conservative config results; only lexically import-bound
`setuptools` setup calls with no recognized binding/module mutation that are
direct unconditional zero-positional module-body expressions with no keyword
unpacking and unique relevant keywords contribute static `setup.py` values.
`package_dir` must be a complete unique string-to-string mapping, and a finder
root is accepted only from that call's direct `packages=` value with at most one
unambiguous literal positional-or-keyword `where`. Local, standalone/lookalike,
conditional, shadowed/deleted/mutated, computed, partial, duplicate,
overridable, or top-level-unreachable recognized values produce abstention or
typed `MissingProjectConfig` rather than guessed roots; empty `setup()` remains
valid. Multiple authoritative setup calls produce `ConflictingFacts` instead of
merged config.
Roots across coexisting config formats are a structural candidate union, not a
packaging/setuptools precedence result; these records cannot prove or suppress
strong family evidence. Subsequent slices should add selective Pyrefly provider
queries for plausible family
candidates, Pyright cross-checks only for claim-upgrading facts, broader
bounded role propagation, cross-function target-centered call recovery, richer
EC-MVFI-lite family induction, and typed `UNKNOWN` governance.
The current Rust code now has owned future Python, Rust, and TS/JS provider
port contracts for request scope, provenance assumptions, cache keys, and
recoverable provider-unavailable `UNKNOWN`s, plus an application-layer planner
that can construct candidate-scoped Pyrefly framework-identity request
envelopes for future adapters from in-memory facts or validated
active-generation snapshots. Apart from the Rust Cargo metadata project-model
stage described below, it does not execute provider tools, store provider facts,
or add production provider-backed Python, Rust, or TS/JS family support.
The default product indexing path now wires the Rust Cargo metadata adapter as a
safe project-model refresh stage when same-generation `Cargo.toml` code units
exist. It can produce owned `PROJECT_CONFIG` facts and provider `UNKNOWN`s
without executing build scripts or procedural macros, but it does not prove
symbol/type/call semantics or family support. The Rust/TSJS semantic analysis
roadmap is tracked in `docs/plans/rust-tsjs-semantic-analysis-plan.md`.
The canonical algorithm contract is
`docs/specifications/python-analysis.md`.

Existing TypeScript/JavaScript discovery, syntax extraction, framework-role
facts, TypeScript worker protocol scaffolding, and release fixtures are
transitional substrate from the earlier bootstrap. They may remain useful, but
they are no longer the official v0.1 language target. The v0.2 preview may emit
conservative exact-anchor families for Express, Jest/Vitest (including Mocha and
`node:test` aliased onto the suite/test surface with a required-equal
`runner_kind`), Next.js, Fastify, Prisma, Drizzle, Zod exact `z.object`/
`z.union`/`z.discriminatedUnion`/`z.enum`/`z.array` schema builders, NestJS
`@Controller`/`@Get`/`@Injectable`/`@Module` decorators bound to
`@nestjs/common`, and Hono literal `app.get`/`post`/`put`/`delete`/`patch`
routes on a `new Hono()` receiver, only when there are at least three
complete-link-compatible derived support facts and no claim-relevant blocking
`UNKNOWN`s. NestJS routes anchor only inside an exact-import `@Controller`
class; a `@Get`-style decorator outside one, or any decorator not bound to
`@nestjs/common`, stays a blocking `tsjs_nest_controller_identity` `UNKNOWN`.
Zod schemas require an exact `zod`/`zod/v4` import of `z`; Hono routes require a
receiver-binding-safe `new Hono()` trace. RepoGrammar does not model the NestJS
DI graph, dynamic modules (`forRoot`/`forRootAsync`), or Zod runtime refinement
semantics; those stay non-blocking typed `UNKNOWN`s. React components/hooks
remain excluded from all family claims. Bounded TS/JS
project inventory may record package/config test-runner context, Next package
context, JSON path aliases, and safe JSON `rootDirs`, and the syntax parser may
record unique repo-local literal relative/path-alias/rootDirs imports as
`STRUCTURAL` context. Dynamic imports, non-literal or conditional `require`,
unresolved aliases/imports/rootDirs targets, ambiguous re-exports, dynamic
framework magic, raw query builders, and missing project context remain typed
`UNKNOWN` rather than family support.
Exact TS/JS framework bindings may use ES imports, CommonJS `require`, or
CommonJS destructuring aliases from the exact supported framework package; this
does not support custom wrappers, injected clients, plugin containers, or
runtime extension mechanisms.
Jest/Vitest script configs such as `jest.config.ts` or `vitest.config.js` are
recorded as metadata/typed `UNKNOWN` only; they are not executed and do not by
themselves provide ambient runner context. React components/hooks remain blocked
from public family claims even if an explicit TypeScript semantic-worker fact
names `react`. Production-quality TS/JS semantic analysis, React family support,
complete Program/TypeChecker coverage, complete re-export/path-alias semantics,
Fastify plugin-effect and dynamic-prefix resolution, Prisma/Drizzle runtime
extensions, Next server/client semantics, middleware, server actions, re-export
semantics, and dynamic-wrapper support remain deferred unless a later ADR
changes the sequence again. A bounded optional TypeScript worker operation
slice may produce
compiler-backed module-resolution facts and may cross-check exact Next.js
file-convention export identity, relative repo-local Express/Fastify named
handler imports, relative repo-local Prisma shared-client bindings, or relative
repo-local Drizzle db/table bindings when a TypeScript compiler API is
available. Only TypeScript-provider
`resolve_export` facts that match the parser Next.js anchor's
path/hash/code-unit/range, export name, framework role, and structural-anchor
provenance, or TypeScript-provider `resolve_reexport` facts that match a
provider-required Express/Fastify handler or Prisma anchor's
path/hash/code-unit/range and named relative import such as
`./handlers#listUsers` or `./db#prisma`, or every required Drizzle named
import such as `./db#db` and `./schema#users`, may become TS/JS-derived
support.
Static fallback facts remain structural context and do not support family claims. Exact local
Next dynamic segments, route groups, and parallel routes may be recorded as
context assumptions on supported page/layout/route anchors; they are not
independent support evidence.

Rust support in the v0.2 preview is a bounded structural preview
(`bounded_v0_2_preview`) covering both RepoGrammar self-dogfooding and general
framework anchors. It uses Tree-sitter Rust for structural code-unit extraction
and RepoGrammar-owned role anchors. It may produce bounded internal families for
RepoGrammar's own indexing phases, family gates, parser adapters, CLI/MCP
handlers, installer actions, storage validation, source-span renderers, and
product tests when support is sufficient and no Rust-specific `UNKNOWN` blocks
the claim.

Beyond self-dogfooding, the preview now recognizes general framework anchors in
any repository, each gated by same-file `use`-path evidence or an inline
fully-qualified path: serde `#[derive(Serialize)]`/`#[derive(Deserialize)]`
models (with `#[serde(...)]` shape assumptions), thiserror `#[derive(Error)]`
enums carrying `#[error(...)]` variants, `#[tokio::main]`/`#[tokio::test]`
entrypoints, clap `#[derive(Parser|Subcommand|Args)]` parsers (with
`#[command]`/`#[arg]` shape assumptions), and axum literal
`Router::new().route("/x", get(handler))` segments. The supported targets are
`serde.Serialize`/`serde.Deserialize`, `thiserror.Error`, `tokio.main`,
`tokio.test`, `clap.Parser`/`clap.Subcommand`/`clap.Args`, and
`axum.routing.route`. RepoGrammar does not claim derive/attribute macro
expansion, trait resolution, or points-to analysis: the derive/attribute
*expansion* stays a non-blocking `MacroOrPreprocessor` subclaim, axum handler
extractor and tower middleware semantics stay non-blocking `FrameworkMagic`
subclaims, derive tokens without use-path evidence and non-literal or
untraceable axum routes are blocking typed `UNKNOWN`s, and `#[cfg]` on a
framework unit keeps its blocking build-variant behavior. General Rust families
require at least three complete-link-compatible derived support facts and no
claim-relevant blocking `UNKNOWN`.

It must not be described as general Rust semantic analysis: the
indexer does not run Cargo, rustc, build scripts, procedural macros, or
whole-program trait/call resolution. Checked-in `Cargo.toml` metadata may
record structural package name, edition, workspace members, dependencies,
features, explicit lib/bin/test/bench target names, and explicit crate-root
paths. Repo-local `use crate::...`, `use super::...`, and `use self::...` paths
may become structural import context only; external dependency semantics remain
Cargo metadata or typed `UNKNOWN`, not family support. Cargo build scripts and
target-specific root manifest sections are typed build-variant `UNKNOWN`s that
block affected Rust self-dogfood families until resolved. Nested
fixture/package manifests must not globally block unrelated root Rust family
support. Source-level `#[cfg]` and `#[cfg_attr]` build-variant UNKNOWNs may
carry bounded nearest `Cargo.toml` feature context, including simple feature
predicates and whether the feature is declared, but that context does not
evaluate cfgs or prove family support.

Java support in the v0.2 preview is a conservative Spring/Spring Boot
structural slice. RepoGrammar can discover `.java` files, parse Java classes,
interfaces, methods, and Spring-shaped units with Tree-sitter Java, and derive
bounded support for exact source-visible Spring annotations only when the
annotation is fully qualified or imported from the expected Spring package.
The supported Spring targets are Spring MVC `@RequestMapping` /
`@GetMapping` / `@PostMapping` / `@PutMapping` / `@PatchMapping` /
`@DeleteMapping`, Spring stereotypes including `@Controller`,
`@RestController`, `@Service`, `@Repository`, and `@Component`,
`@SpringBootApplication`, and Spring Data repository interfaces that exactly
extend/import `JpaRepository` or use exact `@RepositoryDefinition`. The parser
does not execute Maven, Gradle, javac, annotation processors, Spring component
scans, dependency injection, AOP proxies, repository factories, or classpath
resolution. Lookalike simple annotations without exact imports, custom composed
annotations, route constants, DI/proxy semantics, missing project context, and
classpath-sensitive behavior remain typed `UNKNOWN` or non-supporting context.
Java/Spring families require at least three complete-link-compatible derived
support facts and no claim-relevant blocking `UNKNOWN`.

The v0.2 Java preview also deepens beyond Spring with the same exact-import/FQN
gate: JUnit 5 (`@Test`/`@ParameterizedTest`) and JUnit 4/TestNG test methods,
JPA/Jakarta Persistence `@Entity`/`@MappedSuperclass`/`@Embeddable` under dual
`jakarta.persistence` and `javax.persistence` roots, and JAX-RS/Jakarta REST
`@Path` resource classes with `@GET`/`@POST`/... resource methods under dual
`jakarta.ws.rs` and `javax.ws.rs` roots. Mockito annotations attach
`mockito_context` metadata to enclosing tests, Lombok annotations are recognized
only to emit a non-blocking generated-members `UNKNOWN`, and Spring Data
derived-query method names attach structural metadata. Non-claims: no annotation
processing or Lombok member synthesis, no Mockito mock generation, no
`testng.xml`/`orm.xml` parsing, no JPA runtime mapping, and no derived-query
property-path validation. jakarta and javax entities never cluster into the same
family. Each of these framework families also requires support >= 3 with
complete-link compatibility.

Within that Java test preview, RepoGrammar may discharge the non-blocking
test-data-link `UNKNOWN` only for a uniquely provable source-visible binding in
the same class-like boundary: the complete set of direct exact JUnit
`@MethodSource` annotations and scalar/array literal entries must resolve to
unique static methods, or an exact TestNG `@Test(dataProvider = "...")` must
match one exact `@DataProvider` declaration. Link identity requires an FQN or
one unambiguous explicit import and rejects wildcard/colliding imports, local
type shadows, parse-open inventories, nested annotations, and partial positive
sets. The replacement is a separate `STRUCTURAL` fact, not test-family
membership support. External/signature-selected, inherited, type-level,
explicit-container/meta-annotation, `PER_CLASS` non-static, overloaded or
duplicate, dynamic, missing, mixed-framework, parse-degraded, and
nested-boundary forms remain typed `UNKNOWN` or conflict. This follows the
official [JUnit 6.1.1 MethodSource contract](https://docs.junit.org/6.1.1/api/org.junit.jupiter.params/org/junit/jupiter/params/provider/MethodSource.html)
and [TestNG annotation contract](https://testng.org/annotations.html) without
executing javac, Maven, Gradle, annotation processors, test engines, or
repository code.

C# support in the v0.2 preview is a conservative ASP.NET Core / EF Core /
xUnit / NUnit / MSTest structural slice. RepoGrammar can discover `.cs` files
(skipping the MSBuild `obj/` output directory), parse C# classes, records,
structs, interfaces, methods, and properties with Tree-sitter C#, and derive
bounded support for exact source-visible framework anchors only when an
attribute short name is backed by an exact Tree-sitter `using` in its lexical
compilation-unit/namespace scope (with or without an `Attribute` suffix) or an
inline fully-qualified name. Comments, strings, and sibling namespaces never
corroborate identity. The supported targets are
ASP.NET Core controllers (`[ApiController]`, `ControllerBase`/`Controller`
bases), `[HttpGet/Post/Put/Delete/Patch/Head/Options]` actions inside an exact
controller, minimal-API `MapGet/MapPost/MapPut/MapDelete/MapPatch` calls whose
receiver traces in-file to `WebApplication.CreateBuilder(...).Build()`, EF Core
`DbContext` subclasses and their `DbSet<T>` properties, and xUnit
`[Fact]`/`[Theory]`, NUnit `[Test]`/`[TestCase]`, and MSTest `[TestMethod]`
(within a `[TestClass]`) test methods. RepoGrammar never executes MSBuild,
Roslyn, source generators, or the ASP.NET Core runtime, and never evaluates
preprocessor conditions. It does not simulate source-generator or partial-class
output, does not analyze Razor/Blazor, and leaves convention routing
(`MapControllerRoute`/`MapHub`/`MapGrpcService`), runtime dependency injection,
the filter pipeline, `dynamic` binding, and `#if` build variants as typed
`UNKNOWN`. Lookalike attributes without exact usings, route attributes outside a
controller, unresolvable minimal-API receivers, and MSTest methods without a
`[TestClass]` are blocking typed `UNKNOWN`s. C# families require at least three
complete-link-compatible derived support facts and no claim-relevant blocking
`UNKNOWN`.

C/C++ support in the v0.2 preview is a conservative GoogleTest / Catch2 /
doctest / Boost.Test structural slice. RepoGrammar can discover `.c`/`.h` (C
grammar) and `.cc`/`.cpp`/`.cxx`/`.hh`/`.hpp`/`.hxx` (C++ grammar) files
(skipping the CLion `cmake-build-debug`/`cmake-build-release` output
directories) and derive bounded support for registration-macro shapes only when
corroborated by a lexically parsed `#include`: GoogleTest `TEST`/`TEST_F`/
`TEST_P`/`TYPED_TEST` and fixture base `::testing::Test` (via `gtest/gtest.h` or
`gmock/gmock.h`), Catch2 `TEST_CASE`/`SCENARIO` (via `catch2/` or `catch.hpp`),
doctest `TEST_CASE` (via `doctest/doctest.h`), and Boost.Test
`BOOST_AUTO_TEST_CASE`/`BOOST_FIXTURE_TEST_CASE`/`BOOST_AUTO_TEST_SUITE` (via
`boost/test/`). Both the function-definition-with-macro-declarator and the
call-expression parse shapes are handled. RepoGrammar never runs a build,
compiler, preprocessor, or moc/protoc, never expands macros, and never evaluates
`#if` conditions. A `TEST_CASE` with both or neither of Catch2/doctest include
evidence, any registration macro without include evidence, `#if`/`#ifdef`
conditional regions overlapping a unit (except standard include guards), and
Tree-sitter ERROR-node regions are blocking typed `UNKNOWN`s. Qt `Q_OBJECT`/moc
output, string-based SIGNAL/SLOT connect, function-pointer dispatch, and
`compile_commands.json`/`vcpkg.json`/`conanfile.txt` project configuration are
non-blocking subclaims or structural inventory only. C/C++ families require at
least three complete-link-compatible derived support facts and no claim-relevant
blocking `UNKNOWN`.

## Public-preview support matrix

| Area | Status | Public claim |
|---|---|---|
| Python FastAPI | Supported | Bounded framework-family evidence under evidence-gated, abstention-first rules; literal `include_router(..., prefix="...")` is context only, and dynamic prefixes/router factories remain `UNKNOWN`. |
| Python pytest | Supported | Bounded test/fixture family evidence with typed fixture ambiguity `UNKNOWN`. |
| Python Pydantic | Supported | Bounded model/settings family evidence; validator side effects, external bases, and dynamic factories remain `UNKNOWN`. |
| Python SQLAlchemy | Supported | Bounded model/repository evidence; raw SQL, external bases, custom query wrappers, and dynamic declarative patterns remain conservative. |
| Python Django | Bounded v0.2 preview | Exact `django.db.models.Model` bases (with bucketed field-count and `class Meta` variation context), literal `urlpatterns` `path()`/`re_path()` routes, and `django.test.TestCase` classes; settings evaluation, URL reversing, string dispatch/`include()`, and middleware order remain typed `UNKNOWN`. |
| Python Flask | Bounded v0.2 preview | Functions decorated by a same-file `Flask(__name__)`/`Blueprint(...)` receiver with a literal `@app.route`/`@bp.route` rule or Flask 2 method shortcut; unresolvable receivers, non-literal rules, and app-context globals remain `UNKNOWN`. |
| Python unittest | Bounded v0.2 preview | `test_*` methods inside an exact `unittest.TestCase` subclass, with `setUp`/`tearDown` fixture-shape context; `unittest.mock.patch` string targets remain non-blocking `UNKNOWN`. |
| Python click/typer | Bounded v0.2 preview | Functions decorated by `click.command`/`click.group` or a `typer.Typer()` receiver's `@app.command`, with bucketed CLI parameter-count context; entry-point/plugin composition remains `UNKNOWN`. |
| Python Celery | Bounded v0.2 preview | Functions decorated by `@shared_task` or a `Celery(...)` receiver's `@app.task`; broker routing, `.delay()`/`.apply_async()`, and `send_task()` string dispatch remain non-blocking `UNKNOWN`. |
| JS/TS Express | Conservative v0.2 preview | Exact import/require bindings, including CommonJS destructuring aliases, plus direct literal route calls and exact literal `app.use("/prefix", router)` mount context; support>=3 and complete-link compatibility required, while dynamic prefixes and middleware side effects remain unsupported. |
| JS/TS Jest/Vitest | Conservative v0.2 preview | Exact imported/aliased runners or ambient test-file runners with safe project context; support>=3 required. |
| JS/TS Next.js | Compiler-cross-checked v0.2 preview | `next` package context plus exact local App Router pages/layouts/routes and Pages Router pages/API routes; configured TypeScript workers can cross-check exact route/page/layout/API export identity, while dynamic segments, route groups, parallel routes, middleware, server/client semantics, server actions, and re-exported routes remain context or `UNKNOWN`. |
| JS/TS Fastify | Structural v0.2 preview | Exact local Fastify factory receiver, including CommonJS destructuring aliases, plus shorthand or literal `app.route` declarations; exact local `register(plugin, { prefix: "..." })` records plugin/prefix context only, while dynamic methods/options, imported plugins, plugin side effects, and dynamic prefixes remain `UNKNOWN`. |
| JS/TS Prisma | Compiler-cross-checked v0.2 preview | Exact local `new PrismaClient()` bindings, including CommonJS destructuring aliases, plus whitelisted model read/write operations and array `$transaction`; configured TypeScript workers can cross-check relative repo-local named shared-client imports, while bulk operations, injected clients, external shared clients, extensions, callback transactions, dynamic model/op access, and raw SQL remain `UNKNOWN`. |
| JS/TS Drizzle | Compiler-cross-checked v0.2 preview | Exact Drizzle table factories and local `drizzle(...)` db bindings, including CommonJS destructuring aliases, remain structural; configured TypeScript workers can cross-check relative repo-local named db/table imports for whitelisted `select`/`insert`/`update`/`delete` and `db.query.<table>.findMany/findFirst`; unresolved tables/dbs, external imports, dynamic builders, and raw SQL remain `UNKNOWN`. |
| JS/TS React | Not supported | Components/hooks may be detected as roles but cannot form public family claims. |
| Full JS/TS semantics | Not supported | Only bounded optional TypeScript worker operations exist; no full Program/TypeChecker semantics, runtime DI, dynamic wrapper execution, or broad JS/TS family support. |
| Rust self-dogfood | Structural v0.2 preview | RepoGrammar-owned implementation-family evidence only; Tree-sitter structural anchors with no Cargo/rustc/proc-macro execution. |
| Rust general frameworks | Structural v0.2 preview | Exact use-path/FQN-gated serde derive models, thiserror error enums, `#[tokio::main]`/`#[tokio::test]` entrypoints, clap derive parsers, and axum literal `Router::new().route(...)` segments; derive/attribute macro expansion, trait/extractor resolution, tower middleware ordering, and points-to remain typed `UNKNOWN`. No Cargo/rustc/proc-macro execution. |
| Rust provider-backed project model | Preview | Default indexing can refresh Cargo metadata `PROJECT_CONFIG` facts for discovered manifests without build-script/proc-macro execution, and parser-origin cfg UNKNOWNs can carry bounded Cargo feature context; rust-analyzer/rustc/rustdoc JSON adapters and provider-backed family support remain deferred. |
| Go | Discovered only; not supported | Default discovery and indexing persist source-free `go` records for bounded `.go` files and `go-config` records for root or nested `go.mod`/`go.work`. One pure path classifier distinguishes Go-tool-excluded dot/underscore, `vendor`, and `testdata` shapes plus `_test.go` and the Go 1.26.5 known GOOS/GOARCH filename suffix snapshot without selecting an ambient build environment. The indexing loop skips parser-facing source-store reads and parsing for these tokens, derives at most one path-free warning per token from the whole manifest, reports Go-only generations as `file_manifest_only`, keeps inventory-only deltas incremental, purges claim-bearing copy-forward records for Go paths, and produces zero units, facts, IR, or families. Marker scanning, syntax, semantic facts, readiness, and the future exact `go.testing.test_function` family remain unimplemented and gated by a separately reviewed OS sandbox, authoritative claim-impact classification, support >= 3, later atomic modules, and a final completion audit. |
| PHP | Discovered only; not supported | Default discovery and indexing persist bounded source-free `php` metadata for exact `.php` paths and `php-config` metadata for exact root/nested `composer.json`, `composer.lock`, `phpunit.xml`, and `phpunit.xml.dist`. One pure normalized-path classifier gives configuration precedence and rejects PHP candidates below exact `.composer`/`.phpunit.cache` components without globally pruning other languages; exact `vendor` remains globally excluded. Full and incremental indexing perform no PHP source-store read or parser attempt, emit at most one path-free warning per token, report PHP-only generations as `file_manifest_only`, preserve mixed syntax-only mode, keep inventory deltas incremental, purge legacy claim records, and produce zero units, IR, facts, `UNKNOWN`s, or families. No configuration is parsed and custom `vendor-dir` remains unresolved. The sandboxed frontend, project model, exact `php.phpunit.test_method` family, source-free readiness, and final audit remain unimplemented; no dependency or PHP/Composer/PHPUnit execution is authorized. |
| Swift | Discovered only; not supported | Default discovery and indexing persist bounded source-free `swift` metadata for exact `.swift` paths and `swift-config` metadata for exact `Package.swift`, `Package.resolved`, `.swift-version`, and complete ASCII version-manifest basenames. One pure normalized-path classifier gives configuration precedence and rejects Swift candidates below exact `.build`/`.swiftpm` components without globally pruning other languages. Full and incremental indexing perform no Swift source-store read or parser attempt, emit at most one path-free warning per token, report Swift-only generations as `file_manifest_only`, preserve mixed syntax-only mode, keep inventory deltas incremental, purge legacy claim records, and produce zero units, IR, facts, `UNKNOWN`s, project records, or families. No configuration is decoded or evaluated. The sandboxed frontend, static project model, exact `swift.xctest.test_method` family, source-free semantic readiness, and final audit remain unimplemented; no dependency, toolchain, or Swift/SwiftPM/Xcode execution is authorized. |
| Ruby | Discovered only; not supported | Default discovery and indexing persist bounded source-free `ruby` metadata for exact `.rb` paths and `ruby-config` metadata for the accepted root/nested project basenames. One pure normalized-path classifier gives config precedence and rejects Ruby candidates below exact `.bundle`/`.ruby-lsp` components without globally pruning other languages. Full and incremental indexing perform no Ruby source-store read or parser attempt, emit at most one path-free warning per token from the whole manifest, report Ruby-only generations as `file_manifest_only`, preserve mixed syntax-only mode, keep inventory deltas incremental, purge legacy claim records, and produce zero units, IR, facts, `UNKNOWN`s, or families. Autosync retains its generic Git-independent fingerprint behavior. The sandboxed pinned native Prism frontend, project model, authoritative Ruby `UNKNOWN` classification, exact direct `ruby.minitest.test_method` family, source-free readiness, and final completion audit remain unimplemented; no dependency or Ruby/Bundler/project execution is authorized. |
| Java Spring MVC | Structural v0.2 preview | Exact imported/FQN Spring MVC route annotations inside exact controllers; route constants and runtime dispatch remain typed `UNKNOWN` subclaims. |
| Java Spring/Spring Boot components | Structural v0.2 preview | Exact Spring stereotypes and `@SpringBootApplication`; component scan, DI, auto-configuration, and proxy behavior remain runtime `UNKNOWN`s. |
| Java Spring Data | Structural v0.2 preview | Exact imported/FQN `JpaRepository` inheritance or `@RepositoryDefinition`; generated implementations, repository factories, module selection, and classpath resolution remain `UNKNOWN`. Derived-query method names attach structural metadata only, never a support target. |
| Java JUnit/TestNG | Structural v0.2 preview | Exact imported/FQN JUnit `@Test`/`@ParameterizedTest`, JUnit 4 `@Test`, and TestNG `@Test` methods; a complete direct-repeatable scalar/array-literal same-class `@MethodSource` set or one exact local `@DataProvider` link may add structural replacement evidence only. External/signature-selected/inherited/type-level/container/meta/`PER_CLASS` non-static/overloaded/duplicate/dynamic/missing/parse-degraded/nested links and Mockito bytecode mocks remain non-blocking `UNKNOWN` or conflict; mixed test-framework identity is a blocking conflict. |
| Java JPA/Jakarta Persistence | Structural v0.2 preview | Exact imported/FQN `@Entity`/`@MappedSuperclass`/`@Embeddable` under dual `jakarta.persistence`/`javax.persistence` roots; field/mapping annotations are shape metadata; jakarta and javax entities never cluster together; lazy proxies, naming strategies, and `orm.xml` remain `UNKNOWN`. |
| Java JAX-RS/Jakarta REST | Structural v0.2 preview | Exact imported/FQN `@Path` resource classes and `@GET`/`@POST`/`@PUT`/`@DELETE`/`@PATCH`/`@HEAD`/`@OPTIONS` resource methods under dual `jakarta.ws.rs`/`javax.ws.rs` roots; a verb annotation outside a `@Path` class is a blocking `UNKNOWN`. |
| Java Lombok | Recognized as `UNKNOWN` only | Exact imported `lombok.*` annotations emit a non-blocking `MacroOrPreprocessor` generated-members `UNKNOWN`; compiler-AST member synthesis is never simulated, and no family support is derived. |
| C# ASP.NET Core | Structural v0.2 preview | Exact using/FQN-gated `[ApiController]`/`ControllerBase` controllers, `[Http*]` actions inside an exact controller, and literal minimal-API `Map*` routes whose receiver traces to `WebApplication.CreateBuilder(...).Build()`; DI, filter pipeline, convention routing, nonliteral route templates, and MSBuild/Roslyn/source-generator behavior remain typed `UNKNOWN`. No Roslyn/MSBuild execution and no Razor. |
| C# EF Core | Structural v0.2 preview | Exact using/FQN-gated `DbContext` subclasses and their `DbSet<T>` entity-set properties; runtime model building, migrations, and providers remain `UNKNOWN`. |
| C# test frameworks | Structural v0.2 preview | Exact using/FQN-gated xUnit `[Fact]`/`[Theory]`, NUnit `[Test]`/`[TestCase]`, and MSTest `[TestMethod]` (within `[TestClass]`) methods; direct-literal xUnit `MemberData` links may resolve only to a unique unconditional public-static field/property/zero-argument method in the same closed non-generic class. Runtime data rows/discovery, open or dynamic `MemberData`, and `[TestMethod]` outside a `[TestClass]` remain `UNKNOWN`. |
| C/C++ test frameworks | Structural v0.2 preview | Include-evidence-gated GoogleTest `TEST`/`TEST_F`/`TEST_P`/`TYPED_TEST` and `::testing::Test` fixtures, Catch2/doctest `TEST_CASE`/`SCENARIO`, and Boost.Test `BOOST_AUTO_TEST_CASE`/`BOOST_FIXTURE_TEST_CASE`/`BOOST_AUTO_TEST_SUITE` registration macros; macro lookalikes without includes, Catch2-vs-doctest ambiguity, `#if` build variants, and ERROR-node regions remain typed `UNKNOWN`. No build/compiler/preprocessor/moc execution and no macro expansion. |
| C/C++ project config | Structural v0.2 preview | `compile_commands.json`, `vcpkg.json`, and `conanfile.txt` are parsed as source-free `PROJECT_CONFIG` inventory only (never family support); unlocatable entries and malformed manifests become typed `UNKNOWN`. Qt moc, function-pointer dispatch, and string SIGNAL/SLOT stay non-blocking context. |
| TS/JS provider-backed semantics | Limited preview | Bounded TypeScript worker export/binding facts can support exact Next.js file-convention export identity, relative repo-local Express/Fastify named handler imports, relative repo-local Prisma shared-client binding, and relative repo-local Drizzle db/table bindings after path/hash/code-unit/range/role validation; TypeScript Program/TypeChecker, Language Service, CodeQL, abstract-analysis workers, and broad JS/TS provider-backed families remain deferred. |
| Source snippets | Explicit opt-in only | Default output is metadata-only; bounded source spans require explicit CLI/MCP opt-in and hash checks. |
| Token savings | Not claimed by default | Only paired baseline/treatment experiments may report measured savings. |

Django, C/C++ semantic resolution beyond the bounded structural preview (no
build/compiler/preprocessor execution, no macro expansion, no compilation
database execution, no moc/protoc, no points-to, no class-hierarchy dispatch),
whole-program Python call graphs, sound full Python semantic analysis, and
default runtime tracing are deferred.
The Python Django/Flask/unittest/click/typer/Celery slices are ADR-0019
bounded preview anchors gated by the same exact-import discipline as FastAPI
and SQLAlchemy: framework identity requires the base or decorator receiver to
resolve to an exact framework import binding, name lookalikes without the import
stay `UNKNOWN`, and each family still requires at least three
complete-link-compatible derived support facts and no claim-relevant blocking
`UNKNOWN`. These previews do not evaluate Django `settings.py`, reverse URLs,
model middleware order, or resolve Celery task-queue routing, and they do not
change the ADR-0011 v0.1 FastAPI/pytest/SQLAlchemy/Pydantic focus statement.

C/C++, whole-program Python call graphs, sound full Python semantic analysis,
and default runtime tracing are deferred.

## Non-goals

- No cloud service dependency.
- No local LLM, embedding model, vector database, or remote API.
- No global database for repository-derived family facts, evidence, source
  hashes, freshness metadata, or repository paths.
- No automatic modification of user business code from pattern-family results.
- No top-level v0.1 `callers`, `callees`, `impact`, `affected`, `node`, or
  `explore` commands.
- No production-readiness claims outside the scoped v0.1 Python family/read-plan
  contract, and no token-savings claims until measured evidence exists.
- No mandatory CodeGraph dependency. CodeGraph may be considered only as an
  optional lower-layer provider, not as RepoGrammar's product identity.

## Result discipline

RepoGrammar must distinguish the four prevalence classifications
`DOMINANT_PATTERN`, `SUPPORTED_PATTERN`, `MINORITY_PATTERN`, and
`UNKNOWN_PREVALENCE`. Minimum support only qualifies a family for emission; it
does not imply dominance. Low confidence, competing families below minimum
support, incompatible targets, and dynamic runtime behavior must lead to
abstention (a typed `UNKNOWN`, never an emitted family) rather than certainty.
See `domain-model.md` for the `FamilyPrevalence` record and classification
rule.

A family is **selected** (`FOUND`) only when exactly one high-confidence family
resolves. When a fuzzy target resolves a **directory or composite scope** to more
than one in-scope family, RepoGrammar must never collapse the set into a single
guess or a generic `UNKNOWN`: it reports `PARTIAL_CONTEXT` and projects the
candidate-set cardinality through an additive `resolution` object
(`cardinality: one|many|none|truncated` plus bounded, source-free
`{family_id, summary}` candidates). A `many`/`none`/`truncated` resolution carries
**no** `selected_family_id` — the candidates are narrowing handles, not a claim.
The cardinality is expressed additively on `product-schemas.v1`, without a new
top-level status token (see ADR-0029 and `docs/specifications/query-resolution.md`).

Structural similarity may generate candidates, but it must not by itself prove
semantic family membership. Language-native semantic facts take precedence over
framework heuristics and syntax-only fingerprints. Syntax-origin framework-role
facts can record that a code unit has a recognizable framework role shape, but
`FRAMEWORK_HEURISTIC` certainty is not enough to prove family membership,
resolved handler identity, runtime lifecycle equivalence, or conformance.
Freshness is a required gate before semantic facts can become inputs to future
family claim builders. A fresh supported fact kind is still only eligible input;
it is not a `DOMINANT_PATTERN`, `VARIATION`, `EXCEPTION`, or conformance result
until EC-MVFI support, compatibility, and contrastive evidence checks are
implemented.
The current EC-MVFI-lite implementation is deliberately narrow: it can only
store a `DOMINANT_PATTERN` family when repeated compatible framework-role
candidates also have strong same-generation `SEMANTIC` or `DATAFLOW_DERIVED`
non-framework evidence. That support must be role-compatible: an arbitrary
semantic fact for an unrelated package, API, or framework cannot prove an
FastAPI, pytest, SQLAlchemy, Pydantic, Express, Jest/Vitest, Next.js, Fastify,
Prisma, Drizzle, or Java/Spring family. React components/hooks are currently
recognized only as syntax/framework-role shapes and stay `UNKNOWN`.
Otherwise family queries must return typed `UNKNOWN` rather than upgrading
syntax/framework heuristics into claims.
Family query output is selected rather than dumped wholesale. The default
compact mode returns family summary, members, variation slots, and unknowns
without evidence records or source snippets. All matched family modes return a
read plan that tells an agent which target, canonical, support, and
variation/exception spans to inspect by repo-relative path, strict content
hash, and byte range. The read plan reduces blind line-range expansion when
graph/navigation tools omit key function bodies, but it does not eliminate the
requirement to read target source before editing outside rendered ranges.
Explicit evidence/deep modes may return selected repo-relative evidence
metadata under a token budget. Explicit source-span opt-in may return bounded,
line-numbered, hash-checked spans selected from the read plan; stale or
unsupported spans are omitted with fallback guidance. The current selector uses
greedy marginal coverage over conservative metadata labels and reports missing
requested coverage instead of inventing unsupported variation or exception
evidence. The only current variation evidence link is
Python exact-compatible framework-anchor target diversity inside an already
ready family; exception evidence remains deferred. Deep mode remains
metadata-first unless source spans are explicitly requested.

`repogrammar stats --json` reports Python-family repo-shape diagnostics for
local pattern density, family support coverage, abstention rate, and
thin-wrapper/token-saving risk, plus token-saving readiness and concrete
blocking reasons. The JSON includes
`official_family_scope: python_v0_1` and
`repo_shape_scope: python_family_eligible_units` so these readiness diagnostics
are not confused with multi-language inventory. The separate
`indexed_inventory` counts active indexed files, indexed code units, and
semantic facts. Non-Python repositories can have nonzero indexed inventory
while Python-family `eligible_code_units` is `0`; that is an unsupported-scope
readiness result, not an indexing failure. These diagnostics explain when
RepoGrammar can reduce context acquisition cost and when third-party-heavy or
thin-wrapper repositories are unlikely to produce large savings. They are not
measured token savings or causal claims. Measured token savings are reported
only when a local paired baseline/treatment token experiment has comparable
token counts and a measurement source; otherwise stats must mark the
measurement kind as `ESTIMATED` and include a not-measured caveat.
Stats JSON also includes source-free `by_language` readiness buckets for the
official Python v0.1 scope and bounded TS/JS, Rust, Java, C#, and
C/C++ preview scopes, including indexed file/code-unit counts. These buckets keep
preview readiness, support risk, and optional UNKNOWN inventory counts separate
from the top-level Python-family readiness contract. If TS/JS code units are
indexed but no supported TS/JS family rows exist, stats must identify
`tsjs_family_support: none_or_unsupported`, keep
`react_rn_family_support: unsupported`, and recommend exact-path
`find`/`check` calls for `PARTIAL_CONTEXT` read plans. This does not add
React/RN family support or conformance.

`repogrammar status --json` and `repogrammar doctor --json` include a separate
source-free repository `readiness` object. This object is about whether
RepoGrammar can be used in the current checkout now, not whether any particular
family claim is supported. It reports setup states such as `not_initialized`,
`state_only_no_active_index`, `ready_active_index`,
`active_index_unhealthy`, `active_index_stale_or_unreadable`,
`autosync_recommended`, `autosync_active`, and `storage_unhealthy`; whether an
active generation is available; a recommended next command; and whether that
command needs user permission. It also reports local state hygiene for
`.repogrammar/` and foreign provider state such as `.codegraph/` without source
paths or source text. `.codegraph/` remains outside RepoGrammar ownership:
RepoGrammar may mention that it is present or accidentally tracked, but must not
create, initialize, modify, repair, or delete it.

Status and doctor obtain `recommended_next_command` from the same authoritative
application recovery classifier used by setup and query preflight. A missing
repository produces the `setup` action (legacy CLI compatibility may still
render `repogrammar init` until the setup command lands in that surface);
initialized state without a readable active generation recommends
`repogrammar resync`; unhealthy storage or a
blocking lock recommends `repogrammar doctor`; and an enabled but stopped
autosync service recommends `repogrammar autosync start` without making an
otherwise readable active index query-unready. Query-specific stale evidence
uses the classifier's resync action, while unverifiable or unsupported evidence
uses source fallback. Callers may format these actions but must not infer a
different action from raw readiness, freshness, family, or `UNKNOWN` fields.

Status and doctor JSON, and the MCP `inspect_readiness` operation, additionally
expose a decomposed `product_readiness` model from one application-layer
authority. RepoGrammar reports product capability as several independently
truthful dimensions rather than a single optimistic boolean: repository state,
active index (generation available and schema current), family-evidence freshness
(fresh/stale/cannot-verify counts reusing the bounded freshness machinery),
family prevalence by classification (or unreadable), query retrieval (exact and
term-retrieval modes with a vocabulary version), static alignment (available,
unavailable, not-applicable, or unreadable), per-slot providers, autosync, and the
NOT_MEASURED token-saving discipline. A store-read error yields no definite
dimension token: prevalence and the top-unknown list are reported as unreadable
(`null`) and static alignment as `cannot_verify`, never a false zero.

A single low-cardinality summary token — `ready`, `degraded`, or `not_ready` — is
a pure projection of the one combined recovery decision, which is derived from the
same authoritative repository recovery the query preflight consumes (and therefore
already incorporates the repository dirty-record freshness signal) layered with
the hash-checked family-evidence freshness. It is never more optimistic than that
authority: an unservable index is `not_ready`; a servable index that is stale
(family evidence stale/unverifiable, or the repository index carries dirty derived
records) or whose autosync is recommended-but-stopped is `degraded` with the stale
count visible; only a fully clean, fresh servable index is `ready`. There is no
top-level optimistic readiness boolean: a checkout whose index reports
`query_ready` while its family evidence is stale is `degraded`, not `ready`, and
`summary: ready` guarantees the query preflight is `Ready` on the same checkout —
one payload can never carry `readiness.query_ready: false` beside
`product_readiness.summary: ready`. The decomposition is a capability report only;
it is not a claim that any particular family, alignment, or token-saving result is
supported or measured. Assembling it performs bounded stats-scale reads, so like
`status`/`doctor` it is for readiness triage, not routine per-query agent loops.

The same authority also produces a bounded, source-free SCOPED readiness report
for one directory/module scope, exposed by `repogrammar doctor --target/--within`
and the MCP `inspect_readiness` scoped operation. It reuses the shared
target-resolution vocabulary and the bounded directory-scope read/family-mapping
ports to report, for that scope only, a resolvability verdict, indexed-file
coverage and count, languages present, the count of families whose evidence
occupies the scope (counted WITHOUT hydrating any family), a scope freshness token
projected from the same repository recovery authority, and one recovery action.
Its `summary` is projected from the same shared recovery as the whole-checkout
readiness, so a scope is never more optimistic than the repository. It is
source-free (it hydrates no family and reads no source content) and records no
family-query telemetry; every field is a low-cardinality enum, count, or language
token, with no raw target, path, or symbol in the output.

`UNKNOWN` is a typed result with reason codes and affected claims, not an
implementation failure by default. Some unknowns block specific semantic,
security, persistence, or conformance claims while still allowing weaker
structural observations. The canonical taxonomy lives in
`docs/specifications/unknowns.md`. `repogrammar unknowns --json` and
`repogrammar stats --unknowns --json` expose source-free aggregate inventory for
persisted semantic `UNKNOWN` facts. The inventory reports
`inventory_scope: persisted_semantic_unknowns`, stable recovery-code buckets,
role-state buckets, mechanism buckets, support-blocking buckets, and
readiness-scoped language detail for prioritizing provider and analyzer work;
reductions in those counts are diagnostic only unless false certainty is also
controlled.

## Installation and telemetry boundaries

Machine-level `install` and `uninstall` are separate from repository-level
`init`, `resync`, `autosync`, and `uninit`. Installer behavior must be
reversible, scoped, and dry-run friendly. Repository bootstrap is explicit:
`repogrammar init` creates repo-local analysis state and rebuilds or refreshes
the active generation, then starts repository-local auto-sync by default.
Users or agents use `--no-autosync` when authorization covers only a one-shot
index or when CI/experiment determinism forbids a background process.

End-user installation must be binary-first. Public-preview users should be able
to install and run the RepoGrammar CLI from a prebuilt release artifact without
Rust, Cargo, Node.js, npm, Docker, the SQLite CLI, local LLMs, embedding models,
or cloud API keys. Rust/Cargo remains a contributor and source-build dependency
only. The current Python preview still requires a `python3` interpreter at
indexing time for the bundled CPython AST worker asset; it must not require a
Python virtualenv or project dependency installation.
The npm package is an optional thin launcher for users who already have
Node/npm; it downloads and execs the same release binary and must not become a
JavaScript reimplementation of RepoGrammar.

Repository-derived analysis state belongs in the current repository's
`.repogrammar/` state directory, or the directory named by `REPOGRAMMAR_DIR`.
Global user state may contain installation receipts, binary/cache metadata,
anonymous telemetry preference, anonymous machine id, and non-repository-derived
runtime artifacts only.

Anonymous telemetry and research trace collection are separate consent
decisions. Anonymous telemetry is disabled by default, upload is explicit, and
environment opt-outs prevent network upload. Context compression metrics are not
actual token savings unless a comparable baseline and treatment token
measurement exist.
Live install keeps telemetry consent independent from agent configuration:
`--yes` never implies telemetry consent, `--telemetry` and `--no-telemetry` are
the explicit non-interactive choices, and product interactive installs prompt
with default-no `[y/N]` when no telemetry flag is supplied. Enabled
`stats --json` may update only a bucketed repo-local passive diagnostics
rollup; network upload remains limited to explicit `repogrammar telemetry
upload`.
Anonymous telemetry payloads must not include a repository instance id,
repository root hash, source path, symbol, content hash, byte range, raw target,
prompt, source snippet, or raw error. Experiment export is redacted by default
and reports token/count data only through coarse buckets.

RepoGrammar v0.1 first-class coding-agent integrations are Claude Code and
Codex. Both integrations use the same read-only `repogrammar_context` MCP
server through native agent CLI commands and RepoGrammar-owned receipts.
Interactive `repogrammar install` is a machine-level TUI-style wizard that can
wire Codex, Claude Code, or both in one run, skip already managed agents, and
add missing supported agents later. Multi-agent live install is
all-or-rollback, and `--target all --scope global --yes` uses that same
transaction. The installer may place the `repogrammar` command in a
user-writable command directory, but it must not index code, mutate
`.repogrammar/`, edit instruction files, upload telemetry, or run paired
experiments. Project-local live writes and instruction-file edits remain
deferred unless separately specified and tested. The install target registry may
recognize additional CodeGraph-style agents for dry-run and `--print-config`
planning, but recognized target ids are not live support claims until the
adapter has a reversible writer, ownership receipt, uninstall inverse, and
default tests.
