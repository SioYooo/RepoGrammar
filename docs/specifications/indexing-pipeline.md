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
Python `.py` discovery slice, syntax-only code-unit extractors, and
`index`/`sync`/`resync` wiring. The current CLI can discover TS/JS/Python/Java/Rust
files, read source through a hash-checked repo-relative boundary, store
repo-relative file metadata and structural code units in a building generation
inside the mutable `.repogrammar/repogrammar.sqlite` database, validate that
generation, and mark the corresponding `index_generations` row active. `index`
and `resync` do this as full rebuilds; `sync` attempts path-level incremental
copy-forward from the readable active generation and falls back to the full
rebuild path when project context, worker, schema, layout, or dirty-state
preconditions are unsafe. The
current default indexing path also stores syntax-origin `FRAMEWORK_ROLE`
semantic fact records for recognized TS/JS and Python framework-shaped code
units. These records use `FRAMEWORK_HEURISTIC` certainty and same-generation
code-unit evidence; they are candidate grouping facts, not family evidence by
themselves.
The current CLI can also discover `.rs` files and `Cargo.toml` manifests for
RepoGrammar self-dogfooding. Rust parsing uses Tree-sitter Rust to extract
structural modules, use items, structs, enums, traits, impl blocks, functions,
methods, test functions, and macro syntax. Cargo manifests are bounded
structural inventory only. The default safe Rust project-model substage can run
`cargo metadata --format-version=1 --no-deps` after `Cargo.toml` code units are
stored for the building generation, and it records only owned `PROJECT_CONFIG`
facts or recoverable provider `UNKNOWN`s. It never executes build scripts, proc
macros, rustc, rust-analyzer, rustdoc, or project code, and it cannot prove
symbol/type/call semantics or family support. Root `Cargo.toml` build-variant
ambiguity can block repository-wide Rust self-dogfood family support, but
nested fixture/package manifests must not globally block unrelated root Rust
families.
When `REPOGRAMMAR_TYPESCRIPT_WORKER` names an explicit worker executable,
`index`, `resync`, and full-rebuild `sync` fallback can also ask that worker for
facts about the discovered repo-relative TS/JS file set. The request includes
bounded operation scopes for literal module specifiers, exports, re-exports, and
package entries. Optional worker arguments come from
`REPOGRAMMAR_TYPESCRIPT_WORKER_ARGS_JSON` as a JSON array of strings, not a
shell command line. Accepted facts are recorded only when they match the same
building generation's indexed file, code-unit id, content hash, byte range, and
requested operation provenance. Incremental `sync` falls back to a full rebuild
when an explicit worker is configured.

