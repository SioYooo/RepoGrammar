# Phase 7 Evaluation Methodology — RQ1–RQ4 Design

Status: DESIGN (read-only agent output; no repo changes made).
Grounding commit: `feat/product-core` @ `c4099ff` ("feat(family): derive constraint profiles").
Scope: RQ1 retrieval, RQ2 family semantics (prevalence), RQ3 alignment
certificates, RQ4 incrementality. RQ5 (agent impact) is out of this document's
scope except for the claim-discipline interface it imposes (Section 8.6).

## 0. Verified grounding and assumptions

Verified at `c4099ff` by direct file inspection:

- Corpus: `src/fixtures/evaluation/query-corpus-v1.json`, `schema_version
  product-eval-corpus.v1`, 73 queries, intents `retrieval 42 / abstention 25 /
  context 6`, three fixtures (`python-v0_1`, `typescript-v0_2`, `zero-family`).
  Per-query `mutation` blocks exist (`append_line`, used by
  `py-stale-fastapi-family`).
- Harness: `repo-guard product-eval` in `src/rust/bin/repo_guard.rs`; isolated
  temp workspace per fixture (env_clear + isolated HOME/XDG/CODEX_HOME,
  tool-only PATH; repo_guard.rs:865–892); `--condition <token>`
  (`[a-z0-9_-]+`, ≤40 chars, no leading `-`; repo_guard.rs:2283–2301) and
  `--baseline token-overlap` plumbing; results `product-eval-results.v2` with
  `condition`, `baseline`, `false_family_selections`,
  `selected_on_abstention_gold`, per-metric integer numerator/denominator.
- Resolver: `docs/specifications/query-resolution.md` — deterministic term
  retrieval, alias tables in `application::query_terms` (FRAMEWORK_ALIASES 27,
  CONCEPT_ALIASES 21, ROLE_CONCEPTS 54, STOPWORDS 28, PLURALS 15), additive
  weights (framework 6, concept 4, language 2, residue 3), gates
  `MIN_RETRIEVAL_SCORE = 10`, `MIN_RETRIEVAL_MARGIN = 1`, seven typed
  abstention reasons; **calibrated against the full 73-query corpus**
  ("Calibration summary": hit@1 21/42, mrr 0.500 committed-answer, plateau
  scan over `MIN_RETRIEVAL_SCORE ∈ {7,8,10,11}`).
- Prevalence model: `src/rust/core/model/family.rs` —
  `assess_family_prevalence`, classes `DOMINANT_PATTERN / SUPPORTED_PATTERN /
  MINORITY_PATTERN / UNKNOWN_PREVALENCE`, thresholds MINORITY `< 1/3`
  coverage, DOMINANT `≥ 3/5` coverage AND support `≥ 2×` largest competitor
  AND support `≥ 2` (family.rs:375–433, exact integer cross-multiplication).
- Constraint profiles: `FamilyConstraintProfile` (family.rs:626–656) with
  `required_equal_features` (semantics `Equal | EqualEmpty | MustContain`),
  `allowed_variations` (observed-only, `CONSTRAINT_OBSERVED_PROFILE_CAP`,
  representative members capped at 8), `prohibited_or_blocking_features`
  (`ProhibitedPresence`), `unresolved_obligations` (typed UNKNOWNs; a real
  family always carries the runtime-equivalence obligation).
- Env-var precedent for boundary configuration:
  `REPOGRAMMAR_STRICT_GITIGNORE` read through an injected `env_lookup` at the
  CLI boundary and converted into an explicit typed request field
  (interfaces/cli/mod.rs:6882, 6927; tested at 12360ff). No ambient reads in
  the application layer.
- Paired token-experiment recorder: `application/telemetry.rs:1728–1858` —
  `report_for_sessions` requires matched baseline/treatment sessions of equal
  mode/source/task-kind; `claim_validity` is `valid_for_product_claim` only on
  `both_success` paired measurements; otherwise `no_paired_measurement` /
  `unknown`. It is wired but has no recorded paired experiment
  (`token_measurement.status: NOT_MEASURED` in
  `docs/experiments/product-core-baseline.summary.json`).
- Ledger conventions: `docs/experiments/product-core-baseline.md` + dated
  `.summary.json` with `privacy` block (`source_included: false`, no absolute
  paths, no raw logs, no credentials) and `limitations[]`.

Assumptions (not verifiable at this commit; encode as assumptions, not facts):

- A1. The sync-equivalence oracle is landing in a parallel lane and is **not**
  present at `c4099ff` (`sync-equivalence` has no hits in
  `src/rust/bin/repo_guard.rs`). Section 6 therefore specifies the oracle
  contract RQ4 needs; the parallel lane must satisfy or renegotiate it.
- A2. "The program's 11 alignment cases" (RQ3) are not enumerated in any
  committed document found in `docs/` or `.agents/`. Section 5 derives an
  11-case matrix that exhaustively covers the typed constraint-profile
  semantics space; if the program's own list exists elsewhere, reconcile case
  ids before implementation and record the mapping in the ledger.
