# Ruby language completion review

- Status: Incomplete — `structural_substrate`
- Authority: ADR-0020, ADR-0022, and ADR-0049
- Dependency prerequisite: `6a6f88538090ef386a8420c7424a380d97fa1f74`
- Last updated: 2026-09-05

## ADR-0020 gate

- [x] Discovery/configuration — unchanged from ADR-0022 and already audited
  there: `.rb` source paths, Bundler manifests and locks, gemspecs, and
  `.ruby-version` are inventoried without running Ruby or Bundler, and the
  bounded direct-`DEPENDENCIES` reader keeps its typed
  `ruby_dependency_inventory` uncertainty. ADR-0049 D1 narrows source
  admission one step further: only a path whose normalized repository-relative
  form has a `test` or `tests` component and whose basename ends `.rb` and
  either begins `test_` or ends `_test.rb` (never the bare `_test.rb`) is ever
  decoded. The rule is case-sensitive and component-exact, so `spec/`,
  a repository-root `test_catalog.rb`, and a `.RB` suffix all stay zero-read
  inventory (`is_minitest_path`, pinned by
  `only_runner_scoped_paths_are_admitted`).
- [x] Authoritative frontend for the declared scope — ADR-0049's bounded
  in-process lexer (`src/rust/adapters/parsing/ruby/lexer.rs`) and structural
  parser and frontend (`src/rust/adapters/parsing/ruby/minitest.rs`). The
  lexer is real, not a scanner: heredocs in every spelling are tracked to
  their exact terminator line, `%`-literals match nesting delimiters,
  interpolations nest through strings, braces, and comments, and regexes,
  symbols, labels, character literals, `=begin` blocks, and `__END__` are
  single opaque tokens, so a `class`, `def`, or `end` written inside any of
  them can never move the structural grammar. The anchor is a question about
  the parsed declaration structure, not about byte positions. ADR-0049 D4a
  supplies the invariance argument: the invariance set is CRuby 3.x, every
  admitted form predates it and lexes identically across it, and the two
  readings that depend on MRI's runtime local/command distinction — a slash
  and a `<<`-heredoc starter after a mid-statement identifier — abstain the
  whole file rather than being guessed. Outside the declared subset the parse
  abstains, never recovers, and never resynchronizes. The ADR-0022 D5-4
  forbidden rung (regex/text matching for the claim) is discharged by this
  parse per the ADR-0042..0046 precedent; the D9 sandboxed-Prism rung is
  explicitly **not** claimed and ADR-0022 D3/D7 stay in force.
- [x] RepoGrammar-owned Ruby source code units and IR — a module unit per
  admitted file, one `ruby_minitest_test_class` unit per qualifying top-level
  class, one `ruby_minitest_test_method` unit per admitted `def test_*`,
  source ranges, content hashes, IR nodes, and both module and class-to-method
  containment edges through the shared parsing substrate
  (`ir_links_module_to_class_and_method_units`). The class-to-method edges are
  new this session: the shared `is_class_like`/`is_method_like` tables omitted
  both Ruby kinds, so the lane produced module edges only until they were
  added.
- [x] Claim-scoped typed `UNKNOWN` set and provider fallback — Ruby carries
  no registry table of the `PHP_OBLIGATION_REGISTRY` shape; like the Swift
  lane it enumerates its abstentions in the ADR (ADR-0049 D3, D4) and routes
  every one through a single `unknown_fact` emission helper, so the emitted
  vocabulary is closed and fixed. Three claims carry it. `ruby_parse` covers
  the parse boundaries — `unadmitted_ruby_construct`, `unterminated_string`,
  `unterminated_regexp`, `unterminated_percent_literal`,
  `unterminated_heredoc`, `ruby_structural_failure`, and the three input
  bounds `source_byte_limit`, `parser_depth_limit`, `parser_resource_limit` —
  each `InsufficientSupport`. `ruby_lexical_invariance` covers the two refused
  runtime-dependent readings, `ruby_slash_disambiguation` and
  `ruby_heredoc_disambiguation`, as `ConflictingFacts`. The third claim,
  `ruby_minitest_superclass`, carries `test_methods_without_minitest_base` as
  `UnresolvedImport` when a file on an admitted path declares direct `test_*`
  instance methods in a class whose in-file chain never reaches
  `Minitest::Test` or `ActiveSupport::TestCase`. No Ruby semantic provider
  slot exists or is registered, so the identity residuals are irreducible
  under current constraints; they ride each anchor as the standing
  `provider_resolved=false` assumption and the bounded `minitest_require=`
  context token rather than as per-anchor `UNKNOWN` facts.
