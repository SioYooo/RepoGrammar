# ADR-0042: Bounded R testthat frontend

- Status: Accepted
- Date: 2026-08-15
- Scope: R in ADR-0020; admits a family-bearing frontend over one call shape
- Refines: ADR-0036 (authorizes the R source unit it declined to authorize)
- Related: ADR-0040 (the dialect-invariance precedent), ADR-0044 (the same
  conditional-reach rule for Delphi), ADR-0041, ADR-0019,
  `docs/reports/language-support/r-completion-review.md`,
  `docs/plans/multi-language-expansion-plan.md`

## Context

R source is discovered and never decoded. ADR-0036 states plainly that "no R
source code unit, IR, framework role, family, support, or readiness record is
authorized" and that a future frontend "requires a separate sandboxed and
source-backed ADR-0020 stage". This is that stage.

### Why this ADR checks the evidence ladder first

ADR-0041's first version authorized a Go family without reading ADR-0021's
evidence ladder, whose item 4 forbids text or regex matching *for the claim*.
That was a silent override of a standing decision, and it had to be corrected by
demoting the Go scanner to auxiliary evidence.

So this ADR states the check rather than assuming it. ADR-0036 has **no evidence
ladder and no prohibition on text matching**. Its restrictions are: R source is
inventory-only, no R source unit is authorized, and RepoGrammar "must never
invoke R, `parse`, `eval`, `source`, profiles, renv, package
installation/restoration, native code, tests, repository scripts, child
processes, or network access". Every one of those names *executing R*. A bounded
parser in the existing Rust core executes nothing, so the execution prohibitions
are unreached, and the authorization gap is what this ADR closes.

The same is true of "sandboxed": the word binds to routes that run something.
Nothing runs here.

The contrast with Go, Ruby, PHP, and Swift is deliberate and load-bearing, and
it is *not* about who owns the parser. Those ladders name a pinned real
frontend — `go/ast`, Prism, a sandboxed PHP parser, swift-syntax — as primary
evidence precisely because those languages need one: Go has build constraints,
Ruby has heredocs and a lexer that depends on parser state, PHP has a
preprocessor-like open/close mode, Swift has conditional compilation and macros.
Under the zero-external-dependency constraint those families are closed. R
carries no such ladder, and D4a shows it needs none: there is nothing outside a
file's own bytes that selects how those bytes parse.

### Why testthat

testthat is the dominant R testing framework and the one the R packages
convention builds around: `test_dir()` scans `tests/testthat/` and runs files
whose names begin with `test`. The path is not a style preference; it is the
set the runner reads.

### Why a RepoGrammar-owned parser, and not a scanner

The first version of this ADR authorized a byte scanner: string- and
comment-aware, but still answering "does this text sit at bracket depth zero?"
That is the wrong question, and it gave a wrong answer. A scanner cannot
distinguish

```r
test_that("runs", { ... })
if (interactive()) test_that("runs", { ... })
```

because in both the `test_that` token sits at parenthesis depth zero and brace
depth zero. The second is reached only when a condition RepoGrammar does not
evaluate holds, and the scanner anchored it anyway — the same defect
ADR-0044 D4a had to fix for Delphi's `{$IFDEF}`, arrived at by the same route.
It anchored `x <- test_that(...)`, `enabled && test_that(...)`, and a file that
had rebound `test_that` to something else entirely, for the same reason.

The frontend is therefore a hand-written recursive-descent parser over the
declared R subset in D4. It builds a real expression tree with R's own
precedence, associativity, and newline rule, and the anchor becomes a question
about that tree: *is this top-level expression itself the call?* Nothing in the
claim surface changes; what changes is that it is now established structurally
rather than positionally.

Primary specifications this ADR reads as authority:

- *R Language Definition* §10, "Parser", and §3, "Evaluation of expressions":
  <https://cran.r-project.org/doc/manuals/r-release/R-lang.html>, for the token
  set, the operator precedence table, and the rule that a name is looked up in
  the calling environment rather than bound by the parser.
- `?Syntax` in R base, for the precedence and associativity ordering this
  parser reproduces.
