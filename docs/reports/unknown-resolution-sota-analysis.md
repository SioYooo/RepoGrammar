# UNKNOWN resolution provider analysis

- Status: Active baseline
- Last updated: 2026-08-01
- Scope: Provider mechanisms needed by the frozen Top-20 language program and
  third-party dependency/library semantics

## Decision rule

An analyzer, language server, compiler API, package resolver, or format parser is
not accepted because it exists. Admission requires an exact version/artifact,
license and acquisition review, a supplied-input or explicitly bounded project
contract, resource and isolation limits, owned output translation, provenance,
cache invalidation, typed failure behavior, and unresolved/resolved fixtures.
Syntax frontends generate candidates; they do not prove framework identity,
dispatch, runtime effects, or package selection.

## Current provider inventory

| Scope | Current state | Resolves now | Remains `UNKNOWN` |
|---|---|---|---|
| TypeScript compiler | integrated bounded worker operations | selected module/export/package-entry facts | full Program/TypeChecker graph, arbitrary external package symbols, dynamic loading/runtime behavior |
| Python type provider | registered, not integrated | none | third-party import/type identity, framework dependency graph, dynamic Python semantics |
| Rust analyzer | registered, not integrated | none | external-crate item identity, trait dispatch, cfg/macro/build-dependent semantics |
| Cargo metadata | bounded `--no-deps` project model | workspace/package/target/feature/direct manifest dependency records | resolved transitive graph, source/checksum selection, code symbols, build/proc-macro effects |
| C/C++ | no semantic provider | bounded Tree-sitter and project-config candidates | TU/header identity, Clang symbols/types/templates/macros/build variants |
| Java | no semantic provider | bounded Tree-sitter exact-import candidates | classpath/JAR symbol identity, compiler/processor/generated/runtime framework behavior |
| C#/VB.NET | no semantic provider | C# Tree-sitter exact-using candidates only | Roslyn symbol identity, references, partial/generated/dynamic/MSBuild behavior |
| Go/PHP/Ruby | discovery-only | source/config inventory | every parser, IR, symbol, family, and semantic claim |
| Swift | discovery-only plus bounded lock inventory | schema-2/3 SwiftPM identity/exact-version rows with unknown scope/directness | every source parser, IR, symbol, family, semantic, install/build/runtime, and direct-declaration claim |
| N2–N4 new languages | not started | none | dialect/frontend/project/dependency/family obligations pending individual preflight |

## Provider candidate routing

- Python: candidate-scoped Pyrefly/Pyright type/import identity, with no import or
  project code execution.
- JavaScript/TypeScript: a true bounded `Program`/`TypeChecker` operation with
  explicit JS/TS mode and one package-qualified symbol query.
- Rust: rust-analyzer or rustc-derived supplied-project operation that forbids
  build scripts and procedural macros.
- C/C++: Clang compilation-database request with exact language, dialect, target,
  defines, include profile, and TU identity.
- Java: javac or JDT with supplied sources and explicit classpath; no Maven,
  Gradle, annotation processor, or repository execution.
- C#/VB.NET: isolated Roslyn worker with supplied sources and explicit references;
  no MSBuild, NuGet restore, analyzer, or source-generator execution.
- Go/PHP/Swift/Ruby and every N2–N4 candidate: only the frontend and sandbox
  specified by its accepted preflight. No generic process adapter waiver.

## Third-party evidence ladder

1. A bounded manifest declaration inventories a package and version requirement.
2. A pinned lockfile parser may add a resolved version/source/checksum if the
   exact format provides it.
3. An isolated provider may establish package-qualified external-symbol identity.
4. A versioned reviewed library contract may establish a bounded behavior
   capability.
5. A family still requires exact repository source evidence, freshness, support
   at least three, and no blocking UNKNOWN.

No earlier rung can be renamed as a later rung. Unknown packages remain visible
inventory with unknown behavior. Provider absence is recoverable where an
integrated safe provider can be enabled; otherwise it is
`not_implemented_in_current_version` or the language-specific blocker.

## Cross-language failure taxonomy

- `provider_unavailable`: executable/runtime/config absent.
- `provider_not_integrated`: a documented slot has no production adapter.
- `sandbox_unqualified`: required isolation or target matrix is absent.
- `protocol_violation`: malformed, oversized, path-unsafe, or inconsistent
  provider output.
- `stale_or_conflicting_evidence`: hashes, tool/config fingerprints, lockfiles,
  or provider results disagree.
- `missing_project_config`: dialect, target, classpath, package root, or build
  variant cannot be pinned safely.
- `missing_dependency`: required analyzer/artifact/reference unavailable.
- `dynamic_or_runtime_semantics`: reflection, metaprogramming, dynamic loading,
  runtime DI/dispatch, generated code, macro/plugin/build execution, or ambient
  state is necessary.
- `insufficient_support`: fewer than three exact comparable members or incomplete
  required evidence.
- `source_evidence_insufficient` / `license_blocked`: candidate admission cannot
  be justified from authoritative redistributable evidence.

These outcomes are valid negative evidence. They must not be hidden by syntax
fallback, spelling heuristics, one positive fixture, or a prose-only support
claim.
