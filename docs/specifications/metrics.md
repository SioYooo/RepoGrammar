# Metrics Specification

Every reported metric must be classified as one of:

- `MEASURED`
- `DERIVED`
- `ESTIMATED`
- `CAUSAL_EXPERIMENT`

## Context compression

`context_compression_ratio` may be derived from returned context and eligible
family source. It must not be labeled as actual token savings.

## Token savings

Actual token savings may only be reported when a comparable baseline exists:

```text
token_savings = baseline_session_tokens - treatment_session_tokens
```

All token counts must include the tokenizer or host-provided measurement source.
Accepted measurement sources are `host_reported`, `user_entered`, and
`documented_tokenizer`.

`estimated_potential_token_savings` is a separate `ESTIMATED` diagnostic. It is
computed from RepoGrammar's selected family-evidence metadata, read-plan token
estimates, and any explicitly requested source-span token estimate. It is a
potential read-displacement estimate for the current RepoGrammar output shape;
it is not actual token savings, not a causal claim, and not a substitute for a
paired baseline/treatment measurement. The estimate must saturate at zero when
the returned context is larger than the local baseline estimate.

v0.1 records local paired measurements through:

```text
repogrammar telemetry experiment-start --name <id> --experiment-mode record-existing --session baseline --measurement-source user_entered --yes
repogrammar telemetry experiment-record --name <id> --input-tokens <n> --output-tokens <n> --tool-tokens <n> --success true
repogrammar telemetry experiment-stop --name <id>
repogrammar telemetry experiment-start --name <id> --experiment-mode record-existing --session treatment --measurement-source user_entered --yes
repogrammar telemetry experiment-record --name <id> --input-tokens <n> --output-tokens <n> --tool-tokens <n> --success true
repogrammar telemetry experiment-stop --name <id>
repogrammar telemetry experiment-report --name <id> --json
```

`experiment-record` also accepts a redacted local usage import:

```text
repogrammar telemetry experiment-record --name <id> --usage-json <path>
```

The usage JSON may contain only token counts, optional success, and optional
test outcome. Counts may appear at the top level or under `usage`; accepted
count names are `input_tokens`/`prompt_tokens`,
`output_tokens`/`completion_tokens`, optional `tool_tokens`, and optional
`total_tokens` for deriving `tool_tokens`. Command-line token and success
flags override imported values. If no separate or derivable tool-token count is
reported, `tool_tokens` defaults to zero. Unsupported fields are rejected so a
raw host response containing prompts, messages, source, paths, symbols, or
patches cannot become an experiment record.

`--experiment-mode record-existing` records token counts from sessions the user
already performed. `--experiment-mode controlled-pair` records a comparable
baseline/treatment pair and must warn that the user may spend extra time,
tokens, and provider cost by choosing to run separate sessions. `--session`
selects `baseline` or `treatment`. Starting either experiment mode requires
explicit `--yes` in non-interactive use.

Experiment records store token counts, success/test outcome, coarse optional
task buckets, and read-plan metadata only. They must not store prompts, source,
paths, repository names, symbols, patches, or query text.
`repogrammar telemetry experiment-export --json` is redacted by default: it
does not include the user-provided experiment name, session ids, or raw token
counts, and it reports token/count data only through coarse buckets.

## Product claims

Production telemetry may support ecological-validity analysis, but it must not
be used alone to make causal claims about token savings.

## CLI status

