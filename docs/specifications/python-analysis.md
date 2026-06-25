# Python Analysis Specification

- Status: Active v0.1 target specification
- Last updated: 2026-06-25
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

The current implementation covers the first structural slice only:

- `.py` file discovery with Python virtualenv/cache/dependency directory skips;
- CPython `ast` parse-document worker output for code-unit extraction;
- CPython `ast` structural fact output for import bindings, decorator anchors,
  class bases, simple call targets, same-file pytest fixture edges, and typed
  dynamic/unresolved `UNKNOWN` facts;
- path-derived module-name anchors and CPython `symtable` structural scope
  anchors for imported, assigned, and namespace symbols;
- a private `tomllib` project-config parser mode for safe `pyproject.toml`
  summaries, including sanitized project name, safe source roots, recognized
  tool sections, and typed config `UNKNOWN` when parsing support or valid config
  is unavailable;
- semantic-worker-compatible project-mode module graph construction that uses
  safe `.py` paths plus sanitized `pyproject.toml` source roots when `tomllib`
  is available, emits `STRUCTURAL` `RESOLVED_IMPORT` facts only for unique
  repo-local module matches, and emits typed `UNKNOWN` for ambiguous/missing
  repo-local imports or `sys.path` mutation;
- default parser-mode indexing now passes the discovered repo-relative `.py`
  inventory into the private CPython parse-document request so the same
  source-tied parse pass can emit unique repo-local import facts and typed
  unresolved/ambiguous import `UNKNOWN`s without launching a separate Python
  semantic worker;
- default parser-mode indexing discovers root `pyproject.toml` as
  `python-config`, reads it through the Rust source-store path/hash boundary,
  calls the private `parse_project_config` worker mode, and persists a
  `project_config` code unit plus `PROJECT_CONFIG`/`STRUCTURAL` facts for
  sanitized project name, safe source roots, recognized tool sections, or typed
  `UNKNOWN` config facts when `tomllib` or valid TOML is unavailable;
- Rust parser adapter translation into RepoGrammar-owned `CodeUnit` and IR
  metadata, plus same-generation storage of CPython `ast` structural and
  `UNKNOWN` facts after Rust-side envelope, field, path, hash, origin, range,
  note, assumption, and source-snippet validation;
- syntax-origin `FRAMEWORK_ROLE` facts for FastAPI route-shaped functions,
  pytest tests/fixtures, Pydantic model-shaped classes, SQLAlchemy
  model-shaped classes, and SQLAlchemy repository method-shaped functions;
- an application-layer bounded exact-anchor derivation step that consumes
  validated parser-origin structural anchors plus syntax-origin framework-role
  facts and synthesizes separate `DATAFLOW_DERIVED` support facts only when the
  code unit has exactly one Python framework role, the structural evidence
  stays inside the same code-unit path/hash/range, and the target exact-matches
  the canonical FastAPI/pytest/Pydantic/SQLAlchemy compatibility table;
- committed Python release fixtures under `src/fixtures/python/release/v0_1/`
  for FastAPI, pytest, Pydantic, SQLAlchemy, mixed, dynamic-unknown,
  low-support, strong-evidence, and stale-evidence smoke coverage;
- product CLI smoke tests that copy those fixtures into temporary repositories,
  prove low-support or dynamic Python evidence remains typed `UNKNOWN`, prove
  exact FastAPI anchors can produce a default family only through separate
  derived support facts, and keep the test-injected `SEMANTIC` worker fixture as
  coverage for the explicit worker boundary;
- compact/evidence/deep family output modes shared by CLI and MCP. Compact is
  the default and omits evidence records; evidence/deep select only stored
  repo-relative evidence metadata under an optional token budget and explicitly
  report that source snippets are not included. The selector now uses
  deterministic greedy marginal coverage over conservative claim labels:
  current stored evidence can cover `canonical` and `support`, while requested
  variation or exception coverage is reported as missing until storage/model
  records explicitly link evidence to those roles;
- exact canonical Python framework target checks in the EC-MVFI-lite support
  gate for derived and future provider-backed strong facts.

These worker facts use current protocol fact and certainty tokens only:
`RESOLVED_IMPORT`, `RESOLVED_CALL`, `SYMBOL`, `TYPE`, `PROJECT_CONFIG`, and
`UNKNOWN` with `STRUCTURAL` or `UNKNOWN` certainty. They are repo-relative,
hash-backed, and snippet-free, but they are still worker-local structural
anchors or config metadata. Default product indexing persists them only as
internal structural/`UNKNOWN` semantic fact records. It does not expose raw
parser facts through CLI/MCP query commands or treat them as semantic-provider
claims. The current product path may feed only separately synthesized
`repogrammar-python-derived` / `bounded_ast_anchor_v1` facts to EC-MVFI-lite,
and those facts use `DATAFLOW_DERIVED`, `provider_resolved=false`, and
same-generation code-unit evidence.

The strong Python release smoke path is test-only. It uses the existing
transitional worker executable boundary to inject fixture-controlled
`python-fixture-provider` facts and verify product read paths, stale-evidence
fallback, and source/path leakage guards. It is not a Pyrefly/Pyright provider
implementation and must not be documented as production Python semantic
support.

