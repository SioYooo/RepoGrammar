# ADR-0019: Bounded multi-language structural expansion

- Status: Accepted (explicit maintainer direction, 2026-07-11)
- Date: 2026-07-11
- Refines: ADR-0011 (invokes its "unless a later ADR changes the sequence"
  clause for preview-scope additions without changing the official v0.1
  target), ADR-0016 (obligation mapping consequences for new-language
  UNKNOWNs), ADR-0017 (provider-slot vocabulary consequences)
- Related: `docs/reports/unknown-resolution-sota-analysis.md`,
  `docs/specifications/unknowns.md`, `docs/roadmap.md`,
  `docs/plans/multi-language-expansion-plan.md`,
  `docs/experiments/unknown-regression-benchmark.md`

## Context

The maintainer has directed RepoGrammar to complete bounded language support
for Java, C/C++, and C#, to minimize `UNKNOWN` emission as far as soundness
permits, and to cover the mainstream framework and third-party-library
landscape for the new languages and for the already-supported languages.

Current state:

- Python is the official v0.1 target (ADR-0011/0012) with FastAPI, pytest,
  SQLAlchemy, and Pydantic exact-anchor support.
- TS/JS has a conservative v0.2 exact-anchor preview (Express, Jest/Vitest,
  Next.js, Fastify, Prisma, Drizzle). React is excluded from family claims.
- Rust has an internal self-dogfood preview only; no general framework
  anchors.
- Java has a conservative Spring-only v0.2 preview slice
  (`bounded_v0_2_preview`) built on `tree-sitter-java` with exact
  imported/FQN anchors and typed `UNKNOWN` boundaries.
- C/C++ and C# have no support at all.
- ADR-0015 (provider-backed analyzer execution) remains Proposed. No
  language-native analyzer execution (javac/JDT, Roslyn, clangd/libclang,
  Pyrefly, rust-analyzer, tsc) is authorized by this ADR.

External evidence gathered for this decision (survey and registry data,
2025-2026; see the research citations recorded in
`docs/plans/multi-language-expansion-plan.md`):

- Java: Spring leads (~65% of Java developers), with JUnit (~85%), Mockito,
  JPA/Hibernate, Jakarta EE annotations, Lombok, and Jackson as the dominant
  library surface. Most anchors are exact annotation FQNs, base
  classes/interfaces, or method-name grammars — statically recognizable.
- C/C++: CMake ~83%, GoogleTest ~33% and Catch2 ~12% of C++ developers;
  `compile_commands.json` is the only sound no-execution project model;
  CodeQL's build-free C/C++ extraction (GA 2025) succeeds on >70% of
  repositories, demonstrating that a no-build structural slice is viable
  while ~30% degradation must surface as typed uncertainty, not guesses.
- C#: ASP.NET Core dominates (~62% of .NET developers), with xUnit/NUnit,
  EF Core, and attribute/base-class anchors throughout; CodeQL's build-free
  C# extraction (GA 2024) succeeds on >90% of repositories; Roslyn-style
  source-level analysis without MSBuild execution is the sound provider
  shape for a later ADR-0015 stage.
- Framework behavior that is runtime-defined (DI containers, component
  scanning, proxies, reflection, source generators, annotation processing,
  macro expansion, preprocessor variants) is exactly the irreducible or
  provider-gated tail already classified by
  `docs/reports/unknown-resolution-sota-analysis.md`.

## Decision

Adopt a bounded, structural, sound-by-abstention multi-language expansion.
All additions follow the Java v0.2 preview pattern: Tree-sitter structural
extraction, exact-anchor recognition gated by import/using/include evidence,
application-layer `DATAFLOW_DERIVED` promotion, support >= 3 complete-link
family gates, and typed claim-scoped `UNKNOWN` for everything unproven.

### D1. New preview languages: C# and C/C++

- Add C# (`.cs`) and C/C++ (`.c`, `.h`, `.cc`, `.cpp`, `.cxx`, `.hh`,
  `.hpp`, `.hxx`) as bounded structural preview languages with readiness
  scope `bounded_v0_2_preview`, alongside the existing Java preview.
- The official v0.1 language scope remains Python-first. The mirrored
  root contract statement is unchanged by this ADR. No new language may be
  described as an official target.
- Family ids use `family:csharp:*` and `family:cpp:*` prefixes; language
  tokens are `csharp` (+`csharp-config`) and `c`/`cpp` (+`cpp-config`).

### D2. Production dependencies

`tree-sitter-c-sharp`, `tree-sitter-c`, and `tree-sitter-cpp` are authorized
as production dependencies, mirroring the demonstrated-need precedent of
`tree-sitter-java`/`tree-sitter-rust`: they are the syntax and
candidate-generation layer for the new slices and never a semantic oracle.
No analyzer/compiler/LSP dependency is added by this ADR.