- A3. hit@1 21/42 / mrr 0.500 is the committed-answer state at `0568287`+
  (task brief + calibration summary agree); re-run before Phase 7 execution to
  refreeze the pre-Phase-7 anchor at the exact execution commit.

---

## 1. RQ1 corpus splits (dev / validation / held-out)

### 1.1 The central honesty constraint

Every one of the 73 existing queries was used to calibrate
`MIN_RETRIEVAL_SCORE`/`MIN_RETRIEVAL_MARGIN` and to adjudicate gold
(query-resolution.md "Calibration summary"; commit `0568287 feat(query):
calibrate candidate abstention`). Therefore **the entire v1 corpus is
development-contaminated and lands, in full, in the dev split.** No existing
query may be promoted to validation or held-out. Any Phase 7 number quoted on
the 73 queries must be labeled `split: dev` and never presented as
generalization evidence.

### 1.2 Split unit: normalized-signature equivalence class

The resolver is deterministic and total: two queries with identical
`NormalizedQuery` buckets (language/framework/concept/residue) produce
identical outcomes against the same index. A held-out query whose normalized
form collides with a dev query measures nothing new — it is a paraphrase leak
by construction. The split unit is therefore the **equivalence class**
`(normalized signature, gold family constraint, fixture/repo)`, not the query
string.

Leakage control is mechanical, not editorial: build a small dev-only tool
(harness-side, read-only) that runs `normalize_query` over every query in
every split and **fails the corpus check if any class spans two splits**. This
is decidable because normalization is committed, pure, and deterministic
(query-resolution.md "Determinism guarantees"). Residue-term-only differences
that survive normalization distinct are legitimately different queries.

Known dev-side classes to seed the detector (from the committed corpus; these
concepts are closed to val/held-out **on the same fixtures**):

| Concept class (normalized) | Dev members (retrieval + decoys) |
| --- | --- |
| `{fastapi, route}` / `{route}` | py-nl-fastapi-routes, py-nl-para-rest-endpoints, py-nl-para-wire-routes, py-syn-endpoint, py-syn-http-handler; decoys py-nl-api-routes-ambiguous, py-ambiguous-handler |
| `{test}` / `{pytest, fixture}` | py-nl-test-fixtures, py-nl-para-writing-tests, py-nl-para-setup-fixtures, py-syn-unit-test, py-syn-test-case; ts-amb-tests |
| `{sqlalchemy, data_access}` / `{data_access}` | py-nl-db-transactions, py-nl-repository-methods, py-nl-para-db-sessions, py-nl-para-talk-to-db, py-nl-para-repo-pattern, py-nl-para-data-access |
| `{pydantic, validation_model}` / `{validation_model}` | py-nl-validation-models, py-nl-para-validate-payloads, py-nl-para-request-schemas, py-syn-schema-validation |
| dual `model` concept | py-amb-models, py-syn-db-model, py-syn-orm-model |
| TS route/framework | ts-nl-express-routes, ts-nl-fastify-routes, ts-amb-routes, ts-fw-express |

### 1.3 Concrete split assignment and target sizes

| Split | Size target | Content | Purpose |
| --- | --- | --- | --- |
| dev | ~110 (73 existing + ~37 new) | All 73 v1 queries; new queries needed to debug new mechanisms (public-repo plumbing smoke queries, ablation sanity probes, RQ3 mutation-case rehearsals) | Free to inspect, iterate, retune |
| validation | ~75 (all new) | (a) ~25 new-form queries over the **existing** fixtures, authored under the paraphrase protocol (1.4), collision-checked against dev classes; (b) ~40 queries over 2–3 **validation** pinned public repos; (c) ~10 context/abstention fill | Model/threshold selection, ablation comparisons, all iterative statistics |
| held-out | ~75 (all new, sealed) | Queries only over 2–3 **held-out** pinned public repos (disjoint from validation repos) plus one newly authored held-out fixture never used in dev/val | One confirmatory evaluation per pre-declared gate; headline claims |

Per-split intent mix mirrors the v1 taxonomy: ≈55% retrieval / 35% abstention /
10% context. Language mix: Python-dominant in held-out (v0.1 official scope is
Python-first per the repository contract); TypeScript queries are included but
reported separately as transitional, never in the headline claim.

This design separates the two generalization axes deliberately:
validation (a) is **form shift on seen content** (new phrasings, same
fixtures); validation (b) and all of held-out are **content shift** (unseen
repos). Reporting the two sub-slices separately decomposes any degradation
into "resolver overfit to phrasings" vs "resolver overfit to fixture content".

### 1.4 Authoring protocol for new queries (anti-leakage)

1. **Intent-first authoring.** The author writes candidate questions from the
   target repo's README/docs/directory names *before* looking at the index's
   family inventory. Questions whose concept has no indexed family are kept as
   a measured `family_absent` category (gold `unknown`), not discarded —
   discarding them would hide ontology blind spots (gold circularity, threat
   T3).
2. **Gold verification.** For every retrieval gold, index the pinned repo once
   and verify the target family exists and is unique for the constraint (the
   existing calibration discipline: verify by running the harness before
   freezing gold). Record the `fixture_version` hash and generation in the
   gold provenance field.