Outside the internal Rust self-dogfood extractor and explicitly configured
semantic workers, this slice does not use Tree-sitter, call a TypeScript
compiler, perform full multi-view alignment, or anti-unify templates. It does
include
a conservative EC-MVFI-lite family builder that groups by language,
code-unit kind, framework role, and normalized shape, then applies a bounded
role-specific complete-link clustering pass over internal support-family
feature vectors so bridge members cannot single-link incompatible Python or
TS/JS evidence into one claim. It writes family rows only when each supporting
member also has compatible same-generation `SEMANTIC` or `DATAFLOW_DERIVED`
non-framework evidence. The current
Python path can synthesize `DATAFLOW_DERIVED` support facts in the application
layer from exact CPython structural anchors plus a single syntax-origin Python
framework role; raw parser facts and framework heuristics remain insufficient by
themselves.
A parallel internal Rust self-dogfood path can synthesize
`DATAFLOW_DERIVED` support facts from Tree-sitter Rust structural anchors only
for RepoGrammar-owned roles such as family gates, indexing phases, parser
adapters, CLI/MCP handlers, installer actions, storage validation, source-span
renderers, and product tests. Rust families require sufficient compatible
support, complete-link-compatible structural features, fresh same-generation
evidence, and no claim-relevant Rust `UNKNOWN`. Structural anchors alone,
generic Rust functions, unresolved module links, build-variant ambiguity,
macro/proc-macro expansion, trait-object dispatch, and stale evidence must not
be upgraded into confident family claims.
The Rust parser project context includes discovered Rust module paths and
bounded `Cargo.toml` file contents. External `mod` declarations can use that
module inventory to produce structural module-resolution context or typed
`UNKNOWN`s. Source-level `#[cfg]` / `#[cfg_attr]` build-variant UNKNOWNs can
use the nearest Cargo manifest to record simple feature predicates and
declared/undeclared feature state as assumptions. This is only
`cargo_feature_cfg_model` triage: RepoGrammar still does not evaluate cfgs,
select targets/features, or treat Cargo feature metadata as family support.
Family/query recovery text may summarize that feature state for agents without
changing the blocking UNKNOWN classification.
A parallel, deliberately conservative TS/JS path exists for Express,
Jest/Vitest, Next.js, Fastify, Prisma, and Drizzle exact anchors. The syntax
parser emits `STRUCTURAL` exact-anchor facts only when local framework-specific
bindings and file conventions match the adapter registry. Exact bindings may
come from ES imports, CommonJS `require`, or CommonJS destructuring aliases from
the exact supported package, but not from custom wrappers or injected clients:
Express app/router calls, Jest/Vitest runners, Next App/Pages conventions with
`next` package context, Fastify factory receivers plus shorthand routes or full
`app.route` declarations with literal method, literal `url`/`path`, and an
exact `handler` field, local `new PrismaClient()` clients, and Drizzle
table/db/query bindings. Exact local
Next dynamic segments, route
groups, and parallel routes are retained as context assumptions on page/layout
and route-handler anchors; middleware, server actions, re-exports, and
server/client semantics remain unsupported. Dynamic receivers, custom wrappers,
dynamic methods, conditional imports, Fastify plugin prefixes, Fastify full
routes without literal `url`/`path` or handler fields, Prisma
bulk/raw/injected clients, and Drizzle raw/dynamic builders emit typed
`UNKNOWN` for the affected claim instead of support. The application layer
promotes accepted anchors to `DATAFLOW_DERIVED` support facts with engine
`repogrammar-tsjs-derived` and method `bounded_exact_anchor_v1`, carrying
`provider_resolved=false`, `derived_from=tsjs_structural_anchors`, a
framework-specific `derived_from=tsjs_<framework>_structural_anchors`,
`framework_role=<role>`, and `tsjs_anchor_kind=<kind>` assumptions. The
family-support gate accepts only exact recognized targets (for example
`express.route.get`, `jest_vitest.describe`, `next.route.GET`,
`fastify.route.route`, `prisma.query.findMany`, `drizzle.query.select`,
`drizzle.query.query_findMany`, or `drizzle.query.query_findFirst`) under the
`repogrammar-tsjs-derived` safe origin; it must not infer support from package
metadata, TypeScript semantic-worker facts, or fact text that merely contains a
framework name. TS/JS families require at least three compatible exact-anchor
support facts and use complete-link compatibility over route, runner,
component, response, query, and schema profiles. Bounded project inventory for
`package.json`,
`tsconfig.json`, `jsconfig.json`, and Jest/Vitest config files is structural
context only. Package dependencies and JSON `jest.config.json` /
`vitest.config.json` files can provide ambient test-runner context; script
configs such as `jest.config.ts` or `vitest.config.js` are not executed and
remain metadata/typed `UNKNOWN` only. The parser project context also builds a
bounded repo-local TS/JS module inventory plus safe JSON `paths` aliases and
`rootDirs` from `tsconfig.json`/`jsconfig.json`; safe `baseUrl` prefixes are
applied conservatively to alias targets before resolving against discovered
repo files. It can persist `STRUCTURAL` `RESOLVED_IMPORT` facts only for unique
literal relative imports, unique path-alias imports, or unique rootDirs
relative imports that resolve to discovered repo files. Direct repo-local
relative resolution wins before rootDirs fallback. Dynamic
`import(...)`, non-literal `require(...)`, conditional `require(...)`,
unresolved aliases/imports/rootDirs targets, conflicting alias or rootDirs
candidates, and `export *` stay typed `UNKNOWN`; those facts are
context/abstention evidence and do not become family support. Ambient
Jest/Vitest globals require bounded project test-runner
context from package or config inventory; otherwise they emit
`MissingProjectConfig` instead of support. React components and hooks remain
`UNKNOWN` in this slice, including when an external TypeScript semantic worker
emits React-shaped semantic support. TypeScript worker facts are stored as
bounded semantic context only unless a later ADR defines a role-specific support
promotion path. Default worker operation planning now includes module
specifier operations for literal import/export/require facts and re-export
operations for bounded `export * from "<specifier>"` UNKNOWNs, tagging the
specifier as `<specifier>#*` for operation provenance. Provider-resolved
TypeScript compiler facts may suppress a matching parser-origin import/export
`UNKNOWN` in the aggregate UNKNOWN inventory only when the same
path/hash/code-unit/range and operation are proven; the parser fact is not
rewritten into family support. If the checked-in
worker cannot load a TypeScript compiler API, its dependency-free static
fallback facts remain `STRUCTURAL` or `UNKNOWN` and carry
`provider_resolved=false`. This is a token-saving foundation, not full TS/JS
semantic analysis, and TS/JS remains a transitional substrate rather than the
official v0.1 target. A parallel Java/Spring preview path exists for
source-visible
Spring structural anchors. Discovery includes `.java` files. The Tree-sitter
Java adapter emits Java module/class/interface/method units, Spring MVC route
units, Spring component units, Spring Boot application units, Spring Data
repository units, structural anchors, and typed `UNKNOWN`s. Exact support is
limited to Spring annotations or repository types that are fully qualified in
source or imported from the expected Spring package: Spring MVC mapping
annotations, `@Controller`/`@RestController`, stereotype annotations,
`@SpringBootApplication`, `JpaRepository`, and `@RepositoryDefinition`. Simple
lookalike annotations without exact imports, custom composed annotations,
nonliteral route paths, route mappings outside exact controller classes,
missing project/classpath context, dependency injection, component scan, AOP
proxy behavior, repository factories, Maven/Gradle metadata, javac, and
annotation processors remain typed `UNKNOWN` or non-supporting context. The
parser emits non-blocking runtime subclaim `UNKNOWN`s for exact Spring
component scans, dependency injection, proxy semantics, nonliteral route paths,
and generated Spring Data repositories, while classpath and build-tool effects
remain unsupported context until a clean project/module-level representation
exists. Route path shape is conservative: only direct string literals and pure
literal arrays are `literal`; constants, identifiers, concatenations, and mixed
literal/nonliteral arrays are nonliteral route-path `UNKNOWN`s. The application
layer promotes accepted Java anchors to `DATAFLOW_DERIVED` support facts with
engine `repogrammar-java-derived` and method
`bounded_tree_sitter_java_anchor_v1`, carrying `provider_resolved=false`,
`derived_from=tree_sitter_java_structural_anchors`, the role-compatible support
family, and `framework_role=<role>`. Spring MVC route-family compatibility
requires matching `http_method` and `route_path_shape` evidence at minimum, so
GET and POST `@RequestMapping` handlers do not cluster merely because the
annotation name matches. Java/Spring families require at least three
complete-link-compatible support facts and no claim-relevant blocking Java
`UNKNOWN`; raw parser `STRUCTURAL` facts and `FRAMEWORK_HEURISTIC` role facts
are insufficient by themselves.
The future provider-backed path is tracked in
`docs/plans/rust-tsjs-semantic-analysis-plan.md` and must use owned TS/JS
provider facts before widening these claims.
The syntax-only parser emits a lightweight RepoGrammar-owned IR consisting of one
node per code unit and conservative `contains` edges from module-like units to
contained units and classes/impls/traits to methods. Module-like units include
TS/JS modules, Rust file modules, and Rust inline modules. That IR is structural
only: it has empty payloads, does not infer calls or dataflow, and cannot prove
semantic or family claims.
Stored semantic facts, whether syntax-origin framework-role facts or explicitly
configured worker facts accepted by the storage gate, must still pass the claim
builder's support and compatibility rules before they become family evidence.
Compatibility is role-specific: unrelated semantic facts cannot satisfy
framework-family support just because they share a code-unit id, path, content
hash, and byte range.
Syntax-only code units are structural candidates, not semantic or family claims.
The `files` and `units` commands may read active file-manifest-only or
syntax-only index metadata for inventory/debugging, but that read path is not
family-query execution. The application layer can also load an internal
active-generation claim-input snapshot for claim builders after
revalidating files, code units, IR nodes/edges, semantic fact tokens,
assumptions, repo-relative evidence, hashes, and byte ranges. That internal read
path exposes only family-level CLI output; raw semantic facts remain internal.

