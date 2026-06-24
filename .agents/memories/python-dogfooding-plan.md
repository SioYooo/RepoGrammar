# Python Dogfooding Plan

- Status: Active
- Last updated: 2026-06-25
- Scope: Non-normative guidance for future experimental Python validation.
- Evidence: `docs/decisions/ADR-0009-experimental-python-dogfooding.md`
- Related canonical docs: `docs/decisions/ADR-0005-ts-js-first-mvp.md`, `docs/specifications/product.md`
- Supersedes: None
- Superseded by: None

## Durable knowledge

- v0.1 production scope remains TypeScript/JavaScript only.
- Python before v0.2 is experimental dogfooding, not production support.
- The first Python subset should stay focused on FastAPI, pytest, SQLAlchemy,
  and Pydantic; Django and C/C++ remain deferred.
- Python dogfooding should test whether the adapter, worker, provenance, and
  `UNKNOWN` model generalizes. It must not bypass the TS/JS-first MVP.
- Dynamic imports, monkey patching, decorator rewrites, pytest fixture
  injection, runtime dependency injection, unresolved imports, and framework
  magic should default to typed `UNKNOWN` unless source-backed evidence is
  strong.
- Any Python-facing docs must label support as experimental until an accepted
  v0.2 adapter exists.

## Revalidation conditions

Update when a Python adapter design, fixture set, semantic-worker strategy, or
v0.2 ADR is accepted.
