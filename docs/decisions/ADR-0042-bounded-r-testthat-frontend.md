# ADR-0042: Bounded R testthat frontend

- Status: Accepted
- Date: 2026-08-15
- Scope: R in ADR-0020; admits a family-bearing frontend over one call shape
- Refines: ADR-0036 (authorizes the R source unit it declined to authorize)
- Related: ADR-0040, ADR-0041, ADR-0019,
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
scanner in the existing Rust core executes nothing, so the execution
prohibitions are unreached, and the authorization gap is what this ADR closes.

The same is true of "sandboxed": the word binds to routes that run something.
Nothing runs here.

The contrast is deliberate and load-bearing. Go, Ruby, PHP, and Swift each carry
an evidence ladder that forbids this route for the claim and names a pinned real
parser as primary evidence; under the zero-external-dependency constraint those
families are closed. R carries no such ladder. That is the whole reason this
lane is open and those are not.

### Why testthat, and why the scanner route is sound for R

testthat is the dominant R testing framework and the one the R packages
convention builds around: `test_dir()` scans `tests/testthat/` and runs files
whose names begin with `test`. The path is not a style preference; it is the
set the runner reads.

R also lexes cleanly for this purpose. It has line comments, single- and
double-quoted strings, and R 4.0 raw strings — and it has none of the
constructs that make a scanner unsound elsewhere: no heredocs, no regex
literals, no transpose-versus-string ambiguity, no significant indentation, and
unambiguous `(` and `{` nesting. That is why this lane gets a scanner and Ruby
does not.

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

Every condition is required. The call is at top level — nesting depth zero for
both parentheses and braces — so a `test_that` inside another function is not
an anchor. The callee is the bare identifier `test_that`, never a namespaced or
member spelling. There are exactly two arguments. The first is a non-empty
string literal, so a description built from a variable or a call yields no
anchor. The second is a brace block.

`describe`/`it` (testthat's BDD layer), `expect_*` assertions, `setup.R`,
`helper-*.R`, snapshot tests, parameterised generation, and skip conditions are
out of scope. They are named follow-ups, not silent omissions.

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

### D4. The scanner is string- and comment-aware

A call is recognized only from text that is source. Line comments, single- and
double-quoted strings, and R 4.0 raw strings (`r"(...)"` and its bracket and
dash variants) never contribute to an anchor.

This is a decision and not an implementation note because the repository has
shipped the opposite defect three times: a Rust attribute matched inside a
function body, a TS/JS runner matched inside a member call, and a Go
declaration matched inside a string. Each came from asking whether text appears
instead of whether a construct exists.

### D5. What the anchor claims

It claims that a file testthat's runner would execute contains a top-level
`test_that` block with a literal description. It claims nothing about outcomes,
assertions, skips, ordering, parallelism, fixtures, helper loading, or snapshot
state, and it establishes no cross-file identity. R's non-standard evaluation is
untouched: nothing here evaluates, substitutes, or quotes.

### D6. No dependency and no execution

No Rust crate, no grammar, no R installation, no downloaded artifact. R is never
invoked, and neither are `parse`, `eval`, `source`, renv, profiles, package
restoration, or repository scripts. ADR-0036's execution prohibitions are
carried forward unchanged.

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

## Consequences

- R gains a frontend, owned units, typed `UNKNOWN`s, and one exact family for
  one call shape in one path class. It gains no project model and no provider.
- The security posture changes in one way: `tests/testthat/test-*.R` bytes cross
  the source-store boundary. No execution, network, or installation is added.
- ADR-0036 remains in force for discovery, metadata parsing, exclusions, limits,
  and every execution prohibition.
- Widening the admitted path set, the admitted call shape, or the claim surface
  requires a superseding ADR.
