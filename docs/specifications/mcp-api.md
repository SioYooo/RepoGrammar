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
  `inspect_readiness` takes no `target` and ignores the evidence-shaping fields.
- optional `target`: non-empty string, at most 8192 bytes, with no control
  characters.
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
incompatibility, or `UNKNOWN`.
Like `find_analogues`, the initial target may be a path, symbol/member id,
framework role, or pattern question; exact family ids are optional follow-up
handles, not prerequisites.

CLI equivalent: `repogrammar explain`.

### check_conformance

Return a source-backed *static-alignment* certificate for a target, or abstain
with a typed reason. This is not a runtime-conformance verdict: matched family
context is never proof of runtime equivalence. `check_conformance` resolves the
target to a specific indexed code unit, selects a comparison family (the unit's
own family when it is a member, otherwise the single fresh ready family of its
`(language, kind, role)` key), and statically compares the target's indexed
feature profile against that family's constraint profile. The top-level `status`
is a static-alignment token (`STATICALLY_ALIGNED`, `STATIC_DEVIATION`,
`PARTIAL_ALIGNMENT`, `INSUFFICIENT_EVIDENCE`, or `UNKNOWN`) — never
`PASS`/`FAIL`/`CONFORMS` and never the legacy `CONTEXT_ONLY` advisory — and every
certificate carries `runtime_equivalence: "UNKNOWN"`. A stale, ambiguous,
unindexed, or family-less target abstains with `INSUFFICIENT_EVIDENCE` and never
surfaces a selected family. The initial target may be a path, path:line locator,
or `unit:` member id.

CLI equivalent: `repogrammar check`.

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

CLI equivalent: the source-free readiness fields of `repogrammar status --json`
and `repogrammar doctor --json` (`product_readiness`).

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
Fuzzy lookup must not hydrate every family. For `find_analogues`,
`explain_deviation`, and `check_conformance`, the public query mode is
`discover -> hydrate -> compose`: RepoGrammar accepts the path, symbol/member id,
framework role, or pattern question the caller has, discovers candidate family
ids internally from exact ids, exact member ids, repo-relative evidence paths,
and exact member roles, then hydrates only the capped candidate set. If role/path
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
`standard` and `full` keep the complete field set byte-for-byte. Exact `show_family`
lookups still require an exact family id and return typed `UNKNOWN` when that
family id is missing. `check_conformance` responses carry the static-alignment
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
`target_relationship` token no current path emits. `check_conformance` resolves
the target to exactly one code unit honoring a `path:line`, `path:byte-range`, or
`unit:` locator; a path-only target that names a file with more than one
family-eligible unit abstains with `INSUFFICIENT_EVIDENCE` and candidate unit ids.
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

Missing state, missing active indexes, and typed analysis uncertainty are normal
tool results. Unknown JSON-RPC methods, unknown tool names, invalid operations,
blank, oversized, or control-character-containing targets, oversized token
budgets, and malformed argument types are transport/schema errors.

MCP calls must not wait on telemetry network activity and must not trigger
telemetry upload. Anonymous telemetry upload is only attempted by explicit
`repogrammar telemetry upload` after consent and endpoint validation.
Successful family-context MCP calls may best-effort update the repo-local
aggregate `.repogrammar/telemetry/local-metrics/estimated_potential_token_savings.json`.
That local file stores only aggregate estimated token counts, event count,
`ESTIMATED` kind, and caveat text; it must not store operation targets, paths,
content hashes, prompts, source, evidence text, symbols, or raw errors.
MCP `repogrammar_context` calls may also best-effort update
`.repogrammar/telemetry/local-metrics/family_query_outcomes.json`, including
preflight fallback, successful family context, deterministic
`PARTIAL_CONTEXT`, typed query-time `UNKNOWN`, and runtime fallback outcomes.
This local file stores only low-cardinality aggregate buckets for operation
category, lookup mode, status, UNKNOWN class/reason/required-mechanism/recovery
code, read-plan item counts, and source-span request/inclusion/omission counts.
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

The first call for covered work is `repogrammar_context` with
`operation: "find_analogues"`, `target: "<the repo-relative path, symbol/member
id, framework role, or concrete code-work question from the task>"`, and
`mode: "compact"`. It occurs before CodeGraph or source search/read. Agents must
use the returned `read_plan` before editing and treat included line-numbered
`source_spans` as already read. They may fall back when RepoGrammar is
unavailable or explicitly returns `UNKNOWN`, `FALLBACK`, stale, omitted, or
insufficient evidence, and must state that reason before doing so. CodeGraph may
then provide exact source or call-path detail that RepoGrammar did not supply.
Agents must not repeat an identical RepoGrammar call unless the target or
indexed evidence changed. Agents use `show_family` only with exact family ids
returned earlier and leave `include_source_spans` unset unless bounded source
is explicitly needed.

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
When RepoGrammar returns missing-state fallback and the user has allowed
repo-local analysis state, agents may run
`repogrammar init --yes`, or `repogrammar init --yes --autosync` when the
session should keep agent-written files available to later RepoGrammar queries.
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
