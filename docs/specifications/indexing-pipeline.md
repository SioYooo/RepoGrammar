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
placeholders, a safe repo-local lifecycle, a TS/JS file discovery substrate, a
dependency-free syntax-only code-unit extractor, and `index`/`sync` wiring. The
current CLI can discover TS/JS files, read source through a hash-checked
repo-relative boundary, store repo-relative file metadata and structural code
units in a generation-scoped SQLite database, validate that generation, and
activate `.repogrammar/current-generation`.

This slice does not use Tree-sitter, call a TypeScript compiler, build unified
IR, align structures, anti-unify templates, cluster families, persist family
evidence, or answer query commands from stored evidence. Syntax-only code units
are structural candidates, not semantic or family claims.

## File discovery and exclusions

File discovery must respect repository ignore rules and RepoGrammar state
boundaries before parsing begins. RepoGrammar must skip `.repogrammar/` and
`.repogrammar-*` unconditionally, even when `REPOGRAMMAR_DIR` changes the active
state directory.

Discovery must honor `.gitignore` rules when Git is available and use a safe
warning fallback when Git checks are unavailable. It must apply default
exclusions for dependency, build, cache, coverage, virtual environment, and
generated output directories. Files larger than the configured size limit are
skipped, with 1 MB as the default inclusive limit.

The current discovery substrate supports `.ts`, `.tsx`, `.js`, and `.jsx`.
Module-specific extensions such as `.mjs`, `.cjs`, `.mts`, and `.cts` remain
deferred until language-mode policy is defined. Discovery reports contain
repository-relative paths, language classification, strict
`sha256:<64 hex>` content hashes, file sizes, skip reasons, Git ignore status,
and warnings. They must not contain source snippets or absolute paths.

Skip reasons include RepoGrammar state directories, default excluded
directories, unsupported extensions, Git-ignored files, oversized files,
symlinks that are not followed, symlink escapes, paths outside the repository,
non-UTF-8 paths, and unreadable entries. Output ordering must be deterministic
by repository-relative path.

Optional `repogrammar.json` may configure language enablement, custom file
extensions, include/exclude patterns, framework adapters, and family thresholds.
Malformed configuration must warn and fall back to safe defaults rather than
failing indexing.

## Tree-sitter parsing

Tree-sitter will be used in parsing and language adapters. AST nodes must be
converted into `CodeUnit` and unified IR types before entering `core`.

The current implementation uses a dependency-free syntax-only extractor as a
bootstrap parser adapter. It recognizes modules, functions, assigned arrow
functions, classes, methods, React function components, custom hooks, Express
route calls, and Jest/Vitest `describe`/`it`/`test` blocks by structural syntax
only. It preserves byte ranges, returns partial units with diagnostics for
unbalanced syntax, and stores only RepoGrammar-owned `CodeUnit` metadata.
Tree-sitter integration remains planned.

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
units. TypeScript and JavaScript are the first language targets. Current stored
unit kinds are syntax-only and include module, function, arrow function, class,
method, React component, React hook, Express route, test suite, and test case.

## Normalization and fingerprinting

Normalization will remove incidental syntax differences that are not relevant to
pattern-family identity. Fingerprints will provide cheap candidate grouping
before expensive structural alignment.

## Candidate discovery

Candidate discovery will find possible analogues without claiming family
membership. Semantic compatibility filtering must run before family membership
is claimed.

The v0.1 mining design is Evidence-Constrained Multi-View Family Induction
(EC-MVFI). Tree-sitter syntax, TypeScript compiler facts, framework roles,
CFG/dataflow/effect views, API usage, and repository context are separate views.
Weak agreement may rank candidates, but family claims require compatible
source-backed evidence; unresolved or conflicting facts remain `UNKNOWN`.

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
