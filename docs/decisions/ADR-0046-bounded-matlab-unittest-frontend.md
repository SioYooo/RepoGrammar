# ADR-0046: Bounded MATLAB `matlab.unittest` frontend

- Status: Accepted
- Date: 2026-08-15
- Scope: MATLAB in ADR-0020; admits a family-bearing frontend over one
  class-based test shape
- Refines: ADR-0037 (authorizes the source unit it declined to authorize)
- Related: ADR-0045, ADR-0044, ADR-0043, ADR-0042, ADR-0019,
  `docs/reports/language-support/matlab-completion-review.md`,
  `docs/plans/multi-language-expansion-plan.md`

## Context

MATLAB source is discovered and never decoded. ADR-0037 keeps `.m` paths as
metadata-only discovery whose bytes are never passed onward, and forbids
invoking MATLAB or Octave, opening a project, and evaluating code.

### The evidence ladder was read first

ADR-0037 carries no forbidden-evidence list and no prohibition on text matching
for the claim. Its constraints are all about execution and project state, and
this frontend triggers none of them.

### The earlier prediction was wrong, and the correction is the argument

The expansion plan predicted MATLAB would be refused because `'` is both the
transpose operator and the character-array delimiter. ADR-0045 has since shown
that reasoning is not sufficient: Ada's tick is overloaded in exactly the same
way and is resolved by an exact local rule. "One character has two jobs" is a
description of a hazard, not a proof of undecidability, and a refusal built on
it would contradict the ADR shipped one lane earlier.

Re-examined on its own terms, MATLAB's tick is decidable too, and the rule is
**whitespace-sensitive** rather than token-sensitive: a tick that immediately
follows an identifier character, `)`, `]`, `}`, or another tick — with no
intervening blank — is a transpose; otherwise it opens a character array. `.'`
is the non-conjugate transpose and is read the same way. This is the rule the
language itself uses, and it is what makes `[a' b']` two transposes while
`[a 'b']` is a concatenation with a character array: the difference is which
side of the tick the blank falls on.

MATLAB's genuinely undecidable construct is a different one, and it is named
here so a later document cannot reach for the tick again. **Command syntax
cannot be distinguished from an expression without the workspace**: `a -1` is a
subtraction when `a` is a variable and the call `a('-1')` when `a` is a
function, and which one it is depends on runtime binding. That ambiguity is
real, but it does not reach this frontend: command arguments are unquoted, so
they never change what is a comment or a string, and the admitted anchor is a
declaration shape rather than a statement. The claim is bounded accordingly —
this frontend never interprets a statement.

## Decision

### D1. Admission is `.m` files

MATLAB defines no test file set: `matlab.unittest` discovers tests by class
shape wherever the file is on the path. Narrowing by filename would invent a
convention and present it as evidence, so every discovered `.m` file is decoded
and the blast radius is bounded by what the frontend emits, in D3.

### D2. The exact anchor

One shape is admitted:

```matlab
classdef CatalogTest < matlab.unittest.TestCase
    methods (Test)
        function loadsCatalog(testCase)
            testCase.verifyTrue(true);
        end
    end
end
```

Every condition is required. A `classdef` names `matlab.unittest.TestCase` in
its superclass list. A `methods` block carries the `Test` attribute. A
`function` is declared directly inside that block.

The extent of the `methods (Test)` block is found by keyword depth, which is
exact for MATLAB because every block keyword is closed by `end`: `if`, `for`,
`while`, `switch`, `try`, `parfor`, `spmd`, `arguments`, `function`, `methods`,
`properties`, `events`, `enumeration`, and `classdef` open, and a bare `end`
closes. An `end` used as an index (`a(end)`, `x(2:end)`) is inside brackets and
is not a block terminator, so depth is only counted at bracket depth zero.

MATLAB is case-sensitive, unlike the three languages that preceded this lane, so
keyword and attribute matching is case-sensitive.

Out of scope and named rather than left to inference: function-based tests
(`tests = functiontests(localfunctions)`), script-based tests, `TestClassSetup`
and `TestMethodSetup` blocks, `ParameterCombination` and parameterised
properties, `TestTags` filtering, abstract or inherited test classes, and Octave
compatibility — ADR-0037's MATLAB/Octave separation is untouched, and this
anchor claims MATLAB only because `matlab.unittest.TestCase` is a MATLAB name.

### D3. Only admitted declarations become units

The frontend emits a module unit per decoded `.m` file, one unit for an admitted
test class, and one unit per admitted test method. Ordinary MATLAB code produces
no unit, so the absence of a unit is not evidence that a file has no code.

### D4. The scanner is comment-, string-, and tick-aware

`%` comments, `%{ … %}` block comments, `'…'` character arrays with `''`
doubling, `"…"` strings with `""` doubling, and transposes never contribute to
an anchor. The tick rule in the context section is implemented exactly as
stated. This is a decision because the repository has shipped the opposite
defect four times.

### D5. What the anchor claims

It claims that a class derived from `matlab.unittest.TestCase` declares a method
inside a `Test` methods block, which is how the framework discovers a test. It
claims nothing about execution, outcomes, ordering, setup, teardown, tags,
parameters, inheritance, or path resolution, and it never interprets a
statement — the command-syntax ambiguity named above is out of the claim
surface, not resolved. No MATLAB, Octave, project, child process, or network
operation runs; ADR-0037's prohibitions are carried forward.

### D6. No dependency

No Rust crate, no grammar, no toolchain, no downloaded artifact.

## Alternatives considered

- Refuse MATLAB on the overloaded tick: rejected because ADR-0045 establishes
  that the same overload is decidable by a local rule, and the plan's prediction
  was made before that analysis existed.
- Anchor on the `methods (Test)` block itself: rejected because
  `matlab.unittest` runs methods, and a block with none is not a test.
- Bound the block positionally, as ADR-0044 does for Delphi: rejected because
  MATLAB closes every block with `end` and puts local helper functions after the
  `classdef`'s own `end`, so a positional rule would anchor helpers. Depth is
  exact here in a way it is not in Object Pascal, where `class` also appears in
  `class procedure` and `class var`.
- Support function-based tests as well: rejected for the first slice because
  `functiontests(localfunctions)` names its tests by naming convention over the
  file's local functions, which is a different anchor and a separate decision.

## Consequences

- MATLAB gains a frontend, owned units, typed `UNKNOWN`s, and one exact family
  for one class shape. It gains no project model and no provider.
- `.m` bytes cross the source-store boundary.
- ADR-0037 remains in force for discovery, package inventory, the MATLAB/Octave
  separation, limits, and every execution prohibition.
- The expansion plan's MATLAB row is corrected from a predicted refusal to a
  landed lane, and the prediction's reasoning is retracted in writing.
- Widening the admitted test shape or the claim surface requires a superseding
  ADR.
