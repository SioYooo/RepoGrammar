# Tree-sitter frontend migration

Status: **PREREGISTERED**. Base: `main@58e57ab1e3fa4d175a3865a74654db779454660a`,
branch `feat/tree-sitter-frontends`. Local only; no push or merge. Authority:
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

| Lane | Verdict | Fixture capture | In-process differential | External corpus | Non-test lines (before → after) |
| --- | --- | --- | --- | --- | --- |
| Go | `EQUIVALENT` | 4,681 rows, 0 mismatches; `sync-equivalence` 14/14; `product-eval` equal | all committed `_test.go` fixtures and 25 branch sources identical | 1,753 stdlib files: 60 legacy misparse refusals removed (22 legacy debug panics), 4 `refusal_class_changed` | 1,855 → 1,699 |

Go evidence: capture `47ccdddb39ad17af0a7f0b686bf0a37b1e40784b979beb4349cfa74846b7adfe`,
`sync-equivalence` `472442a11617f1fe505858c7b8760752e58e887747c6a55cc106fa1ebed779a0`,
`product-eval` `344569bd8a44c5e0535cc4d442da47836ceb6b327c86a386ec29a788b0567dce`.
The lexer stays as the lexical-subset gate; only the recursive-descent parser
was removed. Details are in the
[Go completion review](../reports/language-support/go-completion-review.md#adr-0058-frontend-change--2026-10-20).

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
