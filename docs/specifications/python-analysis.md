# Python Analysis Specification

- Status: Active v0.1 target specification
- Last updated: 2026-07-17
- Scope: Python-first v0.1 analysis algorithms and claim discipline
- Supersedes: `docs/plans/python-dogfooding-plan.md` for v0.1 scope

## Decision Summary

RepoGrammar v0.1 is now Python-first. The official v0.1 implementation target
is repository-local pattern-family evidence for Python projects, focused on:

- FastAPI;
- pytest;
- SQLAlchemy;
- Pydantic.

The existing TypeScript/JavaScript substrate remains useful implementation
scaffolding and may continue to be maintained, but it is no longer the official
v0.1 language target. TS/JS production claims move behind the Python v0.1
checkpoint unless a later ADR changes the sequence again.

## Algorithm Reasonability Analysis

Python static analysis cannot be "solved" by one analyzer in a way that is both
precise, complete, fast, framework-aware, and safe for arbitrary repositories.
Dynamic imports, monkey patching, runtime dependency injection, pytest fixture
injection, framework decorators, and optional dependencies make full soundness a
bad v0.1 target.

RepoGrammar also does not need a general-purpose Python analyzer. It needs
enough auditable, repo-local evidence to compress recurring implementation
families for coding agents. The right design is therefore a claim-driven
selective cascade:

```text
CPython ast + symtable + tomllib
-> low-cost structural candidate grouping
-> Pyrefly only for candidates that may become families
-> selective Pyright cross-check for claim-upgrading facts
-> bounded framework-role propagation
-> Pyrefly call hierarchy, then JARVIS-lite fallback when needed
-> optional RightTyper observed evidence behind explicit opt-in
-> precision-first constrained family induction
-> selective retrieval and token-budget evidence selection
```

This stack is reasonable because it uses mature parser and language-native
facts for stable evidence, borrows useful ideas from Python type and call-graph
research, and rejects unsupported certainty. It deliberately does not attempt a
whole-program sound Python semantic model.

Static analysis precision and token reduction are related but separate goals.
Stronger analysis usually creates more facts. RepoGrammar only forwards the
minimum evidence needed for the current family claim.

## Claim-driven Selective Cascade

RepoGrammar should not run every analyzer over every file. The cascade is:

1. Use CPython `ast`, `symtable`, and `tomllib` to produce cheap syntax, scope,
   configuration, import, decorator, class-base, and call-shape candidates.
2. Group candidates with low-cost structural and framework-anchor features.
3. Invoke Pyrefly only for candidate groups whose support and shape make a
   family plausible.
4. Invoke Pyright only for facts that would upgrade a candidate into a family
   claim or materially change an exception or variation classification.
5. Apply bounded framework-role propagation and target-centered call recovery
   only where the result affects a claim.
6. Emit typed `UNKNOWN` for unresolved, stale, conflicting, dynamic, or
   unsupported facts.
7. Select compact evidence under the query's token budget instead of returning
   a static-analysis graph.

## Adopted v0.1 Method Stack

### Current Implementation Slice

The current implementation covers a bounded static CPython `ast` slice only:

- `.py` file discovery with Python virtualenv/cache/dependency directory skips;
- CPython `ast` parse-document worker output for code-unit extraction;
- an exact private parse-document host/worker tuple of
  `protocol_version=1` and `contract_revision=1` on requests and responses.
  The current Rust host maps the new worker's low-cardinality rejection, a
  missing or different normal-response revision, or an old worker's bounded
  rejection to typed `PythonFrontendContractMismatch`. A previously published
  host cannot return a variant it never defined: its revision-free request is
  rejected safely by the new worker, but that old host can expose only a
  sanitized generic frontend/protocol failure and must be upgraded. Current-
  host recovery is to rebuild or reinstall the binary and bundled worker from
  the same release, without source, paths, environment values, or raw worker
  payloads. This tuple does not version the separate project-config mode or
  public semantic-worker-compatible mode;
- source-ordered per-name module-scope event histories and immutable AST range
  caching for bounded large-module analysis. Point-in-source framework alias
  and assignment-role queries use read-only history views instead of rescanning
  preceding statements or copying the complete binding map for every top-level
  statement. The private Rust process boundary accepts at most a 2 MiB response
  while preserving the 1 MiB request and 2,000-fact limits, drains stdout while
  the request is running, and returns a typed, source-free timeout after a
  bounded 30-second wall-clock deadline;
- CPython `ast` structural fact output for ordinary import bindings, decorator
  anchors, class bases, simple call targets, bounded same-function application
  call targets, pytest test-function anchors, alias-aware pytest fixture
  decorators, literal pytest fixture `name=` aliases, literal pytest
  parametrize argument anchors, typed dynamic import, `sys.path` mutation,
  dynamic call, dynamic decorator, unresolved bare decorator, monkey-patch,
  dynamic pytest fixture-name, and unresolved import `UNKNOWN` facts. Unique
  repo-local import bindings, direct imported top-level class/function/module
  symbols, static package `__init__.py` re-exports, literal-`__all__` star
  imports, and same-file or applicable `conftest.py` fixture edges are emitted as
  `DATAFLOW_DERIVED` graph facts with `provider_resolved=false` and an explicit
  `derived_from=repo_local_python_import_graph` or
  `derived_from=repo_local_pytest_fixture_graph` marker. Top-level import
  bindings are the only file-level alias source for exact framework anchors;
  function-local imports are not promoted to file-global aliases. Framework
  import visibility is source-position scoped: units before a top-level shadowing
  definition or assignment may still use the import alias, while later units
  cannot use the shadowed name for exact family support;
- path-derived module-name anchors and CPython `symtable` structural scope
  anchors for imported, assigned, and namespace symbols;
- a private project-config parser mode for safe root `pyproject.toml` via
  `tomllib`, `setup.cfg` via `configparser`, and `setup.py` via CPython `ast`,
  including sanitized project name, safe source roots, recognized tool sections
  where applicable, and typed config `UNKNOWN` for malformed/incomplete
  recognized setup config or unavailable TOML support;