## File discovery and exclusions

File discovery must respect repository ignore rules and RepoGrammar state
boundaries before parsing begins. RepoGrammar must skip `.repogrammar/` and
`.repogrammar-*` unconditionally, even when `REPOGRAMMAR_DIR` changes the active
state directory.

Discovery must honor `.gitignore` rules when Git is available and use a safe
warning fallback when Git checks are unavailable. When the project path is a
subdirectory of a parent Git worktree, discovery must resolve the Git top-level
and check ignore rules using Git-root-relative paths while still reporting
project-relative paths. `REPOGRAMMAR_STRICT_GITIGNORE=true` makes unavailable
Git ignore checks a hard indexing/discovery error instead of the normal warning
fallback. It must apply default exclusions for dependency, build, cache,
coverage, virtual environment, and generated output directories. Files larger
than the configured size limit are skipped, with 1 MB as the default inclusive
limit.

The current discovery substrate supports `.ts`, `.tsx`, `.js`, `.jsx`, `.py`,
`.java`, Python `pyproject.toml`, and bounded TS/JS project-config files such as
`package.json`, `tsconfig.json`, `jsconfig.json`, `jest.config.*`, and
`vitest.config.*`.
Module-specific extensions such as `.mjs`, `.cjs`, `.mts`, and `.cts` remain
deferred until language-mode policy is defined. Discovery reports contain
repository-relative paths, language classification, strict
`sha256:<64 hex>` content hashes, file sizes, skip reasons, Git ignore status,
and warnings. They must not contain source snippets or absolute paths.
Repository-relative paths are lexical, slash-separated, non-empty paths; they
must reject absolute paths, Windows drive prefixes, backslashes, URI-like text,
control characters, `.`/`..` traversal segments, and empty path segments.