`repogrammar stats --json` reports v0.1 Python-family repo-shape diagnostics
when repository state is initialized and an active generation is readable. The
output must include `official_family_scope: python_v0_1`,
`repo_shape_scope: python_family_eligible_units`, the active generation, a
source-free `readiness` subset, and an `indexed_inventory` object with
`indexed_file_count`, `indexed_code_unit_count`, and `semantic_fact_count`.
The indexed inventory describes what was indexed, not which families are
supported. A non-Python repository may therefore have nonzero indexed counts
while top-level `eligible_code_units` remains `0`. The output must keep
measured `token_savings`, `token_savings_ratio`, and `measurement_source` as
`null` unless a paired baseline/treatment measurement exists. Current
diagnostics may report
derived or estimated local-pattern-density, family-support-coverage,
abstention-rate, external-dependency-signal, thin-wrapper-risk, and
token-saving-risk values. The default `stats --json` path uses read-model
aggregate counts and must not hydrate family evidence, semantic facts, IR,
full claim snapshots, or family detail; `--unknowns` is the explicit option
that adds the persisted semantic unknown inventory. It may also report the
repo-local aggregate
`estimated_potential_token_savings` with `measurement_kind: ESTIMATED`,
`event_count`, aggregate estimated baseline/returned token counts, and the
caveat that it is not measured token savings.
It also reports a separate local-only `query_outcome_rollup` with
`rollup_scope: local_query_outcomes`. That rollup counts every recorded
family-query outcome by low-cardinality status, entrypoint, command or MCP
operation category, lookup mode, typed UNKNOWN class/reason/mechanism/recovery
bucket, read-plan count bucket, and source-span request/inclusion/omission
bucket. It is not an anonymous telemetry upload payload and is not a token
savings metric.
Top-level stats output must also report `token_saving_readiness`,
`blocking_reasons`, `measurement_kind`, and `caveat`. When no comparable paired
experiment exists, top-level `measurement_kind` remains `ESTIMATED`,
`blocking_reasons` includes `no_paired_experiment`, and the caveat states that
the value is estimated potential only, not measured token savings. With
`--unknowns`, stats may additionally embed the source-free persisted semantic
`unknown_inventory` object. That object carries
`inventory_scope: persisted_semantic_unknowns` and must not be interpreted as a
Python-only repo-shape readiness metric.

Stats JSON must keep official family readiness separate from unsupported and
preview indexed context. The `by_language` buckets cover the official Python
v0.1 scope and the bounded `typescript/javascript`, `rust`, `java`, `csharp`,
and `c/cpp` preview scopes (the `c/cpp` scope groups the raw `c`, `cpp`, and
`cpp-config` language tokens). For the current TS/JS scope, `by_language` reports
source-free `indexed_file_count` and `indexed_code_unit_count` in addition to
family support counts; the same source-free counts apply to the Rust, Java, C#,
and C/C++ preview scopes. The Rust preview readiness scope is now
`bounded_v0_2_preview`/`bounded_preview` (covering both the self-dogfood role
families and general serde/thiserror/tokio/clap/axum framework families), no
longer an internal self-dogfood-only scope. When TS/JS code units are indexed
but no supported TS/JS
families are available, `scope_explanations` must report
`tsjs_indexed_context_available: true`,
`tsjs_family_support: none_or_unsupported`,
`react_rn_family_support: unsupported`, and
`recommended_next_action: use repogrammar find/check with exact repo-relative paths for PARTIAL_CONTEXT read plans`.
This is an indexed-context diagnostic only; it must not create React/RN family
support, React/RN conformance, or token-saving claims.

Stats output is allowed to include aggregate counts and diagnostic ratios, but
it must not include source snippets, query text, repository names, absolute
paths, content hashes, byte ranges, or causal token-savings claims. Source-span
usage may be counted only as aggregate/bucketed values. Missing repository
state or a missing active index should use the standard parseable fallback
rather than inventing repository metrics, while still reporting unknown
readiness, estimated measurement kind, a not-measured caveat, and blocking
reasons. When `--unknowns` was requested and inventory is unavailable, fallback
JSON must include `inventory_available: false` to mark a not-ready inventory
rather than an internal crash.
`stats --json` never opens a telemetry network connection. When anonymous
telemetry is effectively enabled, it may update a repo-local bucketed passive
diagnostics rollup only; disabled telemetry keeps the same diagnostics
local-only and must not create upload queue entries.
Successful family context responses may update a separate repo-local aggregate
under `.repogrammar/telemetry/local-metrics/` for
`estimated_potential_token_savings`. That aggregate is local-only and must not
include source snippets, prompts, query text, paths, repository names, symbols,
content hashes, byte ranges, evidence text, or raw targets.
Family query and MCP context calls may update a second repo-local aggregate in
the same directory for `family_query_outcomes`. That aggregate may count
`UNKNOWN`, `PARTIAL_CONTEXT`, and fallback outcomes, but those counts must not
be added to `estimated_potential_token_savings` events or described as
successful family hits.
If treatment correctness fails, raw token deltas may still be reported, but the
result must be marked invalid for product token-saving claims.
