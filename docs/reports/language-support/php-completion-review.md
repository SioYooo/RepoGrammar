# PHP language completion review

- Status: Incomplete — `structural_substrate`
- Authority: ADR-0020, ADR-0024, and ADR-0047
- Dependency prerequisite: none linked yet (first lane commit)
- Last updated: 2026-10-14 (open admission-contract drift; historical score retained)

## 2026-10-14 open admission-contract drift

**OPEN CONTRACT_DRIFT — current G1 is PARTIAL / NOT REQUALIFIED.**
[ADR-0047 D1](../../decisions/ADR-0047-bounded-php-phpunit-frontend.md)
requires a non-prologue `.php` input to remain zero-read inventory and never
cross the source-store boundary. That accepted requirement is unchanged; the
implementation cannot silently relax it by moving admission into the parser.

Current source contradicts that specific admission guarantee:

1. `application/indexing.rs::file_is_inventory_only` admits every valid `.php`
   path through `is_phpunit_source_path`; it has no prologue evidence.
2. `index_repository_full_after_discovery` calls `read_source_for_parsing` for
   non-inventory paths before constructing the parser's `SourceDocument`.
   That helper invokes `SourceStore::read_source`; the incremental path also
   calls the same helper.
3. `adapters/parsing/php/phpunit.rs::parse_output` checks
   `lexer::prologue_code_start(document.text)` only after the text has crossed
   that boundary, returning `UnsupportedLanguage` if the prologue is missing.

Evidence: [indexing source](../../../src/rust/application/indexing.rs) (full
loop at lines 804–841, PHP classifier at 6350–6353, read helper at 6479–6497)
and [PHP parser](../../../src/rust/adapters/parsing/php/phpunit.rs)
(lines 287–296), inspected at the campaign's `4916f57` baseline. This is a
source-established contract mismatch, not a new product execution or a claim
that non-prologue input creates a family. The parser's rejection does not prove
the stronger zero-read requirement, and the passing general suite does not
close that missing boundary assertion.

The **8/9 below is the 2026-09-05 historical audit**, not a newly qualified
current score. The specific D1 zero-read subclaim fails the source review; G1 as
a whole is partial because discovery/configuration and inventory evidence still
exist. No numeric recount or language-completion promotion is made here. Strict
Top-20 remains 0/20. Closure needs either a conforming admission implementation
with a product-level non-prologue source-read sentinel, or a separately approved
ADR change; this campaign implements neither and authorizes neither broader
source reads nor a new PHP provider.

## ADR-0020 gate (2026-09-05 historical audit)

- [x] Discovery/configuration — unchanged from ADR-0024 D4 and already
  audited there: exact case-sensitive `.php` source suffix, exact
  `composer.json`/`composer.lock`/`phpunit.xml`/`phpunit.xml.dist` config
  basenames with config precedence, global `vendor` exclusion, PHP-only
  `.composer` and `.phpunit.cache` exclusions, bounded static Composer
  dependency inventory with typed `php_dependency_inventory` uncertainty,
  and the shared discovery ceilings. ADR-0047 D1 narrows source admission
  one step further: only `.php` files whose bytes begin with the `<?php`
  prologue are ever decoded, and every other `.php` file stays zero-read
  inventory.
- [x] Authoritative frontend for the declared scope — ADR-0047's bounded
  in-process lexer and recursive-descent parser over the declared subset of
  ADR-0047 D4. The anchor is a question about the parsed declaration
  structure, not about byte positions, and ADR-0047 D4a supplies the
  invariance argument: the admitted lexical forms tokenize identically
  across the declared {PHP 7.4, 8.0–8.5} set; the one divergence inside the
  subset surface (`#[` is an attribute under 8.x and a comment under 7.4) is
  bounded by the same-line-closure rule, with a spanning attribute refused
  whole-file under `php_dialect_invariance` and the marker-existence
  divergence stated with its safe direction. Outside the subset the parser
  abstains whole-file, never recovers, and never resynchronizes. The
  ADR-0024 D6 forbidden rung (text-only matching for the claim) is
  discharged by this parser per the ADR-0042..0046 precedent; the D2
  primary-evidence rung (qualified sandboxed Mago/PHP-Parser/PHP CLI
  frontend) is explicitly **not** claimed.
