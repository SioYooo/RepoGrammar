# Swift language completion review

- Status: Incomplete — `structural_substrate`
- Authority: ADR-0020, ADR-0025, and ADR-0048
- Dependency prerequisite: `9f1f657a27d45264d3b5e9b681750778234f0cb2`
- Last updated: 2026-09-05

## ADR-0020 gate

- [x] Discovery/configuration — exact `.swift`, `Package.swift`,
  version-specific manifests, `Package.resolved`, and `.swift-version`
  inventory, Swift-only `.build`/`.swiftpm` exclusions, bounded schema-2/3
  `Package.resolved` pins with unknown scope/directness, persistence, and
  incremental behavior are covered by the ADR-0025 discovery record. ADR-0048
  changes none of it.
- [x] Authoritative frontend for the declared scope — ADR-0048's bounded
  in-process recursive-descent parser reads every discovered `.swift` file.
  It parses the declared subset of ADR-0048 D2a into real declarations —
  imports, class headers with inheritance lists, method headers with
  modifiers, effects, parameter groups, and Void spellings — and the anchor
  is a question asked of those declarations rather than of byte positions.
  ADR-0048 D4b supplies the invariance the gate asks for: the declared set
  is Swift 5.x source-mode syntax, and the two constructs whose token
  boundaries move inside that set — regex literals from SE-0355 and
  non-ASCII identifier bytes — are refused rather than read, the first
  under the `swift_dialect_invariance` claim mirroring ADR-0046's
  `matlab_dialect_invariance` shape. Conditional compilation abstains the
  whole file under `BuildVariantAmbiguity`. Outside the subset the parser
  abstains, never recovers, and never resynchronizes, which is how
  ADR-0025 D7 item 5's no-text-matching requirement is discharged: with a
  real parse, per the ADR-0041..0046 bounded-parser precedent recorded in
  ADR-0048 D8. No selected project model or profile exists, and none is
  claimed.
- [x] RepoGrammar-owned Swift source code units and IR — a module unit per
  decoded file, one `swift_test_class` unit per admitted test class, one
  `swift_test_method` unit per admitted test method, source ranges, content
  hashes, IR nodes projected to Class/Method, and containment edges via the
  shared lane substrate.
- [x] Typed `UNKNOWN` registry and provider fallback — every abstention the
  frontend can emit is enumerated in the ADR-0048 D7 table and routed
  through one `unknown_fact` shape: three refusal claims
  (`swift_syntax_admission` with fifteen bounded kinds,
  `swift_dialect_invariance` with the regex divergence,
  `swift_conditional_compilation`), two blocking-direction binding claims
  (`swift_xctest_class_binding`, `swift_xctest_import_binding`), and the
  bounded observation claim `swift_xctest_method_shape` with five kinds.
  Counts may fall only via source-backed replacement facts — the import
  appearing or the superclass spelling changing. No Swift semantic provider
  slot exists or is registered, and the repository's zero-external-dependency
  constraint admits none, so module-qualified `XCTest.XCTestCase` identity,
  indirect ancestry, extension-added methods, and execution stay undischarged
  — recorded as the anchor's standing `provider_resolved=false` assumption
  rather than being filled from ambient state. Every claim above is a refusal
  or binding claim that fires only on an unadmitted or unbound construct, so
  a clean positive corpus emits no residual at all: `xctest_exact_tests`
  measures `total_unknowns: 0`, and there is nothing there for the `unknowns`
  surface to classify on either axis.
- [x] Exact family with support at least three — the parser emits the fixed
  `swift.xctest.test_method` support target (engine
  `repogrammar-swift-xctest-parser`, method
  `bounded_swift_xctest_declaration_v1`; the derived-support arm mints
  `repogrammar-swift-derived` facts under the same method), and the committed
  `xctest_exact_tests`, `xctest_setup_override`, and `xctest_throwing_tests`
  fixtures carry eight distinct admitted anchors across three files — three,
  two, and three, exactly as the parser's own tests assert. The
  framework-role registry (`framework:xctest.test`) is registered in the
  shared detector and in `application/query_terms.rs` (as the `xctest` alias,
  known token, and test concept), the `application/family.rs` compatibility
  and derived-support arms and the `application/indexing.rs` admission are
  wired, and the sqlite repo-shape allowlists count the language.
  `min_family_support` pins **3** for `swift` explicitly rather than
  inheriting the shared default of two. Measured through the product CLI
  today: `xctest_exact_tests` and `xctest_throwing_tests` each form
  `family:swift:swift_test_method:framework_xctest_test` with support **3**,
  and every other committed fixture — including `xctest_setup_override`,
  whose two anchors sit under the bar exactly as `xctest_low_support`'s do —
  indexes to `status == "complete"` and forms no family.
