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

### Why the scanner route is sound for Ada

Ada has one comment form, `--` to end of line, and one string form, double
quoted with `""` doubling. The single quote is genuinely overloaded — it opens a
character literal and it introduces an attribute (`X'Access`, `Integer'First`) —
but the overload is resolved by a local, exact rule that real Ada lexers use: a
tick whose preceding non-blank character is an identifier character or `)` is an
attribute tick; otherwise it opens a character literal, which is then exactly
three bytes wide.

The fixed width is what makes it exact rather than approximate. `'''` — the
character literal holding a quote — is the shape that defeats a scanner that
searches for a closing tick, and consuming three bytes handles it without a
special case. `Ptr.all'Address` needs no special case either: `all` ends in an
identifier character.

This is a stronger claim than "the delimiters happen not to collide", and it is
stated this way deliberately. A later refusal for another language must name a
specific undecidable construct, not the mere fact that a character has two jobs.

## Decision

### D1. Admission is `.adb` bodies

AUnit registers routines in a package body. `.ads` specs and `.gpr` project
files remain inventory, so ADR-0033's `.gpr` boundary is untouched.

Like MSTest and unlike `go test` or testthat, AUnit defines no file set: nothing
in the framework distinguishes a test body from any other body. Narrowing by
filename would invent a convention and present it as evidence, so the suffix is
the only filter and the blast radius is bounded by what the frontend emits, in
D3.

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

Every condition is required. The file carries a `with` clause whose first
identifier is `AUnit`. A call is made to `Register_Routine`, either bare or
through a dotted prefix (`Registration.Register_Routine` and
`AUnit.Test_Cases.Registration.Register_Routine` are both written in practice,
so both are admitted). The call has exactly three top-level arguments; the
second is a name followed by `'Access`; the third is a string literal.

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

### D4. The scanner is comment-, string-, and tick-aware

`--` comments, double-quoted strings, and character literals never contribute to
an anchor, and the tick rule in the context section is implemented exactly as
stated. This is a decision because the repository has shipped the opposite
defect four times.

### D5. What the anchor claims

It claims that a body importing AUnit registers a named routine with a literal
description. It claims nothing about execution, outcomes, ordering, the suite
that collects the fixture, assertion results, or whether the registered routine
exists — `'Access` is read as syntax, not resolved. No GNAT, `gprbuild`,
`gnattest`, Alire, Libadalang, child process, or network operation runs, and
ADR-0033's boundaries are carried forward.

### D6. No dependency

No Rust crate, no grammar, no toolchain, no downloaded artifact. ADR-0033 D3's
Libadalang `NO_GO` stands and is not reopened.

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
  three-byte width with the preceding-character rule is exact.

## Consequences

- Ada gains a frontend, owned units, typed `UNKNOWN`s, and one exact family for
  one call shape. It gains no project model and no provider.
- `.adb` bytes cross the source-store boundary; `.ads` and `.gpr` do not.
- ADR-0033 remains in force for discovery, Alire inventory, the Libadalang
  `NO_GO`, limits, and every execution prohibition.
- Widening the admitted call set, the admitted suffix set, or the claim surface
  requires a superseding ADR.
