# C# language completion review

- Language / Top-20 rank: C# / 5
- Review baseline: `8a4033e3eddc93787c997d13c54bba22b1df7102`
- Current product label: `bounded_v0_2_preview`
- ADR-0020 completion state: Incomplete — bounded structural preview under
  convergence audit
- Counted as Top-20 complete: **No**
- Last updated: 2026-09-04

C# has exact ASP.NET Core and xUnit structural family evidence, but no C#
Roslyn provider, no `.csproj` discovery admission, and no full fixture
and source-free readiness matrix. It is not a completed Top-20 lane. A
bounded `.csproj` NuGet manifest reader now exists at the parser layer
(mirroring the `.vbproj` contract), but discovery does not yet admit
`.csproj` files, so it produces no product-path evidence yet.

## Capability snapshot

| Area | Current evidence | Completion boundary |
|---|---|---|
| Discovery | `.cs` selects C#; `.csx`, `.csproj`, and `.razor` do not. The generic discovery path excludes `obj` and limits source input to 1 MiB. `CSharpLanguageAdapter::classify_path` now separates exact `.cs` source from `.csproj` project paths as a ready registry seam, but discovery admission of `.csproj` is not wired. | `bin` is not globally excluded, and no SDK project, target framework, language version, source set, generated-source root, or build configuration is selected. |
| Dialect / variant | Tree-sitter C# `0.23.5`; structural conditional-region handling and typed uncertainty. | No C# language version, target framework moniker, SDK, symbols, nullable context, preprocessor configuration, or generated-code profile is authoritative. |
| Frontend / provider | In-process Tree-sitter syntax analysis with exact using/FQN checks. | No Roslyn, MSBuild, dotnet, source generator, NuGet, or C# provider slot is admitted or executed. |
| Owned code units / IR | RepoGrammar-owned structural units and IR for classes, methods, controllers, routes, DbContext/DbSet, tests, fixtures, and same-class xUnit MemberData helpers. | No authoritative type/overload/inheritance resolution, partial-type merge, external assembly symbols, generator output, DI graph, reflection, or runtime routing. |
| Project model | Parser-layer only: a bounded `.csproj` reader (`csharp/project_config.rs`) emits one `project_config` unit and a `csharp.csproj` structural PROJECT_CONFIG fact from supplied bytes, using the DTD/entity-free bounded XML reader. Not reachable from discovery yet. | SDK imports, MSBuild evaluation, conditions, properties/items, project references, configurations, target frameworks, generated files, and compile constants are unresolved; solution files and `Directory.Build.*`/`Directory.Packages.props` remain unread. |
| Package metadata | Parser-layer only: the bounded reader emits direct `nuget` `manifest_declared` rows for literal `Project/ItemGroup/PackageReference Include(+Version)` declarations (version-less rows keep an unclaimed requirement) and `csharp_dependency_inventory` typed UNKNOWNs for SDK/import/condition/update/override dynamics, conflicts, malformed XML, and resource limits — the `.vbproj` policy verbatim. No rows are persisted because discovery does not admit `.csproj`. | No central package management, lock/assets graph, selected version, transitive dependency, assembly identity, NuGet restore/resolution, or provenance. |
| Third-party analysis | Exact source using/FQN evidence supports bounded ASP.NET Core, EF Core, FluentValidation, xUnit, NUnit, and MSTest structural interpretations. | No package-to-symbol binding, external API ownership, selected framework/package version, compatibility, method contract, or reviewed library contract. |
| Exact families | Exact ASP.NET Core controller, FluentValidation validator, and xUnit family product tests have support at least three; MemberData has same-class bounded structural handling. | Minimal APIs, conventions, DI, inherited/partial/generated endpoints/tests, external data providers, dynamic dispatch, and runtime discovery are incomplete or UNKNOWN. |
| Typed `UNKNOWN` | Parse degradation, lookalikes/low support, partial/generated/dynamic behavior, conditional variants, DI/runtime/convention routing, and unsupported data-provider shapes abstain conservatively. | Obligations are incomplete for MSBuild/config selection, target frameworks, external assemblies, overloads, source generators, nullable/dynamic flow, reflection, and provider fallback. |
| Source-free readiness | Exact, lookalike, low-support, MemberData, and preprocessor-variant tests plus snippet-free family detail are reusable evidence. | The full C# CLI/MCP/status/doctor/stats/unknowns/persistence/leakage matrix and project/provider fallback coverage are incomplete. |

