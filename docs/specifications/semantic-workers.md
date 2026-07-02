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
The current application layer can plan candidate-scoped Pyrefly
`ResolveFrameworkIdentity` requests for future adapters when a Python candidate
group has one supported framework role, enough support, and no parser-origin
blocking `UNKNOWN` for the claim being planned. Planning only constructs
validated request envelopes, including from active-generation snapshot records;
it is not worker execution, provider support, storage ingestion, or a
family-claim upgrade.

Pyrefly and Pyright agreement may support a future cross-checked certainty tier
only after Rust domain, protocol schemas, storage, CLI, MCP, and tests define
that token. Until then, worker output must use current certainty tokens and keep
cross-check status in assumptions/provenance. RightTyper-style runtime evidence
is deferred, explicit opt-in, and observed-only; it must never run during
default `index` or `sync`.

Dynamic imports, monkey patching, decorator rewrites, pytest fixture injection,
settings/framework magic, runtime dependency injection, missing dependencies,
and analyzer disagreement must become typed `UNKNOWN` for affected claims.
The current CPython `ast` worker emits structural FastAPI request-shape anchors
for static `Body`, `Path`, `Query`, `Header`, and `Cookie` route parameter
markers. These anchors are parser-origin context metadata and do not become
family evidence.
The current CPython `ast` worker emits parser-origin typed `UNKNOWN` facts for
non-literal dynamic imports, `sys.path` mutation, dynamic calls, dynamic
decorator factories that block framework identity, monkey-patching calls such
as `setattr(...)`, unresolved imports, and pytest fixture ambiguity. These
facts remain structural parser output and do not become family evidence.

## Rust provider strategy

The Rust provider-backed path is planned in
`docs/plans/rust-tsjs-semantic-analysis-plan.md`. The Rust
`ports::rust_provider` module defines owned request, provenance, cache-key,
output, and unavailable `UNKNOWN` boundaries for future Cargo metadata,
rust-analyzer, rustc, and rustdoc JSON adapters. It is not an adapter and does
not execute Cargo, rustc, build scripts, procedural macros, or repository code.
The current `adapters::semantic_workers::rust` slice implements only the Cargo
metadata project-model provider. During `index`, `sync`, and `resync`, the
product runtime wires this provider into the safe indexing path after
same-generation `Cargo.toml` code units exist. Non-Rust repositories, or
repositories with no discovered `Cargo.toml`, skip this substage. The provider runs
`cargo metadata --format-version=1 --no-deps`, parses
workspace/package/target/feature/dependency metadata into owned
`PROJECT_CONFIG` semantic facts, and returns recoverable `UNKNOWN`s for
unavailable Cargo, unreadable project configuration, or missing manifest
candidates. Absolute manifest paths returned by Cargo are scoped against the
canonical project root before they are accepted, so symlink-equivalent roots do
not become missing candidates. It does not execute build scripts or procedural
macros, and its facts do not prove Rust symbols, types, calls, trait dispatch,
dataflow, borrow facts, or family support.

Future Rust adapters must translate Cargo/rustc/rust-analyzer/rustdoc objects
into RepoGrammar-owned `SemanticFact`, `Evidence`, `Provenance`, and
`TypedUnknown` values. Provider provenance must include tool/provider version,
toolchain, Cargo metadata hash, cfg/profile hash, environment fingerprint,
query operation, build-script execution status, proc-macro execution status,
and candidate file hashes/ranges. Build scripts and proc macros remain disabled
by default; when disabled or unavailable, generated/macro-expanded facts must
stay typed `UNKNOWN` for affected package/crate/claim scope.

Root `Cargo.toml` build-variant ambiguity may block repository-wide Rust
self-dogfood families. Nested fixture/package manifests must not globally block
unrelated root Rust family support.

## TypeScript worker strategy

The existing TypeScript worker boundary remains transitional substrate from the
earlier bootstrap. It should use the official TypeScript compiler or
language-service APIs behind a versioned protocol if TS/JS support is promoted
again after Python v0.1.

