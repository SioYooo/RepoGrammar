# ADR-0025: SwiftSyntax, isolated semantic verification, and XCTest preflight

- Status: Accepted
- Date: 2026-07-16
- Scope: Swift in ADR-0020 wave N1; preflight plus discovery/configuration record
- Refines: ADR-0004 and ADR-0020
- Related: `docs/plans/top-20-language-expansion-plan.md`,
  `docs/plans/swift-n1-qualification-handoff.md`,
  `docs/specifications/semantic-workers.md`, and
  `docs/reports/language-support/swift-completion-review.md`

## Context

ADR-0020 requires discovery/configuration, an authoritative frontend,
RepoGrammar-owned IR, typed uncertainty, one exact-anchor family, source-free
product readiness, independent review, and a linked completion audit before a
new language is supported. Swift has none of those implemented in RepoGrammar
today. This ADR freezes a bounded architecture and delivery order; it does not
authorize a dependency, toolchain, worker, parser, project model, family, or
support claim.

Swift needs separate syntax and semantic authorities. The bounded inventory
stage described below is now implemented, but every semantic gate remains
open. SwiftSyntax is the
official lossless syntax library and `SwiftParser` is suitable for a bounded
syntax worker, but syntax alone cannot prove module or type identity,
conditional-compilation selection, macro expansion, protocol conformance, or
dynamic dispatch. SourceKit-LSP is built on sourcekitd and compiler services,
but it is project-aware and its global results can depend on builds or
background indexing. It therefore cannot be pointed at an untrusted repository
or treated as a passive parser.

The dated primary-source snapshot is:

- Swift 6.3.3, tag `swift-6.3.3-RELEASE`, commit
  `064859e41d68596f486c5d724401cb370f260409`, released 2026-06-30;
- SwiftSyntax 603.0.2, tag `603.0.2`, commit
  `79e4b74a295b6eb74a8b585e3a39d29e70c1dbd1`; the same commit is tagged
  `swift-6.3.3-RELEASE`;
- SourceKit-LSP tag `swift-6.3.3-RELEASE`, commit
  `11994a4c6a469173066bb4b2eb653ecc9570302e`;
- Swift Package Manager tag `swift-6.3.3-RELEASE`, commit
  `5f6969f5b083b4415632114d4897c6f820761a7f`;
- Swift Testing tag `swift-6.3.3-RELEASE`, commit
  `48d727cc1cf4eda667c858c501495f1018f69d21`; and
- swift-corelibs-xctest tag `swift-6.3.3-RELEASE`, commit
  `18494ea84411109c3bc124fe94d93ac2521b30d4`.

Linux Swift toolchain archives have Swift-project PGP signatures, and the
official macOS installer is Developer-ID signed. The inspected Windows
installation documentation does not publish an independent artifact checksum
or signature. SwiftSyntax 603.0.2 has a GitHub-verified release commit but no
independent published source-archive checksum or SBOM was found. Exact archive
hashes, tag/signature status, dependency closure, licenses, advisories, and
reproducible-build evidence therefore remain admission gates. These identities
are research inputs, not admitted artifacts.

## Decision

### D1. Swift is discovery-only and unsupported

Swift is `discovered_only`. Stable `swift` and `swift-config` inventory tokens,
bounded source-free discovery, metadata persistence, incremental lifecycle,
autosync fingerprinting, and ordinary inventory CLI behavior are implemented.
RepoGrammar still has no Swift parser, dependency, toolchain, worker, project
model, code unit, IR, semantic fact, typed `UNKNOWN`, XCTest family, or
readiness promotion. A `.swift` filename proves only inventory presence and
must not be reported as language support.

The next permitted module is documentation/evidence-only artifact,
differential-corpus, dependency, and sandbox qualification. Production
artifacts or runtime behavior require a later separately reviewed atomic stage.

### D2. SwiftSyntax is the syntax candidate, not the semantic oracle

The production syntax candidate is `SwiftParser` and `SwiftSyntax` from exact
SwiftSyntax 603.0.2, built into a separately reviewed worker artifact. It must
be qualified against the exact Swift 6.3.3 compiler parser over a committed
corpus covering valid, malformed, recovered, Unicode, deep, large, conditional,
macro-bearing, and version-sensitive source.

