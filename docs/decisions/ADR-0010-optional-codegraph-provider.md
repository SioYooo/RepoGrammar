# ADR-0010: Optional CodeGraph provider

- Status: Accepted
- Date: 2026-06-25

## Context

RepoGrammar is a pattern-family engine, not a generic graph-navigation tool.
ADR-0006 rejects CodeGraph-style top-level commands for v0.1, and ADR-0008
keeps RepoGrammar-derived state under `.repogrammar/`.

Some development environments may already have CodeGraph indexes or tools
available. Those can be useful for diagnostics, comparison, or future auxiliary
evidence, but RepoGrammar must remain usable without CodeGraph.

## Decision

Allow CodeGraph only as an optional provider. RepoGrammar must not require a
`.codegraph/` directory, CodeGraph MCP tool, CodeGraph CLI, or CodeGraph runtime
for default CLI, MCP, CI, indexing, or query behavior.

The provider must be explicitly enabled by configuration, environment, or a
future feature gate. If CodeGraph is unavailable, RepoGrammar must continue with
native discovery, parsing, storage, and fallback behavior.

RepoGrammar must not create, initialize, modify, or delete `.codegraph/`.
CodeGraph-derived data, if used, must be translated into RepoGrammar-owned facts
or evidence with provider, version, freshness, and source provenance.

CodeGraph evidence is auxiliary. It must not override repository source,
RepoGrammar storage, language-native semantic workers, or freshness checks.
Conflicts must become typed `UNKNOWN` or abstention.

## Alternatives considered

- Make CodeGraph mandatory: rejected because it adds an external local index
  dependency and weakens RepoGrammar's standalone product boundary.
- Ignore CodeGraph entirely: simpler, but misses a useful optional development
  and comparison source.
- Add top-level graph commands: rejected by ADR-0006 for v0.1.

## Consequences

Default tests must use deterministic fixtures or RepoGrammar-native behavior and
must not fail when CodeGraph is absent. Optional provider tests may skip when the
provider is unavailable.

The CLI remains pattern-family-first. If graph navigation is ever added, it must
live under a secondary namespace such as `repogrammar graph ...` and must not be
presented as the primary value proposition.

`.codegraph/` remains generated local state outside RepoGrammar ownership.
Repository guard may ignore it, but RepoGrammar lifecycle commands must not
manage it.

## Follow-up work

Define the provider boundary, opt-in configuration, unavailable-provider error
shape, provenance fields, freshness checks, and fixture strategy before adding
any executable integration.