- semantic-worker-compatible project-mode module graph construction that uses
  safe `.py` paths plus sanitized `pyproject.toml` source roots when `tomllib`
  is available, emits graph-derived `RESOLVED_IMPORT` facts only for unique
  repo-local module matches, emits graph-derived `SYMBOL`/`TYPE` facts only for
  direct imported top-level symbols or static package re-exports visible in
  bounded source context, resolves requested-project `conftest.py` fixture names
  through pytest's directory hierarchy as graph-derived fixture-edge facts only
  when the applicable name is unique, emits `ConflictingFacts` `UNKNOWN` for
  duplicate applicable conftest fixture names, emits metadata-only structural
  context for known pytest built-in fixtures such as `tmp_path` and `capsys`, and
  emits typed `UNKNOWN` for ambiguous/missing repo-local imports, unsafe star
  imports without literal `__all__`, plugin-style fixture names without an
  allowlist or provider, or `sys.path` mutation;
- default parser-mode indexing now passes the discovered repo-relative `.py`
  inventory, bounded module file texts, sanitized root source roots from
  `pyproject.toml`/`tomllib`, `setup.cfg`/`configparser`, and static
  `setup.py`/CPython-AST project-config output, and bounded,
  hash-checked discovered `conftest.py` file contents into the private CPython
  parse-document request so the same source-tied parse pass can emit unique
  repo-local import facts, direct imported symbol facts, `pytest.test` anchors,
  pytest same-file fixture dependency edges, pytest parent-directory
  `conftest.py` fixture-edge facts, known builtin-fixture context, and typed
  unresolved/ambiguous import or fixture `UNKNOWN`s without launching a separate
  Python semantic worker or executing Python/pytest code;
- default parser-mode indexing discovers exact root `pyproject.toml`,
  `setup.cfg`, and `setup.py` as `python-config`, reads them through the Rust
  source-store path/hash/size boundary, calls the private
  `parse_project_config` worker mode, and persists `project_config` code units
  plus `PROJECT_CONFIG`/`STRUCTURAL` facts for sanitized project names, safe
  source roots, and recognized tool sections where applicable. Malformed config
  and incomplete recognized setup fields become typed `UNKNOWN`;
- file-local simple alias propagation for FastAPI router/app objects, such as
  `router = APIRouter(); api = router`, with same-name top-level reassignment
  removing that name's role so stale aliases do not produce exact canonical
  anchors. Module-level dynamic import or `sys.path` mutation is also copied as
  unit-scoped typed `UNKNOWN` evidence for later family-shaped units in the same
  file, using the unit's own range so storage invariants and freshness checks
  remain local to that unit;
- framework-specific structural anchors for FastAPI route decorators,
  static `response_model=...` schema slots, `Depends(...)`, and
  static `Depends(get_db)` dependency target slots;
- module-level FastAPI/APIRouter `include_router(...)` context when the
  receiver is an exact local FastAPI/APIRouter binding, the router argument is
  a local `APIRouter()` binding or repo-local imported router symbol, and the
  optional `prefix` is absent or a literal string. These anchors carry
  only low-cardinality `route_prefix_shape` segment classes such as
  `/:literal/:param`; the literal route text does not cross the worker boundary.
  The Rust host accepts the exact seven-assumption context envelope for this
  anchor and rejects extra or malformed fields. These anchors remain context
  only and do not prove route-family membership;
- typed
  `RuntimeDependencyInjection` `UNKNOWN` facts for dynamic dependency target
  expressions such as `Depends(make_dependency())`, and typed `UNKNOWN` facts
  for dynamic include-router prefixes or unresolved/external router bindings.
  The Rust host allowlist recognizes the exact `fastapi_router_prefix` and
  `fastapi_router_binding` affected-claim tokens so these conservative worker
  outcomes remain valid typed `UNKNOWN`s instead of aborting repository
  indexing;
- FastAPI `HTTPException(...)`, literal `HTTPException(status_code=...)`
  status-code effect slots, static FastAPI request body and request-parameter
  marker slots for `Body`, `Path`,
  `Query`, `Header`, and `Cookie`, alias-aware
  pytest `fixture` decorators and `mark.parametrize` decorators plus literal
  parametrize arguments, and Pydantic model-member anchors for fields, field
  annotation targets, imported `Field(...)` metadata calls, `model_config`,
  nested `Config`, `computed_field`, validator, and `model_validator`
  declarations. These remain CPython `ast` structural facts; they do not
  become provider-backed semantic facts. Direct
  pytest parametrize arguments take precedence over same-name fixture bindings,
  indirect parametrize arguments remain typed `PytestFixtureInjection`
  `UNKNOWN`, literal `@pytest.fixture(name="...")` values replace the
  implementation function name for fixture binding, dynamic or unsafe fixture
  `name=` values become `PytestFixtureInjection` for `pytest_fixture_binding`,
  duplicate applicable conftest fixture names become `ConflictingFacts` for
  `pytest_fixture_binding`, known pytest built-in fixtures become
  `pytest.builtin_fixture.*` context anchors, distinctive fixtures from a bounded
  well-known-plugin allowlist (for example pytest-mock `mocker`, pytest-asyncio
  `event_loop`) become `pytest.plugin_fixture.*` external context anchors while
  plugin-style fixtures outside that allowlist remain `PytestFixtureInjection`
  `UNKNOWN` until a wider allowlist or provider resolves
  them, test/fixture dependency-edge, builtin-fixture, plugin-fixture, and
  parametrize-argument anchors stay out of family membership support, and
  Pydantic member/config metadata likewise does not become support. Dynamic
  Pydantic model factories such as `pydantic.create_model(...)` and dynamic
  `model_config = ConfigDict(...)` values remain typed `FrameworkMagic`
  `UNKNOWN` for framework identity instead of becoming static model-family
  support. Calls inside runtime validator bodies remain typed
  `pydantic_validator_side_effects` UNKNOWNs as non-blocking subclaim metadata;
  imported external model bases remain `FrameworkMagic` UNKNOWNs for framework
  identity unless the base resolves to an exact supported Pydantic base;
