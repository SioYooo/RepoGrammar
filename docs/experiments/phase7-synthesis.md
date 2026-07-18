# Phase 7 Unified Protocol — Synthesis of Eval Methodology (RQ1–RQ4) and Agent Study (RQ5)

Status: SYNTHESIS (read-only design; judged and merged from
`eval-methodology.md` and `agent-study.md`). Grounding: `feat/product-core`
@ `c4099ff`. This document supersedes neither input; it resolves their
conflicts and is the single implementation order. Where it contradicts an
input doc, this document wins.

---

## 1. Judgment summary

Both inputs are sound, mutually consistent on claim discipline (paired
recorder / `valid_for_product_claim` / `NOT_MEASURED` default), privacy
(no source, prompts, absolute paths, or secrets persisted), append-only
schemas, and determinism-aware statistics. Eight concrete conflicts or
duplications exist and are resolved in Section 2. The eval-methodology doc
is adopted wholesale for RQ1–RQ4 mechanics (splits, pinning, ablations,
RQ2/RQ3/RQ4 protocols, statistics, threat register T1–T16). The agent-study
doc is adopted wholesale for RQ5 arms, fixed factors, measurement mechanics,
reviewer oracle, pilot, and threats — subject to the resolutions below.

## 2. Conflict resolutions (binding)

R1. **Metric anchor discrepancy.** Orchestrator state says hit@1 21/42 /
mrr 0.500; the committed baseline doc records older 17/43 numbers
(agent-study §0). Resolution: before any Phase 7 measurement, one anchor
re-run of `repo-guard product-eval` at the execution commit refreezes the
pre-Phase-7 numbers in a new dated ledger section (append-only; the old
section is never edited). All Phase 7 deltas cite the refrozen anchor only.

R2. **Duplicate pinning manifests.** Eval proposes
`src/fixtures/evaluation/pinned-repos-v1.json`
(`product-eval-pinned-repos.v1`); agent-study proposes `repos.lock.json`.
Resolution: ONE schema, `product-eval-pinned-repos.v1`, one tree-hash
algorithm (the existing harness `fixture_version` SHA-256), one fetch/verify
code path. Two committed instances are allowed (`pinned-repos-v1.json` for
RQ1–RQ4, `agent-study-repos-v1.json` for RQ5) but they share schema,
validator, and the fetch subcommand; each entry carries `lane:
"eval"|"agent_study"` and `split`. No second hash algorithm, no second
fetch tool.

R3. **Repo-set overlap policy.** RQ5 repos are worked on heavily
(task authoring, seeded deltas, pilots) and would erode any split they
share. Resolution: RQ5 repos MUST be disjoint from RQ1 held-out repos
(hard rule, enforced by the shared manifest validator). RQ5 repos MAY
coincide with RQ1 validation repos only by explicit Coordinator decision
(D3); default is fully disjoint. The self-dogfood repo stays dev-only
diagnostic in both lanes.

R4. **Ablation mechanism vs A2 arm.** The eval doc's chosen mechanism —
`REPOGRAMMAR_EVAL_ABLATION` read once via the injected `env_lookup` at the
boundary, typed `RetrievalAblation` enum, echo invariant, unset-parity
test — is the single ablation mechanism for the whole phase. Agent-study
arm A2 ("QueryV2-only") is NOT expressible as one of the three ablation
tokens (`no_aliases|no_margin|no_prevalence`); it is a different product
composition. Resolution: A2 is redefined as a **pinned historical binary**
(the commit where query-resolution v2 landed, before later product-core
features), not an env-flag build; if no clean such commit exists, A2 is
dropped and its question is answered by the RQ1 ablation matrix instead
(Coordinator decision D1 confirms which). No new compound ablation tokens
are invented for RQ5.

R5. **Paraphrase-leakage tooling.** Eval uses normalized-signature
equivalence-class collision (hard corpus-check error); agent-study wants a
normalized 6-gram screen for task statements. Resolution: one corpus-check
module with two detectors: (a) signature-class collision across query
splits — hard error, exactly as eval §1.2; (b) n-gram overlap screen
between RQ5 task statements and any corpus query targeting the same family
— hard error at task freeze. Task statements are free prose, so (a) alone
is insufficient for them; queries are resolver inputs, so (b) alone is
insufficient for those. Both live in the same repo_guard corpus-check code.

