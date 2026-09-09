# ADR-0047: Bounded PHP PHPUnit frontend

- Status: Accepted
- Date: 2026-09-04
- Scope: PHP in ADR-0020; admits a family-bearing frontend over one
  declaration anchor
- Refines: ADR-0024 (supersedes its discovery-only source boundary and its
  D6-4 family closure for the bounded slice defined here; the Composer
  inventory scope of ADR-0024 D4 is unchanged)
- Related: ADR-0042 and ADR-0045 (the bounded-frontend precedent this
  follows), ADR-0046 (the same whole-file abstention discipline),
  `docs/reports/language-support/php-completion-review.md`,
  `docs/plans/multi-language-expansion-plan.md`

## Context

PHP source is discovered and inventoried but never decoded: ADR-0024 D1 keeps
`.php` at `discovered_only`, and its stage ledger lists the PHP source
frontend, units/IR, family, and readiness stages as open. The bounded Composer
dependency inventory of ADR-0024 D4 stands and is untouched by this decision.

### The evidence ladder was read first

ADR-0024 D6 forbids exactly one evidence route for a family claim: rung 4,
"regex/text-only matching, extension-only recognition, unpinned parser
output, partial/recovered AST anchors, target execution, runtime test
results, or structural similarity without exact identity." ADR-0041's first
version authorized a Go family without reading ADR-0021's ladder and had to
be withdrawn; ADR-0042 states the check explicitly, and this decision does
the same.

The ladder is discharged the way ADR-0042 through ADR-0046 discharged it for
R, Visual Basic, Delphi, Ada, and MATLAB: a hand-written parser over a
declared subset, with an explicit fidelity boundary and whole-file
abstain-or-admit semantics, replaces text matching as the primary syntax
evidence. The ladder's *primary* rung — fresh output from the qualified
sandboxed frontend of ADR-0024 D2 (Mago, nikic/PHP-Parser, or PHP CLI
references) — remains undelivered: none of those candidates has passed
artifact, conformance, supply-chain, sandbox, or resource qualification, and
the zero-new-dependency constraint of the current expansion program admits
no crate, grammar, toolchain, or downloaded artifact. This ADR therefore does
not claim that rung. It claims the same intermediate rung the five landed
lanes claim: exact anchors derived from a real parse of a declared subset,
which is not the forbidden rung and is the strongest source-only evidence
available without a dependency.

### Why PHPUnit

PHPUnit is the dominant PHP test framework and the one the ecosystem's
`tests/` convention builds around. ADR-0024 D5 already selected
`php.phpunit.test_method` as the first PHP family token, and this decision
keeps that token while replacing D5's heavyweight compatibility closure
(exact `PHPUnit\Framework\TestCase` FQN resolution, coherent
`composer.json`/`composer.lock` content-hash, exact `phpunit/phpunit` 13.2
lock version, PHP 8.5 profile) with a bounded source-visible anchor. The D5
closure is superseded because it bundled the first family with a
project-profile machinery that remains unimplemented and unauthorized; the
ADR-0042..0046 precedent establishes that a declared-subset parse of exact,
source-visible shapes is sufficient family evidence, with everything not
source-visible left a typed `UNKNOWN` or a non-claim.

### Why a parser, and not a scanner

The repository has shipped the text-matching defect four times in other
lanes: a Rust attribute matched inside a function body, a TS/JS runner matched
inside a member call, a Go declaration matched inside a string, and both
branches of a Delphi `{$IFDEF}`. PHP multiplies the hazard: `testFoo` is a
legal name in any class, `class X extends TestCase` is legal inside a heredoc
or a comment, and `#[Test]` is a legal attribute on a property. A scanner
cannot state what it refuses, so its failure mode is a quiet wrong answer.
The frontend is therefore a hand-written lexer and recursive-descent parser
over the declared subset in D4, and the anchor is a question about the parsed
declaration structure.

## Decision

### D1. Admission is `.php` files that begin with the `<?php` prologue

Discovery keeps classifying the exact case-sensitive `.php` suffix as Source;
that is unchanged. The frontend additionally admits only files whose bytes
begin with `<?php` followed by whitespace or end of file (a UTF-8 BOM may
precede it). Every other `.php` file holds no PHP code region — it is inline
HTML output — and never crosses the source-store boundary: the frontend
declines it exactly as the R lane declines a non-testthat path, and it stays
zero-read inventory.

