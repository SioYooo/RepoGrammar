# Build Week Zero-Friction Onboarding Plan

- Status: Active
- Date: 2026-07-16
- Competition deadline: 2026-07-21 17:00 PDT
- Authority: `../decisions/ADR-0026-zero-friction-onboarding-orchestration.md`
- Base: `origin/main@77514b35b9ca0919c9d667b682b67ae617f5ac0f`
- Intended implementation branch: `feat/zero-friction-onboarding`
- Maximum autonomous rounds: 5

## Mission contract

Deliver a code-complete, locally reproducible release candidate for this first
use journey:

```text
npx @sioyooo/repogrammar setup
-> one reviewed confirmation
-> live agent detection and reversible MCP wiring
-> repository init and active index
-> optional autosync
-> product MCP self-test
-> one natural-language question only when a native agent integration is ready
```

Before real GitHub Release assets and the npm package are published, the same
journey is exercised through the source binary, local release fixtures, and
`REPOGRAMMAR_BINARY`. Documentation must continue to distinguish that local
candidate from the published `npx` path.

The successful local label is `CODE_COMPLETE_RELEASE_CANDIDATE`. The label
`PUBLISHED_JUDGE_READY` additionally requires explicit maintainer authorization
and verified remote assets, npm publication, and a clean published-package run.

Allowed negative labels are:

- `BASE_BRANCH_BLOCKED`;
- `ARCHITECTURE_BLOCKER`;
- `EXTERNAL_RELEASE_GATE`;
- `PLATFORM_UNVERIFIED`;
- `UX_GATE_FAILED`;
- `SECURITY_OR_ROLLBACK_GATE_FAILED`;
- `ENVIRONMENT_FAILURE`;
- `INCONCLUSIVE`.

Stop on verified code-complete success, a maintainer decision boundary,
unpreservable safety/ownership guarantees, five exhausted rounds, or a
remaining step that requires an unauthorized external write.

## Scope freeze and non-negotiable boundaries

During this plan:

- add no language, framework, parser, semantic worker, or provider;
- do not continue Go, PHP, Swift, Ruby, Visual Basic, Delphi, Ada, Fortran,
  SQL, R, MATLAB, Assembly, Scratch, or other expansion work;
- add no GUI, dashboard, large TUI, editor plugin, cloud service, local LLM,
  embedding model, vector database, or OpenAI API dependency;
- add no production dependency unless existing Rust/std and repository helpers
  are demonstrably insufficient and an ADR is updated first;
- do not weaken `UNKNOWN`, `PARTIAL_CONTEXT`, evidence, freshness, provenance,
  compatibility, or source-hash checks;
- keep telemetry explicit opt-in and default-off; setup never implies consent;
- preserve the one-tool default MCP surface and pattern-family-first CLI;
- do not add forbidden top-level graph commands;
- do not make install create or delete `.repogrammar/`;
- do not expose source, absolute paths, repository names, symbols, query text,
  prompts, credentials, or raw errors in onboarding/telemetry output;
- do not overwrite or remove foreign configuration, receipts, repository state,
  or user changes;
- do not push, merge, tag, create a release, configure `NPM_TOKEN`, publish npm,
  or submit to Devpost without explicit maintainer authorization.

## Preregistered UX and truth gates

The following gates are fixed before implementation and must not be loosened in
response to results.

| Gate | Required evidence |
|---|---|
| Top-level discovery | default help is no more than 25 lines and visibly routes to `setup`, `find`, `doctor`, and `help --all` |
| Families discovery | default human `families` is no more than 15 lines, groups public roles, and hides cluster signatures |
| Query readability | default human `find` and `check` are each no more than 20 lines |
| Internal-field isolation | default human output omits `cluster_`, `query_pipeline:`, `query_candidate_family_ids:`, and follow-up route internals |
| Machine compatibility | existing JSON fields, canonical `UNKNOWN`, `PARTIAL_CONTEXT`, and MCP operations remain compatible |
| Setup confirmation | an interactive live run performs one final confirmation; `--yes` is required for noninteractive writes |
| Dry-run safety | setup dry-run causes zero observable machine, repo, daemon, receipt, telemetry, or config writes |
| Ownership safety | rollback removes only writes created and owned by the current setup attempt |
| Authority convergence | current owned integrations are skipped; internally consistent obsolete owned integrations refresh safely; foreign, malformed, or drifted integrations are preserved |
| Repository safety | a previous active generation survives index failure; a newly valid generation survives later autosync failure |
| Auto-sync readiness | PID allocation is insufficient; bounded PID+nonce lock ownership and child liveness prove readiness, while exit/refusal/timeout fail typed |
| Output truth | product self-test, native agent, repository index, auto-sync, and family evidence are separate facts; repository-only success emits no coding-agent question |
| Recovery consistency | status, doctor, setup, query, and MCP consume the same authoritative recovery decision |
| Product MCP | product `tools/list` exposes exactly `repogrammar_context`; a call returns family context, `PARTIAL_CONTEXT`, or typed `UNKNOWN` |
| Release-candidate integrity | local artifact and npm-wrapper fixtures verify checksums, platform errors, worker presence, and setup argument passthrough |

