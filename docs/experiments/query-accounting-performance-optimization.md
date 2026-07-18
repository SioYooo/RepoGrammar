# Query accounting and indexing performance optimization

Date: 2026-07-19
Optimization branch baseline: `v0.3.2` (`26ce59e`)
Optimized implementation measured at: `e9ad298`

## Question and preregistered safety gate

This experiment asks why the displayed estimated token-savings yield was low,
whether repeated queries were omitted, and whether indexing and query usage can
be improved without inflating pattern-family claims.

The acceptance gates are:

- repeated invocations must each advance the query denominator;
- numerator and denominator must belong to one explicit cohort and one atomic
  update;
- concurrent writers must not lose increments;
- query retrieval changes must preserve `25/25` correct abstentions, zero false
  family selections, and zero selections on abstention gold;
- incremental and no-op sync must remain canonically equivalent to a clean
  rebuild;
- estimated potential savings must never be reported as measured savings.

## Root cause

The original `14 / 196` display did not omit identical queries. The v1
implementation counted every invocation, including repeats, but stored query
outcomes and estimated-savings events in two independent files and independent
write operations. Their timestamps and totals could diverge, so the ratio did
not describe one valid cohort. Most invocations also used fuzzy, broad prose
targets and ended as `UNKNOWN` or fallback; this limited savings-event yield.

For the 14 context-delivering events, the old aggregate estimated 54,620
baseline tokens and 3,324 returned tokens: 51,296 estimated potential savings,
or about 93.9% compression on a successful event. The primary bottleneck was
therefore event coverage and accounting validity, not per-hit compression.

## Changes under test

- `family-query-metrics.v2` records one query denominator plus its optional
  savings numerator, breakdowns, epoch, start time, and producer version in one
  process-serialized atomic replacement. Legacy v1 files are isolated.
- Managed instruction v3 prioritizes exact repo-relative path/locator, exact
  unit/member/symbol, and exact framework role before concise pattern prose. It
  permits one bounded `show_family` candidate inspection on a single-candidate
  `UNKNOWN`, without converting candidate context into proof.
- The Python parse response carries the strict interface hash used by sync,
  removing the second Python worker process formerly launched for every Python
  file during a full build.
- A supported zero-delta sync retains the active generation and skips full
  snapshot hydration, copy-forward, reparsing, family recomputation, and a new
  generation write.
- Status/doctor reject storage-schema skew, and install self-test now validates
  MCP initialize version, managed instructions, and the exact tool schema.

## Measurements

All timing rows used release binaries, fresh `git archive` workspaces under
`/tmp`, and the same machine/session. The optimized archive contained 21,496
code units and 32,770 semantic facts versus 21,326 and 32,588 in the baseline,
so the optimized run processed a slightly larger revision.

| Operation | v0.3.2 baseline | Optimized | Change |
| --- | ---: | ---: | ---: |
| fresh `init` with index | 17.38 s | 10.78 s | -38.0% |
| full `resync` | 17.49 s | 10.68 s | -38.9% |
| unchanged `sync` | 8.09 s | 1.21 s | -85.0% |
| repeated unchanged `sync` | not measured | 1.22 s | stable fast path |

The unchanged sync retained `gen-000002` and reported 373 unchanged files,
zero copied-forward files, zero reparses, and zero family recomputations.

The ignored read benchmark used 12,000 code units, 300 families, 25,000 family
evidence rows, and 18,000 semantic facts. Representative timings were 150 ms
for stats, 2.26 s for stats plus the complete UNKNOWN inventory, 212 ms for an
exact family, 322 ms for an exact member, and 542 ms for a fuzzy path. The write
benchmark's generation session took 255 ms with one connection and eight
transactions; the granular reference arm took 5.44 s with 6,200 connections
and transactions. This confirms that parser process startup and redundant
generation work, rather than the batched SQLite write session, dominated the
indexing optimization target.

## Query evaluation

The optimized product and token-overlap baseline were rerun over the committed
79-query corpus with one repetition:

| Condition | hit@1 | candidate recall | MRR | correct abstention | false family | selected on abstention |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| optimized product | 21/42 | 13/14 | 0.500 | 25/25 | 0/46 | 0 |
| token-overlap baseline | 11/42 | 3/14 | 0.262 | 22/25 | 0/46 | 4 |

The product retained all safety gates. Exact and anchored targets are already
strong (`17/17` in the query-funnel audit), while natural-language and synonym
targets remain the main retrieval opportunity. Lowering the selection threshold
is rejected because it would trade abstention safety for apparent yield.

## Validation and remaining limits

The 14-scenario sync-equivalence oracle passed, including no-op, Python
body-only, Python interface-change, and project-context fallback cases. The
160-invocation concurrent metric test recorded all 160 denominators and all 160
savings events without loss. Full Rust tests, worker tests, formatting, Clippy,
repository guard, and diff-documentation guard are required before merge.

These results still do **not** constitute measured token savings. Actual
`MEASURED` savings require comparable host/provider token counts from a paired
baseline/treatment coding-session experiment. The next high-value performance
work is a bounded persistent or batched Python parser protocol; it is a larger
worker-boundary change and should be preregistered separately. The next query
work is conservative semantic normalization for general synonyms/paraphrases,
accepted only if the fixed corpus improves without weakening any safety counter.

## Reproduction commands

```text
cargo run --quiet --bin repo-guard -- product-eval --corpus src/fixtures/evaluation/query-corpus-v1.json --out <product-out> --repetitions 1 --bin target/debug/repogrammar --condition optimized_product
cargo run --quiet --bin repo-guard -- product-eval --corpus src/fixtures/evaluation/query-corpus-v1.json --out <baseline-out> --repetitions 1 --bin target/debug/repogrammar --baseline token-overlap
cargo run --quiet --bin repo-guard -- sync-equivalence --fixture src/fixtures/incremental_equivalence/v1 --all --bin target/debug/repogrammar --out <sync-out>
cargo test --lib read_path_benchmark_fixture_measures_bounded_query_paths -- --ignored --nocapture
cargo test --lib write_path_benchmark_fixture_measures_session_vs_per_record -- --ignored --nocapture
```
