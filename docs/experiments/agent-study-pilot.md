# RQ5 Agent-Study Pilot — Harness Proof

Status: PILOT (harness-proving subset). N=2 per cell proves harness
**mechanics only** — this document makes **no** effect claims about RepoGrammar's
impact on agent behavior. Grounded on `feat/product-core` @ `4c872f1`.

This pilot implements the minimal harness-proving subset of the Phase 7
agent-study design. The design-lane authority documents are committed alongside
this pilot: `docs/experiments/agent-study-design.md` (RQ5 primary),
`docs/experiments/phase7-synthesis.md` (RQ1–RQ5 synthesis / conflict
resolutions), and `docs/experiments/phase7-eval-methodology.md` (RQ1–RQ4). It
exists to prove the
measurement machinery — arm isolation, transcript parsing, safety detectors,
mechanical grading, per-run records, and cost accounting — before any authorized
full-grid spend. The full grid ($280–330) is **not** run here.

## 1. Scope and deviations from the design

| Dimension | Design (full grid) | This pilot | Why |
| --- | --- | --- | --- |
| Repos | 4 pinned repos | ONE: `full-stack-fastapi-template` | minimal harness proof (brief) |
| Task types | 6 types × 3 instances | 2 types × 1 instance (T1, T6) | two simplest mechanically-gradable types |
| Arms | A0/A1/A2/A3(/A4) | A0 (baseline) vs A3 (full product) | isolates the product-core delta |
| Reps | N=5 | N=2 (8 runs) | proves mechanics, not statistics |
| Agent | claude-code + codex wave | claude-code headless only | codex skipped in pilot (see §7) |
| Budget | ~$280–330 | HARD cap $30, abort at $27 cumulative | user authorization |
| Model | mid-size dated snapshot | `claude-haiku-4-5-20251001` | cheapest exact dated snapshot; capability irrelevant to proving mechanics |
| Turn cap | `--max-turns 80` | (removed — see §4) | flag no longer exists in the pinned CLI |
| HOME isolation | throwaway HOME + API key | real config for auth (see §4) | keychain credential is coupled to the default config dir on this host |

Where the design and the brief conflict, the brief's **budget** and **scale**
win; the design wins on arms/metrics/detectors/record schema.

## 2. Harness architecture

All harness code is Python 3 stdlib only (no new production dependencies), under
`src/experiments/agent_study/` (automation tooling under `src/`, per the
repository boundary rule):

- `treehash.py` — deterministic worktree tree hash, **byte-for-byte equivalent**
  to the Rust `fixture_version_hash` (`src/rust/bin/repo_guard.rs`): SHA-256 over
  files sorted by UTF-8 path bytes, each fed as `u64le(len(rel)) ‖ rel ‖
  u64le(len(bytes)) ‖ bytes`, symlinks skipped. Equivalence is proven, not
  asserted: `tree_sha256(src/fixtures/python/release/v0_1)` reproduces the
  Rust-committed `python-v0_1` hash `37fec96f7c7b…` recorded in
  `docs/experiments/product-core-baseline.md`.
- `parsers.py` — parses claude-code `--output-format stream-json` transcripts
  into ordered tool events + the metrics of design §6 (tokens/cost/turns from the
  final `result` event; files-read and bytes-read from Read/read-pattern-Bash
  tool results; MCP operation/status/read-plan/adoption).
- `detectors.py` — the three syntactic safety detectors (`unknown_override`,
  `stale_evidence_use`, `index_peek`), defined purely over the transcript event
  order (design §6). Reported as defined counters, never as "the agent acted
  unsafely" without the definition attached.
- `grader.py` — offline mechanical acceptance: T1 = static regex assertions over
  the post-run worktree (verdict gate_pass/gate_fail/no_patch); T6 = gold
  must-have fact checklist over the agent's final answer text.