- [x] Exact family with support at least three in the product support gate —
  the frontend emits the fixed `ruby.minitest.test_method` support target
  (engine `repogrammar-ruby-minitest-parser`, method
  `bounded_ruby_minitest_v1`; the derived-support arm mints
  `repogrammar-ruby-derived` / `bounded_ruby_minitest_v1` facts), the
  framework adapter `src/rust/adapters/frameworks/ruby.rs` supplies the role
  `framework:minitest.test_method` and the family token
  `ruby.minitest.test_method`, and the shared detector, the
  `application/family.rs` compatibility and derived-support arms, the
  `application/indexing.rs` source admission, and the sqlite repo-shape
  allowlists are all wired. `min_family_support` pins **3** for `ruby`
  explicitly rather than inheriting the shared default of two, which the
  low-support fixture's two anchors now pin. Measured through the product CLI
  today: `minitest_exact_tests` forms
  `family:ruby:ruby_minitest_test_method:framework_minitest_test_method` with
  support **8**, and `minitest_lookalikes`, `minitest_low_support`, and
  `minitest_parse_degraded` each index to `status == "complete"` and form no
  family.
- [x] Positive, lookalike, low-support, and parse-degraded family fixtures —
  `src/fixtures/ruby/release/v0_2/`: `minitest_exact_tests` (three files, eight
  anchors — a `Minitest::Test` subclass whose bodies carry a squiggly heredoc
  containing a fake `class`/`def`, a `%w` list with nested delimiters, quoted
  symbols and a nested string inside an interpolation; an in-file superclass
  chain through `OrdersBaseTest` with a `test-unit` require token; and a third
  file with regexes and division in admitted positions, a character literal,
  and an indented heredoc terminator), `minitest_lookalikes` (a class with no
  superclass, a class whose superclass does not resolve in file, and a real
  subclass carrying a class method, a parameterized method, a
  `private def`, and a non-`test_` name — one admitted anchor in total, plus
  class headers inside a comment, a string, and a heredoc that must stay
  inert), `minitest_low_support` (exactly two anchors, pinning the 2-vs-3
  boundary), and `minitest_parse_degraded` (four whole-file abstentions: an
  unterminated `%w` list, an unterminated heredoc, division after a
  mid-statement identifier, and a `%`-literal inside a `#{}` interpolation).
  Unresolved/resolved fixture pairs do not exist: there is no Ruby provider to
  resolve against.
- [x] Source-free readiness and leakage review — the frontend surfaces are
  asserted over the whole committed fixture matrix through the product
  dispatch (`ruby_readiness_output_stays_source_free_and_low_cardinality`): no
  fixture class name, method name, local, or sentinel reaches any fact target,
  note, assumption, diagnostic, or code-unit id, and a non-literal `require`
  argument is recorded as `minitest_require=non_literal` without retaining its
  text. Every emitted note, claim, and kind is a fixed string, and
  `every_frontend_fact_stays_below_family_supporting_certainty` pins that no
  frontend fact claims family-supporting certainty on its own. Product-level
  `init`/`index`/`families` tests over every Ruby fixture live in
  `src/rust/bin/repogrammar.rs`. One qualifier belongs on the record: the
  shared CLI matrix helper `assert_scanner_lane_readiness_is_source_free`
  (`status`, `doctor`, `stats`, `unknowns`, `families`, `files`,
  `inspect_readiness`) is still called only for the delphi, ada, matlab,
  visual-basic, and r lanes. Ruby's `stats` and `unknowns` output was
  re-inspected empirically on 2026-09-05 and exposed only bounded tokens,
  counts, role names, and reason codes; that check is not yet pinned by a
  committed assertion.
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

