# Semantic Worker Specification

RepoGrammar uses a Rust primary core and allows language-native semantic
frontends where they provide the most authoritative project, symbol, and type
information.

## Purpose

Tree-sitter is the universal syntax layer. It is good for tolerant parsing,
incremental syntax updates, code-unit extraction, syntax fingerprints, and
candidate generation. It is not a complete semantic analyzer.

RepoGrammar is Tree-sitter-first, not Tree-sitter-only. Tree-sitter produces
structural candidates. Language-native semantic adapters validate and enrich
them. Framework adapters map syntax plus semantics into repository roles.

Semantic workers provide facts that Tree-sitter cannot reliably infer alone:

- project and module model;
- import and alias resolution;
- symbol resolution;
- type and generic information;
- overload signatures;
- inheritance and implementation relations;
- resolved calls where available;
- framework-specific semantic facts when appropriate.

## TypeScript worker strategy

The first planned worker is a TypeScript semantic worker. It should use the
official TypeScript compiler or language-service APIs behind a versioned
protocol.

Version policy:

- TypeScript 6 public compiler API can be used by a version-pinned adapter.
- TypeScript 7.0 API instability must be handled by CLI or LSP compatibility
  adapters, or by marking unavailable semantic facts as `UNKNOWN`.
- TypeScript 7.1 or later should be evaluated when stable programmatic APIs are
  available.

The bootstrap includes a dependency-free TypeScript worker stub under
`src/workers/typescript/`. That Node script validates the Rust-to-worker v1
request shape and returns typed `SEMANTIC_WORKER_UNAVAILABLE` or
`SEMANTIC_PROTOCOL_VIOLATION` NDJSON fallback messages. It does not run the
TypeScript compiler, inspect source files, or emit semantic facts. Compiler API
integration remains deferred until local TypeScript tooling and package-manager
lockfiles are validated.

## Python worker strategy

Python is planned as the second official language, not part of v0.1 production
support. Before v0.2, Python work may only be experimental dogfooding and should
be scoped to syntax-only or limited semantic evaluation. The first formal subset
should prioritize FastAPI, pytest, SQLAlchemy, and Pydantic. Django and C/C++
are deferred.

Python semantic facts should use a language-native analyzer such as Pyright,
Mypy, a language server, or a framework adapter where appropriate. Dynamic
imports, monkey patching, decorator rewrites, pytest fixture injection, Django
settings, and runtime dependency injection often require `UNKNOWN`.

Experimental Python results must carry support-level and unknown-reason metadata
so CLI, MCP, storage, and docs cannot accidentally present them as official
v0.1 TS/JS support.

## Protocol

Workers communicate with the Rust core through a versioned process protocol.
The first transport is NDJSON over stdio because it isolates
compiler crashes, supports multiple compiler versions, and avoids putting Node
or other runtimes inside the Rust core.

Protocol notes, schemas, and fixtures live under `src/protocol/`.

Rust sends one v1 JSON request object to worker stdin before reading worker
stdout. The request contains `protocol_version`, `request_id`, an absolute
canonical `project_root`, and sorted unique repository-relative `changed_files`.
Request fixtures must reject malformed JSON shape, missing required fields,
wrong protocol versions, duplicate changed files, absolute paths, traversal,
Windows absolute paths, URI-like paths, and backslash paths.

The v1 NDJSON envelope supports these message types:

- `fact`
- `progress`
- `worker_error`
- `end_of_stream`

`worker_error` is terminal for semantic analysis but not exempt from stream
validation: it must still be followed by `end_of_stream`, and no `fact` or
`progress` message may appear after it.

Fact messages use stable protocol tokens for fact kinds and certainty values.
Evidence must carry a core-mappable code unit id, repository-relative path,
strict SHA-256 content hash, repository revision, byte range, and note. Worker
errors must use typed error codes; unsupported TypeScript compiler API versions
use `SEMANTIC_VERSION_UNSUPPORTED` with a syntax-only fallback instead of
semantic certainty.

Fact `target` values are optional. When present, a target must be either `null`
or a non-blank string; empty and whitespace-only strings are not valid
semantic-worker targets.

Strict content hashes use the protocol form `sha256:<64 hex characters>`.
Fixtures and tests must reject placeholder hashes such as `sha256:fixture` and
must not treat non-SHA-256 strings as auditable provenance.