### D2. The exact anchor

One shape is admitted, and every condition is a property of the parse:

1. one named, non-`abstract`, non-anonymous class is declared in the file
   (a `#[…]` attribute on the class is admitted and ignored);
2. the class's `extends` chain, followed through classes declared in the same
   file, terminates at a parent whose last `\`-separated name segment ends
   with `TestCase`. That one rule covers the spellings PHPUnit convention
   uses: the bare imported `TestCase`, `use … as BaseTestCase`, a
   fully-qualified `\PHPUnit\Framework\TestCase`, and a project-local
   `*TestCase` base, because all share the final segment. A chain that
   cycles or leaves the file unresolved proves no terminus, and the class
   does not anchor;
3. the method is declared directly in that class body (trait-supplied
   methods are invisible and can only understate), carries an explicit
   `public` modifier, and is neither `static` nor `abstract` nor
   variadic-shaped: it has zero parameters;
4. exactly one of three markers holds: the method name has the exact
   case-sensitive lowercase `test` prefix, the nearest doc comment carries
   `@test` as a word-bounded token, or the method carries a `#[Test]`
   attribute with zero arguments;
5. the method carries no data-provider attachment: no `@dataProvider`
   docblock token and no `#[DataProvider…]` or `#[TestWith…]` attribute.

The class name need not end in `Test`, and the file name need not end in
`Test.php`; those are suite-selection concerns. The suffix rule of condition 2
is naming-shape evidence, not proof of PHPUnit lineage: the standing
`php_phpunit_base_name_binding` obligation records that on every anchor, and
a `MyTestCase` base that is not PHPUnit's is admitted as a test class only
under the same under-proven lineage claim every other spelling carries.

The anchored unit set is one module unit per admitted file, one
`php_test_class` unit per resolved test class, and one `php_test_method` unit
per admitted method. Only the method carries a framework role; the class is
context.

### D3. Whole-file abstain-or-admit; never recover

The frontend has exactly two outcomes. Either the lexer produces a token
stream for the whole file and the parser consumes it completely, in which
case the admitted anchors are reported; or the file is refused, in which case
it yields its module unit, one typed `UNKNOWN` naming the refusal class, an
Error diagnostic with a fixed source-free message, and **no anchor at all**.
There is no error recovery, no resynchronization, and no partial tree: an
anchor collected before the refusal point would rest on boundaries the
refusal unproves, so it is dropped with them. This matches the bar ADR-0024
D6 set when it forbade partial or recovered AST anchors.

### D4. The declared subset

