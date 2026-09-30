# Adoption v2 offline runner qualification

Date: 2026-09-30. Parent: `6d398d9eac43cfdd7906d322a0326c91003465eb`.
Verdict: **OFFLINE_QUALIFIED** for `MACOS_TOOL_PROCESS` and development static
oracles. **CONTROL_ISOLATION_BLOCKED** for live native agents; adoption,
over-trigger, task effects, token/cost and savings remain **NOT_MEASURED**.
This closes the offline slice of the [milestone plan](../plans/v0.6-adoption-retrieval-release-plan.md),
not v0.6.0 release qualification. GitHub 0.5.0/npm 0.4.3 are unchanged.

## Preregistered scope and implementation

Use fresh B0-B3 profiles and environment allowlists; preserve identical neutral
task/source bytes; qualify denial before any model launch. Eight newly authored
development tasks have references and missed-work/wrong-analogue/convention or
safety mutants. Stop on a leaked control or surviving mutant. Paid budget is
zero. Historical RQ5 parser/ledger/regrade and frozen product queries are untouched.

- [`runner_v2.py`](../../src/experiments/agent_study/runner_v2.py) freezes five
  repetitions of eight tasks/four arms: 160 planned cells, seed `20260930`.
  Blocks are randomized then arms randomized within each block. B0/B1/B2 task
  bytes match; only B3 gets the frozen explicit-use prefix. Only B2 gets
  `HOME/.claude/CLAUDE.md`. No overrides/system-prompt substitute is used.
- The v4 artifact was compiled from the verbatim product formatter/constants.
  Its 825-byte SHA-256 is
  `5df98f0677189042c7ae58aa74d6e7ac90bd99324a18edf263f9be8db7b4b256`.
  The runner requires coordinator-reviewed bytes plus an exact expected hash;
  marker admission alone is not canonical-guide proof.
- OS controls run only the pinned Python runtime and `cat` under deny-default
  Seatbelt with narrowly admitted runtime/source/profile reads and source/tmp
  writes. `.repogrammar/` reads/writes are denied. Hidden oracles, other profile
  data and the product binary are outside the allowlist. No blanket Mach lookup
  or inherited host environment is admitted. Existing symlinks, multilink files
  and special files fail source admission.
- [`isolation_probe_v2.py`](../../src/experiments/agent_study/isolation_probe_v2.py)
  runs malicious filesystem operations inside that actual OS boundary.
  Owned SQLite connections close explicitly on success and failure.
  Read/Grep/Glob labels mean filesystem read/search/enumeration control routes,
  not a qualified installed CLI's native tool dispatch. Harness-owned sentinel
  SQL access succeeds outside the sandbox. It is not a production MCP bridge.
- [`oracles_v2.py`](../../src/experiments/agent_study/oracles_v2.py) grades parsed
  AST/text and complete changed-file scope without executing supplied code.
  Framework recipes intentionally reject arbitrary equivalent rewrites. A
  static recipe PASS is not runtime framework/auth correctness.

## Retained result

The [source-free summary](data/agent-adoption-v2-qualification.summary.json)
pins plan, product, guide, interpreter, Seatbelt, probe, oracle and runner hashes.
All worktrees, exact prompts, reference/mutant bundles, sandbox profiles and raw
logs are retained outside git under the local experiment directory
`/private/tmp/repogrammar-adoption-v2-20260930/`.

| Gate | Result |
| --- | --- |
| Native OS controls | B0/B1/B2/B3 each 22/22 PASS; sentinel indexes unchanged |
| Positive access | Source read, enumeration, write, shell and SQLite PASS |
| Index denial | Read/search/enumeration/recursive glob/shell/SQLite PASS |
| Escape and hidden data | Symlink/hardlink/rename, product read/exec, oracle/other-profile denial PASS |
| Environment/profile | Fresh fixed environment; B2-only actual Claude global path readable PASS |
| Network | Loopback bind/connect denied with permission errors PASS; no external request |
| Reference/mutant | 8/8 references PASS, 24/24 mutants FAIL as required |
| Screening | 116 historical statements/143 source files; zero 8-token or exact-AST overlaps |
| Portable tests | Seven oracle and five runner tests PASS; historical/current adapters preserved |

Initial enclosing-sandbox denial and subsequent Python runtime launch failures
are retained. Resolving the actual framework executable, admitting only its
runtime ancestors and classifying SQLite authorization denial fixed launch and
probe mechanics. No negative control was dropped. Full Rust tests initially
failed the existing native watcher test under the enclosing sandbox; the native
rerun is the required gate, with both logs retained.

## Remaining live gates

This is **DEVELOPMENT_BURNED** qualification. Zero lexical/exact-AST overlap is
not semantic independence; UNKNOWN's decision is explicitly specified in its
development prompt. The stale task freezes old/current source separately but
has no executed product stale index. Cells contain empty `.repogrammar/`
markers and planned MCP availability, not product indexes/configured hosts.
All task success/cost observations in the plan are null, never simulated zero.

Installed native agent flags/tool inventory, credential/service denial, actual
B2 global discovery and harness-owned MCP IPC still need one pinned host's
offline qualification. A new held-out corpus must demonstrate queryable family
support, independent blinded rubrics and stale/UNKNOWN setup before an approved
wave; do not repurpose these development recipes as held-out effects. Linux
native isolation is NOT_RUN. No agent CLI, credential lookup or model request
was executed. The runner has no live launch option.

Next action: qualify the pinned native-host/MCP bridge and real global-file
discovery boundary without granting a paid wave or weakening the isolation gate.