Primary evidence is automated end-to-end, integration, serialization,
line-count/leakage, clean-HOME, MCP product-binary, installer, and release-fixture
tests plus the full repository validation suite. Unit tests, option parsing,
golden strings, dry-run text, and fixture demos are secondary evidence. A
workflow file, documentation alone, one successful manual setup, mock-only MCP,
`npm pack --dry-run`, or a local archive cannot prove publication or the full
journey.

## Workstream and ownership map

| Workstream | Primary ownership | Required synchronization |
|---|---|---|
| Setup application orchestration | `src/rust/application/` | architecture overview/module map, CLI, installation, initialization progress |
| Agent integration delegation and receipts | existing install application/adapters | installation spec and installer tests |
| CLI parsing, confirmation, rendering | `src/rust/interfaces/cli/`, composition root | README, CLI spec, CHANGELOG |
| Recovery classifier | application policy/use-case boundary | product/CLI/MCP specs and consistency tests |
| MCP self-test and recommendation consumer | existing MCP/installer boundaries | MCP spec only if schema or error semantics change |
| npm and release passthrough | `src/npm/`, `src/install/`, release fixtures | installation spec, testing docs, release checklist/proof matrix |
| Demo and release evidence | docs and committed fixtures only | quickstarts, launch kit, reports, durable memory |

The CLI may parse, request confirmation, format, and serialize. It must not
reimplement install, lifecycle, readiness, or rollback policy. The npm launcher
may acquire and execute the binary with unchanged arguments; it must not
implement setup behavior.

## Round 1: baseline, ADR, plan, and branch gate

### Hypothesis

The current safe components can be composed without changing their ownership,
but the first-use surface and recovery recommendations are too fragmented and
verbose.

### Work

1. Protect unrelated worktrees and confirm the implementation base is the
   verified `origin/main` tree.
2. Record branch, version, guide equality, tag, release, and npm state.
3. Measure help, initialized and missing-state output, query leakage, installer
   dry-run behavior, and publication state.
4. Accept ADR-0026 and this plan before implementation.
5. Commit the baseline report and machine-readable summary.

### Completion gate

- baseline evidence records commands, exit status, bytes, lines, leak tokens,
  and external-publication boundaries;
- UX thresholds and failure taxonomy are preregistered;
- the branch is based on verified main and contains no unrelated implementation.

### Failure handling

Classify an unavailable or ambiguous base as `BASE_BRANCH_BLOCKED`; classify
network-only tag/npm checks separately as `ENVIRONMENT_FAILURE`. Do not infer
publication status from a failed lookup.

## Round 2: safe setup orchestration

### Hypothesis

One application use case can reuse current install, init/resync, autosync, and
self-test ports while preserving their independent state ownership.

### Work

1. Define typed setup plan, stage result, outcome, sanitized error class, and
   retained/rolled-back state types.
2. Detect repository root/state and live agent targets without writes.
3. Compose one plan with machine, repo, background, and telemetry sections.
4. Add CLI parsing and a single interactive confirmation.
5. Implement `--yes`, `--dry-run`, `--no-autosync`, JSON, and progress modes.
6. Delegate machine configuration and receipt creation through the install
   boundary.
7. Delegate repository activation through init/resync and optional autosync.
8. Run the bounded product MCP self-test and format one useful natural-language
   prompt only when a supported native integration is verified ready.
9. Reconcile `OwnedCurrent` and `OwnedOutdated` against the current executable
   authority through the install service, preserving foreign/malformed/drifted
   state.