Skip reasons include RepoGrammar state directories, default excluded
directories, unsupported extensions, Git-ignored files, oversized files,
symlinks that are not followed, symlink escapes, paths outside the repository,
non-UTF-8 paths, and unreadable entries. Output ordering must be deterministic
by repository-relative path.

Optional `repogrammar.json` may configure language enablement, custom file
extensions, include/exclude patterns, framework adapters, and family thresholds.
Malformed configuration must warn and fall back to safe defaults rather than
failing indexing.

Python discovery currently discovers `.py` files and skips common Python
virtual-environment, cache, build, and dependency directories such as `.venv`,
`venv`, `env`, `.tox`, `.nox`, `__pycache__`, `.pytest_cache`,
`.mypy_cache`, `.ruff_cache`, `build`, `dist`, and `site-packages` without
executing repository code. Package-root discovery and provider-backed
project-configuration semantics remain deferred. The
implemented Python frontend uses CPython `ast` for code-unit extraction,
CPython `symtable` for structural scope anchors, and a private standard-library
`tomllib` parser mode for sanitized `pyproject.toml` summaries that default
indexing persists only as structural config context or typed config `UNKNOWN`.
The frontend is invoked through `REPOGRAMMAR_PYTHON_EXECUTABLE` when that
environment variable is non-blank. Otherwise it defaults to `python` on Windows
and `python3` on non-Windows platforms so Conda and other Windows Python
installations are not shadowed by the Microsoft Store `python3.exe` app
execution alias. `REPOGRAMMAR_PYTHON_WORKER` may still override only the worker
script path.
Default parser-mode indexing now passes discovered repo-relative `.py`
inventory and sanitized root `pyproject.toml` source roots from the existing
`tomllib` project-config parse report into private parse-document requests, so
source-tied unique repo-local imports can be persisted as `STRUCTURAL` parser
facts and ambiguous/missing imports can remain typed `UNKNOWN`s. Future slices
should add Tree-sitter as a tolerant structural fallback only. Python
syntax-only facts still cannot become semantic claims or family evidence by
themselves; only separately synthesized exact-anchor `DATAFLOW_DERIVED` support
facts or future provider-backed facts may enter the EC-MVFI-lite support gate.
Git ignore regression coverage includes ignored Python files in both repository
roots and parent-worktree subdirectory projects.

## Tree-sitter parsing

Tree-sitter may be used in parsing and language adapters. AST nodes must be
converted into `CodeUnit` and unified IR types before entering `core`.

The current implementation uses dependency-free syntax-only extractor adapters
as bootstrap parser boundaries. The TS/JS extractor recognizes modules,
functions, assigned arrow functions, classes, methods, React function
components, custom hooks, Express route calls, Next.js route/page conventions,
Fastify routes, Prisma queries/transactions, Drizzle schema/query/transaction
anchors, and Jest/Vitest
`describe`/`it`/`test` blocks by structural syntax only. The Python extractor
uses a checked-in CPython `ast` worker and recognizes modules, functions,
async functions, classes, methods, FastAPI route-shaped functions, pytest tests
and fixtures, Pydantic model-shaped classes, SQLAlchemy model-shaped classes,
and SQLAlchemy repository method-shaped functions. Both extractors preserve
byte ranges, return diagnostics for parser errors, and store RepoGrammar-owned
`CodeUnit` metadata plus CodeUnit-derived IR nodes and conservative containment
edges. The Rust self-dogfood extractor uses Tree-sitter Rust for tolerant
structural extraction and typed UNKNOWN generation. No Tree-sitter node type is
stored in core, persistence, CLI, or MCP output.

Tree-sitter provides tolerant syntax and candidate generation. It is not
responsible for complete symbol, type, overload, alias, or module-resolution
facts.

Tree-sitter facts are structural evidence. They can participate in framework
role detection and candidate ranking, but they cannot independently prove
function identity, call targets, framework role semantics, type compatibility,
dependency-injection bindings, transaction semantics, authorization semantics,
or test fixture binding.