- `record_schema.py` — the `agent-study-run.v1` record, a privacy guard (no
  absolute private paths; `transcript_path` is a basename only), and an
  append-only JSONL writer. Extended additively (2026-07-18, review S5) with
  `mcp.result_bytes` (design §10 — MCP result bytes as their own column, kept
  out of `context.bytes_read`), `mcp.read_plan_item_count`, and
  `mcp.selected_family_ids` (design §6); the 8 as-run records predate these keys
  and a v1 reader treats missing keys as null.
- `driver.py` — orchestrator: preflight, clone+pin, one shared `.repogrammar`
  index tarball (identical in both arms), per-run worktree snapshot, arm MCP
  config, claude launch (budget-capped + wall-clock-bounded), grade, record, and
  cumulative cost abort. Refuses a `--work-base` inside the repo tree (review S6)
  so raw transcripts / per-run worktrees never enter the repo.
- `regrade.py` — zero-spend re-derivation of every record's verdict/metrics from
  the saved transcripts (+ saved worktree for T1) with the current
  parser/grader/calibrated checklists; writes the committed, hash-anchored
  `docs/experiments/data/agent-study-regrade.v1.json` (review N2).
- `configs/` — `settings-study.json` (identical tool-permission surface across
  arms), `mcp-A0.json` (empty server list), `mcp-A3.template.json` (repogrammar
  server, binary + project rendered per run).
- `tasks/` — the two frozen task definitions (prompt + oracle), calibrated
  against the pinned SHA.
- `fixtures/` — scripted fake-agent transcripts (four detector seeds + dry-run
  transcripts), a generator (`build_fixtures.py`), and a `mini_repo` for grader
  tests. `selftest.py` unit-tests every module.

### Leak-free treatment (as far as enforceable in the pilot)

- Identical per-run worktree snapshot (pinned repo source + the **same** pinned
  `.repogrammar` index tarball) in **both** arms; the only per-arm differing
  input is the MCP config file. This is asserted by recording `worktree_sha256`
  per run and confirming it is identical across A0/A3/reps for a task, while
  `mcp_config_sha256` differs by arm.
- Product CLI is not on `PATH`; the MCP server is launched by absolute path from
  the A3 config only. Recovery guidance ("run resync") is therefore not
  executable — the conservative fallback (source reads) is the measured behavior.
- `--strict-mcp-config` guarantees no other MCP servers (CodeGraph etc.) exist in
  any arm; A0 has an empty server list.
- Global agent context (`~/.claude/CLAUDE.md`, user memory, plugins, skills) is
  intended to be suppressed via an isolated `CLAUDE_CONFIG_DIR` (`"isolated"`
  auth mode). See §4 for the pilot's auth deviation and the empirical leak check.
- Task environment is offline: `settings-study.json` denies `Bash`, `WebFetch`,
  `WebSearch`, and `Task`. Grading runs offline in the harness, so the agent
  never needs Bash. (The full grid may re-enable a pinned Bash build/test
  allowlist; the pilot denies Bash and documents that deviation.)

## 3. Pinned repo and tasks

- Repo: `full-stack-fastapi-template`, pinned commit
  `4d3d5e92c1ea6b3fa0fab02c41124844ec45bca8`. Tree hash and index-tarball hash
  are recorded in `src/experiments/agent_study/repos.lock.json`
  (metadata only — repo source is never committed). Indexing the pinned worktree
  yields 9 supported families (verified before the runs), so the A3 arm has real
  content to serve.
- **T1 — analogue-guided feature add** (`tasks/t1_item_summary.json`): add a
  `GET …/summary` endpoint for a single item following the repo's existing
  router conventions (`SessionDep`, `CurrentUser`, `response_model`). Hidden
  acceptance = static regex assertions over `backend/app/api/routes/*.py`, plus a
  "worktree changed" guard. Calibrated against the pinned SHA: `items.py` uses
  exactly those conventions.
- **T6 — comprehension question** (`tasks/t6_db_session.json`): how is a database
  session provided to request handlers; name the mechanism, the file(s), and the
  rule new code must follow. Hidden acceptance = gold must-have fact checklist
  (FastAPI dependency injection; `SessionDep`/`get_db`; `backend/app/api/deps.py`;
  declare a `SessionDep` parameter) + a forbidden false-claim guard. Calibrated
  against `deps.py`'s `SessionDep = Annotated[Session, Depends(get_db)]`.

