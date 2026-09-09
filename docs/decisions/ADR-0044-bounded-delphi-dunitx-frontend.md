# ADR-0044: Bounded Delphi DUnitX frontend

- Status: Accepted
- Date: 2026-08-15
- Scope: Delphi/Object Pascal in ADR-0020; admits a family-bearing frontend over
  one attribute shape
- Refines: ADR-0032 (authorizes the source unit it declined to authorize)
- Related: ADR-0043, ADR-0042, ADR-0041, ADR-0019,
  `docs/reports/language-support/delphi-object-pascal-completion-review.md`,
  `docs/plans/multi-language-expansion-plan.md`

## Context

Object Pascal source is discovered and never decoded. ADR-0032 D2 routes `.pas`,
`.dpr`, and `.dpk` around the source store, and D4 records that no source
frontend, framework role, or family exists.

### The evidence ladder was read first

ADR-0032 D4 carries one. Its forbidden item names "`.pas` as a dialect oracle,
`.lpi`/`.lpk` interpreted as `.dproj`, package presence as library behavior, or
build/runtime output". **It does not forbid text matching for the claim**, which
is the check ADR-0041's first version skipped and had to be corrected for.

The dialect clause is the one that shapes this design, and it does so
favourably. ADR-0032 D4 insists Delphi and Free Pascal are never equated, and a
`.pas` suffix decides neither. This anchor never consults the suffix for
dialect: it requires the unit to name `DUnitX.TestFramework` in a `uses` clause,
and DUnitX is a Delphi framework. The dialect evidence is the import, exactly as
the forbidden item demands.

### Why the scanner route is sound for Object Pascal

Object Pascal separates its comment and string delimiters completely. Comments
are `//` to end of line, `{ … }`, and `(* … *)`; strings are single-quoted and
escape by doubling. No delimiter serves two purposes, so the ambiguity that
rules Ruby out and makes MATLAB doubtful is absent. Compiler directives are a
comment form (`{$…}`) and are treated as such.

## Decision

### D1. Admission is `.pas` units

`.dpr` programs and `.dpk` packages remain unread. DUnitX fixtures are declared
in units, and ADR-0032's specific instruction that the `.dpk` `requires` clause
stays unread is untouched.

### D2. The exact anchor

One shape is admitted:

```pascal
uses DUnitX.TestFramework;

type
  [TestFixture]
  TCatalogTests = class
  public
    [Test]
    procedure LoadsCatalog;
  end;
```

Every condition is required. The unit names `DUnitX.TestFramework` in a `uses`
clause. A `[TestFixture]` attribute stands on its own line immediately before a
class declaration. A `[Test]` attribute stands on its own line immediately
before a `procedure` declaration inside that fixture's declaration block.

Attribution is positional and bounded. A `[Test]` belongs to the most recent
class declaration, and three things clear the fixture state: a class declaration
without `[TestFixture]`, the `implementation` keyword, and an `end` closing the
declaration block. So neither a `[Test]` in a plain class that follows a fixture
nor a unit-level procedure after the fixture's `end` anchors — DUnitX discovers
methods of a fixture class, not free procedures.

The `end` rule is deliberately blunt: a nested type declared inside the fixture
closes with its own `end`, which ends the block early and misses any later
`[Test]`. That trade is chosen rather than tolerated. A missed test is a smaller
error than an invented one, and this repository's `UNKNOWN` policy already
prefers absent evidence to asserted evidence.

Object Pascal is case-insensitive, so keyword and attribute matching is
case-insensitive.

`function`-declared tests, `[TestCase]` parameterised rows, `[Setup]`/
`[TearDown]`, `[Ignore]`, inherited fixtures, and DUnit (the older framework)
are out of scope. They are named follow-ups.

### D3. Only admitted declarations become units

The frontend emits a module unit per decoded `.pas` file, one unit for an
admitted fixture class, and one unit per admitted test procedure. Ordinary
Pascal declarations produce no unit, so the absence of a unit is not evidence
that a file has no code — the same bound ADR-0043 sets for VB.NET.

### D4. The scanner is comment- and string-aware

`//`, `{ … }`, `(* … *)`, and single-quoted strings never contribute to an
anchor, and block comments carry across lines. This is a decision because the
repository has shipped the opposite defect three times.