## Language-native semantic frontends

Language-native frontends provide project models, module resolution, symbol
resolution, type information, inheritance, and resolved calls where available.
The next official semantic-frontend design target is Python. Python analysis
should follow the claim-driven selective cascade in
`docs/specifications/python-analysis.md` and
`docs/decisions/ADR-0012-python-selective-analysis-cascade.md`: cheap CPython
syntax/scope/config facts first, Pyrefly only for plausible family candidates,
selective Pyright cross-checks for claim-upgrading facts, bounded role
propagation and call recovery, and compact evidence selection under token
budget.

The existing Rust-side TypeScript process adapter can validate NDJSON worker
output and translate facts into RepoGrammar-owned semantic facts. The
syntax-only `index` and `sync` path does not launch that worker by default. With
`REPOGRAMMAR_TYPESCRIPT_WORKER`, it may launch an explicit worker executable
with optional argv from `REPOGRAMMAR_TYPESCRIPT_WORKER_ARGS_JSON` and store
accepted facts only after evidence matches an indexed manifest entry, code-unit
id, content hash, and byte range in the same building generation.
Worker fallback statuses keep the generation syntax-only, while storage-gate
conflicts abort the new generation. Active-generation semantic facts can be read
back only through the storage/query claim-input snapshot for future claim
construction. The current query application boundary has an internal file-hash
freshness and readiness gate for snapshot semantic facts: stale or missing
source blocks the affected future claim input as `StaleEvidence`, unsupported
fact kinds or weak certainty block as `InsufficientSupport`, and conflicting
certainty blocks as `ConflictingFacts`. Raw semantic facts are not exposed
through CLI/MCP. The storage layer can persist generation-scoped family records,
members, variation slots, and family-bound evidence when the EC-MVFI-lite
builder supplies them. Default syntax-origin framework-role facts do not produce
those rows; an explicit semantic worker or future framework adapter must supply
stronger compatible evidence before a family is stored. Other languages should
use their own compiler, type-checker, or LSP where that is the most
authoritative source.
Not every stored semantic fact is worker-originated: syntax-origin framework
role facts may be recorded by the current TS/JS framework adapter with
`FRAMEWORK_HEURISTIC` certainty. Those facts remain blocked from family-claim
input as insufficient support unless the current claim builder can combine them
with stronger compatible evidence. The TS/JS syntax parser additionally emits
`STRUCTURAL` exact-anchor facts (engine `repogrammar-tsjs-syntax`, method
`exact_anchor_v1`) that, like the Python structural anchors, cannot support
membership by themselves; only the application-layer `repogrammar-tsjs-derived`
promotion can turn them into `DATAFLOW_DERIVED` support. It also emits typed
TS/JS `UNKNOWN` facts for dynamic route calls, unresolved or unsafe Express or
Fastify receivers, unsafe or unresolved Jest/Vitest runner bindings, unsupported
Next middleware/server action/re-export semantics, Prisma bulk, dynamic, raw,
or injected clients, Drizzle dynamic/raw or unresolved table/db bindings, and
bounded config parse/execution ambiguity. The
bounded import resolver additionally emits
structural repo-local import facts or typed `UNKNOWN` records for dynamic
imports, conditional or non-literal `require`, unresolved/conflicting
path-alias resolution, and ambiguous star re-exports. These facts remain blocked
from support and may only affect claim abstention, compatibility, or read-plan
guidance.