- [x] RepoGrammar-owned PHP source code units and IR — a module unit per
  admitted file, one `php_test_class` unit per resolved test class, one
  `php_test_method` unit per admitted method, source ranges, content hashes,
  IR nodes, and both module and class-to-method containment edges through the
  shared substrate helpers. The class-to-method edges are new this session:
  the shared `is_class_like`/`is_method_like` tables omitted `php_test_class`
  and `php_test_method`, so the lane produced module edges only until they
  were added.
- [x] Claim-scoped typed `UNKNOWN` registry — `PHP_OBLIGATION_REGISTRY` in
  `src/rust/adapters/parsing/php/phpunit.rs` is the single table that
  declares every typed `UNKNOWN` the PHPUnit frontend can emit, and all
  emission paths route through it, so an unregistered unknown is a typed
  error rather than a possible output. Twelve entries cover: the three
  parse-boundary refusals (unadmitted construct, unterminated literal,
  dialect-divergent attribute span) and the three input bounds (bytes,
  depth, units); the ancestry obligation (test markers in a class whose
  in-file chain does not reach a `TestCase`-suffixed terminus); the
  method-shape obligation (markered methods that are not the admitted
  public/non-static/non-abstract/zero-parameter shape); the data-provider
  obligation (`@dataProvider`/`#[DataProvider…]`/`#[TestWith…]`
  attachments); the trait-origin obligation; and two standing residuals
  riding every anchor (PHPUnit runtime selection/execution unproven;
  TestCase-suffix naming-shape evidence rather than lineage proof), each
  with its provider-fallback policy. No entry blocks the family claim at
  repository level: whole-file abstentions drop only their own file's
  anchors, and every other obligation excludes the affected declaration.
  The registry records the two standing residuals under
  `no_php_provider_irreducible`: no PHP semantic provider slot exists or is
  registered, the repository's zero-external-dependency constraint admits
  none, and ADR-0024 forbids executing PHP, so the obligation cannot be
  discharged in practice. The product's `unknowns` surface answers a
  different question and reports them on its own axis — on
  `phpunit_exact_tests` it emits `recoverable_unknowns: 13`,
  `irreducible_unknowns: 0`, with `not_implemented_in_current_version`
  recovery for the six `framework_semantic_provider` residuals — that
  generic mechanism token is claimed by a registered but unintegrated
  provider slot — and `manual_review_required` for the six
  `import_resolution_provider` residuals, which no slot claims, plus the
  inventory lane's one governance residual. Both statements are true of the
  same facts.
- [x] Exact family with support at least three in the product support gate —
  fixed support target `phpunit.TestMethod`, framework adapter registry
  `src/rust/adapters/frameworks/php.rs` with role
  `framework:phpunit.test_method`, family token `php.phpunit.test_method`,
  and the adapter registered in the shared framework-role detector. The
  repository-level wiring is landed: `application/family.rs` carries the PHP
  support-compatibility and derived-support arms
  (`repogrammar-php-derived` / `bounded_php_phpunit_v1`),
  `application/indexing.rs` admits `.php` source instead of classifying it
  inventory-only, and the sqlite repo-shape allowlists count the language.
  `min_family_support` pins **3** for `php` explicitly rather than inheriting
  the shared default of two, which the low-support fixture's two anchors now
  pin. Measured through the product CLI today: `phpunit_exact_tests` forms
  `family:php:php_test_method:framework_phpunit_test_method` with support
  **6**, and `phpunit_lookalikes`, `phpunit_low_support`, and
  `phpunit_degraded` each index to `status == "complete"` and form no
  family.
- [x] Positive, lookalike, low-support, and parse-degraded family fixtures —
  `src/fixtures/php/release/v0_2/`: `phpunit_exact_tests` (three distinct
  real-world-shaped test classes exercising all three markers — bare `test`
  prefix, `/** @test */` doc comment, `#[Test]` attribute with namespaced
  imports — plus an in-file `InvoiceTestCase` chain, six method anchors,
  support ≥ 3), `phpunit_lookalikes` (non-TestCase helper class, private and
  protected and static test-prefixed methods, abstract base members, free
  function, zero anchors), `phpunit_low_support` (two methods, one below the
  support threshold), and `phpunit_degraded` (close-tag inline HTML and a
  line-spanning attribute, both whole-file abstentions with typed
  refusals). Unresolved/resolved fixture pairs do not exist: there is no PHP
  provider to resolve against.
