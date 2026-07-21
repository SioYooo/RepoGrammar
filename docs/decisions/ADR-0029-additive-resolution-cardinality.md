# ADR-0029: Additive candidate-set resolution cardinality

- Status: Accepted
- Date: 2026-07-21
- Refines: ADR-0018 (query discovery and hydration routing), ADR-0013 (agent
  adoption and read displacement)

## Context

The universal-target-resolution work (Phases 1–2) taught RepoGrammar to resolve a
directory or composite scope (e.g. `app/api`, `app/api route`) to the pattern
families that occupy it. Phase 2 already refused to false-select: a heterogeneous
or truncated scope abstained with a typed `UNKNOWN` that carried the competing
families as `candidate_family_ids`.

That shape had two problems. First, a resolved locus with several real in-scope
families surfaced the same generic `UNKNOWN` as a genuinely unresolvable target,
so a consumer could not distinguish "no locus" from "a real candidate set awaiting
a choice". Second, the outcome carried the candidate ids but no per-candidate
context, so choosing among them still required a follow-up `show_family` per id.

We wanted to express the **cardinality** of a resolved candidate set as a
first-class, source-free projection, while keeping the pre-1.0 compatibility
promise that `standard`/`full` responses stay byte-stable except for additive
fields, and without ever letting a multi-family scope collapse into a false
selection.

## Decision

Express candidate-set / scope-resolution cardinality as an **additive top-level
`resolution` object**, and keep the schema on `product-schemas.v1` with **no new
top-level status token**:

```json
"resolution": {
  "cardinality": "none" | "one" | "many" | "truncated",
  "candidates": [ { "family_id": "family:...", "summary": "..." } ]
}
```

- `one` → the existing `FOUND` outcome: a single proven in-scope family, hydrated
  normally, and the sole `resolution.candidate`.
- `many` → the existing `PARTIAL_CONTEXT` status with bounded candidate summaries
  and **no** `selected_family_id`. Several real families must never collapse into a
  generic `UNKNOWN` or a guessed selection.
- `none` → `PARTIAL_CONTEXT`: the locus resolved but no family matched; the
  candidate set is empty.
- `truncated` → `PARTIAL_CONTEXT`: the bounded scope read may hide further
  families, so the families seen so far are candidates and no single family is
  claimed.

Each candidate `summary` is a short, source-free line projected from the committed
family search-summary projection (language, framework role or code-unit kind,
classification) — never a hydrated deep family and never raw source. The
`cardinality` token is a low-cardinality enum safe to record in telemetry; the
candidate `family_id`s are already-public handles.

The `resolution` object renders at `standard`/`full` and is dropped at `minimal`.
Dropping it at `minimal` loses no narrowing handle: for a `many`/`truncated`
`PARTIAL_CONTEXT` the candidate `family_id`s are also carried on the resolved
target's `candidate_family_ids` and therefore on
`query_route.follow_up_family_ids`, both retained at `minimal`. Non-scope outcomes
never carry `resolution`, so their bytes are unchanged.

Domain and transport stay separated: the cardinality and candidate set are typed
domain values (`application::query_resolution::{Resolution, ResolutionCandidate,
ResolutionCardinality}`); no serde or MCP-SDK type leaks into the core. `check`
(conformance) and `show_family` are unaffected — `show_family` never enters the
fuzzy scope path, so it never hydrates more than the one exact family.

## Consequences

- A directory/composite scope with more than one in-scope family (or a truncated
  bounded read) now returns `PARTIAL_CONTEXT` instead of `UNKNOWN`. This is a
  user-visible contract change, recorded in the CHANGELOG; the additive
  `resolution` field changes the payload bytes only of the affected scope cases,
  and the payload-measure goldens are regenerated deliberately.
- Committed family precision is unchanged: a family is selected (`FOUND`,
  `resolution.cardinality: "one"`) only when exactly one high-confidence family
  resolves; `many`/`none`/`truncated` never carry a `selected_family_id`.
- Candidate recall is complete: every real in-scope family (up to the bounded cap)
  appears in `resolution.candidates`.
- Telemetry may record `resolution.cardinality` (low-cardinality enum) and the
  already-public candidate `family_id`s; it must never record raw targets, paths,
  symbols, or candidate summaries.

## Alternatives considered

- **Add a new top-level `CANDIDATE_SET` status token** (a `product-schemas.v2`
  break): rejected for now because it would break the additive pre-1.0
  compatibility promise and every existing `standard` consumer. It remains the
  reserved breaking path — a future `product-schemas.v2` may promote the candidate
  set to a first-class `CANDIDATE_SET` status; until then the additive `resolution`
  object on the existing statuses is the compatible transition.
- **Keep the Phase 2 `UNKNOWN` and only enrich its recovery text**: rejected
  because it leaves "resolved candidate set" indistinguishable from "unresolvable
  target" and forces per-candidate follow-up reads.
- **Hydrate every in-scope family for the candidate summaries**: rejected because
  it defeats the bounded-read contract; the source-free search-summary projection
  already carries enough to describe a candidate without a deep read.

## Follow-up work

- If usage shows consumers need a first-class candidate-set status, schedule a
  `product-schemas.v2` migration that adds a `CANDIDATE_SET` top-level status and
  folds the additive `resolution` object into it.

## Phase 5 note: scoped readiness (additive, source-free)

Phase 5 adds an optional `target`/`within` to the MCP `inspect_readiness`
operation and a matching `--target`/`--within` to `repogrammar doctor`. With
either, the response replaces the whole-checkout `readiness` object with a
bounded, source-free `scoped_readiness` object describing how queryable
RepoGrammar is over just that directory/module scope. This stays within the same
additive `product-schemas.v1` posture as the `resolution` decisions above:

- **No new top-level status token.** The scoped report reuses the existing
  low-cardinality readiness `summary` vocabulary (`ready`/`degraded`/`not_ready`),
  projected from the SAME shared repository recovery authority the whole-checkout
  readiness and the query preflight consume, so a scope is never more optimistic
  than the repository. It adds a scope-shape `queryability` verdict
  (`queryable`/`partial_context`/`degraded`/`not_indexed`/`not_ready`/
  `cannot_verify`) and scope counts, not a new global status.
- **One authoritative classifier.** The summary, freshness, and single recovery
  action are all derived from `repository_recovery_for_report`; the summary
  projection is a single shared helper reused by the whole-checkout assembler.
  Phase 5 does not fork a second readiness classifier.
- **Source-free and telemetry-free.** Scoped readiness reuses the bounded
  directory-scope read/family-mapping ports (`list_active_files_in_directory` +
  `find_active_families_by_evidence_path`) exactly as the directory-scope query
  resolver does, but only COUNTS: it hydrates no family, reads no source content
  (its assembler takes no `SourceStore`), and records no family-query telemetry.
- **Cardinality/count discipline.** Every scoped field is a low-cardinality enum,
  count, or language token; a truncated bounded read reports `coverage: truncated`
  with the counts as lower bounds. No raw target, path, or symbol is emitted, and
  telemetry (which records nothing on this path) could only ever see the same
  low-cardinality tokens.
- **No-target output unchanged.** The whole-checkout `readiness` object and the
  scoped `scoped_readiness` object are mutually exclusive and carried under
  distinct keys; the no-target `inspect_readiness`/`doctor` output is
  byte-identical to before Phase 5.
