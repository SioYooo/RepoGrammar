# Go language completion review

- Status: Incomplete — `structural_substrate`
- Authority: ADR-0020, ADR-0021, ADR-0041 (superseded for this family by
  ADR-0050), and ADR-0050
- Dependency prerequisite: `abda602fe38db8549e4f318bd4638ddb94df7282`
- Last updated: 2026-09-05

## ADR-0020 gate

- [x] Discovery/config and dependency inventory — bounded `.go` discovery,
  root/nested `go.mod` `require` inventory, direct/indirect preservation,
  `go.work` abstention, typed malformed/conflict/resource/graph-changing
  UNKNOWNs, persistence, and incremental replacement exist without Go
  execution. Unchanged by this lane.
- [x] Authoritative frontend for the declared scope — ADR-0050's bounded
  in-process recursive-descent parser reads `*_test.go` bytes only, replacing
  the ADR-0041 scanner in the dispatch path. It lexes Go's tokens — both
  string forms with validated escapes, runes, comments, the complete numeral
  grammar, every operator — reproduces Go's semicolon-insertion rule exactly
  because it decides declaration boundaries, and reads the anchor off the
  parsed declaration: top-level plain `func`, no type parameters, exactly one
  optionally-named parameter of pointer-to-qualified-type shape whose
  qualifier is the file's own decoded `"testing"` binding and whose type
  letter matches the family (T/B/F), no results. ADR-0050 D4a supplies the
  invariance the gate asks for, and it is the program's strongest: the Go 1
  compatibility promise fixes the admitted grammar across every Go 1.x
  release, there is no preprocessor or dialect selector in the language, and
  the residual axes (generics, cgo, Unicode identifiers) are refused rather
  than read. Outside the subset the whole file abstains — module unit, typed
  `UNKNOWN`, no anchor — which is ADR-0021's own stricter whole-file stance.
  No worker, provider, or dependency is added, and ADR-0021's worker route
  remains in force for every semantic fact this frontend cannot produce.
- [x] RepoGrammar-owned code units and IR — a module unit per admitted file,
  one `go_function` or `go_test_function` unit per parsed top-level
  declaration, source ranges, content hashes, IR nodes, and containment
  edges.
- [x] Complete source-semantic obligation registry and provider fallback —
  `GO_OBLIGATION_REGISTRY` in
  `src/rust/adapters/parsing/go/testing.rs` is the single table that declares
  every typed `UNKNOWN` the testing frontend can emit, and all emission paths
  route through it, so an unregistered unknown is a typed error rather than a
  possible output. Sixteen entries cover the obligations the
  `go.testing.test_function` family claim rests on: the two blocking identity
  obligations (dot and blank `testing` imports, which bind no qualified
  name), the non-blocking build-constraint subclaim, the ten refusal classes
  (unadmitted construct, generic signature, undecidable receiver type, cgo
  import, unterminated literal, invalid escape, unbalanced braces, duplicate
  import binding, depth/byte/unit ceilings), and two standing residuals that
  ride on every admitted anchor and never block — test execution semantics
  and package identity. Each entry records its reason code, its claim scope,
  whether an unmet obligation blocks the family claim, and its
  provider-fallback policy; the worker-route residuals name the ADR-0021
  worker explicitly. `application/family.rs` remains the authoritative
  claim-impact classifier; a registry test pins the recorded split to the
  two rules it already implements for Go (UnresolvedImport on
  `go_test_declaration` blocks; `go_build_constraint` is a non-blocking
  subclaim).
- [x] Exact family with support at least three — the parser emits the exact
  anchors (`go.testing.test_function`, Test/Benchmark/Fuzz variation), the
  role registry in `src/rust/adapters/frameworks/go.rs` is family-bearing
  with the fixed support target `framework:go_testing.test_function`, and the
  shared wiring is landed: the `application/family.rs` compatibility and
  derived-support arms (`repogrammar-go-derived`), the
  `application/indexing.rs` `*_test.go` admission, the sqlite repo-shape
  allowlists, and the `application/query_terms.rs` registration of
  `go_testing` as a known framework token and test concept.
  `min_family_support` pins **3** for `go` explicitly rather than inheriting
  the shared default of two. Measured through the product CLI today, over
  three positive fixtures: `testing_exact_tests` forms
  `family:go:go_test_function:framework_go_testing_test_function` with
  support **3**, `testing_benchmarks_fuzz` with **4**, and
  `testing_table_driven` with **3**; `testing_lookalikes`,
  `testing_low_support`, and `testing_parse_degraded` each index to
  `status == "complete"` and form none.