- [x] Source-free readiness — parser-level readiness is tested beside the
  module: the fixture-corpus test drives every release fixture through the
  product dispatch and asserts that identifiers, literals, secrets, and
  absolute paths never reach the parse surface, that every unknown exposes
  only the registered bounded kind vocabulary, and that diagnostics carry
  only fixed source-free messages; the framework-registry test asserts the
  exact role set. Product-level `init`/`index`/`families` tests over every
  PHP fixture now live in `src/rust/bin/repogrammar.rs`. One qualifier
  belongs on the record: the shared CLI matrix helper
  `assert_scanner_lane_readiness_is_source_free` (`status`, `doctor`,
  `stats`, `unknowns`, `families`, `files`, `inspect_readiness`) is still
  called only for the delphi, ada, matlab, visual-basic, and r lanes. PHP's
  `stats` and `unknowns` output was re-inspected empirically on 2026-09-05
  and exposed only bounded tokens, counts, role names, and reason codes;
  that check is not yet pinned by a committed assertion.
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

## 2026-09-05 evidence and blocker (historical)

Exactly one admission class is decoded: `.php` files beginning with the
`<?php` prologue. The parse is the bounded in-process one ADR-0047 D4
declares; it loads no grammar and no external artifact, and its refusals are
whole-file. The Composer inventory of ADR-0024 D4 is unchanged and remains
the only PHP configuration evidence.

No integration blocker remains. `application/indexing.rs` admits `.php`
source through `file_is_inventory_only`, `application/family.rs` carries the
PHP support-compatibility and derived-support arms, the framework role is
registered in the shared detector and in `application/query_terms.rs` (as the
`phpunit` alias, known token, and test concept, so the family is reachable by
framework-name and concept query), and the family forms in product output at
the three-member bar. Each of the six anchors in the exact-test corpus carries
its two standing residual `UNKNOWN`s, twelve in total, exactly as the registry
declares.

What is open is gate 9's commit shape, and one known limitation:
`readiness_language_scopes()` in `application/query.rs` — the list deciding
which languages the public `stats` surface reports with a
`language_scope`/`preview_status` — still contains only python,
typescript/javascript, rust, java, csharp, and c/cpp. It was deliberately not
extended, because that list is a product support-scope claim requiring its own
decision rather than internal counting. PHP's repo-shape counts therefore
exist in storage and are not surfaced through the public per-language stats
rows.

Three defects found this session applied to this lane and are recorded rather
than left implied. `min_family_support` fell through to the shared default of
two, so a pair of anchors would have formed a family this review says must not
form. The shared `is_class_like`/`is_method_like` tables omitted both PHP
kinds, so no class-to-method IR containment edge was produced; a
containment-pair guard test now covers every class-bearing lane. And the
sqlite repo-shape stats allowlists (`REPO_SHAPE_LANGUAGE_SCOPES` and all four
`repo_shape_*_where` predicates) omitted PHP, so the language silently counted
zero; the pre-existing four-predicate invariant test now actually covers it.

## 2026-09-05 completion verdict (historical)

Not complete. PHP has a bounded recursive-descent parser over one admission
class and one declaration anchor, owned units and IR, a complete claim-scoped
obligation registry with provider-fallback policy, a framework adapter with
the `php.phpunit.test_method` family token, and committed positive,
lookalike, low-support, and parse-degraded fixtures with parser-level
source-free readiness tests, indexing admission, family-gate wiring, and a
family that forms in product output at the three-member bar. It has no
provider and no final audit. Strict gate count is `8/9` with gate 9 open on
the commit-shape ruling above; PHP is `structural_substrate` and must not be
counted as supported.

## 2026-09-05 program audit fields (historical)