The scanner also reports a degraded parse when its own well-formedness
invariant is violated. A scanner has no syntax failure, but an unclosed `{` or
`(*` at end of file is decidable, and without that signal a malformed file
silently yields fewer anchors -- which is indistinguishable from a file that
simply has fewer declarations. The units already found are kept; the diagnostic
states that the ones not found prove nothing.

### D4a. Conditional compilation and dialect directives bound the claim

Two compiler directives decide what may be claimed, and both are read even
though the text they sit in is not code.

`{$IFDEF}`, `{$IFNDEF}`, `{$IF}`, and `{$IFOPT}` select one branch at compile
time from a define this frontend does not evaluate. **No declaration inside a
conditional region anchors**, in either branch, and the file records
`BuildVariantAmbiguity` under `delphi_conditional_compilation`. Admitting both
branches would invent a member that never compiles; admitting one would assert a
define we do not know. Declarations in unconditional code are unaffected — the
skipped branch understates support, it does not unprove what is proven.

`{$MODE}` and `{$MODESWITCH}` re-select the language dialect, which is precisely
what this frontend does not evaluate, so their presence reports a degraded
parse.

### D2c. Why this is authoritative for the declared scope

ADR-0020 gate 2 asks for a frontend that produces the primary syntax evidence.
This one is RepoGrammar-owned rather than language-native, so it earns that
standing the way ADR-0040 earned it for SQL: by showing the admitted parse does
not depend on what cannot be determined, and abstaining wherever it would.

What cannot be determined here is the dialect and the build variant. The
invariance set is Object Pascal as accepted by Delphi and by Free Pascal in
Delphi mode. Every construct in D2b's subset — `unit`, `uses`, `type`, `class`
with a parent list, visibility sections, field, property and method
declarations, attribute brackets — is lexed and parsed identically by both, and
none of them changes shape with a compiler version. The two things that *would*
move the parse are handled rather than assumed: `{$MODE}`/`{$MODESWITCH}`
re-selects the dialect and reports a degraded parse, and conditional compilation
selects a branch from an unevaluated define and anchors neither.

The claim is therefore bounded twice over: to the subset the parser admits, and
to the dialect set over which that subset is invariant. Widening either needs a
superseding decision.

### D5. What the anchor claims

It claims that a unit importing DUnitX declares a fixture class and a procedure
carrying the attributes DUnitX uses to discover a test. It claims nothing about
execution, outcomes, ordering, setup, parameterised rows, or inheritance, and it
never equates Delphi with Free Pascal. No Delphi, `dcc32`, `dcc64`, RAD Studio,
MSBuild, Free Pascal, `fpc`, Lazarus, package, project, child process, or
network operation runs; ADR-0032's execution prohibitions are carried forward.

### D6. No dependency

No Rust crate, no grammar, no toolchain, no downloaded artifact.

## Alternatives considered

- Anchor on the `.pas` suffix plus an attribute name: rejected because ADR-0032
  forbids the suffix as a dialect oracle, and `[Test]` alone names no framework.
- Admit `.dpr`/`.dpk`: rejected under D1 and because ADR-0032 explicitly keeps
  the `.dpk` `requires` clause unread.
- Track the fixture's class body by keyword depth: rejected because `class`
  appears in `class procedure`, `class var`, and forward declarations, so a
  depth model would be wrong in ordinary code. Positional attribution with
  explicit clearing rules is smaller and states its own boundary, at the cost of
  the nested-type false negative recorded in D2.
- Support DUnit as well as DUnitX: rejected for the first slice because DUnit
  fixtures derive from `TTestCase` without attributes, which is a different
  anchor shape and a separate decision.

## Consequences

- Delphi/Object Pascal gains a frontend, owned units, typed `UNKNOWN`s, and one
  exact family for one attribute shape. It gains no project model and no
  provider, and Free Pascal remains untouched.
- `.pas` bytes cross the source-store boundary; `.dpr` and `.dpk` do not.
- ADR-0032 remains in force for discovery, `.dproj` inventory, the Delphi/Free
  Pascal separation, limits, and every execution prohibition.
- Widening the admitted attribute set, the admitted suffix set, or the claim
  surface requires a superseding ADR.
