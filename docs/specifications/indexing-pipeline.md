# Indexing Pipeline Specification

The intended pipeline is:

```text
Repository files
-> File discovery and exclusion filtering
-> Tree-sitter parsing
-> Code-unit extraction
-> Unified IR
-> Language-native semantic facts
-> Normalization
-> Fingerprinting
-> Candidate discovery
-> Structural alignment
-> Anti-unification
-> Family construction
-> Pattern classification
-> SQLite persistence
```

## Bootstrap status

The repository currently defines module boundaries, semantic-worker protocol
placeholders, a safe repo-local lifecycle, a TS/JS file discovery substrate, a
Python `.py` discovery slice, syntax-only code-unit extractors, and
`index`/`sync`/`resync` wiring. The current CLI can discover TS/JS/Python/Java/C#/C/C++/Rust
files, read source through a hash-checked repo-relative boundary, store
repo-relative file metadata and structural code units in a building generation
inside the mutable `.repogrammar/repogrammar.sqlite` database, validate that
generation, and mark the corresponding `index_generations` row active. `index`
and `resync` do this as full rebuilds; `sync` attempts path-level incremental
copy-forward from the readable active generation and falls back to the full
rebuild path when project context, worker, schema, layout, or dirty-state
preconditions are unsafe. The
current default indexing path also stores syntax-origin `FRAMEWORK_ROLE`
semantic fact records for recognized TS/JS and Python framework-shaped code
units. These records use `FRAMEWORK_HEURISTIC` certainty and same-generation
code-unit evidence; they are candidate grouping facts, not family evidence by
themselves.
The current CLI can also discover `.rs` files and `Cargo.toml` manifests for
RepoGrammar self-dogfooding. Rust parsing uses Tree-sitter Rust to extract
structural modules, use items, structs, enums, traits, impl blocks, functions,
methods, test functions, and macro syntax. Cargo manifests are bounded
structural inventory only. The default safe Rust project-model substage can run
`cargo metadata --format-version=1 --no-deps` after `Cargo.toml` code units are
stored for the building generation, and it records only owned `PROJECT_CONFIG`
facts or recoverable provider `UNKNOWN`s. It never executes build scripts, proc
macros, rustc, rust-analyzer, rustdoc, or project code, and it cannot prove
symbol/type/call semantics or family support. Root `Cargo.toml` build-variant
ambiguity can block repository-wide Rust self-dogfood family support, but
nested fixture/package manifests must not globally block unrelated root Rust
families.
When `REPOGRAMMAR_TYPESCRIPT_WORKER` names an explicit worker executable,
`index`, `resync`, and full-rebuild `sync` fallback can also ask that worker for
facts about the discovered repo-relative TS/JS file set. The request includes
bounded operation scopes for literal module specifiers, exports, re-exports, and
package entries. Optional worker arguments come from
`REPOGRAMMAR_TYPESCRIPT_WORKER_ARGS_JSON` as a JSON array of strings, not a
shell command line. Accepted facts are recorded only when they match the same
building generation's indexed file, code-unit id, content hash, byte range, and
requested operation provenance. Incremental `sync` falls back to a full rebuild
when an explicit worker is configured.

Outside the internal Rust self-dogfood extractor and explicitly configured
semantic workers, this slice does not use Tree-sitter, call a TypeScript
compiler, perform full multi-view alignment, or anti-unify templates. It does
include
a conservative EC-MVFI-lite family builder that groups by language,
code-unit kind, framework role, and normalized shape, then applies a bounded
role-specific complete-link clustering pass over internal support-family
feature vectors so bridge members cannot single-link incompatible Python or
TS/JS evidence into one claim. It writes family rows only when each supporting
member also has compatible same-generation `SEMANTIC` or `DATAFLOW_DERIVED`
non-framework evidence. The current
Python path can synthesize `DATAFLOW_DERIVED` support facts in the application
layer from exact CPython structural anchors plus a single syntax-origin Python
framework role; raw parser facts and framework heuristics remain insufficient by
themselves.
A parallel internal Rust self-dogfood path can synthesize
`DATAFLOW_DERIVED` support facts from Tree-sitter Rust structural anchors only
for RepoGrammar-owned roles such as family gates, indexing phases, parser
adapters, CLI/MCP handlers, installer actions, storage validation, source-span
renderers, and product tests. Rust families require sufficient compatible
support, complete-link-compatible structural features, fresh same-generation
evidence, and no claim-relevant Rust `UNKNOWN`. Structural anchors alone,
generic Rust functions, unresolved module links, build-variant ambiguity,
macro/proc-macro expansion, trait-object dispatch, and stale evidence must not
be upgraded into confident family claims.
The Rust parser project context includes discovered Rust module paths and
bounded `Cargo.toml` file contents. External `mod` declarations can use that
module inventory to produce structural module-resolution context or typed
`UNKNOWN`s. Exact repo-local `use crate::...`, `use super::...`, and
`use self::...` paths can produce structural import context; complex repo-local
use trees remain `UnresolvedImport`, while external dependency uses are not
promoted into module proof. Checked-in Cargo manifests can add structural
package, edition, dependency, feature, workspace-member, explicit target, and
explicit crate-root metadata. Source-level `#[cfg]` / `#[cfg_attr]`
build-variant UNKNOWNs can use the nearest Cargo manifest to record simple
feature predicates and declared/undeclared feature state as assumptions. This is
only `cargo_feature_cfg_model` triage: RepoGrammar still does not evaluate cfgs,
select targets/features, or treat Cargo feature metadata as family support.
Family/query recovery text may summarize that feature state for agents without
changing the blocking UNKNOWN classification.
A parallel, deliberately conservative TS/JS path exists for Express,
Jest/Vitest (with Mocha and `node:test` exact named imports aliased onto the
same suite/test surface with a `runner_kind` provenance token), Next.js,
Fastify, Prisma, Drizzle, Zod exact schema builders, NestJS
controller/route/injectable/module decorators bound to `@nestjs/common`, and
Hono `new Hono()` receiver routes. The syntax
parser emits `STRUCTURAL` exact-anchor facts only when local framework-specific
bindings and file conventions match the adapter registry. NestJS decorators are
recognized by a bounded decorator-prefix scan in the syntax layer (the shared
class/method scanner extends each unit start over its leading `@Decorator(...)`
stack); route anchors additionally require containment inside an exact-import
`@Controller` class. Exact bindings may
come from ES imports, CommonJS `require`, or CommonJS destructuring aliases from
the exact supported package, but not from custom wrappers or injected clients:
Express app/router calls, exact literal `app.use("/prefix", router)` mount
context for subsequent exact router routes, Jest/Vitest runners, Next
App/Pages conventions with `next` package context, Fastify factory receivers
plus shorthand routes or full `app.route` declarations with literal method,
literal `url`/`path`, and an exact `handler` field, exact local
`register(plugin, { prefix: "..." })` plugin/prefix context that is not a
support target, local `new PrismaClient()` clients, and Drizzle table/db/query
bindings. Relative repo-local named Express/Fastify handler
imports and Prisma shared-client imports such as `import { prisma } from
"./db"` are provider-required candidates; Drizzle query anchors with relative
repo-local named `db` and table imports are provider-required candidates as
well. They do not enter the structural support path, and they may become
support only when a configured TypeScript worker proves every matching
`resolve_reexport` operation for the same path/hash/code-unit/range. Exact
local
Next dynamic segments, route
groups, and parallel routes are retained as context assumptions on page/layout
and route-handler anchors; middleware, server actions, re-exports, and
server/client semantics remain unsupported. Dynamic receivers, custom wrappers,
dynamic methods, dynamic Express router prefixes, middleware side effects,
conditional imports, external route handlers, missing route handlers, Fastify
imported plugins, plugin side effects, dynamic plugin prefixes, Fastify full
routes without literal `url`/`path` or handler fields, Prisma
bulk/raw/injected clients, and Drizzle raw/dynamic builders emit typed
`UNKNOWN` for the affected claim instead of support. The application layer
promotes accepted anchors to `DATAFLOW_DERIVED` support facts with engine
`repogrammar-tsjs-derived` and method `bounded_exact_anchor_v1`, carrying
`provider_resolved=false`, `derived_from=tsjs_structural_anchors`, a
framework-specific `derived_from=tsjs_<framework>_structural_anchors`,
`framework_role=<role>`, and `tsjs_anchor_kind=<kind>` assumptions. The
family-support gate accepts only exact recognized targets (for example
`express.route.get`, `jest_vitest.describe`, `next.route.GET`,
`fastify.route.route`, `prisma.query.findMany`, `drizzle.query.select`,
`drizzle.query.query_findMany`, or `drizzle.query.query_findFirst`) under the
`repogrammar-tsjs-derived` safe origin; it must not infer support from package
metadata, TypeScript semantic-worker facts, or fact text that merely contains a
framework name. TS/JS families require at least three compatible exact-anchor
support facts and use complete-link compatibility over route, runner,
component, response, query, and schema profiles. Bounded project inventory for
`package.json`,
`tsconfig.json`, `jsconfig.json`, and Jest/Vitest config files is structural
context only. Package dependencies and JSON `jest.config.json` /
`vitest.config.json` files can provide ambient test-runner context; script
configs such as `jest.config.ts` or `vitest.config.js` are not executed and
remain metadata/typed `UNKNOWN` only. The parser project context also builds a
bounded repo-local TS/JS module inventory plus safe JSON `paths` aliases and
`rootDirs` from `tsconfig.json`/`jsconfig.json`; safe `baseUrl` prefixes are
applied conservatively to alias targets before resolving against discovered
repo files. It can persist `STRUCTURAL` `RESOLVED_IMPORT` facts only for unique
literal relative imports, unique path-alias imports, or unique rootDirs
relative imports that resolve to discovered repo files. Direct repo-local
relative resolution wins before rootDirs fallback. Dynamic
`import(...)`, non-literal `require(...)`, conditional `require(...)`,
unresolved aliases/imports/rootDirs targets, conflicting alias or rootDirs
candidates, and `export *` stay typed `UNKNOWN`; those facts are
context/abstention evidence and do not become family support. Ambient
Jest/Vitest globals require bounded project test-runner
context from package or config inventory; otherwise they emit
`MissingProjectConfig` instead of support. React components and hooks remain
`UNKNOWN` in this slice, including when an external TypeScript semantic worker
emits React-shaped semantic support. TypeScript worker facts are bounded
semantic context by default, with one role-specific promotion path for exact
Next.js file-convention export identity and path-scoped promotion paths for
relative repo-local Express/Fastify route handler imports, Prisma
shared-client imports, and Drizzle db/table imports: after a configured worker
returns a TypeScript-provider
`resolve_export` fact for the same
path/hash/code-unit/range as a parser Next.js route/page/layout/API anchor, or
a TypeScript-provider `resolve_reexport` fact for the same
path/hash/code-unit/range as a provider-required Express/Fastify handler or
Prisma anchor with a matching named import such as `./handlers#listUsers` or
`./db#prisma`, or every required Drizzle named import such as `./db#db` and
`./schema#users`, the application layer may record a
`DATAFLOW_DERIVED` TS/JS support fact carrying `provider=typescript`,
`provider_resolved=true`, the
matching `query_operation`, the framework role, and the structural-anchor
provenance. Default worker operation planning now includes module specifier
operations for literal import/export/require facts, re-export operations for
bounded `export * from "<specifier>"` UNKNOWNs tagged as `<specifier>#*`,
export-identity operations for exact Next.js file-convention anchors, and
provider-required route-handler, Prisma, or Drizzle binding operations tagged as
`<specifier>#<export>`.
Provider-resolved TypeScript compiler facts may
suppress a matching parser-origin import/export `UNKNOWN` in the aggregate
UNKNOWN inventory only when the same path/hash/code-unit/range and operation
are proven; fallback facts and unresolved worker facts are not rewritten into
family support. If the checked-in worker cannot load a TypeScript compiler API,
its dependency-free static fallback facts remain `STRUCTURAL` or `UNKNOWN` and
carry `provider_resolved=false`. This is a bounded provider foundation, not full
TS/JS semantic analysis, and TS/JS remains a transitional substrate rather than the
official v0.1 target. A parallel Java/Spring preview path exists for
source-visible
Spring structural anchors. Discovery includes `.java` files. The Tree-sitter
Java adapter emits Java module/class/interface/method units, Spring MVC route
units, Spring component units, Spring Boot application units, Spring Data
repository units, structural anchors, and typed `UNKNOWN`s. Exact support is
limited to Spring annotations or repository types that are fully qualified in
source or imported from the expected Spring package: Spring MVC mapping
annotations, `@Controller`/`@RestController`, stereotype annotations,
`@SpringBootApplication`, `JpaRepository`, and `@RepositoryDefinition`. Simple
lookalike annotations without exact imports, custom composed annotations,
nonliteral route paths, route mappings outside exact controller classes,
missing project/classpath context, dependency injection, component scan, AOP
proxy behavior, repository factories, Maven/Gradle metadata, javac, and
annotation processors remain typed `UNKNOWN` or non-supporting context. The
parser emits non-blocking runtime subclaim `UNKNOWN`s for exact Spring
component scans, dependency injection, proxy semantics, nonliteral route paths,
and generated Spring Data repositories, while classpath and build-tool effects
remain unsupported context until a clean project/module-level representation
exists. Route path shape is conservative: only direct string literals and pure
literal arrays are `literal`; constants, identifiers, concatenations, and mixed
literal/nonliteral arrays are nonliteral route-path `UNKNOWN`s. The application
layer promotes accepted Java anchors to `DATAFLOW_DERIVED` support facts with
engine `repogrammar-java-derived` and method
`bounded_tree_sitter_java_anchor_v1`, carrying `provider_resolved=false`,
`derived_from=tree_sitter_java_structural_anchors`, the role-compatible support
family, and `framework_role=<role>`. Spring MVC route-family compatibility
requires matching `http_method` and `route_path_shape` evidence at minimum, so
GET and POST `@RequestMapping` handlers do not cluster merely because the
annotation name matches. Java/Spring families require at least three
complete-link-compatible support facts and no claim-relevant blocking Java
`UNKNOWN`; raw parser `STRUCTURAL` facts and `FRAMEWORK_HEURISTIC` role facts
are insufficient by themselves.

