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

This keeps agent tool selection stable while preserving explicit internal
operation semantics. The CLI remains multi-command for human discoverability.

The current input schema is intentionally small:

- required `operation`: one of the four operation strings above.
- optional `target`: non-empty string, at most 8192 bytes, with no control
  characters.
- optional `token_budget`: positive integer up to 200000 used to cap selected
  evidence metadata. Supplying it implies `mode: evidence` unless an explicit
  mode is provided.
- optional `mode`: one of `compact`, `evidence`, or `deep`.
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

Check whether a target conforms to a selected family or abstain with a reason.
In the current v0.1 candidate, matched family context is not proof of runtime
equivalence. `check_conformance` must therefore return top-level
`CONTEXT_ONLY` or typed `UNKNOWN` when conformance is unproven, and its nested
check result must remain advisory `UNKNOWN`.
The initial target may be a path, symbol/member id, framework role, or pattern
question. RepoGrammar discovers and hydrates bounded candidate families
internally, then returns any exact family ids as follow-up handles.

CLI equivalent: `repogrammar check`.

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
metadata-only `output`, a target `read_plan`, optional `source_spans` only when
`include_source_spans: true` succeeds, and a typed `InsufficientSupport`
unknown for `pattern family evidence for resolved target`. `resolved_target`
preserves the raw target, resolved kind, repo-relative path, optional line,
optional byte range, optional family/member ids, symbol hints, residue terms,
candidate paths/ids, confidence, and match kind. It is local read-planning
context, not family evidence or conformance evidence. Exact `show_family`
lookups still require an exact family id and return typed `UNKNOWN` when that
family id is missing. `check_conformance` responses that return
`PARTIAL_CONTEXT` remain advisory: the nested `check` object must keep
`advisory_status: UNKNOWN`, explain that runtime equivalence remains unproven,
and omit proof-like fields such as `pass`, `conforms`, or `fail_on`.
All family lookup responses include `query_route`, a source-free route metadata
object with `route`, `input_kind`, `pipeline`, `family_id_policy`,
`candidate_limit`, `selected_family_id`, `candidate_family_ids`,
`follow_up_family_ids`, and `why_selected`. For fuzzy operations, its policy must
state that family ids are returned follow-up handles rather than required initial
inputs. `selected_family_id` is present only when RepoGrammar actually selected
one supported family. `candidate_family_ids` and `follow_up_family_ids` may be
present on `PARTIAL_CONTEXT` or `UNKNOWN`; those ids are narrowing handles, not a
family or conformance claim. When a natural-language, synonym, or
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
`compact` is the default and returns family summary, members, variation slots,
unknowns, output metadata, and a `read_plan` without evidence records;
`evidence` adds budgeted repo-relative evidence metadata selected by
deterministic greedy marginal coverage per estimated token cost; `deep` is
accepted only as an explicit detail request and remains metadata-first unless
`include_source_spans: true` is also provided. The `read_plan` is returned for
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
text under a separate `source_spans` object. The read plan never includes
absolute paths and does not imply source edits are safe outside listed ranges.
Output metadata includes the selection strategy, estimated evidence tokens,
estimated read-plan tokens, `estimated_potential_token_savings` with
`measurement_kind: ESTIMATED`, covered claim labels, missing claim labels, and
whether the rough budget was satisfied. `estimated_potential_token_savings` is
a potential read-displacement estimate for the returned RepoGrammar metadata
shape; it is not measured token savings and must carry a caveat saying so.
Stored family evidence carries
schema-backed `covered_claims` labels from the allowlist `canonical`,
`support`, `variation`, and `exception`; selectors must consume those labels
rather than infer coverage from notes or record order. The current family
builder emits `canonical` and `support`, plus a narrow Python `variation` label
when an already-ready family has multiple exact-compatible framework-anchor
support targets. It may also emit metadata-only variation slots when
parser-context profiles differ inside an already-supported Python family, but
those slots do not imply variation evidence coverage. Requested exception
coverage and broader variation coverage must be reported as missing until later
builders explicitly link evidence to those claims. Family detail unknowns
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
facade that can only request repository status and pattern-family lookup.
Indexing remains the only writer for repository analysis records. The only
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
For readiness triage, agents may run `repogrammar doctor --json` and read only
the source-free `readiness` object; they must treat `.codegraph/` entries as
foreign unmanaged provider state and must not ask RepoGrammar to create,
modify, or delete them.
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
