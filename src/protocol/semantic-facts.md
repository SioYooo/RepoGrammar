# Semantic Worker Protocol

Semantic workers return facts from language-native analyzers to the Rust core.
Facts must not expose compiler, AST, LSP, or SDK types directly.

Required fields for future wire messages:

- `protocol_version`
- `request_id`
- `fact_kind`
- `subject`
- `target`
- `origin.engine`
- `origin.engine_version`
- `origin.method`
- `certainty`
- `evidence.path`
- `evidence.start_byte`
- `evidence.end_byte`
- `assumptions`

Certainty values are:

- `SEMANTIC`
- `DATAFLOW_DERIVED`
- `STRUCTURAL`
- `FRAMEWORK_HEURISTIC`
- `CONFLICTING`
- `UNKNOWN`

Conflicting facts must not be averaged into a confidence score. They must be
represented as `CONFLICTING` and normally lead to `UNKNOWN` or abstention.
