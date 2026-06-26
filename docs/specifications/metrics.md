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

## Product claims

Production telemetry may support ecological-validity analysis, but it must not
be used alone to make causal claims about token savings.

## CLI status

`repogrammar stats --json` reports v0.1 repo-shape diagnostics when repository
state is initialized and an active generation is readable. The output must keep
measured `token_savings` and `context_compression_ratio` as `null` unless a
paired baseline/treatment measurement exists. Current diagnostics may report
derived or estimated local-pattern-density, family-support-coverage,
abstention-rate, external-dependency-signal, thin-wrapper-risk, and
token-saving-risk values.

Stats output is allowed to include aggregate counts and diagnostic ratios, but
it must not include source snippets, query text, repository names, absolute
paths, or causal token-savings claims. Missing repository state or a missing
active index should use the standard parseable fallback rather than inventing
repository metrics.