R6. **repo_guard.rs contention.** Both lanes put harness logic in
`src/rust/bin/repo_guard.rs` (repository rule: automation logic lives
there). Resolution: repo_guard.rs has exactly ONE owning slice per wave
(Section 4); all other teams in that wave touch fixtures, docs,
application/interfaces modules, or telemetry only. Transcript-parsing and
safety-detector logic for RQ5 is implemented as a library module (e.g.
`src/rust/application/agent_study.rs` or a documented src/ path) owned by
the telemetry team, with repo_guard gaining only thin routing in that
team's assigned repo_guard wave slot. The RQ5 harness is a `repo-guard`
subcommand (agent-study §11's first option; repository boundary rule
decides this, D6 only confirms naming).

R7. **Recorder mirroring.** Adopted: every RQ5 both-success pair is
mirrored into `repogrammar telemetry experiment-*` (`controlled_pair`,
`host_reported`). This is the ONLY path by which any token-savings number
may ever be claimed, for both lanes; RQ1–RQ4 artifacts keep
`token_measurement.status: NOT_MEASURED` until then (eval §8.3 =
agent-study §8 rule; no conflict, now stated once).

R8. **Unverified program lists.** Eval's 11 alignment cases (assumption
A2) and agent-study's 6 task types (assumption §0) are both derived, not
program-verified. Resolution: both are adopted as the working
instantiation; before their respective freezes, reconcile against the
program document and record an id mapping in the ledger (D2). Structure
(oracles, splits, metrics) is frozen regardless of renaming.

## 3. The unified protocol (normative outline)

- **RQ1**: splits per eval §1 (73-query v1 corpus locked to dev;
  validation ≈75 = form-shift on fixtures + content-shift on pinned repos;
  held-out ≈75 sealed, separate author, SHA-256 + access log). Conditions
  per eval §3 (5 conditions on dev/validation; held-out runs `product` vs
  `baseline_token_overlap` once per pre-declared gate). Ablation mechanism
  per R4. Stats per eval §7 (class-cluster Wilson/bootstrap, exact McNemar
  paired, Holm over the pre-registered 8-test family; repetition CIs
  forbidden; safety counters are hard gates, not statistics).
- **RQ2**: eval §4 unchanged (input-stratified ~150 families, dual blind
  annotation, Q1 class + Q2 denominator validity, kappa, repo-cluster
  bootstrap, thresholds frozen for Phase 7).
- **RQ3**: eval §5's 11-case mutation corpus on committed Python fixtures,
  via extended corpus `mutation` kinds (`replace_span`/`insert_before`/
  `delete_span`, exact-match, scope-checked). Typed-JSON gold only.
  Blocked on Phase 3 alignment certificates (Section 5).
- **RQ4**: eval §6 scenario matrix (12 edit kinds × 3 workspaces),
  equivalence as a 100% invariant with committed reproducers on
  divergence; blocked on the parallel sync-equivalence-oracle lane
  honoring the §6.1 interface contract (renegotiate explicitly, never
  silently adapt).
- **RQ5**: agent-study §§2–10 with R3/R4/R5 applied: arms A0/A1/A3
  confirmed, A2 per D1, A4 + codex wave budget-gated; MCP-config-only
  treatment; identical worktrees incl. pinned `.repogrammar/` tarball in
  all arms; product CLI off PATH; N=5 reps, 20-min cap, randomized
  interleaving; mechanical metrics from stream-json; two-stage oracle
  (hidden tests gate, then blinded 2-reviewer rubric, kappa, LLM judge
  auxiliary only); pilot (§9, 7 pass criteria) gates the main grid.
- **Schemas** (all append-only, additive): `product-eval-results.v3`,
  `sync-equivalence-results.v1`, `agent-study-run.v1`,
  `agent-study-review.v1`, `product-eval-pinned-repos.v1` (R2).
- **Ledgers**: `docs/experiments/phase7-evaluation.md` (+`.summary.json`)
  for RQ1–RQ4; `docs/experiments/agent-study.md` (+`.summary.json`) for
  RQ5; baseline docs never edited; each lane's preregistration is a commit
  whose SHA is the prereg timestamp (single umbrella prereg vs per-lane:
  D4).