3. **Author separation for held-out.** Held-out queries are authored by a
   different person/session than dev/val authors, working only from the
   held-out repos and a one-page intent-taxonomy sheet — never from the dev
   corpus text. (For a deterministic resolver the collision detector is the
   hard guarantee; author separation is defense in depth against convergent
   phrasing style.)
4. **Sealing.** The held-out corpus file's SHA-256 is recorded in the ledger
   at freeze time. Every evaluation against it is logged (date, commit,
   purpose). Permitted accesses: one dry mechanical run to verify the harness
   parses it (results discarded, metrics not viewed), then only at
   pre-declared phase gates. Ad-hoc peeking is a protocol violation and taints
   the split (threat T10).
5. **Corpus check integration.** The normalized-signature collision detector
   and the intent-mix audit run as part of `repo-guard product-eval` corpus
   parsing (hard error on cross-split collision), so leakage cannot silently
   re-enter as the corpus grows.

---

## 2. Public-repository selection and pinning

### 2.1 Selection criteria (pre-registered, performance-blind)

The core validity rule: **repository selection must be completed and frozen in
the manifest before any RepoGrammar metric is computed on any candidate.**
Nothing may be added or removed after metrics are seen (threat T4). The only
public-repo measurement that exists today is the self-dogfood repo; it is
usable as a dev-split diagnostic corpus but not as validation/held-out (it is
the tool's own source and maximally contaminated).

Objective inclusion criteria, applied in order to a deterministically sorted
candidate pool (GitHub search by declared dependency in
`pyproject.toml`/`requirements*.txt`/`package.json`, filtered by criteria
below, sorted by stars descending, take the first k satisfying all criteria,
log every rejection with its criterion):

1. Primary language in scope: Python (official v0.1 scope: FastAPI, pytest,
   SQLAlchemy, Pydantic) or TypeScript/JavaScript (transitional scope:
   express, fastify, hono, zod, prisma, drizzle, jest/vitest — the committed
   `KNOWN_FRAMEWORK_TOKENS` vocabulary).
2. Application/service or library that *uses* the framework — not the
   framework's own implementation repo (framework-implementation repos are a
   separate stress stratum, below).
3. Size band: at least one repo per band — small (<100 source files), medium
   (100–1,000), large (1,000–5,000). Bands stress the prevalence denominator
   and RQ4 sync cost differently.
4. License: any license permitting local analysis (we clone and index locally;
   we never redistribute or commit source — the manifest carries metadata
   only). Record the SPDX id.
5. Non-trivial history (>100 commits) so RQ4 branch-switch scenarios have real
   material.
6. Not previously used in any RepoGrammar development, fixture derivation, or
   dogfooding (excludes the RepoGrammar repo itself and anything cited in
   `docs/plans/python-dogfooding-plan.md` / `v0.2-real-repo-dogfood.md`).

Strata to fill (validation gets one of each of a–c; held-out gets a disjoint
one of each; controls split between the two):

