# Query Resolution Specification

This specification defines RepoGrammar's deterministic, dependency-free query
resolution: how a raw fuzzy target is normalized into typed retrieval signals and
how those signals rank the family index. It has no LLM, embedding, or network
dependency.

## Status: substrate, not yet routed

The normalization and retrieval layer described here is implemented in
`application::query_terms` and backed by the `list_active_family_search_summaries`
store projection. It is **substrate**: fully typed, tested, and documented, but
**not yet wired** into `fuzzy_family_match_set` or the production lookup paths.
Today the production fuzzy path still resolves a target only when it equals a
family id, a `unit:` member id, an exact member role, or an exact `//`-suffix
evidence path; natural-language targets therefore abstain with
`InsufficientSupport`. Wiring this substrate into the lookup path and calibrating
its abstention threshold land in a following change.

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
breakdown (`framework_filter`, `concept`, `language_filter`, `residue_hits`) for
the route-metadata explanation the next wave will render.

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