ADR-0030's shared dependency model does not itself create C# package evidence.
The bounded `.csproj` reader can emit `manifest_declared` NuGet rows, but until
discovery admits `.csproj` paths those rows are not produced by any product
path, and current source imports still cannot be promoted to package presence,
resolution, external-symbol authority, or reviewed library contracts.

## Four-part review record

### Correctness and bug findings

- Exact using/FQN checks and support thresholds make current controller/test
  families bounded and conservative; lookalikes and low-support cases abstain.
- Without `.csproj`/MSBuild evaluation, RepoGrammar cannot select compile items,
  target frameworks, constants, language/nullable settings, SDK-generated
  imports, or project references. The discovered `.cs` set can diverge from the
  actual build.
- `bin` is not globally excluded, so generated/copied `.cs` below that directory
  may be discovered. `obj` is excluded, but this is not a substitute for an
  evaluated project source set.
- Tree-sitter cannot merge partial types or resolve overloads, inheritance,
  extension methods, generators, dynamic/reflection, DI, or runtime routing.

### Security and untrusted-input handling

- Current analysis is non-executing: it does not invoke dotnet, MSBuild,
  Roslyn, NuGet, generators, builds, tests, application code, or network access.
- Discovery enforces repository containment, excludes `obj`, and caps source
  input at 1 MiB. Conditional and degraded parse states abstain rather than
  elevating unsupported claims.
- Tree-sitter parses untrusted input in process. The bounded `.csproj`
  reader is also in-process but non-executing: it uses the DTD/entity-free
  bounded XML reader, enforces depth/node/attribute/value and 2,000-record
  limits, emits only static fact notes, and never runs MSBuild, NuGet,
  `dotnet`, restore, or project code. Any Roslyn/MSBuild provider still
  needs a separate sandbox review covering filesystem/network/process access,
  SDK/import evaluation, generator/plugin execution, time, memory, output,
  diagnostics, and fail-closed fallback.
- No C# package/project parser currently expands the attack surface, but no
  complete language-specific product leakage matrix closes source-free
  readiness.

### Implementation completeness

Implemented work includes `.cs` discovery, Tree-sitter owned structural IR,
exact ASP.NET Core/xUnit families, additional EF Core/NUnit/MSTest roles,
same-class MemberData handling, release and unknown-reduction fixtures, and
conservative UNKNOWN behavior. A bounded `.csproj` NuGet manifest reader now
exists at the parser layer with its registry classification seam
(`CSharpLanguageAdapter::classify_path`), unit-tested against the `.vbproj`
inventory contract (literal declarations, version-less rows, SDK/conditional
dynamics, conflicts, malformed/lookalike XML, resource limits, determinism,
and source-free fact notes). Missing work includes discovery admission of
`.csproj` (still `UnsupportedExtension`), persisted product-path NuGet rows
with incremental copy-forward tests, the wider SDK/MSBuild project model,
target/profile selection, Roslyn provider, external symbol/type resolution,
partial/generator/dynamic convergence, complete fixture and readiness
matrices, and final linked audit.

### Performance and resource bounds

- Source files are capped at 1 MiB. Current analysis is in-process and does not
  fan out compiler/build/package processes.
- Direct class-member extraction scans members once; ordered maps provide
  logarithmic helper lookup, scoped data-provider context is shared, and merged
  conditional intervals use bounded ordered lookup. Dense-condition regression
  coverage exists.
- No benchmark closes very large solutions, multi-targeting, partial-type fan-
  out, generator output, metadata-reference scale, or MSBuild condition graphs.
  A future provider must bound projects/targets/sources/references, diagnostics,
  wall time, memory, descendants, and output.

## ADR-0020 nine-item completion gate