Prompts are stored in the harness and hashed into each record (`prompt_sha256`);
prompt text is never written into a committed record.

## 4. Flag-surface and environment findings (design §9 criterion 2)

Re-verifying the design's §4 invocation sketch against the pinned CLI
(`claude 2.1.214`) surfaced two required corrections, plus one host constraint:

1. **`--max-turns` no longer exists** in claude 2.1.214. The design's
   `--max-turns 80` is dropped. Turn/spend bounding uses the CLI-enforced
   `--max-budget-usd` (per run) plus a harness wall-clock `timeout`. This is
   exactly the flag-surface drift the pilot exit checklist is meant to catch.
2. Confirmed working flags: `-p`, `--output-format stream-json --verbose`,
   `--mcp-config … --strict-mcp-config`, `--settings`, `--permission-mode
   acceptEdits`, `--max-budget-usd`, `--add-dir`. Prompt is fed on stdin.
3. **Auth vs. isolation coupling.** A throwaway `HOME` (design §4) yields
   `"Not logged in · Please run /login"` on this host — the keychain OAuth
   credential is coupled to the default config path, so neither a throwaway
   `HOME` nor an empty `CLAUDE_CONFIG_DIR` authenticates. The pilot therefore
   runs in `"real"` auth mode (host's real config). An empirical leak check on a
   real-mode A0 run found **no** global-CLAUDE.md leak markers (no
   `repogrammar` / `find_analogues` / prompt-compiler / gate references in the
   transcript; the agent behaved as a clean coding agent). This suggests
   `claude -p` headless does not inject the user-scoped CLAUDE.md into behavior
   here. **Nonetheless, the full grid MUST switch to `ANTHROPIC_API_KEY` +
   throwaway HOME** so the correct isolated mode authenticates and the guarantee
   is structural rather than empirical.

## 5. Dry-run proof (zero spend)

`python3 driver.py --dry-run` runs the full pipeline over scripted fixture
transcripts with **no agent process and no network**: 8 grid cells (2 tasks × 2
arms × 2 reps) plus the four seeded detector runs. It asserts all 8 cells reach a
deterministic verdict, all records validate, A3 cells record MCP calls, and each
seeded detector fires exactly as specified:

- `transcript_clean` → fires nothing;
- `transcript_stale` → `stale_evidence_use = 1` (and `unknown_override = 1`,
  since StaleEvidence arms both per design §6);
- `transcript_unknown` → `unknown_override = 1`, `stale_evidence_use = 0`;
- `transcript_indexpeek` → `index_peek = 2`.

`python3 selftest.py` unit-tests the same modules (20 tests): tree-hash
equivalence, parser metrics, all four detector fixtures, grader
gate_pass/gate_fail/no_patch and T6 checklist, and the record privacy guard.

## 6. Pilot run results

8 runs, `claude-haiku-4-5-20251001`, CLI `2.1.214`, `"real"` auth mode,
randomized interleaved order (shuffle seed recorded in `driver.py`). Committed
records: `docs/experiments/data/agent-study-run.v1.jsonl` (all 8 validate
against the schema + privacy guard). Raw transcripts are local-untracked.

| # | task | arm | rep | outcome (as run) | cost $ | MCP calls | files read | safety |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | T1 | A0 | 2 | gate_pass | 0.0825 | 0 | 2 | 0/0/0 |
| 1 | T6 | A3 | 1 | gate_fail\* | 0.0532 | 0 | 5 | 0/0/0 |
| 2 | T6 | A3 | 2 | gate_fail\* | 0.0589 | 0 | 4 | 0/0/0 |
| 3 | T6 | A0 | 1 | gate_fail\* | 0.0675 | 0 | 6 | 0/0/0 |
| 4 | T1 | A3 | 1 | gate_pass | 0.0966 | 0 | (patch) | 0/0/0 |
| 5 | T1 | A3 | 2 | gate_pass | 0.0810 | 0 | (patch) | 0/0/0 |
| 6 | T1 | A0 | 1 | gate_pass | 0.0821 | 0 | (patch) | 0/0/0 |
| 7 | T6 | A0 | 2 | gate_fail\* | 0.0569 | 0 | (answer) | 0/0/0 |

safety = index_peek / unknown_override / stale_evidence_use. **Cumulative
host-reported cost: $0.5788** (cap $27, authorized $30). No cell approached the
per-run `--max-budget-usd` cap; no infra/timeout failures.

**What the pilot proved (harness mechanics):**

- **Arm isolation holds.** All 8 runs share one `worktree_sha256`
  (`868cbfff95ae…`, identical bytes: same pinned source + same index tarball in
  both arms); `mcp_config_sha256` differs by arm (A0 vs A3) and is the only
  differing per-arm input. This is the design's §9.1 pass criterion, met.
- **Transcript parsing complete.** 12/12 (8 pilot + 4 dry-run detector) records
  carry non-null host-reported usage/cost; tokens, turns, files/bytes read, and
  MCP operation breakdown all populate. Flag-surface re-verified (§4). (`bytes_read`
  counts Read + read-pattern-Bash results only — the Bash gating was tightened
  after review; the pilot denied Bash, so no records were affected.)
- **Oracle mechanics work — with the scope caveat below.** T1's static gate
  produced deterministic `gate_pass` on all four T1 runs. What the gate verifies,
  precisely: each run's **added diff lines** declare a `GET …/summary` route and
  contain the session dependency (`SessionDep`), the auth dependency
  (`CurrentUser`), and a `response_model=`. After adversarial review (S1), the
  assertions are scoped to the **added lines** (`scope: "added"`), not the whole
  worktree — otherwise the pinned `items.py`, which already contains those
  identifiers, pre-satisfies them and a summary route missing auth would still
  pass. The committed re-grade artifact (`agent-study-regrade.v1.json`,
  `t1_scoped_grader_flips: 0`) confirms all four T1 runs still `gate_pass` under
  the scoped grader (every agent's added route code includes `CurrentUser` +
  `SessionDep`), so the fix does not change the pilot's verdicts — it removes a
  latent false-positive. **`gate_pass` here means "the
  added code carries the required conventions", not "a runtime-conforming,
  auth-enforcing endpoint"**; runtime enforcement and full conformance are the
  blinded reviewer's job (design §7). Reference/mutant discrimination (incl. an
  auth-missing mutant) is exercised by the grader unit tests.