The worker may return only RepoGrammar-owned syntax IR, checked byte ranges,
bounded sanitized diagnostics, parser/profile provenance, and typed obligation
inputs. Missing or unexpected nodes, parser recovery, compiler disagreement,
invalid ranges, incomplete output, or unsupported syntax block every
intersecting family claim. A lossless syntax tree is not evidence of module
identity, type identity, buildability, macro validity, runtime discovery, or
test execution.

SwiftSyntax 603.0.2's package manifest declares Swift tools 5.9, Swift language
modes 5 and 6, and public `SwiftParser`, `SwiftSyntax`,
`SwiftParserDiagnostics`, `SwiftIDEUtils`, and `SwiftIfConfig` products. Those
facts do not prove RepoGrammar's release targets. Windows qualification must
exercise the upstream `SWIFTSYNTAX_BUILD_DYNAMIC_LIBRARY` path because the
official manifest documents an exported-symbol limit for static linkage.

No SwiftSyntax, compiler, SourceKit, C/C++ shim, build plugin, package, binary,
or toolchain is authorized by this ADR.

### D3. Semantic verification is optional, isolated, and fail-closed

Exact XCTest superclass identity requires a separately qualified semantic
verifier tied to the exact Swift 6.3.3 toolchain and platform XCTest module.
SourceKit-LSP/sourcekitd is the initial oracle/differential candidate, not a
default repository-facing service. Admission requires an immutable mapping
from the toolchain artifact to the exact SourceKit/compiler/XCTest revisions,
the accepted dependency closure, stable symbol-identity evidence, and a
single-file request that does not evaluate or build the target project.

Every future Swift frontend or verifier request must run behind the versioned
semantic-worker boundary with these default rules:

- one supplied file and one normalized allowlisted project/profile summary per
  request, with no repository path traversal;
- no repository, home, credential, package cache, DerivedData, SDK discovery,
  editor state, or ambient current working directory;
- only immutable admitted toolchain/module artifacts plus an empty private
  temporary directory are visible;
- no network, package resolution, child process, inherited handle,
  target/repository-provided or otherwise unadmitted binary or dynamic library,
  plugin, macro implementation, or host write; immutable admitted worker,
  toolchain, Swift runtime, SourceKit, SDK, and XCTest libraries are the only
  executable/library exception;
- a minimal allowlisted environment with no proxy, credential, SwiftPM, Xcode,
  compiler-plugin, SDK-selection, or package-manager variables;
- bounded framed input and output, concurrent bounded stdout/stderr draining,
  deterministic sanitized diagnostics, and exact content hashes/ranges; and
- timeout, CPU, memory, thread, process, node, depth, diagnostic, output,
  protocol, crash, signal, or identity failure invalidates the whole response.

SourceKit-LSP must not open the target repository, evaluate `Package.swift`,
run SwiftPM, build modules, enable background indexing, load target compile
commands, resolve dependencies, or invoke compiler plugins. If exact semantic
identity cannot be obtained without those actions, the semantic path is
`NO_GO` for N1 and the affected claim remains unavailable or typed `UNKNOWN`
after the registry lands.

Default analysis must never run `swift`, `swiftc`, `swift-frontend`,
`sourcekit-lsp`, `swift package`, `swift build`, `swift test`, Xcode,
`xcodebuild`, tests, target executables, manifests, plugins, build tools,
macros, generators, or repository scripts outside the admitted worker and its
exact no-execution request contract.

### D4. The implemented discovery module is inventory-only

The implemented module defines stable inventory-only tokens:

- `swift` for an exact case-sensitive `.swift` suffix, including basename
  `.swift`; and
- `swift-config` for exact root or nested basenames `Package.swift`,
  `Package.resolved`, and `.swift-version`, plus version-specific manifest
  basenames matching the complete ASCII grammar
  `Package@swift-M[.m[.p]].swift` where each version component is one or more
  decimal digits.

Configuration classification precedes source suffix classification, so every
accepted manifest basename is `swift-config`, not `swift`. The filename grammar
is only inventory: it does not select an active manifest. Xcode
project/workspace files, `compile_commands.json`, and third-party dependency-
manager files are outside the first config inventory contract. `.swift-version`
is toolchain-selector metadata only; it is not SwiftPM package dialect,
`swift-tools-version`, language-mode, or build evidence.

Exact `.build` and `.swiftpm` components are Swift-only exclusions. They use
the shared `language_specific_exclusion` token and do not cause
the generic walker to prune unrelated-language files. Global exclusion of
`DerivedData`, `Carthage`, `Packages`, or other broad names is not authorized by
this preflight.