The Java parser (`adapters/parsing/java/`) also recognizes, under the same
exact-import/FQN gate, JUnit 5/4 and TestNG test methods, JPA/Jakarta Persistence
entities under dual `jakarta.persistence`/`javax.persistence` roots, and
JAX-RS/Jakarta REST resource classes/methods under dual `jakarta.ws.rs`/
`javax.ws.rs` roots, promoting them to `repogrammar-java-derived` support with
role-compatible targets (`junit.jupiter.Test`, `jpa.persistence.Entity`,
`jaxrs.ws.rs.GET`, ...). Test-family compatibility requires matching
`anchor_kind` and `test_annotation`; JPA-family compatibility requires matching
`support_family` and `jpa_namespace_root`, so jakarta and javax entities with
identical targets never cluster together; JAX-RS resource-method compatibility
requires matching `http_method` and `route_path_shape` like Spring routes.
Mockito test context, Lombok generated members (`MacroOrPreprocessor`), Spring
Data derived-query property paths, test-data providers outside a uniquely
provable same-class binding, and JPA runtime mapping remain typed non-blocking
`UNKNOWN`s; test,
JPA-entity, and JAX-RS annotation lookalikes, verb annotations outside a `@Path`
class, and mixed JUnit 4/5 `@Test` bindings are blocking `UNKNOWN`s. The Java
blocking-claim and copied-assumption policy tables live once in
`adapters/frameworks/java.rs` and are referenced by both the family gate and the
support-minting filter.

`adapters/parsing/java/test_data.rs` builds one bounded method/provider registry
per class-like body and performs deterministic logarithmic lookups instead of
rescanning the class per test. It emits a separate `STRUCTURAL` replacement fact
only when every direct exact JUnit `@MethodSource` entry is a bounded literal
name (scalar or `String[]`, including the blank/omitted same-name convention)
and the complete repeatable set identifies unique source-visible static methods
in that boundary. Exact identity for this link requires either an FQN or one
unambiguous explicit single-type import: wildcard imports, colliding imports,
local type shadows, malformed imports, and parse-open type inventories abstain.
An exact TestNG literal `dataProvider` may likewise identify one source-visible
exact `@DataProvider` in the same boundary; Java trivia around annotation
assignment names is parsed without joining identifier fragments.