Lexically the lexer admits: the `<?php` prologue (admission gate, D1);
`//` and `#` line comments and `/* */` block comments, with `/** … */`
distinguished for the doc-comment marker; single- and double-quoted strings
whose runs may span lines, with backslash escapes consumed so a quote cannot
be hidden; heredocs and nowdocs under the PHP 7.3+ flexible-indentation
closing rule, consumed opaquely so their bodies can hold anything; `#[ … ]`
attributes that close on their opening line, consumed as one token with the
bracket-, parenthesis-, and quote-aware scan of D4a; decimal, hexadecimal,
binary, and exponent numeric literals, opaque; ASCII identifiers and `$`
variables; the full operator set under longest match, including `?->`, `??`,
`<=>`, and `**`; and the punctuation `( ) { } [ ] , ; : :: -> \`.
A `?>` close tag anywhere in code is outside the subset: it reopens
inline-HTML mode, and the file abstains. A non-ASCII byte in code position
is outside the subset; inside literals and comments it is never classified
and is harmless.

Grammatically the parser admits, after the prologue: `declare( … ) ;`
directives; `namespace Name ;` (the braced form abstains); plain and
function/const `use` imports with optional `as` alias (group
`use Foo\{…}` abstains); `abstract`/`final`/plain `class` declarations with
optional `extends` and `implements` lists; class members in the forms of
trait `use` lists, constants, typed and untyped properties, and function
declarations with an optional bounded return type (an optional `?` and one
qualified name; union, intersection, and DNF types abstain); and free
functions at top level, parsed for extent and never anchored. Method bodies
are skipped by exact brace balancing over the token stream — exact because
every brace-bearing literal was already consumed as one token — under a
nesting ceiling of 256.

Method bodies, defaults, and parameter lists are deliberately opaque.
Nothing inside a body is read, matched, or claimed, which is what makes the
bounded subset small enough to audit.

### D4a. The invariance argument

ADR-0020 gate 2 requires that the admitted parse not depend on anything
RepoGrammar cannot determine. For PHP the undeterminable axis is the language
version a project targets: it is selected by `composer.json` constraints,
installed runtimes, and deployment, and this frontend reads none of those.
The declared invariance set is **{PHP 7.4, 8.0, 8.1, 8.2, 8.3, 8.4, 8.5}**,
and the claim takes ADR-0045's conditional form: *for every file this grammar
admits, the token stream and the declaration structure are the same under
every member of the set under which that file is legal.*

**The lexical layer is stable across the set for every admitted form.**
Quoted strings, both comment forms, heredoc/nowdoc (uniform since the 7.3
closing-rule change, before the set's floor), numeric literals, identifiers,
and the operator set tokenize identically from 7.4 through 8.5.

**One divergence exists inside the subset surface, and it is bounded.**
PHP 8.0 introduced attributes: `#[` opens an attribute under 8.x and is a
comment to end of line under 7.4. A same-line-closed `#[ … ]` keeps every
token boundary after that line identical between the readings — the 7.4
comment ends at the newline, the 8.x attribute closed before it — so the
admitted grammar's structure is unaffected. Two consequences are accepted and
named rather than hidden. First, an attribute that runs past its opening line
makes the readings disagree about where the next token starts, so it is
refused whole-file under `php_dialect_invariance` with kind
`attribute_spanning_lines`. Second, the *marker existence* for a `#[Test]`
method presupposes the 8.x reading; under 7.4 those bytes are a comment and
the method has no attribute. The divergence direction is safe and stated:
prefix and `@test` anchors are invariant under every member, and the 7.4
reading of a `#[Test]`-marked method anchors nothing rather than something
different. A file that uses attribute syntax asserts the 8.x reading by that
very spelling, and no 7.4 deployment both parses those bytes as an attribute
and runs a PHPUnit that supports attributes — PHPUnit versions with
attribute support require PHP 8. The other 8.x additions that could reach a
header (union types, `enum`, `match`, `readonly`, `never`, nullsafe
operators) are pure accretion — invalid, not differently-valid, under older
members — and are outside the admitted grammar besides, so they abstain
rather than diverge.

**Constructs selected by anything outside the file are refused, not read.**
The `?>` open/close mode boundary, group use, braced namespaces, conditional
declarations (`if (…) { class … }`), interfaces, traits, and enums as
declarations, lowercase-first attribute names, qualified attribute names,
union/intersection types, and non-ASCII code bytes each abstain whole-file
under `php_test_parse` or the lexical refusals. Each refusal is decidable
from the file's own bytes alone.

### D5. Typed uncertainty and the obligation registry

Every typed `UNKNOWN` the frontend can emit is declared exactly once in
`PHP_OBLIGATION_REGISTRY` in `src/rust/adapters/parsing/php/phpunit.rs`, and
every emission path routes through it, so an unregistered unknown is a typed
error rather than a possible output. No entry blocks the family claim at the
repository level: whole-file abstentions drop only their own file's anchors,
and the shape, ancestry, provider, and trait obligations exclude the affected
declaration rather than unproving the anchors beside it. Two residuals ride
on every admitted anchor because the bounded parse can never discharge them:
PHPUnit's runtime selection/execution of a declared test, and the
TestCase-suffix naming-shape evidence. Each entry records its reason code,
claim scope, blocking impact, and provider-fallback policy; the
provider-fallback vocabulary states that no PHP semantic provider slot
exists or is registered and that ADR-0024 D3 forbids executing PHP, so the
residuals are irreducible under current constraints.

### D6. No dependency and no execution