- **Threat register**: eval T1–T16 plus agent-study §10 items, carried
  verbatim into the respective ledgers.

## 4. Wave / slice breakdown for agent teams (disjoint ownership)

Ownership domains: **[G]** = `src/rust/bin/repo_guard.rs` (one owner per
wave, hard rule R6); **[C]** = core/interfaces/application product code
(`src/rust/application/`, `src/rust/interfaces/`, `src/rust/core/`);
**[F]** = `src/fixtures/**` (corpus JSON, fixtures, manifests);
**[D]** = `docs/**`; **[T]** = telemetry/agent-study library module.
Every slice ships its tests and docs in the same atomic commits
(repository contract).

### Wave 0 — do-now foundations (parallel; no network, no spend, no Phase-3 dep)

- **W0.G** [G]: anchor re-run per R1; corpus schema v3 parsing
  (`split`, `equivalence_class_id`, new mutation kinds, optional
  `pinned_repo_id`); normalized-signature collision detector + intent-mix
  audit + n-gram task screen (R5) as hard corpus-check errors; results-v3
  emitter fields.
- **W0.C** [C]: `REPOGRAMMAR_EVAL_ABLATION` boundary parse via injected
  `env_lookup` → `RetrievalAblation` enum threaded into `query_terms` +
  gate evaluation; JSON `ablation` echo on CLI and MCP; unset-parity test
  + one test per token. Touches interfaces/application only — no
  repo_guard edits (round-trip assert deferred to W1.G).
- **W0.F** [F]: validation-(a) form-shift queries on existing fixtures
  (collision-checked against the dev class table); dev additions (ablation
  sanity probes, RQ3 rehearsal rows); draft RQ3 11-case mutation corpus
  against committed fixtures with typed-gold fields stubbed as
  `pending_phase3`.
- **W0.D** [D]: `phase7-evaluation.md` prereg skeleton (comparison family,
  gates, split-freeze placeholders); repo-selection criteria + strata +
  negative-controls doc (performance-blind, frozen before any metric);
  `agent-study.md` prereg skeleton; program-list reconciliation for D2.
- **W0.T** [T]: RQ5 transcript-parsing + safety-detector library module
  (unknown_override / stale_evidence_use / index_peek) with the four
  seeded fake-transcript fixtures as unit tests; recorder-mirroring
  helper. Pure parsing — no API calls, no repo_guard edits.

Integration gate W0: fmt/clippy/tests/repo-guard check green; ablation
echo verified by unit tests; corpus check rejects a planted cross-split
collision.

### Wave 1 — harness assembly (repo_guard slot rotates)

- **W1.G** [G] (owner: harness team): `--condition` → env-var mapping in
  the subprocess env + hard round-trip ablation assert (incl. asserting
  `null` for `product`); stats module (Wilson, exact McNemar, cluster/
  paired bootstrap) + `summary.stats` emission; pinned-repo manifest
  validation + fetch/verify subcommand (clone at pin, recompute tree
  SHA-256, hard error on mismatch) gated behind explicit invocation —
  merged but NOT executed until Wave 2 authorization; thin routing for the
  W0.T detector module (RQ5 harness subcommand shell, dry-run only).
- **W1.F** [F]: freeze validation-(a); record corpus hashes.
- **W1.D** [D]: commit the RQ1 preregistration section (8-test family,
  Holm, held-out gate definition) — this commit is the prereg timestamp.
- **Runs**: full 5-condition matrix on dev + validation-(a) (fixtures
  only). First real ablation numbers; no public repos yet.

### Wave 2 — public repos (BLOCKED on user authorization U1)

- **W2.G** [G]: execute fetch/verify for both manifests; isolated-workspace
  runs over pinned checkouts (`pinned_repo_id` path).
- **W2.F** [F]: selection log + frozen manifests (eval strata a–e +
  controls N1–N4; RQ5 candidate confirmation by indexing at pin);
  validation-(b) queries; held-out queries by a separate author/session;
  seal held-out (SHA-256 + access log).
