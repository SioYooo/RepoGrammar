# Python frontend runtime admission

- Date: 2026-09-30
- Preregistered baseline: `27d4a1778d39413532df9b390f76b5bca24012a9`
- Scope: enforce the existing public Python 3.10+ host requirement
- Verdict: `VERIFIED_RUNTIME_PREREQUISITE` (unreleased source change)

## Observation and unresolved obligation

The public installation contract requires Python 3.10+, but
`PythonAstParser::run_worker_request` launches the configured interpreter
directly without admitting its version. All three private frontend modes share
this launcher: document parsing, project-configuration parsing, and interface
extraction. Version reporting is a separate syntax-boundary diagnostic; it
does not enforce the requirement. This lets an older interpreter produce
apparently valid output for old syntax even though it is outside the published
host requirement.

The bounded insight is to admit the interpreter in the same process before
loading the worker. This does not require a semantic provider, a dependency,
network access, another interpreter launch, or a version cache.

## Preregistered design and oracle

Use one Python `-c` bootstrap that checks `sys.version_info`: major must be 3
and minor at least 10. It rejects unsupported versions before loading the
worker, using a fixed source-free marker and exit code. The Rust adapter maps
only the exact marker/status pair to typed
`PythonFrontendInterpreterUnsupported`. Accepted interpreters dispatch the
same worker with the existing request, arguments, output cap, and deadline.
Remove the extra current-directory import entry introduced by `-c` before
importing bootstrap helpers, and restore the worker-directory import path.

Primary success requires independent executable regression tests proving:

- Unsupported 2.x, 3.9, and future non-3 major versions cannot dispatch a
  worker in any of the three modes. Document/config calls return the typed
  error; the interface probe remains `Unverified`.
- Supported 3.10+ dispatch remains one process per request, preserves worker
  arguments/stdin, and produces deterministic validated output.
- An exit-code lookalike without the exact marker remains a sanitized generic
  frontend failure; missing executables, malformed/oversized output, worker
  crash, and timeout fail closed.
- Existing application paths present one source-free recovery instruction and
  do not commit a replacement generation after admission failure.

Pre-change reproduction: wire the independent regression module plus the new
error variant while leaving the launcher unchanged, then run
`cargo test --workspace --all-features python_runtime_qualification_tests`.
Expected pre-change state: unsupported-runtime tests fail because the worker is
dispatched. Expected post-change state: those same tests pass with no worker
dispatch. A real installed Python 3.9 probe is auxiliary corroboration when
available; namedtuple-controlled interpreter wrappers are deterministic fixture
evidence, not evidence about every Python implementation.

## Bounds and failure semantics

The existing 1 MiB input, 2 MiB captured-output cap, concurrent stdin/stdout
handling, and 30-second default wall deadline remain authoritative. Admission
shares that deadline. Tests may use a smaller injected deadline to falsify a
blocked worker. Errors carry neither source, absolute paths, configured
environment values, nor raw stderr. The version diagnostic remains independent
and does not grant admission. Failed index/sync builds preserve the previous
active generation through the existing indexing transaction boundary.

No provider state changes in this slice. Pyrefly/Pyright remain
`not_integrated`; provider absent/stale/conflicting/timeout fixtures remain
their existing contract evidence, not newly exercised production semantics.
No interpreter acquisition occurs. Analyzed source and `setup.py` bytes are
parsed rather than executed by the default frontend.

## Non-claims and stop conditions

This closes only a runtime prerequisite, not ADR-0020 discovery gate 1 or
Python completion. Python remains 5/9 and `structural_substrate`; Top-20 stays
0/20. The selected host still caps parseable grammar, and Python 3.10 without
`tomllib` still abstains from TOML-dependent configuration facts. The check
does not pin an implementation, support every future grammar, install an
interpreter, or establish a provider sandbox.
Admission occurs only on an executed private frontend request; unchanged-delta
sync that needs no worker stays a no-op. No interpreter-change invalidation or
stored-fact requalification is added.

Stop on a failed safety/recovery invariant or a required check that cannot be
fixed in this narrow scope. Preserve all failed evidence; do not weaken the
test or increase support claims.

## Execution evidence

The [summary JSON](python-runtime-qualification.summary.json) records the exact
tested source hashes and local binary hash. Tests ran on an uncommitted snapshot
based on `27d4a1778d39413532df9b390f76b5bca24012a9`; that base SHA alone is not
claimed as the implementation or evaluation commit. The atomic commit containing
these hashes is the reproduction revision. No public release was rebuilt.

| Evidence | Result and boundary |
| --- | --- |
| Pre-change independent regression | 4 PASS / 3 FAIL; unsupported worker dispatch, replacement generation activation, and a BrokenPipe masking failure |
| Post-change independent regression | 10 PASS, including all private modes, one-process dispatch, source non-execution, poisoned current-directory import control, invalid output, resource bounds, interface refusal, and full/incremental/config rollback |
| Physical interpreters | Python 3.9.6 rejected; Python 3.14.6 admitted. 2.7/3.9/3.10/4.0 tuples were controlled simulations, not four physical runtime qualifications |
| Full Rust suite | 2,234 library + 183 product-binary + 1 doctest PASS; repository infrastructure suite also passed; 2 existing ignored benchmarks remain optional and were not run |
| Sync-equivalence | All 14 scenarios pass; incremental cases equal clean rebuilds, declared project/interface cases fall back |
| Current development query corpus | 93/110 matches, 17 mismatches; retrieval 30/47, abstention 36/36, context 27/27; zero false-family and abstention-gold selections. This is a fixture diagnostic, not an improvement or token-effect claim |
| Formatting, Clippy, worker/package/installer tests, guards | PASS with the supported interpreter selected |

### Reproduction commands

Choose an installed Python 3.10+ within 3.x; both the parent `python3` PATH entry
and `REPOGRAMMAR_PYTHON_EXECUTABLE` must name the intended supported runtime for
isolated evaluation. See the [required gate](../development/testing.md#required-local-gate).
The coordinator's final report retains the exact local interpreter selection;
private machine paths are not committed in this source-free evidence.

```text
cargo test --lib python_runtime_qualification_tests -- --nocapture
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
python3 src/workers/python/worker.test.py
node src/workers/typescript/worker.test.js
node src/npm/repogrammar.test.js
bash src/install/repogrammar-install.test.sh
npm pack --dry-run
cargo run --quiet --bin repo-guard -- check
repo-guard sync-equivalence --fixture src/fixtures/incremental_equivalence/v1 --all --bin <local-binary> --out <untracked-output>
repo-guard product-eval --corpus src/fixtures/evaluation/query-corpus-v1.json --repetitions 1 --condition python_runtime_admission --bin <local-binary> --out <untracked-output>
git diff --check
cmp -s AGENTS.md CLAUDE.md
```

### Failed attempts preserved

The first full gate exposed three old tests that hardcoded an unsupported
interpreter; their constructors now use the existing configured default, with
assertions unchanged. Native watcher execution failed twice in the sandbox but
passed unchanged outside it; the final full gate used that native environment.
The isolated payload harness initially selected Python 3.9 from the parent PATH;
the final command selected supported Python explicitly without changing the
harness or gold. Raw failures, successful logs, and evaluation JSON are retained
untracked outside the checkout; the committed summary carries their outcomes
and evaluation hashes.

The adversarial review independently passed 10 runtime tests, confirmed
original failure assertions and rollback semantics, and
required the supported-interpreter testing note. No unresolved material finding
remained. None of this closes gate 1, integrates a provider, changes support
counts, or measures agent/search-engine effectiveness.