- (a) FastAPI service/app repo (Python, route+validation families expected).
- (b) SQLAlchemy-centric library or service (data-access families).
- (c) pytest-heavy library (test/fixture families).
- (d) TS express-or-fastify service (transitional, reported separately).
- (e) Stress stratum (dev split only): one framework-implementation repo
  (e.g. a framework's own source) — expected to look *unlike* app usage;
  diagnostic only.

Negative controls (all with authored gold; these are where "don't pick repos
the tool does well on" is enforced structurally — controls are chosen to be
hard or empty by construction):

- (N1) Zero-family public repo: a pure-algorithms/utility Python repo with no
  supported framework imports. Gold: zero families claimed; retrieval queries
  abstain; path queries yield `partial_context` at most (generalizes the
  committed `zero-family-repo` fixture).
- (N2) Out-of-scope stack: a Go or Ruby service. Gold: graceful abstention
  everywhere; zero families; no `fallback` outcomes.
- (N3) Minority-usage repo: a supported framework used in one small corner of
  a large unsupported codebase. Gold: only the corner's families; prevalence
  must not report DOMINANT for corner patterns against the whole-repo peer
  context unless the denominator legitimately supports it (feeds RQ2).
- (N4) Adversarial-naming control: a repo with `routes/`, `models/`, `tests/`
  directory names but an unsupported stack, targeting residue-term
  false positives (`WEIGHT_RESIDUE_HIT` path-component hits). Gold: abstention;
  any family selection counts in `false_family_selections`.

The selection log (queries used, candidate list order, per-candidate
accept/reject + criterion) is committed alongside the manifest. Attrition
rule: after freezing, a repo may be dropped only for a pre-registered
operational reason (clone failure, resync crash) — and a resync crash is
itself a recorded RQ-level result, not a silent exclusion.

### 2.2 Pinning protocol

Committed manifest `src/fixtures/evaluation/pinned-repos-v1.json`
(`schema_version: product-eval-pinned-repos.v1`), metadata only:

```json
{
  "repo_id": "pyval-a-fastapi-svc",
  "url": "https://github.com/<owner>/<name>",
  "pinned_commit": "<40-hex sha>",
  "tree_sha256": "<harness fixture_version algorithm over the checkout>",
  "license_spdx": "MIT",
  "language": "python",
  "stratum": "fastapi_service",
  "split": "validation",
  "size_band": "medium",
  "selection_log_ref": "docs/experiments/phase7-repo-selection.md#pyval-a"
}
```

- Fetch step (network-permitted, manifest-driven, explicit): clone at
  `pinned_commit` into an untracked eval workspace
  (`target/product-eval-repos/<repo_id>/`), then recompute the harness's
  `fixture_version` SHA-256 (sorted relative path + content — the algorithm
  already implemented for fixtures) and hard-error on mismatch with
  `tree_sha256` (threat T11: upstream drift/force-push).
- Evaluation runs are then fully offline; the harness's isolated-workspace
  copy mechanism treats the pinned checkout exactly like a committed fixture
  root (the corpus `fixtures[]` entry gains an optional
  `pinned_repo_id` in place of `root`).
- Never commit cloned source; `.gitignore` the eval workspace; the results
  and ledger reference repos only by `repo_id` + hashes (privacy block:
  `source_included: false` holds).

---

## 3. Ablation matrix and its mechanism

### 3.1 Conditions

| Condition token | Meaning |
| --- | --- |
| `product` | Shipped behavior (default; already plumbed) |
| `baseline_token_overlap` | Committed naive control (already plumbed) |
| `product_no_aliases` | FRAMEWORK_ALIASES + CONCEPT_ALIASES lookups disabled; affected subtokens fall through to residue terms. STOPWORDS/PLURALS stay active (hygiene, not semantics), ROLE_CONCEPTS stays active (it maps index-side roles, not query text). Isolates the alias vocabulary's contribution. |
| `product_no_margin` | Gate 5 `margin_too_close` disabled (effective `MIN_RETRIEVAL_MARGIN = 0`); `truncated_tie` and all other gates retained. Expected: hit@1 may rise while `false_family_selections` / `selected_on_abstention_gold` rise — the safety counters are the justification evidence for the margin gate. |
| `product_no_prevalence` | Prevalence class and coverage removed from the candidate sort key (sort becomes score desc → family_id byte order). Measures the prevalence tiebreak's retrieval contribution. Classification output itself is unchanged (that is RQ2's axis, not RQ1's). |

Matrix: 5 conditions × {dev, validation} for all iterative work.
Held-out runs exactly two conditions once per gate: `product` and
`baseline_token_overlap` (confirmatory; ablations are explanatory and stay on
validation — threat T10).

All condition tokens fit the existing `--condition` validator
(`[a-z0-9_-]+`, ≤40). Results remain comparable only via retrieval metrics and
the two safety counters, per the documented v2 rule that `matches`/latency are
not cross-condition comparable.

### 3.2 Mechanism decision: env var read at the boundary into typed request data

Options judged against the repository's engineering standards ("avoid hidden
global state, silent fallback"; "cross-path decisions have one authoritative
policy entrypoint"; "keep new code minimal"):

- **Cargo feature — rejected.** Conditional compilation means the measured
  binary is not the shipped binary (tested-is-shipped broken), multiplies the
  build matrix 4×, risks feature-unification accidents, and makes an "ablated
  artifact ships" mistake possible. Also the slowest iteration loop (full
  rebuild per condition).
- **`#[cfg(test)]` hook — rejected.** The harness drives a real subprocess
  product binary; test-only hooks are unreachable from it.
- **Env var via the existing injected `env_lookup` — chosen.** This is the
  repo's established precedent (`REPOGRAMMAR_STRICT_GITIGNORE`,
  interfaces/cli/mod.rs:6882): the variable is read exactly once at the
  CLI/MCP boundary through the *injected* `env_lookup` closure (so it is
  testable without process-global mutation), parsed into a typed field, and
  passed down explicitly. What the no-hidden-global-state rule actually
  forbids is ambient reads inside the application layer and silent behavior
  divergence — not boundary-injected configuration.

Contract for `REPOGRAMMAR_EVAL_ABLATION`:

1. Value set: `no_aliases | no_margin | no_prevalence`. Any other non-empty
   value is a typed hard error at the boundary (no silent fallback rule).
   Unset/empty ⇒ `RetrievalAblation::None`.
2. Parsed once at the boundary into `RetrievalAblation` (a field on the query
   request/config), threaded explicitly into `query_terms::normalize_query`
   context and `query`'s gate evaluation. No `std::env` read below the
   interface layer. One enum is the single authoritative policy entrypoint;
   the three sites consult it, they do not re-derive it.
3. **Echo invariant:** every query-route JSON carries `"ablation":
   "<token>"|null`. An ablated response is thereby self-identifying and can
   never be mistaken for production output.
4. **Harness round-trip assert:** `product-eval --condition product_no_margin`
   sets the env var in the (already `env_clear`ed) subprocess environment and
   fails hard if any result's echoed `ablation` differs from the requested
   condition's expected token — including asserting `null` for `product`.
