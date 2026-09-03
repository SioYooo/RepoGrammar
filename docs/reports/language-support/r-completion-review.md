# R language completion review

- Status: Incomplete — `structural_substrate`
- Authority: ADR-0020, ADR-0036, and ADR-0042
- Dependency prerequisite: `82ced893f81a954b64e20546dfc4ad81043b28e5`
- Last updated: 2026-09-04

## ADR-0020 gate

- [x] Discovery/configuration — exact `.R`/`.r`, `DESCRIPTION`, `NAMESPACE`,
  and `renv.lock`, R-specific exclusions, bounded dependency metadata, typed
  inventory uncertainty, persistence, and incremental behavior are covered.
- [x] Authoritative frontend for the declared scope — ADR-0042's bounded
  in-process recursive-descent parser reads `tests/testthat/test-*.R` only. It
  parses the declared R subset of ADR-0042 D4 into a real expression tree with
  R's own token set, precedence, associativity, and newline rule, and the anchor
  is a question asked of that tree rather than of byte positions. ADR-0042 D4a
  supplies the invariance the gate asks for: R's grammar is fixed and
  configuration-free — no preprocessor, dialect selector, conditional
  compilation, macro layer, or include mechanism — and the one remaining axis,
  the interpreter version, is pure accretion, so no R version reads an admitted
  file into a different tree. The two constructs where that would fail,
  option-gated `=>` and locale-classified non-ASCII identifier bytes, are refused
  rather than read. Outside the subset the parser abstains, never recovers, and
  never resynchronizes. ADR-0036 carries no evidence ladder and no prohibition on
  this route; its restrictions all name executing R, and nothing here runs. No
  selected project model or profile exists, and none is claimed.
- [x] RepoGrammar-owned R source code units and IR — a module unit per admitted
  file, one `r_test_that_block` unit per admitted call, source ranges, content
  hashes, IR nodes, and containment edges.
- [x] Complete source-semantic obligation registry and provider fallback —
  `R_OBLIGATION_REGISTRY` in `src/rust/adapters/parsing/r/testthat.rs` is the
  single table that declares every typed `UNKNOWN` the testthat frontend can
  emit, and all emission paths route through it, so an unregistered unknown is
  a typed error rather than a possible output. Twelve entries cover the
  obligations the `testthat.test_that` family claim rests on: the two blocking
  identity obligations (undeclared testthat dependency, in-file rebinding of
  `test_that`), the conditional-registration reach obligation, the
  description-literal obligation (a top-level two-positional call whose
  description is not a source-visible plain literal now records why it was not
  anchored), the four parse-boundary obligations, and four residuals the
  bounded parse can never discharge — callee binding beyond the file
  (`r_testthat_callee_binding`, standing on every anchor), the quoted block's
  NSE/assertion semantics (`r_testthat_block_semantics`, standing on every
  anchor), S3/S4 dispatch (`r_dispatch_target`, triggered by a parsed
  `UseMethod`/`NextMethod`/`setClass`/`setMethod`/`setGeneric`/`setRefClass`/
  `callGeneric`/`callNextMethod` call), and native/package/namespace symbol
  resolution (`r_external_symbol_resolution`, triggered by `library`/
  `require`/`.Call`/`.C`/`.Fortran`/`.External` and `::`/`:::` access). Each
  entry records its reason code, its claim scope, whether an unmet obligation
  blocks the family claim, and its provider-fallback policy. No R semantic
  provider slot exists or is registered, so every residual classifies
  irreducible-under-current-constraints (`manual_review_required` recovery in
  the unknowns inventory); the identity obligations recover only through the
  repository's own metadata, and counts may fall only via source-backed
  replacement facts. `application/family.rs` remains the authoritative
  claim-impact classifier and implements exactly the recorded split
  (MissingDependency/MonkeyPatch on `r_testthat_identity` block;
  `r_conditional_test_registration` is a non-blocking subclaim); a registry
  test pins that only identity entries may record a blocking impact.
