# RQ5 Controlled Coding-Agent Study Design

Phase 7 (research-grade evaluation), RepoGrammar product-core.
Design lane: agent impact (RQ5). Read-only design artifact; no code changed.
Grounded at `feat/product-core` @ `c4099ff`.

## 0. Grounding, assumptions, and reconciliation notes

Verified against the repo at design time:

- MCP surface is one read-only tool `repogrammar_context` with operations
  `find_analogues | show_family | explain_deviation | check_conformance`,
  inputs incl. `mode`, `token_budget`, `include_source_spans` (explicit opt-in;
  metadata-only default), and adoption steering delivered via MCP `initialize`
  instructions plus the tool description
  (`docs/plans/v0.2-agent-adoption-read-displacement-plan.md`,
  `docs/specifications/mcp-api.md`).
- Agent wiring surfaces: `claude mcp add --scope user` / `codex mcp add`, or
  `repogrammar install --target claude-code|codex` (`docs/quickstart-claude.md`,
  `docs/quickstart-codex.md`). Managed instruction-file writes are live-deferred;
  this study deliberately uses **no** managed instruction block (see §3).
- A paired token-experiment recorder already exists and is unused for real runs:
  `repogrammar telemetry experiment-start --name <n> --experiment-mode
  record_existing|controlled_pair --session baseline|treatment
  --measurement-source host_reported|user_entered|documented_tokenizer`,
  then `experiment-record (--usage-json|--input-tokens/--output-tokens
  [--tool-tokens]) [--success]`, `experiment-stop/report/export/purge`
  (`src/rust/application/telemetry.rs`, `src/rust/interfaces/cli/mod.rs`
  ~5490). Its report requires matched mode/source/task-kind, computes
  `token_savings`, and gates `claim_validity: valid_for_product_claim` on
  `both_success`. The study adopts the same claim rule (§8) and can mirror each
  paired cell into this recorder with `--measurement-source host_reported`.
- Existing measurement conventions: `docs/experiments/product-core-baseline.md`
  + `.summary.json`; results schemas versioned and additive; committed-answer
  scoring; condition/baseline provenance fields; safety counters
  (`false_family_selections`, `selected_on_abstention_gold`).
- Telemetry/consent policy: no source text, paths, hashes, query text, prompts,
  diffs, or env in anything exportable; measured token-savings claims require
  comparable paired measurements (`.agents/memories/known-constraints.md`).

Assumptions (not verifiable in-repo; encode, do not treat as fact):

- Orchestrator-supplied current metrics (73-query corpus; retrieval 42 /
  abstention 25 / context 6; hit@1 21/42; mrr 0.500; false_family 0;
  selected_on_abstention_gold 0) are the working state; the committed baseline
  doc still records the older 17/43 numbers. Reconcile at study freeze.
- The "6 program task types" are not written anywhere in-repo; §2 proposes a
  concrete instantiation and must be reconciled against the program document
  before freeze. If the program's list differs, keep the §2 structure (tasks,
  oracles, splits) and re-map names.

---

## 1. Objective and claims under test

RQ5: does RepoGrammar's MCP context change real coding-agent behavior on real
repositories — correctness, context acquisition, efficiency, and safety —
relative to the same agent without it, with the treatment delivered **only**
through MCP configuration?

Primary preregistered endpoints (decided before any main-grid run):

- E1 (correctness): task success rate, full product (A3) vs baseline (A0),
  paired by task.
- E2 (context acquisition): bytes-read and files-read per run, A3 vs A0,
  **conditioned on both-success pairs** (efficiency claims are invalid where
  treatment correctness is worse — same rule as the telemetry recorder's
  `claim_validity`).
- E3 (safety): UNKNOWN-override count, stale-evidence-use count,
  index-peek count; descriptive with exact counts, no rate inflation.

Secondary: tokens, wall time, turns, MCP adoption rate (fraction of runs
that call `repogrammar_context` at least once before the first Read/Grep of a
family-relevant file), per-task-type breakdowns, A1/A2/A4 contrasts.