No Rust crate, no grammar, no generated parser, no PHP runtime, no Composer
or PHPUnit package, no downloaded artifact. The lexer and parser are
hand-written Rust in this repository. PHP, Composer, PHPUnit, Artisan, vendor
binaries, autoloaders, plugins, scripts, tests, generated proxies, `include`,
`require`, `eval`, reflection, and network resolution are never invoked;
ADR-0024 D3's process-isolation contract is unreached because nothing runs.
The ADR-0024 D2 qualification track for Mago, nikic/PHP-Parser, and
Tree-sitter PHP remains open and is not preempted: a later qualified
frontend supersedes this one only through a new decision.

Input is untrusted. The existing input-byte ceiling, a 4,096-unit ceiling,
and the 256-deep nesting ceiling all abstain with a typed `UNKNOWN` rather
than degrading silently or failing unsafely.

## Alternatives considered

- Keep ADR-0024 D5's version-profile closure for the first family: rejected
  because it bundles the family with lockfile content-hash machinery that is
  neither implemented nor near implementation, and the ADR-0042..0046
  precedent shows a declared-subset parse of exact source-visible anchors is
  the right first slice. The closure's *identity strictness* survives in the
  registry as recorded obligations rather than as a hard gate.
- Admit Mago or nikic/PHP-Parser as a Rust or vendored dependency now:
  rejected under the program's zero-external-dependency constraint, and both
  still fail ADR-0024 D2's qualification gates (supply chain, conformance,
  sandbox, five-target proof).
- A byte-oriented scanner with comment/string masking: rejected because a
  scanner cannot state what it refuses; its failure mode is a quiet wrong
  answer, and PHP's lookalike surface (names in any class, declarations
  inside heredocs, attributes on properties) is exactly where it produces
  one.
- Admit every `.php` file regardless of prologue: rejected because a file
  without `<?php` is HTML output with no PHP code; reading it asserts
  authority over bytes there is no reason to parse, and inventing a
  convention to admit it would be worse.
- Recover past an unparsable declaration and keep later anchors: rejected
  because a skip needs the declaration's extent, and computing an extent
  without parsing is the scanner assumption this decision replaces.
- Require a coherent `composer.lock` with an exact PHPUnit version before any
  anchor forms: rejected for this slice as the D5-closure restated; the
  anchor is source-visible, and the version/provider profile it cannot see is
  a typed standing obligation instead of a hidden gate.
- Admit group use, interfaces, traits with method extraction, and lowercase
  attribute names now: rejected as recall widening; each needs its own extent
  or identity argument and is named as follow-up work.

## Consequences

- PHP gains a bounded frontend, owned units, typed `UNKNOWN`s, and one exact
  family target (`php.phpunit.test_method` over `phpunit.TestMethod`) for one
  declaration shape in one admission class. It gains no project model and no
  provider, and remains short of the ADR-0024 D2 primary-evidence rung.
- `.php` bytes cross the source-store boundary only when the file begins with
  the `<?php` prologue; every other `.php` byte stays zero-read inventory.
  ADR-0024 D4's Composer inventory scope is unchanged.
- Recall is deliberately low and the failure direction is safe: legacy
  modifier-less `function testFoo()`, parameterized methods, provider-driven
  methods, group-use files, conditional declarations, multi-line attributes,
  and cross-file ancestry all understate, each with a typed `UNKNOWN` saying
  why where the miss is claim-relevant.
- The family-support wiring in `application/family.rs` and the indexing
  admission in `application/indexing.rs` are shared-surface integrations
  owned outside this decision's file scope; this ADR authorizes the frontend,
  its registry, its framework adapter, and its fixtures, and the completion
  review records the integration state honestly.
- Widening the admitted subset, the admission class, the anchor shape, or the
  invariance set requires a superseding ADR. In particular, admitting an
  attribute form that can span lines, or any construct whose parse depends on
  a PHP version profile, breaks D4a's invariance and may not be admitted
  without one.

## Follow-up work

- Wiring `php.phpunit.test_method` into the shared family-support gate and
  the indexing source-admission path, then the product readiness surfaces.
- Trait method extraction, group use, and cross-file ancestry resolution each
  need their own extent/identity argument before admission.
- The ADR-0024 D2 frontend qualification track remains the route to the
  ladder's primary rung and to discharging the standing residuals.
