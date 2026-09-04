# ADR-0049: Bounded Ruby Minitest frontend

- Status: Accepted
- Date: 2026-09-04
- Scope: Ruby in ADR-0020; admits a family-bearing frontend over one class and
  method shape
- Refines: ADR-0022 (supersedes its D5-4 family closure for the first
  `ruby.minitest.test_method` family, exactly as ADR-0042 superseded the
  equivalent closures ADR-0036..0041 had recorded; nothing else in ADR-0022 is
  weakened)
- Related: ADR-0042 (the owned-frontend precedent and its evidence-ladder
  reading), ADR-0044, ADR-0045, ADR-0046,
  `docs/reports/language-support/ruby-completion-review.md`,
  `docs/plans/top-20-language-expansion-plan.md`

## Context

Ruby source is discovered and never decoded. ADR-0022 D1 keeps Ruby
`discovered_only`; its D9 stage plan made a sandboxed native Prism worker the
only route to a family, and its D5-4 evidence ladder accordingly closed the
family until that worker existed.

ADR-0042 already read that kind of ladder once. It separated two questions the
Prism preflights had merged: *executing* Ruby is forbidden, and an external
parser dependency requires the D7 qualification — but neither prohibition names
a bounded parser inside the existing Rust core, because such a parser executes
nothing and adds no dependency. ADR-0042 closed the R family that way, and
ADR-0043, ADR-0044, ADR-0045, and ADR-0046 followed for VB.NET, Delphi, Ada,
and MATLAB. ADR-0042 also said why those languages had looked harder than they
are: "Ruby has heredocs and a lexer that depends on parser state." That is the
real obstacle, and this ADR answers it the way ADR-0046 answered MATLAB's
command syntax — not by hoping the ambiguity stays away from the anchor, but by
deciding it with a declared, total rule and abstaining wherever the rule cannot
decide.

### Why the lexer must be real

ADR-0022 D5-4 forbids "regex/text matching for the claim" and D2 keeps Ruby
source unread. A byte scanner anchored on `class ... < Minitest::Test` text
would fail the first heredoc, `%w[]` list, or regex literal that happens to
contain the words, and Ruby test files contain all three routinely. The
frontend is therefore a real lexer and a bounded structure parser: heredocs
(`<<~ID`, `<<-ID`, `<<ID`, and the quoted spellings) are consumed by exact
terminator tracking, `%`-literals by delimiter matching with nesting,
interpolations by a nesting scanner, and `def`/`class`/`end` extents by an
end-matching region parser. Where Ruby's own lexer cannot decide without
runtime state, this one abstains for the whole file rather than guessing.

## Decision

### D1. Admission is runner-scoped test paths only

Discovery keeps every `.rb` path as inventory. Only files whose normalized
repository-relative path has a `test/` or `tests/` component and whose basename
matches `test_*.rb` or `*_test.rb` are decoded. Every other Ruby byte remains
unread.

The path is identity evidence, not convenience: those are the shapes Minitest
conventions run — Rake::TestTask's default `test/**/test_*.rb` glob and the
Rails `test/**/*_test.rb` convention. A `Minitest::Test` subclass elsewhere in
a repository is not a test being run, and this frontend does not read it.

### D2. The exact anchor

Two shapes are admitted, and every condition is a property of the parsed token
stream, not of byte positions:

1. A **top-level class declaration** — a `class` header that is a direct
   statement of the program body, on a single logical line, naming a constant
   path — whose superclass resolves to Minitest by the bounded resolution of
   D3. `class << self` bodies, `module`-wrapped classes, and classes nested
   inside other bodies parse structurally but never anchor.
2. An **instance method declaration directly in that class body**: `def name`
   or `def name()` where the name begins `test_` with a non-empty suffix.
   `def self.test_x` is a class method and is not an anchor. `def test_x(args)`
   has parameters and is not an anchor. A `def` reached only inside a nested
   `def` body, block, brace block, or conditional construct is not a direct
   class-body declaration and is not an anchor. `private def test_x` does not
   begin with `def` and is not an anchor.

Visibility is the source-visible default: only a `def` that is itself a
direct statement of the class body is admitted, which is the public-by-default
position. Minitest spec DSL (`test "description" do`), Rails-specific DSLs,
`define_method`, aliases, and inherited test methods are out of scope; they are
named non-claims, not silent omissions.

### D3. The bounded superclass resolution

A class qualifies when its superclass chain, resolved within the file, ends at
`Minitest::Test` or `ActiveSupport::TestCase`:

- a superclass written as a dotted path qualifies only when it is exactly one
  of those two spellings;
- a single-segment superclass resolves through the map of top-level class
  headers declared in the same file, following at most eight further hops and
  refusing cycles;
- a chain that is absent, unresolvable, or cyclic does not qualify. When a
  non-qualifying class on an admitted path declares direct `test_*` instance
  methods, the file records a typed `UNKNOWN` under `ruby_minitest_superclass`
  (`test_methods_without_minitest_base`) rather than anchoring — the same shape
  ADR-0046 uses for a `Test` methods block in a class that is not a
  `matlab.unittest.TestCase`.