This slice does not implement Pyrefly, Pyright, provider cache keys, usage
propagation, call hierarchy recovery, Tree-sitter fallback, runtime
observation, broad Python family mining, source snippet retrieval, or
schema-backed medoid/variation/exception evidence links. Persisted project
configuration facts are structural context only and remain blocked from
family-claim input.

### Layer 0: Authoritative Frontend

Use existing native and public parsers instead of hand-written Python parsing.
Normal, parseable Python files should use:

- Python standard-library `ast` for syntax nodes and UTF-8 byte offsets.
- Python standard-library `symtable` for compiler scope facts.
- Python standard-library `tomllib` for bounded reads of `pyproject.toml` and
  related configuration.

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
may become external dependency context. Ambiguous namespace packages, missing
project configuration, non-literal `importlib.import_module`, mutated
`sys.path`, or runtime import hooks must produce typed `UNKNOWN`.

The current implementation performs the first narrow version in both private
parse-document and semantic-worker-compatible project modes. Default indexing
passes discovered safe `.py` inventory into private parse-document requests and
persists unique module-level repo-local import anchors as structural facts.
Project mode additionally applies sanitized `pyproject.toml` source roots when
the running Python provides `tomllib`. Neither path resolves imported symbols,
re-exports, namespace packages, or site-packages.

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
  `fastapi.FastAPI.get`, `fastapi.FastAPI.post`, `fastapi.APIRouter.get`,
  `fastapi.APIRouter.post`, or another supported FastAPI route method.
- pytest fixture: decorator resolves to `pytest.fixture`, or the provider
  resolves a fixture binding.
- Pydantic model: subclass relation resolves to `pydantic.BaseModel` or
  `pydantic_settings.BaseSettings`.
- SQLAlchemy model: base or mapped members resolve to
  `sqlalchemy.orm.DeclarativeBase`, `sqlalchemy.orm.Mapped`, or
  `sqlalchemy.orm.mapped_column`.

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

Required evidence includes test function naming, `@pytest.fixture`,
`@pytest.mark.parametrize`, nearest `conftest.py` hierarchy, explicit fixture
definitions, built-in/plugin fixture allowlists, and ambiguity detection.

Pydantic roles:

- `PYDANTIC_MODEL`;
- `PYDANTIC_VALIDATOR`;
- `PYDANTIC_SCHEMA_SLOT`.

Required evidence includes `BaseModel` inheritance, field annotations,
`model_config` or `Config`, `@field_validator`, legacy `@validator`, and
`computed_field`.

SQLAlchemy roles:

- `SQLALCHEMY_MODEL`;
- `SQLALCHEMY_REPOSITORY_METHOD`;
- `SQLALCHEMY_TRANSACTION_BOUNDARY`.

The first SQLAlchemy slice should prioritize SQLAlchemy 2.0 typed mappings:
`DeclarativeBase`, `Mapped`, `mapped_column`, `relationship`, `Session` or
`AsyncSession`, `select(...)`, `session.execute(...)`, `commit`, and
rollback/error handling where statically visible. Legacy dynamic
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
- dynamic `getattr`, `setattr`, `globals`, `locals`, `eval`, and `exec`:
  typed `UNKNOWN`.

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

If the target comes from dynamic dependency injection, runtime registries,
monkey patching, plugin hooks, `getattr`, metaclass-generated methods, or
unresolved imports, emit `CALL_TARGET_UNKNOWN` as a typed `UNKNOWN` for that
claim.

### Layer 7: pytest Fixture Graph

Build a dedicated pytest fixture graph because fixture injection is a core
source of Python test-family evidence.

Algorithm:

1. Collect `@pytest.fixture` definitions.
2. Collect `conftest.py` files by directory hierarchy.
3. Collect test function parameters.
4. Map each parameter to the nearest unique fixture definition.
5. Mark built-in or plugin fixtures as external fixture context when known.
6. Emit `ConflictingFacts` or `PytestFixtureInjection` `UNKNOWN` for ambiguous,
   dynamic, or plugin-defined bindings.

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
least three same-role members have exact canonical anchors, but they do not
prove provider-resolved identity, dependency binding, transaction behavior,
fixture graph completeness, or runtime equivalence.

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
- `deep`: minimum source spans only when explicitly requested. The current
  implementation accepts the mode but remains metadata-only until a safe
  source-span rendering contract exists.

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
records. `evidence` and `deep` run a greedy marginal-coverage selector over
conservative metadata candidates and keep source snippets disabled. The current
record model can cover only `canonical` and `support` claims; when callers ask
for variation or exception coverage, the selector reports those labels in
`missing_claims` instead of inferring them from free-text notes. Schema-backed
medoid, variation-slot, and exception evidence links remain future work.

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
- ambiguous pytest fixture injection;
- missing project configuration;
- missing dependencies;
- framework magic not covered by the adapter;
- conflicting analyzer facts;
- stale source evidence;
- insufficient support.

Unknowns should block only the claim they affect. For example, an unknown
fixture binding may block test-behavior equivalence while still allowing a
structural pytest-test family candidate.

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
