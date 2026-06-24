# Indexing Pipeline Specification

The intended pipeline is:

```text
Repository files
-> File discovery and exclusion filtering
-> Tree-sitter parsing
-> Code-unit extraction
-> Unified IR
-> Language-native semantic facts
-> Normalization
-> Fingerprinting
-> Candidate discovery
-> Structural alignment
-> Anti-unification
-> Family construction
-> Pattern classification
-> SQLite persistence
```

## Bootstrap status

The repository currently defines module boundaries, semantic-worker protocol
placeholders, and minimal types only. It does not parse real code, call a
TypeScript compiler, build an index, align structures, anti-unify templates,
cluster families, or persist results.

## File discovery and exclusions

File discovery must respect repository ignore rules and RepoGrammar state
boundaries before parsing begins. RepoGrammar must skip `.repogrammar/` and
`.repogrammar-*` unconditionally, even when `REPOGRAMMAR_DIR` changes the active
state directory.

Discovery must honor `.gitignore` rules and default exclusions for dependency,
build, cache, coverage, virtual environment, and generated output directories.
Files larger than the configured size limit are skipped, with 1 MB as the
default limit.

Optional `repogrammar.json` may configure language enablement, custom file
extensions, include/exclude patterns, framework adapters, and family thresholds.
Malformed configuration must warn and fall back to safe defaults rather than
failing indexing.

## Tree-sitter parsing

Tree-sitter will be used in parsing and language adapters. AST nodes must be
converted into `CodeUnit` and unified IR types before entering `core`.

Tree-sitter provides tolerant syntax and candidate generation. It is not
responsible for complete symbol, type, overload, alias, or module-resolution
facts.

Tree-sitter facts are structural evidence. They can participate in framework
role detection and candidate ranking, but they cannot independently prove
function identity, call targets, framework roles, type compatibility,
dependency-injection bindings, transaction semantics, authorization semantics,
or test fixture binding.

## Language-native semantic frontends

Language-native frontends provide project models, module resolution, symbol
resolution, type information, inheritance, and resolved calls where available.
The TypeScript worker is the first planned semantic frontend. Other languages
should use their own compiler, type-checker, or LSP where that is the most
authoritative source.

The first official language scope is TypeScript/JavaScript. Python should remain
experimental until a focused FastAPI, pytest, SQLAlchemy, and Pydantic subset is
designed and accepted.

## Code-unit extraction

Extraction identifies functions, classes, modules, tests, and framework-specific
units. TypeScript and JavaScript are the first language targets.

## Normalization and fingerprinting

Normalization will remove incidental syntax differences that are not relevant to
pattern-family identity. Fingerprints will provide cheap candidate grouping
before expensive structural alignment.

## Candidate discovery

Candidate discovery will find possible analogues without claiming family
membership. Semantic compatibility filtering must run before family membership
is claimed.

## Alignment, anti-unification, and clustering

Structural alignment compares candidates. Anti-unification derives shared
templates and variation slots. Clustering groups aligned candidates into
families. These algorithms are deliberately deferred.

## Framework adapters

Initial framework adapters are scoped to Express, NestJS, React, Jest, and
Vitest. Framework rules belong in `src/rust/adapters/frameworks/`.

## Classification

Classification must produce dominant pattern, variation, exception, or unknown
with evidence and freshness checks.

## Sync and freshness

The v0.1 indexing model is manual: `init`, `index`, `sync`, freshness warnings
in `status`, and freshness checks before query or MCP claims. A daemon or
watcher is optional and must not be required for correctness.

If a future watcher is implemented, it should reparse changed units, mark
affected families stale, and lazily recompute on query. It should not eagerly
recompute the whole repository by default.
