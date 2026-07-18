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
and a low-cardinality route reason.

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

A resolved family detail carries a hydrated, metadata-only `constraint_profile`
(the source-backed specification, or `null` when none was persisted) and a
`read_plan` whose purposes follow representative selection: `canonical_evidence`
names the cluster medoid, `support_evidence` prefers the `contrast`-labelled
support witness (hydration re-sorts evidence by path, so the label — not the
write order — carries the choice; it falls back to the first distinct-path
`support` member), and `variation_guard` a variation witness. Evidence-mode
selection covers the canonical and support constraints plus one witness per
observed variation dimension (and the anchor-target dimension) when the profile
is hydrated. See the Representative selection rule and the
`FamilyConstraintProfile` in `docs/specifications/domain-model.md`.

### Abstention gates and reasons

A term-retrieval query resolves to a family **only** when the top candidate clears
every gate; otherwise it returns a typed `UNKNOWN` (`InsufficientSupport`, claim
`query target`) plus a low-cardinality `abstention_reason`. Named calibration
constants live in `application::query`:

| Constant | Value | Meaning |
| --- | --- | --- |
| `MIN_RETRIEVAL_SCORE` | `10` | Absolute selection floor. Equals `WEIGHT_FRAMEWORK_FILTER + WEIGHT_CONCEPT` (6 + 4). The score is **additive** over framework/concept/language/residue, so this is a floor, not a structural framework+concept requirement: the common resolving shape is framework + concept, but concept + enough residue hits can also clear it. |
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

### Calibration summary

Calibrated against the 73-query product-eval corpus (`query-corpus-v1.json`;
42 retrieval + 25 abstention + 6 context after the gold adjudications). At
`MIN_RETRIEVAL_SCORE = 10` (with `MIN_RETRIEVAL_MARGIN = 1`) hit@1 is **21/42**
(up from the pre-routing 17/43) while holding every hard constraint — zero
false-family selections, 25/25 correct abstentions, 4/4 unsupported rejections,
6/6 ambiguity precision, 14/14 candidate recall, and no regression among
previously-matching exact/context queries. hit@1 is on a plateau across
`MIN_RETRIEVAL_SCORE ∈ {7, 8, 10}` (all 21/42); `= 11` regresses to 18/42 (the
framework+concept anchor scores exactly 10). `10` is retained as the principled
absolute floor: it equals `WEIGHT_FRAMEWORK_FILTER + WEIGHT_CONCEPT` and keeps the
identical-normalization abstention decoys (below) abstaining with the widest
margin.

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

`NormalizedQuery` has four **disjoint** buckets — a term contributes to exactly
one, and stopwords contribute to none:

- `language_filters`: canonical language tokens (e.g. `python`).
- `framework_filters`: framework tokens (e.g. `fastapi`).
- `concept_tokens`: typed `Concept` values.
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
5. Otherwise the token is lowercased and split on punctuation into subtokens; each
   subtoken is folded singular, then matched against the alias tables in order:
   stopword (dropped) → language alias → framework alias → concept alias →
   residue term (kept only when length ≥ 2 and the residue cap is not reached).

### Committed vocabulary tables

All tables live in `application::query_terms` as committed `pub const` values.
Every alias resolves to a token the current index vocabulary can actually
produce; the module's alias-consistency tests enforce this against
`KNOWN_FRAMEWORK_TOKENS` (43 tokens), `KNOWN_LANGUAGES` (12), and the
`ROLE_CONCEPTS` table.

| Table | Entries | Purpose |
| --- | --- | --- |
| `STOPWORDS` | 28 | Interrogatives and function words dropped before matching. |
| `PLURALS` | 15 | Singular/plural folding (`routes → route`, `queries → query`, …). |
| `LANGUAGE_ALIASES` | 12 | `python`/`py`, `typescript`/`ts`, `javascript`/`js`, `rust`/`rs`, `java`, `csharp`, `cpp`/`cxx`. |
| `COMPOUND_LANGUAGE_ALIASES` | 2 | `c# → csharp`, `c++ → cpp`, detected before punctuation splitting. |
| `FRAMEWORK_ALIASES` | 27 | Framework term → one or more producible framework tokens. |
| `CONCEPT_ALIASES` | 21 | Term → `Concept`. |
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

## Determinism guarantees

Normalization and scoring are pure, total, and deterministic; ordering is a total
order with no float-edge ambiguity (coverage compared with `f64::total_cmp`, ids
by byte order). All bounds — token count, token length, residue count, residue
hits scored, path components projected, and retained candidates — are fixed named
constants.