| Required field | Audited result |
|---|---|
| Language / rank | PHP / 14 |
| Dialect/version | Declared invariance set {PHP 7.4, 8.0, 8.1, 8.2, 8.3, 8.4, 8.5}; no interpreter, composer platform constraint, or runtime is selected or claimed. The `#[` attribute divergence is bounded by same-line closure; a spanning attribute abstains under `php_dialect_invariance`. Non-ASCII code bytes are refused; 8.x accretions outside the admitted grammar abstain. |
| Provider/frontend/version | Bounded in-process `repogrammar-php-phpunit-parser` / `bounded_php_phpunit_v1`, a hand-written lexer and recursive-descent parser over the ADR-0047 D4 subset. No Mago, nikic/PHP-Parser, PHP CLI, Tree-sitter PHP, Composer, or PHPUnit execution; the ADR-0024 D2 qualification track remains open and unclaimed. |
| Discovery/config | Exact `.php` source suffix and exact Composer/PHPUnit config basenames; global `vendor` exclusion; PHP-only `.composer`/`.phpunit.cache` exclusions; source decoded only with a leading `<?php` prologue. |
| Manifest/lockfile | ADR-0024 D4 bounded Composer inventory, unchanged: `composer.json` direct declarations, `composer.lock` static entries with unknown directness, typed `php_dependency_inventory` unknowns. |
| Owned source IR / external symbols | Owned units exist for the ADR-0047 anchor only (`php_file` module, `php_test_class`, `php_test_method`); external symbols, cross-file ancestry, trait methods, dynamic includes, generated proxies, and runtime mutation are unresolved or non-claims. |
| Library Contracts | Registry exists, production packs = 0; inventory never creates a behavior contract. |
| Exact family / fixtures | One family target, `framework:phpunit.test_method` over the `phpunit.TestMethod` anchor, family token `php.phpunit.test_method`, family id `family:php:php_test_method:framework_phpunit_test_method`, support bar three pinned explicitly for `php`. Measured: positive (three marker styles, in-file chain) forms the family with support 6; lookalike (five lookalike classes/functions, zero anchors), low-support (two anchors), and parse-degraded (two whole-file abstentions) index complete and form none. Resolved/unresolved pairs do not exist, because there is no PHP provider. |
| Primary UNKNOWN cases | `php_test_parse` (unadmitted construct, unterminated literal, byte/depth/unit bounds), `php_dialect_invariance` (attribute spanning lines), `php_phpunit_testcase_ancestry` (markers without a resolved TestCase-suffixed terminus), `php_phpunit_test_method_shape` (markered method outside the admitted shape), `php_phpunit_data_provider_obligation` (provider attachment), `php_trait_test_origin` (trait import in a test class), and the standing `php_phpunit_runtime_selection` and `php_phpunit_base_name_binding` on every anchor, plus the inventory lane's `php_dependency_inventory` tokens from ADR-0024. The two standing residuals (`php_phpunit_runtime_selection`, `php_phpunit_base_name_binding`) share one fallback, `no_php_provider_irreducible`: no PHP semantic provider slot exists or is registered and the dependency constraint admits none, so the obligation cannot be discharged in practice. The `unknowns` surface classifies the same residuals on its own axis — measured on `phpunit_exact_tests`, `recoverable_unknowns: 13`, `irreducible_unknowns: 0`, `by_recovery_code` = `manual_review_required` 7 (the six `import_resolution_provider` residuals plus the inventory lane's governance residual), `not_implemented_in_current_version` 6 (the `framework_semantic_provider` residuals). |
| Source-free / security | Only prologue-bearing `.php` bytes are read; no source text, identifier, literal, or absolute path reaches the parse surface (asserted over every fixture through the product dispatch); diagnostics are fixed strings; input is bounded by bytes, units, and nesting depth; nothing executes. |
| Completion state / counted | `structural_substrate`; strict gate count `8/9` with gate 9 open on the commit-shape ruling; Top-20 complete = no. |

## 2026-09-05 four-part review (historical)

