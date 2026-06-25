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
families for coding agents. The right design is therefore a conservative,
research-informed method stack:

```text
syntax structure
-> repo-local import resolution
-> framework role extraction
-> usage-driven context propagation
-> target-centered call recovery
-> framework-specific evidence views
-> EC-MVFI-lite family induction
-> typed UNKNOWN for unresolved claims
```

This stack is reasonable because it uses mature parser and language-native
facts for stable evidence, borrows useful ideas from Python type and call-graph
research, and rejects unsupported certainty. It deliberately does not attempt a
whole-program sound Python semantic model.

## Adopted v0.1 Method Stack

### Layer 0: Syntax Extraction

Use existing parsers instead of hand-written Python parsing:

- Rust adapter: `tree-sitter-python` once the Tree-sitter adapter boundary is
  accepted.
- Python worker: Python standard-library `ast` for language-native syntax facts
  where executing a Python analyzer process is useful.
- Future formatting-sensitive option: LibCST, only if preserving comments or
  exact concrete syntax becomes necessary.

Extract modules, functions, async functions, classes, methods, decorators, call
expressions, imports, assignments, class bases, annotations, and source ranges.
Syntax facts are `STRUCTURAL` or `FRAMEWORK_HEURISTIC`. They cannot prove
semantic family membership by themselves.

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

### Layer 2: Framework Role Extraction

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

Required evidence includes declarative base patterns, `DeclarativeBase`,
`mapped_column`, `Column`, `relationship`, `Session` or `AsyncSession` context,
`select(...)`, `session.execute(...)`, `commit`, and rollback/error handling
where statically visible.

### Layer 3: Usage-driven Fixpoint-lite Context Propagation

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

### Layer 4: Application-centered Call Recovery

Borrow from PyCG/JARVIS-style assignment and type-graph ideas, but do not build
a whole-repository call graph. RepoGrammar needs target-centered recovery for
the current candidate family.

Recover at most one to two hops for high-value edges:

- FastAPI handler to service/repository method;
- pytest test to client request or system-under-test call;
- fixture to fixture dependency;
- repository method to `session.execute`, `select`, `commit`, or rollback.

If the target comes from dynamic dependency injection, runtime registries,
monkey patching, plugin hooks, or unresolved imports, emit
`CALL_TARGET_UNKNOWN` as a typed `UNKNOWN` for that claim.

### Layer 5: pytest Fixture Graph

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

### Layer 6: Python EC-MVFI-lite

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

- support count is at least the configured threshold, default 2;
- framework role is identical or explicitly compatible;
- evidence is fresh against current source hashes;
- import/context facts are resolved or non-blocking for the specific claim;
- semantic support is role-compatible;
- blocking `UNKNOWN` is absent for the emitted claim;
- source evidence remains repo-relative and snippet-free.

Output remains one of `DOMINANT_PATTERN`, `VARIATION`, `EXCEPTION`, or
`UNKNOWN`. Syntax-only or framework-heuristic-only Python observations can rank
candidates but cannot prove a family.

### Layer 7: Optional Runtime Trace Refinement

Runtime tracing is deferred and opt-in. A future bounded trace command may
record observed behavior for selected pytest tests or FastAPI app entrypoints,
but observed evidence must be labeled separately, such as
`OBSERVED_SEMANTIC`, and must not be generalized to unobserved executions.

Runtime tracing requires an explicit consent and safety boundary. It must never
run during default `index` or `sync`.

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
- Tree-sitter Python grammar,
  <https://github.com/tree-sitter/tree-sitter-python>.
- Python `ast` module,
  <https://docs.python.org/3/library/ast.html>.
