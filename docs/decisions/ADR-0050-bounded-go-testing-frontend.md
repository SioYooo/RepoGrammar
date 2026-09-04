# ADR-0050: Bounded Go testing frontend

- Status: Accepted
- Date: 2026-09-04
- Scope: Go in ADR-0020; admits a family-bearing bounded parser over the
  `_test.go` test-function declaration shapes
- Refines: ADR-0021 (supersedes its evidence ladder's family closure for
  `go.testing.test_function` and its D3 name narrowing; leaves every worker
  and provider clause in force), ADR-0041 (supersedes the correction's
  auxiliary-only status for this family; the scanner itself stays as
  historical auxiliary evidence)
- Related: ADR-0042, ADR-0043, ADR-0044, ADR-0045, ADR-0046 (the bounded
  hand-written-parser precedent this decision follows), ADR-0019,
  `docs/reports/language-support/go-completion-review.md`,
  `docs/plans/multi-language-expansion-plan.md`

Primary specifications this ADR reads as authority. Each was checked for the
claim it is cited for.

- Go specification, "Semicolons": <https://go.dev/ref/spec#Semicolons>. A
  semicolon is inserted at end of line when the line's final token is an
  identifier; an integer, floating-point, imaginary, rune, or string literal;
  one of `break`, `continue`, `fallthrough`, `return`; one of `++`, `--`, `)`,
  `]`, `}`. This rule is reproduced exactly, because it decides declaration
  boundaries.
