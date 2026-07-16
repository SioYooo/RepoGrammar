# Public Preview Dogfood Case Study

- Evidence date: 2026-07-16
- Baseline product commit: `73770e6964ba28b5ac1064552fbd722666c4de03`
- First remediation rerun commit: `dd689a4634d0dac4e4cce19d948d046441f99a5d`
- Product version: `0.2.0-preview.0`
- Baseline binary SHA-256: `e8b234a372033710fdb9ec18d1e3ba74679dbdbb5f1ae1aa6417ce2eb0b125a1`
- First remediation rerun binary SHA-256: `54fd8ca3a2db1823bef73fa68e6865b51f20cad132f081c25ef1f3567484de72`
- Host class: macOS arm64
- Protocol: `../experiments/v0.2-real-repo-dogfood.md`
- Machine-readable summary: `public-preview-dogfood.summary.json`

## Verdict

`BLOCKED_BY_PARSER_FAILURE`

The baseline candidate produced an honest, useful `PARTIAL_CONTEXT` result on
the controlled dynamic case, but it could not build an active index for either
RepoGrammar itself or the fixed public FastAPI/pytest repository. Both positive
cases stopped on the same sanitized `parser_internal_error` class, and an
immediate `sync` retry reproduced it. Their later `find`, `check`, and `stats`
commands correctly returned `FALLBACK_TO_CODE_SEARCH` with
`no_active_generation`; those fallbacks are truthful failure handling, not
successful dogfood.

The first remediation fixed bounded analysis of large Python modules and its
worker regression suite passed. Repeating the full public-repository matrix on
the same fixed upstream commit and an isolated home nevertheless produced the
same public outcome. A restored-after-use diagnostic build identified the
remaining root cause as a worker/host contract mismatch: a FastAPI
`include_router` fact carried seven assumptions while the host limit was four.
The current worker itself accepted the failing input with the complete 46-file
Python context, so this post-remediation failure is not evidence of a worker
startup or Python-syntax failure.

The first failing Python input in each positive baseline case was independently
accepted by CPython bytecode compilation with a disposable cache. The first
remediation rerun supplies the stronger root-cause evidence above; the public
CLI remained sanitized. No product code was authored on this documentation
branch: remediation commits were supplied by the coordinator and cherry-picked
for reproducible reruns.

## Results

| Case | `init` | `sync` | `find` | `check` | `stats` |
|---|---|---|---|---|---|
| RepoGrammar self-dogfood | error: `PARSER_FAILURE`; state created, no active generation | same failure reproduced | exit 2, `FALLBACK_TO_CODE_SEARCH` | exit 2, `FALLBACK_TO_CODE_SEARCH` | exit 2, no active generation; estimate unavailable |
| Public FastAPI/pytest repository | error: `PARSER_FAILURE`; state created, no active generation | same failure reproduced | exit 2, `FALLBACK_TO_CODE_SEARCH` | exit 2, `FALLBACK_TO_CODE_SEARCH` | exit 2, no active generation; estimate unavailable |
| Bundled dynamic/insufficient control | initialized generation 1: 3 files, 12 units, 66 semantic facts | incremental generation 2: 3 unchanged files, 0 parser attempts | exit 0, `PARTIAL_CONTEXT`, `InsufficientSupport` | exit 0, `PARTIAL_CONTEXT`; advisory conformance `UNKNOWN` | exit 0, 0 families, 16 typed unknowns |

The first-remediation public rerun produced the same command-level results as
the public row above: `init` and `sync` exited 2 with a sanitized parser error;
`find`, `check`, and `stats` each exited 2 with `no_active_generation`.

The dynamic control's `stats --unknowns --json` reported:

- 12 indexed code units and 66 semantic facts across 3 indexed files;
- 4 Python-family-eligible units, 0 families, and 0 family members;
- 16 typed unknowns: 2 blocking, 12 recoverable, 1 irreducible, and 1
  non-blocking;
- leading reason-code counts `FrameworkMagic: 9`, `DynamicImport: 2`, and
  `RuntimeDependencyInjection: 2`;
- abstention rate `1.0`, family support coverage `0.0`, high support risk, and
  high thin-wrapper/token-saving risk;
- `estimated_potential_token_savings: 0` with measurement kind `ESTIMATED`;
- `token_savings: null`, `token_savings_ratio: null`, and
  `measurement_status: no_paired_measurement`.

This is the expected conservative shape: RepoGrammar resolved one indexed
target and returned a source-free read plan, but it did not turn dynamic or
insufficient evidence into a family or conformance claim.

## Metric truth

No baseline or treatment agent session supplied token counts. The paired local
experiment recorder accepted baseline/treatment lifecycle commands, but no
`experiment-record` command was run. Its report preserved null totals and null
savings with `no_paired_measurement`; the empty local record was then purged.

Accordingly:

- measured token savings: `NOT_MEASURED`;
- paired experiment: `no_paired_experiment`;
- causal claim: not available;
- the dynamic case's zero potential saving is an `ESTIMATED` diagnostic only;
- parser failures and query fallbacks provide no token-saving evidence.

## Provenance and reproducibility

Each product binary was built from its recorded product commit, matched its
recorded SHA-256, and identified itself as `0.2.0-preview.0`. The self case used
a disposable standalone clone of the baseline commit. The public case used a
read-only clone of
`fastapi/full-stack-fastapi-template` fixed at
`4d3d5e92c1ea6b3fa0fab02c41124844ec45bca8` (240 tracked files). The negative
control copied the repository's checked-in `dynamic-unknown` fixture from the
same product commit and is not represented as external evidence.

Every command used a fresh temporary home and a case-local state-directory
name. The exact parameterized command matrix is in the protocol. Temporary
clones, indexes, logs, caches, and telemetry records are intentionally absent
from this commit.

## Limitations and release impact

- This is one macOS arm64 checkpoint, not multi-platform dogfood.
- The public repository was successfully cloned only after network access was
  authorized; its commit fixes the result independently of future network
  availability.
- The baseline parser failures and first-remediation worker/host contract
  failure prevent a truthful positive real-repository case study. They remain a
  release-candidate blocker until the contract is fixed and the same frozen
  public commit completes the command matrix.
- The experiment help/parser disagreement for `--project` is a usability defect
  discovered while checking paired measurement support.
- No live coding-agent session, source-read baseline, treatment run, or host
  token export was available, so there is no measured or causal token result.
- No GitHub release or npm publication claim follows from local dogfood.

The next highest-value action is to fix the parser internal error without
weakening typed failure behavior, then rerun this exact protocol on all three
frozen cases before presenting public-preview real-repository evidence.
