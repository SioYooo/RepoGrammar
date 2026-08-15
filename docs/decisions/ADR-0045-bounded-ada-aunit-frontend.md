# ADR-0045: Bounded Ada AUnit frontend

- Status: Accepted
- Date: 2026-08-15
- Scope: Ada in ADR-0020; admits a family-bearing frontend over one
  registration-call shape
- Refines: ADR-0033 (authorizes the source unit it declined to authorize)
- Related: ADR-0044, ADR-0043, ADR-0042, ADR-0019,
  `docs/reports/language-support/ada-completion-review.md`,
  `docs/plans/multi-language-expansion-plan.md`

## Context

Ada source is discovered and never decoded. ADR-0033 D1 keeps `.ads`, `.adb`,
and `.gpr` as inventory before SourceStore access, creating no code unit, IR,
semantic fact, framework role, family, or support record.

### The evidence ladder was read first

ADR-0033 carries no forbidden-evidence list at all, and its one exclusion — D3's
`NO_GO` on Libadalang — rules out a dependency, an artifact, a process, and a
fallback. This frontend adds none of those, so nothing in ADR-0033 stands
against a bounded scanner. That check is the point: ADR-0041's first version
authorized a Go family without reading ADR-0021's ladder, whose item 4 does
forbid this route, and the family had to be withdrawn.

### Why the lexical layer is exact for Ada

Ada has one comment form, `--` to end of line, and one string form, double
quoted with `""` doubling. The single quote is genuinely overloaded — it opens a
character literal and it introduces an attribute (`X'Access`, `Integer'First`) —
but the overload is resolved by a local, exact rule that real Ada lexers use: a
tick is an attribute tick exactly when the token before it can end a name (an
identifier, `)`, `all`, or an operator symbol); otherwise it opens a character
literal, which is then exactly one character wide.

The fixed width is what makes it exact rather than approximate. `'''` — the
character literal holding a quote — is the shape that defeats a scanner that
searches for a closing tick, and consuming one character handles it without a
special case. `Ptr.all'Address` needs no special case either.

This is a stronger claim than "the delimiters happen not to collide", and it is
stated this way deliberately. A later refusal for another language must name a
specific undecidable construct, not the mere fact that a character has two jobs.

### Why a parser, and not the scanner this ADR first admitted

The first version of this frontend was a byte-oriented scanner: it masked out
comments and literals, found the identifier `Register_Routine`, and matched
parentheses and commas around it. It produced the right answer on the fixtures,
and it could not say why.

ADR-0020 gate 2 asks that the primary syntax evidence come from a frontend whose
fidelity boundary is explicit. A scanner has no fidelity boundary: it cannot
distinguish `with AUnit.Test_Cases;` from the `with` of a type extension, it
cannot tell a call in a package body from text that merely looks like one, and
above all it cannot state what it refuses, because it refuses nothing. Its
failure mode is a quiet wrong answer.

This ADR therefore replaces it with a hand-written lexer and recursive-descent
parser over a declared subset, with **two outcomes and no third**: the whole
compilation unit parses, or the file is refused and contributes no anchor. That
failure mode is what makes a bounded subset acceptable — a gap in the grammar
costs recall and can never invent a registration.

## Decision

### D1. Admission is `.adb` bodies

AUnit registers routines in a package body. `.ads` specs and `.gpr` project
files remain inventory, so ADR-0033's `.gpr` boundary is untouched.

Like MSTest and unlike `go test` or testthat, AUnit defines no file set: nothing
in the framework distinguishes a test body from any other body. Narrowing by
filename would invent a convention and present it as evidence, so the suffix is
the only filter and the blast radius is bounded by what the frontend emits, in
D3.

Within a `.adb`, the admitted compilation units are a **package body** and a
**library-level subprogram body**. A subunit (`separate (Parent) procedure P is
…`) is refused, for the reason D2 gives for the context clause: a subunit
inherits its parent's context clause, so admitting one would assert an import
this frontend cannot see. A `.adb` holding spec text — which happens when a GPR
`Naming` package reassigns the suffix — does not contain a body and therefore
does not parse, so the grammar refuses it without needing to read the GPR.

### D2. The exact anchor

One shape is admitted:

```ada
with AUnit.Test_Cases;

package body Catalog_Tests is

   procedure Register_Tests (T : in out Test_Case) is
   begin
      Register_Routine (T, Loads_Catalog'Access, "loads the catalog");
   end Register_Tests;

end Catalog_Tests;
```

