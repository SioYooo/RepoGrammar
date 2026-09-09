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

### Why a hand-written frontend is sound for VB.NET

VB.NET lexes unusually cleanly. Comments begin with `'` or `REM` and run to end
of line, and VB has no single-quoted string, so the character that starts a
comment is never a string delimiter — the ambiguity that rules Ruby out and
makes MATLAB doubtful simply does not exist here. Strings are double-quoted only
and escape by doubling, with no backslash escapes. Declarations close with
`End Sub` / `End Class` rather than with nesting punctuation, and every keyword
that opens a block belongs to a closed set, which is what lets a body be skipped
without being understood.

The first version of this frontend was a line-oriented scanner. It is now a
lexer and a recursive-descent parser (D4b), because a scanner could not satisfy
ADR-0020 gate 2 on its own terms: it had no syntax failure, so malformed input
was indistinguishable from input with fewer declarations.

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
or without empty parentheses, and it stands in the attribute list of the
declaration it applies to. The declaration is a `Sub` — a `Function` returns a
value and is not an MSTest test. The enclosing class carries `TestClass` under
the same import or FQN rule, because MSTest does not discover a `[TestMethod]`
outside a `[TestClass]`, which is the rule the shipped C# anchor already
enforces.

The attribute binds to its declaration **by grammar, not by line position**.
The original scanner required the attribute to begin its own line, which was a
property of the scanner rather than of the language; `<TestMethod()> Public Sub
LoadsCatalog()` is the same declaration and now anchors identically. This is not
a wider claim — it is the same claim, established by parsing instead of by
looking at column zero. What kept an XML literal from being read as an attribute
list was that line rule; D4b replaces it with a rule about position in the
grammar plus an explicit refusal.

VB.NET is case-insensitive, so keyword and attribute matching is
case-insensitive; namespace and identifier spelling is preserved and never
claimed as identity.

NUnit, xUnit, `<DataTestMethod>`, `<TestInitialize>`/`<TestCleanup>`,
inherited test classes, and `<Ignore>` are out of scope. They are named
follow-ups.

`Partial` is decided rather than left to accident. A part that itself carries
`<TestClass()>` anchors its own `Sub` members, because VB attributes are
additive across parts and that part alone proves the attribute. A part that does
not carry it anchors nothing, even when a sibling file's part does. This
frontend never assembles a type across files, so the second case understates
support rather than guessing — and understatement is the direction this ADR
takes everywhere.

### D3. Only admitted declarations become units

The frontend emits a module unit per decoded file, one unit for an admitted
`TestClass`, and one unit for each admitted `TestMethod`. Ordinary VB
declarations produce no unit.

This is the bound that replaces a filename gate. It also carries a non-claim
that must be stated rather than inferred: the absence of a unit is not evidence
that a file contains no code. This frontend describes MSTest declarations, not
VB structure.

### D4. The frontend reads only text that is source

An attribute or declaration is recognized only from text that is source. `'` and
`REM` comments and double-quoted strings never contribute to an anchor.

This is a decision because the repository has shipped the opposite defect three
times — a Rust attribute matched inside a function body, a TS/JS runner matched
inside a member call, a Go declaration matched inside a string.

### D4b. The declared subset, and why the parse is version-invariant

The frontend is a hand-written lexer and recursive-descent parser over a
declared subset of VB.NET. It abstains; it never recovers. This is the second
half of what ADR-0020 gate 2 asks for, and it follows ADR-0040's argument for
SQL rather than inventing a new one.

**The declared invariance set is VB.NET language versions 10 through 17** —
Visual Studio 2010 onward, .NET Framework 4.0 through the current .NET. This is
a set, not a selection: a construct is admitted only when every member of the
set lexes and nests it the same way, so the frontend never selects a version and
never needs one. VB 9 and earlier are outside the set, exactly as MySQL is
outside ADR-0040's.

Admitted, because every member of the set agrees:

- `'` and `REM` line comments, and `"…"` strings with `""` doubling.
- `[…]` escaped identifiers, which by construction never match a keyword.
- `#…#` date literals, consumed opaquely so their `/` and `:` are not read as
  code.
- Explicit `_` line continuation, and implicit continuation **inside `(` or
  `{`** — the parameter-list and argument-list case, which is VB 10 and later.