The TS/JS provider-backed path is planned in
`docs/plans/rust-tsjs-semantic-analysis-plan.md`. The Rust
`ports::tsjs_provider` module defines owned request, provenance, cache-key,
output, and unavailable `UNKNOWN` boundaries for future TypeScript Compiler
API, TypeScript Language Service, CodeQL, TAJS/JSAI/WALA, and Closure-style
adapters. It is not an adapter and does not execute Node package scripts.
Future adapters must translate TypeScript `Program`/`TypeChecker`, Language
Service, CodeQL, and abstract-analysis objects into owned facts before storage.
Dynamic import, non-literal `require`, `eval`, prototype mutation, proxies,
decorator rewrites, ambient globals without project context, and bundler-only
aliases remain typed `UNKNOWN` unless a configured provider proves a narrower
claim.

Version policy:

- TypeScript 6 public compiler API can be used by a version-pinned adapter.
- TypeScript 7.0 API instability must be handled by CLI or LSP compatibility
  adapters, or by marking unavailable semantic facts as `UNKNOWN`.
- TypeScript 7.1 or later should be evaluated when stable programmatic APIs are
  available.

The checked-in TypeScript worker under `src/workers/typescript/` now implements
a bounded request-operation slice for module/export evidence. It validates the
Rust-to-worker v1 request shape, accepts only repo-relative paths and strict
hash/range evidence, and supports these operations:
`resolve_module_specifier`, `resolve_export`, `resolve_reexport`, and
`resolve_package_entry`. When an official TypeScript module is available from
the worker environment, the worker may use the compiler API's module resolver
and source-file parser for exact export identity, including `resolve_reexport`
operations that prove both a repo-local module target and named export for a
bounded source anchor, then emit `SEMANTIC` facts with `provider=typescript`,
`provider_resolved=true`, `query_operation=<operation>`, config/package hashes,
and a safe environment fingerprint. When no TypeScript API is available, the
checked-in worker falls back to a dependency-free bounded project-model
resolver that can emit only `STRUCTURAL` exact context or typed `UNKNOWN`; that
fallback must not be described as compiler-backed provider evidence and must not
support family claims by itself. Malformed config, missing dependencies,
ambiguous exports/re-exports, dynamic imports, and unsupported package entries
remain typed `UNKNOWN`.

## Protocol

Workers communicate with the Rust core through a versioned process protocol.
The first transport is NDJSON over stdio because it isolates
compiler crashes, supports multiple compiler versions, and avoids putting Node
or other runtimes inside the Rust core.

Protocol notes, schemas, and fixtures live under `src/protocol/`.

