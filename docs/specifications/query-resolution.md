# Query Resolution Specification

This specification defines RepoGrammar's deterministic, dependency-free query
resolution: how a raw fuzzy target is normalized into typed retrieval signals and
how those signals rank the family index. It has no LLM, embedding, or network
dependency.

## Status: routed with calibrated abstention

The normalization and retrieval layer described here is implemented in
`application::query_terms`, backed by the `list_active_family_search_summaries`
store projection, and **routed into the production lookup path**
(`application::query::lookup_family_with_freshness_and_local_context`). Natural
language, synonym, and framework-plus-concept targets now resolve deterministically
with calibrated absolute-score and margin abstention; bare frameworks, bare
concepts, typos, and genuinely ambiguous targets abstain with a typed `UNKNOWN`
and a low-cardinality route reason. Vocabulary v2 additionally recognizes a
bounded set of two-term qualified concepts without relaxing either selection
gate.

## Routed pipeline

For a `FuzzyQuery`, resolution runs these stages in strict priority order. Each
earlier stage is authoritative: a lower-priority signal never overrides a valid
exact identifier.

1. **Exact authority (unchanged, first).** Exact family id, `unit:` member id,
   exact member role, and exact `//`-suffix evidence path — including their own
   candidate-set and ambiguity abstentions — resolve exactly as before. When an
   exact/role/evidence layer finds candidates but cannot pick one (a `unit:`
   member in more than one family, a truncated candidate set, or a multi-family
   role ambiguity), that block keeps its own claim (`query target candidate set`
   or `query target ambiguity`), its candidate family ids, and its narrowing
   recovery text verbatim; term retrieval never runs for it and never rewrites it.
2. **Term retrieval (this spec).** Runs **only** when the exact/role/evidence
   layers produced **no candidate at all** — exactly one `InsufficientSupport`
   block with claim `query target` **and an empty `candidate_family_ids`** —
   **and** the target is **not path-locator-shaped** (`target_has_path_locator_shape`
   is false: no whitespace token contains `/` and none ends in a known indexed
   source-file extension). A single interior-dotted word in prose
   (`fastapi.Depends`, `0.100`, `e.g.`) is **not** path-shaped, so such queries
   still reach term retrieval; genuine file locators (`app/routes.py`, `models.ts`,
   `app.py:12`) stay on the exact/local-context path. Eligible targets flow into
   normalization → scoring → the abstention gates below → bounded hydration.
3. **Local-context read plan (unchanged).** Where term retrieval abstains and its
   preconditions still hold (a path-shaped target resolving to one indexed file or
   unit), the existing `PARTIAL_CONTEXT` local-context fallback still applies.
4. **Directory / composite scope resolution.** When every earlier stage still
   abstained with an empty-candidate `query target` block and the parsed target
   named a directory scope (a `/`-containing token that is not a file locator,
   e.g. `src/rust/interfaces/cli`) — alone or combined with concept, framework, or
   language ranking signals — the scope is resolved deterministically (see below).
   A non-directory target with no hard-constraint conflict flows through this stage
   unchanged, so every prior outcome is byte-identical.

### Directory and composite scope resolution

The target parser (`application::query_resolution::parse_target`) classifies a
raw target into orthogonal HARD (exact `family:`/`unit:`/path locks), SCOPE
(directory prefixes, language), and RANKING (concept, framework role, symbol,
residue) tiers, and records any typed hard-constraint conflict without resolving
it. Stage 4 acts on the SCOPE tier:

- **Scope establishment.** Each directory prefix is normalized (strip `./`,
  trailing slash) and safety-checked by the shared `is_safe_query_path_text`
  authority; an absolute path, a `..` traversal, a backslash, a scheme, a control
  character, or an empty segment is rejected and never used to read. Each safe
  prefix is resolved through the bounded `IndexStore::list_active_files_in_directory`
  read-model port — a prefix range scan over the `(generation_id, path)` primary
  key that returns at most a fixed limit of child files in `path` order and a
  `truncated` flag. The scoped read and every family lookup are checked against the
  same active `generation_id`; a resync mismatch abstains unchanged.
- **Family mapping.** The bounded scoped files are mapped to the pattern families
  whose evidence lives within the scope (via the existing
  `find_active_families_by_evidence_path` projection). Multiple directory scopes
  default to **intersection**. A composite target additionally intersects the
  in-scope families with the positively-scored families from the single
  term-retrieval scoring authority, so `app/api route` narrows to the family that
  is both in the directory and matches the concept.