- [x] Exact family with support at least three — `testthat.test_that` over the
  fixed `testthat.test_that` target, minimum support three rather than the
  shared default of two, and the owned `repogrammar-r-derived` origin.
- [x] Positive, lookalike, low-support, and parse-degraded family fixtures.
  Leaving the declared subset — an unclosed quote or brace, an option-gated or
  locale-dependent construct, a chained comparison, or nesting past the bounded
  depth — reports a degraded parse and a typed `UNKNOWN`, and the expressions
  after the boundary are unread rather than guessed. Every registry obligation
  additionally has a frontend fixture proving its `UNKNOWN` fires on the
  triggering construct and not on the admitted plain case, plus determinism
  and stable-count tests. Unresolved/resolved fixture pairs do not exist:
  there is no R provider to resolve against.
- [x] Complete source-free readiness and leakage matrix across required public
  surfaces — R is registered in the repo-shape language scopes, so its units and
  families are counted rather than silently reported as zero; `status`,
  `doctor`, `stats`, `unknowns`, `families`, `files`, and the MCP
  `inspect_readiness` and `find_analogues` payloads are each asserted over both
  an indexed declared-testthat workspace and an indexed undeclared one to expose
  no identifier, literal, or source text and no absolute path. The assertions
  are non-vacuous: every command must exit zero and parse, the declared
  workspace must report `framework:testthat.test_that`, and the undeclared one
  must report the lane's typed `UNKNOWN` by bounded language token `r` and
  count.
- [x] Four-part review record — this report records correctness, security,
  completeness, and performance findings; open findings remain blockers.
- [ ] Linked atomic prerequisites and final completion audit.

## Current evidence and blocker

Exactly one path class is decoded: `tests/testthat/test-*.R`, the set testthat's
own `test_dir()` runner scans, and only after a `DESCRIPTION` in the repository
declares testthat. Every other `.R` and `.r` byte stays inventory and is never
read. The parse itself is the bounded in-process one ADR-0042 D4 declares; it
loads no grammar and no external artifact.

The metadata adapter is static and non-executing. DESCRIPTION/NAMESPACE package
declarations abstain when CRAN versus Bioconductor cannot be proved. renv rows
retain unknown directness and scope; remote/custom/local sources are omitted
without retaining their values. No R, renv, profile, package, native code,
repository script, child process, or network action runs.

## Completion verdict

Not complete. R has a bounded recursive-descent parser over one path class and
one call shape, owned units and IR, a complete claim-scoped source-semantic
obligation registry with provider-fallback policy, one exact family with
support three, and an audited source-free readiness matrix. It has no final
audit and no provider. Strict gate count is `8/9`;
R is `structural_substrate` and must not be counted as supported.

Two limitations are worth stating rather than leaving to inference.

The parser recognizes only its declared subset, so a file using a construct
outside it stops being read at that point. That is deliberate — the alternative
is guessing — but it means a repository's support count can understate the tests
it really has. The degraded diagnostic and the `r_test_parse` `UNKNOWN` say when
this happened; they do not say how many calls were missed.

The anchor is a claim about a call's shape, not about what the name resolves to.
A binding of `test_that` inside the file is decidable and blocks under
ADR-0042 D2b, but `library()` masking, a `source()`d or testthat-loaded helper,
`attach()`, and a non-literal `assign()` are not statically determinable. Those
residuals are no longer silent: the registry records them as the standing
`r_testthat_callee_binding` and `r_testthat_block_semantics` unknowns on every
admitted anchor plus the triggered `r_dispatch_target` and
`r_external_symbol_resolution` unknowns, each irreducible under current
constraints. Closing them needs an evaluator, which is exactly what ADR-0036
forbids, so they stay typed `UNKNOWN` rather than becoming claims.

## Final program audit fields

