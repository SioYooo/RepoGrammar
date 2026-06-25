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

## Python worker strategy

The official v0.1 semantic-frontend target is Python. Python worker and adapter
work should serve repository-local family evidence for FastAPI, pytest,
SQLAlchemy, and Pydantic rather than trying to build a full Python analyzer.

The canonical Python algorithm contract is
`docs/specifications/python-analysis.md`, refined by
`docs/decisions/ADR-0012-python-selective-analysis-cascade.md`. The worker
strategy should combine:

- CPython `ast`, `symtable`, and `tomllib` for primary syntax, scope, and
  configuration facts;
- Tree-sitter Python only as tolerant structural fallback when CPython parsing
  fails or the worker is unavailable;
- conservative repo-local import and alias resolution;
- Pyrefly as the primary static semantic provider behind a public CLI or
  LSP-style boundary;
- selective Pyright cross-checks only for facts that would upgrade or materially
  change family claims;
- framework-role facts for decorators, base classes, calls, and fixture
  bindings;
- usage-driven fixpoint-lite context propagation;
- Pyrefly call hierarchy followed by bounded JARVIS-lite target recovery when
  needed;
- typed `UNKNOWN` for dynamic behavior.

Provider facts must be translated into RepoGrammar-owned facts before entering
Rust core or storage. Concrete Pyrefly, Pyright, Python AST, LSP, or runtime
objects must not enter Rust core. Provider provenance and freshness metadata
must include provider name/version, command or API operation, provider config
hash, Python version, environment fingerprint, input file hashes, source ranges,
and query operation.
The Rust `ports::python_provider` module now defines those future provider
request, provenance, cache-key, and unavailable-UNKNOWN boundaries as
RepoGrammar-owned types. It is not an adapter and does not execute Pyrefly,
Pyright, RightTyper, or repository code. Future provider adapters must still
translate accepted facts through the existing semantic-worker path and into
same-unit path/hash/range evidence before those facts can support a family
claim.

Pyrefly and Pyright agreement may support a future cross-checked certainty tier
only after Rust domain, protocol schemas, storage, CLI, MCP, and tests define
that token. Until then, worker output must use current certainty tokens and keep
cross-check status in assumptions/provenance. RightTyper-style runtime evidence
is deferred, explicit opt-in, and observed-only; it must never run during
default `index` or `sync`.

Dynamic imports, monkey patching, decorator rewrites, pytest fixture injection,
settings/framework magic, runtime dependency injection, missing dependencies,
and analyzer disagreement must become typed `UNKNOWN` for affected claims.

## TypeScript worker strategy

The existing TypeScript worker boundary remains transitional substrate from the
earlier bootstrap. It should use the official TypeScript compiler or
language-service APIs behind a versioned protocol if TS/JS support is promoted
again after Python v0.1.

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
Semantic-worker request and evidence paths use the same lexical repo-relative
policy as storage: non-empty slash-separated paths only, with absolute paths,
Windows drive prefixes, backslashes, URI-like text, control characters,
`.`/`..` traversal segments, and empty segments rejected.

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
does not read cannot bypass timeout supervision. The Rust-side TypeScript
adapter and the checked-in Node stub share a 1 MiB stdin envelope for a request,
counting the newline terminator that Rust writes after the JSON request object.
Timeout handling must not block on stdout or stderr pipes inherited by worker
descendants after the direct worker process is killed.
Unsupported TypeScript compiler API versions must not be accepted with
`SEMANTIC` certainty; those facts must be rejected as unsupported-version output
unless the worker reports a syntax-only or unknown fallback.
When a request provides changed file paths, the adapter must reject facts whose
evidence path was not requested. If a request has no changed files, an
end-of-stream-only response is allowed but any returned fact must be rejected
rather than treated as repository-wide scope. Runtime fact text fields must
reject obvious absolute paths, URI schemes, NUL/newline payloads, and
source-like snippets until source-retention policy is defined.

`index` and `sync` do not launch a semantic worker by default. The current
implemented worker runtime is TypeScript-specific transitional substrate. If
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

Recorded semantic facts are not pattern-family evidence by themselves. The
storage/query boundary can load an internal active-generation claim-input
snapshot and run file-hash freshness plus readiness checks over its semantic
facts. The current EC-MVFI-lite builder may consider strong same-generation
`SEMANTIC` or `DATAFLOW_DERIVED` non-framework facts as support when compatible
framework-role candidates exist. Raw semantic facts remain internal, and
syntax-origin `FRAMEWORK_HEURISTIC` facts alone must still yield typed
`UNKNOWN`.