- **Outcomes.** A single family in an untruncated scope resolves to `Found`
  (hydrated through the same freshness + profile gates). A **heterogeneous** scope
  (more than one distinct family) surfaces the competing families through
  `candidate_family_ids` with **no** `selected_family_id` — zero false selection.
  A scope that resolves to indexed files but no matching family returns
  `PARTIAL_CONTEXT` with a `directory_scope` resolved target (the bounded child
  paths as candidate handles, an empty read plan — the locus is a directory, not a
  single file) and no invented family. A scope that resolves to no indexed files
  returns a typed `UNKNOWN` naming the accurate reason. Jointly unsatisfiable
  directory scopes, and any parsed hard-constraint conflict (multiple `family:`
  ids, multiple `unit:` ids, or a mixed family/unit identity), return a typed
  conflict `UNKNOWN` — never a union or a silently-dropped constraint.
- **Truncation is never silent.** When the scoped read exceeds its bound, unseen
  files might belong to other families, so the resolver never claims a single
  family under truncation: it surfaces the seen families as candidates with the
  truncation stated in the source-free recovery text. The public
  `resolution.cardinality` field and the bounded candidate-set outcome projection
  with summaries are deferred to a later phase; this stage only never false-selects
  and keeps candidates as handles.

A resolved family detail carries a hydrated, metadata-only `constraint_profile`
(the source-backed specification, or `null` when none was persisted) and a
`read_plan` whose purposes follow representative selection: `canonical_evidence`
names the cluster medoid, `support_evidence` prefers the `contrast`-labelled
support witness (hydration re-sorts evidence by path, so the label — not the
write order — carries the choice; it falls back to the first distinct-path
`support` member), and `variation_guard` a variation witness. Read-plan items are ordered by purpose
priority (`target_body_required_for_edit`, then `canonical_evidence`,
`support_evidence`, the guard purposes, and `optional_context` last), so when a
`token_budget` truncates the plan the retained prefix always keeps the most
decision-critical spans. A truncated plan is honest: at `verbosity: minimal` it
carries a `truncated` flag and `item_count`. When source spans are rendered
(`include_source_spans`), the rendered locus is the single source of truth and
is treated as already read; at `minimal` such an item is left as a
`{purpose, path, rendered: true}` back-reference rather than duplicated in full,
and the empty `source_spans` stub is omitted when spans are not requested.
`standard` and `full` keep the full read-plan items and the stub unchanged.
Evidence-mode selection covers the canonical and support constraints plus one
witness per observed variation dimension (and the anchor-target dimension) when
the profile is hydrated. See the Representative selection rule and the
`FamilyConstraintProfile` in `docs/specifications/domain-model.md`.

### Abstention gates and reasons

A term-retrieval query resolves to a family **only** when the top candidate clears
every gate; otherwise it returns a typed `UNKNOWN` (`InsufficientSupport`, claim
`query target`) plus a low-cardinality `abstention_reason`. Named calibration
constants live in `application::query`:

| Constant | Value | Meaning |
| --- | --- | --- |
| `MIN_RETRIEVAL_SCORE` | `10` | Absolute selection floor. Equals `WEIGHT_FRAMEWORK_FILTER + WEIGHT_CONCEPT` (6 + 4) and `WEIGHT_QUALIFIED_CONCEPT` (10). The score is **additive** over framework/concept/language/residue, so this is a floor, not a structural framework+concept requirement: the common resolving shapes are framework + concept or one committed qualified concept, while concept + enough residue hits can also clear it. |
| `MIN_RETRIEVAL_MARGIN` | `1` | The top candidate must beat any **competing family that itself clears `MIN_RETRIEVAL_SCORE`** by at least this score. A runner-up below the floor is not a competitor and never forces abstention. |
| `MAX_RETRIEVAL_HYDRATIONS` | `5` | Defensive ceiling on top-tier candidates hydrated through the freshness gate (equals `FUZZY_FAMILY_CANDIDATE_LIMIT`). In practice the margin gate reduces the winning score tier to one family before hydration, so this bound does not bind. |

A selection requires exactly two conditions: the top candidate clears
`MIN_RETRIEVAL_SCORE`, **and** it carries a pattern-concept signal. Gates, in
evaluation order, and their `abstention_reason` token:

1. `no_candidate` — nothing scored a positive signal.
2. `below_min_score` — top score `< MIN_RETRIEVAL_SCORE`. A bare concept (4),
   bare framework filter (6), or bare language filter (2) always abstains here.
3. `unsupported_target` — the top candidate cleared the floor on filters/residue
   alone, with no pattern-concept signal (e.g. a framework filter plus residue
   that substring-matches role tokens — a typo such as `framework:fastapi.rout`).
4. `truncated_tie` — the ranking was truncated at K with a competitor tied at the
   top score.
5. `margin_too_close` — a competing family that itself clears `MIN_RETRIEVAL_SCORE`
   is within `MIN_RETRIEVAL_MARGIN` of the top. (Each summary is one row per
   family, so the runner-up is always a different family.)
6. `stale_candidates` — hydration ran but the evidence-freshness gate rejected
   every candidate.
7. `hydration_ambiguous` — a defensive backstop for more than one hydrated
   candidate surviving the single-fresh-family gate. The margin gate reduces the
   winning score tier to a single family before hydration, so this is not reached
   in practice; it keeps the single-fresh-family invariant explicit.

When the gates pass, the winning candidate (the single member of the top score
tier, after the margin gate) is hydrated through the existing
`family_evidence_is_fresh` + single-fresh-family gates and returned as `Found`.
Hydration is skipped entirely when a score/margin gate already abstains.

If a resync lands between the exact layers and term retrieval, the search-summary
projection may describe a newer generation than the exact layers abstained
against. Rather than score and attribute a different generation, term retrieval
detects the generation mismatch and returns the exact-layer `query target` block
unchanged, so a re-query resolves consistently against a single generation.

Because a `PARTIAL_CONTEXT` local-context resolution replaces the term-retrieval
`UNKNOWN`, the `term_retrieval` route object and its `abstention_reason` are
recorded only on `Found` and `UNKNOWN` outcomes; a query that abstains in term
retrieval and then resolves to `PARTIAL_CONTEXT` carries the read-plan context
instead. In the routed pipeline this is rare: term retrieval only claims
non-path-shaped targets, which the local-context path does not resolve.

### Route metadata

Term-retrieval queries extend the `query_route` report with a source-free
`term_retrieval` object (no raw target text is ever persisted): `route`
(`term_retrieval_hydrate` | `term_retrieval_unknown`), `retrieved_summary_count`,
`ranked_candidate_count`, `hydrated_candidate_count`, `retrieval_stage_count`,
raw `top_score`/`margin`, low-cardinality `top_score_bucket`/`margin_bucket`,
`truncated`, the selected candidate's typed `matched_signals`, and
`abstention_reason` (one of the seven tokens above, or null when found). The
top-level `query_route.route` token is unchanged (`discover_hydrate_compose` for a
term-retrieval match, `discovery_unknown` for a term-retrieval abstention), and
`hydrated_family_count` / `retrieval_stage_count` are surfaced at the
`query_route` top level. The abstention reason is additionally rolled up in
anonymous telemetry as the enum-only `by_abstention_reason` dimension.

Response density is gated by `verbosity` (orthogonal to `mode`). `standard` (the
default) and `full` emit the full `query_route` object above and are
byte-identical. `minimal` keeps only the two decision-critical fields — `route`
and `follow_up_family_ids`, the single canonical handle list that is the
normalized union of `candidate_family_ids` and `selected_family_id`, so no id is
lost. `candidate_family_ids` is retained at `minimal` only where it is a
narrowing recovery handle (`PARTIAL_CONTEXT`, `UNKNOWN`, and conformance
abstentions); on a matched family it duplicates the follow-up handle and is
dropped. `selected_family_id` and every diagnostic routing field (`input_kind`,
`pipeline`, `family_id_policy`, `candidate_limit`, `why_selected`,
`hydrated_family_count`, `retrieval_stage_count`, `term_retrieval`) are demoted
out of the `minimal` shape. Abstention still never leaks a family: the reduction
only removes ids already carried by `follow_up_family_ids`.

### Calibration summary