Rust sends one v1 JSON request object to worker stdin before reading worker
stdout. The request contains `protocol_version`, `request_id`, an absolute
canonical `project_root`, sorted unique repository-relative `changed_files`,
and a bounded `operations` array. Each TypeScript operation carries an
operation id, operation token, repo-relative path, strict content hash,
code-unit id, byte range, literal specifier, project-config hash,
package-metadata hash, and max-files/max-bytes bounds. Request fixtures must
reject malformed JSON shape, missing required fields, wrong protocol versions,
duplicate changed files, absolute paths, traversal, Windows absolute paths,
URI-like paths, backslash paths, source-like literal specifiers, invalid hashes,
invalid ranges, and zero bounds.

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
adapter and the checked-in Node worker share a 1 MiB stdin envelope for a
request, counting the newline terminator that Rust writes after the JSON
request object.
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
vector as a JSON array of non-blank strings. For the checked-in Node worker, use
an absolute Node executable as `REPOGRAMMAR_TYPESCRIPT_WORKER` and the worker
script path as an argument in `REPOGRAMMAR_TYPESCRIPT_WORKER_ARGS_JSON`. The
launcher must not parse shell strings, inherit PATH to satisfy shebang lookup,
or accept worker arguments without an executable. The operation plan includes
literal module specifier requests from parser import/export/require facts,
bounded re-export requests for `export * from "<specifier>"` UNKNOWNs encoded
as `<specifier>#*`, `resolve_export` requests for exact Next.js
file-convention route/page/layout/API anchors, and provider-required Prisma
shared-client binding requests encoded as `<specifier>#<export>` for relative
repo-local named imports such as `./db#prisma`. Returned facts are sorted
deterministically, translated into RepoGrammar-owned storage records, and
written only through the storage gate that matches evidence against the
building generation manifest, content hashes, code-unit ranges, and requested
operation provenance. A TypeScript-provider `resolve_export` fact can feed a
post-worker TS/JS derived-support fact only when it matches the parser Next.js
anchor's same path, hash, code unit, range, framework role, and export name. A
TypeScript-provider `resolve_reexport` fact can feed Prisma derived support
only when it matches a provider-required parser anchor's same path, hash, code
unit, range, framework role, relative import specifier, and export name.
Dependency-free fallback facts with `provider_resolved=false` remain context
only.
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
TypeScript process adapter that can send operation-bearing request JSON over
stdin, enforce a timeout, validate NDJSON stdout, map sanitized worker errors,
match returned facts to requested operation evidence, and translate fact
messages into RepoGrammar-owned semantic facts. The checked-in Node worker can
run a bounded TypeScript compiler-API module-resolution path when the API is
available, and otherwise returns only dependency-free structural fallback facts
or typed `UNKNOWN`s without echoing source paths. `index`, `sync`, and `resync`
can optionally execute a configured worker via `REPOGRAMMAR_TYPESCRIPT_WORKER`
plus `REPOGRAMMAR_TYPESCRIPT_WORKER_ARGS_JSON`; default indexing still reports
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
anchors, class bases, Pydantic model-member anchors for fields, field
annotation targets, `model_config`, nested `Config`, `computed_field`,
validator, and `model_validator` declarations, SQLAlchemy mapped fields,
`relationship(...)` calls, typed SQLAlchemy session calls including `add`,
`execute`, `scalar`, `scalars`, `commit`, and `rollback`, and bounded
`__init__`-assigned `self.session`/`self.db` receiver propagation with
same-method reassignment invalidation, simple call targets, bounded
same-function FastAPI service-call context anchors, FastAPI
static `response_model=...` schema slots, static `Depends(get_db)` dependency
target slots, `Depends`/`HTTPException` calls, literal
`HTTPException(status_code=...)` status-code effect slots, `pytest.test`
test-function anchors, same-file pytest test and fixture dependency edges with
literal `name=` aliases, known pytest built-in fixture context, literal pytest
parametrize argument anchors, path-derived module names, and CPython `symtable`
scope anchors, plus typed `UNKNOWN` facts for dynamic import, `__import__`,
`locals()[...]`, `eval`, `exec`, `compile`, unresolved import, framework magic,
dynamic or unresolved decorators, dynamic Pydantic model factories, dynamic pytest fixture names,
duplicate conftest fixture bindings, plugin-style fixture names without an
allowlist or provider, and unresolved pytest fixture injection cases. Literal
pytest parametrize arguments are structural parametrize facts, not unresolved
fixture injections.
Pydantic field,
field-type, config, computed-field, and model-validator anchors are
schema/config/member context only, and FastAPI service-call anchors are
handler/service context only; neither category is an exact family-support
target. Default indexing passes the
discovered repo-relative `.py` inventory plus bounded, hash-checked discovered
`conftest.py` file contents into that private parse-document request, so unique
repo-local module imports, same-file fixture dependencies, and
parent-directory pytest fixture bindings can be recorded as `STRUCTURAL`
source-tied parser facts, known pytest built-ins can be recorded as external
fixture context, and ambiguous/missing imports, dynamic
fixture names, duplicate
conftest fixtures, or unresolved plugin fixtures remain typed `UNKNOWN`s. The
semantic-worker-compatible Python project mode can also resolve requested
unique `conftest.py` fixture names through pytest's parent-directory hierarchy
as structural fixture-edge facts without returning source snippets or absolute
paths. The default product
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

It still does not bundle a TypeScript compiler dependency, run package scripts,
run Pyrefly/Pyright, expose raw semantic facts through query/MCP commands, or
treat stored semantic facts as pattern-family evidence without the family
builder's compatibility and support checks.
