# ADR-0043: Bounded VB.NET MSTest frontend

- Status: Accepted
- Date: 2026-08-15
- Scope: Visual Basic .NET in ADR-0020; admits a family-bearing frontend over
  one attribute shape
- Refines: ADR-0031 (authorizes the VB source unit it declined to authorize)
- Related: ADR-0042, ADR-0041, ADR-0019,
  `docs/reports/language-support/visual-basic-completion-review.md`,
  `docs/plans/multi-language-expansion-plan.md`

## Context

VB.NET source is discovered and never decoded. ADR-0031 D2 states that source is
never read or parsed and D4 states that no VB source frontend, IR, framework
role, family, or readiness exists. This ADR authorizes the narrowest useful
exception.

### The evidence ladder was read first

ADR-0031 D4 does carry an evidence ladder, and its forbidden item names
"extension-only support claims, MSBuild/NuGet output, ambient SDK/toolchain
state, package presence as framework behavior, or any VB6-to-VB.NET
equivalence". **It does not forbid text matching for the claim.**

That check is the point. ADR-0041's first version authorized a Go family without
reading ADR-0021's ladder, whose item 4 does forbid this route, and the family
had to be withdrawn. Go, Ruby, PHP, and Swift are closed for that reason;
VB.NET, like R under ADR-0036, is not.

Two forbidden items still bind here and shape the design. Nothing may be claimed
from an extension alone, so the anchor is an exact attribute with import
evidence rather than a `.vb` suffix. And package presence is never framework
behaviour, so the `.vbproj` `PackageReference` inventory ADR-0031 already
collects is **not** used as the gate — the source itself carries the evidence.

### Why the scanner route is sound for VB.NET

VB.NET lexes unusually cleanly for a bounded scan. Comments begin with `'` or
`REM` and run to end of line, and VB has no single-quoted string, so the
character that starts a comment is never a string delimiter — the ambiguity that
rules Ruby out and makes MATLAB doubtful simply does not exist here. Strings are
double-quoted only and escape by doubling, with no backslash escapes and no
interpolation in the classic form. Declarations are line-oriented and closed by
`End Sub` / `End Class` rather than by nesting punctuation.

MSTest is also the framework whose anchors VB shares with C#, where RepoGrammar
already ships exactly this shape: `[TestMethod]` inside `[TestClass]`, gated by
a using or a fully qualified name. VB writes the same attributes with angle
brackets and gates them with `Imports`.

## Decision

### D1. Admission is every discovered `.vb` file

Unlike Go and R, VB.NET has no runner-defined file set: `go test` compiles only
`_test.go`, and testthat runs only `tests/testthat/`, but MSTest discovers tests
by attribute wherever they are compiled. Narrowing by filename would therefore
be an invented convention rather than inherited evidence, so every discovered
`.vb` file is decoded.

The blast radius is instead bounded by what the frontend emits, in D3.

### D2. The exact anchor

One shape is admitted:

```vb
Imports Microsoft.VisualStudio.TestTools.UnitTesting

<TestClass()>
Public Class CatalogTests
    <TestMethod()>
    Public Sub LoadsCatalog()
    End Sub
End Class
```

Every condition is required. The file carries an exact
`Imports Microsoft.VisualStudio.TestTools.UnitTesting`, or the attribute is
written with that namespace fully qualified. The attribute is `TestMethod`, with
or without empty parentheses, and it sits on its own line immediately before the
declaration it applies to. The declaration is a `Sub` — a `Function` returns a
value and is not an MSTest test. The enclosing class carries `TestClass` under
the same import or FQN rule, because MSTest does not discover a `[TestMethod]`
outside a `[TestClass]`, which is the rule the shipped C# anchor already
enforces.

VB.NET is case-insensitive, so keyword and attribute matching is
case-insensitive; namespace and identifier spelling is preserved and never
claimed as identity.

NUnit, xUnit, `<DataTestMethod>`, `<TestInitialize>`/`<TestCleanup>`,
inherited or partial test classes, and `<Ignore>` are out of scope. They are
named follow-ups.

### D3. Only admitted declarations become units

The frontend emits a module unit per decoded file, one unit for an admitted
`TestClass`, and one unit for each admitted `TestMethod`. Ordinary VB
declarations produce no unit.

This is the bound that replaces a filename gate. It also carries a non-claim
that must be stated rather than inferred: the absence of a unit is not evidence
that a file contains no code. This frontend describes MSTest declarations, not
VB structure.

### D4. The scanner is comment- and string-aware

An attribute or declaration is recognized only from text that is source. `'` and
`REM` comments and double-quoted strings never contribute to an anchor, and XML
literals are not attributes: an admitted attribute must begin a line.

This is a decision because the repository has shipped the opposite defect three
times — a Rust attribute matched inside a function body, a TS/JS runner matched
inside a member call, a Go declaration matched inside a string.

### D5. What the anchor claims

It claims that a class and method carry the MSTest attributes that make the
method discoverable as a test. It claims nothing about execution, outcomes,
ordering, data rows, initialization, deployment items, or the selected test
framework of the project. No MSBuild, Roslyn, `vbc`, `dotnet`, NuGet, analyzer,
source generator, package script, child process, or network operation runs, and
ADR-0031's execution prohibitions are carried forward unchanged.

### D6. No dependency

No Rust crate, no grammar, no toolchain, no downloaded artifact.

## Alternatives considered

- Gate on the `.vbproj` MSTest `PackageReference`: rejected because ADR-0031 D4
  forbids treating package presence as framework behaviour, and the source
  attribute plus import is stronger evidence anyway.
- Admit only files matching a test-name convention: rejected under D1, because
  MSTest defines no such set and inventing one would be a guess dressed as
  evidence.
- Accept `<TestMethod>` outside a `<TestClass>`: rejected because MSTest does not
  discover it, and the shipped C# anchor already treats it as unresolved.
- Accept a `Function`: rejected because an MSTest test method returns nothing.

## Consequences

- VB.NET gains a frontend, owned units, typed `UNKNOWN`s, and one exact family
  for one attribute shape. It gains no project model and no provider.
- `.vb` bytes cross the source-store boundary. No execution, toolchain, or
  package resolution is added.
- ADR-0031 remains in force for discovery, `.vbproj` inventory, limits, and
  every execution prohibition.
- Widening the admitted attribute set or the claim surface requires a
  superseding ADR.