5. **Unset-parity test:** a unit test asserts `RetrievalAblation::None`
   produces byte-identical ranking output to a build where the enum is absent
   (i.e. the default path is provably unchanged); plus one test per token
   asserting the intended single behavioral delta and nothing else.
6. MCP and CLI both honor the echo; there is no new CLI flag and no new
   top-level command (keeps the pattern-family-first CLI contract).

---

## 4. RQ2 prevalence-accuracy protocol

Question: do `DOMINANT_PATTERN` / `SUPPORTED_PATTERN` / `MINORITY_PATTERN`
verdicts, computed by `assess_family_prevalence` thresholds (3/5 coverage +
2× competitor + support ≥2; minority <1/3), agree with human judgment of
"dominant / supported / minority way of doing X in this repo"? This directly
retires baseline contradiction #1 (dominance without prevalence) with earned
evidence.

### 4.1 Gold construction (human-labelable)

- **Sampling frame:** all families from the 4–6 pinned repos (validation +
  held-out strata a–c, plus N3). Stratify the sample on *observable inputs*,
  not the verdict: support band (2–3, 4–9, ≥10), eligible-peer-count band,
  and competitor-presence (largest_competing_support 0 vs >0). Sampling on
  the predicted class itself would condition the sample on the thing under
  test; the input strata reach minority/boundary regions without that bias.
  Target: ~30 families per repo, cap ~150 total.
- **Annotation task:** two annotators, independent, blind to the model's
  class, order randomized. Per family they see an annotation sheet rendered
  from index metadata: the family's member list (repo-relative paths +
  member names), the eligible-peer inventory (the denominator members), and
  competing families' member lists. They read actual source in their own
  local checkout of the pinned repo; **the persisted sheet stores only ids,
  counts, and labels** (privacy: no source, no snippets, no absolute paths).
- **Two questions per family:**
  - Q1 (class): "Among the eligible peers, is this pattern the dominant way
    (clear majority), a supported-but-not-dominant way, or a minority way?"
    3-way forced choice + "cannot judge".
  - Q2 (denominator validity): "Is the eligible-peer set right — does it miss
    obvious peers or include non-peers?" yes / misses-peers / includes-non-peers.
    Prevalence accuracy decomposes into denominator validity × threshold
    validity; without Q2 a threshold error and a peer-set error are
    indistinguishable.
- **Adjudication:** disagreements resolved by discussion with a written
  rationale; unresolved → excluded from accuracy, counted and reported.

### 4.2 Metrics and statistics

- Inter-annotator: Cohen's kappa on Q1 (pre-adjudication). Report raw
  agreement too.
- Model vs adjudicated gold: 3-class accuracy, macro-F1, full confusion
  matrix; `UNKNOWN_PREVALENCE` reported as its own row (abstention, excluded
  from the 3-class denominator but its rate reported).
- Boundary analysis: fraction of families with coverage within ±5pp of the
  1/3 or 3/5 thresholds; accuracy reported with and without boundary cases.
- CIs: Wilson 95% on accuracy; **cluster bootstrap by repo** for all CIs
  (families within one repo are correlated). With n≈150 labels the worst-case
  Wilson half-width is ≈±8pp — state this ceiling in the report.
- **Pre-registration lock:** the thresholds (1/3, 3/5, 2×, min-support 2) are
  frozen for Phase 7. The annotated set may motivate retuning only in a later
  phase, at which point this label set becomes dev and a fresh label set is
  required for any retuned-threshold claim.
- Self-dogfood check: re-list the self-dogfood repo's family class
  distribution at the execution commit and record it in the ledger — the
  baseline's `DOMINANT_PATTERN_ALL` contradiction must be shown resolved
  (distribution now discriminates) or explicitly still open. Diagnostic only;
  not part of the accuracy statistic.

---

## 5. RQ3 alignment-certificate mutation corpus (11 cases)

An alignment certificate is the typed `check`/`explain` verdict of a member
against its family's `FamilyConstraintProfile`. The 11 cases below cover the
full typed semantics space: {Equal, EqualEmpty, MustContain,
ProhibitedPresence} × violation, observed-variation legality, observed-only
discipline, freshness, family erosion, and two specificity controls. (Per
assumption A2: if the program's own 11-case list surfaces, map case ids 1:1
and record the mapping.)

Implementation vehicle: the existing per-query `mutation` mechanism in the
corpus (already proven by `py-stale-fastapi-family`), extended with mutation
kinds `replace_span` / `insert_before` / `delete_span` (path + exact old text
→ new text; exact-match application, hard error if the old text is not found
exactly once) and a per-query `resync_after_mutation: bool`. Anchored on
committed Python fixtures (`fastapi-basic`, `fastapi-route-variation`,
`sqlalchemy-model-strong-evidence`, `pytest-basic`, `low-support`) so gold is
verifiable offline before any public-repo involvement.