- **Safety detectors ran on the real transcripts' non-MCP paths** and fired
  nothing. Because A3 adoption was 0 (no MCP events occurred), the real
  transcripts exercised only the `index_peek` path and the non-armed state of
  the two MCP-armed detectors (`unknown_override`, `stale_evidence_use`); their
  **firing** behavior is validated only on the seeded fixtures (dry run / unit
  tests). Real-transcript coverage of the MCP-armed detector paths awaits the
  full grid, where MCP is actually adopted.
- **Cost accounting + abort path work.** Cumulative cost is summed from
  host-reported `total_cost_usd`; the $27 abort and per-run cap are wired
  (unexercised here because cost stayed low).

**Mechanical issues found (and their disposition):**

1. **T6 checklist false-negative (fixed pre-freeze).** All four T6 runs ran
   `gate_fail` (marked \*), but inspection shows the answers are correct and
   contain every gold fact — e.g. "declare a parameter with the type annotation
   `SessionDep`", `backend/app/api/deps.py`, `get_db`, `Annotated[Session,
   Depends(get_db)]`. The as-run fact-4 regex required `SessionDep`/`session`
   to appear *before* `parameter`, so it false-negatived that phrasing. Per
   design §9.7 (pilot rubric fixes before freeze; dev tasks are burned), the
   regex was made order-insensitive. **The committed re-grade artifact
   `docs/experiments/data/agent-study-regrade.v1.json` (produced by the
   committed `regrade.py`, zero spend, hash-anchored to each transcript) records
   gate_pass 4/4 for the four T6 runs under the calibrated checklist** —
   reproducible evidence that the grader is correct and the failures were a
   checklist artifact, not wrong answers. The `agent-study-run.v1` ledger
   preserves the as-run `gate_fail` for honesty (raw execution record); the
   calibrated checklist is what the full grid uses, and the design's blinded
   human review supersedes the mechanical T6 checklist for scored results anyway.
2. **MCP adoption is 0/4 in A3 — a finding, not a bug.** The `repogrammar` MCP
   server **connected** in every A3 run (init event: `status: connected`, 28
   tools available) but the agent called `repogrammar_context` **zero** times.
   With no managed instruction block and headless mode injecting no CLAUDE.md
   nudge, adoption steering rests entirely on the MCP tool description; a small
   model (Haiku) did not proactively reach for it. This is exactly the design's
   §9 criterion-5 adoption-failure finding. The permission surface was audited
   (`mcp__repogrammar__repogrammar_context` is allowlisted, not suppressed).
   Before the full grid, adoption must be re-audited with the study's chosen
   model and the shipped MCP `initialize` instructions; a persistently low
   adoption rate is itself a reportable RQ5 result, not a harness defect.
3. **`"real"` auth mode also surfaces the host's hooks.** Beyond the (empirically
   absent) CLAUDE.md-content leak, the host's user-level **hooks** fire in real
   mode (visible as `hook_*` events in the transcript). This reinforces §8's
   requirement to move to `ANTHROPIC_API_KEY` + throwaway HOME for the grid so
   no host config (memory, hooks, plugins, settings) can enter any run.

## 7. Codex arm

Skipped in the pilot. `codex` is installed locally (`codex-cli 0.144.5`), so it
is *available*, but the pilot proves the claude-code path only; wiring the codex
A0/A3 confirmation wave (isolated `CODEX_HOME`, `mcp_servers` in
`config.toml`) is deferred to the design's severable codex wave.

## 8. What must change before the full grid

1. **Auth isolation.** Switch to `ANTHROPIC_API_KEY` (or `apiKeyHelper`) +
   throwaway HOME so the `"isolated"` mode authenticates; then global-CLAUDE.md /
   user-memory suppression is structural, not empirical (§4).
2. **Model.** Replace the pilot's Haiku snapshot with the design's chosen
   mid-size dated snapshot; keep it pinned for the whole study.
3. **Bash allowlist.** If the full grid grades by running the repo's pinned
   test suite offline, re-enable the design's pinned `Bash(build/test)` allowlist
   in `settings-study.json` and pre-bake the venv/lockfile at freeze.
4. **Reps / repos / task types / arms.** Scale to N=5, the four pinned repos,
   six task types × three instances, and the A1/A2/A4 arms per the design;
   freeze the task/prompt/oracle/binary hash manifest before any grid run.
5. **Answer-language robustness.** The pilot pins answer language to English in
   the prompt so the T6 English-token checklist is stable; the full grid's
   blinded human review supersedes the mechanical T6 checklist for scored
   results (design §7).

## 9. Risks and honest caveats

- **N=2 proves mechanics only.** No effect, savings, or safety claims are made or
  implied. The per-run records are an existence proof that the pipeline produces
  well-formed, privacy-clean measurements — nothing more.
- Safety counters are **syntactic transcript patterns**, not proof of harm; they
  are reported with their definitions attached.
- The pilot's real-config auth mode is a documented deviation; the empirical
  no-leak finding does not substitute for the structural isolation the full grid
  requires.
- Index portability: the shared `.repogrammar` tarball is built once and unpacked
  into each per-run worktree; any absolute-path coupling in the index would
  surface as MCP UNKNOWN/stale results (a measured product-contract behavior, not
  a harness bug). Observed A3 MCP status is reported in §6.
