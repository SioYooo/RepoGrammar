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

The bootstrap does not include executable TypeScript worker code because local
TypeScript tooling is not yet validated.

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

The v1 NDJSON envelope supports these message types:

- `fact`
- `progress`
- `worker_error`
- `end_of_stream`

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
asserting message type, protocol version, evidence content hash, progress work
payloads, unsupported-version fallback payloads, and end-of-stream messages.
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
vector, not shell interpolation. When a request provides changed file paths, the
adapter must reject facts whose evidence path was not requested. Future indexing
integration must additionally match worker evidence against the active
generation manifest, content hashes, and code-unit ranges before storing facts
or using them for claims.

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
Rust mappings for fact-kind and certainty tokens, includes schemas plus
JSON-parsed NDJSON fixture tests, and has a Rust-side TypeScript process adapter
that can send request JSON over stdin, enforce a timeout, validate NDJSON
stdout, map sanitized worker errors, and translate fact messages into
RepoGrammar-owned semantic facts.

It still does not bundle a Node or TypeScript compiler worker, run TypeScript
compiler APIs, store semantic facts, match worker facts against an active
generation manifest, or parse worker JSON during the current syntax-only
`index`/`sync` slice.
