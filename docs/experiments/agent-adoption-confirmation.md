# Conditional instruction adoption confirmation

Date: 2026-09-30. Status: PREREGISTERED_PREPARATION / NOT_MEASURED.
This is an unreleased source-build study under
[ADR-0054](../decisions/ADR-0054-default-conditional-agent-instructions.md).
No agent, credential lookup or paid request has been run by this preparation.

## Hypothesis and frozen cells

Compare A0 (no RepoGrammar MCP), A1 (MCP, no managed global file), and A2
(the same MCP plus the short conditional global profile). A1 still receives
the product's ordinary MCP initialize/tool guidance; it is not an instruction-
free agent. Freeze two convention tasks (route registration and fixture name),
one documentation-only edit, and one exact function lookup. Tasks, mechanical
oracle fields, order seed `20260930`, and one repetition per cell are in
[`adoption_prepare.py`](../../src/experiments/agent_study/adoption_prepare.py).
The 12 cells are a bounded confirmation pilot, not a population effect estimate.

Fixtures reuse committed Python release route/pytest examples. Every arm has
identical source/tree hashes. The A2 profile lives outside that source worktree
in its isolated user profile. Before execution, build one index and copy the
same pinned index into every arm, including A0; freeze binary/worker/index and
instruction hashes. Verify queryable fixture families without changing tasks
or oracle after observing adoption.

## Offline preparation

```text
python3 src/experiments/agent_study/adoption_prepare.py --selftest
python3 src/experiments/agent_study/adoption_prepare.py --out <new-outside-repo-dir> --instruction-file <coordinator-reviewed-v4-guide>
python3 src/experiments/agent_study/selftest.py
```

Preparation refuses repository-local/nonempty workspaces and requires and bounds the coordinator-provided
instruction input to 1,024 bytes. It records hashes, explicit execution blockers
and null NOT_MEASURED observations. A v4 marker/size check is input-shape
admission only; the coordinator must verify the input equals the install
authority's exact `global_managed_instruction_block` before any A2 run. The
preparer launches no subprocess or agent and has no live-execution switch.
It does not fabricate zero adoption or token counts.

## Execution and measurement gate

An authorized future runner should reuse the existing `driver.run_claude`
budget/timeout mechanics, `parsers.parse_transcript`, safety detectors,
`record_schema` privacy validation and `treehash`; do not run the old pilot
driver unchanged. It clones a repository, rewrites its lock, assumes A0/A3,
and permits real-profile contamination. Recheck current CLI flags before runs.

Require an explicit budget, exact model/CLI version, per-run cap and cumulative
abort threshold, bounded wall time, and verified authentication isolation.
The current Claude `--bare` disables memory discovery but also OAuth/keychain
auth; an isolated `CLAUDE_CONFIG_DIR` cannot be assumed to reuse a host login.
Do not copy or log credentials. A real-login run with memory disabled and A2
append-system-prompt can test instruction-content steering only; it cannot prove
global-file discovery. Treat that as a separate protocol, not silent substitution.

Record eligible adoption numerator/denominator and ineligible over-trigger
numerator/denominator separately, plus source files/read-result bytes, MCP calls
and result bytes, total tool calls, task oracle, wall time, and actual host token
and cost fields when present. Missing host data remain null. Read-result bytes
are a tool-observation measure, not total filesystem I/O. Parse source-free
records only; raw transcripts, source outputs and worktrees stay untracked.

The predeclared useful result requires greater eligible adoption and lower
source-reading work in A2 vs A1, no ineligible MCP calls, and no task correctness
regression. Any failed oracle disqualifies a savings claim. Do not tune
instruction wording, tasks or corpus after results. One repetition cannot
support causal/generalized improvement or expected token savings. Preserve
negative results, auth/network/environment failures and exact stop reasons.

Current outcome: all adoption, over-trigger, correctness, source-work and
token/cost comparisons are **NOT_MEASURED**. Offline selftests validate only
preparation/isolation and retained parser machinery.
The maintainer explicitly selected offline-only validation for this sprint;
there is no authorization to start paid or authenticated confirmation runs.
The coordinator prepared all twelve cells using the actual 825-byte v4
formatter output, compiled verbatim from the checked-in constants/formatter.
All source worktree hashes match across arms. The frozen
[plan](data/agent-adoption-plan.v1.json) contains null measurements rather than
simulated results; it is preparation evidence only.
