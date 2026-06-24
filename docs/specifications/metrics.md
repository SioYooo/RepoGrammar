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

During bootstrap, `repogrammar stats --json` returns a deferred metrics contract
instead of measured repository metrics. The JSON output must keep
`implemented: false`, report an empty `metrics` array, list the allowed metric
kinds, and use `null` for `token_savings` and `context_compression_ratio`.
Deferred stats output must not include source snippets, paths, repository names,
query text, or token-savings claims.