- bounded same-function application call recovery for import-resolved static forms such as
  `service = UserService(); service.list_users()` and
  `runner = run_query; runner()` inside FastAPI route units. These produce
  `fastapi_service_call` structural context anchors only. Reassignment removes
  the local role, dynamic forms such as `getattr(service, name)()` remain typed
  `UNKNOWN`, `setattr(...)` monkey-patching remains typed `UNKNOWN`, and
  service-call anchors do not derive route-family membership support. Dynamic
  decorators such as decorator factories and bare unresolved decorators produce
  typed `UNKNOWN` for framework identity instead of being guessed into a
  framework role; local decorators and native `property`, `classmethod`, and
  `staticmethod` stay structural metadata;
- SQLAlchemy 2.0 structural anchors for model class fields using imported
  `DeclarativeBase` bases, bounded `Base = declarative_base()` assignments,
  `Mapped[...]` annotations, `mapped_column(...)`, and `relationship(...)`
  calls. Literal `relationship("LocalModel")` targets are recorded as local
  relationship-target context when the target is a same-module class; dynamic,
  nonlocal, or unsupported relationship targets remain typed `UNKNOWN`. This
  also includes bounded parameter-role propagation that canonicalizes typed
  `Session` and `AsyncSession` calls such as `session.execute(...)`,
  `session.get(...)`, `session.commit()`, `session.rollback()`,
  `session.scalar(...)`, `session.scalars(...)`, and `session.add(...)` to
  exact SQLAlchemy session targets. Direct raw SQL strings and imported
  `text(...)` SQL text on query methods additionally preserve a typed
  `sqlalchemy_query_shape` `UNKNOWN`; the receiver call remains context but
  raw SQL semantics are not proven. The same bounded pass also propagates
  `__init__` assignments such as
  `self.session = session` and `self.db: AsyncSession = db` into later repository
  methods, while any same-method reassignment of that receiver blocks
  canonicalization, and untyped runtime-injected `self.session`-style receivers
  remain `RuntimeDependencyInjection` `UNKNOWN`s. Relationship and `add` anchors
  are effect/context metadata only and are not membership support targets.
  Calls to local custom query wrapper functions or methods remain
  framework-magic UNKNOWNs for framework identity even when the wrapper body
  contains typed SQLAlchemy session calls.
  `event.listen(...)` and `event.listens_for(...)` hooks remain framework-magic
  UNKNOWNs because runtime listener behavior is not statically proven. Dynamic
  model class factories such as `type(..., (Base,), ...)` remain framework-magic
  UNKNOWNs when the base resolves to a SQLAlchemy declarative base. Imported
  external declarative bases on SQLAlchemy-shaped classes remain
  framework-identity UNKNOWNs rather than static base support;
- Rust parser adapter translation into RepoGrammar-owned `CodeUnit` and IR
  metadata, plus same-generation storage of CPython `ast` structural and
  `UNKNOWN` facts after Rust-side envelope, field, path, hash, origin, range,
  note, assumption, and source-snippet validation;
- syntax-origin `FRAMEWORK_ROLE` facts for FastAPI route-shaped functions,
  pytest tests/fixtures, Pydantic model/settings-shaped classes, SQLAlchemy
  model-shaped classes, and SQLAlchemy repository method-shaped functions;
- an application-layer bounded exact-anchor derivation step that consumes
  validated parser-origin structural anchors plus syntax-origin framework-role
  facts and synthesizes separate `DATAFLOW_DERIVED` support facts only when the
  code unit has exactly one Python framework role, the structural evidence
  stays inside the same code-unit path/hash/range, and the target exact-matches
  the canonical FastAPI/pytest/Pydantic/SQLAlchemy compatibility table. Dynamic
  call/import aliases such as assigned `importlib.import_module`, `__import__`,
  `eval`, `exec`, `compile`, namespace lookups, `getattr`, and `setattr` remain
  typed `UNKNOWN` rather than becoming support;
- a conservative Python EC-MVFI-lite clustering step that builds internal
  feature vectors from language, unit kind, framework role, normalized shape,
  exact support target, support-family group, parser anchor categories, import
  context, call/effect markers, fixture context, Pydantic/SQLAlchemy
  model-context markers, path context, and available AST-skeleton labels, then
  uses a complete-link constraint so bridge members cannot single-link
  incompatible Python support families into one confident claim. Parser-origin
  context facts participate in compatibility, while parser-origin blocking
  `UNKNOWN` facts remove the affected unit from confident support unless the
  UNKNOWN is scoped to a non-membership subclaim such as
  `fastapi_dependency_target`. Non-blocking subclaim `UNKNOWN`s on supported
  members are preserved in family detail/query metadata with the concrete
  family id and subclaim name, so route membership does not silently imply that
  a dependency target was resolved. When parser-context profiles differ inside
  an already-supported Python family, the builder records metadata-only
  variation slots for those context categories instead of silently collapsing
  the difference into an unqualified runtime-equivalence claim. pytest
  non-builtin fixture dependency profiles remain compatibility constraints;
  only known builtin fixture-context differences may stay in the same ready
  family as metadata variation/context;
- committed Python release fixtures under `src/fixtures/python/release/v0_1/`
  for FastAPI, pytest, alias-aware pytest fixtures, Pydantic, SQLAlchemy, mixed,
  dynamic-unknown, dynamic pytest fixture names, low-support, strong-evidence,
  full FastAPI/APIRouter route-method variation, and stale-evidence smoke
  coverage. The route variation fixture covers `delete`, `get`, `head`,
  `options`, `patch`, `post`, and `put` for both `FastAPI` and `APIRouter`.
  The dynamic-unknown fixture covers dynamic import, `sys.path` mutation,
  dynamic call target, dynamic decorator, and monkey-patch boundaries;
- direct-worker, Rust parser-adapter, full-index, and incremental-sync
  regression coverage for the exact committed `pydantic-basic` fixture. Its
  `field_validator` declaration remains a structural Pydantic member fact while
  `value.lower()` in the validator body remains a typed, non-blocking
  `FrameworkMagic` UNKNOWN for `pydantic_validator_side_effects`; neither is
  upgraded into unsupported semantic certainty;
