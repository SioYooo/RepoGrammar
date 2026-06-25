# Python v0.1 Implementation Plan

- Status: Active planning artifact
- Last updated: 2026-06-25
- Scope: Python-first v0.1 implementation coordination
- Canonical algorithm spec: `docs/specifications/python-analysis.md`
- Provider cascade decision: `docs/decisions/ADR-0012-python-selective-analysis-cascade.md`
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

ADR-0012 refines the Python route as a claim-driven selective cascade. Do not
run all analyzers over the whole repository. Start with cheap CPython syntax,
scope, and config facts; escalate only candidate groups and claim-upgrading
facts to Pyrefly, Pyright, bounded propagation, call recovery, or optional
observed runtime evidence.

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

### Phase P2: Python Discovery and Authoritative Frontend

Goal: discover `.py` files, package roots, `__init__.py`, safe project config,
deterministic repo-local module names, and CPython `ast`/`symtable`/`tomllib`
frontend output.

Non-goals: full `sys.path` emulation, dynamic import execution, dependency
installation, or hand-written parsing.

Validation gate: deterministic paths and hashes, skip virtual environments and
generated/dependency directories, no source snippets or absolute paths, Python
version recorded in provenance, and malformed Python producing bounded
diagnostics.

Current progress: `.py` discovery, CPython `ast` code-unit extraction,
path-derived module-name anchors, CPython `symtable` structural scope anchors,
private `tomllib` project-config summaries, and semantic-worker-compatible
project-mode module-level repo-local import resolution are implemented. Default
indexing now passes discovered repo-relative `.py` inventory into private
parse-document requests so source-tied repo-local import facts can be persisted
as structural parser-origin facts. Default indexing also discovers root
`pyproject.toml`, reads it through the Rust source-store boundary, and persists
only a `python-config`/`project_config` structural summary or typed config
`UNKNOWN`; these records are not provider facts and cannot become family claim
input. The worker performs file-local simple FastAPI router/app alias
propagation with same-name top-level reassignment invalidation, and the
application layer derives separate `DATAFLOW_DERIVED` support facts from exact
canonical CPython anchors when a unit has one Python framework role; raw parser
facts and framework heuristics still remain blocked from direct claim input.
The implemented SQLAlchemy slice now includes exact structural anchors for
`Mapped[...]`, `mapped_column(...)`, and calls on parameters typed as
`Session` or `AsyncSession`, keeping those facts provider-unresolved but
eligible for the bounded exact-anchor derivation gate.

### Phase P3: Tree-sitter Fallback and Code-unit Emission

Goal: emit RepoGrammar-owned Python code units, structural facts, semantic
anchors, and diagnostics from the Python worker. Use Tree-sitter only as a
tolerant fallback for syntax errors, incomplete files, or worker
unavailability.

Non-goals: semantic certainty from syntax alone, duplicate primary parsing in
Rust and Python, or treating Tree-sitter as a Python semantic frontend.

Validation gate: malformed syntax produces bounded diagnostics and partial
structural output; syntax-only output cannot create family claims.

### Phase P4: Pyrefly Provider and Framework Role Evidence

Goal: implement repo-local import/alias resolution, Pyrefly primary provider
queries, typed canonical framework identity, and framework-role extraction for
FastAPI, pytest, SQLAlchemy, and Pydantic. Add selective Pyright cross-checks
only for facts that would upgrade a family claim or materially change a
variation/exception.

Current progress: a pre-provider exact-anchor derivation slice is implemented
for canonical Python framework targets already emitted by the CPython frontend.
It records `provider_resolved=false` `DATAFLOW_DERIVED` support. The Rust ports
layer also defines future candidate-scoped Python provider request,
provenance, cache-key, and unavailable-UNKNOWN boundaries. Pyrefly,
Pyright, and RightTyper adapter execution remain deferred.

Non-goals: Django, plugin execution, runtime DI resolution, whole-project
dual-provider analysis, private Pyrefly API use, or substring framework
compatibility.

