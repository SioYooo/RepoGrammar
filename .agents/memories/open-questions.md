# Open Questions

- Status: Active
- Last updated: 2026-06-25
- Scope: Design questions intentionally deferred by bootstrap.
- Evidence: Roadmap and bootstrap specifications.
- Related canonical docs: `docs/roadmap.md`, `docs/specifications/indexing-pipeline.md`
- Supersedes: None
- Superseded by: None

## Context

The repository has implemented the repo-local lifecycle, TS/JS discovery,
syntax-only code-unit indexing, and generation-scoped SQLite substrate. The full
MVP remains intentionally deferred.

## Durable knowledge

Open questions include:

- Unified IR shape and information-loss policy.
- Structural fingerprint stability and collision handling.
- Candidate ranking and support thresholds.
- Anti-unification representation for legal variation slots.
- Clustering method and confidence calibration.
- Benchmark corpus and validation methodology.
- Query read-path semantics over active generations.
- Family/evidence storage schema and migration evolution.
- Freshness/worktree hash and stale-evidence refusal behavior.
- FTS5 and source-evidence retention policy.
- MCP response schema stability.
- Optional CodeGraph provider integration mechanism, freshness model, and
  conflict behavior.
- Python v0.1 analyzer implementation details after ADR-0012: exact Python
  worker transport, Pyrefly CLI/LSP adapter shape, provider cache-key storage,
  Pyright cross-check thresholds, fixture corpus selection, and how to surface
  provider disagreement without introducing future certainty tokens early.
- Runtime trace policy and consent boundary if traces are ever used to recover
  unknowns.
- Python v0.1 fixture selection for FastAPI, pytest, SQLAlchemy, and Pydantic,
  including negative fixtures for dynamic imports, monkey patching, fixture
  injection ambiguity, unresolved decorators, and runtime dependency injection.
- Native agent detection and installation receipt format.
- Local telemetry aggregation format and export path.
- Concrete lock-file validation edge cases and cross-platform stale-process
  detection.

## Implications

Agents should not hard-code these answers in implementation without updating the
relevant specification or ADR.

## Revalidation conditions

Update after parser/IR design, mining algorithm design, storage schema design,
or MCP schema design is accepted.
