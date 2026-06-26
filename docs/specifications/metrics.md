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

`repogrammar stats --json` reports v0.1 repo-shape diagnostics when repository
state is initialized and an active generation is readable. The output must keep
measured `token_savings`, `token_savings_ratio`, and `measurement_source` as
`null` unless a paired baseline/treatment measurement exists. Current
diagnostics may report
derived or estimated local-pattern-density, family-support-coverage,
abstention-rate, external-dependency-signal, thin-wrapper-risk, and
token-saving-risk values.

Stats output is allowed to include aggregate counts and diagnostic ratios, but
it must not include source snippets, query text, repository names, absolute
paths, or causal token-savings claims. Missing repository state or a missing
active index should use the standard parseable fallback rather than inventing
repository metrics.
`stats --json` never opens a telemetry network connection. When anonymous
telemetry is effectively enabled, it may update a repo-local bucketed passive
diagnostics rollup only; disabled telemetry keeps the same diagnostics
local-only and must not create upload queue entries.
If treatment correctness fails, raw token deltas may still be reported, but the
result must be marked invalid for product token-saving claims.