| # | Case id | Mutation (fixture anchor) | Expected certificate (typed fields, exact-match gold) |
| --- | --- | --- | --- |
| 1 | `align-noop-conforming` | Comment/whitespace-only edit to a member (`fastapi-basic/app.py`), resync | Member still conforming; no deviation; family unchanged. Specificity control. |
| 2 | `align-new-conforming-member` | Add a new member following the pattern exactly (new route in `fastapi-basic`), resync | Member joins family; certificate conforming; support +1. |
| 3 | `align-equal-violation` | Change a characteristic-profile-bound feature on one member (e.g. decorator shape) so an `Equal` constraint no longer holds, resync | Deviation naming the exact `prefix`, `origin: characteristic_profile`, `semantics: equal`. Localization is part of gold. |
| 4 | `align-equal-empty-violation` | Introduce a value under a prefix the profile binds `EqualEmpty`, resync | Deviation with `semantics: equal_empty` and the offending prefix. |
| 5 | `align-must-contain-violation` | Remove the shared support-family core usage/import from one member (`origin: support_family_intersection`), resync | Deviation with `semantics: must_contain`; member exits or deviates — never silently retained as conforming. |
| 6 | `align-blocker-introduction` | Add a feature matching an `unknown_blocker:` incompatibility rule to a member, resync | Membership excluded; certificate cites `origin: incompatibility_blocker`, `semantics: prohibited_presence`. |
| 7 | `align-novel-variation` | Change a member's value on an `allowed_variations` dimension to a profile **never observed** in the family, resync | Observed-only discipline: NOT certified legal. Gold = deviation-or-typed-UNKNOWN on that dimension; silent acceptance is the failure being tested. |
| 8 | `align-observed-variation` | Switch a member to a *different already-observed* profile on a variation dimension (`fastapi-route-variation`), resync | Conforming; variation dimension cited as legal. Specificity control. |
| 9 | `align-family-erosion` | Mutate members until support drops below `DOMINANT_MINIMUM_SUPPORT` / cluster viability (`low-support` anchor), resync | Family dissolves or reclassifies; `check` against the old family id returns typed UNKNOWN — never a certificate against a ghost family. |
| 10 | `align-stale-no-certificate` | Any member edit **without** resync (`resync_after_mutation: false`) | `StaleEvidence` abstention; no certificate is ever issued on stale evidence (freshness invariant, mirrors `py-stale-fastapi-family`). |
| 11 | `align-cross-family-move` | Rewrite a member to match a *different* family's profile (e.g. pytest fixture → plain helper), resync | Old family drops the member; no certificate against the old family; if the member joins another family, that certificate names the new family. |

Cross-case invariant (checked on every case): `unresolved_obligations` always
contains the runtime-equivalence obligation, and no certificate field ever
claims runtime equivalence. Prose output is never accepted as evidence — gold
matches typed JSON fields only.

Metrics: violation-detection rate over {3,4,5,6,7,9,11}; false-flag rate over
{1,2,8}; freshness correctness on {10}; **localization accuracy** (the named
prefix/origin/semantics matches the mutated feature) reported separately from
detection. Mutation validity guard: each case's applied diff is scope-checked
(exactly the intended span changed; pre/post indexed-unit-count assertion) so
a sloppy mutation cannot masquerade as a detection failure (threat T12).

---

## 6. RQ4 incrementality — scenario matrix over the sync-equivalence oracle

### 6.1 Required oracle contract (interface to the parallel lane, per A1)

Given one workspace, produce and compare: (i) state after `sync` (incremental)
following an edit, vs (ii) state after a fresh `init`+`resync` of the same
tree. Comparison is over a **canonical projection**: family ids, memberships,
classifications, prevalence fields, constraint profiles, and typed unknowns —
excluding generation counters, timestamps, and latencies. Verdict:
`EQUIVALENT | DIVERGENT(diff)` with a typed field-level diff. Work counters:
files reparsed, units re-extracted, families recomputed, wall ms.

### 6.2 Scenario matrix

Edit kinds (rows) × workspaces (columns: `python-v0_1` fixture [small],
self-dogfood repo [medium, dev-only diagnostic], one large pinned public repo
[from Section 2]):

1. No-op (mtime touch only) — the baseline pathology: no-op sync recorded
   *slower* than full rebuild (251s vs 221s); this scenario is the regression
   test for that recorded fact.
2. Comment/whitespace-only edit in one file.
3. Single-member body edit (family-irrelevant semantics preserved).
4. Family-relevant member edit (reuses RQ3 case-3 mutation).
5. Add member (RQ3 case 2). 6. Delete member.
7. Add file introducing a new family. 8. Delete a file. 9. Rename a file.
10. Cross-cutting edit: touch a shared import used by members of many
    families.
11. Branch switch: `git checkout` between two pinned commits of the public
    repo (bulk change).
12. Sequential-edit soak: N=10 small edits with a sync after each
    (accumulated-drift check: equivalence asserted after each step, not just
    the last).

### 6.3 Metrics and verdicts

- **Equivalence is an invariant, not a statistic:** required pass rate is
  100%; any `DIVERGENT` is a correctness bug with a committed reproducer
  (workspace recipe + edit), reported as a finding — never averaged away.
