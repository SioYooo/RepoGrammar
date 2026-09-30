# Full-resync host-stage attribution

Date: 2026-09-30. Status: QUALIFIED_WALL_AND_COUNTERS; per-phase CPU NOT_MEASURED.
This is diagnostic instrumentation, not a product optimization or release.
Producer hashes, phase medians, counters and full-comparison counts are in
[the summary](data/host-stage-attribution.summary.json). Raw reports and failures
are retained outside git under the sprint evidence directory.

## Preregistered observation and mechanism

Observation: the previous efficiency harness measured worker costs and native
totals, but could not locate remaining host work. Progress events mix computation
and writes, so subtracting worker time or timing progress transitions is invalid.
Hypothesis: after Python sessions, remaining time may move to host construction
or persistence. Instrument existing boundaries before proposing an optimization.

The explicit, session-local collector uses std monotonic timers, exclusive nested
spans, fixed phase vocabulary and a16-frame depth limit. Cross-thread spans are
refused. Normal product paths use None; diagnostic off uses the same linked
pipeline/constructors with collection disabled. Parser/session operations delegate
unchanged. Source hash, UNKNOWN, EOS, validation, activation and rollback gates
remain authoritative. No production dependency or public product CLI/MCP/storage
schema was changed.

## Frozen corpus and run protocol

- Synthetic: committed performance template expanded to16 module files plus
  conftest/test/config/Rust/TSJS inputs; the exact expanded tree hash is recorded.
- Self: existing archive of42cd665cd1d4ad44ce0a7d0541ba55b778400a44.
- FastAPI backend: authorized4d3d5e92c1ea6b3fa0fab02c41124844ec45bca8;
  full tree5016677c09ce2c23534805cfb734d83523f83756, backend tree
  222b814d44ba89a121abf48b23d0987aff4f0f9e. No acquisition or corpus code execution.

Each corpus has three fresh off/on pairs, alternating order, plus independent
public CLI analysis parity. HOME/tool environments are isolated; TMPDIR is
propagated consistently. Python3.14.6, executable/worker/harness/comparer hashes,
condition, session request, timeout and architecture are recorded. Producers
are pinned before execution and rechecked each repetition. HEAD is metadata,
not a claim that uncommitted instrumentation was already part of that commit.

The optional TypeScript worker is absent. The existing default Cargo-metadata
adapter is supplied exactly as on the CLI; Cargo is absent from the isolated
PATH, so its unavailability remains typed UNKNOWN. No repository build, import,
plugin, application, setup script or network acquisition executes.

```text
PATH=<qualified Python-3.10+-tool-PATH> repo-guard host-stage-eval --bin <frozen-product> --worker <frozen-worker> --python-files 16 --repetitions 3 --condition host-attribution-final --out <outside-repo-output>
repo-guard host-stage-eval --bin <frozen-product> --worker <frozen-worker> --fixture <frozen-self-or-backend> --repetitions 3 --condition host-attribution-final --out <outside-repo-output>
```

The actual command options require spaces: `--python-files 16`, `--repetitions 3`.
All product/diagnostic/comparison subprocesses use existing timeouts and bounded
captures. Failed workspaces and a failure manifest are retained; incomplete data
are not zero. All14 active owned tables, declared generation metadata and
migrations are checked through read-only SQLite snapshots including WAL state.
This analysis oracle is not application runtime equivalence or MCP payload parity.

## Native totals: medians of three runs

| Corpus | Off wall s | On wall s | On user/sys CPU s | On peak RSS bytes | Logical rows / transactions / checkpoints |
| --- | ---: | ---: | --- | ---: | --- |
| synthetic_n16 | 0.21 | 0.22 | 0.16/0.04 | 41533440 | 1754/3/3 |
| self_frozen | 7.44 | 7.13 | 6.20/0.73 | 340852736 | 327358/165/3 |
| fastapi_backend | 0.37 | 0.38 | 0.30/0.06 | 42598400 | 9847/7/3 |