Calibrated against the 79-query product-eval corpus (`query-corpus-v1.json`;
42 retrieval + 25 abstention + 12 context after the gold adjudications). With
vocabulary v2 at `MIN_RETRIEVAL_SCORE = 10` and `MIN_RETRIEVAL_MARGIN = 1`,
hit@1 is **25/42** and MRR is **0.595**, up from vocabulary v1's 21/42 and
0.500. The four additional matches are the two fixture phrasings and the
`unit test` / `test case` synonym queries covered by the qualified table. Every
hard constraint remains unchanged: zero false-family selections, 25/25 correct
abstentions, zero selections on abstention gold, 4/4 unsupported rejections,
6/6 ambiguity precision, 13/14 candidate recall, and 12/12 context matches.

The earlier v1 threshold sweep put hit@1 on a plateau across
`MIN_RETRIEVAL_SCORE ∈ {7, 8, 10}` (all 21/42); `= 11` regressed to 18/42 (the
framework+concept anchor scores exactly 10). Vocabulary v2 does not change
either gate: `10` remains the principled absolute floor, equal to both
`WEIGHT_FRAMEWORK_FILTER + WEIGHT_CONCEPT` and `WEIGHT_QUALIFIED_CONCEPT`, and
keeps the non-qualified abstention decoys below the floor.

Some natural-language retrieval queries deliberately abstain because they are
indistinguishable, after normalization, from an intentional abstention decoy — for
example `endpoint` shares its `{concept: route}` normalized form with the
abstaining `How are API routes implemented?` and `handler`, so any resolver that
serves one serves all three; and `unit test`/`test case`/`writing tests` normalize
to `{concept: test}` (a bare concept, score 4) below the floor. These are missed
hits, never constraint violations. The `repository` data-access concept and the
dual `model` concept (validation model + data-access model) are now committed
substrate, so `How do Prisma repositories work?` resolves and `How are models
defined?` remains genuinely ambiguous with both model kinds reachable as
candidates.

## Normalization

`normalize_query(raw) -> NormalizedQuery` is **total** (never errors or panics on
any input) and **bounded** (at most `MAX_QUERY_TOKENS = 64` whitespace tokens,
`MAX_QUERY_TOKEN_BYTES = 128` bytes inspected per token, and `MAX_RESIDUE_TERMS =
32` residue terms). Given identical input it always returns an identical value.

`NormalizedQuery` has five **disjoint** buckets — a term contributes to exactly
one, and stopwords contribute to none. A qualified phrase consumes both of its
terms into one qualified-concept entry:

- `language_filters`: canonical language tokens (e.g. `python`).
- `framework_filters`: framework tokens (e.g. `fastapi`).
- `concept_tokens`: typed `Concept` values.
- `qualified_concept_tokens`: typed concepts named by a committed two-term
  phrase.
- `residue_terms`: leftover normalized fuzzy terms, plus verbatim passthrough
  handles.

### Pipeline

1. ASCII case folding applies to the fuzzy residue only.
2. Whitespace tokenization; each token is classified independently.
3. **Passthrough**: a token that starts with `unit:` or `family:`, contains `/`,
   or ends in a `:line` / `:line-line` locator is preserved **verbatim**
   (case-sensitive) in `residue_terms` for the exact-precedence layer. It is not
   split or folded.
4. **Compound language detection** runs before punctuation splitting so `c#` and
   `c++` are recognized before their punctuation is stripped.
5. Otherwise the token is lowercased and split on punctuation into subtokens;
   subtokens are folded singular and adjacent pairs are checked against the
   bounded qualified-concept table. A matching pair is consumed once. Remaining
   terms are matched in order: stopword (dropped) → language alias → framework
   alias → concept alias → residue term (kept only when length ≥ 2 and the
   residue cap is not reached).

### Committed vocabulary tables

All tables live in `application::query_terms` as committed `pub const` values.
Every alias resolves to a token the current index vocabulary can actually
produce; the module's alias-consistency tests enforce this against
`KNOWN_FRAMEWORK_TOKENS` (43 tokens), `KNOWN_LANGUAGES` (12), and the
`ROLE_CONCEPTS` table.