Protocol fixture tests must parse each NDJSON fixture line as JSON before
asserting message type, protocol version, repository-relative evidence paths,
evidence content hash, sanitized target/note text, progress work payloads,
unsupported-version fallback payloads, and end-of-stream messages.
These tests validate the fixture contract only. The Rust-side TypeScript
semantic-worker adapter also validates runtime worker stdout line by line before
translating fact messages into RepoGrammar-owned `SemanticFact` values. It must
reject malformed JSON, missing end-of-stream messages, blank targets, invalid
hashes, absolute or URI evidence paths, impossible progress counts, oversized
output, and unsupported source/snippet fields.

Worker process failures must be sanitized. The adapter may classify unavailable
workers, unsupported TypeScript versions, timeouts, crashes, and protocol
violations, but it must not return raw stderr, source snippets, or absolute
paths in errors.

Worker execution must use an explicit absolute executable path plus argument
vector, not shell interpolation. The Rust-side adapter must reject relative,
missing, symlink, or non-directory project roots before spawning the worker.
Requests must be size-bounded before writing to worker stdin so a worker that
does not read cannot bypass timeout supervision. Unsupported TypeScript compiler
API versions must not be accepted with `SEMANTIC` certainty; those facts must be
rejected as unsupported-version output unless the worker reports a syntax-only
or unknown fallback.
When a request provides changed file paths, the adapter must reject facts whose
evidence path was not requested. Runtime fact text fields must reject obvious
absolute paths, URI schemes, NUL/newline payloads, and source-like snippets
until source-retention policy is defined.

`index` and `sync` do not launch a semantic worker by default. If
`REPOGRAMMAR_TYPESCRIPT_WORKER` names an explicit worker executable, the current
indexing path sends the discovered repo-relative TS/JS file set to that worker
after syntax-only code units are recorded for the building generation.
`REPOGRAMMAR_TYPESCRIPT_WORKER_ARGS_JSON` may supply the executable argument
vector as a JSON array of non-blank strings. For the checked-in Node stub, use
an absolute Node executable as `REPOGRAMMAR_TYPESCRIPT_WORKER` and the worker
script path as an argument in `REPOGRAMMAR_TYPESCRIPT_WORKER_ARGS_JSON`. The
launcher must not parse shell strings, inherit PATH to satisfy shebang lookup,
or accept worker arguments without an executable. Returned facts are sorted
deterministically, translated into RepoGrammar-owned storage records, and
written only through the storage gate that matches evidence against the
building generation manifest, content hashes, and code-unit ranges.
Unavailable workers, unsupported TypeScript versions, timeouts, crashes, and
protocol violations produce syntax-only fallback statuses and sanitized
warnings. A worker fact that passes protocol parsing but does not match the
building generation's indexed path, hash, or code-unit range aborts the new
generation instead of becoming stale or partial semantic evidence.

Recorded semantic facts are not pattern-family evidence by themselves. Query,
MCP, conformance, and family membership claims remain deferred until freshness,
family-evidence read paths, and claim builders are implemented.

## Certainty

Facts use categorical certainty:

- `SEMANTIC`
- `DATAFLOW_DERIVED`
- `STRUCTURAL`
- `FRAMEWORK_HEURISTIC`
- `CONFLICTING`
- `UNKNOWN`

Do not average conflicting analyzer results. Conflicts normally become
`CONFLICTING` and lead to `UNKNOWN` or abstention.

`UNKNOWN` values should include a reason code and affected claim whenever they
cross a CLI, MCP, storage, metric, or protocol boundary. The shared taxonomy is
defined in `docs/specifications/unknowns.md`.

## Core boundary

All language-specific AST, compiler, type-checker, LSP, and SDK types must be
translated into RepoGrammar-owned semantic facts and unified IR before entering
`src/rust/core`.

Compiler-native semantic facts take precedence over structural heuristics.
Structural similarity may generate candidates, but it must not alone prove
semantic family membership.

## Implementation status

The bootstrap now pins semantic worker protocol version `1`, defines stable
Rust mappings for fact-kind and certainty tokens, includes request and output
schemas plus JSON-parsed request/NDJSON fixture tests, has a Rust-side
TypeScript process adapter that can send request JSON over stdin, enforce a
timeout, validate NDJSON stdout, map sanitized worker errors, and translate fact
messages into RepoGrammar-owned semantic facts, and includes a no-dependency
Node worker stub that reports semantic analysis as unavailable without echoing
source paths. `index` and `sync` can optionally execute a configured worker via
`REPOGRAMMAR_TYPESCRIPT_WORKER` plus
`REPOGRAMMAR_TYPESCRIPT_WORKER_ARGS_JSON`; default indexing still reports
`semantic_worker: deferred`.

It still does not bundle a TypeScript compiler dependency, run TypeScript
compiler APIs, use worker facts for family claims, expose semantic facts through
query/MCP commands, or treat stored semantic facts as freshness-validated
pattern-family evidence.