**Correctness.** Every anchor is established from parsed declaration
structure — class resolution follows written `extends` chains through
in-file declarations to a `TestCase`-suffixed terminus, and method admission
requires the explicit public modifier, non-static, non-abstract,
zero-parameter shape, and one of exactly three source-visible markers.
Declaration text inside strings, comments, heredocs, and attributes cannot
contribute because each is one opaque token. Every refusal is whole-file
with a registered typed kind, and an unregistered unknown is a typed error.
Registry tests pin uniqueness, per-trigger firing, standing-obligation
scoping, determinism, and stable counts. Findings: none open at frontend
level; cross-file ancestry, trait-supplied tests, and provider-driven
methods understate by design and are recorded, not silent.

**Security.** Repository contents are untrusted. Input bytes, emitted unit
counts, and nesting depth are bounded with typed refusals; the prologue gate
minimizes the bytes that cross the source-store boundary. No PHP, Composer,
PHPUnit, Artisan, vendor binary, autoloader, plugin, script, `include`,
`require`, `eval`, reflection, or network resolution runs. Fixture secrets,
identifiers, and the absolute manifest path are asserted absent from the
parsed surface. Notes, claims, kinds, and diagnostics are fixed source-free
strings that pass the stored-assumption content rules.

**Completeness.** Gate 9 is open on the commit-shape ruling, and the public
per-language `stats` rows do not yet report PHP, by the deliberate
`readiness_language_scopes()` decision recorded above. Recall is deliberately
low: modifier-less legacy methods, parameterized and provider-driven
methods, group use, braced namespaces, conditional
declarations, multi-line attributes, interfaces/enums/traits as declarators,
and cross-file ancestry all abstain or understate, each with a typed reason
where claim-relevant. Widening any of these needs a superseding ADR.

**Performance.** The parse is a single lexing pass plus one linear
recursive-descent walk with iterative brace-balanced body skipping — O(file
bytes) with no backtracking, no generated parser, no allocation proportional
to body size, and bounded recursion. No PHP-runtime or large-repository
benchmark exists for this lane; the resource exposure is bounded by the
shared discovery ceilings, the 1 MiB input check, and the 4,096-unit and
256-depth ceilings, each tested exact-then-plus-one at the byte and unit
bounds.

Evidence: `src/rust/adapters/parsing/php/{lexer,syntax,phpunit}.rs` (the
frontend, the `PHP_OBLIGATION_REGISTRY` gate 4 table, and its emission +
registry + fixture + source-free tests), `src/rust/adapters/languages/php.rs`
(unchanged discovery classification), `src/rust/adapters/parsing/php.rs`
(Composer inventory, unchanged), `src/rust/adapters/frameworks/php.rs` and
its registration in `src/rust/adapters/frameworks/mod.rs`,
`src/rust/application/family.rs` and `src/rust/application/indexing.rs` (the
support-compatibility, derived-support, and admission arms),
`src/rust/bin/repogrammar.rs` (the product-CLI family tests over every PHP
fixture), the fixtures under `src/fixtures/php/release/v0_2/`, ADR-0024, and
ADR-0047.

Exact non-claims: the anchor proves a source-visible declaration shape, not
that the base class binds to PHPUnit, that PHPUnit selects, runs, or passes
the test, or that the file is in any suite; the Composer inventory proves
static declarations and lock entries, not installation, autoloading, or
runtime selection; and nothing here proves PHP language support beyond the
declared subset of ADR-0047 D4.

## ADR-0058 frontend change — 2026-10-20

The PHPUnit frontend now reads declarations from the pinned
`tree-sitter-php` 0.25.1 grammar (`LANGUAGE_PHP`, the full-file grammar: the
prologue gate admits only files that open with `<?php`, so the tag is part of
every parsed file, and the grammar tolerates the optional BOM without byte
slicing). The ADR-0047 lexer stays the lexical-subset gate and runs first, so
every lexical refusal (`unterminated_literal`, `attribute_spanning_lines`,
`?>` and non-ASCII code bytes) is unchanged and still wins over any tree
refusal. The anchor shape, obligation registry and family claim are unchanged;
`php.rs` (Composer and PHPUnit XML inventory) is untouched. The open ADR-0047 D1
admission-contract mismatch recorded above is not addressed here and its
behaviour is preserved exactly.