External `class#method`/`dataProviderClass`, method signature selectors,
explicit repeatable containers or meta-annotations, type-level
`@ParameterizedClass` sources, inheritance, JUnit `PER_CLASS` non-static
factories, duplicate/overloaded names, dynamic values, unknown identity,
`@MethodSource` without `@ParameterizedTest`, missing targets, mixed
framework/data-source ambiguity, parse-degraded scopes, and nested-boundary
crossings retain typed `FrameworkMagic`, `UnresolvedImport`, or
`ConflictingFacts` evidence. Direct matching annotations are capped at 64 and
64 KiB of cumulative extracted segments; larger inputs abstain. These bounds
implement only the source-visible subset of the official
[JUnit 6.1.1 MethodSource contract](https://docs.junit.org/6.1.1/api/org.junit.jupiter.params/org/junit/jupiter/params/provider/MethodSource.html)
and [TestNG annotation contract](https://testng.org/annotations.html). No
compiler, build tool, annotation processor, lifecycle engine, or test code is
run, and replacement facts cannot promote family support.

A parallel C# preview path exists for source-visible ASP.NET Core, EF Core, and
xUnit/NUnit/MSTest structural anchors. Discovery includes `.cs` files and skips
the MSBuild `obj/` output directory (`bin/` is not skipped). The Tree-sitter C#
adapter emits module/class/method/property units, ASP.NET Core controller and
controller-action units, minimal-API route units, EF Core `DbContext` and
`DbSet` entity-set units, xUnit/NUnit/MSTest test-method units, structural
anchors, and typed `UNKNOWN`s. Exact support is gated on an attribute short name
backed by an exact Tree-sitter `using` in its compilation-unit/namespace lexical
scope (with or without an `Attribute` suffix) or an inline FQN; comments,
strings, sibling-namespace usings, `using static`, and alias usings never gate.
Lookalike attributes
without exact usings, `[Http*]` actions outside an exact controller,
unresolvable minimal-API receivers, MSTest methods outside a `[TestClass]`, and
`#if` conditional regions overlapping a unit are blocking typed `UNKNOWN`s,
while runtime DI, the filter pipeline, nonliteral route templates, convention
routing, partial-class/source-generator boundaries, and `dynamic` binding remain
non-blocking subclaim `UNKNOWN`s. xUnit `MemberData` is resolved only for the
bounded source-identity form documented by the current
[xUnit v3.2.2 API](https://api.xunit.net/v3/3.2.2/Xunit.MemberDataAttribute.html):
one or more exact attributes that each have one direct identifier string naming a unique,
unconditional, `public static` field, property, or zero-argument method in the
same non-partial, non-generic class with no explicit base. The resolver accepts
only an exact `Xunit.MemberData` FQN or lexical-scope `using Xunit`; it records the
source kind but never evaluates the member or claims row/signature compatibility.
`MemberType`, additional attribute properties, runtime source arguments,
`nameof`, escaped/verbatim/raw strings,
missing/private/instance/conditional sources, overloads, inheritance, partial
parts, generic or non-class containers, and unresolved attribute identity remain
`csharp_test_member_data` `UNKNOWN`. Tree-sitter ERROR/MISSING state on the
attribute, provider declaration, or containing class also prevents discharge.
Route-template shape is
conservative: only a single direct string literal is `literal`; interpolation,
`nameof`, concatenation, constants, and other expressions are nonliteral. The
application layer promotes accepted C# anchors to `DATAFLOW_DERIVED` support
facts with engine `repogrammar-csharp-derived` and method
`bounded_tree_sitter_csharp_anchor_v1`, carrying `provider_resolved=false`,
`derived_from=tree_sitter_csharp_structural_anchors`, the role-compatible
support family, and `framework_role=<role>`. Controller-action and minimal-route
compatibility requires matching `http_method` and `route_template_shape`
evidence. C# families require at least three complete-link-compatible support
facts and no claim-relevant blocking C# `UNKNOWN`. RepoGrammar never executes
MSBuild, Roslyn, source generators, or the ASP.NET Core runtime, and never
evaluates preprocessor conditions.

A parallel C/C++ preview path exists for include-evidence-gated GoogleTest,
Catch2, doctest, and Boost.Test registration-macro shapes. Discovery includes
`.c`/`.h` (parsed with the C grammar) and `.cc`/`.cpp`/`.cxx`/`.hh`/`.hpp`/`.hxx`
(parsed with the C++ grammar), skipping the CLion
`cmake-build-debug`/`cmake-build-release` output directories. The Tree-sitter
C/C++ adapter emits module/class/function structural units, GoogleTest/Catch2/
doctest/Boost.Test test-case and fixture/suite units, structural anchors, and
typed `UNKNOWN`s. Anchoring requires an exact framework-header path from a
Tree-sitter `preproc_include` node that is unconditional or protected solely by
a verified whole-file include guard (gtest/gmock, `catch2/`/`catch.hpp`,
`doctest/doctest.h`, `boost/test/`). Commented, string-contained, pseudo-,
computed, and non-normalized includes do not corroborate framework identity;
includes inside ordinary or nested conditional branches are never treated as
unconditional evidence. The same evidence gate applies to an exact
`::testing::Test`/`testing::Test` fixture base: without an exact unconditional
gtest/gmock include it remains a class plus blocking `UnresolvedImport`
`cpp_test_framework_identity`, never a GoogleTest fixture anchor. `TEST_CASE` is
disambiguated between Catch2 and doctest by include evidence, and
both/neither become a blocking `ConflictingFacts`/`UnresolvedImport`
`cpp_test_framework_identity`.

The supported macro contract is an audited 2026-07-16 structural snapshot, not
a claim that every framework release has identical signatures:

- current [GoogleTest testing reference](https://google.github.io/googletest/reference/testing.html):
  `TEST`, `TEST_F`, and `TEST_P` require exactly two valid C++ identifiers and
  neither identifier may contain `_`; `TYPED_TEST` remains the narrower claim
  of exactly two identifier-shaped arguments because that reference does not
  attach the same explicit no-underscore sentence to its `TYPED_TEST` entry;
- [Catch2 v3 test-case reference](https://catch2-temp.readthedocs.io/en/latest/test-cases-and-sections.html):
  `TEST_CASE` and `SCENARIO` accept one string-literal name plus at most one
  quoted, nonempty, adjacent square-bracket tag list such as
  `"[catalog][fast path]"`; free-form strings, unbalanced brackets, empty tags,
  separators/trailing text, and escaped/raw/prefixed forms are not decoded;
- doctest 2.x [test-case reference](https://github.com/doctest/doctest/blob/master/doc/markdown/testcases.md):
  this slice accepts only the one-string-literal `TEST_CASE` form; official
  decorator expressions and doctest `SCENARIO` remain explicit non-claims;
- Boost.Test's current primary documentation for
  [decorator syntax](https://www.boost.org/doc/libs/latest/libs/test/doc/html/boost_test/tests_organization/decorators.html),
  [dependencies](https://www.boost.org/doc/libs/latest/libs/test/doc/html/boost_test/tests_organization/tests_dependencies.html),
  [labels](https://www.boost.org/doc/libs/latest/libs/test/doc/html/boost_test/tests_organization/tests_grouping.html),
  [enablement/preconditions](https://www.boost.org/doc/libs/latest/libs/test/doc/html/boost_test/tests_organization/enabling.html),
  [descriptions](https://www.boost.org/doc/libs/latest/libs/test/doc/html/boost_test/tests_organization/semantic.html),
  and [fixtures](https://www.boost.org/doc/libs/latest/libs/test/doc/html/boost_test/utf_reference/test_org_reference/decorator_fixture.html):
  `BOOST_AUTO_TEST_CASE` and `BOOST_AUTO_TEST_SUITE` accept one identifier plus
  at most one structurally verified, fully qualified
  `boost::unit_test::<known-decorator>(...)` argument. The ordinary-call
  whitelist is signature-specific: `depends_on("...")`, `description("...")`,
  `label("...")`, `enabled()`, `disabled()`, `fixture(expr[, expr])`, and
  `precondition(expr)`;
  `BOOST_FIXTURE_TEST_CASE` accepts two identifiers plus at most that one
  decorator argument; `BOOST_AUTO_TEST_SUITE_END` accepts zero arguments.

Namespace-alias decorators such as `utf::label`, multi-decorator expressions,
the template-only `enable_if<condition>()` spelling, templated
`fixture<Fx>(...)` forms, doctest decorator chains, nonliteral or malformed
Catch2 tags, wrong decorator arities or required literal kinds, additional
arguments, and other macro/version shapes do not produce support. They become
blocking `MacroOrPreprocessor` `cpp_test_framework_identity` rather than letting a
lookalike macro erase `UNKNOWN`.

Boost.Test suite markers are validated in source order by an explicit linear
stack. Root cases are valid members of Boost.Test's implicit master suite;
properly nested scopes and matching ends are accepted. Orphan or invalid ends
make following scope identity ambiguous, and any suite still open at EOF blocks
the opener and registrations in that scope with `MacroOrPreprocessor`
`cpp_test_framework_identity`. Boost case family compatibility depends on the
exact case target and test-framework/macro evidence, not a suite name, so the
scanner deliberately validates scope without copying any suite identifier onto
a case. Tree-sitter `#if`/`#ifdef`/`#ifndef` regions,
including complex and unclosed regions, are blocking `cpp_build_variant` when
they overlap a unit. The only conditional exclusion is a standard guard that
truly wraps the file, has no alternative branch, and immediately defines the
same identifier with an empty object-like `#define`; ordinary partial or
value-defining `#ifndef` regions remain build variants. `#pragma once` is not a
conditional. Tree-sitter ERROR-node regions are blocking `cpp_macro_boundary`.
Qt `Q_OBJECT`/moc, string SIGNAL/SLOT dispatch, function-pointer dispatch, and
`compile_commands.json`/`vcpkg.json`/`conanfile.txt` project configuration stay
non-blocking subclaims or structural `PROJECT_CONFIG` inventory. The application
layer promotes accepted C/C++ anchors to `DATAFLOW_DERIVED` support facts with
engine `repogrammar-cpp-derived` and method `bounded_tree_sitter_c_cpp_anchor_v1`,
carrying `provider_resolved=false`,
`derived_from=tree_sitter_c_cpp_structural_anchors`, the role-compatible support
family, and `framework_role=<role>`. Test-role compatibility requires present-
and-equal `test_framework` and `test_macro` evidence. C/C++ families require at
least three complete-link-compatible support facts and no claim-relevant blocking
C/C++ `UNKNOWN`. RepoGrammar never runs a build, compiler, preprocessor, or
moc/protoc, and never expands macros.
The future provider-backed path is tracked in
`docs/plans/rust-tsjs-semantic-analysis-plan.md` and must use owned TS/JS
provider facts before widening these claims.
The syntax-only parser emits a lightweight RepoGrammar-owned IR consisting of one
node per code unit and conservative `contains` edges from module-like units to
contained units and classes/impls/traits to methods. Module-like units include
TS/JS modules, Rust file modules, and Rust inline modules. That IR is structural
only: it has empty payloads, does not infer calls or dataflow, and cannot prove
semantic or family claims.
Stored semantic facts, whether syntax-origin framework-role facts or explicitly
configured worker facts accepted by the storage gate, must still pass the claim
builder's support and compatibility rules before they become family evidence.
Compatibility is role-specific: unrelated semantic facts cannot satisfy
framework-family support just because they share a code-unit id, path, content
hash, and byte range.
Syntax-only code units are structural candidates, not semantic or family claims.
The `files` and `units` commands may read active file-manifest-only or
syntax-only index metadata for inventory/debugging, but that read path is not
family-query execution. The application layer can also load an internal
active-generation claim-input snapshot for claim builders after
revalidating files, code units, IR nodes/edges, semantic fact tokens,
assumptions, repo-relative evidence, hashes, and byte ranges. That internal read
path exposes only family-level CLI output; raw semantic facts remain internal.

## File discovery and exclusions

File discovery must respect repository ignore rules and RepoGrammar state
boundaries before parsing begins. RepoGrammar must skip `.repogrammar/` and
`.repogrammar-*` unconditionally, even when `REPOGRAMMAR_DIR` changes the active
state directory.

Discovery must honor `.gitignore` rules when Git is available and use a safe
warning fallback when Git checks are unavailable. When the project path is a
subdirectory of a parent Git worktree, discovery must resolve the Git top-level
and check ignore rules using Git-root-relative paths while still reporting
project-relative paths. `REPOGRAMMAR_STRICT_GITIGNORE=true` makes unavailable
Git ignore checks a hard indexing/discovery error instead of the normal warning
fallback. It must apply default exclusions for dependency, build, cache,
coverage, virtual environment, and generated output directories. Files larger
than the configured size limit are skipped, with 1 MB as the default inclusive
limit.

Repository discovery also has fixed inclusive aggregate limits: 100,000
accepted supported files, 512 MiB of accepted supported-file bytes, 100,000
reported skipped paths, 250,000 visited directory entries, and directory depth
256 with the repository root at depth zero. These are safety limits, not CLI or
environment configuration. Exact-limit input succeeds; the first plus-one
observation fails the whole discovery with a typed resource error and no partial
report. Walking charges visited entries before retaining them for deterministic
sorting, and every reported skip uses the same skip counter. For a readable,
supported, non-ignored regular file, accepted-file count is checked before the
aggregate accepted-byte total. Zero-byte files consume one accepted-file slot;
oversized, ignored, unsupported, unreadable, or symlink candidates consume
neither accepted budget. Checked arithmetic must fail closed.

The current path checks and aggregate limits do not prove confinement when an
attacker replaces a file or directory between canonicalization and reopen.
ADR-0023 defines the required future closure: after a no-follow root pin,
discovery must enumerate and reopen each child relative to retained directory
handles, treat entry metadata as advisory, and hash the bytes read from the
same no-follow regular-file handle used for metadata. Source-store reads and
autosync fingerprinting must migrate in the same implementation series, with
no ambient-path fallback. That decision and its candidate dependency are
preflight only; the current runtime still has the documented P2 gap.

`FileDiscoveryError::ResourceLimitExceeded` carries the stable resource token
(`accepted_files`, `accepted_bytes`, `reported_skipped_paths`,
`visited_entries`, or `directory_depth`), the inclusive limit, and the first
observed value that exceeded it. Its rendered error contains only the resource,
counts, and narrowing/exclusion guidance; it must not contain repository paths
or source. This public enum addition is intentionally additive in the pre-1.0
API, but downstream exhaustive matches must add the new variant. Indexing maps
it to invalid input and must fail before preparing or activating a generation.
Non-strict Git-unavailable warnings are deduplicated while walking from a fixed
two-message vocabulary rather than accumulated once per candidate.

The current discovery substrate supports `.ts`, `.tsx`, `.js`, `.jsx`, `.py`,
`.java`, `.go`, exact root Python `pyproject.toml`/`setup.cfg`/`setup.py`, root
or nested Go `go.mod`/`go.work`, and bounded TS/JS project-config files such as
`package.json`, `tsconfig.json`, `jsconfig.json`, `jest.config.*`, and
`vitest.config.*`.
Module-specific extensions such as `.mjs`, `.cjs`, `.mts`, and `.cts` remain
deferred until language-mode policy is defined. Discovery reports contain
repository-relative paths, language classification, strict
`sha256:<64 hex>` content hashes, file sizes, skip reasons, Git ignore status,
and warnings. They must not contain source snippets or absolute paths.
Repository-relative paths are lexical, slash-separated, non-empty paths; they
must reject absolute paths, Windows drive prefixes, backslashes, URI-like text,
control characters, `.`/`..` traversal segments, and empty path segments.

Skip reasons include RepoGrammar state directories, default excluded
directories, unsupported extensions, Git-ignored files, oversized files,
symlinks that are not followed, symlink escapes, paths outside the repository,
non-UTF-8 paths, and unreadable entries. Output ordering must be deterministic
by repository-relative path.

Optional `repogrammar.json` may configure language enablement, custom file
extensions, include/exclude patterns, framework adapters, and family thresholds.
Malformed configuration must warn and fall back to safe defaults rather than
failing indexing.

Python discovery currently discovers `.py` files and skips common Python
virtual-environment, cache, build, and dependency directories such as `.venv`,
`venv`, `env`, `.tox`, `.nox`, `__pycache__`, `.pytest_cache`,
`.mypy_cache`, `.ruff_cache`, `build`, `dist`, and `site-packages` without
executing repository code. General package-root discovery and provider-backed
project-configuration semantics remain deferred. The
implemented Python frontend uses CPython `ast` for code-unit extraction,
CPython `symtable` for structural scope anchors, and a private standard-library
project-config mode for sanitized root `pyproject.toml`/`tomllib`,
`setup.cfg`/`configparser`, and static `setup.py`/CPython-AST summaries that
default indexing persists only as structural config context or typed/
conservative config results. `setup.py` is never executed.
The frontend is invoked through `REPOGRAMMAR_PYTHON_EXECUTABLE` when that
environment variable is non-blank. Otherwise it defaults to `python` on Windows
and `python3` on non-Windows platforms so Conda and other Windows Python
installations are not shadowed by the Microsoft Store `python3.exe` app
execution alias. `REPOGRAMMAR_PYTHON_WORKER` may still override only the worker
script path.
Default parser-mode indexing now passes discovered repo-relative `.py`
inventory and sanitized root source roots from the three project-config parse
methods into private parse-document requests, so
source-tied unique repo-local imports can be persisted as `STRUCTURAL` parser
facts and ambiguous/missing imports can remain typed `UNKNOWN`s. Future slices
should add Tree-sitter as a tolerant structural fallback only. Python
syntax-only facts still cannot become semantic claims or family evidence by
themselves; only separately synthesized exact-anchor `DATAFLOW_DERIVED` support
facts or future provider-backed facts may enter the EC-MVFI-lite support gate.
Git ignore regression coverage includes ignored Python files in both repository
roots and parent-worktree subdirectory projects.

## Tree-sitter parsing

Tree-sitter may be used in parsing and language adapters. AST nodes must be
converted into `CodeUnit` and unified IR types before entering `core`.

The current implementation uses dependency-free syntax-only extractor adapters
as bootstrap parser boundaries. The TS/JS extractor recognizes modules,
functions, assigned arrow functions, classes, methods, React function
components, custom hooks, Express route calls, Next.js route/page conventions,
Fastify routes and plugin registrations, Prisma queries/transactions, Drizzle
schema/query/transaction anchors, and Jest/Vitest
`describe`/`it`/`test` blocks by structural syntax only. The Python extractor
uses a checked-in CPython `ast` worker and recognizes modules, functions,
async functions, classes, methods, FastAPI route-shaped functions, pytest tests
and fixtures, Pydantic model-shaped classes, SQLAlchemy model-shaped classes,
and SQLAlchemy repository method-shaped functions. Both extractors preserve
byte ranges, return diagnostics for parser errors, and store RepoGrammar-owned
`CodeUnit` metadata plus CodeUnit-derived IR nodes and conservative containment
edges. The Rust self-dogfood extractor uses Tree-sitter Rust for tolerant
structural extraction and typed UNKNOWN generation. No Tree-sitter node type is
stored in core, persistence, CLI, or MCP output.

Tree-sitter provides tolerant syntax and candidate generation. It is not
responsible for complete symbol, type, overload, alias, or module-resolution
facts.

Tree-sitter facts are structural evidence. They can participate in framework
role detection and candidate ranking, but they cannot independently prove
function identity, call targets, framework role semantics, type compatibility,
dependency-injection bindings, transaction semantics, authorization semantics,
or test fixture binding.

## Language-native semantic frontends

Language-native frontends provide project models, module resolution, symbol
resolution, type information, inheritance, and resolved calls where available.
The next official semantic-frontend design target is Python. Python analysis
should follow the claim-driven selective cascade in
`docs/specifications/python-analysis.md` and
`docs/decisions/ADR-0012-python-selective-analysis-cascade.md`: cheap CPython
syntax/scope/config facts first, Pyrefly only for plausible family candidates,
selective Pyright cross-checks for claim-upgrading facts, bounded role
propagation and call recovery, and compact evidence selection under token
budget.

Go is `discovered_only` and unsupported. Default discovery classifies bounded
`.go` inputs as `go` and root or nested `go.mod`/`go.work` as `go-config`, then
stores their repo-relative path, strict hash, size, and language token in the
normal file inventory. The full and incremental parser loops recognize both as
inventory-only before any source-store read, emit at most one deterministic,
path-free unsupported warning per token from the whole discovery report, and
store no Go code units, IR, semantic facts, framework roles, or families. This
preserves warnings for unchanged inventory and preserves metadata for non-UTF-8
Go content without interpreting or persisting its bytes. Go-only and empty
generations report `file_manifest_only` with `parser: deferred`; mixed
generations with parser-capable language tokens remain
`syntax_only_code_units` even when an unchanged incremental round performs zero
parser attempts.

While `go` and `go-config` remain inventory-only and absent from
`ParserProjectContext`, their add/modify/remove deltas use incremental metadata
persistence rather than project-context fallback. The token classification is
the sole exception authority: it requires zero Go source-store/parser calls and
filters every copied code unit, IR node/edge, semantic fact, derived-support
input, and recomputed family for current inventory-only paths. Only file
metadata may survive. The frontend/IR module must add Go inputs to
`ParserProjectContext` and restore token-based project-context invalidation
before any cross-file Go semantic or claim-bearing record is implemented.

`GoLanguageAdapter::classify_source_path` is the single current Go path-shape
authority. It accepts only normalized repo-relative paths and separates
inventory from future build eligibility by classifying dot/underscore path
components, `vendor`, `testdata`, `_test.go`, and the Go 1.26.5
`internal/syslist` known GOOS/GOARCH filename suffix snapshot without reading
ambient GOOS, GOARCH, tags, modules, workspaces, or toolchain state. The dated
list is discovery metadata only: unknown/future suffixes remain ordinary
inventory, and neither recognized nor unrecognized suffixes mean a build
configuration was selected. Generic discovery may retain ineligible candidates
except where an existing global exclusion such as `vendor` already applies;
end-to-end fixtures retain dot/underscore, `testdata`, and platform-suffix Go
candidates where that generic policy permits them. The classification cannot
support a family.

This slice intentionally does not scan source text for `//go:build`, generated-
file markers, `import "C"`, or `//go:generate`: ADR-0021 assigns those parser-
backed markers to frontend/IR because regex or line-prefix guesses would
overclaim structural facts. A future
version-pinned Tree-sitter Go grammar may produce syntax candidates, while
claim-supporting primary facts require an explicit, opt-in, sandboxed standard-
library worker over supplied inputs. Default indexing must not execute
`go/packages`, `go list`, gopls, cgo, or repository build/test/generate
commands. Build/environment/workspace selection, generated files, external
types, dispatch, cgo, and generators remain unimplemented obligations until
their atomic modules and the completion audit land. The current worker process
boundary remains insufficient for Go, and no discovered `.go` path may become
family support without the future sandboxed frontend and authoritative claim-
impact gate.

PHP is `discovered_only` and unsupported. One pure normalized repository-
relative classifier returns exact case-sensitive `.php` source, exact accepted
root/nested `php-config` basename, PHP-specific exclusion, or not-PHP.
Configuration classification precedes suffix matching. Exact `.composer` and
`.phpunit.cache` components receive `language_specific_exclusion` only for PHP
candidates and do not globally prune other languages; exact `vendor` retains
the existing global exclusion. Deferred `.inc`, `.phtml`, `.phpt`, `.php.dist`,
extensionless `artisan`, `composer.phar`, and `auth.json` shapes are not N1
inventory.

Full and incremental indexing treat `php` and `php-config` as inventory-only
before parser-facing source-store access. They persist only repo-relative path,
strict raw-byte SHA-256, size, and token, including bounded non-UTF-8 bytes, and
emit at most one deterministic path-free unsupported warning per accepted token.
PHP-only generations are `file_manifest_only`; mixed generations remain
`syntax_only_code_units`. While the tokens are absent from
`ParserProjectContext`, add/modify/remove deltas stay incremental and generation
copy-forward purges legacy PHP unit, IR, fact, support, evidence, and family
records while retaining file metadata. Discovery honors Git ignore; autosync
fingerprinting retains its generic Git-independent conservative charging. This
stage decodes/parses no source or configuration and creates no PHP unit, IR,
fact, `UNKNOWN`, family, project model, or readiness/support claim.

ADR-0024's frontend/project-model contract remains future work. The candidate
`mago-syntax` 1.43.0 frontend may enter only through a separately reviewed OS-
sandboxed worker after its dependency and artifact gates pass. Official PHP
8.5.8 `php -n -l` is the isolated syntax-validity oracle;
`nikic/PHP-Parser` 5.8.0 is the isolated AST/location differential and
separately qualification-gated fallback. Tree-sitter PHP 0.24.2 may generate
syntax candidates only. A future bounded project model may treat Composer JSON/
lock and PHPUnit XML as non-executing supplied data, but must never run Composer,
PHPUnit, autoloaders, plugins, scripts, repository PHP, or target dependencies.
No PHP source-store read, frontend request, or claim may occur until that model
applies project selection and validated custom vendor/cache prefix exclusions
over already discovered paths. Profile changes must reclassify affected paths
and purge claims. No PHP path may support the future exact
`php.phpunit.test_method` family before the ADR-0024 obligation registry,
fixtures, product wiring, and final completion audit land.

Swift is `discovered_only` and unsupported. One pure normalized repository-
relative classifier returns exact case-sensitive `.swift` source, exact
accepted root/nested `swift-config` basename, Swift-specific exclusion, or
not-Swift. Configuration classification precedes suffix matching. The complete
version-manifest grammar is ASCII `Package@swift-M[.m[.p]].swift`, with one or
more digits per component; a malformed lookalike that still has exact `.swift`
remains ordinary source inventory. Exact `.build` and `.swiftpm` components
receive `language_specific_exclusion` only for Swift candidates and do not
globally prune other languages.

Full and incremental indexing treat `swift` and `swift-config` as inventory-
only before parser-facing source-store access. They persist only bounded repo-
relative path, strict raw-byte SHA-256, size, and token, including non-UTF-8
bytes, and emit at most one deterministic path-free unsupported warning per
accepted token. Swift-only generations are `file_manifest_only`; mixed
generations retain `syntax_only_code_units`. While the tokens are absent from
`ParserProjectContext`, add/modify/remove deltas stay incremental and copy-
forward purges legacy Swift units, IR, facts, support, evidence, and families
while retaining file metadata. Discovery honors Git ignore; autosync retains
its generic Git-independent conservative charging. This stage decodes/parses
no source or configuration and creates no Swift unit, IR, fact, `UNKNOWN`,
family, project model, or readiness/support claim.

ADR-0025's frontend/project-model contract remains future work. SwiftSyntax
603.0.2 may enter only through a separately reviewed OS-sandboxed worker after
artifact, differential, dependency, five-target, and native-sandbox gates pass.
Exact Swift 6.3.3 SourceKit/compiler is only a separately qualified semantic
identity candidate. A future bounded project model may parse supplied SwiftPM
data but must never evaluate manifests or run Swift, SwiftPM, Xcode, builds,
tests, macros, plugins, generators, dependencies, children, or network access.
No Swift path may support `swift.xctest.test_method` before the ADR-0025
obligation registry, project model, fixtures, product wiring, reviews, and
completion audit land.

Ruby is `discovered_only` and unsupported. One pure normalized repo-relative
classifier returns exact `.rb` source, accepted root/nested `ruby-config`,
Ruby-specific exclusion, or not-Ruby. Exact configuration matching precedes
source suffixes, so `gems.rb` is config; literal `.rb` and `.gemspec` basenames
are accepted. Candidates below exact `.bundle` or `.ruby-lsp` components receive
`language_specific_exclusion`, but those directories are not globally pruned and
other languages retain their own policy.

Full and incremental indexing treat `ruby` and `ruby-config` as inventory-only
before parser-facing source-store access. They persist only repository-relative
path, strict raw-byte hash, size, and token, including for bounded non-UTF-8
content, and emit at most one deterministic path-free unsupported warning per
token from the whole accepted manifest. Ruby-only generations are
`file_manifest_only`; mixed generations remain `syntax_only_code_units` even
when an incremental round dispatches no parser. While the tokens are absent from
`ParserProjectContext`, add/modify/remove deltas remain incremental and
generation copy-forward purges any legacy Ruby unit, IR, fact, derived support,
or family while retaining file metadata. Discovery honors Git ignore; the
autosync fingerprint intentionally retains its generic Git-independent
conservative charging. This stage creates no Ruby unit, IR, fact, `UNKNOWN`,
family, project model, or support and never evaluates project files or selects an
ambient engine.

The later dependency-and-sandbox qualification stage is documentation/evidence
only. Production dependency/artifact admission and the sandboxed worker must
land separately before frontend/IR may use that accepted boundary. Before
spawning, a bounded project-model step may validate only the sole repository-
root `.ruby-version` exact `4.0.6` plus optional LF and pass normalized profile
metadata. Nested/absent/alternate/conflicting forms must become
`ruby_syntax_version` `UNKNOWN` after the registry lands; until then Ruby
semantic capability is unavailable. The worker receives only `.rb` source, not
raw executable configuration. It must restore project-context invalidation
before any cross-file Ruby claim-bearing record exists. Default indexing must
never run Ruby, Bundler, RubyGems, Rake, Rails, tests, generators, installed
gems, project tooling, child processes, or network access. No Ruby path may
become family support without the authoritative Ruby claim-impact classifier,
exact direct Minitest slice, support >= 3, source-free product wiring, review,
and completion audit required by ADR-0022.

The existing Rust-side TypeScript process adapter can validate NDJSON worker
output and translate facts into RepoGrammar-owned semantic facts. The
syntax-only `index` and `sync` path does not launch that worker by default. With
`REPOGRAMMAR_TYPESCRIPT_WORKER`, it may launch an explicit worker executable
with optional argv from `REPOGRAMMAR_TYPESCRIPT_WORKER_ARGS_JSON` and store
accepted facts only after evidence matches an indexed manifest entry, code-unit
id, content hash, and byte range in the same building generation.
Worker fallback statuses keep the generation syntax-only, while storage-gate
conflicts abort the new generation. Active-generation semantic facts can be read
back only through the storage/query claim-input snapshot for future claim
construction. The current query application boundary has an internal file-hash
freshness and readiness gate for snapshot semantic facts: stale or missing
source blocks the affected future claim input as `StaleEvidence`, unsupported
fact kinds or weak certainty block as `InsufficientSupport`, and conflicting
certainty blocks as `ConflictingFacts`. Raw semantic facts are not exposed
through CLI/MCP. The storage layer can persist generation-scoped family records,
members, variation slots, and family-bound evidence when the EC-MVFI-lite
builder supplies them. Default syntax-origin framework-role facts do not produce
those rows; an explicit semantic worker or future framework adapter must supply
stronger compatible evidence before a family is stored. Other languages should
use their own compiler, type-checker, or LSP where that is the most
authoritative source.
Not every stored semantic fact is worker-originated: syntax-origin framework
role facts may be recorded by the current TS/JS framework adapter with
`FRAMEWORK_HEURISTIC` certainty. Those facts remain blocked from family-claim
input as insufficient support unless the current claim builder can combine them
with stronger compatible evidence. The TS/JS syntax parser additionally emits
`STRUCTURAL` exact-anchor facts (engine `repogrammar-tsjs-syntax`, method
`exact_anchor_v1`) that, like the Python structural anchors, cannot support
membership by themselves; only the application-layer `repogrammar-tsjs-derived`
promotion can turn them into `DATAFLOW_DERIVED` support. It also emits typed
TS/JS `UNKNOWN` facts for dynamic route calls, unresolved or unsafe Express or
Fastify receivers, unsafe or unresolved Jest/Vitest runner bindings, unsupported
Next middleware/server action/re-export semantics, Prisma bulk, dynamic, raw,
or injected clients, Drizzle dynamic/raw or unresolved table/db bindings, and
bounded config parse/execution ambiguity. The
bounded import resolver additionally emits
structural repo-local import facts or typed `UNKNOWN` records for dynamic
imports, conditional or non-literal `require`, unresolved/conflicting
path-alias resolution, and ambiguous star re-exports. These facts remain blocked
from support and may only affect claim abstention, compatibility, or read-plan
guidance.

The checked-in Python worker currently has three bounded modes relevant to
indexing: private parse-document, private project-config, and semantic-worker-
compatible project analysis. Its private
parse-document mode is used by the Rust parser adapter to get CPython
`ast`-derived code-unit metadata without hand-written Python parsing. The
private parse-document boundary requires the exact request/response tuple
`protocol_version=1, contract_revision=1`. Full rebuilds and incremental
reparses use the same gate. The current host maps missing or different response
revisions and mixed installations where an older worker rejects the revision-
bearing request to typed `PythonFrontendContractMismatch`. It also classifies
the new worker's low-cardinality response to a legacy request as that typed
error in the subprocess composition test. A previously published host predates
the type and can report only a sanitized generic frontend/protocol failure when
the new worker rejects its legacy request; upgrading the host is required.
Current-host mismatches abort the candidate generation with only a rebuild/
reinstall recovery; they do not activate partial facts, reveal paths or
payloads, or turn the mismatch into an empty Python result. Default
indexing now passes the discovered repo-relative `.py` inventory, bounded
module file texts, sanitized root source roots from the matching
`pyproject.toml`/`tomllib`, `setup.cfg`/`configparser`, or static
`setup.py`/CPython-AST project-config path, and bounded, hash-checked discovered
`conftest.py` file contents into that private mode, letting the worker build a
bounded module, direct-symbol, package re-export, safe literal-star-import, and
fixture context for the current parse request. That worker pass produces
repo-relative structural fact payloads for ordinary imports, decorator anchors,
class bases,
Pydantic model-member anchors for fields, field annotation targets,
imported `Field(...)` metadata calls, `model_config`, nested `Config`,
dynamic `ConfigDict` UNKNOWNs, external-base UNKNOWNs, validator side-effect UNKNOWNs,
`computed_field`, validator, and `model_validator` declarations, SQLAlchemy
mapped field and relationship anchors, bounded `declarative_base()` class-base
bindings, local literal `relationship("LocalModel")` target context, typed
SQLAlchemy session calls including `add`, `execute`, `scalar`, `scalars`,
`commit`, and `rollback`, typed raw-SQL query-shape UNKNOWNs for direct SQL
strings or imported `text(...)` SQL text, bounded
`__init__`-assigned `self.session`/`self.db` receiver propagation with
same-method reassignment invalidation, runtime-injected untyped session receiver
UNKNOWNs, custom query-wrapper UNKNOWNs, SQLAlchemy event-listener UNKNOWNs,
dynamic SQLAlchemy model-class UNKNOWNs, simple calls, bounded same-function
FastAPI service-call context anchors,
`pytest.test` test-function anchors, graph-derived unique repo-local import
bindings, graph-derived direct imported `SYMBOL`/`TYPE` facts for top-level
class/function/module symbols, static `__init__.py` re-exports, literal-`__all__`
star imports, same-file pytest test and fixture dependency edges, unique
parent-directory `conftest.py` pytest fixture edges, literal pytest fixture
`name=` aliases, literal `request.getfixturevalue("name")` fixture lookups,
known pytest built-in fixture context,
FastAPI static
`response_model=...` schema-slot anchors, static `Depends(get_db)`
dependency-target anchors, literal `HTTPException(status_code=...)`
status-code effect anchors, static FastAPI `Body`/`Path`/`Query`/`Header`/
`Cookie` request-shape anchors, literal FastAPI/APIRouter
`include_router(..., prefix="...")` router-prefix context anchors whose stored
shape buckets do not retain the literal prefix text,
path-derived module names, CPython `symtable` scope anchors, and typed
dynamic/unresolved decorator, dynamic call, monkey-patch,
dynamic/unresolved/ambiguous import, dynamic include-router prefix or router
binding, unsafe star import without literal `__all__`, dynamic Pydantic model
factory, dynamic pytest fixture name, nonliteral `request.getfixturevalue`,
duplicate conftest fixture, plugin fixture, and fixture-injection `UNKNOWN`
cases. The semantic-worker-compatible
project mode can also resolve requested-project `conftest.py` fixture names
through pytest's directory hierarchy as graph-derived fixture-edge facts. The
Rust parser adapter validates and persists parse-document payloads as internal
`STRUCTURAL`, approved parser-origin `DATAFLOW_DERIVED`, or `UNKNOWN` semantic
fact records tied to the same code-unit evidence. Parser-origin graph facts must
carry `provider_resolved=false` and a `derived_from=repo_local_python_import_graph`
or `derived_from=repo_local_pytest_fixture_graph` assumption; unlabeled parser
facts remain blocked from support readiness as insufficient support. They may
enter the family builder only as context features or claim-scoped blocking
`UNKNOWN`s unless separately synthesized framework support facts pass the exact
compatibility table. Pydantic member/config/computed anchors are
schema/config/member context only, and FastAPI service-call anchors are
handler/service context only. FastAPI request body and request-parameter
anchors are route-shape context only; none of these categories synthesizes
family support facts.
Its private `parse_project_config` mode sanitizes exact root `pyproject.toml`,
`setup.cfg`, and `setup.py` through standard-library `tomllib`, `configparser`,
and CPython `ast` respectively. Default indexing discovers all three as
`python-config`, reads them through the Rust source-store path/hash/size
boundary, and persists `project_config` code units plus sanitized
`PROJECT_CONFIG`/`STRUCTURAL` records or typed/conservative config results.
`setup.py` is never executed; only complete literal project-name, `package_dir`,
and `find_packages`/`find_namespace_packages` source-root values from calls
lexically traced to direct, aliased, or qualified `setuptools` imports with no
recognized name, relevant attribute, or namespace mutation are accepted. Setup
must be a direct unconditional zero-positional module-body call with no keyword
unpacking and unique relevant keywords. `package_dir` must be a complete unique
string-to-string dict literal; a finder root must be its direct `packages=`
value and may use at most one literal positional-or-keyword `where`, never both,
with no unpacking. Local/helper or standalone/lookalike finder calls,
conditional/dead setup calls, shadowed/deleted/mutated bindings (including
builtins-qualified explicit mutation) abstain. A recognized call with a
computed/incomplete/duplicate/overridable relevant field, or after an
unconditional top-level `raise`, emits `MissingProjectConfig`; empty `setup()`
does not. Exactly one authoritative setup call is required;
multiple calls yield `ConflictingFacts`, while malformed syntax yields
`MissingProjectConfig`.
These records are structural context only, are not provider facts, do not
participate in family membership support, and stay blocked from claim-input
readiness. Roots from coexisting Python config formats are deduplicated only as
structural candidate parser context. This is not packaging/setuptools precedence,
and config conflicts cannot become or suppress strong family-claim evidence.
The worker's semantic-worker-compatible NDJSON mode can emit those structural
facts plus project-scope module-level repo-local import facts for unique safe
`.py` module matches, typed `UNKNOWN` for ambiguous/missing repo-local imports
and `sys.path` mutation, and conservative `FRAMEWORK_ROLE`/
`FRAMEWORK_HEURISTIC` facts for
Python framework-shaped units. The product indexing path does not launch a
Python semantic worker separately. Pyrefly, Pyright, provider-backed usage
propagation, cross-function call hierarchy recovery, and runtime observation
remain deferred beyond the current same-function structural service-call
context anchors.

The official v0.1 language scope is Python-first, focused on FastAPI, pytest,
SQLAlchemy, and Pydantic. The existing TypeScript/JavaScript path remains
transitional substrate until a later ADR re-promotes it.
The current Rust ports layer also defines future Python, Rust, and TS/JS
semantic-provider boundaries for candidate-scoped requests, provider provenance
assumptions, cache-key dimensions, and recoverable provider-unavailable
`UNKNOWN`s. Default indexing does not call a provider adapter. The application
layer now includes
an internal planner for validated Pyrefly `ResolveFrameworkIdentity` request
scopes over plausible Python family candidate groups. It skips parser-origin
blocking `UNKNOWN`s that affect Python framework identity, import resolution,
or pytest fixture binding for the planned claim, and it can read the same
validated active-generation claim-input snapshot used by query/family code.
It does not execute those requests, persist provider facts, or expose them
through CLI/MCP.
No Pyrefly, Pyright, RightTyper, or runtime-trace adapter is implemented.
The Rust Cargo metadata provider adapter is wired into the default product
indexing path as a safe project-model refresh stage for repositories with
same-generation `Cargo.toml` code units. It parses
`cargo metadata --format-version=1 --no-deps` output into owned
`PROJECT_CONFIG` facts, records provider `UNKNOWN`s when Cargo or project
configuration is unavailable, and does not execute build scripts or procedural
macros. These facts are context only: package metadata, targets, features, and
dependencies do not directly prove family membership.

## Optional providers

Optional providers such as a future CodeGraph provider may enrich candidate
retrieval, call/dependency context, or graph-neighborhood views after their
facts are translated into RepoGrammar-owned evidence. Provider facts must carry
provider provenance and freshness metadata. They cannot independently prove
pattern-family membership and must not be required for default indexing.

Unavailable, stale, or conflicting provider facts become auxiliary diagnostics,
typed `UNKNOWN`, or abstention for the affected claim.

## Code-unit extraction

Extraction identifies functions, classes, modules, tests, and framework-specific
units. Python v0.1 extraction should cover modules, functions, async functions,
classes, methods, decorators, imports, assignments, calls, annotations, class
bases, FastAPI route/dependency roles, pytest tests/fixtures, Pydantic models,
and SQLAlchemy model/session roles. Current stored TS/JS unit kinds are
syntax-only and include module, function, arrow function, class, method, React
component, React hook, Express route, Next.js App/Pages route/page/layout units,
Fastify route and plugin registration, Prisma query/transaction, Drizzle
schema/query/transaction, test suite, and test case.

## Normalization and fingerprinting

Normalization will remove incidental syntax differences that are not relevant to
pattern-family identity. Fingerprints will provide cheap candidate grouping
before expensive structural alignment.

## Candidate discovery

Candidate discovery will find possible analogues without claiming family
membership. Semantic compatibility filtering must run before family membership
is claimed.

The v0.1 mining design is Evidence-Constrained Multi-View Family Induction
(EC-MVFI). Tree-sitter syntax, language-native semantic facts, framework roles,
CFG/dataflow/effect views, API usage, and repository context are separate views.
Weak agreement may rank candidates, but family claims require compatible
source-backed evidence; unresolved or conflicting facts remain `UNKNOWN`.

## Alignment, anti-unification, and clustering

Structural alignment compares candidates. Anti-unification derives shared
templates and variation slots. Clustering groups aligned candidates into
families. These algorithms are deliberately deferred.

## Representative evidence selection

Representative selection is implemented only for query rendering over already
stored family evidence metadata. The selector uses deterministic greedy
marginal coverage per estimated token cost. Matched family queries also build
a read plan from stored family evidence. Read-plan items carry repo-relative
paths, strict content hashes, byte ranges, purpose labels, and estimated token
cost; they do not contain absolute paths. Source text remains disabled by
default. Before returning metadata-only output, the query layer attempts
hash-checked line-range enrichment for read-plan items. Fresh sources should
produce `start_line` and `end_line` without returning source text; stale,
missing, hash-mismatched, too-large, non-UTF-8, unavailable, or invalid ranges
must keep the item and add omission guidance. When the caller explicitly
requests source spans, the query layer renders only selected read-plan spans
through the hash-checked source-store boundary, fills line ranges for rendered
spans, and omits stale or unsupported spans with fallback guidance.
Family evidence records carry schema-backed `covered_claims` labels from the
allowlist `canonical`, `support`, `variation`, and `exception`; the current
builder emits `canonical` and `support`, plus a narrow Python `variation`
label when a ready family's exact-compatible framework-anchor support targets
differ. The builder may also emit metadata-only variation slots when
parser-context profiles differ inside an already-supported Python family, but
those slots do not imply variation evidence coverage. Requested exception
coverage and broader variation coverage are reported as missing until family
evidence is explicitly linked to variation slots or counterexamples. This
selector does not replace future medoid
selection, template induction, or exception mining.

## Framework adapters

Initial Python v0.1 framework adapters are scoped to FastAPI, pytest,
SQLAlchemy, and Pydantic. Framework rules belong in
`src/rust/adapters/frameworks/`.
The current framework adapter maps CPython AST-origin code-unit kinds for
FastAPI routes, pytest tests/fixtures, Pydantic models, SQLAlchemy models, and
SQLAlchemy repository methods into syntax-origin `FRAMEWORK_ROLE` fact records.
The ADR-0019 wave E1 bounded preview adds Django models/url-patterns/tests,
Flask routes, stdlib `unittest` test methods, click/typer commands, and Celery
tasks (`framework:{django,flask,unittest,click,typer,celery}.*` roles). All of
them reuse the same `repogrammar-python-derived` / `bounded_ast_anchor_v1`
`DATAFLOW_DERIVED` support derivation, support>=3 gate, and typed-`UNKNOWN`
boundaries as the official adapters; they are preview scope and do not change
the v0.1 focus statement.
Python framework compatibility must use typed canonical identities and explicit
compatibility tables, never framework-name substring matching. The current
EC-MVFI-lite gate requires at least three Python members plus stronger
compatible same-generation semantic/dataflow support before storing a Python
family; framework heuristics alone stay `UNKNOWN`.

The current lightweight TS/JS adapter maps syntax-only code-unit kinds for
Express routes, React components, React hooks, Jest/Vitest suites/tests,
Next.js conventions, Fastify routes, Prisma queries/transactions, and Drizzle
schema/query/transaction anchors into syntax-origin `FRAMEWORK_ROLE` fact
records. Fastify plugin-registration code units are context-only and do not map
to a route-handler role. The adapter records
repo-relative evidence and unresolved-binding assumptions. For the conservative
v0.2 path, the parser also emits structural exact-anchor facts for those
framework adapters; the application layer may derive `DATAFLOW_DERIVED` support
from those exact anchors. It still does not perform TypeScript compiler-backed
binding/export propagation, React runtime behavior, Next server/client or
middleware semantics, Fastify plugin side effects or dynamic prefixes,
Prisma/Drizzle runtime extensions, dependency injection, or lifecycle
semantics. Exact local Next
dynamic segments, route groups, and parallel routes remain context assumptions
on accepted file-convention anchors rather than blocking those anchors by
themselves.

The current Rust adapter maps only RepoGrammar's own repository structure into
internal self-dogfood roles. It records structural anchors and typed UNKNOWNs
for signature shape, visibility, arity, return kind, attributes, test shape,
bounded Cargo dependency inventory, safe repo-relative module declarations,
Cargo/build variants, unresolved or conflicting external modules,
macro/proc-macro expansion, and trait-object dispatch. Those facts are bounded
evidence for RepoGrammar self-dogfood only; they are not provider-backed Rust
semantics and do not imply general Rust target-language support. Cargo build
scripts and target-specific sections in the root manifest are repository
build-variant UNKNOWNs that block affected Rust self-dogfood family claims until
resolved; nested fixture/package manifests remain package/claim scoped and must
not globally block unrelated root Rust families. The indexer records manifests
without executing Cargo or build scripts.

Beyond the self-dogfood roles, the Rust adapter also recognizes general
framework anchors in any repository via the `repogrammar-rust-syntax` engine and
promotes them under the shared `repogrammar-rust-derived` /
`bounded_tree_sitter_anchor_v1` support path (`RUST_MIN_FAMILY_SUPPORT` stays 3,
family ids `family:rust:<kind>:*`). The new code-unit kinds are `serde_model`,
`thiserror_error_enum`, `tokio_entry`, `tokio_test`, `clap_parser`, and
`axum_route`, each gated by same-file `use`-path evidence or an inline
fully-qualified path. Framework roles are `framework:serde.model`,
`framework:thiserror.error`, `framework:tokio.entry`, `framework:tokio.test`,
`framework:clap.parser`, and `framework:axum.route`, with support targets
`serde.Serialize`/`serde.Deserialize`, `thiserror.Error`, `tokio.main`,
`tokio.test`, `clap.Parser`/`clap.Subcommand`/`clap.Args`, and
`axum.routing.route`. serde evidence-pair compatibility requires a present-and-equal
trait/target profile; axum requires a present-and-equal HTTP method and literal
path shape; thiserror/clap/tokio require an equal support family. The
derive/attribute macro expansion stays a non-blocking `rust_derive_expansion`
subclaim, axum tower middleware and extractor semantics stay non-blocking
subclaims, and derive-without-use plus non-literal/untraceable axum routes are
blocking typed UNKNOWNs. The adapter never expands derive/attribute macros,
resolves traits, or performs points-to analysis.

## Classification

Classification must produce dominant pattern, variation, exception, or unknown
with evidence and freshness checks. The current EC-MVFI-lite implementation can
produce `DOMINANT_PATTERN` only for repeated compatible candidates backed by
strong semantic/dataflow support; otherwise query output remains typed
`UNKNOWN`.
FastAPI route decorator targets can become derived support only when they
exact-match the canonical route-method table:
`fastapi.FastAPI.{delete,get,head,options,patch,post,put}` and
`fastapi.APIRouter.{delete,get,head,options,patch,post,put}`. Generic
`api_route` and WebSocket decorators are not v0.1 support targets. Static
`response_model=...`, static `Depends(get_db)` dependency-target, `Depends(...)`,
and `HTTPException(...)` parser anchors, including literal status-code effect
anchors, plus static FastAPI request body, request-parameter, and literal
`include_router` prefix anchors, stay schema/context/effect metadata and do not
prove membership support. Dynamic FastAPI dependency target expressions become
`RuntimeDependencyInjection` `UNKNOWN` for `fastapi_dependency_target`; dynamic
include-router prefixes or unresolved/external router bindings likewise stay
typed `UNKNOWN`. These do not erase route-shape evidence and do not become
family support. Canonical
`pytest.mark.parametrize` decorator anchors can support pytest test families,
but `pytest.parametrize.<name>` argument anchors remain context metadata and do
not prove support. Known pytest built-in fixture context targets such as
`pytest.builtin_fixture.tmp_path`, dynamic fixture-name UNKNOWNs, duplicate
conftest fixture UNKNOWNs, and plugin-style fixture UNKNOWNs remain
context/abstention metadata and do not prove support. Pydantic field,
field-type, `Field(...)`, `model_config`, nested `Config`,
computed-field, field-validator, legacy validator, and model-validator anchors
likewise stay model schema/config/member metadata and do not prove membership
support; dynamic Pydantic `create_model(...)` factories remain typed UNKNOWNs
instead of static model support. SQLAlchemy raw SQL query shapes remain typed
UNKNOWNs even when the session receiver is context-resolved. FastAPI service-call anchors stay
handler/service context metadata and also do not prove membership support.

`UNKNOWN` classifications and sub-claim unknowns must use the taxonomy in
`docs/specifications/unknowns.md`. Unknowns caused by dynamic imports, monkey
patching, pytest fixture injection, runtime dependency injection, macro or
preprocessor ambiguity, stale evidence, conflicts, or insufficient support must
remain visible to query and MCP callers.

## Sync and freshness

The baseline indexing model remains explicit: `init`, `index`, `sync`,
freshness warnings in `status`, and freshness checks before query or MCP
claims. Optional repository-local auto-sync can be enabled with
`repogrammar autosync start`. Auto-sync is not required for correctness, is not
started by MCP serving or agent installation, and does not scan repositories
that have not explicitly initialized RepoGrammar state.

The current auto-sync worker is conservative and reuses the normal `sync` path
after detecting a changed lightweight supported-file metadata fingerprint and
debouncing file changes. The detector avoids reading source contents during
idle polling; the subsequent `sync` remains authoritative for content hashes,
Git-ignore enforcement, parser/provider facts, incremental-versus-fallback
decisions, freshness, and active-generation activation. The detector uses the
same visited-entry, accepted-file, accepted-byte, and directory-depth budgets
as discovery and fails closed before retaining an over-limit directory entry or
fingerprint record. Although it hashes path, size, modification-time, and
language metadata rather than content, accepted-byte accounting uses each
supported file's metadata size to align its admission units with indexing.
Polling deliberately does not run Git-ignore checks: supported Git-ignored
candidates therefore count toward the fingerprint file/byte budgets, so
autosync can conservatively refuse a repository that manual Git-aware discovery
would accept. The subsequent `sync` remains the authoritative Git-aware path.
The reported-skipped-path budget does not apply because the fingerprint emits
no skip report. Incremental `sync`
copy-forwards unchanged active records into a new building generation only
after the project-context gate passes; changes to parser project-context source
inventories such as TS/JS, Python, or Rust source files fall back to a full
rebuild. Current inventory-only Go, PHP, Ruby, and Swift source/config tokens
are the explicit exceptions described above. When safe, incremental `sync`
reparses added or modified paths, omits
removed paths, and recomputes local derived support and families before
validation. Derived-support facts (including
provider-resolved TS/JS support) are excluded from copy-forward and recomputed;
the recomputation includes the provider-resolved support derived from the
copied-forward worker facts, so a worker-less incremental `sync` preserves the
base generation's provider-resolved family support for unchanged files instead
of silently dropping it and diverging from a full rebuild. Lazy query-time
recomputation remains future work.