- **W2.D** [D]: selection log doc; held-out seal record.
- **Runs**: validation-(b) matrix; RQ2 sampling-frame extraction +
  annotation-sheet generation (annotation itself blocked on U3).

### Wave 3 — RQ3/RQ4 execution (BLOCKED on Phase-3 alignment + sync-oracle lane)

- **W3.G** [G]: RQ3 mutation-runner execution path (apply, scope-check,
  resync, compare typed certificate fields); RQ4 scenario runner emitting
  `sync-equivalence-results.v1` against the §6.1 oracle contract.
- **W3.F** [F]: un-stub RQ3 typed gold once certificate shapes are real;
  reconcile case ids per D2.
- **W3.D** [D]: RQ3/RQ4 ledger sections; any oracle-contract
  renegotiation recorded explicitly.

### Wave 4 — RQ5 pilot then main grid (BLOCKED on user authorization U2)

- **W4.T+G**: pilot (2 burned tasks × A0/A3 × 3 reps + 4 detector seeds,
  ~$15); 7 pass criteria incl. flag-surface re-verification and worktree
  SHA identity; escalation rule applied before freeze. Then freeze
  tasks/prompts/oracles/binaries (hash manifests), prereg endpoints
  E1/E2/E3, run main grid A0–A3 (360 runs, ≈$280–$330); A4/codex per D5
  budget gate; blinded review; recorder mirroring per R7.

### Wave 5 — confirmatory + close-out

- One held-out confirmatory run (`product` vs `baseline_token_overlap`) at
  the pre-declared gate; RQ2 adjudication + stats; final ledgers,
  `.summary.json`s, limitations, threat-register status; atomic commits
  per contract; branch/commit/verification report.

## 5. Do-now / blocked partition (explicit)

**Do-now (no authorization, no missing dependency):** W0 entirely; W1
entirely except executing the fetch subcommand; dev + validation-(a) runs;
RQ3 corpus authoring with stubbed gold; RQ5 detector library + seeded
tests; all prereg docs; anchor refreeze (R1).

**Needs Phase-3 alignment (and parallel lanes) first:** RQ3 execution and
typed-gold finalization (alignment certificates must exist); RQ4 scenario
runs (sync-equivalence oracle lane must land or renegotiate the §6.1
contract — assumption A1); RQ5 arm A2 binary selection if D1 chooses the
pinned-commit route.

**Needs user authorization:**
- **U1 — network/public-repo cloning** (Wave 2): cloning pinned public
  repos into untracked workspaces; nothing cloned is ever committed.
- **U2 — agent-study API spend** (Wave 4): pilot ≈$15; main grid A0–A3
  ≈$280–$330; optional A4 + codex wave ≈+$110; ~18 reviewer-hours.
- **U3 — human annotators** (RQ2 and RQ5 review): two independent
  annotators/reviewers; who they are is a user resource decision.

## 6. Open Coordinator decisions

- **D1 — A2 arm**: pinned historical query-v2 commit binary, or drop A2
  and rely on RQ1 ablations for that decomposition. (Default if
  undecided: drop A2; smallest grid that still isolates product-core via
  A1 vs A3.)
- **D2 — program-list reconciliation**: map the derived 11 alignment cases
  and 6 task types to the program document's own lists; record mappings
  before RQ3/RQ5 freezes.
- **D3 — repo-set overlap**: confirm RQ5 repos fully disjoint from all
  RQ1 repos (default) or allow overlap with RQ1 validation repos only.
- **D4 — preregistration granularity**: one umbrella prereg commit vs
  per-lane prereg commits (default: per-lane; each lane freezes when
  ready, held-out gate defined in the RQ1 prereg).
- **D5 — budget scope**: authorize A4 spans arm and codex confirmation
  wave now, or hold as severable extensions (default: hold; preregistered
  drop order already exists).
- **D6 — RQ5 harness naming**: confirm the `repo-guard` subcommand name
  for the study harness (boundary rule already decides its location).
- **D7 — stats implementation**: in-repo Rust (no new production
  dependency without ADR) vs a documented dev-tool under `src/`; default:
  in-repo Rust inside repo_guard, exact tests against known fixtures.
- **D8 — anchor commit**: confirm the exact execution commit for the R1
  anchor refreeze once the parallel lanes merge.