10. Make reruns idempotent and implement current-attempt rollback that removes
   only newly configured targets, never reconfigured pre-existing targets.
11. Require bounded PID-plus-startup-nonce and child-liveness proof before
   auto-sync becomes ready.
12. Preserve family inventory as available-zero, available-positive, or unknown
   and render all setup limitations.

### Required scenarios

- clean setup and already-installed setup;
- already-initialized repository;
- explicit and automatic Codex/Claude target selection;
- missing selected agent and no live agent detected;
- foreign or malformed configuration;
- native CLI, receipt, index, autosync, and MCP self-test failures;
- dry-run zero writes;
- partial rollback and previous-state preservation;
- obsolete owned authority refresh followed by downstream failure;
- immediate auto-sync child exit, lock refusal, timeout, and successful
  readiness;
- family inventory query failure and no-agent repository-only output;
- rerun idempotency;
- index success followed by autosync failure retains the active generation.

### Completion gate

Targeted parsing, plan, transaction, rollback, idempotency, and integration
tests pass. Human failures state the failed stage, completed and retained state,
rolled-back state, and one next action without a raw internal error.

### Failure handling

If current application/port boundaries cannot express safe composition, stop as
`ARCHITECTURE_BLOCKER`. If rollback can touch state not created by the current
attempt, stop as `SECURITY_OR_ROLLBACK_GATE_FAILED` rather than weakening the
contract.

## Round 3: progressive help and compact human rendering

### Hypothesis

Default human output can become task-oriented without deleting advanced
commands or changing the established JSON contract.

### Work

1. Add compact top-level help and `help --all`.
2. Aggregate default `families` by language and public pattern role; retain full
   JSON and explicit advanced detail.
3. Render compact `find`, `check`, and `explain` from existing result types.
4. Translate human uncertainty without changing canonical machine tokens.
5. Make `PARTIAL_CONTEXT` explicitly a read plan and not a family claim.
6. Add automated line-count and leak-token assertions for successful,
   partial-context, `UNKNOWN`, missing, and stale paths.

### Completion gate

Every preregistered line cap and leakage assertion passes. Full help and JSON
remain reachable, parseable, and compatibility-tested.

### Failure handling

A cap failure or hidden required safety boundary is `UX_GATE_FAILED`. Fix the
rendering hierarchy; do not delete uncertainty or machine fields to make the
snapshot pass.

## Round 4: authoritative readiness and recovery

### Hypothesis

One application classifier can remove contradictory advice while preserving
entrypoint-specific presentation.

### Work

1. Define recovery inputs and typed action classes.
2. Route status and doctor readiness through the classifier.
3. Route query, setup completion, and MCP recommendation through the same
   decision.
4. Keep transport/schema changes backward compatible; invoke the MCP contract
   workflow if any field or error semantic changes.
5. Cover ready families, no state, no generation, stale evidence, unhealthy
   storage, active/unknown lock, autosync configured/running/stopped, and
   target-specific `UNKNOWN`.

### Completion gate

Cross-entrypoint tests prove the same underlying state has one action. Doctor no
longer claims all query/family evidence is deferred when active families exist,
and `query_ready` has the same meaning across status and doctor.

### Failure handling

Do not paper over inconsistent domain meanings in renderers. Classify an
unresolvable semantic conflict as `ARCHITECTURE_BLOCKER` and record the exact
state combination.

## Round 5: clean-environment journey, demo, and release candidate

### Hypothesis

The composed path can be demonstrated and packaged without a source rebuild,
while local proof remains clearly separated from publication proof.

### Work

1. Run setup in a clean temporary HOME with fake/native configurators and a
   deterministic FastAPI/pytest fixture.
2. Verify active generation, autosync state, product MCP `tools/list`, a useful
   family/read-plan call, and a safe `UNKNOWN` call.
3. Test npm `setup` argument passthrough via `REPOGRAMMAR_BINARY` and local
   release fixtures.
4. Verify checksum success/failure, missing asset, unsupported platform,
   bundled Python worker, exact packaged `version`/setup/product-MCP smoke,
   install/upgrade/uninstall, and receipt cleanup.
5. Verify workflow dispatch is build-only, tag publication fails before asset
   upload when npm credentials are absent, and GitHub assets stage before npm.