- [x] Positive, lookalike, low-support, and parse-degraded family fixtures —
  `xctest_exact_tests` (three bare test-prefix methods plus a non-test
  helper), `xctest_setup_override` (`override func setUp`/`tearDown` plus
  tests), `xctest_throwing_tests` (`throws` methods, file-level `enum` and
  `struct` consumed opaquely), `xctest_lookalikes` (a class not deriving
  `XCTestCase`, a `static func test…`, a parameterized `func test…`, and a
  free `func test…` — zero anchors, four recorded kinds),
  `xctest_unbound_import` (bare `XCTestCase` without the import),
  `xctest_conditional` (`#if`-guarded class — whole-file abstention),
  `xctest_degraded` (attribute-with-arguments refusal), and
  `xctest_low_support` (two anchors, one below support three).
  Unresolved/resolved fixture pairs do not exist: there is no Swift provider
  to resolve against.
- [x] Source-free readiness and leakage matrix — at the module boundary the
  product dispatch path and the framework-role detector are asserted over
  both an anchored workspace and an unbound one
  (`readiness_surfaces_stay_source_free_and_low_cardinality`): the positive
  workspace really yields the anchor and its role, the unbound one really
  yields the lane's typed `UNKNOWN`, and no fact, note, assumption, or
  diagnostic carries fixture identifiers, source text, or absolute paths —
  every emitted note comes from the fixed two-sentence anchor vocabulary.
  Product-level `init`/`index`/`families` tests over every Swift fixture now
  live in `src/rust/bin/repogrammar.rs`. One qualifier belongs on the record:
  the shared CLI matrix helper
  `assert_scanner_lane_readiness_is_source_free` (`status`, `doctor`,
  `stats`, `unknowns`, `families`, `files`, `inspect_readiness`) is still
  called only for the delphi, ada, matlab, visual-basic, and r lanes.
  Swift's `stats` and `unknowns` output was re-inspected empirically on
  2026-09-05 and exposed only bounded tokens, counts, role names, and reason
  codes; that check is not yet pinned by a committed assertion.
- [x] Four-part review record — this report records correctness, security,
  completeness, and performance findings below; open findings remain
  blockers.
- [ ] Linked atomic prerequisites and final completion audit — open. The
  maintainer ruled on 2026-09-05 that this gate requires the frontend/IR
  slice and the `UNKNOWN`/provider slice to land as separate atomic commits.
  This lane arrived as one integrated slice whose frontend emits its typed
  `UNKNOWN`s inline, so satisfying the ruling would mean re-authoring
  intermediate states that never existed rather than re-slicing existing
  ones. No external artifact blocks it.

## Current evidence and blocker

Exactly one decode path exists: every discovered `.swift` file through the
ADR-0048 bounded parser. The parse loads no grammar and no external
artifact, and the ADR-0025 discovery/inventory substrate — including the
static `Package.resolved` reader — is unchanged. No `swift`, `swiftc`,
`sourcekit-lsp`, SwiftPM, Xcode, macro, plugin, package script, child
process, or network action runs.

No integration blocker remains. `application/indexing.rs` admits every
`.swift` file through `file_is_inventory_only`, `application/family.rs`
carries the Swift support-compatibility and derived-support arms, the role is
registered in the shared detector and in `application/query_terms.rs`, the
sqlite repo-shape allowlists count the language, and
`src/rust/bin/repogrammar.rs` indexes every Swift fixture through the product
CLI runtime.

What is open is gate 9's commit shape, and one known limitation:
`readiness_language_scopes()` in `application/query.rs` — the list deciding
which languages the public `stats` surface reports with a
`language_scope`/`preview_status` — still contains only python,
typescript/javascript, rust, java, csharp, and c/cpp. It was deliberately not
extended, because that list is a product support-scope claim requiring its
own decision rather than internal counting. Swift's repo-shape counts
therefore exist in storage and are not surfaced through the public
per-language stats rows.