Forbidden overclaims: no "X% token savings" without both-success paired
evidence; no generalization beyond the pinned repos, pinned agent CLI + model
snapshot, and the 6 task types; abstention-driven fallback reads in treatment
arms are not "failures of RepoGrammar" but must be reported, not hidden.

---

## 2. Tasks: 6 program task types, concretely instantiated

### 2.1 Pinned repositories

Requirements: public; permissively licensed; inside v0.1/v0.2 official scope
(Python-first: FastAPI, pytest, SQLAlchemy, Pydantic; TypeScript Express/Jest
as the transitional substrate); big enough that context acquisition is
non-trivial (>=150 source files for the two app repos); families verifiably
present at the pinned SHA (verified by indexing the pinned worktree and
listing families **before** freeze — same calibration discipline as the query
corpus: no invented targets).

Proposed set (pin exact commit SHAs at freeze into `repos.lock.json` with a
worktree SHA-256 per repo, computed the same way as the harness
`fixture_version`):

| repo_id | Candidate | Why | Task types hosted |
| --- | --- | --- | --- |
| `py-app-large` | Netflix/dispatch | Large real FastAPI + SQLAlchemy + Pydantic app; many analogues per family | T1, T3, T4, T5, T6 |
| `py-app-template` | fastapi/full-stack-fastapi-template | Medium, canonical conventions, SQLModel/Pydantic + pytest | T1, T2, T6 |
| `py-lib` | fastapi-users/fastapi-users | pytest-heavy library; fixture conventions | T2, T5 |
| `ts-app` | hagopj13/node-express-boilerplate | Express + Jest, matches TS fixture families | T1, T2, T6 (one instance each) |

Candidates are proposals; the freeze step may substitute a repo if indexing the
pinned SHA does not yield the needed families (that check is mandatory and
recorded). Model-training contamination is unavoidable for popular repos; the
mitigation is task construction (below), not repo obscurity — see §10.

### 2.2 The six task types

Each type gets 3 concrete instances (18 tasks), each instance a tuple:
`{task_id, task_type, repo_id+sha, prompt_template_id, oracle_id, gold_refs}`.
Task statements are written fresh at freeze, never copied from the eval
corpus, and screened for paraphrase leakage against the 73-query corpus
(token-overlap screen: no task statement may share a normalized 6-gram with a
corpus query targeting the same family; reuse the corpus paraphrase-control
tooling from the RQ1 lane).

