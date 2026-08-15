# ADR-0046: Bounded MATLAB `matlab.unittest` frontend

- Status: Accepted
- Date: 2026-08-15
- Scope: MATLAB in ADR-0020; admits a family-bearing frontend over one
  class-based test shape
- Refines: ADR-0037 (authorizes the source unit it declined to authorize)
- Related: ADR-0045, ADR-0044, ADR-0043, ADR-0042, ADR-0040, ADR-0019,
  `docs/reports/language-support/matlab-completion-review.md`,
  `docs/plans/multi-language-expansion-plan.md`

Primary specifications this ADR reads as authority. Each quotation below was
read from the cited page.

- MathWorks, "Command vs. Function Syntax":
  <https://www.mathworks.com/help/matlab/matlab_prog/command-vs-function-syntax.html>.
  "With command syntax, MATLAB passes all inputs as character vectors (that is,
  as if they were enclosed in single quotation marks)." Function-versus-variable
  status is determined from "syntactic rules, the current workspace, and path".
  The page documents a partial syntactic tiebreak — "Space after an identifier,
  but not after a potential operator, implies a function call using command
  syntax", while "Spaces on both sides of a potential operator, or no spaces on
  either side of the operator, imply an operation on variables" — and that
  tiebreak covers operator-adjacent forms only.
- MathWorks, `classdef` reference:
  <https://www.mathworks.com/help/matlab/ref/classdef.html>. "Only blank lines
  and comments can precede `classdef`."
- MathWorks, "Abstract Classes and Class Members":
  <https://www.mathworks.com/help/matlab/matlab_oop/abstract-classes-and-interfaces.html>.
  "Abstract methods have no implementation in the abstract class", and "Do not
  use a `function...end` block to define an abstract method, use only the method
  signature."
- GNU Octave manual, "Single Line Comments":
  <https://docs.octave.org/latest/Single-Line-Comments.html>. "In the Octave
  language, a comment starts with either the sharp sign character, '#', or the
  percent symbol '%' and continues to the end of the line." MATLAB admits only
  the second.
- GNU Octave manual, "Escape Sequences in String Constants":
  <https://docs.octave.org/latest/Escape-Sequences-in-String-Constants.html>.
  Backslash introduces an escape sequence inside a double-quoted string, and
  "In single-quoted strings, backslash is not a special character." MATLAB
  treats backslash literally in both.

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

### The first version of this ADR was wrong about command syntax, and that correction is also the argument

MATLAB's genuinely undecidable construct is a different one. **Command syntax
cannot be distinguished from an expression without the workspace**: `a -1` is a
subtraction when `a` is a variable and the call `a('-1')` when `a` is a
function. MathWorks says so directly — function-versus-variable status comes
from "syntactic rules, the current workspace, and path" — and this frontend has
the first of those three and neither of the others.

The first version of this ADR named that ambiguity and then dismissed it:
"command arguments are unquoted, so they never change what is a comment or a
string, and the admitted anchor is a declaration shape rather than a statement."
That reasoning is **retracted**. It is sound about comments and strings and it
answers the wrong question. A line scanner only had to know which bytes were
comment or string; a parser has to decide where each block closes, and command
syntax reaches that decision directly, because a command argument is an
*unquoted word*:

- `end` can sit in argument position. Under the command reading it is the
  character vector `'end'`; under the expression reading it closes the enclosing
  block. `dbstop if error` is the everyday proof that reserved words do appear
  as command arguments.
- A bracket can sit in argument position. Under the command reading it is one
  character of the argument; under the expression reading it opens or closes a
  group, and the group can run past the end of the statement.

In both cases the two readings of one statement disagree about block structure,
and nothing in the file selects between them. D4b bounds the claim accordingly:
where the readings agree the file is parsed, and where they disagree the file
abstains and says so. This is the same move ADR-0040 makes for SQL — admit only
what is invariant across the thing that cannot be determined — and the same move
ADR-0044 D4a makes for Delphi's conditional compilation.

## Decision

### D1. Admission is `.m` files, and the structural parse is classdef-first