### D3. Framework-adapter program (usage x static recognizability)

Frameworks are added in waves, each wave shipping anchors, typed UNKNOWNs,
fixtures, and tests together. Initial waves:

- Java deepening (wave J1): JUnit 5 and JUnit 4 annotations, TestNG,
  Mockito annotations/extension, JPA/Jakarta Persistence entity annotations
  with dual `jakarta.*`/`javax.*` roots, JAX-RS/Jakarta REST resource
  annotations (dual roots), and Spring Data repository derived-query
  method-name grammar as structural metadata. Lombok annotations are
  recognized only as typed `MacroOrPreprocessor` UNKNOWN (compiler-AST
  synthesis is never simulated as certainty).
- C# (wave CS1): ASP.NET Core attribute-routed controllers
  (`Microsoft.AspNetCore.Mvc` attributes + `ControllerBase`/`Controller`
  bases), minimal-API literal `Map*` call shapes, EF Core (`DbContext` base,
  `DbSet<T>` properties), xUnit/NUnit/MSTest test attributes.
- C/C++ (wave C1): GoogleTest/GoogleMock, Catch2, doctest, and Boost.Test
  registration-macro shapes with include-evidence corroboration; Qt
  `Q_OBJECT`/signal-slot structural context with moc output typed UNKNOWN.
- Existing-language widening (wave E1): Rust general framework anchors
  beyond self-dogfood (serde/thiserror/anyhow derives, tokio entry
  attributes, clap derives, axum literal routes); Python additions
  (Django models/urls/views, Flask routes/blueprints, stdlib
  unittest.TestCase, click/typer commands, Celery tasks) as bounded preview
  anchors that do not alter the ADR-0011 v0.1 framework focus statement;
  TS/JS additions (Zod schema builders, NestJS decorators, Mocha and
  `node:test` aliasing onto the existing suite/test surface, Hono literal
  routes). React remains excluded.

Later waves (SignalR, FluentValidation, MediatR, Razor Pages, Jakarta
Servlet/CDI, Micronaut, Quarkus, MyBatis XML join, Drogon/Crow routes,
wxWidgets/GLib, Playwright, Mongoose/TypeORM, sqlx/tonic/tracing, Airflow,
marshmallow, and others) are enumerated with priorities in
`docs/plans/multi-language-expansion-plan.md` and follow the same rules; no
further ADR is needed per wave unless a rule below must change.

### D4. UNKNOWN mapping policy (no new reason codes)

New languages map their granular cases onto the existing 13 reason codes and
must document the mapping in `docs/specifications/unknowns.md` before public
output:

- C/C++: undischarged `#if`/`#ifdef`/`#elif` conditions →
  `BuildVariantAmbiguity` (with the guarding condition recorded as bounded
  assumptions, never evaluated); function-like/object-like macro expansion,
  token pasting, computed `#include`, and absent generated artifacts (moc,
  protoc, flatc outputs) → `MacroOrPreprocessor`; unresolvable
  angle-bracket/system headers → `MissingDependency`; absent or unreadable
  `compile_commands.json`/manifest context when a claim needs it →
  `MissingProjectConfig`; framework-macro lookalikes without include
  corroboration (for example `TEST_CASE` shared by Catch2 and doctest with
  no header evidence) → `UnresolvedImport` blocking framework identity, or
  `ConflictingFacts` when two frameworks' include evidence both match;
  function-pointer/virtual dispatch and template-instantiation-dependent
  semantics → `FrameworkMagic` scoped to dispatch/type claims.
- C#: Roslyn source-generator outputs, `partial` declarations whose other
  half is not in the checked-in file set, and generated gRPC/Razor
  artifacts → `MacroOrPreprocessor`; MSBuild `Condition` attributes and
  multi-targeting → `BuildVariantAmbiguity`; reflection, DI container
  resolution, assembly scanning, and runtime proxies →
  `RuntimeDependencyInjection`/`FrameworkMagic` per the Java/Spring
  precedent; attribute lookalikes without exact `using`/FQN evidence →
  `UnresolvedImport`; `dynamic`-typed member binding → `FrameworkMagic`
  scoped to the affected call claim; missing/unparseable `.csproj` context
  when a claim needs it → `MissingProjectConfig`; unresolved NuGet package
  interfaces → `MissingDependency`.
- Java deepening reuses the existing Java mapping; annotation-processing
  frameworks (Lombok, MapStruct, Dagger) → `MacroOrPreprocessor` for
  generated-member claims.

New required-mechanism buckets (for example `cpp_build_variant_model`,
`cpp_macro_boundary`, `cpp_compile_commands_model`, `csharp_project_model`,
`csharp_source_generator_boundary`, `java_test_annotation_model`,
`jpa_entity_model`) must be registered in `docs/specifications/unknowns.md`
and the telemetry vocabulary with tests, and every new reason-code usage
extends the deterministic ADR-0016 obligation mapping.

