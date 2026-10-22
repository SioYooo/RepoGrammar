# Tree-sitter frontend migration

Status: **ACCEPTED 2026-10-22**; every `CLASSIFIED` lane was accepted and its
legacy parser deleted (see [Decisions](#decisions)). TS/JS is a separate
phase under ADR-0059. Preregistered in `22b0a31`. Base: `main@58e57ab1e3fa4d175a3865a74654db779454660a`,
branch `feat/tree-sitter-frontends`. Local only; no push. Authority:
[ADR-0058](../decisions/ADR-0058-tree-sitter-frontends-for-every-language.md).

## Objective

Move every hand-written source frontend admitted by ADR-0058 onto its pinned
Tree-sitter grammar, keeping each lane's declared-subset policy, refusal
classes, anchor shapes and obligations. Each lane ends as `EQUIVALENT`,
`CLASSIFIED` or `BLOCKED`.

## Lane order

Go, Ruby, PHP, Swift, R, Fortran, MATLAB, Delphi/Object Pascal, Ada, SQL,
Assembly, then TS/JS. Go goes first because it has one role, a first-party
grammar and an external corpus. TS/JS goes last: it is the largest lane and the
least likely to be byte-identical. VB.NET and Python are out of scope under
ADR-0058.

## Method per lane

1. Admit the lane's crate (exact version) in the same change.
2. Replace only the syntax layer. Build the lane's existing parsed structure
   from the Tree-sitter tree, and keep the downstream anchor, unit, fact and
   obligation code unchanged.
3. Abstain on the whole file when the tree has an `ERROR` or `MISSING` node.
   Reproduce every existing refusal class from the tree. Lexical validations
   the grammar does not perform reuse the lane's existing validator on the
   relevant leaf nodes.
4. Keep the hand-written frontend under `cfg(test)` until the tier verdict.
   An in-process differential compares the old and new frontends on every
   committed fixture of the lane, a branch corpus of the lane's refusal and
   anchor cases, and an external corpus when one exists locally (Go: the
   1,753 `_test.go` files of the installed Go 1.26.4 standard library).

## Declared intentional divergence

ADR-0058 makes any `ERROR` or `MISSING` node refuse the file. Legacy
frontends that skip regions (for example Go function bodies) accepted a file
whose skipped region is syntactically invalid but token-balanced. Such inputs
now abstain. This single class, `syntax_error_in_skipped_region`, is declared
in advance. It is excluded from the branch-corpus equality and pinned by its
own test instead. It affects only source that does not compile.

## Oracle tiers

- **`EQUIVALENT`**: the fixture-wide capture (179 roots, CLI/MCP surfaces and
  the full 14-table generation fingerprint) shows zero mismatches against the
  baseline, and the in-process differential shows identical reports on every
  committed fixture and branch-corpus source. The hand-written frontend is
  deleted in the same lane change. External-corpus differences do not block
  this tier but are reported and classified.
- **`CLASSIFIED`**: mismatches exist. Every differing report or row gets one
  class:
  - `range_shift`: unit byte range differs, but family and anchor output is
    identical.
  - `unknown_added`: the grammar sees an ambiguity or error the scanner
    missed.
  - `unknown_removed`: must cite why the old refusal was a parser limitation,
    not a semantic claim of the lane's ADR; otherwise it is a regression.
  - `anchor_changed`: blocks the lane.
  - `refusal_class_changed`: the file still abstains, but under another
    class.

  The hand-written frontend stays as a test-only oracle until the maintainer
  accepts the classification.
- **`BLOCKED`**: the grammar cannot express the lane's subset policy without
  reimplementing the parser, fails to build, or mis-parses committed
  fixtures. The exact reason is recorded.

## Product-eval comparison

Results must be equal after excluding every field whose name contains
`latency`, plus `started_at`, `finished_at` and `repogrammar_commit`. This
fixes the timing-field gap recorded by the language-pack spike.

## Rounds and stop rules

At most three rounds per lane. Each round targets the observed failure class
with one conservative change and a negative test. A lane stops at its tier
verdict, at `BLOCKED`, or after three rounds as `CLASSIFIED`.

## Later phases (preregistered separately before they run)

- The anchor-rule layer experiment, after Go and TS/JS are migrated. It runs
  on C# first, under the byte-equivalence oracle, with a framework-specific
  plugin residue of at most 25%.
- Optimization: a shared query helper, compiled-query caching and a measured
  `Node::parent()` cost. Results are in [Optimization](#optimization).

## Baseline

Frozen baseline binaries from `58e57ab` (after `cargo clean -p repogrammar`):
`repogrammar` `64d74bd0a1ad9a41d572215c29227730b3cd196d79cd4b8bb6439a32aaadfb4f`,
`repo-guard` `7aae51ef03fd209221502e4694c87430639e158deb6aae966d126f0694224de2`.
Two baseline captures agree on all 4,681 rows (179 roots); SHA-256
`e493fdb9ae47e2f5e83a0b2a81d7f95de1e374fe4621eef3b9c6ad283423a85e` and
`e2ed884dc7d853d89846ca59a0a9152bfdfaf960cede3dae8163b22ef60653b9`.
`sync-equivalence --all` passes 14/14
(`52d49583aff5b1d8ff6af7c221ee3294b7d60ecd3d0dd826b8b15f32becab7dd`).
`product-eval` reports retrieval 34/47, abstention 36/36 and context 27/27
(`72d61389476cd08b275dba87cfc5c4fff23299e3bd6665f8eb4149c599c3541f`).

## Lane results

| Lane | Verdict | Fixture capture | In-process differential | External corpus | Frontend non-test lines |
| --- | --- | --- | --- | --- | --- |
| Go | `EQUIVALENT` | 0 mismatches | all committed `_test.go` fixtures and 25 branch sources identical | 1,753 stdlib files: 60 legacy misparse refusals removed (22 legacy debug panics), 4 `refusal_class_changed` | 2,661 → 2,520 |
| Ruby | `CLASSIFIED` | 0 mismatches | 9 fixtures and 67 branch sources identical | 1,219 CocoaPods `.rb` files: 437 `unknown_removed` (legacy statement-modifier misparse), 1 `unknown_added` (grammar gap) | 2,318 → 2,329 |
| PHP | `CLASSIFIED` | 0 mismatches | 7 fixtures and 195 branch sources identical | no PHPUnit corpus (33 files identical); 23,195 mutants: 419 `unknown_added` (skipped-region errors), 401 `refusal_class_changed` (broken and over the depth bound), 2 `unknown_removed` | 1,805 → 1,739 |
| Delphi/Object Pascal | `CLASSIFIED`, with `anchor_changed` | 0 mismatches | 3 fixtures and the branch corpus identical apart from pinned divergences, which include new anchors | no DUnitX corpus; 2 of 3 `.pas` examples differ (not compilable units) | 1,001 → 952 |
| Fortran | `CLASSIFIED` | 0 mismatches | 9 fixtures and 252 branch sources identical | 80 free-form files: 75 identical, 5 legacy refusals removed | 1,964 → 1,734 |
| MATLAB | `CLASSIFIED` | 0 mismatches | 5 fixtures and 247 of 257 branch sources identical; 10 pinned | no `matlab.unittest` corpus; 2,554 deletion mutants: 359 differ, all invalid MATLAB (7 change the unit set) | 1,346 → 1,597 |
| Swift | `CLASSIFIED` | 0 mismatches | 8 fixtures and 115 branch sources identical | 1,084 files: 101 differ (59 `unknown_removed`, 34 `refusal_class_changed`, 8 `unknown_added` grammar gaps), 0 `anchor_changed` | 1,821 → 1,664 |
| R | `CLASSIFIED` | 0 mismatches | 3 fixtures and 486 branch sources identical | 3,434 installed `.R` files: 0 differ; 33,108 mutants: 8 differ, all with 0 anchors | 2,731 → 3,055 |
| Ada | `CLASSIFIED` | 0 mismatches | 3 fixtures identical; 4,378 of 4,482 branch sources identical (70 `unknown_added`, 34 `unknown_removed`) | 3 Pygments files, identical | 2,355 → 1,485 |
| Assembly | `BLOCKED` | n/a | n/a | `tree-sitter-asm` 0.24.0 does not parse GNU `as` AT&T syntax: all 679 local `.s` files have `ERROR` or `MISSING` nodes | unchanged |
| SQL | `BLOCKED` | n/a | n/a | `tree-sitter-sequel` 0.3.11: 158 of 213 scanned files have `ERROR` or `MISSING` nodes that swallow statement boundaries | unchanged |
| TS/JS scope graph ([ADR-0059](../decisions/ADR-0059-tree-sitter-tsjs-frontend-in-slices.md) slice 1) | `CLASSIFIED`, with `unknown_removed` and `unknown_added` | 0 mismatches | 82 fixtures: 0 decision differences; 43 branch sources attributed | 882 external files: 0 decision differences apart from 48 `unknown_kind_changed`; 11 refused by grammar gaps | 901 → 778 (`scope_graph.rs`) |
| TS/JS framework anchors ([ADR-0059](../decisions/ADR-0059-tree-sitter-tsjs-frontend-in-slices.md) slice 2) | `CLASSIFIED` pending acceptance, with `anchor_changed`, `unknown_removed` and `unknown_added` | 2 mismatches, one cause: `drizzle_exact_repositories` `unknowns` reason code (a mid-chain unit) and its `generation` hash | 82 fixtures: 4 `unknown_kind_changed` (2 units); 65 branch sources attributed (11 `anchor_changed`) | 885 external files: 0 `anchor_changed`, 196 `unknown_added` (commented-out test lines), 162 `unknown_kind_changed`; no framework sources locally | 3,168 → 3,615 (touched `tsjs/` modules; +792 test-only) |

- The per-lane evidence is in each lane's completion review under
  `docs/reports/language-support/`. Go's row is from
  [go-completion-review](../reports/language-support/go-completion-review.md).
- Fixture capture was run after each lane landed, cumulatively, in branch
  order: Ruby, PHP, Delphi, Fortran, MATLAB, Swift, R, Ada. Every run had
  179 roots, 4,681 rows and 0 mismatches; `sync-equivalence` passed 14/14 and
  `product-eval` was equal.
- Frontend non-test lines count `src/rust/adapters/parsing/<lane>/*.rs`.
  Items under `#[cfg(test)]` and test-only modules (the legacy parsers and
  differentials) are left out. Both sides use the same count, before the
  migration (`22b0a31^`) and now. The count leaves out the lane's
  project-inventory file (`<lane>.rs`), which the migration did not touch. An
  earlier version of the Go row gave 1,855 → 1,699 under a different count.
- TS/JS has a dependency-free extractor with no declared subset or refusal
  classes to reproduce, so this port protocol does not apply to the whole
  lane. ADR-0059 (accepted) ports it in slices; slice 1 (the scope graph, accepted with its legacy reader deleted) is recorded in
  [tsjs-scope-graph-tree](tsjs-scope-graph-tree.md), slice 2 (framework
  anchors read the tree) in
  [tsjs-framework-anchors-tree](tsjs-framework-anchors-tree.md) (classified,
  awaiting acceptance), and slice 3 is not ported.

## Decisions

The maintainer accepted every `CLASSIFIED` lane on 2026-10-22, including
Delphi's `anchor_changed`, and authorized deleting the placeholder
`adapters/parsing/tree_sitter.rs`. The legacy parsers and their in-process
differentials were deleted the same day; each classified difference below
that a differential pinned now lives on as an ordinary test of the Tree-sitter
frontend's current output. The list below is the record of what was accepted.
Each lane loses or changes recall where its grammar has a gap:

- Delphi/Object Pascal: a fixture after a `record` or `interface` in the same
  `type` section, or after a nested record, now anchors. The legacy skip lost
  it. This is `anchor_changed`, which the oracle tiers say blocks a lane
  unless the maintainer accepts it. Every case is a legacy misparse, pinned by
  a test.
- PHP: `declare(strict_types=1, ticks=1)` (a directive list) is valid PHP that
  grammar 0.25.1 rejects, so such a file now refuses.
- Ruby: a method call on a command-call `do ... end` block is valid Ruby that
  grammar 0.23.1 rejects; one external file now refuses.
- Fortran: an alternate-return dummy (`subroutine s(a, *)`) is a grammar gap
  and now refuses.
- MATLAB: an empty attribute list, `methods ()`, adds a degraded-parse
  diagnostic. 7 mutants of invalid MATLAB change the unit set.
- Swift: 8 of 1,084 external files use Swift 5.10/6 forms, a labeled trailing
  closure on its own line, or `f(())`, which grammar 0.7.4 cannot read, so
  they abstain.
- R: no valid-code regression; 8 mutant-only differences on inputs R rejects.
- Ada: Ada 2022 `for I : T in 1 .. 3 loop` is a grammar gap and now refuses.
  `type T is new A and B with null record` (an interface progenitor list) is
  now admitted. ADR-0045 D4a excludes interface types but does not name
  progenitor lists.

## Build cost

ADR-0058 requires binary size and build time to be recorded. Clean
`cargo build --release --locked --bin repogrammar` on the maintainer machine,
each in a fresh target directory:

| Revision | Wall time | `repogrammar` size | `__const` section |
| --- | --- | --- | --- |
| baseline `58e57ab` | 34.2 s | 22,321,856 bytes | 11,230,556 bytes |
| `33ed6a7` (9 lanes migrated) | 34.7 s | 38,608,576 bytes | 26,197,724 bytes |
| TS/JS slice 1 (`tree-sitter-typescript` + `tree-sitter-javascript`) | not timed | 41,899,392 bytes | not measured |

The binary grows by 16.3 MB (73%), almost all in parse tables. The new grammar
static libraries total about 16.5 MB:

| Grammar | Static library |
| --- | --- |
| `tree-sitter-fortran` | 6.3 MB |
| `tree-sitter-swift` | 4.0 MB |
| `tree-sitter-php` | 2.1 MB |
| `tree-sitter-ruby` | 2.0 MB |
| `tree-sitter-pascal` | 0.7 MB |
| `tree-sitter-r` | 0.5 MB |
| `tree-sitter-matlab` | 0.4 MB |
| `tree-sitter-ada` | 0.3 MB |
| `tree-sitter-go` | 0.2 MB |

TS/JS slice 1 adds 3.3 MB (8.5%) for the TypeScript, TSX and JavaScript
grammars.

Fortran's grammar is the largest single cost for a lane that reads only the
test-drive subset. Build wall time is unchanged, because the grammar C
sources compile in parallel with the Rust crate.

## Optimization

Both changes are byte-equivalent under the lane oracle: 0 capture mismatches
over 179 roots, `sync-equivalence` 14/14, `product-eval` equal, and identical
parse snapshots of the Unity corpus and the C# fixtures.

- Compiled-query caching (`6302f64`). Compiling the C# `nodes.scm` query took
  about 3.5 ms per file in a release build; parsing a small fixture takes
  about 44 µs. `CSharpSyntaxParser` now compiles it once per instance, in a
  `OnceLock`.
- Ancestor lookup. `Node::parent()` descends from the root on every call, so
  the C# scanner's per-node ancestor walks (pre-order depth and inherited
  context) were cubic in nesting depth. One cursor walk, `capture_ancestry`,
  now records each captured node's depth and nearest `scope` or `class_like`
  ancestor. A unit test checks it against the parent walks.

Snapshot wall time, release build, single file of nested calls `F(F(…))`:

| Nesting depth | Before | After |
| --- | --- | --- |
| 200 | 1.09 s | 0.08 s |
| 400 | 2.57 s | 0.08 s |
| 800 | 19.14 s | 0.07 s |

The "after" times are mostly the test harness's start-up. On the 4,818-file
Unity corpus the medians of three alternating runs were 5.51 s before and
6.06 s after. Single runs ranged from 5.0 s to 7.2 s, so the difference is
within run-to-run noise. Reading files from the external volume dominates that
run.

No other lane walks ancestors with `parent()`; the remaining calls read a
single parent. The shared query helper is deferred: only the C# lane compiles
a Tree-sitter query, and the others walk with a cursor.
