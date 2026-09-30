# Query serving efficiency

Date: 2026-09-30. Status: native A/B and payload parity measured; final gates pending.
This is separate from indexing and instruction adoption.

## Observation and selected mechanism

The frozen self index has 497 families, 29,987 units and 53,838 family evidence
rows. The initial eight-call warm baseline discarded two warmups per category.
Its default p50/p95 milliseconds were family 454.08/455.29, member
627.93/630.92, path 714.16/724.46, natural language 724.32/749.80, unsupported
UNKNOWN 1,094.94/1,114.92 and readiness 1,455.65/1,490.45. These are one-machine
observations, not cross-platform guarantees.

Separate Python SQLite diagnosis found whole-database integrity and foreign-key
checks much more expensive than family-summary retrieval. That diagnostic is
not a Rust serving-time decomposition. Keep both checks and source-hash
validation; it does not justify a speculative index or readiness cache.

The unresolved obligation is unnecessary local-context work: an unsupported
target loads all units, then searches file/unit combinations, even when no
indexed path or valid unit identity could resolve. The candidate reuses the
existing path matcher and the admitted `unit:{indexed_path}#` prefix as a
conservative prefilter. Impossible loci keep the original UNKNOWN and proceed
to directory fallback. Possible loci run the original resolver unchanged.

The portable regression proves identical UNKNOWN/recovery with zero unit reads,
where the old implementation reads once. Extensionless `Gemfile` and embedded
unit IDs retain hydration; lookalikes cannot create support. `Gemfile:1` remains
the old unresolved case. The query module's 151 tests passed, including existing
ambiguity, stale evidence, directory and source-free cases.

## Frozen-index comparison protocol

Run the same initialized snapshot with both release binaries, sequentially on
a quiet machine:

```text
python3 src/experiments/performance/warm_queries.py --bin <old-binary> --project <frozen-copy> --out <before> --repetitions 8
python3 src/experiments/performance/warm_queries.py --bin <candidate-binary> --project <same-frozen-copy> --out <after> --repetitions 8
```

Require equal active-index fingerprints, selected-target hashes and whole
payload hashes/bytes for every shared category/tier. Record all samples, native
CPU/RSS and producer hashes. Partial-frame/write deadlines, oversized frames
and first-bad/last-good content sequences have offline regressions. Reversing
JSON field order is not content drift. Alternating repetitions can characterize
machine variation; no absolute timing threshold becomes a CI gate.

Default, compact-standard and optional compact-minimal payloads use the existing
schema and byte/4 token estimator. Any existing tier difference belongs to that
tier, not this query patch. Preserve evidence identity, freshness, bounded read
plan, UNKNOWN/recovery and narrowing handles. No paired host token data means
no token-saving claim.

The corrected native A/B used eight calls per category/tier, with two warmups
excluded. Full index and selected-target fingerprints matched. Every default
and compact-minimal payload hash/byte count matched. The source-free span stub
is an object with an empty `spans` array, not a source-bearing payload; a live
harness smoke caught this shape assumption before measurement and a focused
negative still rejects actual spans or source-included flags.

| Target / default tier | Baseline p50/p95 ms | Candidate p50/p95 ms | Bytes, unchanged |
| --- | ---: | ---: | ---: |
| Exact family | 494.30 / 501.27 | 530.44 / 554.44 | 12,590 |
| Exact member | 695.52 / 707.60 | 698.79 / 778.61 | 12,759 |
| Exact path | 784.63 / 822.66 | 801.15 / 879.30 | 12,759 |
| Natural language | 790.99 / 819.32 | 833.01 / 938.24 | 13,098 |
| Unsupported UNKNOWN | 1,198.74 / 1,259.44 | 888.73 / 915.05 | 1,123 |
| Readiness | 1,601.74 / 1,772.27 | 1,749.68 / 1,806.19 | 1,696 |

Only the unsupported local-context lane changes production work. Other paths
show timing variation and have no optimization claim. The entire sequential
profile used baseline/candidate 76.95/74.55 CPU seconds and 280,723,456/
282,624,000 maximum RSS bytes. Default-to-minimal byte differences already
existed before this patch: UNKNOWN is 1,123/390 bytes and exact family
12,590/12,025 bytes in both arms. No byte or token saving is attributed to this
change. Individual samples and producer/index/payload hashes are in
[the A/B data](data/query-serving-ab.v2.json).

Full product-eval safety counters, sync-equivalence and repository checks remain
independent gates. A source-free response or fewer reads alone cannot prove
correctness or agent usefulness. The candidate is not accepted solely from its
unit tests or timing table.