The checked-in Python worker currently has two narrow modes. Its private
parse-document mode is used by the Rust parser adapter to get CPython
`ast`-derived code-unit metadata without hand-written Python parsing. Default
indexing now passes the discovered repo-relative `.py` inventory, bounded
module file texts, sanitized root `pyproject.toml` source roots from the same
parser/tomllib project-config path, and bounded, hash-checked discovered
`conftest.py` file contents into that private mode, letting the worker build a
bounded module, direct-symbol, package re-export, safe literal-star-import, and
fixture context for the current parse request. That worker pass produces
repo-relative structural fact payloads for ordinary imports, decorator anchors,
class bases,
Pydantic model-member anchors for fields, field annotation targets,
`model_config`, nested `Config`, `computed_field`, validator, and
`model_validator` declarations, SQLAlchemy mapped field and relationship
anchors, typed SQLAlchemy session calls including `add`, `execute`, `scalar`,
`scalars`, `commit`, and `rollback`, bounded
`__init__`-assigned `self.session`/`self.db` receiver propagation with
same-method reassignment invalidation, simple calls, bounded same-function
FastAPI service-call context anchors,
`pytest.test` test-function anchors, graph-derived unique repo-local import
bindings, graph-derived direct imported `SYMBOL`/`TYPE` facts for top-level
class/function/module symbols, static `__init__.py` re-exports, literal-`__all__`
star imports, same-file pytest test and fixture dependency edges, unique
parent-directory `conftest.py` pytest fixture edges, literal pytest fixture
`name=` aliases, literal `request.getfixturevalue("name")` fixture lookups,
known pytest built-in fixture context,
FastAPI static
`response_model=...` schema-slot anchors, static `Depends(get_db)`
dependency-target anchors, literal `HTTPException(status_code=...)`
status-code effect anchors, static FastAPI `Body`/`Path`/`Query`/`Header`/
`Cookie` request-shape anchors, path-derived module names, CPython `symtable`
scope anchors, and typed dynamic/unresolved decorator, dynamic call, monkey-patch,
dynamic/unresolved/ambiguous import, unsafe star import without literal
`__all__`, dynamic Pydantic model factory, dynamic pytest fixture name,
nonliteral `request.getfixturevalue`, duplicate conftest fixture, plugin fixture,
and fixture-injection `UNKNOWN` cases. The semantic-worker-compatible
project mode can also resolve requested-project `conftest.py` fixture names
through pytest's directory hierarchy as graph-derived fixture-edge facts. The
Rust parser adapter validates and persists parse-document payloads as internal
`STRUCTURAL`, approved parser-origin `DATAFLOW_DERIVED`, or `UNKNOWN` semantic
fact records tied to the same code-unit evidence. Parser-origin graph facts must
carry `provider_resolved=false` and a `derived_from=repo_local_python_import_graph`
or `derived_from=repo_local_pytest_fixture_graph` assumption; unlabeled parser
facts remain blocked from support readiness as insufficient support. They may
enter the family builder only as context features or claim-scoped blocking
`UNKNOWN`s unless separately synthesized framework support facts pass the exact
compatibility table. Pydantic member/config/computed anchors are
schema/config/member context only, and FastAPI service-call anchors are
handler/service context only. FastAPI request body and request-parameter
anchors are route-shape context only; none of these categories synthesizes
family support facts.
Its private `parse_project_config` mode can sanitize `pyproject.toml` summaries
with `tomllib` when available. Default indexing now discovers root
`pyproject.toml` as `python-config`, reads it through the Rust source-store
path/hash boundary, and persists a `project_config` code unit plus sanitized
`PROJECT_CONFIG`/`STRUCTURAL` records or typed config `UNKNOWN`s. Those records
are structural context only, are not provider facts, do not participate in
family membership support, and stay blocked from claim-input readiness. The
worker's
semantic-worker-compatible NDJSON mode can emit those structural facts plus
project-scope module-level repo-local import facts for unique safe `.py` module
matches, typed `UNKNOWN` for ambiguous/missing repo-local imports and `sys.path`
mutation, and conservative `FRAMEWORK_ROLE`/`FRAMEWORK_HEURISTIC` facts for
Python framework-shaped units. The product indexing path does not launch a
Python semantic worker separately. Pyrefly, Pyright, provider-backed usage
propagation, cross-function call hierarchy recovery, and runtime observation
remain deferred beyond the current same-function structural service-call
context anchors.

The official v0.1 language scope is Python-first, focused on FastAPI, pytest,
SQLAlchemy, and Pydantic. The existing TypeScript/JavaScript path remains
transitional substrate until a later ADR re-promotes it.
The current Rust ports layer also defines future Python, Rust, and TS/JS
semantic-provider boundaries for candidate-scoped requests, provider provenance
assumptions, cache-key dimensions, and recoverable provider-unavailable
`UNKNOWN`s. Default indexing does not call a provider adapter. The application
layer now includes
an internal planner for validated Pyrefly `ResolveFrameworkIdentity` request
scopes over plausible Python family candidate groups. It skips parser-origin
blocking `UNKNOWN`s that affect Python framework identity, import resolution,
or pytest fixture binding for the planned claim, and it can read the same
validated active-generation claim-input snapshot used by query/family code.
It does not execute those requests, persist provider facts, or expose them
through CLI/MCP.
No Pyrefly, Pyright, RightTyper, or runtime-trace adapter is implemented.
The Rust Cargo metadata provider adapter is wired into the default product
indexing path as a safe project-model refresh stage for repositories with
same-generation `Cargo.toml` code units. It parses
`cargo metadata --format-version=1 --no-deps` output into owned
`PROJECT_CONFIG` facts, records provider `UNKNOWN`s when Cargo or project
configuration is unavailable, and does not execute build scripts or procedural
macros. These facts are context only: package metadata, targets, features, and
dependencies do not directly prove family membership.