Exactly one admission class is decoded: a `.rb` file under a `test`/`tests`
path component whose basename carries a Minitest runner name shape. Every
other Ruby byte stays inventory. The parse is the bounded in-process one
ADR-0049 D4 declares; it loads no grammar, adds no crate, downloads no
artifact, and runs no Ruby, Bundler, RubyGems, Rake, or Rails. The
`Gemfile.lock` direct-`DEPENDENCIES` inventory of ADR-0022 is unchanged and
remains the only Ruby configuration evidence.

The anchor requires a top-level `class` header on one logical line whose
superclass chain, resolved through top-level headers in the same file across
at most eight hops with cycles refused, terminates at `Minitest::Test` or
`ActiveSupport::TestCase`, and directly in that class body a `def test_*`
instance method with no parameters. `class << self` bodies, module-wrapped and
nested classes, `def self.test_x`, parameterized tests, `private def`, and any
`def` reached only inside a nested body, block, or conditional parse
structurally but never anchor. The top-level `require` line is context, not a
precondition: each class anchor carries one low-cardinality
`minitest_require=` token (`autorun`, `test_unit`, `literal_other`,
`non_literal`, `absent`) and never the literal argument text.

No integration blocker remains. What is open is gate 9's commit shape, and one
known limitation: `readiness_language_scopes()` in `application/query.rs` —
the list deciding which languages the public `stats` surface reports with a
`language_scope`/`preview_status` — still contains only python,
typescript/javascript, rust, java, csharp, and c/cpp. It was deliberately not
extended, because that list is a product support-scope claim requiring its own
decision rather than internal counting. Ruby's repo-shape counts therefore
exist in storage and are not surfaced through the public per-language stats
rows.

Four defects found this session applied to this lane and are recorded rather
than left implied. `min_family_support` fell through to the shared default of
two, so a pair of anchors would have formed a family this review says must not
form. The Ruby slash-abstention note contained a bare `/`, which the
semantic-fact text validator rejects as an embedded absolute path, so `index`
failed outright with `semantic fact note contains unsupported content` on the
committed slash fixture; the note now spells the operators out. The positive
fixture `minitest_exact_tests/test/test_catalog.rb` used a `%w[...]` literal
inside a `#{}` interpolation, which the declared subset deliberately refuses,
so the whole file abstained and the positive corpus silently lost four of its
nine anchors — support was 5 and is now 8; the fixture stays inside the
subset and a new
`minitest_parse_degraded/test/test_percent_in_interpolation.rb` pins that
boundary where it belongs. And the shared `is_class_like`/`is_method_like`
tables omitted both Ruby kinds, so no class-to-method IR containment edge was
produced; a containment-pair guard test now covers every class-bearing lane.

## Completion verdict

Not complete. Ruby has a bounded lexer and structural parser over one
admission class and one declaration anchor, owned units and IR with
containment, a closed typed-`UNKNOWN` vocabulary with its provider-fallback
position stated, a registered framework role, a family that forms in product
output at the three-member bar, committed positive, lookalike, low-support,
and parse-degraded fixtures, and source-free evidence at the frontend and
product-CLI boundaries. It has no provider and no final audit. Strict gate
count is `8/9` with gate 9 open on the commit-shape ruling above; Ruby is
`structural_substrate` and must not be counted as supported.

Two limitations are worth stating rather than leaving to inference.

The parser recognizes only its declared subset, so a file using an endless or
operator method definition, a multi-line class header, a non-ASCII identifier,
a `%`-literal or heredoc or slash inside an interpolation, or either refused
disambiguation contributes no anchors at all. That is deliberate — the
alternative is reading under an assumption — but a repository's support count
can understate the tests it really has. The typed `UNKNOWN` says which class
fired; it does not say how many anchors were missed. The spec DSL
(`test "description" do`), `define_method`-generated tests, aliases, and
inherited test methods are named non-claims, not silent omissions.

