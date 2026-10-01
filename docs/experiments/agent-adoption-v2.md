# Agent adoption v2: availability, global discovery and explicit steering

Status: OFFLINE_TOOL_BOUNDARY_AND_ORACLES_QUALIFIED / effects NOT_MEASURED. Verified 2026-09-30 against
`main@d981d57c6e662b52f62e3e915af7382c5c8ed8f7`.
No real-model request, credential access or external spend is authorized here.
The separately scoped native-host controls use a local scripted protocol fixture,
never a remote model or an authenticated profile.
This new protocol does not amend the historical [RQ5 design](agent-study-design.md),
[pilot](agent-study-pilot.md), ledger or regrade. Its historical 0/4 MCP adoption
is mechanics-only evidence, with real-profile/hooks contamination explicitly
recorded. It is not a current conditional-instruction result.

## Product identity and question

Primary question: does the current conditional global guide increase useful
early adoption relative to the same available MCP without that global guide,
without increasing ineligible invocation or reducing correctness?
Current source implements the 825-byte global v4 formatter under
[ADR-0054](../decisions/ADR-0054-default-conditional-agent-instructions.md).
Published GitHub 0.5.0 retains override-only wiring. Freeze one **source build**
for all treatment arms; do not describe B2 as a released-0.5.0 behavior test.
The repository's full v3 guide is different from this short global profile.

| Arm | MCP | Managed global guide | Task prompt |
| --- | --- | --- | --- |
| B0 | Absent | Absent | Common neutral task |
| B1 | Same pinned product | Absent | Same neutral task |
| B2 | Same as B1 | Exact current global v4 | Same neutral task |
| B3 | Same as B1 | Absent | Common task plus frozen explicit-use prefix |

B1 still receives ordinary MCP initialize/tool guidance. B3 is an efficacy
upper bound, never spontaneous adoption. Hash base and delivered prompts
separately. B0/B1/B2 delivered task bytes must match. Do not tune guide wording
after seeing results. The [v1 confirmation preparation](agent-adoption-confirmation.md)
has burned preparation fixtures; its twelve cells are not held-out evidence.

## Freeze and isolation gate

Before an execution wave, record a closed `agent-adoption-run.v2` manifest with:

- agent CLI version/executable SHA, exact model identity, supported flag surface;
- product source commit/tree, executable/build provenance and worker hashes;
- corpus repository commit/tree, seeded delta, initial worktree/index archive
  hashes and active generation; autosync off;
- task/oracle/rubric/split, permission policy, guide, MCP initialize/tool
  definitions, base/delivered prompt hashes;
- seed, exact randomized order, repetition, attempt, start/end timestamps and
  raw transcript/patch/oracle-log hashes.

Source-bearing raw artifacts stay outside the repository. Committed records
accept only closed fields/enums, hashes and numeric/null observations: no source,
prompt, answer, patch, raw tool text, arbitrary notes or private paths.
The current metrics adapter is one closed projection, not this complete runner
or record validator. Task success/rubrics remain a separate oracle obligation.

Use fresh HOME, CODEX_HOME, CLAUDE_CONFIG_DIR and XDG directories per run, with
an environment allowlist. Do not reuse `driver.run_claude` unchanged: it does
not set HOME, permits real-profile mode and inherits the host environment.
No hooks, plugins, skills, extra MCP servers, user memories, CodeGraph or
RepoGrammar repo-local guide may enter an arm. Source and index bytes must be
identical between arms. Product CLI is inaccessible to agents; the harness
launches only the allowed MCP arms by pinned absolute path.

**Uniform enforcement must deny direct index acquisition**, including Read,
Grep, Glob, shell and SQLite routes, in every arm while allowing harness-owned
MCP access. A transcript detector is diagnostic, not access control. Prove
denial with malicious control probes before any model run. Post-hoc exclusion
does not restore a contaminated control. If the tool permission/sandbox cannot
enforce this, classify `CONTROL_ISOLATION_BLOCKED` and do not execute.

