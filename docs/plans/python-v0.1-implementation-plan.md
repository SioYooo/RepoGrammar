# Python v0.1 Implementation Plan

- Status: Active planning artifact
- Last updated: 2026-06-25
- Scope: Python-first v0.1 implementation coordination
- Canonical algorithm spec: `docs/specifications/python-analysis.md`
- Supersedes: `docs/plans/python-dogfooding-plan.md`

## Goal

Make Python the official v0.1 implementation target for RepoGrammar while
preserving the existing EC-MVFI discipline: repository-local pattern-family
evidence, source-backed claims, and typed `UNKNOWN` for unsupported facts.

The first Python slice targets FastAPI, pytest, SQLAlchemy, and Pydantic. It
does not attempt full Python semantic analysis.

## Current Reality

The current codebase still contains a TypeScript/JavaScript-oriented bootstrap:
TS/JS discovery, syntax-only extraction, framework-role facts, TypeScript
worker protocol scaffolding, SQLite generation storage, FamilyStore-backed
query reads, and conservative EC-MVFI-lite gates.

That substrate is useful and should not be thrown away, but the next v0.1
implementation work should pivot toward Python. Existing TS/JS behavior must be
described as transitional until a later release re-promotes it.

## Optimized Phase Sequence

### Phase P0: Scope Pivot and Planning Registry

Goal: align ADRs, specs, plans, memories, README, and mirrored root guides with
Python-first v0.1.

Non-goals: parser implementation, storage migration, worker runtime, or new
query behavior.

Validation gate: repo-guard, guide equality, docs link review, and no
overclaim that Python is already implemented.

### Phase P1: Python Language Boundary

Goal: add the owned language/support-level model needed for Python without
leaking parser or Python runtime types into `core`.

Non-goals: full parser, whole-program call graph, runtime tracing.

Agent ownership: architecture/domain agents own Rust model and port boundaries;
test agents own support-level and `UNKNOWN` regression tests.

Validation gate: Python can be represented as the v0.1 target in domain types,
but unimplemented analysis remains fallback or typed `UNKNOWN`.

### Phase P2: Python Discovery and Module Graph

Goal: discover `.py` files, package roots, `__init__.py`, safe project config,
and deterministic repo-local module names.

Non-goals: full `sys.path` emulation, dynamic import execution, dependency
installation.

Validation gate: deterministic paths and hashes, skip virtual environments and
generated/dependency directories, no source snippets or absolute paths.

### Phase P3: Syntax Extraction

Goal: extract Python modules, functions, async functions, classes, methods,
decorators, imports, assignments, calls, annotations, and class bases through
public parsers.

Non-goals: hand-written parser, semantic certainty from syntax alone.

Validation gate: malformed syntax produces bounded diagnostics and partial
structural output; syntax-only output cannot create family claims.

### Phase P4: Import and Framework Role Evidence

Goal: implement repo-local import/alias resolution plus framework-role
extraction for FastAPI, pytest, SQLAlchemy, and Pydantic.

Non-goals: Django, plugin execution, runtime DI resolution, full type checker.

Validation gate: unique static evidence produces framework/context facts;
dynamic or ambiguous evidence produces typed `UNKNOWN`.

### Phase P5: Usage Propagation and Target-centered Calls

Goal: add fixpoint-lite role propagation and application-centered call recovery
for family compatibility.

Non-goals: whole-program call graph, sound alias analysis, complete symbolic
execution.

Validation gate: recovery is bounded, deterministic, and role-specific; unknown
targets remain visible as typed `UNKNOWN`.

### Phase P6: Python EC-MVFI-lite Families

Goal: build Python family records only from repeated compatible candidates with
fresh source evidence and no blocking unknown for the emitted claim.

Non-goals: full anti-unification, broad clustering, token-savings claims.

Validation gate: positive fixtures prove at least one FastAPI/pytest/Pydantic
or SQLAlchemy family can be emitted; negative fixtures prove dynamic imports,
fixture ambiguity, stale evidence, and insufficient support abstain.

### Phase P7: Query, MCP, and Release Smoke

Goal: expose Python family evidence through existing pattern-family CLI and MCP
contracts without changing the product into graph navigation.

Non-goals: new top-level graph commands, automatic user-code edits, runtime
trace by default.

Validation gate: human and JSON outputs distinguish fallback, stale evidence,
typed `UNKNOWN`, `CONTEXT_ONLY`, and confident family classifications.

## Required Python Fixtures

Fixtures should live under `src/fixtures/python/` once implementation begins.

Minimum positive fixture groups:

- FastAPI route families with `APIRouter`, `Depends`, response models, service
  calls, and error mapping.
- pytest fixture/test families with `conftest.py` hierarchy and parametrized
  tests.
- Pydantic model families with fields, validators, config, and response schema
  reuse.
- SQLAlchemy repository/model families with session usage and transaction
  boundaries.

Minimum negative fixture groups:

- dynamic imports;
- monkey patching;
- unresolved decorators;
- ambiguous pytest fixtures;
- runtime dependency injection;
- missing dependencies or project config;
- stale evidence;
- low support.

## Agent Ownership

- Context Agent: ADR/spec/root-guide consistency and claim discipline.
- Algorithm Agent: parser/import/framework/call/family method review.
- Implementation Agent: one owned Rust/Python boundary at a time under `src/`.
- Test Agent: positive, negative, stale, conflicting, and dynamic fixtures.
- Docs Agent: canonical specs, README, changelog, ADRs, and memories.
- Review Agent: false positives, false negatives, source/path leakage,
  dependency creep, and unsupported certainty.

Do not split agents across overlapping files unless the coordinator controls
integration in the main session.

## Commit Discipline

Use small Conventional Commits. Each implementation commit must include tests
and docs for the behavior it changes. Planning-only commits must not add parser,
storage, worker, runtime-trace, or mining implementation.

## Deferred Work

- TypeScript/JavaScript production-quality family evidence after Python v0.1.
- Django support.
- C/C++ support.
- Optional CodeGraph provider runtime integration.
- Optional bounded runtime tracing.
- LLM-assisted explanation or docs generation.