Every condition is required, and every one of them is now read off the parse
rather than off the text. The file carries a real **context clause** whose
withed unit's first identifier is `AUnit` — the `with` of a type extension or of
an aspect specification is a different production and does not count. A
**procedure call statement** names `Register_Routine`, either bare or through a
dotted prefix (`Registration.Register_Routine` and
`AUnit.Test_Cases.Registration.Register_Routine` are both written in practice,
so both are admitted). Its actual parameter part has exactly three
associations; the second is a positional actual that is a plain dotted name with
a final `'Access` attribute and nothing else; the third is a positional actual
that is exactly one string literal.

"Exactly one string literal" is stricter than the text test it replaces:
`"a" & "b"` starts and ends with a quote but is a concatenation, and
`Table (1)'Access` is an `'Access` on a call result rather than on a name.
Neither is admitted. Named notation for the second and third actuals
(`Routine => X'Access`) is not the admitted positional shape.

The call must be a **procedure call statement**. AUnit's `Register_Routine` is a
procedure, so `Handle := Register_Routine (…)` names something else, and the
text test that preceded this frontend could not tell the two apart.

Ada is case-insensitive, so keyword, identifier, and attribute matching is
case-insensitive. The call may span lines, because Ada style routinely breaks
before the argument list.

The `with` clause must be in the same file. A package body inherits its spec's
context clause, so a body that uses AUnit without naming it is a real shape and
this frontend does not admit it. That is a false negative, chosen for the same
reason ADR-0044 chose one: an anchor that assumes an import it cannot see is
asserting evidence it does not have.

Out of scope and named rather than left to inference: `AUnit.Simple_Test_Cases`,
which overrides `Run_Test` and makes no registration call at all;
`AUnit.Test_Caller` with `Add_Test (Caller.Create (...))`; a `Register_Routine`
call passing a `Routine_Access` variable instead of a `'Access` attribute; and
GNATtest-generated harnesses.

### D3. Only admitted calls become units

The frontend emits a module unit per decoded `.adb` file and one unit per
admitted registration call. Ordinary Ada declarations produce no unit, so the
absence of a unit is not evidence that a file has no code.

### D4. The frontend abstains; it never recovers

`--` comments, double-quoted strings, and character literals never contribute to
an anchor, and the tick rule in the context section is implemented exactly as
stated. This is a decision because the repository has shipped the opposite
defect four times.

Beyond that, the frontend has exactly two outcomes. Either the lexer produces a
token stream and the parser consumes the entire compilation unit, in which case
the admitted registration calls are reported; or the file is **refused**, in
which case it yields its module unit, a typed `UNKNOWN` naming the refusal
class, and **no registration at all**. There is no error recovery, no
resynchronisation, and no partial tree. This matches the bar ADR-0024 D6 and
ADR-0025 D7 set for PHP and Swift, which forbid partial or recovered AST
anchors; Ada holds the same bar.

An earlier version of this decision said that "the units already found are
kept" when a string literal was left open. That was the right rule for a
scanner and is the wrong rule for a parser: an open literal means every token
after it was read as literal text, so the boundaries that would delimit a later
call are unproven. Such a file now abstains entirely. The degraded-parse
diagnostic that reports it survives unchanged and is strengthened, because it
now accompanies an abstention rather than a partial result.

Refusals are tiered, because they are not all the same event:

- A **malformed file** (an open literal) and a **selected build variant** (D4b)
  report an `Error` parse diagnostic in addition to the typed `UNKNOWN`. The
  file claims to be Ada and either is not well formed or is one of several
  texts.
- A construct that is merely **outside the declared subset** reports the typed
  `UNKNOWN` only. Most real Ada is outside any bounded subset; that is a limit
  of this frontend, not a defect in the repository, and it must not raise an
  operator warning on every Ada file.

No refusal carries source text. The refusal vocabulary is a fixed enumeration:
no offending token, identifier, literal, or line number reaches a diagnostic
message, a fact note, or an assumption, because those surfaces are read by
`index --json`, `unknowns`, and the MCP readiness payloads.

### D4a. The declared subset

The subset is declared here so that "the frontend refused it" is auditable
rather than an implementation accident. Lexically it admits Ada RM 2.x
identifiers (ASCII), decimal and based numeric literals, character literals,
string literals, `--` comments, and the delimiter set including every compound
delimiter. Grammatically it admits:

```ebnf
compilation      ::= { pragma | with_clause | use_clause } library_item
library_item     ::= package_body | subprogram_body
package_body     ::= "package" "body" name [ aspect_spec ] "is"
                     declarative_part [ "begin" handled_statements ]
                     "end" [ name ] ";"
declarative_item ::= pragma | use_clause
                   | object_declaration | number_declaration
                   | exception_declaration | renaming_declaration
                   | type_declaration | subtype_declaration
                   | subprogram_declaration | subprogram_body
                   | package_declaration | package_body
                   | generic_instantiation
statement        ::= null_statement | procedure_call | assignment
                   | if_statement | case_statement | loop_statement
                   | block_statement | exit | goto | label
                   | return_statement | extended_return | raise | pragma
expression       ::= the full RM 4.4 operator grammar, names with selected,
                     indexed, attribute and qualified suffixes, aggregates,
                     allocators, and the Ada 2012 conditional, case, and
                     quantified expressions
```

Named rather than left to inference, the following are **not** admitted, and a
file containing one abstains as a whole: generic declarations (instantiations
are admitted), task and protected types and bodies, entries, `accept`,
`select`, `requeue`, `abort`, `delay`, `terminate`, interface and synchronized
types, representation clauses (`for X'Address use …`), subunits, Ada 2022
declare expressions, square-bracket array aggregates, and the `@` target name.

Two resource ceilings bound the parse: the existing input-byte limit, and a
nesting depth ceiling. Untrusted input may nest arbitrarily and a stack overflow
aborts the process, so a depth breach is a refusal like any other.

### D4b. The admitted parse does not depend on anything undeterminable

This is the argument ADR-0020 gate 2 asks for, and it is the reason the subset
is shaped the way it is. Four things could decide how an Ada file parses, and
RepoGrammar can see none of them. Each is either proved irrelevant or refused.

**The Ada edition (83 / 95 / 2005 / 2012 / 2022).** The edition is selected by a
GNAT switch, a GPR scenario, or a configuration pragma. The declared invariance
set is **{Ada 95, Ada 2005, Ada 2012, Ada 2022}**, and the claim is the
conditional form: *for every text this grammar admits, the token stream and the
phrase structure are the same under every member of the set under which that
text is legal.*

Ada 83 is excluded and the exclusion is free: `'Access` does not exist in Ada
83, so no Ada 83 compilation unit can contain the admitted anchor at all. The
anchor's own second actual parameter proves the edition is not 83.

Within the set, the one construct class that can make a single text parse two
ways is a word whose classification moved: `interface`, `overriding`, and
`synchronized` became reserved in Ada 2005, `some` in Ada 2012, and `parallel`
in Ada 2022. `Interface : Boolean;` is a legal Ada 95 object declaration and an
Ada 2005 syntax error — two editions, two readings, and no repository-local
evidence to choose. Those five words therefore get their own token kind and are
refused everywhere except one position: `overriding` immediately before
`procedure` or `function`, where the Ada 95 reading is not legal at all, so the
editions under which the text is legal cannot disagree about it. Everything else
the later editions added — aspect specifications, expression functions,
conditional and quantified expressions, extended return — is new syntax that was
a syntax error before, not a re-reading of old syntax, so the same conditional
argument covers it.

**Configuration pragmas.** `pragma Ada_95`, `Ada_05`, `Ada_2005`, `Ada_12`,
`Ada_2012`, and `Ada_2022` select a member of the invariance set, so the
admitted parse provably cannot change and they are admitted. `pragma Ada_83` and
`pragma Extensions_Allowed` select something outside the set, so they are
refused with a degraded parse. This is the sharper form of ADR-0044's `{$MODE}`
rule: the invariance argument does the work, and only the pragmas that escape
the set cost anything.

**GNAT preprocessor directives.** A file containing a line-leading `#if`,
`#elsif`, `#else`, or `#end if`, or a `$symbol` substitution, is refused as a
whole, with `BuildVariantAmbiguity` under `ada_conditional_compilation` and a
degraded parse. This is stricter than ADR-0044's region-level rule for Delphi,
and deliberately so: `{$IFDEF}` is a construct the Object Pascal compiler
itself processes, whereas `gnatprep` is a separate text pass whose *output* is
the Ada source. The text on disk is preprocessor input, not a compilation unit;
neither branch need be independently well formed, and a `$symbol` can expand to
anything, including tokens that change the call it sits inside. There is no
sound region to admit.

**GPR project files and scenario variables.** ADR-0033 keeps `.gpr` as
inventory, so the project model is unavailable. It changes three things, and
none of them reaches the admitted parse. It can select the edition — covered
above. It can define preprocessor symbols — covered above. It can change source
naming, so that a `.adb` is not a body — but the grammar requires a body, so
such a file simply fails to parse. What a GPR also decides, and this frontend
does not claim, is whether a file is in the build at all: the anchor asserts
that a body registers a routine, never that the body is compiled.