Three defects found this session applied to this lane and are recorded rather
than left implied. `min_family_support` fell through to the shared default of
two, so a pair of anchors — `xctest_setup_override`'s or
`xctest_low_support`'s — would have formed a family this review says must not
form. The sqlite repo-shape stats allowlists (`REPO_SHAPE_LANGUAGE_SCOPES`
and all four `repo_shape_*_where` predicates) omitted Swift, so the language
silently counted zero; the pre-existing four-predicate invariant test now
actually covers it. And the framework role was `framework:swift.xctest.test`,
whose derived framework token is the language name `swift` rather than a
framework; it was renamed to `framework:xctest.test`, matching the
framework-not-language convention every peer uses (`testthat`, `phpunit`,
`dunitx`, `aunit`), which is why the family id reads
`family:swift:swift_test_method:framework_xctest_test`.

One cross-lane inconsistency belongs on the record as a non-claim rather than
a finding: PHP, Go, and Fortran attach standing per-anchor residual
`UNKNOWN`s, while Swift and Ruby emit none on a clean positive corpus,
because ADR-0048 D7 defines refusal-style unknowns that fire only on an
unadmitted or unbound construct and carries the standing residual as the
anchor's `provider_resolved=false` assumption instead. Each lane implements
the unknown set its own ADR defines, so this is a difference in ADR shape,
not a gate failure.

## Completion verdict

Not complete. Swift has a bounded recursive-descent frontend over one
class-and-method shape, owned units and IR, a complete typed-`UNKNOWN`
inventory, a registered framework role, eight committed fixture corpora,
module-level source-free readiness evidence, and a family that forms in
product output at the three-member bar. It has no final audit and no
provider. Strict gate count is `8/9` with gate 9 open on the commit-shape
ruling above; Swift is `structural_substrate` and must not be counted as
supported.

Three limitations are worth stating rather than leaving to inference.

The parser recognizes only its declared subset, so a file using conditional
compilation, a raw string, a regex-literal position, a non-ASCII identifier,
an attribute with arguments, generics on a parsed declaration, an accessor
property, or a wrapped class header contributes no anchors at all. That is
deliberate — the alternative is reading under an assumption — but it means
a repository's support count can understate the tests it really has. The
degraded diagnostic and the typed `UNKNOWN` say when this happened; they do
not say how many anchors were missed.

The anchor binds the bare `XCTestCase` spelling through the file's own
import; it does not resolve `XCTest.XCTestCase` module identity, indirect
ancestry, or extension-added methods. Those residuals are ADR-0025 D3's
semantic-verifier obligations, recorded as standing non-claims rather than
filled in, and closing them needs the worker ADR-0048 keeps unauthorized.

The superclass evidence is a spelling plus a module-name binding, not
semantic identity: a same-named class from another module in a file that
imports XCTest satisfies the anchor's source-visible conditions. This is
the widest gap against ADR-0025 D6's verifier-backed anchor and is stated
as such in ADR-0048 D6.

## Final program audit fields