### D5. Sound no-execution project-configuration parsing

Authorized as structural `PROJECT_CONFIG` facts (never family support),
following the `pyproject.toml`/`Cargo.toml`/`tsconfig` precedent:

- C/C++: `compile_commands.json` (tier-1 ground truth; staleness and
  header-without-entry conditions are typed UNKNOWN), `vcpkg.json`,
  `conanfile.txt` dependency inventories. CMakeLists/Makefile/Meson parsing
  is deferred; their absence of modeling keeps affected claims typed
  UNKNOWN rather than guessed.
- C#: SDK-style `.csproj`, `Directory.Build.props`,
  `Directory.Packages.props` parsed as declarative XML with no MSBuild
  property-function evaluation; `Condition` branches are recorded as
  build-variant UNKNOWN, never chosen.
- Java: root `pom.xml` parsed as declarative XML for
  dependency/module inventory; Gradle build scripts (Groovy/Kotlin
  programs) stay typed `MissingProjectConfig`/config UNKNOWN, never
  partially evaluated.

### D6. Invariants that do not change

- No execution of builds, compilers, annotation processors, source
  generators, macro expanders, moc/protoc, package scripts, or repository
  code during `index`/`sync`/`resync`.
- Structural similarity never proves semantic family membership; anchors
  require exact import/using/include/FQN corroboration plus the per-language
  anchor-engine provenance gate.
- One authoritative per-language classifier for family-affecting UNKNOWNs;
  support >= 3 complete-link compatibility; exact whitelisted support
  targets; conflicts preserved, never voted away.
- Source-free output surfaces; readiness scopes and inventory buckets stay
  low-cardinality codes and counts.
- Every new mechanism ships an unresolved→resolved benchmark fixture pair
  proving a source-backed replacement fact while the unresolved side forms
  no family (`docs/experiments/unknown-regression-benchmark.md`).
- UNKNOWN-rate reduction is never reported as a quality improvement without
  false-certainty control.

## Alternatives considered

- Wire analyzer providers (javac/JDT, Roslyn, clangd) now to reduce UNKNOWN
  further: rejected — ADR-0015 is still Proposed; execution consent,
  isolation, and dependency-acquisition decisions are not in place. This
  ADR keeps the recoverable tail typed and mechanism-labeled so a later
  accepted execution ADR can convert it.
- Promote one of the new languages to official scope: rejected — Python
  v0.1 validation remains the product checkpoint; previews must not dilute
  the official claim.
- Preprocess C/C++ (pick one branch of each `#if`) or simulate C# source
  generators to shrink UNKNOWN counts: rejected — manufactures false
  certainty; variant conditions and generated-code absence are recorded as
  typed UNKNOWN instead.
- LLM/neural inference to resolve dynamic behavior: rejected — no auditable
  provenance.

## Consequences

- `Cargo.toml` gains three tree-sitter grammar dependencies; the parsing
  layer gains `csharp` and `c/cpp` adapters plus per-framework Java
  modules; the frameworks/application/query/persistence layers gain the
  corresponding role registries, derivation paths, gates, readiness scopes,
  and repo-shape whitelists.
- `repo-guard` `SOURCE_EXTENSIONS` gains `cs`, `cxx`, `hh`, `hxx`.
- Documentation cascade per language slice: `docs/specifications/unknowns.md`
  (claim registry + mechanisms), `product.md` (support matrix + non-claims),
  `indexing-pipeline.md`, `domain-model.md`, `metrics.md` (scopes),
  `storage.md` (language token list), `docs/roadmap.md`,
  `docs/development/testing.md`, README, CHANGELOG, and
  `.agents/memories/project-state.md`/`unknown-governance.md`.
- The UNKNOWN regression benchmark gains per-language fixture pairs and
  pinned bucket baselines; release fixture smoke gates gain per-language
  positive/negative/dynamic/low-support/stale coverage.
- Query surfaces (`stats`, `doctor`, `unknowns`) gain `csharp` and `cpp`
  readiness/inventory scopes with `preview_status: bounded_preview`.

## Follow-up work

- Execute the wave plan in `docs/plans/multi-language-expansion-plan.md`
  (CS1 → C1 → J1 → E1, then later waves), one atomic commit per coherent
  slice with tests and docs.
- When ADR-0015 (or a successor) is accepted, add per-language provider
  stages: Roslyn-style source-level C# binding, clangd/libclang C/C++
  configuration-scoped resolution, javac/JDT Java resolution — each behind
  its own dependency-acquisition decision, converting the
  mechanism-labeled recoverable tail into provider-backed facts.
- Extend ADR-0017 provider-slot vocabulary (`csharp_compiler`,
  `cpp_clangd`, `java_compiler`) as documented-not-integrated slots when
  the capability registry is next revised.