- `Imports`, plain and aliased.
- `Option Strict`, `Option Explicit`, `Option Infer`, and `Option Compare`.
- Attribute lists, including a `<Assembly: …>` target specifier.
- `Namespace`, `Class`, `Module`, `Structure`, `Interface`, and `Enum` blocks.
- `Sub` and `Function` declarations, with the ordinary modifier set; and their
  bodyless forms under `MustOverride`, `Declare`, and inside an `Interface`.
- `Property` — auto and with `Get`/`Set` accessors, distinguished by one member
  of lookahead, which is the language's own distinction — plus `Operator`,
  `Event`, `Custom Event`, and the event accessors.
- The block statements `If … Then`, `Select`, `Try`, `With`, `While`, `Do …
  Loop`, `For … Next`, `Using`, and `SyncLock`, and the single-line `If`, which
  is decidable from whether `Then` ends the statement.
- Multi-line and single-line lambdas.

Statement *contents* are never parsed, because no anchor rests on what a
statement means. What the parser must get right is where a statement ends and
whether it opens a block, and that is decidable: the keywords that open a block
are a **closed set**, so an identifier outside it cannot open one. Closers are
keyword-typed — `End Sub` closes only a `Sub` — so if the parser ever mistracks
a nested block it meets the wrong closer and abstains. It cannot silently
attribute an inner `End` to an outer declaration.

Refused, each as a **whole-file** abstention with a typed `UNKNOWN` and a
degraded-parse diagnostic:

- **Interpolated strings, `$"…"`.** VB 14 and later only. In VB 10 to 13 a `$`
  is solely a legacy type character on an identifier, so `$"` is not legal at
  all: a file using one is not a program in every member of the declared set,
  which is the same exclusion shape as VB 9 itself. Lexing it would also mean
  tracking interpolation holes, which nest arbitrary expressions and their own
  string literals and brace escapes, so the frontend refuses the character
  rather than carrying a second and harder lexer for a construct the declared
  set does not share. This is ADR-0040's refusal of `$` in a different
  language, for a version divergence rather than a dialect one.
- **XML literals.** A `<` that begins an XML name outside attribute position is
  refused. An XML literal may contain any text at all, including lines that read
  exactly like a declaration, so once one opens the token stream stops being
  decidable. This is not hypothetical: it is the same class of defect as
  anchoring both branches of a `#If`, and the line-oriented scanner had it.
- **A string or escaped identifier left open at end of line.**
- **A `#` directive this frontend does not read, or a `#If` that never closes.**
- **Anything else outside the grammar** — an unclosed block, a mismatched `End`,
  an attribute list split across lines without a continuation.

The continuation rule has one consequence worth naming, because it is not
obvious from the rule itself. VB 10 also continues a line implicitly between
LINQ query operators, which this frontend does not admit: a query written across
several lines splits into separate statements, its `Select` clause reads as the
start of a `Select` block, and the file abstains. That is the safe direction and
it is decidable, but it is real lost recall on real code, so it is stated rather
than discovered.

A refusal is file-level rather than declaration-level for ADR-0040 D3's reason:
once the token stream diverges, every later boundary in the file is unproven, so
a per-declaration degradation would report confident boundaries derived from an
unproven split.

### D4c. What the unproven project profile cannot change

ADR-0020 gate 2 is read in this repository as asking for an authoritative
frontend *and* a versioned project profile. The second half is discharged the
way ADR-0040 discharged the unproven SQL dialect: by showing it is
**claim-irrelevant** for the admitted parse, not by pinning it.

- `Option Strict`, `Option Explicit`, and `Option Infer` change binding,
  overload resolution, and inference. They change no token boundary and no
  declaration shape, so they cannot change which declarations this frontend
  admits.
- The target framework and the MSBuild profile change which assemblies are
  referenced. They do not change the token stream. The anchor claims that the
  source binds the MSTest attribute spelling by import or qualification; it
  does not claim the attribute type resolves, that the package is restored, or
  that the test is discovered at run time — D5 already says so.
- The language version is bounded by the declared set above, and every construct
  outside the set is refused rather than read under an assumption.

What remains genuinely unknown stays `UNKNOWN`: `#If` constants (D4a), which
part of a `Partial` type another file contributes (D2), assembly identity,
generated code, and every runtime behaviour.

This distinction is load-bearing in the same way ADR-0040 D2's is. If a later
change admits a construct whose parse differs across the declared version set —
interpolated strings being the obvious candidate — that construct's anchors
become version-dependent and lose family eligibility. Widening the admitted
subset or the declared version set is therefore an ADR decision, not an
implementation detail.

### D4a. Conditional compilation bounds the claim