| Table | Entries | Purpose |
| --- | --- | --- |
| `STOPWORDS` | 28 | Interrogatives and function words dropped before matching. |
| `PLURALS` | 16 | Singular/plural folding (`routes → route`, `cases → case`, `queries → query`, …). |
| `LANGUAGE_ALIASES` | 12 | `python`/`py`, `typescript`/`ts`, `javascript`/`js`, `rust`/`rs`, `java`, `csharp`, `cpp`/`cxx`. |
| `COMPOUND_LANGUAGE_ALIASES` | 2 | `c# → csharp`, `c++ → cpp`, detected before punctuation splitting. |
| `FRAMEWORK_ALIASES` | 27 | Framework term → one or more producible framework tokens. |
| `CONCEPT_ALIASES` | 21 | Term → `Concept`. |
| `QUALIFIED_CONCEPT_ALIASES` | 3 | Two aligned terms → one qualified `Concept`: `test fixture`, `unit test`, and `test case` (singular/plural folded). |
| `ROLE_CONCEPTS` | 54 | `framework_role` → `Concept`. |

The concept vocabulary is exactly five tokens: `route`, `fixture`,
`validation_model`, `data_access`, `test`. There is intentionally **no**
`migration` concept — no framework role produces migrations in the current
vocabulary — so `migration`/`migrations` folds to a residue term instead. Bare
`c` is intentionally not a language alias (too ambiguous); the C family is reached
through `cpp`/`csharp` and the compound aliases. `controller` is not a concept
alias (not in the committed route-term list) and folds to residue.

Framework aliases expand to every producible token a spelling covers: `jest` and
`vitest` both map to `jest_vitest`; `junit` covers `junit4` and `junit5`; `spring`
covers `spring`, `spring_boot`, and `spring_data`; `aspnet` maps to `aspnetcore`.

`ROLE_CONCEPTS` maps 54 concrete roles (17 route, 2 fixture, 4 validation-model,
13 data-access, 18 test). Roles with no concept (CLI commands, tasks, generic
components, framework entrypoints) are omitted; such families can still match via
a framework filter or residue terms.

## Metadata projection contract

`list_active_family_search_summaries` is one bounded store read that projects, per
active-generation family, a source-free `IndexedFamilySearchSummaryRecord`:

- `family_id`, `language`, `code_unit_kind`, `framework_role`
- `classification` (prevalence class) and `prevalence`
- `support` (distinct member count)
- `evidence_path_components`: distinct repo-relative path segments (ancestor
  directory components and basenames) from the family's evidence paths, sorted and
  capped at `FAMILY_SEARCH_PATH_COMPONENT_CAP = 16`.

The projection is **strictly source-free**: it carries no source text, comments,
snippets, raw queries, or absolute paths — only structural identifiers,
classification, counts, and repo-relative path segments. `support_targets` are
**not** persisted per evidence row (they are transient during family building), so
no per-evidence support token appears in the projection; the framework token is
derived at score time from `framework_role`.

Language, code-unit kind, and framework role are uniform across a family's members
by construction (part of the family key), so a single grouped query aggregates
them with `MIN` while a correlated subquery collects the family's distinct
evidence paths. The read is generation-consistent (all rows come from one active
generation) and deterministically ordered by `family_id` in byte order, mirroring
the family-evidence freshness projection.

## Scoring

`score_family_candidates(normalized, summaries) -> FamilyCandidateRanking` ranks
the projection with explainable additive integer weights (named constants):