| Required field | Audited result |
|---|---|
| Language / rank | R / 9 |
| Dialect/version | R has no dialects. No R release, platform, project, library path, profile, renv activation, native-code, or repository selection is made or claimed, and ADR-0042 D4a shows the admitted parse does not need one: R's 4.x additions are pure accretion, so no R version reads an admitted file into a different tree. |
| Provider/frontend/version | Bounded in-process `repogrammar-r-testthat-parser` / `bounded_r_test_that_v2`, a hand-written recursive-descent parser over the ADR-0042 D4 subset. No external R parser, grammar, languageserver, compiler/runtime, or provider. |
| Discovery/config | `.R`, `.r`, `DESCRIPTION`, `NAMESPACE`, and `renv.lock` with managed-library/IDE exclusions. |
| Manifest/lockfile | Bounded DESCRIPTION/NAMESPACE declarations plus exact explicit CRAN/Bioconductor `renv.lock` versions; ambiguous registries and remote/custom/local sources are omitted. |
| Owned source IR / external symbols | Owned units exist for the ADR-0042 anchor only, and the IR abstains on their kind because a testthat block is a call, not a declaration; external symbols stay absent, and package imports, S3/S4/R6 dispatch, native symbols, NSE, and generated code are unresolved. |
| Library Contracts | Registry exists, production packs = 0; inventory never creates a behavior contract. |
| Exact family / fixtures | One exact family, `framework:testthat.test_that` over the testthat block anchor, gated at support three. Positive, lookalike, low-support, and parse-degraded fixtures exist; resolved/unresolved do not, because there is no R provider. |
| Primary UNKNOWN cases | `r_testthat_identity` (undeclared testthat dependency; a file that rebinds `test_that` itself — both blocking), `r_conditional_test_registration` (non-blocking reach), `r_testthat_description_literal` (description not a source-visible plain literal), `r_test_parse` (source outside the declared subset, byte/unit/depth bounds), `r_testthat_callee_binding` and `r_testthat_block_semantics` (standing on every anchor), `r_dispatch_target` (S3/S4 generic registration or dispatch), `r_external_symbol_resolution` (`library`/`require`/dotted native entries/`::`/`:::`), plus the inventory lane's `r_dependency_inventory` tokens for repository identity, selected lock/project/profile, remote sources, and package directness/scope. All source-semantic residuals share one provider fallback: no R semantic provider slot exists or is registered, so they classify irreducible under current constraints with `manual_review_required` recovery. |
| Source-free / security | Metadata results are source-free; only `tests/testthat/test-*.R` bytes are read, every other `.R`/`.r` byte stays zero-read, and no source text, identifier, or literal reaches a readiness surface; input is bounded by byte, unit, and recursion-depth limits; no R, renv, package/profile script, native code, child, repository code, or network runs. |
| Completion state / counted | `structural_substrate`; strict gate count `8/9`; Top-20 complete = no. |

Four-part review: correctness preserves only explicit registry evidence, never
defaults ambiguous packages to CRAN, establishes the anchor from a parsed
expression tree rather than from byte positions, so a conditionally reached,
assignment-captured, piped, or rebound call is not mistaken for a registered
test, and routes every typed `UNKNOWN` through one obligation registry whose
blocking records match the authoritative family classifier; security discards
remote/path values, bounds input by bytes, units, and recursion depth, executes
nothing, and keeps every registry note, claim, and kind a fixed source-free
string; completeness lacks the provider layer and the final audit; performance
is linear in file bytes for the bounded parse and for DCF/JSON inventory, with
no R-runtime or large-lock benchmark. Evidence:
`src/rust/adapters/parsing/r/testthat.rs` (the `R_OBLIGATION_REGISTRY` gate 4
table and its emission + registry tests), `src/rust/adapters/languages/r.rs`,
`src/rust/adapters/parsing/r.rs`, `src/rust/adapters/frameworks/r.rs`,
ADR-0036, ADR-0042, product/incremental tests, and
`82ced893f81a954b64e20546dfc4ad81043b28e5`. Exact non-claims: package metadata
does not prove installation, loading, namespace binding, dispatch, native
compatibility, or R support; and the source anchor proves a call's shape, not
that `test_that` resolves to testthat's function or that the file's tests run.
