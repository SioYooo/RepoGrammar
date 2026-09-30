# Measured efficiency sprint

- Date: 2026-09-30
- Status: fixed baseline measured; production optimization under qualification
- Consolidated checkpoint: `42cd665cd1d4ad44ce0a7d0541ba55b778400a44`
- Scope: indexing/process/context cost, bounded memory, query work and conditional adoption

## Frozen inputs and evidence ladder

Native within-machine runs use the retained release executable and Python
worker hashes in the local build-provenance record. The executable includes a
Tree-sitter 0.27 borrow-lifetime compatibility fix; the checkpoint SHA alone is
not a claim that the unmodified commit built. Synthetic fixtures, the exact
RepoGrammar checkpoint tree, and an already-authorized pinned public Python
corpus are distinct strata. No result from one stratum proves the others.

Primary resources are unwrapped native wall, user/system CPU and rusage maximum
RSS. RSS is not the simultaneous sum of all resident processes. A separate
observer pass counts actual frontend dispatches, bytes and AST operations;
its inclusive timers overlap and include observer overhead. They are not
subtracted from native wall or attributed to SQLite. Unobservable host phases,
Linux resources and agent runs remain explicitly unmeasured until executed.

Counter/native count-and-shape parity is diagnostic only, not full canonical
analysis equivalence. Production qualification requires the Python golden
outputs, complete persistent-analysis comparison and sync-equivalence oracle.

The strict full-generation A/B helper is
`python3 src/experiments/performance/compare_generations.py <old.sqlite> <new.sqlite> --out <comparison.json>`.
It opens read-only snapshots, checks integrity/foreign keys and hashes every
row of all fourteen admitted active owned tables, including family membership,
evidence, dependency/interface ledgers and constraint profiles. It excludes
clock/generation identity and SQLite planner statistics, emits only counts and
hashes, and rejects unknown tables. A sixty-second SQLite progress deadline,
512 MiB database cap and one-million-row per-table cap bound each comparison.
Fixed-size row hashes are sorted as a multiset after canonical JSON decoding;
raw SQL JSON-text order cannot change equivalence. Duplicate JSON fields fail
closed rather than silently using the last value. Five offline checks prove same-count fact
changes, family changes, invalid JSON, invalid foreign keys and missing active
generations cannot silently pass. Strict evidence IDs require identical full
indexing order; this is an additional frozen-transport check, not a replacement
for the semantic full-versus-incremental oracle.

## Observation and hypothesis before production changes

The first synthetic N16 run admitted 18 Python files (16 generated modules,
`conftest.py` and a test module), with 3,806 bytes of module source. Complete
indexing dispatched 18 document workers plus two configuration workers. It
transferred 68,508 module-source bytes: exactly 18 copies of 3,806 bytes, plus
18 copies of conftest context. The observer recorded 378 AST parses. Zero-delta
sync dispatched no workers and transferred no frontend input.

These are actual portable counters from a qualifying small context. The old
1 MiB input admission can omit all context on larger projects, so this is not
an unconditional claim that every project transmits N times its source.
Serialization of a context that is later omitted remains a separate host cost
to quantify. Initial query metrics had three harness defects (read-plan shape,
repetition checking and missing real-corpus path measurement); the original
artifact is retained. The fixed N16, N32 and self-dogfood runs completed.

The native baseline below is macOS/aarch64, release build, Python 3.14.6.
Synthetic resource values are medians over three repetitions, with maximum RSS
over those repetitions. Self-dogfood has one repetition, so it establishes a
baseline observation, not a variance estimate. CPU is user plus system time.
Individual samples, portable counters, producer/corpus hashes and unmeasured
fields are retained in the [baseline data](data/efficiency-sprint-baseline.v1.json).

| Corpus / operation | Wall seconds | CPU seconds | Max RSS bytes | Python dispatches | Frontend input bytes | Repeated module-source bytes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 18 Python files / fresh init | 1.10 | 0.92 | 42,074,112 | 20 | 104,735 | 68,508 |
| 18 Python files / full resync | 1.01 | 0.90 | 42,057,728 | 20 | 104,735 | 68,508 |
| 34 Python files / full resync | 1.83 | 1.63 | 42,074,112 | 36 | 362,335 | 252,348 |
| Self / fresh init | 22.03 | 21.00 | 331,759,616 | 176 | 106,308,055 | 94,980,160 |
| Self / full resync | 22.42 | 21.41 | 336,150,528 | 176 | 106,308,055 | 94,980,160 |
| Self / zero-delta sync | 1.86 | 1.84 | 15,630,336 | 0 | 0 | 0 |