## Optional providers

Optional providers such as a future CodeGraph provider may enrich candidate
retrieval, call/dependency context, or graph-neighborhood views after their
facts are translated into RepoGrammar-owned evidence. Provider facts must carry
provider provenance and freshness metadata. They cannot independently prove
pattern-family membership and must not be required for default indexing.

Unavailable, stale, or conflicting provider facts become auxiliary diagnostics,
typed `UNKNOWN`, or abstention for the affected claim.

## Code-unit extraction

Extraction identifies functions, classes, modules, tests, and framework-specific
units. Python v0.1 extraction should cover modules, functions, async functions,
classes, methods, decorators, imports, assignments, calls, annotations, class
bases, FastAPI route/dependency roles, pytest tests/fixtures, Pydantic models,
and SQLAlchemy model/session roles. Current stored TS/JS unit kinds are
syntax-only and include module, function, arrow function, class, method, React
component, React hook, Express route, Next.js App/Pages route/page/layout units,
Fastify route, Prisma query/transaction, Drizzle schema/query/transaction, test
suite, and test case.

## Normalization and fingerprinting

Normalization will remove incidental syntax differences that are not relevant to
pattern-family identity. Fingerprints will provide cheap candidate grouping
before expensive structural alignment.

## Candidate discovery

Candidate discovery will find possible analogues without claiming family
membership. Semantic compatibility filtering must run before family membership
is claimed.

The v0.1 mining design is Evidence-Constrained Multi-View Family Induction
(EC-MVFI). Tree-sitter syntax, language-native semantic facts, framework roles,
CFG/dataflow/effect views, API usage, and repository context are separate views.
Weak agreement may rank candidates, but family claims require compatible
source-backed evidence; unresolved or conflicting facts remain `UNKNOWN`.

## Alignment, anti-unification, and clustering

Structural alignment compares candidates. Anti-unification derives shared
templates and variation slots. Clustering groups aligned candidates into
families. These algorithms are deliberately deferred.

## Representative evidence selection

Representative selection is implemented only for query rendering over already
stored family evidence metadata. The selector uses deterministic greedy
marginal coverage per estimated token cost. Matched family queries also build
a read plan from stored family evidence. Read-plan items carry repo-relative
paths, strict content hashes, byte ranges, purpose labels, and estimated token
cost; they do not contain absolute paths. Source text remains disabled by
default. Before returning metadata-only output, the query layer attempts
hash-checked line-range enrichment for read-plan items. Fresh sources should
produce `start_line` and `end_line` without returning source text; stale,
missing, hash-mismatched, too-large, non-UTF-8, unavailable, or invalid ranges
must keep the item and add omission guidance. When the caller explicitly
requests source spans, the query layer renders only selected read-plan spans
through the hash-checked source-store boundary, fills line ranges for rendered
spans, and omits stale or unsupported spans with fallback guidance.
Family evidence records carry schema-backed `covered_claims` labels from the
allowlist `canonical`, `support`, `variation`, and `exception`; the current
builder emits `canonical` and `support`, plus a narrow Python `variation`
label when a ready family's exact-compatible framework-anchor support targets
differ. The builder may also emit metadata-only variation slots when
parser-context profiles differ inside an already-supported Python family, but
those slots do not imply variation evidence coverage. Requested exception
coverage and broader variation coverage are reported as missing until family
evidence is explicitly linked to variation slots or counterexamples. This
selector does not replace future medoid
selection, template induction, or exception mining.

## Framework adapters

Initial Python v0.1 framework adapters are scoped to FastAPI, pytest,
SQLAlchemy, and Pydantic. Framework rules belong in
`src/rust/adapters/frameworks/`.
The current framework adapter maps CPython AST-origin code-unit kinds for
FastAPI routes, pytest tests/fixtures, Pydantic models, SQLAlchemy models, and
SQLAlchemy repository methods into syntax-origin `FRAMEWORK_ROLE` fact records.
Python framework compatibility must use typed canonical identities and explicit
compatibility tables, never framework-name substring matching. The current
EC-MVFI-lite gate requires at least three Python members plus stronger
compatible same-generation semantic/dataflow support before storing a Python
family; framework heuristics alone stay `UNKNOWN`.

