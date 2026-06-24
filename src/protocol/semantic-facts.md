# Semantic Worker Protocol

Semantic workers return facts from language-native analyzers to the Rust core.
Facts must not expose compiler, AST, LSP, or SDK types directly.

The v1 transport is newline-delimited JSON over stdio. Each line is one of:

- `fact`
- `progress`
- `worker_error`
- `end_of_stream`

Schemas live beside this document:

- `semantic-worker-message.schema.json`: full NDJSON message envelope.
- `semantic-worker.schema.json`: fact payload shape retained for focused tests.
- `progress-event.schema.json`: transport-neutral progress event shape.

Fixtures live under `fixtures/`.

Required fields for fact messages:

- `protocol_version`
- `message_type`
- `request_id`
- `fact_kind`
- `subject`
- `origin.engine`
- `origin.engine_version`
- `origin.method`
- `certainty`
- `evidence.code_unit_id`
- `evidence.path`
- `evidence.content_hash`
- `evidence.repository_revision`
- `evidence.start_byte`
- `evidence.end_byte`
- `evidence.note`
- `assumptions`

`target` is optional and nullable because facts such as local symbols,
framework roles, or unknown outcomes may not have a resolved target. When
present as a string, `target` must contain at least one non-whitespace
character.

Fact kind values are:

- `RESOLVED_CALL`
- `RESOLVED_IMPORT`
- `SYMBOL`
- `TYPE`
- `FRAMEWORK_ROLE`
- `UNKNOWN`

Certainty values are:

- `SEMANTIC`
- `DATAFLOW_DERIVED`
- `STRUCTURAL`
- `FRAMEWORK_HEURISTIC`
- `CONFLICTING`
- `UNKNOWN`

Conflicting facts must not be averaged into a confidence score. They must be
represented as `CONFLICTING` and normally lead to `UNKNOWN` or abstention.

If a TypeScript worker sees an unsupported compiler API version, it must emit
`SEMANTIC_VERSION_UNSUPPORTED` and fall back to syntax-only evidence. It must
not fabricate semantic certainty from Tree-sitter structure or framework
heuristics.
