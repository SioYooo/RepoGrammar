# ADR-0013: Agent adoption and read displacement

- Status: Accepted
- Date: 2026-06-27
- Refines: ADR-0006, ADR-0007, ADR-0011, ADR-0012

## Context

RepoGrammar v0.1 can build bounded Python framework-family evidence and expose
metadata-only read plans through CLI and MCP. That is not enough for daily
agent adoption: coding agents still default to broad grep/read loops unless
the MCP contract tells them when RepoGrammar evidence should be consulted first
and when raw file reads remain required.

The next product slice is not broader static analysis. It is read
displacement: make `repogrammar_context` the first context source for initialized
repositories, return token-budgeted read plans, and optionally render small
line-numbered source spans when explicit source output is requested and the
underlying evidence is fresh.

## Decision

Add an agent-adoption layer for v0.2 with these rules:

- MCP remains read-only and exposes `repogrammar_context` as the default tool.
- MCP initialization and tool descriptions should instruct agents to consult
  RepoGrammar before grep/find/manual file reads in initialized repositories
  when the task is about implementation patterns, family conformance, analogues,
  deviations, or repeated framework behavior.
- Default output remains metadata-first. Source spans are returned only when
  explicitly requested by the caller or CLI flag.
- Rendered source spans must be bounded, line-numbered, hash-checked, and tied
  to existing read-plan evidence. They must never return whole files by default.
- Stale, missing, hash-mismatched, unsupported, insufficient, dynamic, or
  conflicting evidence remains typed `UNKNOWN` or omitted from source rendering.
  The output must tell the agent to use normal Read/Grep for the affected case
  rather than presenting stale source as current.
- Token-budgeted read plans remain first-class. A family summary may be enough;
  source spans are a second step, and whole-file reads are outside RepoGrammar's
  default output contract.
- Telemetry and metrics may record aggregate read-plan/source-span counts only
  under existing opt-in policies. They must not include source text, paths,
  content hashes, query text, symbols, byte ranges, prompts, raw targets, diffs,
  patches, or evidence text.
- Instruction-file integration may be added only through a managed, reversible,
  marker-fenced writer that preserves user content and does not impose
  RepoGrammar's own mirrored guide policy on consuming repositories.

## Consequences

The MCP schema must gain an explicit source-span opt-in instead of overloading
`mode=deep`. CLI query commands need a matching explicit flag so terminal users
can compare metadata-only output against bounded source rendering.

The source renderer must use existing source-store and hash-validation
boundaries. It must not implement a new filesystem reader when the repository
already has `SourceStore` and strict content-hash checks.

Tests should be batched by coherent feature slice: finish the source-span/MCP
slice, review the logic, then run targeted and full validation. Agents must not
run the full test suite after every small helper change.

## Alternatives considered

- Return source snippets by default: rejected because it undermines
  token-budgeted context compression and increases leakage risk.
- Treat `deep` mode as implicit source output: rejected because it would change
  an existing metadata-only contract without explicit caller consent.
- Return stale source with warnings: rejected because agents may still copy or
  rely on stale text. Stale evidence must abstain or require a normal read.
- Add graph-navigation commands: rejected for v0.2 because RepoGrammar remains
  pattern-family-first, not a general code graph UI.

## Follow-up work

- Update `docs/specifications/mcp-api.md` and CLI docs for explicit source-span
  opt-in.
- Implement a bounded source-span renderer over `SourceStore`.
- Add MCP and CLI tests proving metadata-only default behavior, explicit source
  output, line numbering, hash-mismatch/stale omission, and UNKNOWN fallback.
- Add installer-managed instruction guidance only after the writer is
  reversible and covered by tests.