- product CLI smoke tests that copy those fixtures into temporary repositories,
  prove low-support or dynamic Python evidence remains typed `UNKNOWN`, prove
  exact FastAPI, FastAPI router-alias, pytest tests, pytest fixtures,
  Pydantic model,
  Pydantic settings, SQLAlchemy model, and SQLAlchemy session/repository
  anchors can produce default families only through separate derived support
  facts, prove exact-anchor family reads across `families`, `family`,
  `member`, `find`, `explain`, and advisory `check`, prove token-budget
  automatic evidence selection plus explicit compact/evidence/deep modes stay
  metadata-only, prove FastAPI request-shape and SQLAlchemy relationship/add
  auxiliary anchors remain metadata-only and blocked from claim-input readiness,
  prove dynamic-boundary facts remain typed `UNKNOWN`, blocked from claim-input
  readiness, and absent from derived support, including dynamic FastAPI
  dependency-target expressions that affect only the dependency-target
  sub-claim rather than route-family membership, and prove local framework
  lookalikes such as `@client.get(...)`, user-defined `BaseModel`, or
  user-defined SQLAlchemy-shaped `Base` classes do not become FastAPI,
  Pydantic, or SQLAlchemy family support,
  prove supported MCP operations return the same family context,
  prove the committed stale-evidence fixture returns blocking `StaleEvidence`
  `UNKNOWN` after source mutation or deletion, and keep the test-injected
  `SEMANTIC` worker fixture as coverage for the explicit worker boundary;
- compact/evidence/deep family output modes shared by CLI and MCP. Compact is
  the default and omits evidence records; evidence/deep select only stored
  repo-relative evidence metadata under an optional token budget and explicitly
  report that source snippets are not included. The selector now uses
  deterministic greedy marginal coverage over conservative claim labels:
  current stored evidence can cover `canonical`, `support`, and the narrow
  Python exact-anchor target `variation` case. Requested exception coverage or
  broader variation coverage is reported as missing until storage/model records
  explicitly link evidence to those roles;
- exact canonical Python framework target checks in the EC-MVFI-lite support
  gate for derived and future provider-backed strong facts. FastAPI
  `response_model=...`, static `Depends(get_db)` dependency-target,
  `Depends(...)`, `HTTPException(...)`, and literal HTTPException status-code
  anchors, plus FastAPI request body and request-parameter anchors, remain
  route schema/context/effect metadata and are explicitly excluded from
  membership support derivation. Pydantic field, field-type, `Field(...)`,
  `model_config`, nested `Config`, `computed_field`, `field_validator`, legacy
  `validator`, and `model_validator` anchors remain model schema/config/member
  metadata and are also excluded from membership support derivation.
  SQLAlchemy relationship-target anchors remain model relationship context
  only, while dynamic or nonlocal targets preserve `UNKNOWN`.
- a Rust `ports::python_provider` boundary for future candidate-scoped
  Pyrefly/Pyright/RightTyper requests, provider provenance assumptions,
  provider cache-key dimensions, and recoverable provider-unavailable
  `UNKNOWN`s. This boundary is not a provider adapter, does not execute external
  tools, and does not add production Pyrefly/Pyright/RightTyper support. Future
  provider adapters must translate accepted provider spans into existing
  same-code-unit path/hash/range support evidence before EC-MVFI-lite can use
  them; provider origin alone cannot bypass canonical target compatibility.
- an application-layer Pyrefly framework-identity request planner for future
  provider adapters. It groups only plausible Python family candidates that
  have one supported framework role and enough support under the current
  Python threshold, skips units with parser-origin blocking `UNKNOWN`s for the
  claim being planned, then builds validated `ResolveFrameworkIdentity` request
  scopes. The planner can also run over the same validated active-generation
  snapshot used by query/family reads, so future provider adapters consume
  stored repo-relative code-unit and fact records instead of reparsing. It does
  not execute Pyrefly, write storage rows, emit facts, alter CLI/MCP output, or
  turn syntax/framework-role evidence into a family claim.

These worker facts use current protocol fact and certainty tokens only:
`RESOLVED_IMPORT`, `RESOLVED_CALL`, `SYMBOL`, `TYPE`, `PROJECT_CONFIG`, and
`UNKNOWN` with `STRUCTURAL`, `DATAFLOW_DERIVED`, or `UNKNOWN` certainty. The
only parser-origin `DATAFLOW_DERIVED` facts accepted by product indexing are
source-local repo graph facts with `provider_resolved=false` plus
`derived_from=repo_local_python_import_graph` or
`derived_from=repo_local_pytest_fixture_graph`; all other CPython facts remain
structural or `UNKNOWN`. They are repo-relative, hash-backed, and snippet-free,
but they are still bounded static graph evidence, not semantic-provider claims.
Default product indexing does not expose raw parser facts through CLI/MCP query
commands. The current product path may additionally feed separately synthesized
`repogrammar-python-derived` / `bounded_ast_anchor_v1` facts to EC-MVFI-lite,
and those facts use `DATAFLOW_DERIVED`, `provider_resolved=false`, and
same-generation code-unit evidence. They are synthesized only for exact
compatible framework anchors on a unit with one framework role and no
claim-relevant parser-origin blocking `UNKNOWN`. This is sound-by-abstention
bounded Python framework-family claims, not sound Python semantic analysis or
external dependency proof.

The strong Python release smoke path is test-only. It uses the existing
transitional worker executable boundary to inject fixture-controlled
`python-fixture-provider` facts and verify product read paths, stale-evidence
fallback, and source/path leakage guards. It is not a Pyrefly/Pyright provider
implementation and must not be documented as production Python semantic
support.

This slice does not implement Pyrefly, Pyright, a provider adapter,
provider-backed usage propagation, cross-function call hierarchy recovery,
Tree-sitter fallback, runtime observation, broad Python family mining, source
snippet retrieval, or schema-backed medoid/general-variation/exception evidence
links. The provider cache-key shape exists only as a Rust port contract for
future adapters. The only current variation evidence is exact-compatible Python
framework-anchor target diversity inside an already-ready family. Persisted
project configuration facts and FastAPI service-call anchors are structural
context only and remain blocked from family-claim input.