Discovery persists only bounded repo-relative path, strict raw-byte hash, size,
and token. It does not decode or parse inventory bytes, enter the source store,
invoke a parser, or create a unit, IR, fact, typed `UNKNOWN`, family, project
model, readiness, or support claim. Full and incremental indexing, warnings,
generation mode, copy-forward purge, autosync, CLI output, and aggregate
resource behavior follow the established Go/Ruby/PHP inventory-only policy
through one authoritative classifier rather than duplicated filename checks.

This inventory stage inherits the open ADR-0023 canonicalize-then-reopen
tree-swap/TOCTOU limitation. Existing symlink and root-containment checks are
not concurrent filesystem confinement, and adding Swift discovery must not be
reported as closing that P2 gap. The future handle-relative migration remains
cross-language authority.

### D5. SwiftPM project scope is static data only

`Package.swift` is executable Swift. RepoGrammar must never evaluate it. A
future bounded project-model stage may parse supplied manifest bytes with the
qualified syntax worker and accept only a narrow, source-visible subset:

- one selected repository root with one unversioned `Package.swift` and no
  competing version-specific manifest eligible for the selected profile;
- one valid leading `swift-tools-version` declaration admitted by the pinned
  Swift 6.3.3/PackageDescription profile;
- one unconditional top-level `let package = Package(...)` initializer;
- literal package and target names, literal target arrays, and literal
  `.testTarget(...)` declarations;
- for an N1 selected test target, absent `sources`, absent/default-empty
  `exclude`, and either one literal repo-relative `path` that normalizes beneath
  the selected root or the exact default `Tests/<literal-target-name>` path
  after that default is proven against the pinned SwiftPM differential corpus;
  target paths must be unique and non-overlapping, and a candidate belongs only
  when its already discovered path is a strict descendant of exactly one
  selected test-target path; and
- only allowlisted language-mode/platform fields whose values are statically
  complete.

Functions, computed values, loops, conditions, mutation, environment access,
filesystem access, imports beyond `PackageDescription`, macros, plugins,
traits, dynamic target construction, version-specific manifest selection,
nonempty/unsupported `exclude`, explicit `sources`, overlapping target paths,
unknown arguments, or ambiguous roots make the affected project scope or test-
target membership unavailable. The parser must not attempt to emulate general
Swift execution.

`Package.resolved` may later enter a separate bounded JSON parser as dependency
resolution inventory. Official SwiftPM documentation says it coordinates
resolved versions for a top-level leaf package and most SwiftPM commands may
implicitly resolve dependencies. RepoGrammar must not invoke those commands or
treat a lockfile as proof that dependencies are present, authentic, buildable,
or selected. The first XCTest family does not require dependency resolution;
XCTest identity comes only from the admitted toolchain module and selected test
target profile.

Raw manifest/lock contents, URLs, revisions, paths, credentials, plugin
configuration, and free-form values must not reach CLI/MCP output. A validated
literal target path is used only to select already discovered files; it never
causes traversal. Configuration changes invalidate the normalized profile and
purge claim-bearing copy-forward rows before semantic analysis resumes.

The compatibility/cache key must include exact tools version, Swift language
mode, target triple, deployment target, toolchain/SDK/XCTest identities and
digests, admitted conditional defines and feature flags, parser/semantic-
verifier artifacts, protocol version, normalized project-profile hash, and
source hash. Missing, conflicting, unsupported, or inferred dimensions become
typed obligations after the registry lands; they are never filled from the
host environment.

### D6. The first exact family is a direct XCTest method

The first family token is `swift.xctest.test_method`. It represents a
source-visible direct declaration, not a claim that the test builds, is
selected, runs, or passes. An anchor must satisfy every condition:

1. the file belongs to one statically selected SwiftPM `.testTarget` under the
   admitted Swift 6.3 profile and has a clean qualified syntax result;
2. the file has an unconditional ordinary `import XCTest`, and the semantic
   verifier resolves the immediate superclass of one non-generic, non-local,
   named class directly and exactly to the admitted platform's
   `XCTest.XCTestCase` identity;
3. the method is declared directly in that class body as an instance `func`,
   is not `static` or `class`, is non-generic, has zero explicit parameters,
   and has no return annotation or returns exactly `Void`, `Swift.Void`, or
   `()` under the verifier's normalized type identity;
4. the method basename starts with the exact lowercase ASCII prefix `test` and
   contains at least one following identifier character; and
