# ADR-0051: Bounded Fortran test-drive frontend

- Status: Accepted (maintainer-ordered language-set completion, 2026-09-04)
- Date: 2026-09-04
- Scope: Fortran in ADR-0020; records the framework-backlog entry and admits one
  family-bearing frontend over one signature shape
- Refines: ADR-0034 (fpm inventory; keeps its discovery and Flang `NO_GO`
  boundary), ADR-0020
- Related: ADR-0040 and ADR-0044 (the bounded hand-written frontend precedents
  this design follows), ADR-0042 (the in-file import-proof pattern),
  `docs/plans/multi-language-expansion-plan.md` (Wave F2, which closed Fortran
  as `no_qualifying_candidate_yet` pending exactly this backlog entry),
  `docs/reports/language-support/fortran-completion-review.md`

## Context

ADR-0034 left Fortran `discovered_only`: free/fixed-form suffixes are inventoried
without decoding, `fpm.toml` yields a bounded dependency inventory, and Flang is
`NO_GO` for production admission because its prescanner expands `INCLUDE` and
preprocesses before parsing. Wave F2 then closed Fortran's framework lane as
`no_qualifying_candidate_yet`: the survey-backed priority backlog named no
Fortran framework, and choosing one at implementation time was recorded as the
exact failure mode the backlog exists to prevent. The plan's closing sentence is
explicit — reopening needs a backlog entry first, not an ADR first.

The maintainer has now explicitly ordered completing the language set. That
order is the authority this ADR uses to make the backlog decision itself:
selecting the first Fortran family is no longer an agent inventing scope, but an
agent recording a selection the maintainer authorized.

### The evidence was read from the framework's own sources, 2026-09-04

- <https://github.com/fortran-lang/test-drive> (`README.md` and
  `src/testdrive.F90`, current `main`): module name `testdrive`; the abstract
  `test_interface` is `subroutine test_interface(error)` whose single dummy is
  `type(error_type), allocatable, intent(out) :: error`; tests are collected by
  `collect` subroutines registering `new_unittest(name, test)` entries and run
  by `run_testsuite`. The project is pure Fortran (single redistributable
  `testdrive.F90`), integrates natively with fpm (`test-drive` dev-dependency),
  meson, and CMake, and is dual-licensed Apache-2.0 OR MIT.
