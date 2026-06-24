# Open Questions

- Status: Active
- Last updated: 2026-06-24
- Scope: Design questions intentionally deferred by bootstrap.
- Evidence: Roadmap and bootstrap specifications.
- Related canonical docs: `docs/roadmap.md`, `docs/specifications/indexing-pipeline.md`
- Supersedes: None
- Superseded by: None

## Context

The repository is intentionally not implementing the full MVP during bootstrap.

## Durable knowledge

Open questions include:

- Unified IR shape and information-loss policy.
- Structural fingerprint stability and collision handling.
- Candidate ranking and support thresholds.
- Anti-unification representation for legal variation slots.
- Clustering method and confidence calibration.
- Benchmark corpus and validation methodology.
- SQLite schema and migration format.
- MCP response schema stability.
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