- [x] Positive, lookalike, low-support, and parse-degraded family fixtures —
  `testing_exact_tests` (three plain Test declarations plus TestMain and a
  helper as non-anchors), `testing_benchmarks_fuzz` (Benchmark ×2, Fuzz ×1,
  and the bare `Test` the toolchain itself runs), `testing_table_driven`
  (t.Run bodies skipped, prose in strings/comments never anchors),
  `testing_lookalikes` (methods, wrong parameter types, wrong name shapes,
  prose), `testing_low_support` (two anchors, below the minimum), and
  `testing_parse_degraded` (generic signature, cgo import, unterminated
  string, unbalanced braces — each abstains whole-file, with the degraded leg
  reporting an Error diagnostic). Unresolved/resolved fixture pairs do not
  exist: there is no Go provider to resolve against, and ADR-0050 adds none.
- [x] Complete source-free readiness and leakage matrix across required
  public surfaces — the frontend's own surfaces are tested source-free (no
  refusal diagnostic, fact note, or assumption carries source text, and a
  dedicated leakage test asserts this over hostile inputs), and the
  module-level tests exercise every committed fixture through the parse
  path. Product-level `init`/`index`/`families` tests in
  `src/rust/bin/repogrammar.rs` now cover all six Go fixtures, not the three
  they covered before this session, and additionally assert the derived
  support facts for the positive corpora and their absence for the lookalike
  and degraded ones. One qualifier belongs on the record: the shared CLI
  matrix helper `assert_scanner_lane_readiness_is_source_free` (`status`,
  `doctor`, `stats`, `unknowns`, `families`, `files`, `inspect_readiness`) is
  still called only for the delphi, ada, matlab, visual-basic, and r lanes.
  Go's `stats` and `unknowns` output was re-inspected empirically on
  2026-09-05 and exposed only bounded tokens, counts, role names, and reason
  codes; that check is not yet pinned by a committed assertion.
- [x] Four-part review record — this report records correctness, security,
  completeness, and performance findings; open findings remain blockers.
- [ ] Linked atomic prerequisites and final completion audit — open. The
  maintainer ruled on 2026-09-05 that this gate requires the frontend/IR
  slice and the `UNKNOWN`/provider slice to land as separate atomic commits.
  This lane arrived as one integrated slice whose frontend emits its typed
  `UNKNOWN`s inline, so satisfying the ruling would mean re-authoring
  intermediate states that never existed rather than re-slicing existing
  ones. No external artifact blocks it.

## Current evidence and blocker

Exactly one filename class is decoded: `*_test.go`, the set `go test` itself
compiles as tests. Every other `.go` byte stays inventory and is never read.
The parse is the bounded in-process one ADR-0050 D4 declares; it loads no
grammar and no external artifact. The anchor's parameter type is proven by
the parse — an optional identifier, a pointer star, a qualifier resolved
through the file's own decoded `"testing"` import binding, and the family's
type letter — never by matching text.

The metadata adapter is static and non-executing, as before: `go.mod`
`require` declarations remain bounded language-neutral inventory, `go.work`
abstains, and `go.sum` is not promoted to a lockfile.

No integration blocker remains. `application/indexing.rs` admits `*_test.go`
through `file_is_inventory_only`, `application/family.rs` carries the Go
support-compatibility and derived-support arms, and the family forms in
product output at the three-member bar. Each of the three anchors in the
exact-test corpus carries its two standing residual `UNKNOWN`s, six in total,
exactly as the registry declares.