MATLAB defines no test file set: `matlab.unittest` discovers tests by class
shape wherever the file is on the path. Narrowing by filename would invent a
convention and present it as evidence, so every discovered `.m` file is decoded
and the blast radius is bounded by what the frontend emits, in D3.

The **structural parse**, however, runs only on a file whose first construct is
`classdef`. MathWorks requires exactly that placement — "Only blank lines and
comments can precede `classdef`" — so a file that begins any other way cannot
contain the admitted anchor. The restriction is not a convenience: MATLAB
function files may omit their terminating `end`s entirely, and a parser that
assumed `function … end` would report a degraded parse over most ordinary `.m`
files while proving nothing. Inside a `classdef` the end-less form does not
arise. A file that is not classdef-first yields its module unit, no structural
claim, and no diagnostic — nothing was attempted, so nothing degraded.

The parse also stops at the `classdef`'s terminating `end`. Text after it —
local functions, in practice — receives no structural claim, which keeps the
end-less function form out of the parser's way there too.

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
its superclass list. A `methods` block carries the bare `Test` attribute. A
`function` is declared directly inside that block.

MATLAB is case-sensitive, unlike the three languages that preceded this lane, so
keyword and attribute matching is case-sensitive.

Out of scope and named rather than left to inference: function-based tests
(`tests = functiontests(localfunctions)`), script-based tests, `TestClassSetup`
and `TestMethodSetup` blocks, `ParameterCombination` and parameterised
properties, `TestTags` filtering, inherited test classes, and Octave
compatibility — ADR-0037's MATLAB/Octave separation is untouched, and this
anchor claims MATLAB only because `matlab.unittest.TestCase` is a MATLAB name.

### D2a. The declared subset the parser admits

The frontend is a recursive-descent parser over the grammar below, not a line
scanner. Each nonterminal accepts or refuses; there is no error recovery and no
resynchronisation.

```
file        := comment* classdef_block        -- anything else: no structural claim
classdef    := 'classdef' attrs? name ('<' base ('&' base)*)? EOS
               classdef_member* 'end'
member      := methods_block
             | ('properties' | 'events' | 'enumeration') attrs? EOS opaque* 'end'
methods     := 'methods' attrs? EOS (function_def | signature)* 'end'
function    := 'function' (out_spec '=')? name ('.' name)? ('(' params ')')? EOS
               statement* 'end'
signature   := <tokens to EOS, no block keyword>          -- declared elsewhere
statement   := 'end'-terminated block | continuation header | opaque statement
block       := ('if'|'for'|'while'|'switch'|'try'|'parfor'|'spmd'|'function') …'end'
             | 'arguments' …'end'                         -- leading position only
continuation:= 'elseif' | 'else' | 'case' | 'otherwise' | 'catch'
EOS         := ';' | ',' | end of line, all outside brackets
```

Three properties of that grammar carry weight.

**Expressions are deliberately unparsed.** An opaque statement is consumed as a
token run up to its `EOS`; the parser tracks brackets and block keywords and
reads nothing else. This is what lets D5 say the frontend never interprets a
statement, and it is exactly what D4b has to protect.

**Block keywords are contextual, not a flat list.** `methods`, `properties`,
`events`, and `enumeration` open a block only in a `classdef` body, and
`arguments` only as a function's leading block. A flat keyword list opens a
phantom block for `methods = getMethods(x);` inside a test body and swallows the
rest of the class.

**A `classdef` body admits four member kinds and nothing else.** An unrecognised
one — including a block kind a later MATLAB release adds — is outside the
declared subset, so the extents of the blocks around it are unproven and the
file abstains. This is the release-difference answer: the parser does not guess
at a keyword it does not know, and it does not silently mis-nest.

### D2b. Abstractness is not read for the class, and is read for the block

The `classdef` attribute list is parsed, because the header grammar has to reach
the class name past it, and `Abstract` is deliberately **not** consulted. The
methods of an abstract `matlab.unittest.TestCase` therefore anchor even though
the framework runs them only through a concrete subclass. This is stated rather
than fixed: the declaration is real and the methods are real tests of the
subclasses, and reading the attribute to exclude them would suppress evidence
that exists. What the anchor does not claim is that the declaring class is
itself runnable. This carries forward the decision recorded as D2a in the first
version of this ADR.

