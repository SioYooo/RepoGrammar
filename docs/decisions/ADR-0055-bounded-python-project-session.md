# ADR-0055: Bounded Python project-session transport

- Status: Accepted; default activation qualified on 2026-09-30
- Date: 2026-09-30
- Scope: private CPython frontend transport only, no semantic/support expansion
- Authority: the maintainer-requested measured efficiency sprint, ADR-0004 and
  ADR-0012; [preregistered observation and gates](../experiments/efficiency-sprint.md)

## Observation and decision

The frozen self-dogfood baseline admitted 176 Python files with 539,660 source
bytes. Full indexing spawned 176 document workers, transferred 94,980,160
module-source bytes, and performed 32,912 AST parses. Small N18/N34 runs likewise
transferred exactly N copies of project context. These observations apply to
requests below the existing context-omission cap. Native resources and overlapping
observer timers are reported separately in the experiment.

Qualify one explicit generation-owned Python project session when at least two
Python documents will be parsed. Single-document incremental work retains the
existing transport. Current source builds enable the qualified session by
default; `REPOGRAMMAR_PYTHON_PROJECT_SESSION=0` retains the old transport for
controlled comparison/rollback. Explicit `1` enables it and other values fail
closed. This internal switch is not a public CLI or permission to skip gates.
Published 0.5.0 assets remain immutable and keep their shipped transport.

`SourceParser::begin_project_session` optionally returns an owned
`ParserProjectSession`. Non-Python parsers default to no session. The indexing
application routes Python documents through it and observes successful `finish`
before its parse-phase checkpoint and generation activation. Early returns drop
the session and kill/wait the direct child. Storage rollback remains authoritative.

## Private protocol and resource contract

The admitted interpreter starts with shared isolated-stdlib flags `-I -S`,
then runs the checked-in worker through the existing bootstrap
with exact `--project-session`. Runtime admission occurs before worker load in
that same child. No analyzed source, import, plugin, test, setup script or network
operation executes.
This closes the inherited ambient `PYTHONPATH` startup-hook gap: bootstrap
alone starts too late to block `sitecustomize`. A live sentinel control proves
the old startup executes the fixture; all private request/session/version paths
must leave it untouched. This isolation is not a sandbox for a malicious
configured interpreter or a modified trusted worker asset.

The private control tuple is `protocol_version=1`, `contract_revision=2`,
`project_session_revision=1`, independent from public semantic-worker NDJSON
and the private project-config revision. Every header includes an opaque session
identity, SHA-256 of exact transmitted context bytes, strictly increasing integer
request id and message type. Parse headers also include the exact content hash
and Boolean `use_context`. Foreign/stale identity, duplicate keys, extra fields,
wrong types and tuple drift fail closed.

Newline-delimited frames have independent bounds:

- Control header: 4 KiB, including newline.
- Initial context and legacy document data frame: 1 MiB, including newline.
- Legacy document-result frame: 2 MiB, including newline.
- Host transport queues: capacity one. Detailed ASTs and current-file source/facts
  are ephemeral. Only bounded module identity, symbol/literal `__all__`/ordered
  reexport and conftest fixture-count projections survive.
- At most 100,000 document requests and 512 MiB target source, matching product
  discovery limits. Context source arrays are discarded after projection.
- Initialization, each operation and deterministic shutdown: bounded 30-second
  deadlines. Timeout kills/waits the child. Writes and output draining run
  concurrently; replies flush before the next request.

Start sends a header and one context frame, then receives `ready`. Parse sends
a header and existing revision-2 document payload, then receives a matching
`result` header and existing revision-2 result. Finish receives matching
`end_of_stream`, closes stdin and requires successful child exit plus clean
stdout EOF. Trailing data, missing EOS, crash, malformed or oversized output
cannot activate partial facts. Only sanitized typed contract/admission errors
or generic private frontend failures cross the port; raw payloads and stderr do not.

## Equivalence and omission

The legacy context decision remains exact: merging nonempty document and context
JSON objects has byte length `document.len + context.len - 1`. The host evaluates
that original length regardless of compact transport size. Formerly omitted
context remains omitted, with the same diagnostic. Context too large to fit any
legacy document is not transmitted. The old exact-1-MiB JSON boundary already
fails because its newline exceeds the worker cap; both transports reject it.

Projection preserves sorted-path overwrite and sorted-module, source-ordered
first-wins package reexports. Facts, UNKNOWNs, interface hashes, ranges, freshness
and support derivation remain canonically unchanged.

## Qualification before default activation

The gate passed: complete active-owned SQLite records matched the frozen old
binary/worker on self and pinned FastAPI backend; all eleven paired synthetic
body snapshots matched; the nine measured body pairs showed no repeatable
regression at native time resolution. Full indexing reduced worker/context/AST
work with bounded RSS, sync-equivalence passed 14/14, product-eval retained
zero false selections and 36/36 correct abstentions, and default payload
summaries remained byte-identical. Full final-source/main gates still apply.
See [complete comparisons](../experiments/data/full-generation-comparisons.v1.json)
and the [measured sprint](../experiments/efficiency-sprint.md).

Require complete parser/stored-analysis equivalence against the frozen old binary
and worker, every Python release fixture, N8/N16 process/context/AST regressions,
malformed/stale/hash/runtime/resource/timeout/crash/EOS rollback tests, full gates,
`sync-equivalence --all` and product-eval safety counters. Compare native
within-machine wall/CPU/peak RSS separately from observer counters. Reject
unbounded retention or single-document incremental regression. Configuration and
interface calls stay separately counted; one document session is not one total
subprocess. No language or provider completion gate is closed by this transport.

Query serving, invalidation narrowing, providers, parallel workers and instruction
adoption remain separate decisions and atomic changes.
