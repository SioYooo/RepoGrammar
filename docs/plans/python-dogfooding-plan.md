# Python Dogfooding Plan

- Status: Active planning artifact
- Last updated: 2026-06-25
- Scope: Experimental internal validation only

Python work before v0.2 is experimental dogfooding. It does not change the
official v0.1 TypeScript and JavaScript support claim.

## Boundary

Python may be used to test whether RepoGrammar's language-adapter,
semantic-worker, provenance, provider, and `UNKNOWN` model generalizes beyond
TS/JS. It must remain opt-in, clearly labeled experimental, and excluded from
default production-support claims.

Documentation must use "experimental Python dogfooding" or equivalent wording.
Do not describe this as unqualified "Python support" until a later ADR accepts a
focused production adapter.

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

## Dogfooding Steps

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
6. Write a future v0.2 ADR before promoting Python beyond experimental
   dogfooding.

## Promotion Criteria

Python can be considered for official support only after:

- an accepted ADR changes the language scope;
- a stable fixture suite covers the target frameworks;
- dynamic limits and `UNKNOWN` reasons are documented;
- semantic facts have clear provenance and freshness;
- CLI, MCP, README, and release notes can describe the support level without
  overclaiming.
