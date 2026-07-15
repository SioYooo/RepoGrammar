---
name: telemetry-and-metrics
description: Use for telemetry consent, telemetry schemas, metric reporting, token measurement, stats, or research trace changes.
---

# Purpose

Keep telemetry privacy-preserving and metric claims auditable.

# Trigger conditions

Use when editing `repogrammar stats`, `repogrammar telemetry`, telemetry ports,
anonymous telemetry schema, research trace collection, or token metrics.

# Required reading

- `docs/specifications/telemetry.md`
- `docs/specifications/metrics.md`
- `docs/decisions/ADR-0007-safe-install-progress-telemetry.md`

# Preconditions

- Separate anonymous product telemetry from research trace consent.
- Confirm telemetry is disabled in CI and by environment opt-out.
- Confirm metric kind is one of `MEASURED`, `DERIVED`, `ESTIMATED`, or
  `CAUSAL_EXPERIMENT`.

# Step-by-step procedure

1. Use the versioned allowlist schema.
2. Reject code, paths, repository names, symbols, prompts, query text, evidence
   text, environment variables, credentials, and raw error messages.
3. Use coarse buckets and typed error codes.
4. Avoid telemetry latency on MCP calls.
5. Honor `REPOGRAMMAR_TELEMETRY=0` and `DO_NOT_TRACK=1`.
6. Do not report derived context compression as actual token savings.
7. Require a comparable baseline before reporting token savings.

# Required verification

Run unit tests for consent, environment disablement, schema allowlist, and
metric classification.

# Documentation updates

Update telemetry and metrics specifications with any schema or claim change.

# Commit requirements

Commit code, tests, and docs together. Do not commit local telemetry exports.

# Completion report

Report schema version, consent behavior, metric kinds, and any disabled data
collection paths.

# Failure and rollback handling

If a telemetry field could reveal source, user, repository, or prompt content,
remove it before merging.