- **T1 — Analogue-guided feature addition.** Add a new endpoint/handler that
  must follow existing conventions. Instance sketch: "Add a `GET
  /tags/{id}/summary` endpoint to <module>, following how existing detail
  endpoints in this repo handle auth, service delegation, and response
  models." Oracle: hidden acceptance tests (route exists, status codes, auth
  behavior, response schema) + conformance rubric.
- **T2 — Convention-conforming test authoring.** Write pytest/Jest tests for
  a named existing function using the repo's fixture/factory conventions.
  Oracle: hidden meta-tests (their tests run green on correct code, red on a
  provided mutant) + conformance rubric (uses repo fixtures vs ad-hoc mocks).
- **T3 — Cross-cutting contract change.** Add/rename a field on a Pydantic/
  SQLModel model and propagate through schema, service, route, and tests.
  Oracle: hidden acceptance tests exercising every propagation point;
  missed-analogue count from the diff (measures exactly what
  `find_analogues` should help with).
- **T4 — Bug diagnosis and fix.** A seeded, framework-mediated bug (e.g. a
  dependency override or fixture wired inconsistently with the family norm),
  introduced as a small pinned delta commit on top of the base SHA. Oracle:
  hidden failing-then-passing test + scope check (diff touches only the
  culprit region).
- **T5 — Conformance refactor.** "This module deviates from how the rest of
  the repo does X; align it." The deviant is either naturally present
  (verified against the indexed families) or a pinned seeded delta. Oracle:
  behavior-preservation tests (existing suite green) + blinded conformance
  rubric against the family definition captured at freeze.
- **T6 — Repository comprehension / contract question.** "How are DB sessions
  provided to request handlers in this repo? Name the files and the rule new
  code must follow." Output is a written answer, no patch. Oracle: gold fact
  checklist (authored from the indexed families + manual verification),
  scored by blinded reviewers.

Seeded deltas (T4, some T5) are committed as patch files in the study harness,
applied deterministically at worktree creation; the same delta is applied for
every arm and repetition.

### 2.3 Splits

- **Dev/pilot tasks** (2, see §9) are burned: used to debug the harness,
  excluded from all reported results.
- **Held-out main grid** (18 tasks) is frozen after the pilot passes and
  before any main-grid run: the task file's SHA-256 is recorded in the
  preregistration block of the results doc. No task edits after freeze; a
  defective task is dropped (recorded), never repaired mid-study.
- No task may target a family used by the RQ1 dev split's tuned queries in a
  way that reuses tuned phrasings (paraphrase screen above).

---

## 3. Condition arms

The treatment is delivered through **MCP configuration only**. Everything else
— worktree bytes, task prompt, model, agent CLI version, permissions,
timeouts, HOME contents — is byte-identical across arms. Concretely, the only
per-arm artifact is the file passed to `--mcp-config` (claude-code) or the
`mcp_servers` table in the per-run `CODEX_HOME/config.toml` (codex).

| Arm | Name | MCP config content | Product binary served |
| --- | --- | --- | --- |
| A0 | baseline | empty server list (`{"mcpServers":{}}`) | none |
| A1 | +RepoGrammar-stable | `repogrammar` server | pinned **v0.2.2 stable** binary (pre-product-core) |
| A2 | +QueryV2-only | `repogrammar` server | pinned build with only the query-resolution upgrade active (ablation build/flags from the RQ1 condition plumbing lane); env vars carried inside the MCP server entry |
| A3 | full product | `repogrammar` server | pinned product-core binary at the Phase 7 freeze SHA; server-side source-span rendering **disabled** via server env (spans refused with the standard omission guidance) |
| A4 | +source-spans (optional) | same as A3 | same binary, span rendering honored when the agent opts in with `include_source_spans` |

Design rules that keep the treatment leak-free:

- **No managed instruction block anywhere.** No `repogrammar install`
  instruction writes, no CLAUDE.md/AGENTS.md mention of RepoGrammar in any
  arm. Adoption steering reaches the agent only through the MCP `initialize`
  instructions and the tool description — which are part of the product and
  therefore part of the treatment, not a confound.
- **Identical worktrees including the index.** `.repogrammar/` is built once
  per repo at freeze (pinned generation, autosync OFF), archived as a tarball
  with SHA-256, and unpacked identically into every arm's worktree — including
  A0. This makes file trees byte-identical; A0's lack of access is enforced by
  having no MCP server, and direct reads of `.repogrammar/**` in any arm are
  mechanically counted as `index_peek` events (§7) and handled by sensitivity
  analysis, not by making trees differ.
- **Product CLI not on PATH in any arm.** Recovery guidance that says "run
  repogrammar resync" is not executable by the agent; the conservative
  fallback (source reads) is the measured behavior. Consequence: after the
  agent edits files, MCP answers about those files go stale and must abstain —
  that is the product's contract and is measured, not patched around.
  (Incrementality benefits are RQ4's scope, not RQ5's.)
- `--strict-mcp-config` (claude-code) / isolated `CODEX_HOME` (codex) ensure
  no other MCP servers (CodeGraph etc.) exist in any arm.
- Arm A1 vs A3 isolates the product-core delta; A2 isolates the query-v2
  resolution upgrade; A4 isolates source-span read displacement. A4 runs only
  if budget remains after A0–A3 (it is the only arm whose benefit RQ5 cannot
  get elsewhere, but it is severable).

---

## 4. Fixed factors and how to fix them practically

Primary agent: **claude-code headless** (`claude -p`). Secondary
generalization agent: **codex exec**, run as a smaller confirmation wave
(A0 + A3 only), not fully crossed — cost control, §8.

Fixed and how:

- **Model + version.** claude-code: `--model <exact dated snapshot slug>`
  (never an alias like `sonnet`); record the slug in every run record. codex:
  `-c model=<exact slug>` in the per-run config; record. Freeze one slug per
  agent for the whole study; a provider deprecation mid-study aborts the grid
  rather than mixing snapshots.
- **Agent CLI version.** Pin the installed `claude` and `codex` binaries;
  record `claude --version` / `codex --version` per run; the harness refuses
  to run if the version differs from the lock.
- **Prompt.** One byte-identical task prompt per task, identical across arms
  and reps; stored in the harness keyed by `prompt_template_id`; the run
  record stores its SHA-256, never its text (§7). The prompt never mentions
  RepoGrammar, MCP, tools, or strategy; it states the task, the acceptance
  intent in product terms, and "produce a unified diff / final answer".
- **System-prompt surface.** Per-run throwaway `HOME` (and
  `CLAUDE_CONFIG_DIR` / `CODEX_HOME` inside it): empty global CLAUDE.md /
  AGENTS.md, no user memory, no plugins/skills. The pinned repos' own
  in-repo agent files are part of the repo at the pinned SHA and stay
  identical across arms.
- **Tool permissions.** claude-code: a fixed `--settings settings-study.json`
  with an explicit allowlist — `Read`, `Grep`, `Glob`, `Edit`, `Write`,
  `Bash(<pinned build/test commands>:*)`, plus `mcp__repogrammar__*`
  (the entry is inert in A0); denied: `WebFetch`, `WebSearch`, `Task`,
  `Bash(curl:*)`, `Bash(wget:*)`, `Bash(git push:*)`. Identical file in all
  arms. codex: `--sandbox workspace-write` with network disabled, same
  command allowlist expressed in config.
- **Network.** Denied for the agent's tools in all arms (the model API
  itself obviously excepted). MCP server is a local stdio process.
- **Timeout / turn budget.** Wall-clock 20 min per run enforced by the
  harness; `--max-turns 80` (claude-code); codex equivalent turn/time bound
  in config. A timeout is recorded as `outcome: timeout` (counts as failure),
  never silently retried.
- **Retries.** Only infrastructure failures retry (CLI crash, transport
  error, API 5xx before the first tool result): full fresh rerun, original
  logged `infra_failure` and excluded from analysis (count reported). Agent
  giving up, wrong answer, or timeout: no retry.
- **Temperature / sampling.** Not exposed by either CLI; treated as provider
  default and identical across arms; sampling noise handled by repetition
  (§5). If a future CLI flag exposes it, pin it and record it.
- **Context limits / compaction.** Same model ⇒ same context window.
  Auto-compaction cannot be disabled reliably in claude-code; the harness
  detects compaction events in the stream-json transcript and records
  `compaction_count` per run; runs are not excluded, but E2 gets a
  sensitivity analysis excluding compacted runs.
- **Worktree.** Fresh copy per run from the pinned SHA (+ seeded delta if the
  task defines one) + the pinned `.repogrammar/` tarball; `git init`-ed to a
  deterministic synthetic history so `git diff` works identically; SHA-256 of
  the pre-run worktree recorded and asserted equal across arms/reps.
- **Run order.** Interleaved and randomized over (task, arm, rep) with a
  recorded shuffle seed, so provider-side drift over calendar time cannot
  align with arms.

Invocation sketch (claude-code):

```
HOME=$RUN/home CLAUDE_CONFIG_DIR=$RUN/home/.claude \
claude -p "$(cat task-prompt.txt)" \
  --model <pinned-slug> --max-turns 80 \
  --mcp-config $ARM/mcp.json --strict-mcp-config \
  --settings settings-study.json \
  --output-format stream-json --verbose \
  > $RUN/transcript.jsonl
```

codex: `CODEX_HOME=$RUN/codex codex exec --json --sandbox workspace-write
--output-last-message $RUN/final.md "$(cat task-prompt.txt)" >
$RUN/transcript.jsonl` with model + `mcp_servers` pinned in
`$RUN/codex/config.toml`.

The exact flag names above must be re-verified against the pinned CLI
versions during the pilot (flag surfaces drift); the pilot exit checklist
(§9) includes this.

---

## 5. Repetitions and seed handling

- Agents are nondeterministic with no seed control. **N = 5 repetitions** per
  (task × arm) cell in the main grid; pilot uses 3.
- Nothing is intentionally varied across reps; each rep is a fresh worktree,
  fresh HOME, fresh session. Rep index and a run UUID are recorded.
- Analysis treats the task as the pairing unit: per-cell success = mean over
  reps; paired A-vs-A0 contrasts computed per task, then permutation-tested
  over the 18 task-level differences; bootstrap (10k resamples over tasks,
  stratified by task type) for CIs on rate differences; Wilson intervals for
  per-arm raw rates. Holm correction across the A1/A2/A3(/A4) family of
  contrasts on E1.
- Preregistered escalation rule: if the pilot shows within-cell success
  flapping (any cell with both success and failure across reps) in more than
  half of cells, raise N to 8 for the main grid **before** it starts; never
  raise N after seeing main-grid results.
- Provider prompt caching is left on (it changes cost, not behavior); token
  accounting therefore reports `input_tokens`, `cache_read_input_tokens`, and
  `cache_creation_input_tokens` separately, and E2's token metric is
  `total_billed_equivalent_tokens` plus the raw split — never cost USD as a
  primary metric.

---

## 6. Measurement mechanics (no source, no prompts persisted)

All metrics derive mechanically from (a) the agent CLI's structured
transcript (`stream-json` for claude-code; `--json` JSONL for codex), (b) the
harness's own observations (wall clock, worktree diff), and (c) the pinned
oracle commands. Raw transcripts are retained **locally only** in an
untracked archive for audit (they contain source excerpts by nature);
committed artifacts contain only the derived records below.

Per-metric mechanics:

- **Tokens / cost / turns / duration** — from the final `result` event of
  claude-code stream-json (`usage.input_tokens`, `usage.output_tokens`,
  `usage.cache_*`, `total_cost_usd`, `num_turns`, `duration_ms`); from
  codex `token_count` events. Source of record: `host_reported` (matching the
  telemetry recorder's `MeasurementSource`). If a field is absent, record
  `null` — never estimate into the same field.
- **Files read** — count of distinct `file_path` over `tool_use` events named
  `Read`, plus distinct files matched by `Bash` commands whose command string
  matches a read-pattern regex (`\b(cat|head|tail|less|sed -n)\b`); Grep/Glob
  calls counted separately as `search_calls` (they return matches, not file
  bodies).
- **Bytes read** — sum of UTF-8 byte lengths of `tool_result` content blocks
  for read-type tool calls (Read + read-pattern Bash). This is measured off
  the transcript in memory; the bytes themselves are not persisted.
- **Tool tokens** — two fields: `tool_result_tokens_estimated` = ceil(bytes/4)
  summed over tool results (labelled estimated), and the host-reported
  aggregate usage as the primary number. Estimated and host-reported are never
  summed or interchanged (recorder discipline: measurement sources must
  match to compare).
- **MCP usage** — for each `tool_use` named `mcp__repogrammar__repogrammar_context`:
  operation, mode, `include_source_spans` flag, and from the paired
  `tool_result` JSON: status (`ok|PARTIAL_CONTEXT|UNKNOWN|...`),
  `unknown_reason`, selected family id, read-plan item count, result byte
  size. Derived: `mcp_calls`, `mcp_unknown_count`, `mcp_first_call_before_first_read`
  (adoption), `read_plan_paths` (repo-relative paths only).
- **Safety: UNKNOWN-override** — event counted when: an MCP result for target
  T returns `UNKNOWN`/`InsufficientSupport`/`StaleEvidence` whose affected
  paths (from the result's read plan / target resolution) include path P, and
  the agent subsequently **edits** P (Edit/Write tool_use) with **no
  intervening read-type acquisition of P** (Read of P, or a Grep whose
  results include P). Purely syntactic over the transcript event sequence; no
  semantic judgment. Reported as a count plus the run list.
- **Safety: stale-evidence-use** — event counted when: an MCP result flagged
  `StaleEvidence` (or a span omitted for staleness) for path P is followed by
  an Edit/Write to P with no intervening fresh Read of P **after** the stale
  flag. Same mechanical pattern, keyed on the stale marker. (The seeded
  detector test in §9 verifies both detectors fire.)
- **Safety: index peek** — any Read/Grep/Glob/Bash touching `.repogrammar/**`
  in any arm. Counted; baseline runs with `index_peek > 0` trigger the E1/E2
  sensitivity analysis (exclude and re-report).
- **Correctness (mechanical gate)** — after the run, the harness snapshots
  `git diff` (patch stored locally untracked; only its SHA-256 and stat
  summary — files touched, insertions, deletions — go in the record), copies
  the hidden acceptance tests into a fresh oracle checkout, applies the
  agent's patch, and runs the pinned oracle commands in the repo's pinned
  environment (venv/npm lockfile from the pinned SHA; no network installs at
  run time — environments are pre-baked at freeze). Outcomes:
  `gate_pass | gate_fail | patch_no_apply | no_patch`.
- **Correctness (reviewer oracle)** — §7 below; recorded as a separate
  append-only record referencing `run_id`.
- **Efficiency** — wall time (harness clock), `num_turns`, tokens as above.
- **Compaction** — count of compaction markers in the transcript.

### Per-run record schema — `agent-study-run.v1` (append-only JSONL)

```json
{
  "schema_version": "agent-study-run.v1",
  "run_id": "uuid", "rep_index": 1,
  "task_id": "t1-dispatch-endpoint-a", "task_type": "T1",
  "repo_id": "py-app-large", "repo_sha": "<pinned>", "worktree_sha256": "<hex>",
  "seeded_delta_sha256": null,
  "arm": "A3", "mcp_config_sha256": "<hex>",
  "agent": {"cli": "claude-code", "cli_version": "x.y.z", "model": "<slug>"},
  "prompt_template_id": "t1a-v1", "prompt_sha256": "<hex>",
  "started_at": "RFC3339", "finished_at": "RFC3339", "duration_ms": 0,
  "outcome": "gate_pass|gate_fail|patch_no_apply|no_patch|timeout|infra_failure",
  "oracle": {"oracle_id": "t1a-tests-v1", "commands_sha256": "<hex>",
             "tests_passed": 0, "tests_failed": 0},
  "diff": {"sha256": "<hex>", "files_touched": 0, "insertions": 0, "deletions": 0},
  "tokens": {"source": "host_reported", "input": 0, "output": 0,
             "cache_read": 0, "cache_creation": 0, "total_cost_usd": 0.0},
  "context": {"files_read": 0, "bytes_read": 0, "search_calls": 0,
              "read_tool_calls": 0, "bash_read_calls": 0,
              "tool_result_tokens_estimated": 0},
  "mcp": {"calls": 0, "by_operation": {}, "unknown_count": 0, "stale_count": 0,
          "partial_context_count": 0, "spans_requested": 0, "spans_rendered": 0,
          "first_call_before_first_read": false},
  "safety": {"unknown_override": 0, "stale_evidence_use": 0, "index_peek": 0},
  "compaction_count": 0, "num_turns": 0,
  "transcript_path": "<local untracked path>",
  "notes": null
}
```

No prompt text, no source text, no patch text, no tool-result text is in the
record — only hashes, counts, repo-relative paths, and pinned public SHAs.
Committed artifacts: the JSONL ledger (append-only), the aggregate results doc
`docs/experiments/agent-study.md` + `.summary.json` (following the
product-core-baseline conventions), `repos.lock.json`, task/oracle/prompt
hash manifests. Transcripts and patches stay in the local untracked archive.

---

## 7. Reviewer oracle for patch correctness

Two-stage, mechanical-first:

1. **Mechanical gate (all patch tasks).** Hidden acceptance tests authored at
   freeze, stored outside every agent worktree, run post-hoc (§6). T2 uses
   meta-tests (agent tests must pass on correct code and fail on a pinned
   mutant). Gate failure is final: no reviewer can promote a gate-fail run to
   success.
2. **Blinded human review (gate-pass patch tasks + all T6).** What tests
   cannot decide: convention conformance (T1/T2/T5), propagation completeness
   beyond the tested points (T3), scope discipline (T4), factual completeness
   (T6).
   - Reviewers see: task statement, the diff (or T6 answer), and the frozen
     gold refs (family definition captured at freeze, gold fact checklist).
     They never see: arm, rep, transcript, token counts, or MCP output.
   - Presentation order randomized; arm stripped; diffs re-serialized
     uniformly so formatting artifacts cannot identify the arm.
   - Rubric per task type, 0–3 anchored scale (0 wrong / 1 works but violates
     conventions / 2 minor deviation / 3 conforming), plus binary
     `missed_analogue` flags for T3 keyed to the frozen analogue list.
   - Two independent reviewers; disagreement > 1 point → adjudication by a
     third; report Cohen's kappa. Success for E1 = gate_pass AND median
     rubric >= 2. (T6 success = all must-have gold facts present, no false
     claims; reviewers tick a checklist, not a vibe score.)
   - An LLM judge may pre-screen for reviewer efficiency but is auxiliary
     only: every reported number is human-decided; LLM-judge agreement is
     reported as a diagnostic, never substituted.

Reviewer records: `agent-study-review.v1` (append-only), referencing
`run_id`, with reviewer pseudonym ids, scores, checklist bits, adjudication
flag. No patch text in the record.

---

## 8. Cost and time estimate

Assumptions (stated, machine/provider-dependent): mean per-run usage on the
two app repos for a mid-size agent model ~150k–400k effective input tokens
(cache-discounted) + 10k–30k output; ~$0.40–$1.20 per run; use $0.75 planning
mean. Wall ~6–15 min/run, 20-min cap.

| Arm | Runs (18 tasks × 5 reps) | Est. cost | Est. serial wall |
| --- | --- | --- | --- |
| A0 baseline | 90 | ~$70 | ~15 h |
| A1 +stable | 90 | ~$70 | ~15 h |
| A2 +QueryV2-only | 90 | ~$70 | ~15 h |
| A3 full product | 90 | ~$70 | ~15 h |
| A4 spans (optional) | 90 | ~$70 | ~15 h |
| codex wave (A0+A3, 9 tasks × 3 reps) | 54 | ~$40 | ~9 h |
| pilot (§9) + detector seeds | ~16 | ~$15 | ~3 h |

Main grid A0–A3: **360 runs, ≈ $280–$330, ≈ 2.5 days at 4 parallel workers**
(parallel workers are safe: each run is a fresh worktree + fresh HOME; API
rate limits are the binding constraint). With A4 and the codex wave:
≈ $340–$440 total. Reviewer time: ~120 gate-pass diffs + 15 T6 answers × 2
reviewers × ~4 min ≈ 18 reviewer-hours. Budget gate: if pilot mean cost/run
exceeds $1.50, drop A4 first, then cut reps to 4 (preregistered order).

---

## 9. Minimal pilot (harness proof)

Scope: **2 dev tasks** (one T1 on `py-app-template`, one T6 on
`py-app-large`) × **2 arms (A0, A3)** × **3 reps** = 12 runs, plus 4 seeded
detector runs. Dev tasks are burned (§2.3).

Pilot must prove, with explicit pass criteria:

1. Arm isolation: pre-run worktree SHA-256 identical across all 12 runs of
   the same task; the only differing input byte-for-byte is the mcp-config
   file (assert by hashing every input artifact).
2. Transcript parsing: 12/12 runs yield a complete `agent-study-run.v1`
   record with non-null host-reported usage; flag-surface check of the pinned
   CLI versions (the §4 invocation works as written or is corrected and
   re-locked).
3. Oracle mechanics: hidden tests run green on the reference solution and
   red on the unmodified repo, for both dev tasks, in the pre-baked offline
   environment.
4. Safety detectors: 4 seeded runs — (a) a scripted fake-agent transcript
   with a stale-marked MCP result followed by an edit (must fire
   `stale_evidence_use`), (b) same with UNKNOWN + no fallback read (must fire
   `unknown_override`), (c) a run with a deliberate `.repogrammar/` read
   (must fire `index_peek`), (d) a clean treatment run (must fire nothing).
5. MCP behavior in anger: at least one A3 pilot run actually calls
   `repogrammar_context`; if 0/6 A3 runs call it, that is a **finding**
   (adoption failure) and the study proceeds — but the prompt/permission
   setup is first re-audited for accidental suppression.
6. Variance read-out: within-cell success flapping rate → apply the §5
   escalation rule before freeze.
7. Reviewer dry-run: both reviewers score the pilot T1 diffs + T6 answers;
   kappa computed; rubric ambiguities fixed before freeze.

Only after all seven pass: freeze tasks/prompts/oracles/binaries (hash
manifest), preregister endpoints in `docs/experiments/agent-study.md`, then
run the main grid.

---

## 10. Threats to validity

- **Training-data contamination.** Pinned repos are public and likely in the
  model's training data, deflating the context-acquisition contrast (the
  model may "know" the repo). Mitigations: prefer repo states newer than the
  model cutoff where possible; T3/T4/T5 instances built on seeded deltas the
  model cannot have seen; report per-repo results. Residual risk stated in
  the writeup; it biases **against** the treatment, which is the conservative
  direction for the product claim.
- **Treatment-side instruction surface.** MCP initialize instructions and the
  tool description reach only treatment arms. This is the product as shipped
  — but any comparison claim must say "RepoGrammar as configured", not
  "context quality alone". A2 vs A3 partially decomposes this.
- **Baseline index visibility.** `.repogrammar/` present in A0 worktrees;
  mitigated by `index_peek` counting + sensitivity exclusion, not by breaking
  worktree identity.
- **No resync path.** Product CLI off PATH means treatment agents cannot
  refresh a self-staled index; understates full-product benefit on long
  editing tasks. Deliberate scope split with RQ4; stated in claims.
- **Nondeterminism and compaction.** No seeds; N=5 with paired task-level
  analysis; compaction recorded and sensitivity-analyzed. Provider-side model
  drift bounded by pinned snapshot slugs and randomized interleaved run order.
- **Usage-metric asymmetry.** MCP tool results consume input tokens in
  treatment arms; token totals are therefore an honest net measure, but
  `bytes_read` excludes MCP result bytes by construction — report MCP result
  bytes as their own column so no arm's acquisition is hidden.
- **Reviewer bias / leakage.** Diff style can hint at the arm (e.g. edits
  clustered on read-plan files). Blinding + uniform re-serialization + kappa
  reporting; cannot be fully eliminated; stated.
- **Small n / multiplicity.** 18 tasks, 6 types ⇒ 3 per type: type-level
  conclusions are descriptive only; confirmatory claims restricted to the
  pooled preregistered endpoints with Holm correction.
- **Harness-defined safety metrics.** `unknown_override` and
  `stale_evidence_use` are syntactic transcript patterns, not semantic
  proof of harm; they are reported as defined counters with their definitions,
  never as "the agent acted unsafely" without the pattern definition attached.
- **Generalization.** One primary agent CLI + one model snapshot + 4 repos in
  the v0.1 scope. The codex A0/A3 wave probes agent-generality; anything
  beyond that is future work, not a claim.

## 11. Deliverables

- `src/`-side (implementation lane, not this doc): study harness as a
  `repo-guard` subcommand or documented tool under `src/`, per repository
  boundary rules; detector unit tests with the §9 seeded transcripts as
  fixtures.
- `docs/experiments/agent-study.md` + `agent-study.summary.json`
  (preregistration block, frozen hashes, results, per-arm tables, CIs,
  safety counters, sensitivity analyses, threat list).
- `repos.lock.json`, task/prompt/oracle hash manifests (committed);
  transcripts/patches archive (local, untracked, documented).
- Optional mirror of each both-success pair into
  `repogrammar telemetry experiment-*` (`controlled_pair`, `host_reported`)
  so the product's own recorder holds a real paired measurement.
