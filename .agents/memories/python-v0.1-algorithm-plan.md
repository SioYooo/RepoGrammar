# Python v0.1 Algorithm Plan

- Status: Active memory
- Last updated: 2026-06-25
- Scope: Durable non-normative summary of the Python-first v0.1 algorithm plan
- Evidence: `docs/decisions/ADR-0011-python-first-v0-1.md`,
  `docs/decisions/ADR-0012-python-selective-analysis-cascade.md`,
  `docs/specifications/python-analysis.md`,
  `docs/plans/python-v0.1-implementation-plan.md`
- Supersedes: `.agents/memories/python-dogfooding-plan.md`

## Durable Knowledge

RepoGrammar v0.1 is now Python-first. The target subset is FastAPI, pytest,
SQLAlchemy, and Pydantic. Existing TS/JS bootstrap code remains transitional
substrate, but agents must not describe TS/JS as the official v0.1 target unless
a later ADR changes the scope again.

The accepted Python algorithm stack is the ADR-0012 claim-driven selective
cascade:

1. CPython `ast`, `symtable`, and `tomllib` for primary syntax, scope, and
   config facts;
2. low-cost structural and framework-anchor candidate grouping;
3. Pyrefly through public CLI/LSP-style boundaries only for plausible family
   candidates;
4. Pyright cross-checks only for claim-upgrading facts;
5. typed canonical framework identities instead of framework-name substring
   matching;
6. usage-driven fixpoint-lite context propagation;
7. Pyrefly call hierarchy first, then JARVIS-lite bounded call recovery when
   needed;
8. Python EC-MVFI-lite family induction with complete-link constrained
   clustering and Aroma-style template intersection;
9. selective compact/evidence/deep output with token-budget evidence selection
   and metadata-only read plans;
10. optional RightTyper-style observed evidence later, explicit opt-in only.

Do not implement a hand-written Python parser, whole-program call graph, full
sound Python analyzer, LLM-evidence path, default runtime trace, Django adapter,
or C/C++ adapter as part of Python v0.1.

## Claim Discipline

Python syntax and framework heuristics can rank candidates but cannot prove
family membership alone. Family claims require fresh source evidence,
role-compatible semantic/dataflow support, sufficient repeated support, and no
blocking unknown for the emitted claim.

Future cross-checked or observed semantic certainty labels are not current Rust,
protocol, CLI, MCP, or storage tokens. Until those layers change together,
cross-check and observed-runtime details must remain in assumptions/provenance.

Current matched family output is metadata-only. CLI and MCP may return
repo-relative paths, strict content hashes, byte ranges, purpose labels,
estimated token costs, and code-unit/family ids, but not source snippets or
absolute paths. The target source body is still required reading before edits.

Dynamic imports, monkey patching, unresolved decorators, ambiguous pytest
fixtures, runtime dependency injection, missing dependencies, stale evidence,
conflicting analyzer facts, and insufficient support remain typed `UNKNOWN`.

## Revalidation Conditions

Update this memory after any accepted ADR changes Python scope, framework
subset, runtime trace policy, TypeScript/JavaScript sequencing, or UNKNOWN
governance.