An abstract **methods block** is different and is read. Its members are bare
signatures with no body and no `end`, so there is no implementation to anchor.
A `methods (Abstract, Test)` block therefore yields no anchor and records
`InsufficientSupport` under `matlab_abstract_test_methods`.

`methods (Test = <expr>)` also yields no anchor. Deciding whether the block is a
test block would mean evaluating the expression, and this frontend evaluates
nothing; the abstention is recorded under `matlab_test_attribute_value`.

### D3. Only admitted declarations become units

The frontend emits a module unit per decoded `.m` file, one unit for an admitted
test class, and one unit per admitted test method. Ordinary MATLAB code produces
no unit, so the absence of a unit is not evidence that a file has no code.

### D4. The lexer is comment-, string-, and tick-aware

`%` comments, `%{ … %}` block comments, `'…'` character arrays with `''`
doubling, `"…"` strings with `""` doubling, transposes, and the text after a
`...` continuation never contribute to an anchor. The tick rule in the context
section is implemented exactly as stated.

This is a decision because the repository has shipped the opposite defect four
times.

The `...` case is called out separately because it is not obviously a comment:
MATLAB ignores everything after an ellipsis to end of line, so a bare `end`
written there is prose, not a block terminator, and counting it would close the
`methods (Test)` block early. A `...` also suppresses the line break as a
statement terminator, which is what makes the continued statement one statement
to the parser.

The frontend reports a degraded parse on three decidable well-formedness
violations: an unclosed `%{`, a character array or string left open at end of
line, and a block structure that does not close. Without that signal a malformed
file silently yields fewer anchors — indistinguishable from a file that simply
has fewer declarations. The units already found are kept, because each one rests
only on headers that precede the failure point; the diagnostic states that the
ones not found prove nothing. Keeping a proven prefix is not error recovery: the
parser stops at the failure and never resumes.

### D4b. Command syntax bounds the claim

A statement is **possibly command syntax** when an unreserved identifier at
statement start is followed by a blank and then by something that is neither an
assignment `=` nor a call's `(`. The test over-approximates on purpose:
over-detection only adds a check, while under-detection would miscount an `end`.

For such a statement the parser compares what the two readings contribute to
block structure:

- If the statement's tokens contain no `end` outside brackets and its brackets
  balance within the statement, the readings **agree**. Both consume the same
  span, neither opens or closes a block, and the ambiguity is irrelevant to
  everything this frontend claims. `hold on`, `a -1`, and `dbstop if error` are
  all in this class — `if` is not at statement start, so no reading opens a
  block from it — and the file parses normally.
- If they contain an unbracketed `end`, or the brackets do not balance, the
  readings **disagree** about where a block closes. The file's block extents are
  unproven, and it abstains: module unit, `ConflictingFacts` under
  `matlab_block_structure`, a degraded diagnostic, and **no anchor at all**.

Abstention is file-level rather than statement-level. A diverged reading leaves
every later boundary in the file unproven — including where the `methods (Test)`
block ends — and reporting the anchors that happened to precede it would be
asserting a block extent the file does not fix. The application-side classifier
records the same conclusion so that "may this file support a family" has one
authoritative answer rather than depending on the absence of a fact.

Three consequences are stated rather than left to inference.

Command syntax occurs only at statement positions inside function bodies, never
in a `classdef` or `methods` body, where the grammar admits declarations only;
the exposure is confined to the opaque statement regions.

MathWorks' documented tiebreak does not rescue the divergent cases, and the
reason is worth recording so a later reader does not reach for it. The rule is
about *potential operators* — "Space after an identifier, but not after a
potential operator, implies a function call using command syntax" — and both
divergence classes here are about a bare word (`end`) and a bare bracket, which
the rule does not address. The prior question of whether the identifier names a
variable at all still comes from the workspace and path.

