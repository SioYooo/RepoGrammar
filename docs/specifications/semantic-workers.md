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
support. Before v0.2, Python work may only be experimental and should be scoped
to syntax-only or limited semantic evaluation. The first formal subset should
prioritize FastAPI, pytest, SQLAlchemy, and Pydantic. Django is deferred.

Python semantic facts should use a language-native analyzer such as Pyright,
Mypy, a language server, or a framework adapter where appropriate. Dynamic
imports, monkey patching, decorator rewrites, pytest fixture injection, Django
settings, and runtime dependency injection often require `UNKNOWN`.

## Protocol

Workers should communicate with the Rust core through a versioned process
protocol. The first planned transport is NDJSON over stdio because it isolates
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

Strict content hashes use the protocol form `sha256:<64 hex characters>`.
Fixtures and tests must reject placeholder hashes such as `sha256:fixture` and
must not treat non-SHA-256 strings as auditable provenance.

Protocol fixture tests must parse each NDJSON fixture line as JSON before
asserting message type, protocol version, evidence content hash, progress work
payloads, unsupported-version fallback payloads, and end-of-stream messages.
These tests validate the protocol contract only; they do not prove that a
TypeScript worker process exists or that runtime worker JSON is being consumed
by indexing.

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

## Core boundary

All language-specific AST, compiler, type-checker, LSP, and SDK types must be
translated into RepoGrammar-owned semantic facts and unified IR before entering
`src/rust/core`.

Compiler-native semantic facts take precedence over structural heuristics.
Structural similarity may generate candidates, but it must not alone prove
semantic family membership.

## Implementation status

The bootstrap now pins semantic worker protocol version `1`, defines stable
Rust mappings for fact-kind and certainty tokens, and includes schemas plus
JSON-parsed NDJSON fixture tests for a TypeScript semantic fact, progress, an
unsupported-version fallback, and end-of-stream messages. It still does not
launch a Node worker, run TypeScript compiler APIs, or parse worker JSON during
real indexing.
