# Interleaved one-file body comparison

Status: native coordinator comparison measured; no regression claim from coarse cohorts.
Date: 2026-09-30. Scope: the preregistered synthetic N16 expansion only.

The coarse body-edit measurements appeared slower in the candidate. This
protocol isolates that observation without waiving the incremental acceptance
gate or inferring a regression from measurements made at different times.

[`incremental_pair.py`](../../src/experiments/performance/incremental_pair.py)
copies the exact six committed performance fixture files into two isolated
temporary repositories and creates `module_0001.py` through `module_0015.py`
from the same `app.py` bytes, matching `performance_prepare_synthetic`. This
is 16 route modules plus conftest/test, or 18 Python files. Unexpected files,
symlinks or per-template files over 64 KiB refuse qualification.

Initialize both outside timing with no autosync and isolated HOME/tool paths.
Baseline explicitly disables project sessions; candidate enables them. Run two
warmup pairs, then nine measured pairs. Toggle only the first `return []` in
`app.py` to/from `return [1]`; alternate AB/BA order. Native `/usr/bin/time`
wraps the actual binary directly, never a shell wrapper. Each command is bounded
to 30 seconds; group termination and bounded output reuse `autosync_eval`.

Every call must report the product indexing status `complete` and remain
incremental, with one modification, one reparse and no
fallback. After each pair, outside timing, require complete active-owned SQLite
fingerprint equality via `compare_generations`. Initial canonical equality and
producer/fixture/Python hash stability are required too. A same-count fact
change cannot pass. Filesystem bytes read and worker spawns are explicitly
NOT_MEASURED in this uninstrumented native pass.

Source-free `incremental-pair-results.v1` contains input/producer hashes,
portable product counters, all native resource samples, canonical parity hashes
and paired median deltas/ratios. A zero baseline value produces a null ratio.
No machine-dependent threshold is a CI assertion. Outcomes are MEASURED or a
stable FAIL reason; failed samples and raw native files remain inspectable
outside the repository. Source edits, init and database comparison time are
excluded from resource measurements; RSS remains native rusage maximum,
not a simultaneous process-tree sum. No agent, network, credential, global
configuration or live repository index is used.

```text
python3 src/experiments/performance/incremental_pair.test.py
python3 src/experiments/performance/incremental_pair.py --baseline-bin <frozen-old-binary> --baseline-worker <frozen-old-worker> --candidate-bin <frozen-candidate-binary> --candidate-worker <frozen-candidate-worker> --python <qualified-interpreter> --out <new-outside-repo-directory>
```

Run native measurements only in the coordinator's quiet window, with macOS
resource permissions where needed. Report relative within-machine observations
and parity separately. No body-path improvement is claimed by this protocol.

The coordinator's nine measured pairs completed with full active-owned
analysis equality after every pair. Baseline/candidate wall medians were both
0.49 seconds; median paired difference was -0.01 seconds. CPU medians were
0.42/0.43 seconds, but median paired difference was zero and ratio 1.0. Native
time has 0.01-second reporting granularity. No repeatable body regression was
observed under this interleaved Git-repository protocol; this does not prove a
body-speed improvement. Median RSS was 41,910,272/41,369,600 bytes. Earlier
separate-cohort rows remain retained rather than being relabelled as paired.
See [all samples and parity hashes](data/incremental-body-pair.v1.json).
