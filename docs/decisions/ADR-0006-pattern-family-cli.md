# ADR-0006: Pattern-family-first CLI

- Status: Accepted
- Date: 2026-06-24

## Context

RepoGrammar's product identity is implementation-pattern families with evidence,
variation points, exceptions, unknowns, and conformance checks. A CLI shaped
around callers, callees, impact, nodes, and exploration would reposition the
tool as generic symbol-graph navigation.

## Decision

The v0.1 CLI is pattern-family-first. Top-level query commands are `find`,
`families`, `family`, `member`, `explain`, `check`, `files`, and `units`.

The CodeGraph-style command names `callers`, `callees`, `impact`, `affected`,
`node`, and `explore` must not be top-level v0.1 commands. If graph navigation
is later needed, it must live under a secondary namespace such as
`repogrammar graph callers`.

## Alternatives considered

- Call-graph-first CLI: familiar to static-analysis users but weakens
  RepoGrammar's pattern-family positioning.
- Mixed top-level graph and family commands: convenient but creates product
  ambiguity and encourages top-k file similarity workflows.

## Consequences

`find` must return family-oriented evidence, not only similar files. `family`,
`explain`, and `check` map directly to MCP pattern-family tools. Documentation,
tests, and future command additions must preserve this identity.

## Follow-up work

Implement real query execution after indexing, storage, and family construction
are available.