### D3a. The require line is context, not precondition

The R lane required the repository's `DESCRIPTION` to declare testthat before
any anchor formed, because an R test file never names its framework. The honest
Ruby analogue is the superclass itself: `class CatalogTest < Minitest::Test` is
an in-file declaration of the framework, made in the very construct that
carries the anchor, and requiring a project manifest as well would add no
identity the superclass does not already state. The explicit top-level
`require` line is therefore recorded, not gated: each admitted class anchor
carries one bounded `minitest_require=` assumption token — `autorun`,
`test_unit`, `literal_other`, `non_literal`, or `absent` — derived from the
literal arguments of top-level `require`/`require_relative` statements in the
same file. The literal text of a non-matching require argument is never
retained. A file with no require line at all still anchors; a Rails-engined
test that requires `test_helper` and derives `ActiveSupport::TestCase` anchors
with `literal_other`.

### D4. The declared lexer subset

The lexer tokenizes, exactly: `#` comments and `=begin`/`=end` block comments
(delimiter lines at column 0); single-quoted strings (only `\\` and `\'`
escape); double-quoted strings with any-backslash escapes and `#{...}`
interpolation nested through strings, braces, and comments; backtick command
strings; heredocs in every spelling — `<<ID`, `<<-ID`, `<<~ID`, `<<'ID'`,
`<<"ID"`, `` <<`ID` `` — with the body tracked to the exact terminator line
(indented terminator allowed for `-`/`~`, not for plain `<<`), multiple pending
heredocs consumed in order, and, for interpolating bodies, interpolation-aware
terminator tracking plus the backslash line-continuation rule; `%`-literals
`%w %W %i %I %q %Q %r %s %x` and bare `%(...)`, with paired-delimiter nesting
for `() [] {} <>`; regular expression literals with interpolation and trailing
flag letters; symbols in every spelling — `:name`, `:name?`, `:name!`,
operator symbols, `:"name"`, `:'name'`, `:@ivar`, `:$gvar`; hash labels
(`name:`); character literals (`?x`); numeric constants including hexadecimal,
binary, underscore separators, exponents, and `r`/`i` suffixes; ASCII
identifiers and constants; `__END__`; backslash-newline continuation; and
Ruby's operator set.

Two disambiguations are decided by a total rule over facts the lexer already
has — the previous token's class, whether whitespace preceded, and whether an
identifier is the first token of its statement — because those are the inputs
MRI's own state machine uses in the positions that matter:

- **Slash.** `/` after a closing bracket, a literal, a variable sigil, a
  constant, or a value keyword is division. `/` after an operator, an opening
  bracket, a separator, a newline, an expression keyword, or an identifier in
  statement-first (command) position is a regular expression. `/` after an
  identifier in any other position **abstains the whole file** with the typed
  `ruby_slash_disambiguation` `UNKNOWN`: MRI's reading there depends on the
  runtime distinction between a local variable and a command call, and either
  reading can move the token stream that `end` matching depends on
  (`expected / 2` versus `puts /end/`).
- **Double less-than.** `<<=` is assignment and `<<` followed by anything but a
  directly attached heredoc starter is left shift. `<<[-~]?ID-or-quote`
  directly attached is a heredoc in beginning-of-expression position, after an
  operator, bracket, or separator, or after an identifier in statement-first
  position; it is left shift after a value-ending token with no whitespace. In
  the one remaining corner — identifier not in statement-first position,
  whitespace before, identifier directly attached — the file abstains with
  `ruby_heredoc_disambiguation`, for the same reason as the slash rule.

Everything else outside the subset — a non-ASCII byte or stray control byte in
code position (they are content, not code, inside literals and comments), a
backslash in code, an unknown `%` designator, an endless method definition
(`def x = expr`, whose body has no `end`), an operator method definition, a
multi-line `class` header, heredocs or `%`-literals or slashes inside an
interpolation, and an unterminated string, regex, `%`-literal, or heredoc —
makes the file abstain whole-file with a typed `ruby`-named `UNKNOWN`
(`unadmitted_ruby_construct`, `unterminated_string`,
`unterminated_regexp`, `unterminated_percent_literal`, `unterminated_heredoc`).
A structural failure of the end-matching grammar — blocks that do not close or
closers without openers — reports a degraded parse and the
`ruby_structural_failure` `UNKNOWN`; class and method declarations completed
before the failure point are whole, matched constructs and are kept, exactly as
ADR-0042 D4 and ADR-0046 keep the constructs proven before their boundaries.
The parser never resynchronizes past either kind of boundary, so no anchor
after one can be invented.

Statements are otherwise opaque. The parser does not build expression trees,
and it does not need to: the only questions it asks are which tokens are
statement boundaries, which keywords open `end`-regions, and whether a
construct that text can fake — `def`, `class`, `end`, a newline — sits inside a
literal. Those are exactly the questions the lexer answers exactly.

### D4a. The invariance argument

ADR-0020 gate 2 requires that the admitted parse not depend on anything
RepoGrammar cannot determine. The invariance set is CRuby 3.x — the
maintenance window a repository's `.ruby-version` realistically pins today —
and the argument has the same shape as ADR-0042's:

**Everything the subset admits is older than the window and stable inside
it.** Heredocs including `<<~` (2.3), all `%`-literal spellings, symbol and
label spellings, `__END__`, `=begin`, character literals, and the operator set
predate Ruby 2.5 and have not changed lexing since. The constructs where Ruby
3.x *added* readings — endless definitions (3.0), rightward assignment (3.0),
hash shorthand (3.1), `it` block parameters (3.4) — either never reach the
structural grammar (rightward assignment and shorthand are opaque statement
soup; `it` is an identifier) or are refused outright (endless definitions
abstain). No construct in the subset changes meaning, token boundaries, or
`end` association between any two Ruby 3.x releases.

**The two runtime-dependent readings are refused, not guessed.** MRI's own
lexer resolves slash and heredoc corners from the local-variable table and the
command-argument stack; this frontend's rule covers the positions those states
make decidable from source alone and abstains on the one corner they do not.
That is the whole exposure, and it is closed by abstention — a wrong guess
would be worse.

JRuby and TruffleRuby are not claimed: they aim at CRuby syntax, and a file
inside the admitted subset lexes identically under them, but no engine claim is
made or needed. Prism stays excluded: ADR-0022 D3/D7 are untouched, no
dependency is admitted, and the evidence ladder's primary evidence is the
bounded parse itself, which is the standing ADR-0042..0046 precedent for an
owned frontend over an invariant subset. Nothing executes: ADR-0022 D8's
prohibitions all name running Ruby or its tooling, and nothing here runs.

### D5. What the anchor claims, and what it explicitly does not

It claims that a file in the runner-scoped path class declares, at the top
level of the program body, a class whose in-file superclass chain terminates at
`Minitest::Test` or `ActiveSupport::TestCase`, and directly in that class body
an instance method named `test_*` with no parameters.

It does not claim that the tests run or pass, that `Minitest::Test` resolves to
the gem's constant rather than a rebound one, that inherited or dynamically
defined test methods exist, that spec-DSL tests exist, anything about ordering,
hooks, assertions, or fixtures, or anything about which engine executes the
file. Rebinding from outside the file — `Minitest` reopened by a helper,
load-path mutation, metaprogramming — is not statically determinable by this
frontend and is a recorded non-claim, carried by the anchor's standing
`provider_resolved=false` assumption rather than by a guess.

### D6. No dependency, no execution, bounded input

No crate, no grammar, no generated parser, no Ruby installation, no downloaded
artifact. Input is untrusted: the frontend keeps the shared source byte limit,
bounds emitted units, and bounds region nesting depth, each abstaining with a
typed `UNKNOWN` rather than degrading silently.

## Alternatives considered

- **Wait for the sandboxed Prism worker:** rejected for this slice because the
  ADR-0042..0046 precedent established that the program's zero-dependency
  constraint admits an owned frontend over a declared invariant subset, and
  Ruby's hostile constructs are exactly as decidable as MATLAB's command syntax
  once the refusal rule is declared. Prism remains the route to a *full* Ruby
  frontend; a successor ADR can still take it.
- **A comment- and string-aware byte scanner:** rejected for the reason
  ADR-0042 rejected the R scanner, plus Ruby's own additions — heredoc bodies
  begin on the line *after* their marker, `%w[]` lists nest their delimiters,
  and regex literals can contain `end`. Text cannot answer where these
  constructs end; only tokenization can.
- **Guess slash and heredoc corners by lookahead heuristics:** rejected
  because a wrong guess moves `def`/`class`/`end` boundaries and can invent or
  destroy anchors. Abstaining the whole file understates support; guessing can
  lie.
- **Gate the anchor on `require "minitest/autorun"`:** rejected because the
  superclass is a stronger, in-file declaration, and Rails-shaped files
  legitimately require `test_helper` instead. The require line is recorded as
  bounded context instead (D3a).
- **Admit `test "description" do` spec DSL:** rejected for the first slice; it
  is a different anchor shape (a method call, not a declaration) with its own
  binding hazards, and widening needs a superseding decision.
- **Admit module-wrapped and nested classes:** rejected to keep the superclass
  resolution bounded to top-level headers; nested and singleton classes still
  parse structurally so their `end`s match.

## Consequences

- Ruby gains a bounded frontend, owned units, typed `UNKNOWN`s, and one exact
  family for one class-and-method shape in one path class. It gains no project
  model, no provider, and no dependency.
- The security posture changes in one way: runner-scoped `.rb` bytes cross the
  source-store boundary. No execution, network, or installation is added.
  Recursion over untrusted input is depth-bounded and unit counts are bounded.
- ADR-0022 remains in force for discovery, the `Gemfile.lock` inventory
  exception, every execution prohibition, and the Prism qualification path;
  only its D5-4 family closure is superseded, by the same precedent that
  superseded the R, Delphi, Ada, and MATLAB closures.
- Support counts fall for files using shapes the rule refuses — a division
  after a mid-statement identifier abstains the whole file — and the typed
  `UNKNOWN`s say why. Widening the admitted path set, anchor shape, lexer
  subset, or claim surface requires a superseding ADR.