The worker must fail safe on adversarial repository contents. Any unexpected
internal failure (including `RecursionError` from a deeply chained
attribute/subscript expression that overflows the CPython AST or a recursive
name helper) is converted into a typed `worker_error` terminated by
`end_of_stream`, never a truncated stream or a nonzero process exit. The
self-recursive name helpers apply an explicit recursion-depth guard so one
pathological expression abstains instead of aborting the whole request. In
project mode the worker also enforces an aggregate source-byte budget across all
changed files — in addition to the per-file size and file-count caps — so a
request naming thousands of near-maximal files fails closed with a
`worker_error` rather than reading an unbounded amount of source into memory.

### Layer 0: Authoritative Frontend

Use existing native and public parsers instead of hand-written Python parsing.
Normal, parseable Python files should use:

- Python standard-library `ast` for syntax nodes and UTF-8 byte offsets.
- Python standard-library `symtable` for compiler scope facts.
- Python standard-library `tomllib` and `configparser` for bounded reads of
  `pyproject.toml` and `setup.cfg`, plus `ast` for bounded static parsing of
  `setup.py` without execution.

Extract modules, functions, async functions, classes, methods, decorators, call
expressions, imports, assignments, class bases, annotations, and source ranges.
Syntax facts are `STRUCTURAL` or `FRAMEWORK_HEURISTIC`. They cannot prove
semantic family membership by themselves.

Tree-sitter Python is a fallback adapter only:

```text
ast.parse success
  -> authoritative structural facts

ast.parse SyntaxError + tree-sitter parse success
  -> STRUCTURAL candidates + ParseDiagnostic
  -> no family claim

both fail
  -> UnsupportedSyntax UNKNOWN
```

The worker Python version must be recorded in provenance because Python AST
shape changes across Python releases. Long-term, the Python worker should emit
code units, structural facts, semantic anchors, and diagnostics in one pass so
Rust does not need to parse the same file a second time.

### Layer 1: Repo-local Import Resolution

Build a conservative repository module graph from:

- `.py` files;
- package directories and `__init__.py`;
- `pyproject.toml`, `setup.cfg`, and `setup.py` where safely parseable;
- known dependency manifests such as `requirements.txt` when present.

Resolve absolute imports, relative imports, `from x import y`, and aliases.
Only unique repo-local matches become resolved import facts. External imports
may become external dependency context. A literal `importlib.import_module` or
`__import__` target, and a target read from a single-static local string
constant (sound intra-scope constant propagation: excluded if the name is
reassigned, parameter-bound, or otherwise ambiguously bound), may resolve to a
repo-local module. A `__import__` call resolves only when its argument shape
cannot make it relative: a nonzero or non-literal `level` argument, or a
positional/keyword splat that could hide `level`, keeps the call a typed
`UNKNOWN`. Data-dependent or truly non-literal `importlib.import_module` /
`__import__`, mutated `sys.path`, or runtime import hooks must produce typed
`UNKNOWN`.

The current implementation performs the first narrow version in both private
parse-document and semantic-worker-compatible project modes. Default indexing
passes discovered safe `.py` inventory and sanitized source roots extracted
from the project-config parse report into private
parse-document requests, then persists unique module-level repo-local import
anchors as structural facts. Root `pyproject.toml` is parsed with `tomllib` and
root `setup.cfg` is parsed with the standard-library `configparser` (available
on every supported Python), so `setup.cfg` `[tool:pytest]` test paths and
`[options.packages.find] where` roots are merged into the same source-root
context; a malformed `setup.cfg` produces a typed `MissingProjectConfig`
`UNKNOWN`. Root `setup.py` is discovered as a Python config file and parsed with
the standard-library `ast` module and is **never executed**. A setup call must be
a direct unconditional module-body expression lexically resolved through a
direct, aliased, or module-qualified `setuptools` import with no recognized
name, relevant module-attribute, or namespace mutation, including
builtins-qualified explicit mutation helpers. It must have zero positional
arguments, no keyword unpacking, and at most one `name`, `package_dir`, and
`packages` keyword. A `package_dir` contributes roots only when its entire dict
literal is a unique string-to-string mapping with no unpacking. A package finder
contributes only as the direct `packages=` value and only with zero or one
literal positional `where`, or one literal `where=` keyword, never both, with no
keyword unpacking. Same-leaf local functions, unrelated qualified helpers,
standalone or lookalike finders, conditional/dead calls, and bindings
conditionally shadowed, deleted, or explicitly mutated after import abstain.
A recognized unique setup call with a dynamic, partial, duplicate, unpacked, or
otherwise overridable relevant field, or one made unreachable by an earlier
unconditional top-level `raise`, emits typed `MissingProjectConfig` and no roots
from the incomplete field; `setup()` remains a complete empty config. Multiple
authoritative calls produce typed `ConflictingFacts`, and syntax that does not
parse also produces `MissingProjectConfig`. Project mode also applies sanitized
`pyproject.toml` source roots when the running Python provides `tomllib`.
Neither path resolves imported symbols, re-exports, namespace packages, or
site-packages. The `setup.py` scan is lexical and does not model arbitrary
runtime side effects hidden inside unrelated helper calls.

When more than one of `pyproject.toml`, `setup.cfg`, and `setup.py` exists,
default indexing deduplicates the safe roots from every successfully parsed file
into candidate parser context. This union is not Python packaging or setuptools
precedence resolution. Each config record remains structural and blocked from
claim-input readiness, so a config conflict cannot upgrade a family claim or
suppress otherwise required strong evidence.

The Rust product parser routes only the exact root config paths and stamps
structural fact provenance with the actual frontend: `tomllib` for
`pyproject.toml`, `configparser` for `setup.cfg`, and `cpython_ast` for
`setup.py`. Similar names and nested `setup.py` files remain ordinary Python
sources rather than project config. The routing fix and audit are recorded in
`docs/reports/python-setup-py-project-config-review.md`.

### Layer 2: Typed Canonical Framework Identity

Python framework compatibility must use typed canonical identities. Do not
reuse substring matching over fact kind, engine, method, target, or assumptions.
For example, a user module named `myproject.react_utils` must not become React
evidence merely because a string contains `react`.

The Python fact model should distinguish framework identity from textual fact
fields:

```text
Framework = FastApi | Pytest | Pydantic | SqlAlchemy

PythonSemanticFact =
  ResolvedSymbol(canonical_fqn)
  SubclassOf(canonical_base_fqn)
  DecoratorBinding(canonical_decorator_fqn)
  CallTarget(canonical_target_fqn)
  FixtureBinding(fixture_fqn)
```

Compatibility examples:

- FastAPI route: decorator or call target resolves to
  `fastapi.FastAPI.{delete,get,head,options,patch,post,put}` or
  `fastapi.APIRouter.{delete,get,head,options,patch,post,put}`. The generic
  `api_route` decorator and WebSocket routes are deferred and must not become
  v0.1 exact-anchor membership support without an explicit compatibility-table
  update and tests.
- pytest fixture: decorator resolves to `pytest.fixture`, or the provider
  resolves a fixture binding.
- Pydantic model/settings: subclass relation resolves to `pydantic.BaseModel`,
  `pydantic.BaseSettings`, or `pydantic_settings.BaseSettings`.
- SQLAlchemy model: base or mapped members resolve to
  `sqlalchemy.orm.DeclarativeBase`, bounded `sqlalchemy.orm.declarative_base`,
  `sqlalchemy.orm.Mapped`, or `sqlalchemy.orm.mapped_column`.
- SQLAlchemy repository method: calls on parameters annotated as
  `sqlalchemy.orm.Session` or `sqlalchemy.ext.asyncio.AsyncSession` resolve to
  exact supported sync or async session method targets, including `execute`,
  `get`, `commit`, `rollback`, `scalar`, and `scalars`.

Unresolved canonical identity blocks only the claim that depends on that
identity. A FastAPI route family may still exist when a specific dependency
target is unknown, but the dependency-target claim must remain `UNKNOWN`.

### Layer 3: Selective Semantic Providers

Pyrefly is the primary Python semantic provider for v0.1. Use it through public
CLI or LSP-style boundaries for definition, type definition, references, hover
or type text, call hierarchy, and type hierarchy. Do not depend on Pyrefly
private Rust crates or internal data structures.

Pyrefly facts must carry:

- provider name and exact pinned provider version;
- Python version;
- provider config hash;
- environment fingerprint;
- file content hash;
- source range;
- query operation;
- freshness status.

Pyright is the selective cross-check provider. Use it only for facts that would
upgrade a candidate to a family claim or materially change a variation or
exception. If Pyrefly and Pyright agree, the future implementation may classify
the fact as cross-checked semantic evidence after domain/protocol support is
added. If they disagree, preserve `ConflictingFacts` and block the affected
claim. If Pyright is unavailable, Pyrefly-primary facts may still be used only
with stricter support thresholds.

Mypy is project-native auxiliary evidence only when the repository already uses
mypy. `ty` is an experimental benchmark provider until validated on the
RepoGrammar Python fixture corpus.

### Layer 4: Framework Role Extraction

Framework-role extraction is the highest-value Python v0.1 layer. It should not
wait for full type inference.

FastAPI roles:

- `FASTAPI_ROUTE`;
- `FASTAPI_DEPENDENCY`;
- `FASTAPI_RESPONSE_MODEL`.

Required evidence includes normalized decorators such as `@app.get`,
`@router.post`, `Depends(...)`, `HTTPException`, and `response_model=...`,
plus import/alias evidence and local `APIRouter()` assignment propagation.

pytest roles:

- `PYTEST_TEST`;
- `PYTEST_FIXTURE`;
- `PYTEST_FIXTURE_EDGE`;
- `PYTEST_PARAMETRIZE`.

Required evidence includes test function naming, alias-normalized
`@pytest.fixture`, `@pytest.mark.parametrize`, nearest `conftest.py` hierarchy,
explicit fixture definitions, built-in/plugin fixture allowlists, and ambiguity
detection. Bare canonical fixture decorators may become exact-anchor support for
pytest fixture families when the compatibility table, support count, and
freshness/readiness gates pass. Canonical `pytest.mark.parametrize` decorator
anchors may support `framework:pytest.test` families under the same gates.
`pytest.fixture` decorators must not support `framework:pytest.test`
membership; they belong to pytest fixture families or fixture-binding context.
`pytest.fixture.<name>` fixture-edge anchors and `pytest.parametrize.<name>`
argument anchors are context only and must not derive family support.
Non-builtin fixture-edge profiles are family compatibility constraints; known
pytest built-in fixture context may differ only as metadata variation/context.

Pydantic roles:

- `PYDANTIC_MODEL`;
- `PYDANTIC_VALIDATOR`;
- `PYDANTIC_SCHEMA_SLOT`.

Required evidence includes `BaseModel` inheritance, field annotations,
`Field(...)` metadata, `model_config` or `Config`, `@field_validator`, legacy
`@validator`, and `computed_field` / `@model_validator` declarations. In the
current implementation slice, fields, field types, field metadata, config,
computed fields, and model validators are structural schema/member anchors
only; they do not derive family support without an exact compatible model/base
support target. Dynamic `ConfigDict` values remain typed UNKNOWNs.

SQLAlchemy roles:

- `SQLALCHEMY_MODEL`;
- `SQLALCHEMY_REPOSITORY_METHOD`;
- `SQLALCHEMY_TRANSACTION_BOUNDARY`.

The first SQLAlchemy slice should prioritize SQLAlchemy 2.0 typed mappings:
`DeclarativeBase`, `Mapped`, `mapped_column`, `relationship`, `Session` or
`AsyncSession`, `select(...)`, `session.execute(...)`, `session.get(...)`,
`session.scalar(...)`, `session.scalars(...)`, `commit`, and
rollback/error handling where statically visible. Raw SQL strings and imported
`text(...)` SQL text remain typed query-shape UNKNOWNs. Legacy dynamic
`declarative_base()` support may be recognized with lower certainty, but the
deprecated SQLAlchemy mypy plugin must not become a default provider.

### Layer 5: Usage-driven Fixpoint-lite Context Propagation

Use a small, bounded version of usage-driven propagation inspired by modern
Python type-inference research. The v0.1 goal is not full type inference; it is
framework context recovery.

Propagate only high-signal facts:

- `router = APIRouter()` gives `router` a FastAPI router role;
- `class UserOut(BaseModel)` gives the class a Pydantic model role;
- `db: Session` or `AsyncSession` gives a parameter/session context;
- `Depends(get_db)` links route dependency slots;
- `@pytest.fixture` binds fixture names;
- simple alias assignments preserve framework object roles.

The initial propagation budget should be file-local plus simple import-linked
cross-file edges. Deep symbolic execution, broad project-wide type inference,
and outcome-driven special cases are deferred.

Recommended bounds:

- max fixpoint iterations: 4;
- max cross-file propagation depth: 2;
- site-packages traversal: disabled;
- dynamic `getattr`, `setattr`, bare or indexed `globals`/`locals`, `eval`,
  `exec`, `compile`, and `__import__`: typed `UNKNOWN`.

### Layer 6: Application-centered Call Recovery

Use Pyrefly call hierarchy first. Borrow from JARVIS-style application-centered
analysis only as a bounded fallback. Do not build a whole-repository call graph;
RepoGrammar needs target-centered recovery for the current candidate family.

Recover at most one to two hops for high-value edges:

- FastAPI handler to service/repository method;
- pytest test to client request or system-under-test call;
- fixture to fixture dependency;
- repository method to `session.execute`, `select`, `commit`, or rollback.

The JARVIS-lite fallback should support only simple high-value forms:

- `x = Constructor(); x.method()`;
- `x = imported_symbol; x(...)`;
- `return service.method(...)`;
- `self.service = service; self.service.method(...)`.

The current implemented fallback covers only same-function static local
assignments for import-resolved FastAPI route context anchors. It is not
cross-function call hierarchy recovery and does not produce membership support.

If the target comes from dynamic dependency injection, runtime registries,
monkey patching, plugin hooks, `getattr`, metaclass-generated methods, or
unresolved imports, emit `CALL_TARGET_UNKNOWN` as a typed `UNKNOWN` for that
claim.

### Layer 7: pytest Fixture Graph

Build a dedicated pytest fixture graph because fixture injection is a core
source of Python test-family evidence.

Algorithm:

1. Collect `@pytest.fixture` definitions. Literal `name="..."` aliases become
   the fixture binding name; dynamic or unsafe `name=` values do not fall back
   to the implementation function name.
2. Collect `conftest.py` files by directory hierarchy.
3. Collect test function parameters.
4. Classify direct `pytest.mark.parametrize` arguments before fixture lookup.
5. Preserve indirect parametrize arguments as typed `PytestFixtureInjection`
   `UNKNOWN`.
6. Map each remaining parameter to a unique applicable fixture definition.
   Duplicate applicable `conftest.py` definitions remain ambiguous until a
   provider proves pytest's exact runtime resolution.
7. Resolve literal `request.getfixturevalue("name")` with the same exact
   fixture lookup as parameter injection. Nonliteral `getfixturevalue(...)`
   stays `PytestFixtureInjection` `UNKNOWN`.
8. Mark known pytest built-in fixtures as external fixture context. Plugin
   fixtures are external context only when a declared allowlist or provider
   proves the binding.
9. Emit `ConflictingFacts` or `PytestFixtureInjection` `UNKNOWN` for ambiguous,
   dynamic, duplicate, or unresolved plugin-defined bindings.

### Layer 8: Python EC-MVFI-lite

Candidate grouping key:

```text
language = python
unit_kind
framework_role
decorator_shape
import_context
call_shape
path_context
```

Family claim gates:

- support count satisfies the configured threshold and the initial support
  policy below;
- framework role is identical or explicitly compatible;
- canonical framework identity is provider-resolved or cross-checked when the
  claim depends on it;
- evidence is fresh against current source hashes;
- import/context facts are resolved or non-blocking for the specific claim;
- semantic support is role-compatible;
- blocking `UNKNOWN` is absent for the emitted claim;
- source evidence remains repo-relative and snippet-free.

Hard blocking conditions:

- different language;
- different code-unit kind;
- different typed framework role;
- incompatible provider-resolved framework identity;
- stale source, config, or provider evidence;
- blocking `UNKNOWN`.

Initial fingerprint features:

- canonical decorators;
- canonical base classes;
- normalized parameter and type signature;
- framework API calls;
- bounded repo-local call targets;
- import multiset;
- fixture dependencies;
- effect-lite markers such as `HTTPException`, `Depends`, `session.execute`,
  `commit`, rollback, response model, and fixture client use;
- path context;
- normalized AST shape.

For retrieval, small and medium repositories may use an inverted index over
framework role, code-unit kind, canonical API, decorator, and base class.
Larger repositories may later add SourcererCC-style rare-feature-first and
prefix-filtering ideas to reduce pairwise comparisons.

For clustering, prefer complete-link constrained agglomerative clustering over
single-link or raw connected components. Complete-link is more conservative
because every pair inside a family must satisfy the threshold. Initial support
policy:

- support >= 3: primary family candidate if all hard gates pass;
- support == 2: only allowed after the Rust domain, protocol, storage, CLI,
  MCP, and tests define a cross-checked semantic certainty; until then use a
  stricter support threshold or return `UNKNOWN`;
- otherwise: `UNKNOWN`.

The current `repogrammar-python-derived` support facts satisfy only the
bounded-propagation side of this policy. They can support a family when at
least three same-role members have exact canonical anchors and pass the
support-family complete-link constraint, but they do not prove
provider-resolved identity, dependency binding, transaction behavior, fixture
graph completeness, or runtime equivalence. When one coarse bucket contains
multiple ready support-family clusters, the first cluster preserves the stable
base family id and later clusters receive sanitized cluster suffixes; no suffix
contains source text or absolute paths.

Output remains one of `DOMINANT_PATTERN`, `VARIATION`, `EXCEPTION`, or
`UNKNOWN`. Syntax-only or framework-heuristic-only Python observations can rank
candidates but cannot prove a family.

### Layer 9: Optional Runtime Trace Refinement

Runtime tracing is deferred and opt-in. A future RightTyper-style bounded trace
command may record observed behavior for selected pytest tests or FastAPI app
entrypoints, but observed evidence must be labeled separately, such as a future
`OBSERVED_SEMANTIC` certainty tier, and must not be generalized to unobserved
executions.