The anchor is a claim about a declaration, not about execution. Whether
Minitest selects, runs, orders, or passes the test, whether `Minitest::Test`
resolves to the gem's constant rather than one rebound by a helper or by
load-path mutation, and whether inherited or dynamically defined tests exist
are all outside it, carried by the standing `provider_resolved=false`
assumption.

## Final program audit fields

| Required field | Audited result |
|---|---|
| Language / rank | Ruby / 20 |
| Dialect/version | ADR-0049 D4a's invariance set is CRuby 3.x; no engine, `.ruby-version`, gemset, or project profile is selected or claimed. The constructs where Ruby 3.x added readings either never reach the structural grammar or are refused; the two runtime-dependent lexical readings abstain. JRuby and TruffleRuby are not claimed. The ambient host Ruby is not evidence and is never consulted. |
| Provider/frontend/version | Bounded in-process `repogrammar-ruby-minitest-parser` / `bounded_ruby_minitest_v1`, a hand-written lexer plus structural end-matching parser over the ADR-0049 D4 subset. No Prism, CRuby, JRuby, Bundler, RubyGems, Tree-sitter Ruby, or provider; ADR-0022 D9's sandboxed native-Prism qualification track remains open and unclaimed. |
| Discovery/config | `.rb`, Bundler manifests/locks, gemspecs, and `.ruby-version` inventory with Ruby-specific exclusions (ADR-0022, unchanged); source decoded only for `test`/`tests`-scoped `test_*.rb` and `*_test.rb` basenames. |
| Manifest/lockfile | Direct root rows from exact `Gemfile.lock` `DEPENDENCIES`, unchanged; no resolved-spec join, group/scope, or executable Gemfile/gemspec evaluation. |
| Owned source IR / external symbols | Owned units for the ADR-0049 anchor only (`ruby_minitest_test_class`, `ruby_minitest_test_method`, plus the file's module unit), projected with module and class-to-method containment. External symbols stay absent; `require`/`load`/`autoload` graphs, constant rebinding, refinements, metaprogramming, inherited and dynamically defined tests, and gem ownership are unresolved or non-claims. |
| Library Contracts | Registry infrastructure exists, production packs = 0; inventory rows and anchors create no behavior contract. |
| Exact family / fixtures | One family target, `framework:minitest.test_method` over the `ruby.minitest.test_method` anchor, family id `family:ruby:ruby_minitest_test_method:framework_minitest_test_method`, support bar three pinned explicitly for `ruby`. Measured: positive corpus support 8; lookalike (one anchor), low-support (two anchors), and four parse-degraded abstentions form no family and index complete. Resolved/unresolved pairs do not exist, because there is no Ruby provider. |
| Primary UNKNOWN cases | `ruby_parse` (`unadmitted_ruby_construct`, `unterminated_string`, `unterminated_regexp`, `unterminated_percent_literal`, `unterminated_heredoc`, `ruby_structural_failure`, `source_byte_limit`, `parser_depth_limit`, `parser_resource_limit`), `ruby_lexical_invariance` (`ruby_slash_disambiguation`, `ruby_heredoc_disambiguation`), `ruby_minitest_superclass` (`test_methods_without_minitest_base`), plus the inventory lane's `ruby_dependency_inventory` tokens from ADR-0022. All source-semantic residuals share one fallback: no Ruby semantic provider slot exists or is registered and ADR-0022 forbids executing Ruby or its tooling, so they classify irreducible under current constraints. |
| Source-free / security | Only runner-scoped `.rb` bytes cross the source-store boundary; no source text, identifier, literal, or absolute path reaches a fact, note, assumption, diagnostic, or unit id (asserted over the whole fixture matrix through the product dispatch); notes are fixed strings that pass the stored-assumption content rules; input is bounded by bytes, 4,096 units, 256 region-nesting levels, and eight superclass hops; nothing executes. |
| Completion state / counted | `structural_substrate`; strict gate count `8/9` with gate 9 open on the commit-shape ruling; Top-20 complete = no. |

## Four-part review

**Correctness.** Every anchor is established from the parsed token stream:
the superclass chain is followed through in-file top-level headers with a hop
bound and cycle refusal, and method admission requires a direct class-body
`def`, the `test_` prefix with a non-empty suffix, no parameters, and no
`self.` receiver. Declaration text inside comments, strings, heredocs,
`%`-literals, and regexes cannot contribute because each is one opaque token,
which the lookalike fixture pins with fakes in all three positions. Refusals
are whole-file except the structural-failure case, which keeps only the
constructs that closed before the failure point and never resynchronizes past
it. Findings: none open at frontend level; module-wrapped classes, spec-DSL
tests, `define_method` tests, and inherited tests understate by design and are
recorded, not silent. One correctness defect was found and fixed this session
— the positive fixture itself left the declared subset and silently cost the
corpus four anchors, which is exactly the failure mode the low-support and
degraded fixtures exist to expose.

**Security.** Repository contents are untrusted. Input bytes, emitted unit
counts, and region nesting depth are bounded with typed refusals, and the
path rule keeps the decoded byte class narrow. No Ruby, Bundler, RubyGems,
gemspec or Gemfile DSL, Rake, Rails, native extension, child process, or
network resolution runs. Fixture identifiers and sentinels are asserted absent
from every emitted surface. One security-adjacent defect is recorded above: a
note containing a bare `/` was rejected by the semantic-fact text validator as
an embedded absolute path and failed `index` outright — the validator held,
the note was wrong, and it now spells the operators out.

**Completeness.** Gate 9 is open on the commit-shape ruling. Recall is
deliberately low: the refused disambiguations, endless and operator method
definitions, multi-line class headers, non-ASCII identifiers, and nested
literal-in-interpolation forms all abstain whole-file, each with a typed
reason. The public `stats` per-language rows do not yet report Ruby, by the
deliberate `readiness_language_scopes()` decision recorded above. Widening the
admitted path set, anchor shape, lexer subset, or claim surface requires a
superseding ADR.

**Performance.** The parse is one linear lexing pass plus one linear
end-matching walk with iterative statement consumption — O(file bytes), no
backtracking, no generated parser, and depth-bounded recursion. No Ruby
runtime, Prism, large gem-graph, or large-repository benchmark exists for this
lane; resource exposure is bounded by the shared discovery ceilings, the
shared input-byte limit, and the 4,096-unit and 256-depth ceilings.

Evidence: `src/rust/adapters/parsing/ruby/lexer.rs` and
`src/rust/adapters/parsing/ruby/minitest.rs` (the frontend and its lexer,
anchor, fixture, and source-free tests), `src/rust/adapters/languages/ruby.rs`
and `src/rust/adapters/parsing/ruby.rs` (discovery classification and the
Bundler inventory, unchanged), `src/rust/adapters/frameworks/ruby.rs` with its
registration in `src/rust/adapters/frameworks/mod.rs`,
`src/rust/application/family.rs` and `src/rust/application/indexing.rs` (the
support-compatibility, derived-support, and admission arms),
`src/rust/bin/repogrammar.rs` (the product-CLI family tests over every Ruby
fixture), the fixtures under `src/fixtures/ruby/release/v0_2/`, ADR-0022,
ADR-0049, and prerequisite `6a6f88538090ef386a8420c7424a380d97fa1f74`.

Exact non-claims: the Bundler declaration inventory is not installed or
resolved gem evidence; the anchor proves a source-visible declaration shape
under an in-file superclass chain, not that `Minitest::Test` binds to the
gem's constant, that Minitest selects, runs, or passes the test, or that the
file is in any suite; and nothing here proves Ruby language support beyond the
declared subset of ADR-0049 D4, nor any behavior of an arbitrary third-party
library.