5. the declaration is not inside conditional compilation, generated or macro
   output, an extension, a nested local type, or a recovered/degraded range.

Plain synchronous, `throws`, `async`, and `async throws` methods are compatible
variations after the exact Swift/XCTest profile proves them. `rethrows`,
parameterized methods, operator/subscript declarations, free functions,
protocol requirements/defaults, extension-only declarations, indirect
subclasses, inherited methods, dynamic runtime suites, Objective-C selector
customization, and methods added by generated or macro-expanded code are
outside N1. Attributes, including `@MainActor` and `@available`, remain
blocking obligations until each shape is explicitly qualified; this keeps the
first implementation smaller than the official runtime surface.

Apple's XCTest contract says a test method is an instance method on an
`XCTestCase` subclass, has no parameters and no return value, and begins with
lowercase `test`. RepoGrammar deliberately narrows that contract to direct
source-visible methods with exact immediate ancestry and project/profile
evidence.

Swift Testing is deferred. Its `@Test` marker is an attached compiler macro,
and macro identity/legality/registration cannot be inferred from spelling
alone. Until a separately admitted non-target-executing macro-identity
mechanism exists, `@Test` declarations intersect `swift_macro_expansion` and
must not anchor a family.

A family requires at least three fresh compatible exact anchors under one
project/toolchain/platform profile and no claim-relevant blocking Swift
`UNKNOWN`. Three method-shaped declarations alone are insufficient.

### D7. Evidence ladder and typed uncertainty

The evidence order is:

1. **Primary syntax:** fresh output from exact SwiftSyntax 603.0.2 over supplied
   bytes, clean against the Swift 6.3.3 compiler differential corpus, with
   path/hash/range, parser/profile/artifact, protocol, and sandbox provenance.
2. **Primary identity:** fresh accepted semantic-verifier output proving exact
   platform XCTest module and immediate-superclass identity without project
   execution, builds, indexing, plugins, macros, or ambient discovery.
3. **Claim-supporting derived fact:** a RepoGrammar-owned direct XCTest anchor
   created only after project, syntax, identity, signature, freshness, and
   claim-impact gates pass.
4. **Auxiliary:** extension/config inventory, literal static manifest facts,
   `Package.resolved`, syntax candidates, and unselected roots.
5. **Forbidden:** regex/text-only matching, extension/import spelling alone,
   recovered trees, unpinned snapshots or `main`, manifest evaluation,
   build/test output, macro execution, runtime discovery, or structural
   similarity without exact identity.

The first Swift obligation registry must define one authoritative claim-impact
classifier. These are normative future mechanisms, not implemented public
reason codes:

