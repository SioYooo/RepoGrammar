# ADR-0005: TypeScript/JavaScript-first MVP with Python as second language

- Status: Accepted
- Date: 2026-06-24

## Context

RepoGrammar's first release should validate the pattern-family representation
rather than maximize language coverage. TypeScript, JavaScript, and Python are
all high-value language targets, but supporting two production language families
in v0.1 would mix language adaptation, framework semantics, and family mining
risk at the same time.

## Decision

RepoGrammar v0.1 officially supports TypeScript and JavaScript only. The first
release optimizes for high-quality pattern-family evidence in TS/JS web
repositories.

Python is the planned second official language. Python may appear before v0.2
only as an experimental adapter and must not be documented as production
support.

The v0.1 framework focus is:

- Express;
- NestJS;
- React;
- Jest;
- Vitest.

The planned Python v0.2 subset is:

- FastAPI;
- pytest;
- SQLAlchemy;
- Pydantic.

Django is deferred until after the FastAPI and pytest subset validates the
language-adapter abstraction.

## Alternatives considered

- TS/JS and Python production support in v0.1: higher market coverage but likely
  shallower family quality and higher `UNKNOWN` rates.
- Python-first MVP: valuable, but Python's dynamic features and broader use-case
  spread make it harder to isolate the pattern-family contribution.
- Tree-sitter-only multi-language MVP: faster language coverage, but would
  reduce RepoGrammar to structural candidates instead of semantic families.

## Consequences

README and product documentation must not claim Python production support in
v0.1. Python adapters before v0.2 must be labeled experimental and should avoid
strong semantic claims. TS/JS work should prioritize semantic evidence through
the TypeScript worker and framework adapters.

## Follow-up work

Define TS/JS benchmark repositories and family-quality criteria. Later, design a
Python experimental adapter and evaluate FastAPI, pytest, SQLAlchemy, and
Pydantic before promoting Python to official support.