| Required field | Audited result |
|---|---|
| Language / rank | Swift / 15 |
| Dialect/version | No selected tools version, Swift language mode, target triple, SDK, deployment target, manifest profile, or macro set. ADR-0048 D4b's invariance set is Swift 5.x source-mode syntax; the constructs that move inside that set are refused, so the admitted parse never depends on a selection. |
| Provider/frontend/version | Bounded in-process `repogrammar-swift-xctest-parser` / `bounded_swift_xctest_declaration_v1`, a hand-written lexer and recursive-descent parser over the ADR-0048 D2a subset. No SwiftSyntax, SourceKit, compiler/runtime, or provider. |
| Discovery/config | `.swift`, `Package.swift`, valid versioned manifests, `Package.resolved`, and `.swift-version` inventory with `.build`/`.swiftpm` exclusions; unchanged by ADR-0048. |
| Manifest/lockfile | Bounded schema-2/3 `Package.resolved` exact pins with unknown scope/directness; executable manifests are not evaluated; the XCTest family consults none of it. |
| Owned source IR / external symbols | Owned module/class/method units for the ADR-0048 anchor, projected to Class/Method with containment edges; external symbols stay absent, and module identity, indirect ancestry, extensions, macros, and generated code are unresolved. |
| Library Contracts | Registry infrastructure exists, production packs = 0; inventory never creates a behavior contract. |
| Exact family / fixtures | One exact family target, `swift.xctest.test_method` over the bounded XCTest anchor under role `framework:xctest.test`, family id `family:swift:swift_test_method:framework_xctest_test`, support bar three pinned explicitly for `swift`. Eight admitted anchors across three anchor-bearing fixtures (three, two, three, as the parser's tests assert); `xctest_exact_tests` and `xctest_throwing_tests` each form the family at support 3 through the product CLI, while `xctest_setup_override`'s two anchors sit under the bar and the lookalike/unbound/conditional/degraded/low-support matrix forms nothing. Resolved/unresolved pairs do not exist, because there is no Swift provider. |
| Primary UNKNOWN cases | `swift_syntax_admission` (unterminated string/comment/backtick, raw string, non-ASCII code, attribute argument, generic declaration, accessor, unadmitted member/file/header/hash shapes, unbalanced structure, resource ceilings), `swift_dialect_invariance` (`unadmitted_regex_literal`, blocking-direction `ConflictingFacts`), `swift_conditional_compilation` (`conditional_compilation_region`), `swift_xctest_class_binding` (`test_methods_without_testcase_base`), `swift_xctest_import_binding` (`testcase_without_xctest_import`), `swift_xctest_method_shape` (`static_test_method`, `test_method_with_parameters`, `test_method_non_void_return`, `test_method_rethrows`, `free_test_function`), plus the inventory lane's `swift_dependency_inventory` tokens. No Swift semantic provider slot exists and the dependency constraint admits none, so the identity obligations cannot be discharged in practice; they ride each anchor as the standing `provider_resolved=false` assumption. Every listed claim fires only on an unadmitted or unbound construct, so a clean positive corpus emits no residual: `xctest_exact_tests` measures `total_unknowns: 0`. |
| Source-free / security | Only `.swift` bytes are read beyond the existing inventory; every fact, note, assumption, kind, and diagnostic is a fixed source-free string, asserted over both an anchored and an unbound workspace at the module boundary; input is bounded by byte, token, unit, header, interpolation-depth, and recursion ceilings; no toolchain, manifest, macro, plugin, child process, repository code, or network runs. |
| Completion state / counted | `structural_substrate`; strict gate count `8/9` with gate 9 open on the commit-shape ruling; Top-20 complete = no. |

Four-part review: correctness establishes the anchor from parsed
declarations rather than byte positions, so a conditionally compiled,
static, parameterized, non-Void, backticked, operator, extension-added, or
free-standing test shape is not mistaken for a discovered test, tracks the
modifier set the XCTest contract depends on, and refuses whole files rather
than recovering; security keeps every refusal kind and note a bounded fixed
string that never copies divergent source, bounds input on six axes, and
executes nothing; completeness lacks the provider layer, the committed
CLI-surface readiness assertions, the public per-language `stats` rows, and
the final audit; performance is a single linear lex and parse per file with
iterative body consumption and bounded class recursion — no toolchain,
SourceKit, SDK-scale, macro, or package-graph benchmark exists or is
claimed. Evidence: `src/rust/adapters/parsing/swift/{syntax.rs,xctest.rs}`,
`src/rust/adapters/parsing/swift.rs`, `src/rust/adapters/frameworks/swift.rs`,
`src/fixtures/swift/release/v0_2/`, `src/rust/application/family.rs` and
`src/rust/application/indexing.rs`, `src/rust/bin/repogrammar.rs`, ADR-0025,
ADR-0048, the module test suites, and prerequisite
`9f1f657a27d45264d3b5e9b681750778234f0cb2`. Exact non-claims: a
`Package.resolved` pin does not establish a usable module, XCTest identity,
source semantics, API compatibility, or family; and the source anchor proves
a declaration's shape and import binding, not that `XCTestCase` resolves to
the platform's XCTest module or that the file's tests compile, run, or pass.