| Gate | Status | Evidence or exact gap |
|---|---|---|
| G1 Discovery/configuration | Partial | `.cs` discovery exists; a bounded `.csproj` NuGet reader and path classifier are implemented at the parser/registry layer, but discovery does not admit `.csproj` yet, and no target framework, language version, compile-item selection, or persisted project-config inventory exists. |
| G2 Authoritative frontend/provider | Open | Tree-sitter only; no admitted Roslyn provider or authoritative project/assembly context. |
| G3 RepoGrammar-owned units/IR | Satisfied | C# structural code/framework units and IR are RepoGrammar-owned. |
| G4 Typed `UNKNOWN` / fallback | Partial | Parse, partial/generated/dynamic, conditional, DI/runtime, and data-provider uncertainty exists; project/provider obligations are incomplete. |
| G5 Exact recurring family | Satisfied | Exact ASP.NET Core controller and xUnit families have support-at-least-three product evidence. |
| G6 Completion fixture matrix | Partial | Exact, lookalike, low-support, MemberData, preprocessor, and unknown-reduction fixtures exist; full project/target/generator/stale-conflict/provider coverage does not. |
| G7 Source-free readiness | Partial | Conservative family/product tests and snippet-free detail exist; the full C# public-surface and fallback matrix is incomplete. |
| G8 Four-part review | Satisfied by this snapshot | This document records findings and bounds; open findings remain blockers/risks and require tests. |
| G9 Atomic delivery/final audit | Open | The structural prerequisite exists, but no completion audit links and verifies all nine gates. |

Result: **3 satisfied, 4 partial, 2 open; not 9/9.** C# is not included in the
Top-20 completion count.

## Prerequisite commit evidence

- `497d5f6107598e5f2d2cc94b07990ec6d6f7671c` — C# bounded structural preview and exact family substrate.
- `9ea396b96ae11eb79ec9f80dd52b66e631c7dc1a` and
  `5ef354249b58784b1e872aa6ac3d83c7502dd4c8` — language-neutral dependency
  domain and persistence substrate, currently without a C# package consumer.

These are substrate prerequisites, not a completed C# atomic delivery chain.

## Primary evidence paths

- Authority: `docs/decisions/ADR-0019-bounded-multi-language-structural-expansion.md`,
  `docs/decisions/ADR-0020-top-20-language-expansion-gate.md`, and
  `docs/decisions/ADR-0030-language-neutral-dependency-library-semantics.md`.
- Plan/specification: `docs/plans/top-20-language-expansion-plan.md`,
  `docs/specifications/indexing-pipeline.md`,
  `docs/specifications/product.md`, `docs/specifications/domain-model.md`,
  `docs/specifications/semantic-workers.md`,
  `docs/specifications/unknowns.md`, and `docs/limitations.md`.
- Implementation: `src/rust/adapters/languages/csharp.rs`,
  `src/rust/adapters/parsing/csharp.rs`,
  `src/rust/adapters/parsing/csharp/project_config.rs`,
  `src/rust/adapters/parsing/csharp/test_data.rs`, and
  `src/rust/adapters/frameworks/csharp.rs`.
- Tests/fixtures: `src/fixtures/csharp/release/v0_2/`,
  `src/fixtures/unknown_reduction/csharp_aspnet_resolved/`,
  `src/fixtures/unknown_reduction/csharp_aspnet_unresolved/`,
  `src/rust/application/{indexing.rs,family.rs}`, and
  `src/rust/bin/repogrammar.rs`.

## Exact non-claims and remaining risks

- Do not claim C# completion, Top-20 inclusion, Roslyn semantics, compilation
  success, SDK/MSBuild project evaluation, authoritative source-set/target-
  framework selection, NuGet resolution, or generator/runtime behavior.
- The bounded `.csproj` reader is parser-layer evidence only until discovery
  admits `.csproj`: it has no product-path persistence, does not evaluate
  MSBuild/SDK imports/conditions/central package management, emits
  requirements rather than resolved versions, and its rows prove neither
  restore, installation, transitive closure, assembly identity, framework
  behavior, nor family support.
- Do not infer installed packages, selected versions, external assemblies or
  symbols, API contracts, library compatibility, or family membership from
  source using directives.
- Do not extend current ASP.NET/test families beyond their exact using/FQN,
  compatibility, and structural source scope.
- The highest correctness risks are unevaluated compile items/targets,
  partial/generated types, dynamic/reflection, DI, and runtime convention
  routing. They remain typed uncertainty or non-claims until authoritative
  bounded evidence exists.

## Completion verdict

**Not complete and not counted.** C# has bounded structural family evidence,
but no project/package model or authoritative Roslyn path and therefore does
not satisfy the 9/9 ADR-0020 gate.
