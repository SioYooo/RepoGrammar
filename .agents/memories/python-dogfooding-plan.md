# Python Dogfooding Plan

- Status: Superseded by `.agents/memories/python-v0.1-algorithm-plan.md`
- Last updated: 2026-06-25
- Scope: Historical record of the former experimental Python boundary.
- Evidence: `docs/decisions/ADR-0009-experimental-python-dogfooding.md`,
  `docs/decisions/ADR-0011-python-first-v0-1.md`
- Related canonical docs: `docs/specifications/python-analysis.md`,
  `docs/plans/python-v0.1-implementation-plan.md`
- Supersedes: None
- Superseded by: `.agents/memories/python-v0.1-algorithm-plan.md`

## Durable knowledge

- ADR-0011 changed the official v0.1 implementation target to Python-first.
  Use `.agents/memories/python-v0.1-algorithm-plan.md` for active guidance.
- This file is retained only to explain the previous experimental-dogfooding
  boundary and why ADR-0011 superseded it.
- The first Python subset remains FastAPI, pytest, SQLAlchemy, and Pydantic;
  Django and C/C++ remain deferred.
- Dynamic imports, monkey patching, decorator rewrites, pytest fixture
  injection, runtime dependency injection, unresolved imports, and framework
  magic should default to typed `UNKNOWN` unless source-backed evidence is
  strong.
- Python-facing docs must not call the current code implemented Python
  analysis until parser/import/framework evidence extraction actually lands.

## Revalidation conditions

Update only if historical context needs correction. Active Python v0.1 guidance
belongs in `.agents/memories/python-v0.1-algorithm-plan.md`.