| Signal | Weight |
| --- | --- |
| Framework-filter match (role's framework token is in the filter set) | `WEIGHT_FRAMEWORK_FILTER = 6` |
| Concept match (role's concept is in `concept_tokens`) | `WEIGHT_CONCEPT = 4` |
| Qualified-concept match (role's concept is in `qualified_concept_tokens`) | `WEIGHT_QUALIFIED_CONCEPT = 10` |
| Language-filter match | `WEIGHT_LANGUAGE_FILTER = 2` |
| Per distinct residue-term hit (role tokens, framework token, language, kind, or path components; capped at `MAX_RESIDUE_HITS_SCORED = 4`) | `WEIGHT_RESIDUE_HIT = 3` |

Rules:

- **Hard exclusion**: when a language filter is present, a family in a different
  language is excluded; when a framework filter is present, a family whose role
  token is not in the filter set is excluded. A `python` filter can never surface
  a `typescript` family.
- Prevalence is a **tiebreak only**, never a gate: higher coverage ranks above
  lower within the same score and class, never as an exclusion.
- A candidate is retained only with a strictly positive score, so unrelated
  families are never surfaced.
- **Empty query → empty ranking**: a query with no language, framework, concept,
  or residue signal (for example a stopword-only question) scores to zero
  candidates and never dumps all families.

### Deterministic total order and cap

Candidates sort by: score descending → prevalence class ascending
(`DOMINANT_PATTERN` < `SUPPORTED_PATTERN` < `MINORITY_PATTERN` <
`UNKNOWN_PREVALENCE`) → coverage descending → `family_id` byte order ascending.
Ranks are 1-based positions. At most `MAX_RANKED_CANDIDATES = 16` candidates are
retained; `FamilyCandidateRanking.truncated` flags when more scored above zero.

Each retained candidate carries a typed, low-cardinality `MatchedSignals`
breakdown (`framework_filter`, `concept`, `language_filter`, `residue_hits`) that
the routed pipeline renders as the `term_retrieval.matched_signals` route
metadata (see [Route metadata](#route-metadata)).

### Worked example

`How are FastAPI routes implemented?` normalizes to `framework_filters =
{fastapi}`, `concept_tokens = {route}` (`How`, `are`, `implemented` are
stopwords). A `framework:fastapi.route` family scores `6 + 4 = 10` with signals
`{framework_filter, concept}` and ranks first; `flask`/`hono` route families are
excluded by the framework filter, and `pytest`/`sqlalchemy` families never share
the `fastapi` token.

`How are test fixtures defined?` normalizes `test fixture` into
`qualified_concept_tokens = {fixture}`; the two terms do not also emit the broad
`test` concept. A unique fixture family scores 10, while multiple fixture
families would still tie and abstain through the unchanged margin gate. A typo
such as `pytset fixture` and broad `tests written` do not match the qualified
table and remain below the floor.

## Determinism guarantees

Normalization and scoring are pure, total, and deterministic; ordering is a total
order with no float-edge ambiguity (coverage compared with `f64::total_cmp`, ids
by byte order). All bounds — token count, token length, residue count, residue
hits scored, path components projected, and retained candidates — are fixed named
constants.

## Static-alignment check resolution

The `check` operation (CLI `check`, MCP `check_conformance`,
`FamilyLookupMode::Conformance`) reuses this resolution pipeline and layers a
source-backed static-alignment certificate on top. It is implemented in
`application::query::check_static_alignment` and decides the alignment with the
single authority `application::conformance::compute_alignment`. It never proves
runtime equivalence: every certificate carries `runtime_equivalence: UNKNOWN`.

Resolution proceeds in three deterministic stages:

1. **Locator-honoring target resolution.** `check` resolves the target to exactly
   one indexed code unit — it does not delegate unit selection to the fuzzy family
   lookup, and there is no canonical-member fallback:

   - a `unit:` member id pins that unit directly;
   - a `path:byte-start-byteend` or `path:line` locator pins the *innermost* code
     unit that contains the location (a `path:line` locator is mapped to a byte
     offset by reading the target source);
   - a path-only target pins the file's single family-eligible unit; a file with
     more than one family-eligible unit is **ambiguous** and abstains with
     `INSUFFICIENT_EVIDENCE` and candidate unit ids (narrow with a locator);
   - the target file's content hash is verified against the store first (reusing
     the shared `verify_evidence_path` primitive); a stale file abstains with
     `INSUFFICIENT_EVIDENCE`/`StaleEvidence` before any certificate is built;
   - a target that matches no indexed file/unit, or whose locator contains no unit,
     abstains without a certificate.

2. **Comparison-family selection.** When the resolved unit is a member
   (`find_active_families_by_member`), its own family is the comparison family
   (`MEMBER`). When it is a non-member, the single fresh ready family of its
   `(language, kind, role)` key is selected from the source-free
   `list_active_family_search_summaries` projection. Multiple plausible families
   abstain with `INSUFFICIENT_EVIDENCE` and candidate ids; no family for the key,
   or a target with no supported role or non-eligible kind, abstains
   `OUT_OF_SCOPE`; a stale comparison family abstains with `StaleEvidence`. A
   selected family is **never** surfaced for an abstaining outcome (the field is
   structurally `None`), so an abstention is never telemetered as a resolved
   outcome.

3. **Feature extraction and alignment.** The target's feature profile is extracted
   by the SAME family-induction authority that built the family's constraint
   profile (`family::extract_target_unit_features`, reusing the per-unit feature
   map, the per-language characteristic/variation prefix tables, the typed
   blocking-unknown vocabulary, and the repository-level rust build-variant
   blocker — source is never re-parsed). `compute_alignment` then compares the
   profile against the family's constraint profile and returns the deterministic
   outcome:

   - any required-feature *violation* or a prohibited-presence match →
     `STATIC_DEVIATION`;
   - else a blocking unknown, a non-violating deviation signal (an unobserved or
     truncated variation, or a blocking-suppressed requirement), or degraded
     extraction → `PARTIAL_ALIGNMENT`;
   - else every required constraint matched with no deviation →
     `STATICALLY_ALIGNED`;
   - no or ambiguous family → `INSUFFICIENT_EVIDENCE`; otherwise `UNKNOWN`.

### Deviation precedence under blocking unknowns

A blocking unknown on the target plausibly suppressed a feature from the static
view, so an **absence-driven** required check must not fabricate a
`STATIC_DEVIATION` from an incomplete view. **Presence-driven** checks — a value
that is definitely present and wrong or prohibited — still deviate:

For an `Equal` constraint the absence/presence split is decided by set containment:
the failure is **absence-driven** exactly when the observed values are a strict
subset of the expected set (the empty set included) — the target carries only
expected values but is missing one or more. It is **presence-driven** as soon as
any offending value is present (a value not in the expected set), even when
required values are simultaneously missing; presence always wins.

| Constraint | Failure shape | With blocking unknown | Without blocking unknown |
| --- | --- | --- | --- |
| `Equal` | observed empty (absence) | `blocking_suppressed_requirement` → PARTIAL | `required_mismatch` → DEVIATION |
| `Equal` | observed a strict subset of expected — pure missing values, no offending value (absence) | `blocking_suppressed_requirement` → PARTIAL | `required_mismatch` → DEVIATION |
| `Equal` | observed carries an offending value not in expected — extra or wrong, even if values are also missing (presence) | `required_mismatch` → DEVIATION | `required_mismatch` → DEVIATION |
| `MustContain` | core missing (absence) | `blocking_suppressed_requirement` → PARTIAL | `missing_required_core` → DEVIATION |
| `EqualEmpty` | value present (presence) | `must_be_empty_violation` → DEVIATION | `must_be_empty_violation` → DEVIATION |
| `ProhibitedPresence` | value present (presence) | `prohibited_presence` → DEVIATION | `prohibited_presence` → DEVIATION |

### Variation truncation

Observed-profile enumerations are capped (`CONSTRAINT_OBSERVED_PROFILE_CAP`). When
a dimension's enumeration was truncated, a target profile that is not among the
enumerated profiles cannot be proven "never observed": it is reported as a
`truncated_observation` deviation (a partial-alignment signal, never a violation),
distinct from `unobserved_variation` which asserts the value was genuinely never
observed among an untruncated enumeration.

The non-member relationship is classified deterministically as `BLOCKED_UNKNOWN`
(a blocking unknown prevented membership), `EXCEPTION` (a required-feature
violation against the only ready family of its key — source-backed negative
evidence), or `NEAR_MISS` (satisfies every required constraint but was not
admitted); `OUT_OF_SCOPE` names an unsupported kind/role. `COMPETING_PATTERN` is
**reserved and not yet emitted**: a member always compares against its own family,
so no current path constructs it. Deviation summaries are RepoGrammar feature
TOKENS, never repository source text.

### Certificate serialization: scale guard and duplicate handles

The serialized certificate (CLI and MCP, byte-parallel) applies a fixed
scale-protection cap (`ALIGNMENT_DEVIATION_CAP`) to the otherwise structurally
unbounded `static_deviations[]` and `legal_observed_variations[]` arrays. When a
target exceeds the cap the array is truncated to the bound and the computation
gains an honest `<name>_truncated: true` flag plus a `<name>_count` total; below
the cap the full arrays are emitted with no extra fields. The certificate's
top-level `status` and `alignment_status` carry the same token; the
`alignment_status` duplicate is suppressed at `verbosity: minimal`. The top-level
`selected_family_id` is the authoritative carrier of the selected-family handle
and is retained at every tier — the `query_route.selected_family_id` copy is the
one the route lane suppresses at `minimal`, so the certificate top-level copy is
what keeps the selection determinable in the lean shape. The invariant
`runtime_equivalence: "UNKNOWN"` is emitted at every tier and never suppressed.
`standard` and `full` keep the complete certificate shape byte-for-byte. See
`docs/specifications/mcp-api.md` and `docs/specifications/cli.md` for the full
per-field output contracts.
