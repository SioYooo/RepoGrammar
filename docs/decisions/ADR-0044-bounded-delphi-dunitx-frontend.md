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

Attribution is positional and bounded: a `[Test]` belongs to the most recent
class declaration, and a class declaration without `[TestFixture]` clears the
fixture state. So a `[Test]` in a plain class that follows a fixture does not
anchor.

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
  depth model would be wrong in ordinary code. Positional attribution with an
  explicit clearing rule is smaller and states its own boundary.
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