The grammar supplies the declaration structure and the ADR-0047 subset is
applied over the tree and the token stream: top-level `declare`, `namespace`,
`use` (single clause only), `const`, free functions and non-`readonly` classes
with at most one `extends` name; class members `use` (no adaptation block),
`const`, properties (no hook list) and methods; the bounded return-type
vocabulary (an optional `?` and one name, so union, intersection and DNF types
refuse); same-line uppercase-first attribute spelling; and the 256-deep body
brace bound, counted over the token stream. Refusals are collected with their
byte position and the earliest wins, as in a left-to-right parse. An `ERROR`
or `MISSING` node refuses the file only as the generic
`unadmitted_php_construct`, and only when no more specific refusal applies: a
root that is itself an `ERROR` node is not walked as top-level declarations,
and a method with neither body nor `;` is left to the generic class. Two
legacy quirks of the member reader are reproduced because they are
header-position refusals: a block comment inside a declaration header (outside
the parameter list) refuses, and a method whose modifier run is broken by a
block comment or a `readonly`/`var` modifier is read as a property extent
(refused if it has a body, silently dropped if it does not). The doc-comment
and attribute prefix of a member is read off the token stream, so it is
independent of where the grammar attaches comment nodes.

Verdict `CLASSIFIED`. The in-process differential against the legacy parser
(full `ParseReport`/`SourceParseOutput` equality) matched on all 7 committed
fixture sources and on a 195-source branch corpus that covers every refusal
class, every anchor variation (doc, attribute, name, shape, provider,
ancestry, trait) and near-misses (comments in each header position, depth at
255/256/299 closed and 512 unclosed, BOM, CRLF).

The machine holds no real-world PHPUnit corpus: the 21 files containing
`extends TestCase` are copies of the repository's own fixtures in other
checkouts. The external run compared 33 `.php` files (those 21 plus 12 unrelated
files such as gettext and pygments examples) and all 33 were identical, which
is weak evidence by itself. To widen it a mutation differential rewrote every
fixture and branch source (drop a line, drop a token, insert a block comment
before a token, truncate after a token): 23,195 mutants, 822 differing, all
explained by three classes:

- 419 `unknown_added`: the mutant is not valid PHP and the legacy parser
  admitted it because it skipped the broken region (body, parameter list,
  property default, constant value, an unrecognized class-member head, a
  trailing attribute at end of file, a `|` or `&` return type). This is the
  declared `syntax_error_in_skipped_region` divergence; the new behaviour is
  pinned by `a_syntax_error_in_a_skipped_region_refuses_the_whole_file`.
- 401 `refusal_class_changed`: all come from the inputs that nest more than 256
  braces. Legacy refused structurally (`unadmitted_php_construct`) because of a
  mutation earlier in the file; the new reader cannot see a body inside a
  recovered `ERROR` tree and reports `parser_depth_limit` from a token-level
  depth count (a global depth above the bound plus one). Both refuse the whole
  file; only the class differs, and only for a file that is both broken and
  nested past the bound.
- 2 `unknown_removed`: `\\Name` (a doubled backslash) is accepted by the
  grammar and was refused by the legacy name reader.

Three further divergences are pinned by `declared_divergences_are_pinned`
because the legacy parser admitted source that is not PHP or that the grammar
cannot read: a bodiless free function, a stray `;` in a class body, and a
`declare` directive list (`declare(strict_types=1, ticks=1)`), which PHP
admits but grammar 0.25.1 reads as an error. `?>` inside a `//` comment ends
PHP mode in PHP and in the grammar but was comment text to the legacy lexer,
so such a file now refuses. The `declare` list is the only valid-PHP
regression found; it costs recall on a file that uses it, never a false anchor.

Non-test lines of the lane: 1,802 before (`lexer.rs` 421, `syntax.rs` 712,
`phpunit.rs` 669), 1,729 after with the legacy parser excluded (`syntax.rs`
625 of which the reader is 475 against a 564-line parser, `phpunit.rs` 683).
The maintainer accepted the classification on 2026-10-22, and the legacy
parser and its differential tests were deleted that day (the `declare` list and
the other pinned differences now live in the ordinary `phpunit.rs` tests). Gate 2 evidence must be re-checked against ADR-0058 before any
completion promotion; the status above is unchanged.
