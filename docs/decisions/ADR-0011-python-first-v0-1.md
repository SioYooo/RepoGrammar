# ADR-0011: Python-first v0.1

- Status: Accepted
- Date: 2026-06-25
- Supersedes: ADR-0005, ADR-0009

## Context

RepoGrammar's product value is repository-local implementation-pattern family
evidence for coding agents. The previous v0.1 language decision optimized for
TypeScript/JavaScript first and kept Python as experimental dogfooding.

The maintainer has changed the v0.1 implementation target to Python. Python's
dynamic behavior makes full static analysis unrealistic for v0.1, but Python
backend and test repositories have high-value recurring framework patterns in
FastAPI, pytest, SQLAlchemy, and Pydantic. Those patterns fit RepoGrammar's
evidence-constrained family model when dynamic gaps remain typed `UNKNOWN`.

## Decision

RepoGrammar v0.1 is Python-first. The official v0.1 language target is Python,
focused on:

- FastAPI;
- pytest;
- SQLAlchemy;
- Pydantic.

The v0.1 Python analyzer must be a pragmatic, evidence-constrained method stack
instead of a full Python analyzer:

- public parser-backed syntax extraction;
- conservative repo-local import resolution;
- framework-role extraction;
- usage-driven fixpoint-lite context propagation;
- target-centered call recovery;
- pytest fixture graph construction;
- EC-MVFI-lite family induction;
- typed `UNKNOWN` for unresolved dynamic behavior.

The existing TypeScript/JavaScript bootstrap remains useful substrate and may be
maintained, but it is no longer the official v0.1 implementation target. TS/JS
production-quality family evidence is deferred until after the Python v0.1
checkpoint unless a later ADR changes the sequence again.

## Alternatives considered

- Keep TypeScript/JavaScript first: lower disruption, but no longer matches the
  maintainer's v0.1 product direction.
- Support TS/JS and Python equally in v0.1: broader appeal, but too likely to
  dilute evidence quality and increase false certainty.
- Build a full Python static analyzer: attractive academically, but not needed
  for RepoGrammar's pattern-family compression goal and too expensive for v0.1.
- Use LLM output as static-analysis evidence: rejected because it lacks
  auditable provenance and completeness guarantees for family membership.
- Default runtime tracing: rejected for v0.1 because observed behavior is
  environment-dependent and covers only executed paths.

## Consequences

Root guides, README, roadmap, product specs, semantic-worker specs, plans, and
memories must describe Python as the v0.1 target and must stop labeling Python
as merely experimental dogfooding.

Docs and implementation must still be clear about current reality: existing
code is transitional and still contains TS/JS bootstrap pieces until Python
discovery, parsing, framework facts, and family fixtures are implemented.

Python family claims require source-backed, fresh, role-compatible evidence.
Syntax-only Python facts, unresolved imports, dynamic behavior, ambiguous pytest
fixtures, runtime dependency injection, and conflicting analyzer facts must
remain typed `UNKNOWN`.

Optional Python runtime traces, if added later, must be explicit and bounded;
they produce observed evidence only and must not be generalized to unobserved
behavior.

## Follow-up work

- Keep `docs/specifications/python-analysis.md` as the canonical Python
  analysis algorithm spec.
- Use `docs/plans/python-v0.1-implementation-plan.md` for phase sequencing.
- Add Python fixtures under `src/fixtures/python/` when implementation begins.
- Update repo-guard required documents so the Python v0.1 ADR, spec, plan, and
  memory cannot silently disappear.
