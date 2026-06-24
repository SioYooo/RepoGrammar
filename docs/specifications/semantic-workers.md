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

Protocol notes and a draft schema live under `src/protocol/`.

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