- `cmd/go` documentation, "Testing functions":
  <https://pkg.go.dev/cmd/go#hdr-Testing_functions>, and the `testing` package
  (<https://pkg.go.dev/testing>) for the `TestXxx(t *testing.T)`,
  `BenchmarkXxx(b *testing.B)`, and `FuzzXxx(f *testing.F)` shapes and the
  `_test.go` filename rule.
- The `cmd/go` test-loader rule that a name is a test when it is the bare
  prefix or the prefix plus a suffix whose first character is not a lowercase
  letter (`isTest` in `cmd/go/internal/load/test.go`), which this ADR mirrors
  exactly and cites as the definition of "a function `go test` would
  recognize".
- Go 1 compatibility promise: <https://go.dev/doc/go1compat>. "It is intended
  that programs written to the Go 1 specification will continue to compile and
  run correctly, unchanged, over the lifetime of that specification." The
  grammar of the admitted subset is fixed by this promise for every Go 1.x
  release; Go 2 does not exist as a released grammar that any repository
  compiles against.

## Context

Go source is discovered and, since ADR-0041, exactly one filename class is
decoded: `*_test.go`, because `go test` compiles only those files as tests and
the filename is therefore part of the anchor's meaning. ADR-0041 admitted a
string- and comment-aware scanner over one declaration shape, and its own
correction then demoted that scanner to auxiliary evidence: ADR-0021's
evidence ladder item 4 forbids text or regex matching *for the claim*, and its
item 1 names a pinned standard-library worker as primary evidence, which the
program's zero-external-dependency constraint puts out of reach. Under that
constraint the `go.testing.test_function` family was closed, not merely
unstarted.

ADR-0042 through ADR-0046 changed the landscape. R, VB.NET, Delphi, Ada, and
MATLAB each admit a hand-written recursive-descent parser over a declared
subset as their family-bearing frontend, in the existing Rust core, with no
dependency and no execution. For each of those languages the no-text-matching
requirement was discharged the same way: by proving the anchor from a real
parse over a declared subset, with whole-file abstention outside it.

### The evidence ladder is read first, as ADR-0042 teaches

ADR-0021 item 4's prohibition is on text or regex matching *for the claim*.
ADR-0042 D4 records why a recursive-descent parser over a declared subset is
not that: strings, runes, and comments are literals in the token stream, so
they cannot contribute to an anchor *by construction* rather than by an
awareness rule, and the anchor becomes a question about a parsed declaration
rather than about byte positions. The defect class this repository has shipped
four times — a Rust attribute matched inside a function body, a TS/JS runner
matched inside a member call, a Go declaration matched inside a string, and
both branches of a Delphi `{$IFDEF}` — is closed by asking "is this a
declaration with this shape" instead of "does this text appear".

This ADR therefore supersedes ADR-0021's family closure **for
`go.testing.test_function` only**: the ladder's forbidden-evidence clause is
discharged by a real recursive-descent parse over the subset declared below,
on the ADR-0042..0046 precedent. It does **not** supersede anything else in
ADR-0021: the pinned `go/parser` worker route, its sandbox contract, its
execution prohibitions, and its resource ceilings all remain in force and
remain the authority for any future semantic fact this frontend cannot
produce. No semantic worker, provider, or dependency is authorized here.

### Why the scanner is not enough, in Go's own terms

The scanner asks whether the text `func TestXxx(t *testing.T)` appears at
brace depth zero outside strings and comments. A parser asks whether the
file's package-level declaration list contains a plain function whose parsed
single parameter is a pointer to a qualified type. The difference shows in the
same places it showed for R: the scanner cannot cheaply distinguish a
parameter list from a result list, cannot apply Go's own semicolon-insertion
line discipline, cannot validate that what it skipped was well formed, and —
the reason ADR-0021 closed the family — cannot state a fidelity boundary,
because it refuses nothing. A bounded parser refuses, and the refusal is the
fidelity boundary.

## Decision

### D1. Admission stays `*_test.go` only

Discovery keeps every `.go` path as inventory. Only files whose normalized
repository-relative name ends in `_test.go` are decoded and parsed. Every
other `.go` byte remains unread. This is ADR-0041 D2 unchanged: the filename
is part of the anchor's meaning, and ADR-0021's GOOS/GOARCH filename-suffix
and build-tag selection abstention is unaffected — the parser may read a
platform-suffixed test file, and the selection of whether that file is
compiled remains the recorded non-evaluated `go_build_constraint`-class
uncertainty, never a guessed selection.

### D2. The exact anchor, and its admitted variation

The family is `go.testing.test_function`, and its admitted variation is the
three forms `go test` itself recognizes:

```go
func TestXxx(t *testing.T)
func BenchmarkXxx(b *testing.B)
func FuzzXxx(f *testing.F)
```

Every condition is required, and every one is read off the parse:

- The declaration is a top-level `func` declaration: not a method (no
  receiver) and not a function literal. Function literals cannot appear at
  package level in valid Go, and a top-level token sequence that begins no
  declaration refuses the file.
- The function declares no type parameters. A `[` after the name or a bracket
  in a method receiver refuses the whole file (D4), so a generic function can
  never anchor and neither can a method on a type whose receiver spelling is
  undecidable.
- The name follows the `go test` rule mirrored exactly: the bare prefix
  (`Test`, `Benchmark`, `Fuzz`) or the prefix plus a suffix whose first
  character is not a lowercase letter. `TestFoo`, `Test_foo`, `Test123`, and
  bare `Test` anchor; `Testify` and `Testx` do not. This supersedes ADR-0021
  D3's narrower exported-uppercase narrowing, which existed to keep a text
  scanner from guessing positives; the narrowing's cost was that `Test`,
  `Test_underscore`, and `Test123` — all run by `go test` — were non-claims.
  A parse-proven declaration does not need that extra caution, and mirroring
  the toolchain's own predicate is the honest version of the claim "a
  function `go test` would recognize". `TestMain` remains a named non-claim:
  it is the special entrypoint form whose recognized signatures are a
  toolchain detail this ADR does not guess at, and its conventional
  `*testing.M` parameter fails the type-letter match anyway.
- There is exactly one parameter by Go AST field-name semantics: an optional
  parameter name (an identifier, `_` included) followed by a pointer to a
  qualified type. A field with multiple names (`t, u *testing.T`), a variadic
  parameter, extra parameters, or any other spelling is not the anchor.
- The parameter type is exactly `*` + qualifier + `.` + the family's type
  letter — `T` for `Test`, `B` for `Benchmark`, `F` for `Fuzz` — where the
  qualifier is the file's own non-blank, non-dot import binding of the exact
  standard-library import path `"testing"`, compared after decoding the
  import-path string literal. A benchmark spelled with `*testing.T` is not a
  benchmark and anchors nothing.
- There is no result list. A function with results is not a test shape.

The qualifier rule carries ADR-0041 D3's import resolution forward unchanged:
the default and explicit-alias bindings normalize to the same import identity
(`import tt "testing"` makes `*tt.T` the anchor spelling and `*testing.T`
meaningless in that file), and dot and blank imports defeat resolution rather
than bending it — see D5.

Standard-library identity is free in a way R's was not: an import path is
either a standard-library path or a module path, and no module can occupy the
path `testing`. The exact-path comparison therefore proves the standard
library without a package-graph resolution, and no `testing` lookalike module
can spoof it.

### D3. Only admitted declarations become family-bearing units

The frontend emits a module unit per decoded `_test.go` file and one unit per
parsed top-level function or method declaration. A unit is
`go_test_function`-kinded only when D2 admits it; every other declaration is
an ordinary `go_function` unit, so the absence of a test unit is not evidence
that a file has no code. Anchor facts carry `Structural` certainty and the
fixed target `go.testing.test_function`; derived family support is produced by
the shared family layer from those anchors, never by this frontend.

### D4. The declared subset, and whole-file abstention outside it

The parser is a hand-written lexer and recursive-descent parser over the
following subset, modeled on the ADR-0042/0045 discipline. **The whole file
parses or the whole file abstains**: a refusal yields the module unit, one
typed `UNKNOWN` naming the refusal class, no function units, and no anchor.
There is no error recovery and no resynchronization, because a recovered
anchor cannot be told apart from a real one. This matches ADR-0021's own
stricter stance that any parse error that can affect declaration identity
blocks every confident anchor in the file.

**Lexical.** Line comments and non-nesting block comments; interpreted string
literals with the full Go escape set, each escape validated (`\a \b \f \n \r
\t \v \\ \' \"`, octal `\ooo` of exactly three digits, `\xHH`, `\uXXXX`,
`\UXXXXXXXX`; anything else refuses the file); raw backtick strings with no
escapes; rune literals with validated escapes; the complete Go numeral
grammar including `0x`/`0X` hex integers and `p`-exponent hex floats,
`0b`/`0o`, legacy octal, decimal floats and exponents, the imaginary suffix,
and digit-separator underscores validated to lie between digits; ASCII
identifiers `[A-Za-z_][A-Za-z0-9_]*`; every Go keyword and every Go operator
and delimiter, longest-match. `import "C"` in any spelling refuses the file
under the cgo boundary. Non-ASCII bytes are welcome in strings, runes, raw
strings, and comments, and begin no token in code position: identifiers in
this subset are ASCII, so a Unicode identifier refuses the file rather than
being classified by a Unicode-table approximation. Raw-string import paths
are outside the subset (the toolchain requires the interpreted form).

**Semicolons.** Go's insertion rule is reproduced exactly, because it decides
declaration boundaries: a newline inserts a semicolon exactly when the line's
final token is an identifier, a numeral, a rune or string literal, `break`,
`continue`, `fallthrough`, `return`, `++`, `--`, `)`, `]`, or `}`. Inside the
strict regions — parameter lists, receiver lists, result groups, import
specifications — a semicolon is always a syntax error in Go, and it refuses
the file here too, which is exactly the language's own line discipline:
`func f(a int,` continues across the newline and `func f(a int` without the
comma does not. A line comment does not shield the newline that follows it.

**Grammatical.** A package clause; import declarations in both single and
grouped forms with `.`, `_`, and identifier aliases and decoded exact-path
comparison; top-level `func` declarations with receivers, parameters, and
results, requiring a body (body-less assembly-forwarding declarations are
outside the subset and refuse the file); and `type`, `var`, and `const`
declarations, which are consumed to their terminator rather than parsed,
because they carry no anchor. Function bodies are consumed by bracket-stack
skipping over the token stream — strings, runes, and comments are already
tokens, so nothing inside a body can move a boundary — and everything inside
a body is unread semantics: subtests, helpers, literals, and prose included.

**Refused**, each because the construct's identity or boundaries are not
decidable from source alone or are outside the declared subset:

- A type-parameter list after a function name, or a bracket in a method
  receiver (array type or instantiated generic: undecidable without type
  information).
- `import "C"`, in any alias spelling: the cgo boundary.
- Two imports binding the same decidable local name (two `testing` imports,
  or a repeated explicit alias). Default local names of other paths come from
  the imported package's clause, which no source-local read can see, so only
  the decidable classes are checked.
- A top-level token sequence that begins no declaration — including a
  function literal, which cannot appear at package level in valid Go.
- An unterminated string, raw string, rune, or block comment; an invalid
  escape; brackets that do not balance or a closer that does not match its
  opener; a missing package clause; a missing function body; a raw-string
  import path.
- Nesting past a fixed depth, input past the existing byte ceiling, or
  declarations and facts past fixed unit and fact ceilings. Untrusted input
  must abstain rather than exhaust the stack.

**Tiered diagnostics**, as ADR-0045 tiers them: a malformed file (open
literal or comment, invalid escape, unbalanced brackets, duplicate binding)
reports a degraded parse in addition to the typed `UNKNOWN`; a construct that
is merely outside the declared subset reports the typed `UNKNOWN` only,
because most real Go is outside any bounded subset and an ordinary
repository must not raise an operator warning on every file. No refusal
carries source text: the vocabulary is a fixed enumeration, and the offending
token, identifier, literal, and line never reach a diagnostic, fact note, or
assumption, because those surfaces feed `index --json`, `unknowns`, and the
MCP readiness payloads.

### D4a. The invariance argument

ADR-0020 gate 2 requires that the admitted parse not depend on anything
RepoGrammar cannot determine. Go's version of this argument is the strongest
of the program, and it is the reason a bounded hand-written parser is
unusually safe here: **the Go 1 compatibility promise fixes the grammar of
the admitted subset for every Go 1.x release.** Semicolon insertion,
identifier and literal syntax, declaration structure, and the `_test.go`
filename rule have been syntactically invariant since Go 1.0, and the promise
is explicit that Go 1 programs continue to compile unchanged. There is no
preprocessor, no conditional compilation *in the language* (build constraints
select files, and they are recorded rather than evaluated), no macro layer,
no dialect or mode selector, and no include mechanism. Nothing outside the
file's own bytes selects how those bytes parse.

The constructs the subset refuses cover the residual axes. Generics were
added in Go 1.18 as new syntax that was previously a syntax error — pure
accretion — and they are refused outright, so no Go version reads an
admitted file into a different tree. Build constraints and GOOS/GOARCH
selection decide whether a file is *compiled*, never how it parses, and they
stay typed uncertainties. Unicode identifiers are refused rather than
classified, which is the one place a Unicode-table approximation could
diverge from `go/parser`'s `unicode.IsLetter`. The toolchain's own test-name
rule is mirrored exactly, including the bare-prefix form, so no name-shape
judgment depends on an unproven convention.

Two deliberate divergences are stated rather than left to inference, both in
the safe direction. `type`/`var`/`const` declarations are consumed, not
parsed, so a malformed declaration interior is admitted where `go/parser`
would error (the consumption still requires balanced brackets and a
terminator); and rune-literal contents are boundary-checked but not
single-value-validated. Each can only lose an anchor after it, never invent
one, and the anchor-bearing `func` and `import` paths apply the strict rules.

### D5. What the anchor claims, and what stays UNKNOWN

The anchor claims that a file `go test` compiles as a test file declares a
function the toolchain's own name and signature rules recognize as a test,
benchmark, or fuzz target. It claims nothing else. Every typed `UNKNOWN` the
frontend emits is declared exactly once in the lane's ADR-0020 gate 4
obligation registry (`GO_OBLIGATION_REGISTRY` in
`src/rust/adapters/parsing/go/testing.rs`), with its claim scope, its
blocking impact, and its provider-fallback policy.

The blocking obligations are the two identity forms ADR-0021 D3 and
ADR-0041 D3 already established: a dot or blank `testing` import binds no
qualified name, so no test signature in that file resolves, and the file
anchors nothing. Build constraints in the header record the established
non-blocking `go_build_constraint` subclaim: the declarations are what they
are whether or not the target platform compiles the file. The refusals of D4
record their own classes under `go_test_parse`, `go_cgo_boundary`, or the
receiver-undecidability claim. Two standing residuals ride on every admitted
anchor and never block: test execution semantics (outcomes, skips,
parallelism, `t.Run` subtest identity — runtime state) and package identity
(no module, package-graph, or cross-file resolution is performed). Each
records which future mechanism could discharge it, and both name the ADR-0021
worker route.

Non-claims worth naming rather than leaving to inference: `TestMain` in its
special role; subtests (a `t.Run` body is skipped semantics, and its inner
`func(t *testing.T)` literal never anchors); helper functions; example
functions (`ExampleXxx`, a different family not admitted here); test
execution, outcomes, and coverage; generated files; and the module graph.

### D6. No dependency, no execution, no worker authorization

No Rust crate, no grammar, no toolchain, no downloaded artifact. The lexer
and parser are hand-written Rust in the existing core. Nothing runs: no `go`
command, no `go/packages`, `go list`, gopls, cgo, compiler, linker,
generator, test binary, or network operation. ADR-0021's containment
obligation remains unreached rather than waived, and its D2 worker route —
the pinned `go/parser`/`go/token`/bounded `go/types`/`go/build/constraint`
worker behind the versioned semantic-worker boundary with its fail-closed
sandbox — remains future provider work, unmodified and unauthorized by this
ADR. When that worker lands, it can discharge the recorded residuals (package
identity, cgo semantics, receiver undecidability, build-environment
selection) and can cross-check this frontend's admitted anchors; until then
those residuals stay typed `UNKNOWN` rather than becoming claims.

### D7. The historical scanner

`src/rust/adapters/parsing/go/source.rs` stays in the tree as historical
auxiliary evidence per ADR-0041's correction, and this parser replaces it in
the dispatch path. Its `is_go_test_path` helper and engine constants remain
referenced by the application layer's inventory routing until the shared
registry integration repoints them at this module. Nothing new may dispatch
to the scanner.

## Alternatives considered

- Keep the scanner and leave the family closed: rejected. ADR-0042..0046
  established that a bounded recursive-descent parser over a declared subset
  discharges the no-text-matching requirement without a dependency, and Go's
  compatibility promise makes the invariance argument stronger than any lane
  that preceded it.
- Wait for the ADR-0021 worker: rejected as the *family's* prerequisite. The
  worker remains the route for semantic facts, but declaration identity over
  a compatibility-frozen grammar does not need `go/parser` to be exact, and
  the anchor claims only declaration identity.
- Admit a Tree-sitter Go grammar: rejected — ADR-0021 declined the
  dependency, and the zero-external-dependency constraint excludes it.
- Parse all `.go` files and filter tests inside the parser: rejected under
  D1, for ADR-0041 D2's reason.
- Anchor only `TestXxx(t *testing.T)` and defer Benchmark/Fuzz: rejected
  because they are one family's variation under the toolchain's own rules,
  and splitting them would understate support for every repository that
  benchmarks.
- Keep ADR-0021 D3's narrower name rule: rejected; see D2. The narrowing was
  scanner caution, and mirroring `cmd/go`'s own predicate is exact rather
  than approximate. The cost is that `Test`, `Test_1`, and `Test123` now
  anchor where ADR-0021 recorded non-claims; each is a function `go test`
  runs, and the ADR records the supersession openly.
- Parse function bodies to anchor subtests: rejected for this slice. The
  anchor is the declaration; subtest identity is runtime behavior and a
  recorded standing residual.
- Recover to the next declaration after a refusal: rejected, as ADR-0045
  argues for Ada. Computing the extent of what was refused without parsing
  it is the scanner assumption again, and once a construct is unparsed every
  later boundary is unproven.

## Consequences

- Go gains a family-bearing bounded parser, owned units, typed `UNKNOWN`s
  with a complete obligation registry, and family-eligible anchors for the
  `go.testing.test_function` family over its Test/Benchmark/Fuzz variation.
- `_test.go` bytes cross the source-store boundary as before; no other `.go`
  byte does. No execution, network, toolchain, or module resolution is
  added. Recursion over untrusted input is depth-bounded, and byte, unit,
  and fact ceilings abstain with typed `UNKNOWN`s.
- ADR-0021 remains in force for everything except the family closure this
  ADR supersedes; its worker route, sandbox, ceilings, and prohibitions are
  untouched and are the named fallback for the recorded residuals.
- ADR-0041's correction is superseded for this family only: the scanner's
  historical module stays as auxiliary evidence, and the dispatch path, the
  role registry, and the family evidence now come from the parser.
- Recall is lower than a tolerant parser's, and the failure direction is
  safe: a `_test.go` file using generics, cgo, Unicode identifiers, or any
  construct outside D4 contributes no anchor and records why. The completion
  review must not read the resulting absence as evidence that the file
  declares no tests.
- Widening the admitted shapes, the declared subset, or the claim surface
  requires a superseding ADR. In particular, admitting generic signatures,
  cgo files, `ExampleXxx`, or `TestMain` semantics, or re-widening the name
  rule beyond the toolchain's own predicate, is a decision rather than an
  implementation detail.

## Follow-up

- The shared family layer's Go arm (derived support from these anchors, the
  role-compatibility dispatch, and the engine-constant repoint away from the
  historical scanner) is main-session integration work on the shared
  registries, as it was for every landed lane.
- The ADR-0021 worker remains the route for package identity, cgo semantics,
  receiver undecidability, build-environment selection, and any type-level
  fact; none of those is claimed here.
- `ExampleXxx` functions are a natural later family candidate and need their
  own anchor argument.
