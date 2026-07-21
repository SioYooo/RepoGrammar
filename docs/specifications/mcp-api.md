# MCP API Specification

The MCP interface is implemented as a minimal pre-alpha read-only stdio server.
This file records the concrete bootstrap tool boundary and the current
Codex/Claude Code installer wiring without claiming a final stable API.

## Default tool surface

The default v0.1 MCP surface should expose one primary tool:

```text
repogrammar_context
```

The tool carries an `operation` field. Supported v0.1 operations are:

- `find_analogues`
- `show_family`
- `explain_deviation`
- `check_conformance`
- `inspect_readiness`

This keeps agent tool selection stable while preserving explicit internal
operation semantics. The CLI remains multi-command for human discoverability.

The current input schema is intentionally small:

- required `operation`: one of the five operation strings above.
  `inspect_readiness` accepts an optional `target` and/or `within` to request a
  bounded, source-free SCOPED readiness report (see below); with neither it
  returns the whole-checkout readiness report and ignores the evidence-shaping
  fields.
- optional `target`: non-empty string, at most 8192 bytes, with no control
  characters.
- optional `against`: non-empty string, same length/character bounds as `target`.
  It names the COMPARISON family for the two-sided operations `explain_deviation`
  and `check_conformance`, and is rejected (never silently ignored) on any other
  operation. It pins the comparison side to exactly one family: an exact `family:`
  id resolves that family directly; a framework role or pattern resolves the
  unique fresh ready family of that role, otherwise the request abstains with
  bounded candidate family handles and no selection. When omitted, the comparison
  family is inferred from the target unit's own membership, else the single fresh
  ready family of its `(language, kind, role)` key. `against` is additive to the
  closed input schema and never false-selects a comparison family.
- optional `within`: non-empty string, at most 8192 bytes, with no control
  characters. A directory/module scope. Consumed only by `inspect_readiness` to
  request a scoped queryability report; the family-query operations ignore it
  (they already scope through `target`).
- optional `token_budget`: positive integer up to 200000 used to cap selected
  evidence metadata. Supplying it implies `mode: evidence` unless an explicit
  mode is provided.
- optional `mode`: one of `compact`, `evidence`, or `deep`.
- optional `verbosity`: one of `minimal`, `standard`, or `full`; default
  `standard`. `verbosity` selects response field density and is orthogonal to
  `mode`, which selects evidence detail. It is additive under
  `product-schemas.v1`: `standard` is the current, byte-stable response shape,
  `minimal` opts into the lean shape, and `full` retains every diagnostic field.
  `standard` and `full` are byte-identical to each other and to this
  development line's pre-precision response — which already carries the
  inline-member cap, so this is byte-stability against the pre-precision shape,
  not identity with v0.2.2; each precision slice suppresses its demoted fields
  only at `minimal`, and every removal is a demotion `full` restores. The
  current `minimal` reductions are the `query_route` slimming, the
  `resolved_target`/certificate dedup, and the `read_plan`/`source_spans`
  reductions (honest truncation flags, read-plan/span dedup, and the dropped
  empty `source_spans` stub). An unrecognized value is an invalid-operation
  schema error, never a silent fallback. The per-operation reductions are
  documented with their output contracts below.
- optional `include_variations` and `include_exceptions`: booleans requesting
  those evidence-coverage labels. Current output reports them as missing unless
  stored family evidence explicitly covers them.
- optional `include_source_spans`: boolean, default `false`. When true,
  RepoGrammar may render bounded line-numbered source spans selected from the
  `read_plan` after content-hash validation. This is an explicit source-output
  opt-in and is not implied by `mode: deep`.

Advanced MCP tools may exist later, but they must be hidden by default and
enabled only by configuration or environment variable, for example:

```text
REPOGRAMMAR_MCP_TOOLS=context,find,family,check
```

## Operation intent

### find_analogues

Find source-backed implementations closest to a target and return conservative
classification evidence.
The target should be the repo-relative path, symbol/member id, framework role,
or pattern question the caller already has. The caller does not need to know a
family id before this operation; fuzzy lookup performs bounded candidate
discovery internally and returns family ids only as follow-up handles.

CLI equivalent: `repogrammar find`.

### show_family

Show the canonical template, variation points, exceptions, representative
implementations, and provenance for a known family.
The target is an exact family id. `show_family` must not resolve arbitrary
path, framework-role, classification, or substring targets.

CLI equivalent: `repogrammar family`.

### explain_deviation