Native /usr/bin/time-l is a within-machine observation on macOS/aarch64.
RSS is the native child-rusage maximum, not simultaneous aggregate tree RSS.
Off/on differences include instrumentation/rendering and noise; the self on
median being lower does not mean profiling speeds up the product. No speed,
memory, battery or cross-machine claim follows. The final runs had no concurrent
build/test workload. Earlier compile-contendedN16 captures are qualification
only and excluded from these medians.

## Exclusive diagnostic wall: phase medians in milliseconds

| Phase | Synthetic N16 | Frozen self | FastAPI backend |
| --- | ---: | ---: | ---: |
| discovery | 12.261 | 52.481 | 12.301 |
| hashing_fingerprinting | 0.038 | 6.565 | 0.144 |
| frontend_context_preparation | 1.749 | 46.836 | 5.314 |
| analyzer_execution | 175.503 | 1485.898 | 260.486 |
| fact_normalization | 1.017 | 365.319 | 5.748 |
| family_construction | 2.485 | 2296.819 | 5.255 |
| sqlite_persistence | 17.351 | 2307.796 | 69.456 |
| integrity_foreign_key_validation | 3.397 | 559.590 | 14.122 |
| generation_activation | 0.479 | 0.473 | 0.479 |
| other | 0.985 | 21.941 | 1.665 |

Every individual run verifies exact root-span partition and nonnegative
unassigned time. Medians of components need not sum to the median total.
Analyzer time includes owned request preparation/serialization, worker start,
wait and response validation, not pure AST execution. Context includes source
reads/project preparation excluding nested hashing/analyzer spans. Persistence
includes adapter validation/serialization/batching/checkpoints, not pure SQL or
disk time. Validation is the generation-level integrity/FK/evidence boundary.
Phase CPU is unavailable and is never allocated proportionally from total CPU.

On frozen self, persistence and family construction each consume about2.3s
of the roughly7.1s profiled full pipeline; analyzer is about1.5s and validation
about0.56s. These host phases are material. On the Python-heavy backend and
small synthetic corpus, the analyzer boundary still dominates. Neither a
universal SQLite bottleneck nor pure-parser CPU dominance follows.

## Counter scope and falsification

Hash counters count actual SHA calls/input bytes; context counters count bounded
source reads/bytes. Discovery counts visited entries, including exclusions.
Analyzer counts actual parser/provider/session operations, including handshake
and EOS. Normalization counts actual record visits, not unique facts or a DTO
allocator high-water. Family counts constructed claims, not all candidate pairs.
Write-session stats count admitted logical rows, transactions and checkpoints;
they do not enumerate every internal/cascade/generation-metadata SQLite write.
Undefined bytes/work are null. More granular candidate/member/deriver operation
and pure-SQL attribution remains follow-up work; do not infer it from these totals.

Before accepting timings, the full oracle caught a real harness composition
defect: self public output had47107 facts versus47106 in the diagnostic arm,
with one evidence row and two dependency rows different. The omitted default
Cargo-unavailable UNKNOWN caused it. The adapter was restored; the oracle was
not weakened. The failed manifest/databases remain retained. Final9 off/on and9
public/on comparisons all pass14 tables. This frozen-self command is a
reproduction gate against suppressing that default context again.

Independent checks cover nested accounting, disabled collection, error unwinding,
cross-thread refusal, admission/bounds and preservation of a previous active
generation on parser timeout. Existing capture/comparer tests cover process
timeouts, inherited pipes, malformed resources and noncanonical table mutation.
Full product/sync/payload gates remain separate. No false-selection, abstention,
support, freshness or public-output relaxation is authorized by this measurement.

## Decision

Attribution is qualified for wall/counters on these frozen inputs. Per-phase
CPU, Linux-native qualification and real-agent efficiency remain NOT_MEASURED.
No production optimization is implemented. A future measured optimization may
target persistence/family cost, with complete canonical and sync-equivalence
gates; the [adoption protocol](agent-adoption-v2.md) preserves the separate
unmeasured agent-efficiency obligation.