Validation gate: unique static evidence produces framework/context facts with
provider provenance and freshness; Pyrefly/Pyright disagreement produces
`ConflictingFacts`; dynamic or ambiguous evidence produces typed `UNKNOWN`.

### Phase P5: Usage Propagation and Target-centered Calls

Goal: add bounded fixpoint-lite role propagation, Pyrefly call hierarchy, and
JARVIS-lite fallback recovery for family compatibility.

Non-goals: whole-program call graph, sound alias analysis, complete symbolic
execution.

Validation gate: recovery is bounded, deterministic, and role-specific; unknown
targets remain visible as typed `UNKNOWN`; site-packages traversal stays
disabled by default.

### Phase P6: Python EC-MVFI-lite Families

Goal: build Python family records only from repeated compatible candidates with
fresh source/provider/config evidence and no blocking unknown for the emitted
claim.

Non-goals: broad clustering, single-link chaining, token-savings claims, or
neural/LLM evidence.

Validation gate: positive fixtures prove at least one FastAPI/pytest/Pydantic
or SQLAlchemy family can be emitted; negative fixtures prove dynamic imports,
fixture ambiguity, stale evidence, provider conflict, substring false matches,
and insufficient support abstain. Clustering is complete-link constrained, and
template extraction uses Aroma-style intersection only after hard gates pass.

### Phase P7: Token Selector, Query, MCP, and Release Smoke

Goal: expose Python family evidence through existing pattern-family CLI and MCP
contracts without changing the product into graph navigation. Default output is
`compact`; `evidence` and `deep` modes expand only under explicit query or
budget.

Non-goals: new top-level graph commands, automatic user-code edits, runtime
trace by default, or source snippets in compact/evidence output.

Validation gate: human and JSON outputs distinguish fallback, stale evidence,
typed `UNKNOWN`, `CONTEXT_ONLY`, and confident family classifications.

Current slice: CLI and MCP now share compact/evidence/deep output selection for
stored family evidence metadata. `deep` is still metadata-only and reports no
source snippets until a safe source-span reader is implemented. The current
builder can link one narrow Python variation evidence record when an
already-ready family has multiple exact-compatible framework-anchor support
targets; broader variation, medoid, and exception evidence remain deferred.

### Phase P8: Optional Observed Runtime Evidence

Goal: add an explicit, bounded RightTyper-style observed evidence path only
after static family evidence exists.

Non-goals: default runtime execution, mutation of user source, generalizing
observed behavior to unexecuted paths, or using runtime evidence as universal
truth.

Validation gate: command/test provenance, coverage/run id, Python version,
environment hash, source hash, and no-mutation behavior are recorded; observed
facts are labeled separately and never bypass static conflict/UNKNOWN gates.

## Required Python Fixtures

Initial release-smoke fixtures now live under `src/fixtures/python/release/v0_1/`.
They cover low-support and dynamic `UNKNOWN` behavior, a no-worker exact-anchor
FastAPI family path through derived support, and a test-only strong FastAPI
semantic-support fixture for explicit worker family read and stale-evidence
smoke coverage. They also include a no-worker FastAPI exact-anchor target
variation fixture and SQLAlchemy session exact-anchor fixtures covering
`execute`, direct `scalar`/`scalars` calls, and direct sync/async
`commit`/`rollback` transaction anchors, plus an alias-aware pytest fixture
exact-anchor smoke fixture. They are not yet the full Python provider corpus.

Minimum positive fixture groups:

- FastAPI route families with `APIRouter`, `Depends`, response models, service
  calls, and error mapping.
- pytest fixture/test families with `conftest.py` hierarchy and parametrized
  tests.
- Pydantic model/settings families with fields, validators, config, and
  response schema reuse.
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
- provider disagreement;
- substring framework-name false positives;
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
- Provider-backed Python project-configuration semantics beyond the current
  structural `pyproject.toml` summary records.
- Django support.
- C/C++ support.
- Optional CodeGraph provider runtime integration.
- Optional bounded runtime tracing.
- LLM-assisted explanation or docs generation.