Not every stored semantic fact is emitted by a semantic worker. The current
TS/JS indexing path may store syntax-origin framework-role facts produced by a
lightweight framework adapter while still reporting `semantic_worker: deferred`.
Those facts are not TypeScript compiler analysis and do not change the worker
protocol status.

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
`semantic_worker: deferred`. The storage/query boundary can load an internal
active-generation claim-input snapshot after validating files, units, IR
nodes/edges, stored fact tokens, assumptions JSON, repo-relative evidence paths,
strict content hashes, and byte ranges. The query application layer can
additionally check snapshot semantic facts against current source hashes and
block stale facts, unsupported fact kinds, weak certainty, or conflicting facts
with typed `UNKNOWN` readiness outcomes. The default TS/JS indexing path can
also store syntax-origin `FRAMEWORK_ROLE` facts with `FRAMEWORK_HEURISTIC`
certainty for recognized framework-shaped code units; these records are not
worker facts and remain blocked from family claims as insufficient support.
The default Python indexing path can now call the checked-in
`src/workers/python/worker.py` in private parse-document mode to extract
CPython `ast` code-unit metadata for `.py` files. That private mode now also
returns worker-local structural fact payloads for import bindings, decorator
anchors, class bases, SQLAlchemy mapped fields, `relationship(...)` calls,
typed SQLAlchemy session calls including `add`, simple call targets, FastAPI
static `response_model=...` schema slots, static `Depends(get_db)` dependency
target slots, `Depends`/`HTTPException` calls, literal
`HTTPException(status_code=...)` status-code effect slots, `pytest.test`
test-function anchors, same-file pytest fixture edges, literal pytest
parametrize argument anchors, Pydantic validator decorators, path-derived
module names, and CPython `symtable` scope anchors, plus typed `UNKNOWN` facts for
dynamic import, unresolved import, framework magic, and unresolved pytest
fixture injection cases. Literal pytest parametrize arguments are structural
parametrize facts, not unresolved fixture injections. Default indexing passes the
discovered repo-relative `.py` inventory plus bounded, hash-checked discovered
`conftest.py` file contents into that private parse-document request, so unique
repo-local module imports and parent-directory pytest fixture bindings can be
recorded as `STRUCTURAL` source-tied parser facts while ambiguous/missing
imports or fixtures remain typed `UNKNOWN`s. The semantic-worker-compatible
Python project mode can also resolve requested `conftest.py` fixture names
through pytest's parent-directory hierarchy as structural fixture-edge facts
without returning source snippets or absolute paths. The default product
indexing path validates and stores those
private parse-document payloads as internal parser-origin semantic facts with
`STRUCTURAL` or `UNKNOWN` certainty, but does not expose raw facts through
CLI/MCP query commands and does not pass raw facts to family construction. The
application layer may synthesize separate `DATAFLOW_DERIVED` support facts from
exact canonical Python anchors plus a single framework role; those derived
facts are RepoGrammar-owned and are not emitted by the CPython worker itself.
The worker also has a private `parse_project_config` mode that uses
standard-library `tomllib`
when available to return sanitized `pyproject.toml` summaries and typed
project-config `UNKNOWN` values. Default indexing uses that private mode for a
root `pyproject.toml`, while Rust still reads the file through the source-store
path/hash boundary and translates the sanitized summary into a `python-config`
file, `project_config` code unit, and internal `PROJECT_CONFIG`/`STRUCTURAL` or
`UNKNOWN` records. These config records are not provider facts, are not passed
to family construction, and remain blocked from claim-input readiness as
insufficient support. The same Python worker
has a semantic-worker-compatible NDJSON project mode that builds a bounded
repo-local module graph from requested `.py` files, applies sanitized
`pyproject.toml` source roots when `tomllib` is available, emits structural
repo-local import facts only for unique module-level matches, emits typed
`UNKNOWN` for ambiguous/missing repo-local imports and `sys.path` mutation, and
also emits conservative Python framework-role heuristic facts for direct worker
smoke testing. The product runtime does not separately launch it as a Pyrefly,
Pyright, usage-propagation, or call-hierarchy provider, and does not expose
these worker-local facts through CLI/MCP query commands.
Python worker tests run under `python3` and assert parseable JSON/NDJSON,
repo-relative paths, strict content hashes, no source snippets, invalid path
rejection, syntax diagnostics, structural fact output, typed `UNKNOWN` output,
bounded semantic-mode file reads, project-config summary sanitization, and
framework-role heuristic output.

It still does not bundle a TypeScript compiler dependency, run TypeScript
compiler APIs, run Pyrefly/Pyright, expose semantic facts through query/MCP
commands, or treat stored semantic facts as pattern-family evidence without the
family builder's compatibility and support checks.
