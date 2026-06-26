# Anonymous Telemetry Schema v1

`src/protocol/telemetry-v1.schema.json` is the machine-readable allowlist for
anonymous product telemetry upload.

Allowed payload fields are coarse and anonymous only: schema/version fields, OS
family, unknown or configured agent target, anonymous machine id, count/ratio
buckets, external-dependency/thin-wrapper/token-saving risk buckets, typed
UNKNOWN/error-code count buckets, `source_snippets_returned: false`, and an
optional measured-token-savings bucket when a local paired experiment exists.

The payload intentionally has no repository instance id, repository root hash,
file path, symbol name, content hash, byte range, raw target, prompt, source
snippet, or raw error field.

The schema forbids source code, source snippets, prompts, query text, raw tool
input/output, file paths, repository names, package-private names, symbols,
function/class names, evidence text, environment variables, credentials, raw
error messages, patches, and diffs.

Payload validation happens before a batch is written to
`.repogrammar/telemetry/queue/` and again before explicit upload. Invalid
payloads must be refused rather than uploaded.
Bucket-map keys are restricted to short identifier-like reason/category names;
path-like strings, content hashes, byte ranges, raw targets, and symbol-like
dotted names are rejected.
