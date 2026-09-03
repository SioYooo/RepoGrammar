# Top-20 UNKNOWN taxonomy and resolution matrix

Date: 2026-08-01. This report complements
`docs/reports/unknown-resolution-sota-analysis.md`. It maps program blockers to
the evidence that may discharge them. An `UNKNOWN` may disappear only when the
same affected claim gains fresh, stronger evidence; a lower count by deletion,
fallback, or reclassification is not progress.

## Cross-language taxonomy

| Mechanism class | Typical affected claim | Current evidence | Valid resolution | Forbidden shortcut |
|---|---|---|---|---|
| Dialect/version/profile | parse, symbol, family | extension/config candidate | pinned language/tool version plus coherent repository profile | infer dialect from suffix or ambient tool |
| Project/workspace selection | dependency/source scope | bounded manifest inventory | authoritative bounded root/source-set/workspace model | select first manifest or run executable DSL |
| Parser degradation | code unit/anchor | diagnostics/error nodes | clean authoritative parse or a claim-specific degraded-proof rule | treat partial AST as complete |
| Macro/preprocessor/generated code | anchor/symbol/dataflow | source-visible candidate | isolated compiler/provider result with inputs/config/provenance | regex expansion or ignore generated tail |
| Import/include/module resolution | external symbol/family | spelling/path candidate | package-qualified provider fact under selected project graph | name matching or manifest presence |
| Type/overload/dispatch | call/role/dataflow | structural receiver/call candidate | authoritative type/dispatch provider evidence | guess from identifier or method name |
| Dependency graph/coherence | package/version/directness | manifest or lock row | admitted effective model/lock join with conflict and freshness handling | label manifest requirement “installed/resolved” |
| Library behavior | framework role/call/dataflow | package and optional symbol identity | compatible exact-version reviewed contract plus exact anchor and provenance | infer behavior from dependency name |
| Runtime/reflection/plugins | route/test/DI/extension | source/config candidate | bounded trace or provider specifically authorized for that claim | execute repository/dependency code during indexing |
| Resource/protocol failure | all facts from request | bounded failure token | fresh complete within-limit request with exact version/hash/ranges | convert timeout/truncation/crash to negative evidence |
| Stale/conflicting evidence | family/readiness | multiple generations/providers | same-generation re-analysis or authoritative conflict policy | keep copied stale rows or choose first result |
| Binary/archive boundary | Scratch project/assets/extensions | disconnected stored-entry preflight | bounded binary-document port plus vetted ZIP/deflate implementation | treat ZIP as UTF-8 source or extract to ambient paths |
| External environment | SDK/catalog/toolchain/system libs | unavailable/not integrated | explicit immutable environment identity and isolation | use ambient cache, credentials, database, or PATH silently |

## Language resolution routing

| Language | Highest-impact UNKNOWNs | Current resolution state | Next evidence that would discharge them |
|---|---|---|---|
| Python | import graph, framework/type identity, dynamic fixtures | Python type provider `not_integrated` | pinned isolated Pyrefly plus selective Pyright conflict checks |
| C | translation unit, header language, includes, ABI | no Clang slot | selected compilation command plus isolated Clang identity query |
| C++ | translation unit, templates, includes, overloads, ABI | no Clang slot | pinned Clang/clangd provider under exact TU profile |
| Java | Maven effective model, classpath, processors, types | no javac/JDT slot | bounded effective-model input plus isolated javac/JDT symbol query |
| C# | MSBuild project, target framework, generators, assemblies | no Roslyn slot and no dependency lane | static project model then supplied-source isolated Roslyn query |
| JavaScript | runtime/module mode, package exports, external symbols | TS worker partial; fallback structural | pinned JS-mode Program/TypeChecker project profile and lock/config join |
| VB.NET | MSBuild effective model, Roslyn symbols/generators | no provider | bounded `.vbproj` profile plus isolated Roslyn VB query |
| SQL | dialect/version, migration order, catalog/extensions | no provider and source zero-read | repository-owned dialect/profile plus versioned non-executing parser |
| R | callee binding beyond the file, block NSE, S3/S4 dispatch, native/package symbols (typed, claim-scoped, non-blocking in `R_OBLIGATION_REGISTRY`) plus project/repository selection | bounded testthat frontend types them; no R provider slot registered, so every residual is irreducible under current constraints (`manual_review_required`) | a sandboxed R semantic provider/evaluator, which ADR-0036's execution boundary currently forbids |
| Rust | module graph, cfg/features, macros, trait dispatch | rust-analyzer slot `not_integrated` | isolated rust-analyzer/rustc query with selected Cargo profile |
| Delphi | compiler dialect/project, packages, units | no provider | evidence-pinned Delphi project/frontend; FPC separately qualified |
| Scratch | binary/deflate, extension opcodes, block semantics | product `NO_GO` | binary port plus vetted deflate then bounded format/extension resolver |
| Go | workspace/build target, module/types/cgo/generated | frontend sandbox unqualified | OS-sandboxed supplied-byte standard-library frontend |
| PHP | profile/autoload, source recovery, PHPUnit semantics | frontend qualification incomplete | qualified Mago/PHP differential worker and Composer coherence |
| Swift | tools/language/SDK profile, modules/macros/XCTest | frontend qualification incomplete | pinned SwiftSyntax plus isolated toolchain/SDK symbol verifier |
| Ada | GPR/toolchain/naming, generics/overload/dispatch | Libadalang `NO_GO` for current lane | new isolated project/frontend decision with supplied-input proof |
| Assembly | target/dialect/object format, macros/includes/symbols | lexical candidates only | repository target profile plus pinned parse-only provider |
| MATLAB | release/project/dialect, toolbox install/symbols | no provider; licensed tool unresolved | license-aware isolated supplied-byte frontend qualification |
| Fortran | standard/source form/preprocess/includes/modules/ABI | Flang `NO_GO` for current lane | new pinned frontend whose prescan/input boundary is contained |
| Ruby | engine/project, require/constants/metaprogramming/native | Prism qualification incomplete | isolated pinned Prism worker plus CRuby differential and project profile |
| TypeScript extra | project references/options, package exports/locks/symbols | compiler operations partial | pinned compiler package and full bounded Program/TypeChecker project model |

## Resolution state policy

- `Recoverable` means a named authorized mechanism could discharge the claim;
  it does not mean that provider is currently integrated.
- `Blocking` prevents the affected family/support claim. It must never be
  converted to `no_family` merely because analysis stopped.
- Inventory-specific uncertainty affects only dependency/project inventory and
  must not fabricate a source-semantic UNKNOWN or vice versa.
- Provider absence remains non-fatal to indexing, but it is fatal to any claim
  whose oracle requires that provider.