- *R News* for R 4.0.0 (raw string constants), 4.1.0 (`\(x)` lambda shorthand
  and the `|>` native pipe), and 4.2.0 (the `_` pipe placeholder), for the
  version boundaries D4a reasons over. The `=>` pipe bind is refused on the
  grounds that it parses only when the `_R_USE_PIPEBIND_` option is set, which
  holds regardless of which release introduced it, so no release is cited for
  it.

R is a good fit for a bounded owned parser for a reason that does not
generalize: its grammar is small, fixed, and configuration-free. There is no
preprocessor, no conditional compilation, no macro layer, no dialect selector,
no include mechanism, and no user-extensible syntax beyond `%any%` operators
that all lex and bind identically. D4a turns that into the invariance argument
ADR-0020 gate 2 asks for.

## Decision

### D1. Admission is `tests/testthat/test-*.R` only

Discovery keeps every `.R` and `.r` path as inventory. Only files whose
normalized repository-relative path has a `tests/testthat/` component and whose
basename begins `test-` and ends `.R` or `.r` are decoded. Every other R byte
remains unread.

The path is identity evidence, not convenience: it is exactly what testthat's
own runner scans. A `test_that(...)` call elsewhere in a repository is not a
testthat test being run, and this frontend does not read it.

### D2. The exact anchor

One shape is admitted:

```r
test_that("description", {
  ...
})
```

Every condition is required, and each is a property of the parsed tree.

The **top-level expression is itself the call**. Not a `test_that` token that
happens to sit at bracket depth zero: the parsed program's expression at that
position must be the call node. So none of these anchors, because in each the
top-level expression is something else that merely contains a call:

```r
if (interactive()) test_that("x", { })      # an `if`
for (i in seq_len(n)) test_that("x", { })   # a `for`
result <- test_that("x", { })               # an assignment
test_that("x", { }) |> invisible()          # a pipe
local({ test_that("x", { }) })              # a call to `local`
```

The **callee is the bare identifier** `test_that` — never `testthat::test_that`,
never `obj$test_that`, and never the backtick spelling `` `test_that` ``, which
denotes the same symbol in R but is not the source-visible bare name this anchor
claims.

There are **exactly two positional arguments**. A named spelling
(`test_that(desc = "x", code = { })`) is the same call to R and is deliberately
not admitted; widening to it is a superseding decision, not an implementation
detail. The first argument is a **non-empty plain string literal**, so a
description built from a variable, a call, or an R 4.0 raw string yields no
anchor. The second is a **brace block**.

