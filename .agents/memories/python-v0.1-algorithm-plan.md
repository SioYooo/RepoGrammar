# Python v0.1 Algorithm Plan

- Status: Active memory
- Last updated: 2026-06-25
- Scope: Durable non-normative summary of the Python-first v0.1 algorithm plan
- Evidence: `docs/decisions/ADR-0011-python-first-v0-1.md`,
  `docs/specifications/python-analysis.md`,
  `docs/plans/python-v0.1-implementation-plan.md`
- Supersedes: `.agents/memories/python-dogfooding-plan.md`

## Durable Knowledge

RepoGrammar v0.1 is now Python-first. The target subset is FastAPI, pytest,
SQLAlchemy, and Pydantic. Existing TS/JS bootstrap code remains transitional
substrate, but agents must not describe TS/JS as the official v0.1 target unless
a later ADR changes the scope again.

The accepted Python algorithm stack is:

1. public parser-backed syntax extraction;
2. conservative repo-local import resolution;
3. framework-role extraction;
4. usage-driven fixpoint-lite context propagation;
5. target-centered call recovery;
6. pytest fixture graph construction;
7. Python EC-MVFI-lite family induction;
8. typed `UNKNOWN` governance;
9. optional bounded runtime trace later, never by default.

Do not implement a hand-written Python parser, whole-program call graph, full
sound Python analyzer, LLM-evidence path, default runtime trace, Django adapter,
or C/C++ adapter as part of Python v0.1.

## Claim Discipline

Python syntax and framework heuristics can rank candidates but cannot prove
family membership alone. Family claims require fresh source evidence,
role-compatible semantic/dataflow support, sufficient repeated support, and no
blocking unknown for the emitted claim.

Dynamic imports, monkey patching, unresolved decorators, ambiguous pytest
fixtures, runtime dependency injection, missing dependencies, stale evidence,
conflicting analyzer facts, and insufficient support remain typed `UNKNOWN`.

## Revalidation Conditions

Update this memory after any accepted ADR changes Python scope, framework
subset, runtime trace policy, TypeScript/JavaScript sequencing, or UNKNOWN
governance.