6. Update README, quickstarts, CLI/installation/initialization/MCP specs as
   behavior requires, CHANGELOG, demo script, launch kit, release checklist,
   install proof matrix, completion report/JSON, and project-state memory.
7. Run the complete validation matrix and perform a requirement-by-requirement
   final audit.

### Completion gate

All code-complete gates pass, no P0/P1 usability defect remains, every commit is
coherent, temporary assets are cleaned or recorded, and the completion report
truthfully labels external publication state.

### Failure handling

Use `PLATFORM_UNVERIFIED` for an untested platform and
`EXTERNAL_RELEASE_GATE` for tag/release/npm work awaiting authorization. A local
fixture success never upgrades either label.

## Validation matrix

Run targeted tests at coherent workstream checkpoints, then run the full suite:

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
python3 src/workers/python/worker.test.py
node src/workers/typescript/worker.test.js
node src/npm/repogrammar.test.js
bash src/install/repogrammar-install.test.sh
npm_config_cache=/private/tmp/repogrammar-npm-cache npm pack --dry-run
cargo run --quiet --bin repo-guard -- check
cargo run --quiet --bin repo-guard -- check-diff --base <verified-base> --head HEAD
git diff --check
git diff --cached --check
cmp -s AGENTS.md CLAUDE.md
```

Additional mandatory automated gates:

- setup option parsing and invalid combinations;
- one confirmation and noninteractive `--yes` enforcement;
- dry-run zero writes;
- idempotency, partial rollback, and existing-state preservation;
- missing agent, autosync failure, and product MCP self-test;
- top-help, families, find, and check line-count/leakage;
- JSON backward compatibility;
- recovery consistency across status, doctor, setup, query, and MCP;
- clean temporary HOME;
- npm setup-argument passthrough;
- release-fixture checksum and Python-worker presence.

No failing check may be disabled, skipped, ignored, or weakened. Record any
unrunnable check with its reason, impact, and failure class.

## Artifact and commit policy

Required committed artifacts are:

- ADR-0026;
- this plan;
- `../reports/build-week-usability-baseline.md` and `.summary.json`;
- `../reports/build-week-usability-completion.md` and `.summary.json`;
- `../demo/build-week-demo.md`;
- synchronized specifications, architecture, README, quickstarts, CHANGELOG,
  release documents, and `.agents/memories/project-state.md`;
- automated tests in documented `src/` paths.

Large archives, build output, temporary HOME directories, raw logs, and video
assets remain untracked. Reports record their path, hash where useful, and
cleanup state.

Use small Conventional Commits. A coherent sequence is:

1. `docs(ux): define zero-friction onboarding contract`;
2. `feat(cli): add safe setup orchestration`;
3. `feat(cli): simplify default human output`;
4. `fix(readiness): unify recommended recovery actions`;
5. `test(ux): add clean-environment onboarding coverage`;
6. `docs(release): add Build Week demo and release evidence`.

Every behavior commit includes its tests and affected canonical documents. Do
not commit a failed partial implementation as success. Do not push without
explicit authorization.

## Completion audit

The completion report must answer yes or no, with evidence, for every item:

- Can a new user understand only setup rather than install/init/autosync?
- Is there exactly one confirmation?
- Is dry-run proven to make zero writes?
- Is telemetry still off by default?
- Are machine and repository ownership separate?
- Does rollback touch only current-attempt RepoGrammar-owned writes?
- Do all four output caps pass?
- Are internal query pipeline, candidate identifiers, and cluster signatures
  absent from default human output?
- Are JSON/MCP `UNKNOWN` and `PARTIAL_CONTEXT` canonical semantics preserved?
- Do status, doctor, setup, query, and MCP agree on recovery?
- Does doctor accurately recognize active family/query readiness?
- Does the clean-HOME path pass?
- Does the npm wrapper pass setup arguments unchanged?
- Do local release fixtures contain the binary, Python worker, and checksum?
- Was language/framework/provider scope kept frozen?
- Are docs, tests, memory, CHANGELOG, reports, and commits synchronized?
- Did the full validation suite pass?
- Were unauthorized push, merge, tag, release, and publish actions avoided?

Any “no” prevents a completion claim. The final report distinguishes
implemented, automatically verified, manually verified, externally published,
and blocked work, then names one highest-value next action.