`describe`/`it` (testthat's BDD layer), `expect_*` assertions, `setup.R`,
`helper-*.R`, snapshot tests, parameterised generation, and skip conditions are
out of scope. They are named follow-ups, not silent omissions.

### D2a. A call reached only at run time is not admitted

R decides at run time whether — and how often — a top-level expression runs a
`test_that` call. `if (interactive())`, `if (Sys.getenv("CI") == "true")`, and
`for (...)` are the ordinary spellings, and RepoGrammar evaluates none of them.

Such a call is **not** an anchor, and the file records `BuildVariantAmbiguity`
under a claim of its own, `r_conditional_test_registration`. That claim is
deliberately separate from `r_testthat_identity` and deliberately non-blocking:
declining to anchor a conditional call understates support, it does not unprove
the unconditional calls beside it, and those still form their family.

This is the same rule ADR-0044 D4a states for Delphi, arrived at from the same
defect. The difference is only where the undecidable branch comes from: a
compile-time define there, a runtime condition here.

### D2b. A file that binds `test_that` proves nothing

`test_that` is an ordinary R binding, not a reserved word. A file that assigns
the name has changed what every call in it means, and RepoGrammar cannot
evaluate the replacement.

Every syntactic binding form in the file is therefore checked — `<-`, `<<-`,
`=`, `->`, `->>`, a `for` loop variable, and `assign("test_that", ...)` with a
literal name — in symbol, backtick, and string spellings. If any is present, the
file emits `MonkeyPatch` under `r_testthat_identity` and **anchors nothing**.
That `UNKNOWN` blocks, for the same reason the missing DESCRIPTION declaration
does: it unproves the framework identity rather than one call's shape.

Rebinding from *outside* the file — `library()` masking, a `source()`d or
testthat-loaded helper, a non-literal `assign`, `attach`, or any binding made in
an enclosing environment — is not statically determinable by any frontend
without an evaluator, and this one claims no resolution: see D5.

### D3. The repository must declare testthat

An admitted path is necessary and not sufficient. The anchor also requires that
the repository's `DESCRIPTION` declares `testthat` in one of the official
dependency fields that ADR-0036 already parses.

Without that declaration the file emits a typed `UNKNOWN` and anchors nothing:
a directory named `tests/testthat` in a project that does not depend on testthat
does not establish the framework. This is the same shape as the TS/JS ambient
test-runner gate, which requires project context before an unimported runner
name may anchor.

Because this flag changes how every admitted R file parses, a `DESCRIPTION`
change must force a full rebuild rather than a file-local reparse.

### D4. The declared R subset

The parser admits the following, and abstains on everything else.

**Lexical.** Line comments; single- and double-quoted string constants with
backslash escapes; R 4.0 raw string constants in every form — `r"(...)"`,
`r"[...]"`, `r"{...}"`, any number of dashes between the quote and the opener,
and an uppercase `R` prefix; backtick-quoted names; numeric constants including
hexadecimal, decimal and binary exponents, and the `L` and `i` suffixes; ASCII
identifiers matching `[A-Za-z.][A-Za-z0-9._]*`; the reserved words `if`, `else`,
`for`, `in`, `while`, `repeat`, `function`, `break`, `next`; and R's operators
`<- <<- -> ->> = ~ | || & && ! < > <= >= == != + - * / ^ : :: ::: $ @ %any% |>
\ ( ) { } [ [[ ] , ;` together with the newline.

Reserved words that are only values — `TRUE`, `FALSE`, `NULL`, `NA` and its
typed forms, `Inf`, `NaN` — are read as ordinary symbols. They parse exactly as
symbols do, so the tree has the same shape either way and this frontend reads no
value.

**Grammatical.** R's expression language over those tokens: calls; `[` and `[[`
indexing; `$`, `@`, `::` and `:::` access; unary and binary operators at R's own
precedence and associativity, comparison non-associative as R declares it;
`function(...)` and the R 4.1 `\(...)` lambda, with formals and defaults;
`if`/`else`; `for`; `while`; `repeat`; `break`; `next`; brace blocks;
parenthesized grouping; and argument lists whose elements may be empty (`m[, 1]`)
or named (`f(a = 1)`). R's split between `expr_or_assign` and `expr` is
reproduced, so `=` is an assignment at statement level and inside `(...)` and
`{...}`, and names an argument inside a call or an index.

**Newlines.** R's rule, because expression boundaries decide which expression is
top-level and therefore which calls can anchor at all. A newline terminates a
complete expression at top level and inside `{ }`; it is whitespace inside `(`,
`[`, and `[[`; and it is eaten wherever an expression is still required — after
any operator or comma, and between `if (cond)`, `for (...)`, `while (cond)`, or
`function(formals)` and the body. Consequently `x <- 1` / newline / `-2` is two
expressions, `x <- 1 -` / newline / `2` is one, and `test_that("a", { })` /
newline / `(x)` is two rather than a call of a call.

`else` follows this rule and not a special case: at top level a newline before
`else` ends the `if`, so `if (a) b` / newline / `else c` is the R syntax error
it really is, while inside `{ }` the same spelling is legal. Newlines before an
`else` are consumed only once an `else` is known to follow, so an `if` that ends
a block leaves the block's own separator alone.

**Refused**, each because what the construct means is not determinable from
source:

- `=>`, R's pipe bind, which parses only when the `_R_USE_PIPEBIND_` option is
  set. Option-selected syntax is undeterminable by construction.
- `?`, the help operator.
- `_` in code position: never a valid R identifier, and the R 4.2 pipe
  placeholder.
- Any non-ASCII byte in code position. R classifies identifier characters with
  the C library's `iswalpha` under the session's locale, so whether such a byte
  continues an identifier depends on an environment RepoGrammar cannot
  determine. Comments and string constants are unaffected: their bytes are never
  classified. R's own portability guidance for packages says the same thing from
  the other direction.
- Chained comparison (`a < b < c`), which R declares non-associative.
- Nesting past a fixed depth. Repository contents are untrusted, and a file of
  nothing but open parentheses must abstain rather than exhaust the stack.

**Abstention never recovers.** Where the parse leaves the subset, the frontend
records the boundary and stops. It does not guess, does not error-recover, and
does not resynchronize to a later expression — so no anchor after the boundary
can be invented. Top-level expressions completed *before* the boundary are
whole, parsed expressions and are kept; the file reports a degraded parse and an
`InsufficientSupport` `UNKNOWN` under `r_test_parse`, which states that the
expressions not found prove nothing. This is the same guarantee ADR-0024 D6 and
ADR-0025 D7 demand when they forbid partial, recovered, or error-node anchors.

A call is therefore recognized only from parsed source. Line comments, both
quote styles, and raw strings can never contribute to an anchor — not by a
separate awareness rule, but because they are literals in the tree. This is
worth stating because the repository has shipped the opposite defect four times:
a Rust attribute matched inside a function body, a TS/JS runner matched inside a
member call, a Go declaration matched inside a string, and both branches of a
Delphi `{$IFDEF}`. Each came from asking whether text appears instead of whether
a construct exists.

### D4a. The invariance argument

ADR-0020 gate 2 requires that the admitted parse not depend on anything
RepoGrammar cannot determine. ADR-0040 satisfied this for SQL by declaring a
dialect-invariance set and refusing every construct the members disagree about.
R's version of the argument is a different shape, because R has no dialects.

**R's grammar is fixed and configuration-free.** There is no preprocessor, no
conditional compilation, no macro layer, no dialect or mode selector, no include
mechanism, and no user-extensible syntax beyond `%any%` operators, which all lex
identically and share one precedence level. Nothing outside the file's own bytes
selects how those bytes parse. The whole class of hazards ADR-0044 D4a has to
handle for Delphi — a define picking one of two branches, a directive
re-selecting the language mode — has no R counterpart.

**The one remaining axis is the interpreter version, and R's differences on it
are pure accretion.** Each construct this parser admits that a pre-4.0 R lacks
was previously *invalid* syntax, not differently-valid syntax:

| Construct | Added | Under an older R |
|---|---|---|
| Raw strings `r"(...)"` | 4.0.0 | syntax error |
| Lambda `\(x)` | 4.1.0 | syntax error |
| Native pipe `\|>` | 4.1.0 | syntax error |
| Pipe placeholder `_` | 4.2.0 | syntax error (and refused here anyway) |

`=>` is absent from that table on purpose. It is not accretion: it is gated on
an option rather than a release, so it is refused outright rather than reasoned
about by version.

So for every file this frontend admits, the expression boundaries and admitted
shapes it computes are the ones R computes, under **every** R version that
parses the file at all. No R version reads an admitted file into a *different*
tree; an older one either agrees or refuses the file outright. The unproven
interpreter version therefore bounds only whether the file runs — which D5
already declines to claim — and not what the file says.

The two places where invariance would fail are refused rather than read, above:
`=>` depends on an option, and non-ASCII identifier bytes depend on a locale.
That is the whole exposure, and it is closed by abstention.

**One representational divergence, stated because a reader will ask.** R
desugars during parsing: `quote(x |> f())` is literally `f(x)`, and
`quote(1 -> x)` is `x <- 1`. This parser does not desugar; a pipe and a rightward
assignment stay binary nodes. So the claim is about identical expression
*boundaries* and identical admitted *shapes*, not byte-identical trees. The
divergence is one-directional and safe: `"desc" |> test_that({ })` is the
admitted call in R's own tree and yields no anchor here. It can only
under-anchor, never invent an anchor the source does not spell.

### D5. What the anchor claims, and what it explicitly does not

It claims that a file testthat's runner would execute contains a top-level
`test_that` call, spelled with the bare name, whose two positional arguments are
a literal description and a brace block.

It claims nothing about outcomes, assertions, skips, ordering, parallelism,
fixtures, helper loading, or snapshot state, and it establishes no cross-file
identity. R's non-standard evaluation is untouched: `test_that` quotes its second
argument, and nothing here evaluates, substitutes, or quotes anything.

Four non-claims are worth naming rather than leaving to inference, because they
are the ones a reader will reach for. None is determinable by a static frontend
without an evaluator, and each is already covered by the fact's standing
`provider_resolved=false` assumption:

- **Name resolution.** The anchor asserts the *call shape*, not that
  `test_that` resolves to testthat's function. A binding created inside the file
  is decidable and abstains under D2b; one created by `library()` masking, a
  `source()`d or testthat-loaded helper, `attach()`, a non-literal `assign()`, or
  an enclosing environment is not, and is not claimed either way.
- **Execution.** That testthat's runner *reads* the file is evidence from its
  own convention. That the file's tests actually run — that R is installed, that
  its version accepts the file's syntax, that no earlier expression errors —
  is runtime state and is not claimed.
- **Test count.** Calls added by `source()`, `eval(parse(...))`, generation
  helpers, or a conditional or looped registration are not anchored. Every one
  of these can only *understate* support; none can invent an anchor.
- **Interpreter version.** No R release, library path, profile, or renv
  activation is selected or claimed. D4a shows why the admitted parse does not
  depend on one.

### D6. No dependency and no execution

No Rust crate, no grammar, no generated parser, no R installation, no downloaded
artifact. The parser is hand-written Rust in the existing core. R is never
invoked, and neither are `parse`, `eval`, `source`, renv, profiles, package
restoration, or repository scripts. ADR-0036's execution prohibitions are carried
forward unchanged.

Input is untrusted. The frontend keeps ADR-0036's input-byte limit, bounds the
number of emitted units, and bounds recursion depth; all three abstain with a
typed `UNKNOWN` rather than degrading silently or failing unsafely.

## Alternatives considered

- Anchor on `library(testthat)` inside each test file: rejected because
  testthat's own convention does not require it in test files, so it would miss
  the ordinary case while adding no identity the path and DESCRIPTION do not
  already give.
- Admit every `.R` file and filter inside the parser: rejected for the reason
  ADR-0041 gives for Go — it would claim authority over source there is no
  reason to read.
- Accept a non-literal description: rejected because the description is the
  only thing distinguishing one anchor from another, and a computed one is not
  source-visible.
- Start with `describe`/`it`: rejected because that layer is far less used than
  `test_that` and shares its hazards without its convention.
- Keep the byte scanner and add a depth rule for `if`/`for`: rejected. The
  scanner's question — where does this token sit between brackets? — is the
  wrong one, and patching it case by case would have left the next spelling
  wrong in the same way. `x <- test_that(...)`, `enabled && test_that(...)`, and
  `test_that(...) |> invisible()` are three more shapes with no bracket signal
  at all.
- Vendor a maintained R grammar: rejected under the program's
  zero-external-dependency constraint, which admits no new crate, grammar,
  toolchain, or downloaded artifact. It is also unnecessary: D4a shows the
  admitted subset needs no configuration a generated parser would supply.
- Desugar `|>` and `->` the way R's parser does, so that piped spellings anchor:
  rejected. Reconstructing a call the source does not spell would make the
  anchor a claim about a transformation rather than about visible source, and
  the current behaviour errs toward abstention. D4a records the divergence.
- Admit the named spelling `test_that(desc = ..., code = ...)`: rejected here
  because it widens the claim surface, which this decision explicitly does not
  do. It is a candidate for a superseding ADR, not an implementation choice.

## Consequences

- R gains a frontend, owned units, typed `UNKNOWN`s, and one exact family for
  one call shape in one path class. It gains no project model and no provider.
- The security posture changes in one way: `tests/testthat/test-*.R` bytes cross
  the source-store boundary. No execution, network, or installation is added.
  Recursion over untrusted input is depth-bounded.
- ADR-0036 remains in force for discovery, metadata parsing, exclusions, limits,
  and every execution prohibition.
- The frontend recognizes strictly fewer calls than the scanner it replaces, and
  that is the point: every call it stopped anchoring was one whose registration
  or callee identity was never proven. Support counts for repositories using
  those shapes fall, and the typed `UNKNOWN`s of D2a and D2b say why.
- Widening the admitted path set, the admitted call shape, the declared subset
  of D4, or the claim surface requires a superseding ADR. In particular, a
  construct whose parse would depend on an option, a locale, or an interpreter
  version breaks D4a's invariance and may not be admitted without one.
