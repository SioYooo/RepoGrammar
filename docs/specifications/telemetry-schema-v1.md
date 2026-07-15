# Anonymous Telemetry Schema v1

`src/protocol/telemetry-v1.schema.json` is the machine-readable allowlist for
anonymous product telemetry upload.

Allowed payload fields are coarse and anonymous only: schema/version fields, OS
family, unknown or configured agent target, anonymous machine id, count/ratio
buckets, external-dependency/thin-wrapper/token-saving risk buckets, typed
UNKNOWN/error-code count buckets, `source_snippets_returned: false`, and an
optional measured-token-savings bucket when a local paired experiment exists.
When experiment recording has a comparable local pair, uploads may also include
only bucketed/category experiment aggregate fields: experiment mode,
measurement-source category, token-savings-ratio bucket, correctness category,
whether a read plan was used, and read-plan item-count bucket.
Future source-span adoption fields may include only aggregate/bucketed counts,
for example source-span opt-in used, returned-span count bucket, or
omitted-span count bucket. They must not include source text, paths, hashes, or
byte ranges.

The payload intentionally has no repository instance id, repository root hash,
file path, symbol name, content hash, byte range, raw target, prompt, source
snippet, or raw error field.

The schema forbids source code, source snippets, prompts, query text, raw tool
input/output, file paths, repository names, package-private names, symbols,
function/class names, evidence text, environment variables, credentials, raw
error messages, patches, and diffs.

Payload validation happens before a passive diagnostics rollup is written,
before a batch is written to `.repogrammar/telemetry/queue/`, and again before
explicit upload. Invalid payloads must be refused rather than uploaded.
Bucket-map keys are restricted to short identifier-like reason/category names;
path-like strings, content hashes, byte ranges, raw targets, and symbol-like
dotted names are rejected.