- pFUnit (<https://github.com/Goddard-Fortran-Ecosystem/pFUnit>): xUnit-style
  framework requiring a CMake + CppUnit-heritage (C/Lua) build machinery and
  preprocessor-heavy directives (`@mpTest`/`@test` annotations processed by its
  own preprocessor).
- Vegetables (<https://gitlab.com/everythinginvegetables/fortran-vegetables>)
  and tUnit: smaller community frameworks; neither is the fpm ecosystem's
  default.

## Decision

### D0. The backlog entry: stdlib/test-drive is the first Fortran family

The priority-ordered framework backlog gains its Fortran entry, selecting
`test-drive` (github.com/fortran-lang/test-drive, module `testdrive`) as the
first Fortran family target. Rationale:

1. **Pure Fortran and fpm-native.** test-drive is the testing framework of the
   fortran-lang ecosystem that also maintains fpm and the stdlib; it is the
   default `test-drive` dev-dependency in fpm projects and needs no CMake,
   CppUnit heritage, or generator machinery.
2. **Exact, source-visible anchors.** The framework's own `test_interface` gives
   a simple exact anchor shape: a `use testdrive` statement in scope plus a
   `subroutine test_<name>(...)` whose first dummy argument is declared
   `type(error_type)` with `intent(out)`. No macro layer, no generated code, no
   attribute reflection is required to recognize it.
3. **Actively maintained** under fortran-lang with CI and releases.
4. **MIT OR Apache-2.0** dual license — permissive; and nothing is copied or
   linked, so the license only needs to permit study, which it does.

Why not pFUnit first: pFUnit's xUnit-style directives (`@test`,
`@before`/`@after`, `@assert...` macros) are realized by pFUnit's own
preprocessor and CMake/CppUnit-heritage machinery. Its anchors are therefore
preprocessor products, and ADR-0034 already refuses preprocessing inside this
lane; admitting pFUnit first would either widen that boundary or anchor on text
whose meaning the preprocessor manufactures. Why not Vegetables or tUnit first:
both are smaller-community frameworks whose runner conventions are less settled
in the ecosystem this snapshot targets, and neither offers a more exact anchor
than test-drive's interface. They remain named follow-ups, not rejections on
fitness grounds.

### D1. The declared subset is bounded free-form Fortran 2008+ with fpm
  conventions

The frontend admits only:

- paths discovery already classifies as free-form source (`.f90`, `.f95`,
  `.f03`, `.f08` under ADR-0034's lowercase rule);
- whole files whose program-unit structure is a subset of `module`, `program`,
  and top-level `subroutine`/`function` units, with specification parts holding
  only `use` statements, `implicit` statements, `public`/`private`
  accessibility statements, `interface` blocks, derived-type definitions, and
  `::`-form type declarations; and bodies skipped by an explicit block-depth
  counter;
- free-form lexing: case-insensitive keywords and names, `!` comments,
  single/double-quoted literals with doubling escapes, `&` continuation
  (including the leading-`&` form and token gluing across a split), and `;`
  statement separators.

Fixed-form files and FPP-preprocessed files are outside the admitted set and
always have been: discovery already defers uppercase/`.fpp` forms, and this
frontend additionally refuses, whole-file, any admitted-suffix file that carries
a preprocessor line (`#…` at statement start), an `INCLUDE` statement, or a
decidable fixed-form signature (a non-continuation line whose columns 1–5 are
blank and whose column 6 is a continuation indicator — a digit or `+ - * /` —
or a `*` in column 1). The refusal is typed and named; it is an abstention, not
a parse. The column-6 rule can conservatively refuse a free-form labeled
statement indented by exactly five spaces (`     100 continue`); that
understatement is chosen over a heuristic guess about source form.

The invariance argument mirrors ADR-0040 D1/D2: the admitted constructs lex and
nest identically under every free-form Fortran 2008+ compiler — the token
boundaries of `!` comments, quoted literals, `&` continuation, and `;`
separators do not move with compiler version, and none of the admitted shapes
changed between Fortran 90 and 2008 free form. What *would* move the parse —
preprocessor conditionals, `INCLUDE` text, fixed-form column semantics,
submodules, coarray `critical`-adjacent extensions — is refused rather than
read. The claim is bounded twice: to the admitted subset, and to the free-form
invariance set. Widening either needs a superseding decision.

### D2. The exact anchor

A code unit is emitted for exactly one shape, and every condition is required:

1. a `subroutine` statement at module scope (a module subprogram after the
   module's `contains`) or at file top level (an external subroutine), whose
   name folds case-insensitively to a `test_` prefix;
2. the statement has a non-empty dummy-argument list;
3. the first dummy argument is declared, in that subroutine's own specification
   part, by a `type(error_type)` type-spec carrying `intent(out)` and at most
   the additional `allocatable` attribute — the current upstream
   `test_interface` spells `type(error_type), allocatable, intent(out)` and this
   ADR pins the admitted attribute set to `intent(out)` (required) plus
   `allocatable` (accepted), in either order, matching the mission-pinned and
   upstream spellings;
4. the accessibility of `error_type` is **parse-proven in scope**: the scoping
   unit itself, or the enclosing module, holds a `use testdrive` statement that
   is a plain use (`use [::] testdrive`), or an `only:` list containing a bare
   `error_type` (not `x => error_type`). A rename list, an `only:` list without
   `error_type`, or `use, intrinsic :: testdrive` does not prove it. This
   mirrors ADR-0042's DESCRIPTION-declares-testthat identity fact, with the
   declaration living in the file instead of project metadata, and an unproven
   use with a test-shaped subroutine records a blocking
   `testdrive_use_not_proven` typed `UNKNOWN` instead of an anchor.

The anchor claims only that a test-drive-shaped test subroutine is declared with
the framework's error interface imported in scope. It claims nothing about
registration (`new_unittest` wiring), execution, outcomes, skips, or ordering —
those ride as standing non-blocking `UNKNOWN`s. Module accessibility
(`public`/`private`) deliberately does **not** affect the anchor: test-drive's
own documented pattern keeps test procedures module-private behind a public
`collect` function, so refusing `private` tests would refuse the framework's
canonical shape.

`function test_x`, interface-body declarations, internal procedures (a
`contains` section of a program, subroutine, or function), type-bound procedure
sections, `class(error_type)` polymorphic spellings, first dummies of any other
type, and renamed error bindings never anchor.

### D3. Typed refusals are whole-file

Hostile inputs refuse the whole file with a named, source-free typed
`UNKNOWN`, keeping only the module unit, mirroring the MATLAB lane's refusal
shape: preprocessor lines, `INCLUDE` statements, fixed-form signatures, any
construct outside the admitted subset (including `submodule`, `block data`,
`module procedure` prefixes, and a `module` nested inside a module — nesting is
budgeted to one), block depth past the bounded limit, and byte/unit resource
limits. The parser never recovers past a refusal and never resynchronizes.

### D4. Units, facts, and the obligation registry

The frontend emits one module unit per admitted file and one
`fortran_test_drive_subroutine` unit per admitted anchor, with source ranges
over the whole subroutine construct, plus a structural anchor fact on the fixed
`testdrive.test_subroutine` target. No subroutine name, module name, dummy
name, literal, or other repository identifier reaches a fact target, note,
assumption, or code-unit id; units are identified by kind and byte range.

Every typed `UNKNOWN` the frontend can emit is declared exactly once in a
lane-local obligation registry (the ADR-0020 gate 4 table, mirroring
`R_OBLIGATION_REGISTRY`): the blocking identity obligation
(`testdrive_use_not_proven`), the parse-boundary obligations (unadmitted
construct, preprocessor directive, include statement, fixed-form signature,
byte/depth/unit limits), and the standing residuals the bounded parse can never
discharge — cross-file `testdrive` module binding and `new_unittest`
registration — which ride on every admitted anchor as non-blocking `UNKNOWN`s.
No Fortran provider slot exists or is registered, and ADR-0034 forbids running
fpm, gfortran, flang, preprocessors, or repository code, so those residuals are
irreducible under current constraints.

### D5. No dependency; nothing runs

This ADR authorizes a hand-written bounded free-form lexer and parser in the
existing Rust core and nothing else. It adds no crate, grammar, toolchain, or
downloaded artifact, and it does not reserve the right to add one without a
superseding ADR. ADR-0034's execution prohibitions and Flang `NO_GO` verdict
remain in force. The `fpm.toml` inventory scope is unchanged.

## Alternatives considered

- **pFUnit first:** rejected in D0 — preprocessor-manufactured anchors inside a
  lane that refuses preprocessing.
- **Admit fixed-form by dual lexing:** rejected — fixed-form column semantics
  would double the invariance burden for a form fpm-era test suites rarely use;
  abstention is cheaper and honest.
- **Treat `private` tests as lookalikes:** rejected — it would refuse
  test-drive's own documented collect pattern (module-private tests behind a
  public collector).
- **Anchor on the `new_unittest("name", test_x)` registration call instead:**
  rejected for the first slice — the registration string plus call shape is a
  second, independent anchor surface; the declaration-side interface anchor is
  the smaller exact claim. Registration stays a standing residual.
- **Project-context use proof (fpm dev-dependency gate) as a blocking
  precondition like R's DESCRIPTION:** rejected — the use statement is in-file
  parse evidence; adding a manifest precondition would block anchors on
  inventory this frontend can already prove locally. The fpm dependency row
  remains inventory only.

## Consequences

- Fortran gains a bounded frontend, owned units, typed `UNKNOWN`s, and one
  exact family `fortran.testdrive.test_subroutine` gated at support three.
  Wiring the family through the shared application/persistence claim pipeline
  is the integration step this lane's ADR feeds, per ADR-0020 D2 gate 9.
- Free-form `.f90`-family bytes may cross the source boundary; fixed-form and
  preprocessed files never do.
- ADR-0034's discovery suffix set, fpm inventory contract, Flang `NO_GO`, and
  every execution prohibition remain in force.
- Widening the admitted attribute set, admitting fixed form or preprocessed
  files, adding a second anchor surface (registrations, parameterised tests),
  or claiming registration semantics each require a superseding ADR.