The abstention is nonetheless wider than strict undecidability, and the ADR says
so rather than dressing over-caution as necessity. `x end` and `x off]` have no
well-formed expression reading, so on a valid MATLAB file the command reading is
forced and `end` really is an argument. Establishing that requires deciding
whether an expression is well-formed, which D2a's grammar deliberately cannot
do: expressions are opaque token runs. So the frontend abstains where it cannot
distinguish, understating support rather than asserting a block extent it has
not established. Narrowing this with an expression grammar is a superseding
decision, not an implementation detail.

### D4c. MATLAB/Octave lexical invariance bounds the claim

ADR-0037 separates MATLAB from Octave, and `.m` is shared between them. This
frontend does not detect which one a file targets and does not need to, on the
same footing as ADR-0040: it admits only constructs the two read identically,
and abstains on constructs they read differently. The refused set is scoped to
what could move a token boundary or a block boundary, because nothing else can
change what is claimed:

- `#` and `#{` — Octave comments; MATLAB has no `#`, so the two disagree about
  where the line's code ends.
- A backslash inside a double-quoted string — Octave honours escapes and MATLAB
  does not, so `"a\"b"` is one string under Octave and a string followed by an
  unterminated one under MATLAB.
- `endfunction`, `endif`, `endfor`, `endwhile`, `endswitch`, `endparfor`,
  `endclassdef`, `endmethods`, `endproperties`, `endenumeration`, `endevents`,
  `end_try_catch`, `end_unwind_protect`, `unwind_protect`, and
  `unwind_protect_cleanup` — Octave block words that MATLAB reads as ordinary
  identifiers.
- A trailing `\` before a line break — an Octave continuation and a MATLAB
  syntax error.
- `!` — a shell escape in MATLAB, whose rest-of-line is a command, and logical
  negation in Octave.

Any of these makes the file abstain with `ConflictingFacts` under
`matlab_dialect_invariance`. The divergence is named by bounded class; the
divergent text is never copied into a fact target, note, assumption, or
diagnostic.

Octave's `do … until` is **not** detected, and is recorded here as a residual
risk rather than claimed: `do` and `until` are ordinary identifiers in MATLAB,
and refusing them would abstain on legal MATLAB that merely names a variable
`do`. `++`, `--`, `+=`, and `!=`-style divergences are deliberately out of the
set: they change what a statement means, and this frontend reads no statement
meaning, so both readings consume the same span and move no block boundary.

### D5. What the anchor claims

It claims that a class derived from `matlab.unittest.TestCase` declares a method
inside a `Test` methods block, which is how the framework discovers a test. It
claims nothing about execution, outcomes, ordering, setup, teardown, tags,
parameters, inheritance, or path resolution, and it never interprets a
statement. `eval`, `feval`, and every other dynamic construction path are
outside the claim in both directions: a class brought into being at runtime is
not a declaration and is never anchored, and a declared class whose methods are
invoked dynamically is still declared. No MATLAB, Octave, project, child
process, or network operation runs; ADR-0037's prohibitions are carried forward.

### D6. No dependency

No Rust crate, no grammar, no toolchain, no downloaded artifact. The lexer and
parser are hand-written Rust.

## Alternatives considered

- Refuse MATLAB on the overloaded tick: rejected because ADR-0045 establishes
  that the same overload is decidable by a local rule, and the plan's prediction
  was made before that analysis existed.
- Keep the line scanner and the argument that command syntax never reaches it:
  rejected, and retracted above. The argument is true about comments and strings
  and silent about block structure, which is the decision a parser has to make.
- Parse expressions so the forced command readings can be decided: rejected for
  this slice. It would widen the claim surface from a declaration shape to
  statement semantics, and the abstention it would remove is rare. It remains
  the obvious next decision if the abstention rate ever matters.
- Abstain per statement rather than per file on a command-syntax divergence:
  rejected because the divergence unproves every later block boundary in the
  file, including the extent of the `methods (Test)` block the earlier anchors
  sit in.
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
- A file carrying a command-syntax divergence, an Octave-only lexeme, or an
  unadmitted `classdef` member produces no anchor. Support is understated by
  design, and the completion review must not read the resulting absence as
  evidence that the file declares no tests.
- Widening the admitted test shape, the declared subset, or the claim surface
  requires a superseding ADR.