B2 must exercise discovery of the actual global file. An override shadow,
append-system-prompt substitute or discovery-disabled mode changes the study.
Authentication in isolated config requires separately approved credentials and
budget; no copying credentials or fallback to real HOME. Recheck official CLI
paths/flags before freeze, rather than assuming historical versions remain valid.

## Tasks, order, failures and spend

Proposed first held-out wave: four supported convention tasks, two ineligible
documentation/exact-lookup tasks, one UNKNOWN and one seeded stale case.
Their **exact statements/oracles are not frozen yet**. Author reference solutions
and mutants first: positive reference must pass, missed/wrong analogue and
convention mutants must fail. Static gates remain static; they do not establish
runtime auth. Hide tests and analogue rubrics outside agent worktrees. Screen
against product-eval queries and burned pilot tasks; do not reuse tuned phrasing.

Proposed five repetitions per arm/task = 160 runs for one pinned agent; a second
agent is a separate 160-run confirmation wave requiring separate authorization.
Seed `20260930` randomizes arms inside `(task, repetition)` blocks, then
interleaves randomized blocks. Commit the generated order before execution.
The order seed does not control LLM sampling. Report within-cell variation,
compaction, truncation and parallel-tool ordering. CLI/model drift aborts a wave.

Predeclare run and cumulative cost caps and wall timeout after budget approval.
Missing cost blocks continuation; null is not zero. Preserve all charged usage
and every attempt. Retry only infrastructure failure before substantive tool
output, on a fresh run; retain the original. Wrong answer, non-adoption, timeout,
budget exhaustion and failed task receive no success-seeking retry.
Classify `SUCCESS`, `TASK_FAILURE`, `NO_PATCH`, `TIMEOUT`, `BUDGET_CAP`,
`AUTH_FAILURE`, `TRANSPORT_FAILURE`, `CLI_CRASH`, `PARSE_FAILURE`, `CONTROL_LEAK`
and `MODEL_DRIFT` separately. Exclusions and denominators are frozen before runs.

## Measurements and decisions

Record invocation/count, first invocation assistant-message ordinal and event,
invocation before first relevant broad acquisition, eligible adoption and
ineligible over-trigger denominators. Record distinct successful source Read
acquisitions, rendered read/search result bytes, failed-read bytes separately,
MCP result bytes, total tool calls and before/after broad actions. Also record
host turns/duration/input/output/cache tokens/cost; missing values null with
reason. Do not combine host turns with derived assistant-message counts.

Measure task success, convention compliance, missed/wrong/duplicate analogues,
candidate-as-proof misuse and UNKNOWN/stale handling through the frozen oracle
and blinded rubric. Mechanical unread-edit counts are syntactic diagnostics,
not semantic harm. Runs with malformed/unpaired/truncated tool results or
unscoped abstentions have incomplete measurement coverage, never implied safety.

Primary B2-B1: useful eligible adoption and invocation before relevant broad
read/search. Coequal constraints: ineligible over-trigger, task correctness,
convention compliance and safety. B1-B0 measures configured-product efficacy;
B3-B1 measures explicit steering ceiling. Publish unconditional outcomes and
both-success-paired acquisition/token comparisons. Pair by task; report exact
numerators/denominators, uncertainty and nondeterminism. No claims beyond the
frozen agents/models/tasks; 100% invocation is not the objective. No observed
over-trigger is an observed zero count, not proof of zero population risk.

## Offline current-contract adapter

Observation: the historical parser expects list-shaped `read_plan` and misses
nested selected-family identity. On the committed MCP pregolden it reports zero
items/no family, despite one item and a selected family. It also cannot scope
current stale omission paths. Historical parser and ledgers remain untouched.

[`adoption_v2.py`](../../src/experiments/agent_study/adoption_v2.py) reuses the
ordered tool extractor, admits current `product-schemas.v1`, and projects
`agent-adoption-metrics.v2`. It reads object-shaped plans/nested identity and
exact reason fields, including path-scoped stale omissions. It rejects duplicate
JSON keys, conflicting identity, malformed plans and nonnumeric/negative host
measurements. CLI input is bounded to 64 MiB; each MCP result to 2 MiB.