| Mechanism | Initial claim impact | Exact resolution evidence |
|---|---|---|
| `swift_frontend_availability` | Blocks all Swift syntax/semantic claims for an unavailable artifact, target, sandbox, or handshake. | One fresh successful exact-artifact request on the claimed target. |
| `swift_resource_protocol` | Blocks every claim from a truncated, timed-out, crashed, signaled, malformed, mismatched, range-invalid, or over-limit request. | A new complete within-limit request with exact hash/ranges/protocol and no discarded output. |
| `swift_syntax_profile` | Blocks anchors in a file after recovery, missing/unexpected nodes, unsupported syntax, compiler disagreement, or profile conflict. | Clean exact SwiftSyntax/compiler differential evidence under the selected 6.3 profile. |
| `swift_dialect_version` | Blocks syntax/identity/family claims when tools version, Swift language mode, deployment target, feature flags, or compiler profile is absent, conflicting, unsupported, or inferred from ambient state. | One complete allowlisted normalized compatibility key proven under the exact toolchain/parser corpus. |
| `swift_package_project_scope` | Blocks project-derived claims for absent, multiple, nested, dynamic, version-selected, malformed, oversized, or ambiguous manifests/roots. | One bounded static selected root and normalized allowlisted profile. |
| `swift_package_manifest_execution` | Blocks any fact that would require evaluating arbitrary manifest Swift. | N1 resolves only by proving the required fact is present in the admitted static subset; no execution is a resolution. |
| `swift_package_resolution` | Blocks dependency identity/buildability claims, but not the toolchain-owned XCTest anchor when no dependency fact is used. | A future bounded authenticated resolution model; `Package.resolved` presence alone never resolves it. |
| `swift_platform_sdk` | Blocks platform module identity when the exact SDK/toolchain/XCTest module is absent or mismatched. | Immutable admitted toolchain+SDK+XCTest identity and a fresh verifier handshake. |
| `swift_test_target_membership` | Blocks the affected family anchor when target kind, path, `sources`/`exclude`, overlap, conditional settings, or file membership is unresolved. | One exact static `.testTarget` with the admitted explicit/default path rule, absent `sources`, empty `exclude`, a unique non-overlapping normalized path, and exactly one matching discovered-file prefix. |
| `swift_module_identity` and `swift_xctest_case_identity` | Block the affected class/method when `XCTest` import or immediate superclass identity is unresolved, shadowed, ambiguous, indirect, or unavailable. | Exact verifier-backed module and immediate-base identity from the admitted profile. |
| `swift_test_method_signature` | Blocks the affected method for parameters, return ambiguity, generics, unsupported modifiers/attributes, or verifier disagreement. | Exact normalized direct instance signature admitted by D6. |
| `swift_conditional_compilation` | Blocks only declarations or project selection intersected by unresolved `#if` variants; it never guesses the active branch. | One explicit allowlisted compilation profile and complete branch evaluation by an admitted mechanism. |
| `swift_macro_expansion` and `swift_plugin_execution` | Block any identity/declaration requiring a macro or plugin; target implementations never execute in N1. | No N1 execution-based resolution; a later separately sandboxed, provenance-bound mechanism is required. |
| `swift_protocol_dispatch` | Does not block an unrelated direct XCTest method; blocks dispatch/implementation claims intersecting protocol requirements, defaults, existentials, or dynamic replacement. | A future semantic slice with exact conformance/witness evidence. |
| `swift_generated_source` | Blocks only when positive allowlisted generated-file/region evidence, a bounded generator mapping, or a RepoGrammar path+hash receipt applies or conflicts. | Exclude the proven generated region/file or provide fresh exact evidence that the signal is stale/inapplicable. Filename suspicion or marker absence is not proof. |
| `swift_xctest_runtime_selection` | Does not block the direct source-visible family; blocks only build/selection/run/pass claims, which N1 does not make. | A future executable test-plan model; source or manifest presence alone never resolves it. |

Callers may route, persist, count, format, or test the authoritative classifier's
result but must not reimplement policy from raw mechanism strings or fields.

### D8. Resource, supply-chain, and platform gates

Discovery inherits the shared exact ceilings: 1 MiB/file, 100,000 accepted
files, 512 MiB accepted bytes, 100,000 reported skips, 250,000 visited entries,
and depth 256.

The proposed semantic request ceiling is exactly one Swift file, 1 MiB source,
256 KiB normalized profile, 2 MiB total decoded payload, 3 MiB encoded request,
1 MiB response/stdout, 1 MiB stderr, 250,000 syntax nodes, syntax depth 512,
4,096 diagnostics, and 10,000 emitted facts/obligations. One request gets five
wall-clock seconds, four CPU-seconds, four threads including main, 512 MiB
memory, one worker process, and no descendants. Exact limits succeed; plus one
fails before partial output is accepted. Qualification may lower these
ceilings. Raising one requires measured evidence and an ADR update before
admission.

The project-model parser accepts at most one selected manifest and one matching
lockfile, each at most 1 MiB and at most 2 MiB decoded in total. Each parse is
capped at 100,000 syntax/JSON nodes, depth 256, 16,384 aggregate object/array
entries, 64 KiB per string, 4,096 target records, 512 diagnostics, and 256 KiB
normalized output. One project request gets two wall-clock seconds, two CPU-
seconds, two threads including main, 256 MiB memory, one process, and no
descendants. Duplicate JSON keys or normalized target identities, malformed or
trailing input, invalid UTF-8 where text is required, limit exhaustion,
diagnostic/output truncation, or incomplete parsing invalidates the entire
project profile; no partial summary is accepted.

The minimum compile/corpus/runtime matrix is:

- `x86_64-unknown-linux-gnu`;
- `aarch64-unknown-linux-gnu`;
- `x86_64-apple-darwin`;
- `aarch64-apple-darwin`; and
- `x86_64-pc-windows-msvc`.

Linux, macOS, and Windows additionally require native sandbox tests proving
filesystem, network, descendant-process, timeout, CPU, memory, thread, and
output enforcement. Cross-compilation and upstream availability are not
RepoGrammar support evidence. Windows artifact authenticity remains an
explicit gate until independently verified.