The current lightweight TS/JS adapter maps syntax-only code-unit kinds for
Express routes, React components, React hooks, Jest/Vitest suites/tests,
Next.js conventions, Fastify routes, Prisma queries/transactions, and Drizzle
schema/query/transaction anchors into syntax-origin `FRAMEWORK_ROLE` fact
records. It records
repo-relative evidence and unresolved-binding assumptions. For the conservative
v0.2 path, the parser also emits structural exact-anchor facts for those
framework adapters; the application layer may derive `DATAFLOW_DERIVED` support
from those exact anchors. It still does not perform TypeScript compiler-backed
binding/export propagation, React runtime behavior, Next server/client or
middleware semantics, Fastify plugin-prefix resolution, Prisma/Drizzle runtime
extensions, dependency injection, or lifecycle semantics. Exact local Next
dynamic segments, route groups, and parallel routes remain context assumptions
on accepted file-convention anchors rather than blocking those anchors by
themselves.

The current Rust adapter maps only RepoGrammar's own repository structure into
internal self-dogfood roles. It records structural anchors and typed UNKNOWNs
for signature shape, visibility, arity, return kind, attributes, test shape,
bounded Cargo dependency inventory, safe repo-relative module declarations,
Cargo/build variants, unresolved or conflicting external modules,
macro/proc-macro expansion, and trait-object dispatch. Those facts are bounded
evidence for RepoGrammar self-dogfood only; they are not provider-backed Rust
semantics and do not imply general Rust target-language support. Cargo build
scripts and target-specific sections in the root manifest are repository
build-variant UNKNOWNs that block affected Rust self-dogfood family claims until
resolved; nested fixture/package manifests remain package/claim scoped and must
not globally block unrelated root Rust families. The indexer records manifests
without executing Cargo or build scripts.

## Classification

Classification must produce dominant pattern, variation, exception, or unknown
with evidence and freshness checks. The current EC-MVFI-lite implementation can
produce `DOMINANT_PATTERN` only for repeated compatible candidates backed by
strong semantic/dataflow support; otherwise query output remains typed
`UNKNOWN`.
FastAPI route decorator targets can become derived support only when they
exact-match the canonical route-method table:
`fastapi.FastAPI.{delete,get,head,options,patch,post,put}` and
`fastapi.APIRouter.{delete,get,head,options,patch,post,put}`. Generic
`api_route` and WebSocket decorators are not v0.1 support targets. Static
`response_model=...`, static `Depends(get_db)` dependency-target, `Depends(...)`,
and `HTTPException(...)` parser anchors, including literal status-code effect
anchors, plus static FastAPI request body and request-parameter anchors, stay
schema/context/effect metadata and do not prove membership support. Dynamic
FastAPI dependency target expressions become
`RuntimeDependencyInjection` `UNKNOWN` for `fastapi_dependency_target`; they do
not erase route-shape evidence and do not become family support. Canonical
`pytest.mark.parametrize` decorator anchors can support pytest test families,
but `pytest.parametrize.<name>` argument anchors remain context metadata and do
not prove support. Known pytest built-in fixture context targets such as
`pytest.builtin_fixture.tmp_path`, dynamic fixture-name UNKNOWNs, duplicate
conftest fixture UNKNOWNs, and plugin-style fixture UNKNOWNs remain
context/abstention metadata and do not prove support. Pydantic field,
field-type, `model_config`, nested `Config`,
computed-field, field-validator, legacy validator, and model-validator anchors
likewise stay model schema/config/member metadata and do not prove membership
support; dynamic Pydantic `create_model(...)` factories remain typed UNKNOWNs
instead of static model support. FastAPI service-call anchors stay
handler/service context metadata and also do not prove membership support.

`UNKNOWN` classifications and sub-claim unknowns must use the taxonomy in
`docs/specifications/unknowns.md`. Unknowns caused by dynamic imports, monkey
patching, pytest fixture injection, runtime dependency injection, macro or
preprocessor ambiguity, stale evidence, conflicts, or insufficient support must
remain visible to query and MCP callers.

## Sync and freshness

The baseline indexing model remains explicit: `init`, `index`, `sync`,
freshness warnings in `status`, and freshness checks before query or MCP
claims. Optional repository-local auto-sync can be enabled with
`repogrammar autosync start`. Auto-sync is not required for correctness, is not
started by MCP serving or agent installation, and does not scan repositories
that have not explicitly initialized RepoGrammar state.

The current auto-sync worker is conservative and reuses the normal `sync` path
after detecting a changed lightweight supported-file metadata fingerprint and
debouncing file changes. The detector avoids reading source contents during
idle polling; the subsequent `sync` remains authoritative for content hashes,
Git-ignore enforcement, parser/provider facts, incremental-versus-fallback
decisions, freshness, and active-generation activation. Incremental `sync`
copy-forwards unchanged active records into a new building generation, reparses
added or modified paths, omits removed paths, and recomputes local derived
support and families before validation. Lazy query-time recomputation remains
future work.