Runtime tracing requires an explicit consent and safety boundary. It must never
run during default `index` or `sync`.

Observed evidence must carry test command hash, run id, Python version,
environment hash, observed timestamp, and source content hash. It must use a
no-mutation execution mode when available.

## Template and Evidence Compression

Python family output should use Aroma-style cluster-and-intersect rather than
returning top-k similar files. Preserve common AST node kinds and introduce
slots for identifiers, literals, type annotations, decorator arguments, and
service calls. Do not slot away framework decorators, canonical base classes,
critical API calls, effect markers, or fixture dependencies.

Default query output has three levels:

- `compact`: family id, classification, support, invariants, variation slots,
  exception summary, and unknowns. No source snippets.
- `evidence`: compact output plus one canonical medoid, one representative
  variation, and one exception when present, using repo-relative path, byte
  range, and hash. No source snippets by default.
- `deep`: minimum source spans only when explicitly requested through the
  source-span opt-in flag/input. `deep` alone remains metadata-first.

Evidence selection should be budgeted. Each evidence item records the claims it
covers, such as route decorator invariant, dependency variation, response model
variation, service call invariant, transaction exception, or runtime unknown.
Under token budget `B`, choose evidence greedily by marginal weighted coverage
per estimated token cost, with these constraints:

- at least one canonical evidence item;
- at most one or two variation items unless explicitly requested;
- at least one exception when exceptions exist;
- at most one evidence item from the same file by default.

Current selector status: CLI and MCP share a deterministic metadata selector
over stored `IndexedFamilyEvidenceRecord`s. `compact` returns no evidence
records but still returns a read plan. `evidence` and `deep` run a greedy
marginal-coverage selector over conservative metadata candidates and keep
source snippets disabled unless source spans are explicitly requested. The read
plan recommends target, canonical, support, and variation/exception spans by
repo-relative path, strict content hash, and byte range. When explicit source
spans are requested, RepoGrammar renders only selected hash-checked spans with
line numbers and omits stale, missing, unsupported, dynamic, insufficient, or
conflicting spans with recovery guidance. Agents must read the target source
body before editing outside rendered ranges. Evidence records carry
schema-backed `covered_claims` labels from the allowlist
`canonical`, `support`, `variation`, and `exception`; the selector consumes
those labels and never infers coverage from free-text notes. The current builder
emits `canonical` and `support`, plus one narrow Python `variation` evidence
label when a ready family has multiple exact-compatible framework-anchor
support targets. It may also emit metadata-only variation slots when parser
context profiles differ inside an already-ready family. Requested exception
coverage and broader variation evidence coverage remain in `missing_claims`
until explicit medoid, variation-slot, and exception evidence links exist.

## Rejected v0.1 Routes

- Hand-written Python parser: use public parser libraries instead.
- Whole-program Python call graph: too much cost and false precision for the
  pattern-family goal.
- Sound full Python semantics: not realistic for the target frameworks.
- LLM-derived facts as family evidence: LLM output may help explanation later,
  but cannot be provenance for membership.
- Default runtime tracing: too much execution risk and environment dependency.
- Django in v0.1: framework magic and settings/runtime configuration are
  deferred until the focused backend subset is validated.
- Neural type prediction and LLM semantic inference as production evidence:
  useful as baselines or explanations, but not auditable static facts for
  family membership.

## UNKNOWN Rules

The following conditions must produce typed `UNKNOWN` for affected claims:

- dynamic or non-literal imports;
- mutated `sys.path` or import hooks;
- monkey patching;
- unresolved decorators;
- runtime dependency injection;
- dynamic Pydantic model factories;
- ambiguous pytest fixture injection;
- dynamic or unsafe pytest fixture `name=` aliases;
- duplicate applicable `conftest.py` fixture names without provider resolution;
- plugin-style pytest fixture names without an allowlist or provider;
- missing project configuration;
- missing dependencies;
- framework magic not covered by the adapter;
- conflicting analyzer facts;
- stale source evidence;
- insufficient support.

Unknowns should block only the claim they affect. For example, an unknown
fixture binding may block test-behavior equivalence while still allowing a
structural pytest-test family candidate.
When a claim-relevant `UNKNOWN` removes a unit from confident family support,
family output must preserve that original reason code and affected claim in
addition to any aggregate `InsufficientSupport` result caused by the remaining
support count. When a non-blocking subclaim `UNKNOWN` does not remove support,
family detail/query output must still preserve the subclaim, such as
`<family_id>:fastapi_dependency_target`, rather than implying that the subclaim
is known.

## References

These sources inform the v0.1 algorithm stack; they are not implementation
dependencies by themselves.

- Typify: usage-driven static analysis for Python type inference,
  <https://arxiv.org/abs/2604.05067>.
- PyCG: practical static Python call graph generation,
  <https://arxiv.org/abs/2103.00587>.
- JARVIS: application-centered Python call graph construction,
  <https://arxiv.org/abs/2305.05949>.
- RightTyper: hybrid runtime-observed Python type inference,
  <https://arxiv.org/abs/2507.16051>.
- Pyrefly documentation and repository,
  <https://pyrefly.org/en/docs/>,
  <https://github.com/facebook/pyrefly>.
- Pyright repository,
  <https://github.com/microsoft/pyright>.
- Tree-sitter Python grammar,
  <https://github.com/tree-sitter/tree-sitter-python>.
- Python `ast` module,
  <https://docs.python.org/3/library/ast.html>.
- Python `symtable` module,
  <https://docs.python.org/3/library/symtable.html>.
- Python `tomllib` module,
  <https://docs.python.org/3/library/tomllib.html>.
- SQLAlchemy mypy plugin deprecation and SQLAlchemy 2.0 typing guidance,
  <https://docs.sqlalchemy.org/en/20/orm/extensions/mypy.html>.
- SourcererCC: inverted-index clone retrieval,
  <https://arxiv.org/abs/1512.06448>.
- Aroma: structural code search with clustering and intersection,
  <https://arxiv.org/abs/1812.01158>.
- Repoformer: selective retrieval for repository-level code completion,
  <https://arxiv.org/abs/2403.10059>.
