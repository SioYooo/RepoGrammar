# ADR-0007: Safe installation, progress, metrics, and telemetry contracts

- Status: Accepted
- Date: 2026-06-24

## Context

RepoGrammar needs both machine-level agent integration and repository-level
indexing. These have different safety boundaries. Installation modifies agent
configuration, while initialization and indexing modify repository-local index
state.

## Decision

Separate agent integration commands from repository indexing commands. Installer
commands must support dry runs, scoped targets, reversible receipts, native
agent configuration where available, backups before repair, atomic writes,
self-tests, and marker-fenced optional instruction edits. `install` and
`uninstall` must not create, delete, or rewrite repository-local `.repogrammar/`
indexes.

Initialization and indexing must emit typed progress and atomically activate new
index generations only after validation. `init` creates repository-local state;
`uninit` is responsible for removing it.

Telemetry and research trace consent are separate. Anonymous telemetry uses a
versioned allowlist and must not contain code, paths, repository names, prompts,
symbols, query text, evidence text, credentials, environment variables, or raw
error messages.

Metrics must be classified as `MEASURED`, `DERIVED`, `ESTIMATED`, or
`CAUSAL_EXPERIMENT`.

## Alternatives considered

- Single `init` command for both installation and repository indexing: simpler
  but conflates machine and repository safety boundaries.
- Raw progress percentages and ETAs: familiar but likely fabricated before a
  reliable denominator exists.
- Telemetry opt-in combined with research traces: simpler UX but weaker consent
  separation.

## Consequences

The CLI has explicit `install` and repository lifecycle commands. Repository
lifecycle and syntax-only indexing write paths now operate on repo-local state.
Narrow global Codex and Claude Code MCP registration writes are allowed only
after `--yes`, a read-only MCP self-test, native agent CLI execution, and
RepoGrammar-owned receipt creation. Broader agent integration writes and
telemetry network transport remain deferred until their safety contracts are
implemented.

## Follow-up work

Broaden native agent detection, project-local writes, instruction-file
integration, local telemetry storage, and telemetry export behavior with tests
before enabling those write paths.