Broad actions are Grep/Glob, or Read without positive integer offset **and**
limit. This is a tool-specific heuristic, not all filesystem I/O. Bash remains
unclassified; shell index-keyword flags are distinct from component-scoped
Read/Grep/Glob index attempts. Successful Read results count as acquisition;
failed results or reads started before the abstention cannot clear its flag.
Repeated warnings count each later unread edit at most once per diagnostic.
First invocation turn is an assistant-message ordinal, not a host-reported turn.
Family identities are hashed in the exported projection; paths remain in memory.
This command never invokes a model, alters config or evaluates real adoption:

```text
python3 src/experiments/agent_study/adoption_v2.test.py
python3 src/experiments/agent_study/adoption_v2.py <local-transcript.jsonl> --root <isolated-worktree>
```

Offline tests cover the historical reproduction, the actual committed MCP
pregolden, UNKNOWN/stale unread edits, successful/failed/parallel reads, repeated
warnings, scoped omissions, malformed/duplicate output, id reuse, path/bounds
lookalikes, missing usage and source-free derived output. Scripted denial tests
prove the counter, **not sandbox enforcement**. Codex JSONL needs a separately
qualified adapter; Claude-only parsing cannot silently stand in for it.

Current adoption, over-trigger, source reduction, task correctness, token/cost
effects and B3 efficacy remain **NOT_MEASURED**. Execution is blocked on the
freeze/isolation/oracle gates and explicit human budget/auth authorization.

## Offline runner qualification

The separate [qualification record](agent-adoption-v2-qualification.md) and
[closed summary](data/agent-adoption-v2-qualification.summary.json) qualify
`runner_v2.py` for a **macOS tool-process** filesystem/environment/network
boundary and eight development task oracles. They do not qualify an installed
agent host. The runner has no live-execution switch, agent invocation or
credential path. It refuses existing/repository-local output, pins the reviewed
v4 guide hash, clears the environment, and freezes a deterministic 160-cell
B0-B3 order before running adversarial controls. Source/reference/mutant bundles
and exact prompts stay outside git. Only B2 gets the actual Claude global path;
the probe checks path readability, not native CLI discovery.

The eight newly authored tasks are DEVELOPMENT_BURNED after qualification.
Four are bounded framework recipes, two are ineligible, one is UNKNOWN and one
has distinct old/current source seeds. Eight references pass and 24 mutants
fail; static AST recipes do not prove runtime framework behavior or auth.
The lexical/exact-AST historical screen is diagnostic, not semantic independence.
The 160 cells have empty `.repogrammar/` markers, not prepared product indexes
or configured MCP hosts. A sentinel SQLite index is used only in OS controls.
The product binary hash is a denied-access probe identity, not a release gate.

Live execution remains `CONTROL_ISOLATION_BLOCKED` until pinned native CLI,
actual global discovery, controlled harness-owned MCP IPC, native-host hidden
tool/credential denial and a separate held-out task/oracle/index freeze pass.
Budget/auth authorization is still required. A Linux implementation/native
qualification is separate; macOS control success must not become a Linux claim.

## Native-host follow-up

The [2026-10-01 native-host record](agent-adoption-native-host-qualification.md)
starts the actual pinned Claude CLI against a deny-default macOS boundary and
`SCRIPTED_LOOPBACK` provider, with a nonsecret dummy key. It has no real-model
or credential mode. Builtin plugins/agents are explicitly disabled and checked
in the native init manifest; global memory discovery stays enabled. The harness
owns the product MCP process outside the sandbox and relays only bounded,
read-only MCP frames through one exact Unix socket.

Five bounded local rounds reached `CONTROL_ISOLATION_BLOCKED`. B0/B1 acquisition
denial, positive routes and named SecurityServer capability denial pass. Native
MCP initialize/tool-list succeeds, but the B1 tool call fails after transport
closure. B2/B3 are NOT_RUN; actual global discovery remains NOT_MEASURED. The
subsequent stdin/cleanup/inventory fixes pass local regressions but require a
fresh native producer qualification. A native CLI's terminal success is not
qualification success, and the fixture's synthetic usage/cost/tool choices must
never enter real adoption/effect denominators.