Explain whether a target differs from a family as a legal variation, exception,
incompatibility, near miss, static deviation, or `UNKNOWN`. `explain_deviation` is
**not** a `find_analogues` alias: it performs the same two-sided static-alignment
resolution as `check_conformance` and projects a real deviation certificate. It
resolves the target to exactly one indexed code unit, pins exactly one comparison
family (the caller-named `against` family when provided, else inferred from
membership or the unit's `(language, kind, role)` key), and projects the target's
indexed feature profile against that family's constraint profile via the shared
`compute_alignment` authority.

Whenever a unit and a comparison family both resolve, the response carries a real
`target_relationship` — one of `MEMBER`, `LEGAL_VARIATION`, `NEAR_MISS`,
`EXCEPTION`, `BLOCKED_UNKNOWN`, `OUT_OF_SCOPE`, or `INCOMPATIBILITY` (the
`COMPETING_PATTERN` token remains reserved and is emitted by no path) — never
`null`. When the target cannot be pinned to one unit + one family (ambiguous,
stale, unindexed, out of scope, family-less, or an ambiguous/unmatched `against`),
it abstains with a typed `UNKNOWN`/`INSUFFICIENT_EVIDENCE` status, a `null`
`selected_family_id`, and bounded candidate handles. Like `check_conformance`, it
claims no runtime behavior: every response carries `runtime_equivalence:
"UNKNOWN"`. The target should pin one unit — a path, path:line locator,
path:byte-range, or `unit:` member id; a bare path that maps to several
family-eligible units abstains rather than guessing. `include_variations` /
`include_exceptions` request the legal-variation and exception coverage labels.

CLI equivalent: `repogrammar explain` (with optional `--against`).

### check_conformance

Return a source-backed *static-alignment* certificate for a target, or abstain
with a typed reason. This is not a runtime-conformance verdict: matched family
context is never proof of runtime equivalence. `check_conformance` resolves the
target to a specific indexed code unit, pins a comparison family, and statically
compares the target's indexed feature profile against that family's constraint
profile. The comparison family is the caller-named `against` family when provided,
otherwise the unit's own family when it is a member, otherwise the single fresh
ready family of its `(language, kind, role)` key. `against` lets the caller fix the
comparison side (an exact `family:` id, framework role, or pattern that resolves to
exactly one fresh ready family; an ambiguous or unmatched `against` abstains with
candidate handles and never false-selects). The top-level `status`
is a static-alignment token (`STATICALLY_ALIGNED`, `STATIC_DEVIATION`,
`PARTIAL_ALIGNMENT`, `INSUFFICIENT_EVIDENCE`, or `UNKNOWN`) — never
`PASS`/`FAIL`/`CONFORMS` and never the legacy `CONTEXT_ONLY` advisory — and every
certificate carries `runtime_equivalence: "UNKNOWN"`. A stale, ambiguous,
unindexed, or family-less target (or an unresolvable `against`) abstains with
`INSUFFICIENT_EVIDENCE`/`UNKNOWN` and never surfaces a selected family. The initial
target may be a path, path:line locator, or `unit:` member id.

CLI equivalent: `repogrammar check` (with optional `--against`).

### inspect_readiness

Return a bounded, read-only, source-free report of RepoGrammar's product
capability state in the current checkout. It runs neither the query preflight nor
family lookup and takes no `target`. The response `readiness` object is the shared
decomposed readiness model (see `docs/specifications/product.md`): a
low-cardinality `summary` token (`ready`, `degraded`, or `not_ready`), and
independently truthful dimensions for `repository_state`, `active_index`,
`family_evidence` (fresh/stale/cannot_verify counts), `family_prevalence` (counts
by classification, or `null` when the family store is unreadable),
`query_retrieval` (exact and term-retrieval modes plus the vocabulary version),
`static_alignment` (`available`/`unavailable`/`not_applicable`, or `cannot_verify`
when the family store is unreadable), `providers` (per-slot integration and
availability), `autosync`, and `measurement` (the NOT_MEASURED token-saving
discipline). It also carries `top_blocking_unknowns` (the bounded top-five
required-mechanism buckets from the persisted unknown inventory, or `null` when
that inventory is unreadable — distinct from `[]` for genuinely none) and one
`recovery` object whose `action` comes from the shared recovery classifier, with
`recommended_command` present only when the action is an executable RepoGrammar
command (`executable: true`) and null otherwise. The summary is a pure projection
of that one recovery decision — the same authority the query preflight consumes —
so it is never more optimistic than the query path: an unservable index (not
initialized, unhealthy storage, blocking lock, or missing active generation) is
`not_ready`; a servable index that is stale (family evidence stale/unverifiable
OR the repository index carries dirty derived records) or has a
recommended-but-stopped autosync is `degraded` with the stale count visible; only
a fully clean, fresh servable index is `ready`. Consequently `summary: ready`
implies the query preflight is `Ready` on the same checkout, and one payload can
never pair `readiness.query_ready: false` with `product_readiness.summary: ready`.
The output is low-cardinality typed tokens and counts only — never source text,
evidence, paths, content hashes, or family detail — and `inspect_readiness`
records no family-query telemetry (it is a status-like inspection, like
`status`/`doctor`). It performs the same bounded stats-scale reads as those
commands, so like them it is for readiness triage, not routine per-query loops.
When readiness cannot be assembled at all, the response is the standard
`FALLBACK_TO_CODE_SEARCH` object.

#### Scoped readiness (optional `target`/`within`)

When `inspect_readiness` is called with a `target` and/or `within`, the response
replaces the whole-checkout `readiness` object with a bounded, source-free
`scoped_readiness` object describing how queryable RepoGrammar is over just that
directory/module scope. The no-target response is byte-identical to before; the
two are mutually exclusive and carried under distinct keys (`readiness` vs
`scoped_readiness`) so consumers can tell them apart. The scoped report carries:

- `summary`: the same `ready`/`degraded`/`not_ready` token as the whole-checkout
  readiness, projected from the same shared recovery authority, so a scope is
  never more optimistic than the repository.
- `queryability`: the scope resolvability verdict —
  `queryable` (indexed scope with at least one resolvable family on a ready
  repository), `partial_context` (indexed scope, no resolvable family),
  `degraded` (indexed scope on a degraded repository), `not_indexed` (the scope
  resolved to no indexed files; with `scope.prefix_count == 0` the target named
  no safe directory scope), `not_ready` (the repository cannot serve queries), or
  `cannot_verify` (a store read failed or the generation changed mid-read).
- `scope`: `prefix_count` (how many safe directory prefixes were read — a count
  only, never the paths), `indexed_file_count`, `coverage`
  (`empty`/`present`/`truncated`), `truncated` (a bounded read hit its cap, so the
  counts are lower bounds), `languages` (low-cardinality language tokens present
  in scope), `resolvable_family_count` (distinct families whose evidence occupies
  the scope, counted WITHOUT hydrating any family), and `freshness`
  (`fresh`/`stale`/`cannot_verify`/`not_applicable`, projected source-free from the
  shared repository recovery — not a per-file re-hash).
- `providers`: the same per-slot integration/availability list as the
  whole-checkout readiness.
- `recovery`: the single recovery object from the shared recovery classifier.

Scoped readiness is SOURCE-FREE: it hydrates no family and reads no source
content (no `SourceStore` access). It records NO family-query telemetry, exactly
like the no-target inspection. Every field is a low-cardinality enum, count, or
language token; no raw target, path, or symbol appears in the output. The scope
must be path-like: a bare single-segment token (e.g. `pkg`) that carries no `/`
or `.` is rejected by the shared path-safety authority and reads to an empty
scope (`not_indexed` with `prefix_count 0`), exactly as the directory-scope query
resolver treats it. When scoped readiness cannot be assembled, the response is the
standard `FALLBACK_TO_CODE_SEARCH` object.

CLI equivalent: the source-free readiness fields of `repogrammar status --json`
and `repogrammar doctor --json` (`product_readiness`), and the scoped report via
`repogrammar doctor --target <scope>` / `--within <scope>`.

## Missing and stale indexes

MCP serving must not implicitly initialize a repository. If no project state
directory is found, the response must be a clean fallback recommendation rather
than a panic or noisy transport failure:

```text
FALLBACK_TO_CODE_SEARCH
reason: repository is not initialized
guidance: run repogrammar setup
```
For agent-safe bootstrap, MCP guidance recommends `repogrammar setup` only after
the user has allowed machine integration and repo-local RepoGrammar state. That
orchestrator builds the first active index and starts auto-sync by default;
after a successful setup or resync, stale auto-sync state may still recommend
`repogrammar autosync start`. The MCP server itself remains read-only and must
not run `setup`, `init`, `resync`,
`autosync start`, or any other repository writer. If repository state exists but
no readable active generation exists, guidance should tell the agent to run
`repogrammar resync` rather than claiming analysis has run.
Agents can inspect `repogrammar doctor --json` readiness diagnostics to
distinguish `not_initialized`, `state_only_no_active_index`, storage or active
index health problems, and a ready active index that still has no supported
family evidence. MCP may surface fallback guidance and recommended commands,
but it must not initialize, resync, start autosync, or repair hygiene itself.

If an index is stale, MCP responses must include a stale warning or refuse
family claims whose evidence changed. Freshness checks must compare the active
index generation and repository state described in
`docs/specifications/storage.md`.
The same freshness gate applies to public MCP family detail and conformance
responses: stale family evidence must become typed `StaleEvidence` `UNKNOWN`
rather than a top-level successful claim.

Typed analysis uncertainty must not be flattened into transport failure. MCP
responses preserve `UNKNOWN` class, reason code, affected claim, and suggested
recovery action where available.
The current Rust storage/query boundary has an internal active-generation
claim-input snapshot, semantic-fact freshness/readiness gate, and conservative
EC-MVFI-lite family read model. MCP responses must not expose semantic-worker
facts, raw snapshot contents, or treat framework heuristics as family evidence.
The MCP call handler reuses the same application query preflight and
FamilyStore-backed lookup path as the CLI rather than inventing a parallel
contract.
Exact MCP lookups are bounded: exact family-id targets hydrate only that family,
exact member/code-unit targets first use the member index and hydrate one family
only when the member id is unique, and ambiguity remains typed `UNKNOWN`.
Fuzzy lookup must not hydrate every family. For `find_analogues` (and the
comparison-family `against` side of `explain_deviation` / `check_conformance`,
which reuses the same family resolution), the public query mode is
`discover -> hydrate -> compose`: RepoGrammar accepts the path, symbol/member id,
framework role, or pattern question the caller has, discovers candidate family
ids internally from exact ids, exact member ids, repo-relative evidence paths,
and exact member roles, then hydrates only the capped candidate set. The
`explain_deviation` / `check_conformance` SUBJECT side, in contrast, resolves to
exactly one indexed code unit by locator and never fuzzy-discovers a subject
family. If role/path
discovery is truncated or exceeds the cap, the family probe blocks with typed
`UNKNOWN` (reason `InsufficientSupport`, affected claim `query target candidate
set`) and candidate ids instead of scanning all family detail. `show_family`
keeps the opposite contract: it is exact-family-id only and is intended for
family ids returned by an earlier query.
For `find_analogues`, `explain_deviation`, and `check_conformance`, fuzzy path
or path-suffix targets that match evidence in multiple families must abstain
from a family claim instead of returning whichever family appears first in
storage order. That block uses reason `InsufficientSupport`, affected claim
`query target ambiguity`, and recovery guidance that names the candidate family
ids and asks the caller to narrow the query to an exact family id or member id.
Each of these fuzzy family blocks — no candidate, a too-broad or truncated
candidate set, or several competing families — still routes through the
deterministic local-context resolver below, so a target that resolves to exactly
one indexed path or code unit earns `PARTIAL_CONTEXT` while the family stays
unguessed; a target that stays ambiguous or unresolvable keeps the typed
`UNKNOWN`. This local-context fallback is fuzzy-only: exact family and exact
member lookups never resolve local context and keep their strict `UNKNOWN`.
The deterministic target resolver recognizes exact repo-relative indexed paths,
exact member/code-unit ids, embedded indexed paths inside longer text, unique
indexed path suffixes, `path:line`, and `path:start-end` byte-range forms.
When those same fuzzy operations can deterministically resolve the target to
exactly one indexed repo-relative path or code unit in the active generation but
no family evidence supports a claim for that target, MCP returns top-level
`PARTIAL_CONTEXT`. This response contains `query_route`, `resolved_target`,
metadata-only `output`, a target `read_plan`, an
`estimated_potential_token_savings` block, optional `source_spans` only when
`include_source_spans: true` succeeds, and a typed `InsufficientSupport`
unknown for `pattern family evidence for resolved target`. The
`estimated_potential_token_savings` block (at CLI parity) carries
`outcome_shape: partial_context`, the resolved file's `language` scope, the
estimated baseline/returned/potential token counts, `ESTIMATED` kind, and the
not-measured caveat; the baseline is the estimated whole-file read the read plan
displaces (from the indexed inventory's stored size), and an unavailable size
yields null counts with an `unavailable_reason` rather than a guess.
`resolved_target`
preserves the raw target, resolved kind, repo-relative path, optional line,
optional byte range, optional family/member ids, symbol hints, residue terms,
candidate paths/ids, confidence, and match kind. It is local read-planning
context, not family evidence or conformance evidence. At `verbosity: minimal`
the shared `resolved_target` serializer suppresses the pure input echo
(`original_target`), the normalizer internals (`residue_terms`), and each
`candidate_paths`/`candidate_family_ids`/`candidate_code_unit_ids` list that only
echoes an already-resolved locus (the `check_conformance` target echo); when
resolution stayed genuinely ambiguous — no single path, family, or unit pinned —
the corresponding candidate list is retained as the caller's narrowing handle.
`standard` and `full` keep the complete field set byte-for-byte.

#### The additive `resolution` object (candidate-set cardinality)

`find_analogues` (the `FuzzyQuery` operation) can resolve
a **directory / composite scope** to a set of pattern families. RepoGrammar
reports the cardinality of that set through an additive top-level `resolution`
object rather than a new top-level status token — the response stays on
`product-schemas.v1` (see ADR-0029 for the compatibility rationale, and
`docs/specifications/query-resolution.md` for the resolver). Shape:

```json
"resolution": {
  "cardinality": "none" | "one" | "many" | "truncated",
  "candidates": [ { "family_id": "family:...", "summary": "python fastapi.route · DOMINANT_PATTERN" } ]
}
```

- `one` → the existing `FOUND` (`status: "ok"`) outcome: a single proven in-scope
  family, hydrated normally; it is the sole `resolution.candidate`.
- `many` → `PARTIAL_CONTEXT`: several distinct in-scope families with **no**
  selection. `resolution.candidates` lists the bounded, source-free candidate
  summaries; `resolution` **never** carries a `selected_family_id`.
- `none` → `PARTIAL_CONTEXT`: the directory locus resolved to indexed files but no
  matching family; `resolution.candidates` is empty.
- `truncated` → `PARTIAL_CONTEXT`: the bounded scope read may hide further
  families, so the families seen so far are candidates and no single family is
  claimed.

Each candidate `summary` is a short line projected from the committed family
search-summary projection (language, framework role or code-unit kind,
classification) — never a hydrated deep family and never raw source. The
`cardinality` token is a low-cardinality enum safe to record in telemetry; the
candidate `family_id`s are already-public handles. `resolution.candidates` is
bounded by the same fuzzy candidate cap.

The `resolution` object is an additive **`standard`/`full`** field: non-scope
outcomes never carry it, so their bytes are unchanged, and it is dropped at
`verbosity: minimal`. Dropping it at `minimal` loses no narrowing handle — for a
`many`/`truncated` PARTIAL_CONTEXT the candidate `family_id`s are also on the
resolved target's `candidate_family_ids` and therefore on
`query_route.follow_up_family_ids`, both of which are retained at `minimal`.

Exact `show_family`
lookups still require an exact family id and return typed `UNKNOWN` when that
family id is missing. `show_family` never enters the fuzzy scope path, so it never
carries a candidate-set `resolution` and never hydrates more than the one exact
family. `check_conformance` AND `explain_deviation` responses (both drive the
shared two-sided static-alignment path and differ only in the `command`/`operation`
label and emphasis) carry the static-alignment
certificate fields: `alignment_status`, `runtime_equivalence` (always
`"UNKNOWN"`), `target_relationship`, `selected_family_id`, `target` (the
resolved code-unit locator), and `alignment` (the computation — `outcome_reason`,
`required_features_matched[]`, `static_deviations[]` with source-free token
summaries, `legal_observed_variations[]`, `blocking_unknowns[]`, and
`unresolved_runtime_obligations[]`, or `null` when abstaining). A committed or
partial certificate also carries the `estimated_potential_token_savings` block
with `outcome_shape: alignment`; an abstaining certificate reports the null
block. They must omit
the legacy nested `check` advisory object and any proof-like fields such as
`pass`, `conforms`, or `fail_on`. The top-level `status` is the alignment status
token and `alignment_status` duplicates it byte-for-byte; the `alignment_status`
duplicate is dropped at `verbosity: minimal` while `standard` and `full` keep it.
The top-level `selected_family_id` is the authoritative carrier of the
selected-family handle and is retained at every tier, including `minimal`: the
`query_route.selected_family_id` copy is the one the route lane suppresses at
`minimal`, so dropping the certificate top-level copy too would leave "which
family was selected" undeterminable (`follow_up_family_ids` is an unordered set).
The invariant `runtime_equivalence: "UNKNOWN"` is emitted at every tier and never
suppressed. As a scale guard, `alignment.static_deviations[]` and
`alignment.legal_observed_variations[]` are capped at a fixed bound in every
tier; when a target exceeds the cap the array is truncated to the bound and the
computation gains an honest `<name>_truncated: true` flag and a `<name>_count`
total. Below the cap the full arrays are emitted with no truncation metadata. `static_deviations[].kind` is one of the
required-feature violations (`required_mismatch`, `must_be_empty_violation`,
`missing_required_core`, `prohibited_presence`) or a non-violating partial signal
(`unobserved_variation`, `truncated_observation`, `blocking_suppressed_requirement`)
that forces `PARTIAL_ALIGNMENT`, never `STATIC_DEVIATION`. `selected_family_id` is
`null` for every abstaining outcome, and `COMPETING_PATTERN` is a reserved
`target_relationship` token no current path emits. The non-abstaining
`target_relationship` values are `MEMBER`, `LEGAL_VARIATION`, `NEAR_MISS`,
`EXCEPTION`, `BLOCKED_UNKNOWN`, `OUT_OF_SCOPE`, and `INCOMPATIBILITY`.
`check_conformance` and `explain_deviation` both resolve
the target to exactly one code unit honoring a `path:line`, `path:byte-range`, or
`unit:` locator; a path-only target that names a file with more than one
family-eligible unit abstains with `INSUFFICIENT_EVIDENCE` and candidate unit ids.
The optional `against` input pins the comparison family for both operations (an
ambiguous or unmatched `against` abstains with `INSUFFICIENT_EVIDENCE`, a `null`
`selected_family_id`, and candidate handles — never a false selection).
All family lookup responses include `query_route`, a source-free route metadata
object with `route`, `input_kind`, `pipeline`, `family_id_policy`,
`candidate_limit`, `selected_family_id`, `candidate_family_ids`,
`follow_up_family_ids`, and `why_selected`. For fuzzy operations, its policy must
state that family ids are returned follow-up handles rather than required initial
inputs. `selected_family_id` is present only when RepoGrammar actually selected
one supported family. `candidate_family_ids` and `follow_up_family_ids` may be
present on `PARTIAL_CONTEXT` or `UNKNOWN`; those ids are narrowing handles, not a
family or conformance claim. At `verbosity: minimal` the `query_route` object
collapses to its two decision-critical fields — `route` and
`follow_up_family_ids` (the single canonical handle list, a superset of both
`candidate_family_ids` and `selected_family_id`, so no id is lost). On a matched
family the duplicate `candidate_family_ids` is dropped as well; on
`PARTIAL_CONTEXT`, `UNKNOWN`, and conformance abstentions it is retained at
`minimal` as a narrowing recovery handle. `selected_family_id`, `input_kind`,
`pipeline`, `family_id_policy`, `candidate_limit`, `why_selected`,
`hydrated_family_count`, `retrieval_stage_count`, and `term_retrieval` are
diagnostic and appear only at `standard` and `full`. When a natural-language,
synonym, or
framework-plus-concept target is resolved by the deterministic term-retrieval
fallback, `query_route` additionally carries `hydrated_family_count`,
`retrieval_stage_count`, and a source-free `term_retrieval` object with `route`
(`term_retrieval_hydrate` | `term_retrieval_unknown`), `retrieved_summary_count`,
`ranked_candidate_count`, `hydrated_candidate_count`, `retrieval_stage_count`,
raw `top_score`/`margin`, `top_score_bucket`/`margin_bucket`, `truncated`,
`matched_signals`, and `abstention_reason`. The `abstention_reason` vocabulary is
`no_candidate`, `below_min_score`, `unsupported_target`, `margin_too_close`,
`truncated_tie`, `stale_candidates`, and `hydration_ambiguous` (null when a family
was found). These fields carry no raw target text and are null for exact/role/path
routes.
Matched family responses use the same output selection contract as the CLI:
`compact` is the default and returns family summary, members, variation slots, a
metadata-only `constraint_profile` (the family's hydrated source-backed
specification — `required_equal_features`, `allowed_variations`,
`prohibited_or_blocking_features`, and `unresolved_obligations`, each a typed
token or count, or `null` when none was persisted; see
`docs/specifications/domain-model.md`),
unknowns, output metadata, and a `read_plan` without evidence records;
`evidence` adds budgeted repo-relative evidence metadata selected by
deterministic greedy marginal coverage per estimated token cost; `deep` is
accepted only as an explicit detail request and remains metadata-first unless
`include_source_spans: true` is also provided. The inline `members` array is
bounded exactly as on the CLI: outside `deep` mode it is capped at the first 20
members in unchanged deterministic order, and every response carries the true
`member_count` and a `members_truncated` flag so a large family never inflates a
single MCP response; `deep` restores the full member list. The `read_plan` is returned for
the existing `find_analogues`, `show_family`, `explain_deviation`, and
`check_conformance` operations. It contains suggested target, canonical,
support, and variation/exception spans by repo-relative path, strict content
hash, byte range, estimated token cost, and purpose. When source spans are not
requested, RepoGrammar still attempts metadata-only line-range enrichment after
source-store path and content-hash validation. Fresh hashes should return
`start_line` and `end_line` while `source_snippets_included` remains `false`.
Stale, missing, hash-mismatched, too-large, non-UTF-8, unavailable, or invalid
ranges must preserve the read-plan item and add `line_range_omissions`
guidance. When `include_source_spans: true` is requested, RepoGrammar renders
only selected read-plan spans after source-store path and content-hash
validation, fills line ranges for rendered spans, and returns line-numbered
text under a separate `source_spans` object. Read-plan items are ordered by
purpose priority (target body, then canonical, support, and guard spans) so a
budget-truncated plan always keeps the most decision-critical prefix. At
`verbosity: minimal` the read plan adds an honest `truncated` flag and
`item_count`; a span that has been rendered into `source_spans` is not repeated
as a full read-plan item but left as a `{purpose, path, rendered: true}`
back-reference, because the rendered `source_spans` entry is the single source
of truth and must be treated as already read; and the empty `source_spans` stub
is omitted entirely when spans are not requested. `standard` and `full` keep the
full read-plan items and the stub unchanged. The read plan never includes
absolute paths and does not imply source edits are safe outside listed ranges.
Output metadata includes the selection strategy, estimated evidence tokens,
estimated read-plan tokens, `estimated_potential_token_savings` with
`measurement_kind: ESTIMATED`, covered claim labels, missing claim labels, and
whether the rough budget was satisfied. `estimated_potential_token_savings` is
a potential read-displacement estimate for the returned RepoGrammar metadata
shape; it is not measured token savings and must carry a caveat saying so. The
same estimate is computed by a single query authority for every
context-delivering outcome shape (found, partial_context, alignment) and each
recorded event is attributed a low-cardinality `outcome_shape` and `language`
token; see `docs/specifications/metrics.md` for the per-shape baseline/returned
accounting.
Stored family evidence carries
schema-backed `covered_claims` labels from the allowlist `canonical`,
`support`, `contrast`, `variation`, and `exception`; selectors must consume those
labels rather than infer coverage from notes or record order. The family builder
assigns labels by coverage, not storage order: the canonical medoid carries
`canonical`, every member carries `support`, the farthest-from-medoid support
witness additionally carries `contrast`, and one representative per observed
variation profile carries `variation`, with the medoid excluded from `contrast`
and variation witnesses (see the Representative selection rule in
`docs/specifications/domain-model.md`). Hydration re-sorts evidence by path, so
the `contrast` label — not the write order — is what lets the read plan recover
the witness. When the hydrated `constraint_profile` enumerates variation
dimensions, evidence selection covers one witness per dimension plus the
anchor-target dimension when its slot exists; otherwise a single variation witness
is requested from the variation-slot signal. Read-plan purposes follow the same
labels (`canonical_evidence` names the medoid, `support_evidence` prefers the
`contrast`-labelled witness and falls back to the first distinct-path `support`
member, `variation_guard` a variation witness). Requested exception coverage may
be missing; a variation dimension is missing only under a real budget shortfall,
since the canonical satisfies any dimension it solely represents. `exception`
evidence remains unlinked in this slice. Family detail unknowns
identify runtime-equivalence gaps with the concrete
`<family_id>:runtime_equivalence` affected claim. MCP responses must report
whether source snippets were included. Stale, missing, hash-mismatched,
unsupported, dynamic, insufficient, or conflicting evidence must omit rendered
spans and preserve typed `UNKNOWN` or omission guidance telling the agent to
use normal Read/Grep for the affected case.

## Serving mode

`repogrammar serve` runs a newline-delimited JSON-RPC stdio loop for
`initialize`, `notifications/initialized`, `tools/list`, `tools/call`, and
`shutdown`. Each request line is read under a 1 MiB bound: the reader stops one
byte past the limit and rejects the request rather than buffering an
unterminated multi-gigabyte line into memory. v0.1 serving behavior defaults to read-only for source,
family/index content, and business-code state and must not modify business code
from pattern-family results. MCP serving uses a read-only analysis runtime
facade that can only request repository status, pattern-family lookup, and the
decomposed product readiness report. Indexing remains the only writer for
repository analysis records. The only
allowed MCP side effects are the local aggregate metrics described below and
idempotent SQLite schema/index migration for an already initialized mutable
database; none of these is source, family content, business-code state, or
agent configuration state.

`tools/list` returns exactly one default tool, `repogrammar_context`.
`tools/call` wraps the RepoGrammar JSON payload in a standard MCP text content
item:

```json
{
  "content": [
    {
      "type": "text",
      "text": "{\"status\":\"UNKNOWN\"}"
    }
  ],
  "isError": false
}
```

Every wrapped RepoGrammar result object carries a `schema_version` field, shared
with the CLI structured payloads (`product-schemas.v1`; see
`docs/specifications/cli.md`). The pre-1.0 compatibility policy is additive:
fields may be added within a version; removing or renaming a field, or changing
its meaning, requires a version bump and a CHANGELOG entry. Consumers must ignore
unknown fields.

### Recovery from consumer-side truncation

RepoGrammar cannot prevent a consuming client from truncating a tool response
when it compacts its own context window; that is outside the server's control.
It does, however, guarantee a deterministic recovery path. The server is
read-only and stateless across calls: it holds no per-caller cursor, session,
or continuation token. For a fixed active generation, a given
`repogrammar_context` call is deterministic, so a caller whose context
compaction dropped or truncated a response recovers the full result by
re-issuing the identical call — the same arguments return the same bytes. The
only thing that changes a result is the underlying index: if a `resync` or a
background autosync `sync` activated a new generation between calls, family ids
and evidence may differ, so a caller that persisted handles across an index
change must re-resolve them rather than assume byte-stability across generations.

Three response-shaping rules make that recovery cheap and reduce how much a
single response can be truncated in the first place. First,
`follow_up_family_ids` is the canonical, precise handle for the selected and
candidate families and is retained at every verbosity tier, including `minimal`;
a caller that kept only the follow-up handles can re-issue an exact
`show_family` or `check_conformance` against them. Second, the inline `members`
array is bounded to the first 20 members outside `deep` mode, with a true
`member_count` and a `members_truncated` flag, so a large family cannot inflate
one response into the range where truncation is likely. Third,
`verbosity: minimal` demotes diagnostic fields to shrink the payload further
while preserving the decision-critical route and handle fields. None of these
prevent client-side compression; they narrow the exposed surface and keep the
re-request path exact.

Missing state, missing active indexes, and typed analysis uncertainty are normal
tool results. Unknown JSON-RPC methods, unknown tool names, invalid operations,
blank, oversized, or control-character-containing targets, oversized token
budgets, and malformed argument types are transport/schema errors.

MCP calls must not wait on telemetry network activity and must not trigger
telemetry upload. Anonymous telemetry upload is only attempted by explicit
`repogrammar telemetry upload` after consent and endpoint validation.
Each MCP `repogrammar_context` invocation may best-effort update the repo-local
`.repogrammar/telemetry/local-metrics/family_query_metrics.json` atomic cohort,
including preflight fallback, successful family context, deterministic
`PARTIAL_CONTEXT`, typed query-time `UNKNOWN`, and runtime fallback outcomes.
Schema `family-query-metrics.v2` increments the query denominator exactly once
and records any estimated-savings event in the same file replacement. It carries
the explicit `atomic-query-accounting.v2` epoch, epoch start, and producer
version. Legacy v1 savings and query-outcome files are unpaired historical
evidence and are excluded from v2 stats. The v2 file stores only aggregate token
totals and low-cardinality buckets for operation category, lookup mode, status,
UNKNOWN class/reason/required-mechanism/recovery code, read-plan item counts,
source-span request/inclusion/omission counts, outcome shape, and language.
It must not store raw arguments, targets, paths, repository names, content
hashes, prompts, source, evidence text, symbols, family ids, member ids,
code-unit ids, raw tool input/output, raw errors, diffs, or patches.
Claude Code and Codex integrations both point at this same read-only MCP server;
agent installation and uninstallation are machine-level configuration workflows
and must not change MCP tool semantics, initialize a repository, index code,
touch `.repogrammar/`, upload telemetry, or enable telemetry by themselves.
Interactive multi-agent install and `--target all --scope global --yes` are
installer transactions around this same read-only MCP server, not additional
MCP tools.

## Agent guidance

The MCP initialize response is the canonical runtime guidance for agents.
Installer-written instruction-file content is optional, short, marker-fenced,
and owned by the installation workflow. Both surfaces use the same authoritative
pre-flight text.

After reading mandatory repository authority and instruction documents, the
pre-flight is a gate in an initialized repository when implementation, fix,
refactor, test, or diagnosis requires a repository-local contract or
convention, repeated implementation, framework role, or analogue comparison.
This explicitly includes root-cause repair and schema, protocol, API,
prompt-output, or Meaning Contract qualification, conformance, or drift. A
mixed contract task remains covered even when its immediate target is an exact
file, YAML, configuration, or generated prompt.

Before the first call, the agent builds one precision-first target in this
strict order: exact repo-relative path or locator, exact `unit:`/member/symbol,
exact framework role, then a concise pattern question only when no stronger
anchor is available. It must not replace a concrete locus with broad task or
governance prose. A prose target preserves any task-provided language/framework
and a supported concept (`route`, `fixture`, `validation model`, `data access`,
or `test`). The agent then calls `find_analogues` in compact mode once for that
target before CodeGraph or source search/read.

Agents use the returned `read_plan` before editing and treat included
line-numbered `source_spans` as already read. They state the reason before
falling back on unavailable, `FALLBACK`, stale, omitted, or insufficient
evidence. On `UNKNOWN`, exactly one returned candidate family id may be inspected
once with exact `show_family` as candidate context only, never as selected-family
or conformance proof; multiple candidates require a stronger target or fallback.
A given target is called only once; a materially narrower locator is a new
target. Agents leave `include_source_spans` unset unless bounded source is
explicitly needed.

Pure prose documentation, operational release/Git/environment/credential
inspection, syntax-only YAML/configuration validation, and exact one-symbol,
file, or call-path lookup skip the gate only when they require no repository
contract, convention, repeated implementation, framework-role, analogue,
code-behavior, or implementation decision. The file type never overrides a
covered conformance subtask.
Agents must not run `repogrammar stats` in normal coding-agent loops; stats is a
diagnostic inventory command, not a context lookup.
When stats is used for readiness triage, agents must read the source-free
official scope and indexed inventory fields separately: top-level stats remains
`python_v0_1` / `python_family_eligible_units`, while TS/JS indexed context
with no supported families should route the agent to exact-path
`repogrammar_context`/`find`/`check` calls that may return `PARTIAL_CONTEXT`.
React/RN remains unsupported and must not be inferred from stats.
For readiness triage, agents may call `inspect_readiness` (or run
`repogrammar doctor --json`) and read only the source-free readiness object; they
must treat `.codegraph/` entries as foreign unmanaged provider state and must not
ask RepoGrammar to create, modify, or delete them.
When RepoGrammar returns missing-state fallback and the user has allowed both
repo-local analysis state and the default repo-local background daemon, agents
may run `repogrammar init --yes`; it builds the active index and starts
auto-sync. When authorization or execution policy permits only a one-shot index,
agents use `repogrammar init --yes --no-autosync` instead.
When RepoGrammar reports missing or stale analysis for an already initialized
repository, agents may run `repogrammar resync`. When a session should start
auto-sync after a successful standalone `resync`, agents may run
`repogrammar autosync start`.
When Rust self-dogfood families or conservative TS/JS
Express/Jest/Vitest/Next/Fastify/Prisma/Drizzle families are present, MCP
returns them through the same metadata-only/read-plan contract as Python
families. The MCP surface does not expose Tree-sitter nodes, Cargo output, rustc
output, TypeScript compiler output, source text by default, or additional
language-specific tools.

## Boundary rules

- MCP schema and transport errors stay in `src/rust/interfaces/mcp/`.
- Core types must not depend on MCP SDK types.
- MCP responses may include semantic-worker-derived facts only after they have
  been translated into RepoGrammar-owned evidence and certainty categories.
- Future Python provider facts from Pyrefly, Pyright, or RightTyper-style
  observed runs may appear in MCP only after translation into RepoGrammar-owned
  facts with provider provenance, freshness metadata, and current supported
  certainty tokens. MCP must not expose raw provider graphs, Python AST nodes,
  LSP payloads, or runtime traces as product results.
- Optional provider facts, including any future CodeGraph-derived facts, may
  appear only after translation into RepoGrammar-owned evidence with provider
  provenance and freshness metadata. Provider facts cannot independently prove
  pattern-family membership.
- Serialization tests are required before concrete schemas are accepted.
- Any tool name, parameter, return-shape, or error-semantics change must use
  `.agents/skills/mcp-contract-change/SKILL.md`.