- Incrementality ratio: incremental wall / full-rebuild wall, per scenario ×
  workspace; and work proportionality: files reparsed vs files changed
  (invariant: reparsed ⊆ predicted dirty set ∪ declared dependents).
- Repetitions: verdicts are deterministic — 1 run + 1 confirmation rerun
  (must be identical; a flake is itself a finding). Wall times: 5 reps,
  median + IQR, labeled `MACHINE_DEPENDENT`, never a cross-machine claim.

---

## 7. Statistical plan

### 7.1 What determinism does and does not give us

Verdicts and retrieval metrics are deterministic functions of (corpus, binary
commit, fixture hash) — the harness already asserts parsed-field equality
across repetitions. Therefore **repetition-based CIs on hit@1 etc. are
meaningless and forbidden** (they would be fake precision). The uncertainty
that exists is *sampling* uncertainty over queries/repos/families, plus
machine variance on wall times only.

### 7.2 Units, CIs, and tests

- **Unit of analysis for RQ1:** the normalized-signature equivalence class
  (Section 1.2), because same-class queries are perfectly correlated under a
  deterministic resolver. Report both n_query and n_class; all CIs use
  cluster bootstrap over classes (and over repos for public-repo slices).
- Proportion metrics (hit@1, correct_abstention, unsupported_rejection,
  candidate_recall, RQ3 detection/false-flag rates, RQ2 accuracy): Wilson 95%
  CI at cluster level; always alongside the integer num/denom already
  mandated by the results schema.
- MRR: cluster bootstrap (10k resamples, percentile; BCa if the tooling is
  easy) over classes.
- **Paired comparisons** (product vs token-overlap; product vs each
  ablation): the corpus is identical across conditions, so per-query outcomes
  are paired. Binary metrics: exact McNemar (binomial on discordant pairs),
  computed on class-collapsed outcomes; report discordant counts b and c, not
  just p. MRR deltas: paired cluster bootstrap CI on the per-class
  reciprocal-rank difference; Wilcoxon signed-rank only as a robustness
  footnote (heavy ties limit it).
- **Multiplicity:** pre-register the comparison family — on validation: 4
  condition contrasts × 2 headline metrics (hit@1, correct_abstention) =
  8 tests, Holm–Bonferroni within each metric. Held-out carries exactly one
  confirmatory contrast (product vs token-overlap on hit@1 with the safety
  counters as hard constraints); everything else is descriptive there.
- Power honesty: at n≈40 retrieval queries (≈25 classes) per split, Wilson
  half-widths are ±13–16pp; McNemar detects only large paired effects
  (≥8–10 discordant classes). State this in the report; do not narrate
  sub-CI differences as findings.
- Hard-constraint metrics are not tested statistically:
  `false_family_selections = 0`, `selected_on_abstention_gold = 0`, RQ4
  equivalence 100%, RQ3 case-10 freshness — these are gates; a single
  violation fails the gate regardless of any p-value.
- Latency/resync timings: median/p95 over 5 reps, `MACHINE_DEPENDENT`,
  descriptive only; A/B latency deltas only within one machine+session and
  never as a headline claim.
- RQ2: kappa + accuracy with repo-cluster bootstrap (Section 4.2).

### 7.3 What gets CIs (summary table)

| Quantity | CI? | Method |
| --- | --- | --- |
| hit@1, abstention, recall, RQ3 rates, RQ2 accuracy | yes | Wilson, cluster-level |
| MRR, metric deltas between conditions | yes | cluster bootstrap (paired for deltas) |
| McNemar contrasts | p + discordant counts | exact binomial |
| Safety counters, RQ4 equivalence, freshness invariants | no — hard gates | pass/fail with reproducer on fail |
| Latency, resync wall, incrementality ratios | no — descriptive | median+IQR, MACHINE_DEPENDENT label |
| Self-dogfood one-off facts | no | recorded observations, dated |

---

## 8. Result schema extensions (append-only)

### 8.1 `product-eval-results.v3` (strictly additive over v2)

New optional top-level fields: `split` (`dev|validation|heldout`),
`corpus_sha256`, `ablation` (the echoed token or null — must equal what the
condition implies, harness-asserted), `pinned_repos[]` (repo_id, url,
pinned_commit, tree_sha256, license_spdx — copied from the manifest for
self-containment).

New optional per-result fields: `equivalence_class_id` (normalized-signature
class), `mutation_case` `{case_id, mutation_kind, resync_after_mutation,
scope_check: pass|fail}` for RQ3 rows, and the RQ3 certificate gold/actual
typed fields (`constraint_prefix`, `constraint_origin`,
`constraint_semantics`, `verdict_token`).

New optional `summary.stats` block: per metric `{numerator, denominator,
n_class, ci_method, ci_low, ci_high}`; for paired runs
`paired_against: <condition>`, `discordant_b`, `discordant_c`, `mcnemar_p`,
`delta_ci_low/high`. A legacy v2 reader ignores all of this (additive rule);
the v3 reader is null-tolerant like the existing
`hydrated_family_count` precedent.