What is open is gate 9's commit shape, and one known limitation:
`readiness_language_scopes()` in `application/query.rs` — the list deciding
which languages the public `stats` surface reports with a
`language_scope`/`preview_status` — still contains only python,
typescript/javascript, rust, java, csharp, and c/cpp. It was deliberately not
extended, because that list is a product support-scope claim requiring its
own decision rather than internal counting. Go's repo-shape counts therefore
exist in storage and are not surfaced through the public per-language stats
rows.

Three defects found this session applied to this lane and are recorded rather
than left implied. `min_family_support` fell through to the shared default of
two, and this lane proved it live: the `testing_low_support` fixture's two
anchors formed a family the review says must not form.
`application/family.rs` imported `GO_ANCHOR_ENGINE`/`GO_ANCHOR_METHOD` from
the retired ADR-0041 scanner module (`go::source`) instead of the dispatched
ADR-0050 parser (`go::testing`), so
`FamilyUnknownDomain::from_language_and_origin` never matched a real Go fact
and both Go claim-impact classifiers were dead against product data; the
import now names `go::testing` and an engine-identity guard test pins both the
current constants and the retired scanner's rejection. And the sqlite
repo-shape stats allowlists (`REPO_SHAPE_LANGUAGE_SCOPES` and all four
`repo_shape_*_where` predicates) omitted Go, so the language silently counted
zero; the pre-existing four-predicate invariant test now actually covers it.

## Completion verdict

Not complete. Go has a bounded recursive-descent parser over one filename
class and the toolchain-exact family of declaration shapes, owned units and
IR, a complete claim-scoped source-semantic obligation registry with
provider-fallback policy, family-eligible anchors with the committed fixture
corpus, source-free frontend surfaces, and a family that forms in product
output at the three-member bar. It has no provider and no final audit. Strict
gate count is `8/9` with gate 9 open on the commit-shape ruling above; Go is
`structural_substrate` and must not be counted as supported.

Two limitations are worth stating rather than leaving to inference.

The parser recognizes only its declared subset, so a `_test.go` file using
generics, cgo, a Unicode identifier, or a body-less declaration abstains as
a whole file. That is deliberate — the alternative is guessing — but it
means a repository's support count can understate the tests it really has.
The refusal's typed `UNKNOWN` says which class fired; it does not say how
many declarations were missed.

The anchor is a claim about a declaration, not about execution or the
package graph. Whether the test runs, passes, skips, runs in parallel, what
its `t.Run` subtests are, whether the file is selected by build constraints
on the target platform, and what the declaring package resolves to all stay
typed `UNKNOWN` — the first as a standing residual on every anchor, the
second recorded per file when a header directive is present, the rest as
registry fallbacks naming the ADR-0021 worker route.

## Final program audit fields

