# ADR-0012: Python selective analysis cascade

- Status: Accepted
- Date: 2026-06-25
- Refines: ADR-0011

## Context

ADR-0011 makes Python the official v0.1 language target. The next decision is
how to turn Python's broad analyzer ecosystem into RepoGrammar family evidence
without running every tool on every file or turning analyzer output into false
certainty.

Python analysis tools are good at different jobs: CPython exposes syntax and
compiler scope facts, Pyrefly and Pyright provide static type and navigation
facts, RightTyper can observe runtime types, and clone/retrieval research
informs grouping and context selection. RepoGrammar's product target is not a
full static analyzer. It is precision-first, repository-local family evidence
and token-budget-aware summaries.

## Decision

Use a claim-driven selective cascade for Python v0.1:

```text
CPython ast + symtable + tomllib
-> low-cost structural candidate grouping
-> Pyrefly only for candidates that may become families
-> selective Pyright cross-check for claim-upgrading facts
-> bounded framework-role propagation
-> Pyrefly call hierarchy, then JARVIS-lite fallback when needed
-> optional RightTyper observed evidence behind explicit opt-in
-> precision-first constrained family induction
-> selective retrieval and token-budget evidence selection
```

The default frontend is CPython `ast`, `symtable`, and `tomllib`. Tree-sitter
Python is a fallback for syntax errors, incomplete files, or worker
unavailability. It must not become the primary Python semantic frontend.

Pyrefly is the primary static semantic provider for Python v0.1, accessed only
through public CLI/LSP-style boundaries. RepoGrammar must pin the exact Pyrefly
version and record provider version, Python version, provider config hash,
environment fingerprint, source content hash, source range, and query
operation for cached provider facts. Pyrefly internal Rust crates or private
data structures must not enter RepoGrammar core.

Pyright is a selective cross-check provider for facts that would upgrade a
candidate into a family claim. RepoGrammar must not run Pyright over the whole
project by default when Pyrefly has already supplied candidate facts. If
Pyrefly and Pyright disagree on a claim-upgrading fact, the affected claim
becomes `CONFLICTING` or typed `UNKNOWN`. If Pyright is unavailable, a Pyrefly
fact may remain usable only under stricter support thresholds.

Framework compatibility must use typed canonical identities, not substring
matching. A framework role can be upgraded only when a provider or bounded
propagation resolves canonical framework facts such as:

- FastAPI route decorators resolving to `fastapi.FastAPI.*` or
  `fastapi.APIRouter.*` route methods;
- pytest fixtures resolving to `pytest.fixture` or a provider-resolved fixture
  binding;
- Pydantic models resolving to `pydantic.BaseModel` or
  `pydantic_settings.BaseSettings`;
- SQLAlchemy 2.0 typed mappings resolving to `sqlalchemy.orm.DeclarativeBase`,
  `Mapped`, or `mapped_column`.

The first bounded propagation slice may run before Pyrefly by deriving separate
`DATAFLOW_DERIVED` support facts from validated CPython structural anchors, but
only when the target exact-matches this compatibility table, the code unit has
one Python framework role, and the derived fact records
`provider_resolved=false`. Raw `STRUCTURAL` parser facts and
`FRAMEWORK_HEURISTIC` role facts remain insufficient by themselves.

RightTyper-style runtime evidence is deferred and optional. It may produce
observed evidence after an explicit command, but observed facts cannot prove
universal runtime equivalence or replace static freshness/provenance checks.

Family induction must be precision-first: hard gates, typed canonical
framework identity, support thresholds, complete-link constrained clustering,
Aroma-style template intersection, and budgeted evidence selection. RepoGrammar
should output compact family summaries by default and return source spans only
when explicitly requested.

## Consequences

Implementation must add typed Python framework fact schemas before Python facts
can become claim inputs. The existing TS/JS compatibility helper that inspects
fact text must not be copied into Python.

Provider facts require freshness and provenance storage. Cache keys must include
provider identity and configuration, Python version, environment fingerprint,
file content hash, source range, and query operation.

`CROSS_CHECKED_SEMANTIC` and `OBSERVED_SEMANTIC` may be introduced as future
certainty tiers only with matching Rust domain, schema, CLI, MCP, storage, and
test updates. Until then, documents must describe them as planned Python
semantics, not emitted protocol tokens.

## Alternatives considered

- Run all analyzers over the whole repository: rejected because it increases
  cost, noise, and token pressure without necessarily improving family claims.
- Make Pyright the only provider: mature and standards-oriented, but Pyrefly's
  current LSP feature set and framework support better match Python v0.1
  candidate verification.
- Depend on RightTyper by default: rejected because runtime tracing executes
  user code and observes only covered behavior.
- Use neural type prediction or LLM semantic inference as evidence: rejected
  because predictions are not auditable static facts.

## Follow-up work

- Update `docs/specifications/python-analysis.md` with the selective cascade.
- Update implementation planning to add Pyrefly provider/caching after the
  CPython frontend.
- Add tests proving Python framework compatibility never uses framework-name
  substring matching.
- Add future protocol/domain changes before emitting cross-checked or observed
  certainty tokens.