`#If` selects one branch at compile time from a constant this frontend does not
evaluate. **No declaration inside a conditional region anchors**, in any branch,
and the file records `BuildVariantAmbiguity` under `vb_conditional_compilation`.
Admitting every branch would invent a member that never compiles; admitting one
would assert a constant we do not know.

The rule applies to the whole span an anchor covers — its attribute, its
declaration, and its `End` — because an anchor's recorded evidence range runs
from the attribute to the `End`, and a span whose end line sits in a branch has a
branch-dependent range even when its opening does not. A `#If` wholly inside a
method body leaves the declaration and its `End Sub` unconditional, so the
common case of a conditional statement inside a test does not cost the anchor.

It also applies one level up, to `Imports`. An import selected by an unevaluated
constant cannot make a bare attribute name resolve, so only unconditional code
binds the spelling; a conditional import plus bare attributes falls to the
existing `mstest_attribute_without_import` unknown, which does block. A fully
qualified attribute never needed the import and is unaffected.

Directive classification is deliberately asymmetric. `#If` is matched on its name
alone rather than on a trailing `Then`, and only `End` followed by `If` closes a
region — `#End Region` and `#End ExternalSource` do not, or a region opened by
`#If` would silently reopen the file mid-branch. A missed open would anchor a
declaration the build may not contain, which is unsound; a missed close only
leaves the depth high, which understates support and cannot invent a member.
`#ElseIf` and `#Else` stay inside the region. `#Region`, `#ExternalSource`,
`#Const`, and `#Enable`/`#Disable Warning` select no branch and change no claim.
Matching is case-insensitive, as the language is.

The claim is deliberately separate from `vb_mstest_attribute_binding` and
deliberately non-blocking. A skipped branch understates support; it does not
unprove the declarations sitting in unconditional code, and those still form
their family.

A `#If` that never closes is different, and is handled under D4b instead: the
file is malformed, so it abstains whole rather than reporting a clean parse of
whatever happened to close.

This is the first half of what ADR-0020 gate 2 asks for: the admitted parse must
not depend on what cannot be determined. Where it would, the frontend abstains
and says so rather than choosing.

### D5. What the anchor claims

It claims that a class and method carry the MSTest attributes that make the
method discoverable as a test. It claims nothing about execution, outcomes,
ordering, data rows, initialization, deployment items, or the selected test
framework of the project. No MSBuild, Roslyn, `vbc`, `dotnet`, NuGet, analyzer,
source generator, package script, child process, or network operation runs, and
ADR-0031's execution prohibitions are carried forward unchanged.

### D6. No dependency

No Rust crate, no grammar, no toolchain, no downloaded artifact. The lexer and
parser are hand-written in the existing Rust core. This ADR does not authorize a
Tree-sitter VB grammar, a Roslyn binding, `vbc`, `dotnet`, or any other external
artifact, and it does not reserve the right to add one later without a
superseding decision.

## Alternatives considered

- Gate on the `.vbproj` MSTest `PackageReference`: rejected because ADR-0031 D4
  forbids treating package presence as framework behaviour, and the source
  attribute plus import is stronger evidence anyway.
- Keep the line-oriented scanner and argue gate 2 from the cleanliness of VB's
  lexis: rejected because a scanner has no syntax failure, so malformed input is
  indistinguishable from input with fewer declarations, and because the scanner
  demonstrably read an XML literal's contents as declarations.
- Pin a Roslyn version and a target framework to satisfy gate 2's project
  profile: rejected under D4c. The profile changes binding and reference
  resolution, not the admitted parse, so pinning it would assert a selection
  this frontend has no evidence for while proving nothing the anchor needs.
- Recover from an unadmitted construct and keep the declarations already found:
  rejected because a recovered tree cannot prove a declaration, and because a
  lexical divergence invalidates every later boundary in the same file.
- Select one VB language version as the reference grammar: rejected because it
  would make every anchor depend on an unproven selection and would forfeit
  family eligibility for no gain in the admitted subset.
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
- A file that uses an interpolated string contributes no anchors at all. This
  costs real recall, because `$"…"` is common in test bodies, and it is the
  intended conservative failure rather than something to soften by falling back
  to one language version.
- Malformed VB now fails. A file the parser cannot admit yields its module unit,
  a typed `UNKNOWN`, and a degraded-parse diagnostic instead of quietly yielding
  fewer anchors. Fewer anchors and no anchors are no longer the same output.
- Widening the admitted attribute set, the admitted grammar, the declared
  version set, or the claim surface requires a superseding ADR.