| Required field | Audited result |
|---|---|
| Language / rank | Go / 13 |
| Dialect/version | No selected Go release, GOOS, GOARCH, build tags, toolchain, generated-code, or cgo profile. The admitted parse needs none: ADR-0050 D4a's Go 1 compatibility-promise argument shows no Go 1.x release reads an admitted file into a different tree, and the residual axes are refused rather than read. |
| Provider/frontend/version | Bounded in-process `repogrammar-go-testing-parser` / `bounded_go_test_declaration_v2`, a hand-written lexer and recursive-descent parser over the ADR-0050 D4 subset. No external Go parser, grammar, toolchain, compiler/runtime, or provider. The ADR-0021 pinned standard-library worker remains unauthorized future work. |
| Discovery/config | `.go`, `go.mod`, and `go.work` with documented exclusions and bounded path/size handling, unchanged. |
| Manifest/lockfile | Bounded `go.mod` direct/`// indirect` declaration inventory; `go.work` abstains; `go.sum` is not promoted to a lockfile, unchanged. |
| Owned source IR / external symbols | Owned module and function/test units for parsed `_test.go` files with ranges, hashes, IR nodes, and containment edges. External symbols, imports beyond the `testing` binding, the package graph, and type identity are unresolved. |
| Library Contracts | Registry infrastructure exists; production contract packs = 0 and no Go package version/symbol can match one. |
| Exact family / fixtures | One exact family, `go.testing.test_function` over the Test/Benchmark/Fuzz variation with the `go test` name rule mirrored exactly (bare prefixes anchor; ADR-0021 D3's narrower name rule superseded), under role `framework:go_testing.test_function`, family id `family:go:go_test_function:framework_go_testing_test_function`, support bar three pinned explicitly for `go`. Measured: three positive fixtures form the family at support 3, 4, and 3; lookalike, low-support, and parse-degraded index complete and form none. Resolved/unresolved pairs do not exist, because there is no Go provider. |
| Primary UNKNOWN cases | `go_test_declaration` (dot/blank testing import — both blocking), `go_build_constraint` (non-blocking subclaim), `go_test_parse` (generic signature, undecidable receiver type, unadmitted construct, unterminated literal, invalid escape, unbalanced braces, duplicate import binding, depth/byte/unit ceilings), `go_cgo_boundary` (cgo import), plus the standing `go_test_execution` and `go_package_identity` residuals on every anchor. Fallbacks: identity obligations are source-decidable and permanent; build-environment selection, cgo semantics, and package identity name the ADR-0021 worker route; execution semantics name runtime observation, which is not authorized. The inventory lane's `go_dependency_inventory` tokens are unchanged. |
| Source-free / security | Only `*_test.go` bytes are read, every other `.go` byte stays zero-read, and no source text, identifier, or literal reaches a frontend surface (asserted by a leakage test over hostile inputs); input is bounded by byte, depth, unit, and fact ceilings, each abstaining with a typed `UNKNOWN`; no Go command, worker, cgo path, generator, child process, repository code, or network runs. The committed CLI-surface source-free matrix helper is not yet extended to this lane; the 2026-09-05 re-inspection of `stats` and `unknowns` was empirical. |
| Completion state / counted | `structural_substrate`; strict gate count `8/9` with gate 9 open on the commit-shape ruling; Top-20 complete = no. |

## Four-part review

- Correctness: the anchor is established from a parsed declaration, so a
  method, a generic function, a wrong type letter, an unresolvable import
  binding, a result-bearing signature, and a name the toolchain would not
  run are all non-anchors, and prose in comments and both string forms can
  never contribute one. Go's semicolon-insertion rule is reproduced exactly,
  so a parameter list split after a comma parses and one split after a
  complete token refuses, as the language does. Every typed `UNKNOWN` routes
  through one obligation registry whose blocking records match the
  authoritative family classifier's existing Go rules.
- Security: supplied source is bounded by bytes, nesting depth, and unit and
  fact counts; refusal diagnostics, fact notes, and assumptions are fixed
  source-free vocabulary (asserted); no process, network, toolchain, module
  resolution, or repository code execution is added; `_test.go` is the only
  new byte class crossing the source-store boundary, and it already did
  under ADR-0041.
- Completeness: the frontend, obligation registry, role registry, fixtures,
  shared derived-support/family arm, and product-level family tests over all
  six fixtures are real; the committed CLI-surface readiness assertions, the
  public per-language `stats` rows, the provider layer, and the final audit
  remain.
- Performance: lexing and parsing are single linear passes over supplied
  bytes with bounded token vectors; body skipping is bracket-stack counting
  over the token stream; no Go-runtime, module-graph, or large-repository
  benchmark exists yet.

Evidence: `src/rust/adapters/parsing/go/testing.rs` (the parser, the
`GO_OBLIGATION_REGISTRY` gate 4 table, and its emission + registry +
fixture tests), `src/rust/adapters/parsing/go.rs` and
`src/rust/adapters/parsing/mod.rs` (module declaration and the Go dispatch
arm), `src/rust/adapters/frameworks/go.rs` (the family-bearing role
registry), `src/rust/application/family.rs` and
`src/rust/application/indexing.rs` (the support-compatibility,
derived-support, and admission arms), `src/rust/bin/repogrammar.rs` (the
product-CLI tests over all six corpora), `src/fixtures/go/release/v0_2/`
(the six fixture corpora),
ADR-0021, ADR-0041, ADR-0050, and prerequisite
`abda602fe38db8549e4f318bd4638ddb94df7282`. Exact non-claims: package
presence does not prove selection, installation, buildability, symbol
identity, testing semantics, or runtime behavior; and the source anchor
proves a declaration's shape, not that the file's tests run or that the
declaring package resolves.