Obsolescent RM J.2 replacement characters (`%` for `"`, `!` for `|`, `:` for
`#`) are refused rather than interpreted, because each one moves literal
boundaries and reading the character either way is a choice this frontend has no
evidence for.

Each refusal is recorded under a claim that names what it affects, and the
claims are deliberately separate from `ada_aunit_registration_binding`, which
stays the one blocking claim:

| Refusal | Reason code | `affected_claim` | Degraded |
|---|---|---|---|
| `gnatprep` directive or `$symbol` | `BuildVariantAmbiguity` | `ada_conditional_compilation` | yes |
| Edition-selecting pragma; edition-sensitive reserved word | `BuildVariantAmbiguity` | `ada_language_edition` | yes |
| Unterminated literal | `InsufficientSupport` | `ada_registration_scan` | yes |
| Replacement character; character or construct outside D4a; nesting or byte ceiling | `InsufficientSupport` | `ada_registration_scan` | no |

### D5. What the anchor claims

It claims that a body importing AUnit registers a named routine with a literal
description. It claims nothing about execution, outcomes, ordering, the suite
that collects the fixture, assertion results, or whether the registered routine
exists — `'Access` is read as syntax, not resolved. No GNAT, `gprbuild`,
`gnattest`, Alire, Libadalang, child process, or network operation runs, and
ADR-0033's boundaries are carried forward.

### D6. No dependency

No Rust crate, no grammar, no toolchain, no downloaded artifact. The lexer and
the parser are hand-written Rust in this repository. ADR-0033 D3's Libadalang
`NO_GO` stands and is not reopened, and this ADR does not reserve the right to
add an Ada dependency later without a superseding decision.

## Alternatives considered

- Anchor on `AUnit.Test_Cases.Test_Case` type derivation: rejected because the
  derivation declares a fixture type, and the registered routines are what AUnit
  runs; the registration call names both the routine and its description.
- Read the sibling `.ads` for the context clause: rejected for this slice under
  D2. It is a project-context change, and the false negative it removes is
  smaller than the coupling it adds.
- Admit `.ads`: rejected under D1; registration calls live in bodies.
- Treat a tick as a character-literal opener and scan for its closing tick:
  rejected because `'''` and every attribute reference break it. The fixed
  one-character width with the previous-token rule is exact.
- Keep the byte-oriented scanner and argue gate 2 from its accuracy on the
  fixtures: rejected because a scanner has no fidelity boundary to state. It
  cannot refuse, so it cannot say what it does not know, and its failure mode is
  a quiet wrong answer rather than an abstention.
- Skip only the unparsable declaration and keep the rest of the file: rejected
  because a skip needs the declaration's extent, and computing an extent without
  parsing is the scanner assumption this ADR is replacing. Once a construct is
  unparsed, every later boundary in the unit is unproven.
- Select one Ada edition — for instance the GNAT default — and parse under it:
  rejected because every anchor would then rest on an unproven selection, which
  is exactly the position ADR-0038 records for assembly and ADR-0040 refuses for
  SQL. The invariance set costs nothing in the admitted subset and buys family
  eligibility.
- Admit `gnatprep` files by parsing one branch, or by parsing the text with
  directives stripped: rejected under D4b. Stripping asserts a symbol
  definition that RepoGrammar cannot see, and admitting both branches invents a
  registration that never compiles.

## Consequences

- Ada gains a frontend, owned units, typed `UNKNOWN`s, and one exact family for
  one call shape. It gains no project model and no provider.
- `.adb` bytes cross the source-store boundary; `.ads` and `.gpr` do not.
- ADR-0033 remains in force for discovery, Alire inventory, the Libadalang
  `NO_GO`, limits, and every execution prohibition.
- Recall is lower than the scanner's and the failure direction is safe: any
  `.adb` using a construct outside D4a contributes no anchor, and the absence is
  recorded as a typed `UNKNOWN` rather than passed off as an empty file.
- Widening the admitted call set, the admitted suffix set, the declared subset,
  the invariance set, or the claim surface requires a superseding ADR.

## Follow-up

- Widening D4a to admit representation clauses, generic declarations, or
  subunits is the highest-value recall work, and each needs its own extent
  argument before it is admitted.
- Nothing here resolves a registered routine to a subprogram, so overload,
  dispatch, and generic-instantiation identity remain `UNKNOWN` with no
  provider to discharge them.