Self is the exact archived checkpoint above, with 176 Python files and 539,660
module-source bytes. There were no context omissions. Its full resync sent
exactly `176 × 539,660` module-source bytes and parsed ASTs 32,912 times. The
synthetic 34-file case sent exactly `34 × 7,422` module-source bytes. These
finite counters prove amplification for the admitted contexts, without claiming
that all repository sizes remain below the legacy request cap.

Separate self-resync diagnostics recorded about 4.40 seconds loading workers,
6.63 seconds building module-symbol indexes (including 6.15 seconds in AST
parsing), and 12.21 seconds of total inclusive worker runtime. These overlap
and have observer overhead. Native filesystem/hash/serialization/storage/family
phase times remain `NOT_MEASURED`; the unassigned time is not called SQLite
time. This evidence selects startup and repeated project projection as the
bounded first optimization, while preserving the cheaper zero-delta path.

The candidate indexing mechanism is one explicit bounded project-session
worker, a context handshake, then file requests and deterministic EOS. Retain
only module/interface/re-export/conftest projections, not project source or
ASTs indefinitely. Preserve per-file legacy context omission, hashes,
timeouts, input/output caps, UNKNOWNs and failure-before-generation-activation.
Configuration/interface calls must be counted separately; a constant number
of those is not falsely described as one total subprocess.

## Predeclared production gate

Before editing the runtime, finish fixed N16/N32 counters and current native
baselines. Preregister the selected mechanism and risks against those results.
After implementation:

- Complete canonical parser and stored-analysis results must match the frozen
  transport baseline; compare stable values, not JSON formatting or counts.
- False-family selections remain zero; correct abstention cannot decrease;
  stale evidence, schema integrity and source-free output remain enforced.
- All sync-equivalence scenarios pass. Narrow invalidation is a separate slice.
- Repeated native runs on the same machine must improve the intended workload;
  source/process/context counters substantiate any complexity claim.
- Peak memory remains bounded under declared protocol/project/file limits.
  Reject an unbounded retention trade even if wall time improves. Report
  absolute measurements with their platform; no cross-machine guarantee.
- No new language, semantic provider, worker parallelism or production dependency
  is introduced by the optimization.

## Independently measured query direction

Current isolated self-dogfood has 497 families, 29,987 units and 53,838 family
evidence rows. Warm MCP observations show unsupported UNKNOWN around 1,095 ms
p50 versus an exact family around 454 ms. Separate SQL diagnosis places full
integrity and foreign-key validation far above summary retrieval; that SQL
diagnosis uses Python SQLite and is not the Rust serving-time oracle.

The immediate bounded query candidate is avoiding the complete unit inventory
when no indexed file or valid unit identity could resolve local context. Keep
all integrity, foreign-key, generation and source freshness checks. Do not add
a speculative index or cache final freshness verdicts. Profile the same frozen
index before/after and preserve configuration basenames, embedded unit IDs,
ambiguity, directory fallback and read-plan obligations.

## Adoption and consolidation boundaries

Historical PR #20 is not merged directly. Any replacement uses current
effective Codex/Claude paths, a short conditional global profile, exact marker
ownership, opt-out and reversible receipts. The full repository contract and
MCP preflight must not be weakened to shorten global guidance.

A0/A1/A2 confirmation requires isolated eligible/ineligible tasks and actual
correctness/read/call/token records. Dry runs do not prove adoption; no paired
token measurement means no token-saving claim. Credential/budget or corpus
availability blockers are reported rather than bypassed.

The separately authorized branch consolidation retains audited superseded
inventory history and historical release evidence. Remote cleanup happens only
after validated main is synchronized and each branch is contained or has an
explicit reviewed replacement. Existing unrelated detached worktrees remain
intact.
