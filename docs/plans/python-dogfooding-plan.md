# Python Dogfooding Plan

- Status: Superseded by `docs/plans/python-v0.1-implementation-plan.md`
- Last updated: 2026-06-25
- Scope: Historical record of the previous experimental Python boundary

ADR-0011 promotes Python to the official v0.1 implementation target. This file
is retained for historical context only. Use
`docs/specifications/python-analysis.md` and
`docs/plans/python-v0.1-implementation-plan.md` for current Python v0.1 work;
the refined provider cascade is recorded in
`docs/decisions/ADR-0012-python-selective-analysis-cascade.md`.

## Boundary

This historical boundary treated Python as opt-in dogfooding only. That is no
longer the active v0.1 scope.

## First Subset

Initial dogfooding targets:

- FastAPI routes and dependency declarations;
- pytest tests, fixtures, setup, and assertions;
- SQLAlchemy model and session patterns;
- Pydantic schemas and validation models.

Deferred:

- Django;
- C/C++;
- sound full Python semantic analysis;
- production readiness claims.

## Evidence Rules

The first usable Python dogfooding target is syntax and framework-role evidence,
not sound full semantic analysis.

Python facts must become typed `UNKNOWN` unless supported by sufficient
source-backed evidence. Common reason codes include:

- `DynamicImport`;
- `MonkeyPatch`;
- `PytestFixtureInjection`;
- `RuntimeDependencyInjection`;
- `UnresolvedImport`;
- `FrameworkMagic`;
- `MissingDependency`;
- `ConflictingFacts`;
- `InsufficientSupport`.

Syntax-only Python observations are structural candidates. They cannot prove
semantic equivalence, route conformance, fixture binding, ORM transaction
behavior, or production family membership by themselves.

## Historical Dogfooding Steps

These steps are superseded by `docs/plans/python-v0.1-implementation-plan.md`;
they remain here only to explain the older boundary.

1. Define an experimental language flag and adapter boundary after repo-local
   lifecycle and language/provider abstraction are stable.
2. Add deterministic fixtures for FastAPI, pytest, SQLAlchemy, and Pydantic.
3. Extract functions, classes, methods, decorators, route declarations, pytest
   tests/fixtures, Pydantic models, and rough SQLAlchemy model/session roles.
4. Emit typed `UNKNOWN` for dynamic imports, monkey patching, unresolved
   decorators, fixture injection, runtime dependency injection, and analyzer
   disagreement.
5. Evaluate whether Pyright, Mypy, an LSP, optional CodeGraph facts, user hints,
   or bounded runtime traces should refine specific unknowns.
6. Write a future ADR before changing Python's support level. ADR-0011 is that
   superseding decision for v0.1.

## Historical Promotion Criteria

The former plan required these conditions before official Python support:

- an accepted ADR changes the language scope, now ADR-0011;
- a stable fixture suite covers the target frameworks;
- dynamic limits and `UNKNOWN` reasons are documented;
- semantic facts have clear provenance and freshness;
- CLI, MCP, README, and release notes can describe the support level without
  overclaiming.