RQ4 runs emit a sibling document `sync-equivalence-results.v1`
(`scenario_id`, `workspace_id`, `verdict`, typed diff on divergence, work
counters, wall stats) rather than overloading the query-results schema —
different oracle, different shape.

### 8.2 Ledger and summary conventions

- `docs/experiments/phase7-evaluation.md` + `phase7-evaluation.summary.json`
  following the baseline conventions exactly: `schema_version`, `verdict`
  vocabulary (e.g. `MEASURED`, `GATE_FAILED`, `INCONCLUSIVE`), `privacy`
  block (all false), `limitations[]`.
- **Append-only rule:** prior baseline documents and frozen numbers are never
  edited; new results are new dated sections/files. Held-out access log lives
  in the ledger. The pre-registration (comparison family, gates, split
  freeze hashes, repo manifest hash) is committed *before* the first
  validation-split measurement — that commit SHA is the preregistration
  timestamp.
- Privacy invariants across every artifact: no public-repo source text, no
  snippets, no absolute paths, no raw prompts, no credentials/tokens; repos
  referenced by `repo_id`+SHA only; RQ2 sheets carry ids/counts/labels only.

### 8.3 Claim discipline hooks (RQ5 interface)

No token-savings or agent-impact number may appear in any Phase 7 artifact
except through the existing paired recorder
(`application/telemetry.rs:1728ff`): `claim_validity` must be
`valid_for_product_claim` (paired sessions, same mode/source/task-kind, both
success). Until such a paired experiment exists, every summary carries
`token_measurement.status: NOT_MEASURED` exactly as the baseline does.
RQ1–RQ4 results must never be narrated as "saves tokens/time for agents".

---

## 9. Leakage and validity threats (register)

| # | Threat | Mitigation (section) |
| --- | --- | --- |
| T1 | Calibration leakage: all 73 v1 queries tuned the gates | Entire v1 corpus locked to dev; val/held-out are new (1.1) |
| T2 | Paraphrase leakage across splits | Normalized-signature collision detector as a hard corpus-check error; author separation; intent-first authoring (1.2, 1.4) |
| T3 | Gold circularity: gold authored from the index's own family inventory can't see missing-family blind spots | Intent-first authoring; `family_absent` measured category kept (1.4) |
| T4 | Repo selection bias toward tool-friendly repos | Pre-registered objective criteria, deterministic pool ordering, frozen manifest before any metric, attrition log, structural negative controls N1–N4 (2.1) |
| T5 | Fixture overfitting narrated as real-repo performance | Fixture numbers always labeled fixture-scope; public-repo slices reported separately; content-shift vs form-shift decomposition (1.3) |
| T6 | Ablation contamination (ablated behavior in a production run, or vice versa) | Echo invariant + harness round-trip assert + unset-parity test; hard error on unknown token (3.2) |
| T7 | RQ2 annotator bias / ontology mismatch | Blind to model class, dual annotation + kappa, denominator-validity question Q2, input-stratified (not verdict-stratified) sampling (4.1) |
| T8 | Fake precision from deterministic reruns | Repetition CIs forbidden; sampling CIs at class/repo cluster level only (7.1) |
| T9 | Correlated queries inflating effective n | Equivalence-class unit; cluster bootstrap; both n's reported (7.2) |
| T10 | Held-out erosion by repeated peeking; ablation fishing on held-out | Sealed hash, access log, pre-declared gates, held-out limited to one confirmatory contrast (1.4, 3.1, 7.2) |
| T11 | Upstream drift/force-push changes pinned repo content | tree_sha256 verify on fetch, hard error on mismatch (2.2) |
| T12 | Invalid RQ3 mutations (edit changes more than intended) | Exact-span mutation kinds, scope check, unit-count pre/post assertion (5) |
| T13 | Machine variance narrated as performance results | MACHINE_DEPENDENT labeling; descriptive only; within-machine pairing only (7.2) |
| T14 | TS transitional results inflating the official-scope claim | Python-first headline; TS reported separately per the repository contract (1.3) |
| T15 | Multiple-comparison inflation across 5 conditions × many metrics | Pre-registered 8-test family, Holm–Bonferroni, one held-out contrast (7.2) |
| T16 | Sync-equivalence oracle divergence (A1) between lanes | Section 6.1 is the interface contract; renegotiate explicitly, never silently adapt (6.1) |

---

## 10. Execution-order sketch (for the implementing session, non-binding)

1. Land ablation mechanism + echo/parity tests (3.2). 2. Corpus schema v3 +
collision detector + intent audit (1.2, 8.1). 3. Repo selection log + manifest
freeze + pinning fetch (2). 4. Author validation split; freeze hash;
pre-register comparisons/gates (1.3, 7.2). 5. Run dev+validation matrix (3.1).
6. RQ3 mutation corpus on fixtures (5). 7. RQ2 sampling + annotation (4).
8. RQ4 scenario runs once the sync-equivalence lane lands its oracle (6).
9. Held-out authoring by a separate author; seal; single confirmatory run at
the phase gate. 10. Ledger + summary JSON per Section 8.