Before any dependency or artifact admission, record exact source/archive and
installer hashes, signatures, toolchain-to-source mapping, transitive packages,
licenses, advisories, build scripts/C/C++ shims/generated code, compiler/linker
inputs, reproducibility, SBOM, and five-target results. No floating branch,
snapshot, mutable `main`, ambient SDK, or download-at-analysis-time behavior is
permitted.

### D9. Delivery is staged and atomic

Swift work must land in this order:

1. this decision-only preflight;
2. discovery/configuration inventory;
3. documentation/evidence-only artifact, differential-corpus, dependency, and
   sandbox qualification;
4. sandboxed worker/protocol/artifact admission;
5. bounded static SwiftPM project profile;
6. SwiftSyntax IR plus isolated semantic identity resolver;
7. authoritative Swift obligation registry and classifier;
8. `swift.xctest.test_method` plus fixtures and support threshold;
9. source-free CLI/MCP/readiness wiring;
10. cross-module correctness, security, completeness/design, and performance
    review with scoped fixes; and
11. a separate completion audit linking every prerequisite commit.

Each completed stage is one or more independently coherent Conventional
Commits containing code, tests, and synchronized documentation where behavior
changes. Failed qualification may land as a documentation/evidence-only
negative result. It must not be disguised as support or folded into a later
success commit. Do not push without explicit authorization.

## Alternatives considered

### Use SourceKit-LSP directly on the repository

Rejected. Project opening, toolchain discovery, module preparation, indexing,
and build-dependent global results violate the default no-execution/no-ambient-
state boundary. Only a synthesized supplied-input request in a reviewed sandbox
may be qualified.

### Use Tree-sitter Swift as the product frontend

Rejected for the authoritative path. A structural grammar may later generate
fallback candidates, but it cannot establish Swift compiler compatibility,
module identity, conditional selection, macro semantics, or XCTest ancestry.

### Make Swift Testing `@Test` the first family

Deferred. It is the modern framework and is included in Swift 6 toolchains,
but its marker is a compiler macro. Spelling-based recognition would overclaim
identity, while executing macro implementations violates the N1 safety
boundary. Direct XCTest methods provide a smaller exact source-visible slice.

### Evaluate `Package.swift` with SwiftPM

Rejected. A package manifest is executable Swift and may declare plugins and
dependencies. N1 accepts only a bounded static syntax subset and abstains on
the rest.

## Consequences

- Swift is `discovered_only`, remains unsupported, and is excluded from every
  supported/readiness count.
- Bounded inventory is useful without admitting the large Swift toolchain
  surface.
- Exact XCTest support is intentionally narrower than XCTest runtime discovery
  and depends on a qualified semantic identity path.
- Swift Testing, Xcode projects, version-specific manifests, dependency
  resolution, macros, plugins, conditional variants, indirect ancestry,
  protocol dispatch, and runtime execution remain explicit future work or
  non-claims.
- A qualification `NO_GO` is an acceptable outcome. It does not authorize a
  regex fallback, recovered-tree claim, or unsafe repository execution.

## References

- [Swift 6.3.3 release](https://github.com/swiftlang/swift/releases/tag/swift-6.3.3-RELEASE)
- [Swift Linux artifact verification](https://www.swift.org/install/linux/tarball/)
- [Swift macOS package verification](https://www.swift.org/install/macos/package_installer/)
- [Swift Windows manual installation](https://www.swift.org/install/windows/manual/)
- [Swift platform support](https://www.swift.org/platform-support/)
- [SwiftSyntax 603.0.2](https://github.com/swiftlang/swift-syntax/releases/tag/603.0.2)
- [SwiftSyntax 603.0.2 manifest](https://raw.githubusercontent.com/swiftlang/swift-syntax/603.0.2/Package.swift)
- [SourceKit-LSP](https://github.com/swiftlang/sourcekit-lsp)
- [SourceKit-LSP Swift 6.3.3 tag](https://github.com/swiftlang/sourcekit-lsp/tree/swift-6.3.3-RELEASE)
- [Swift PackageDescription](https://docs.swift.org/swiftpm/documentation/packagedescription/)
- [SwiftPM dependency resolution](https://docs.swift.org/swiftpm/documentation/packagemanagerdocs/resolvingpackageversions/)
- [XCTest test method contract](https://developer.apple.com/documentation/xctest/defining-test-cases-and-test-methods)
- [Swift Testing test declarations](https://developer.apple.com/documentation/testing/definingtests)
- [Swift macro model](https://docs.swift.org/swift-book/documentation/the-swift-programming-language/macros/)
