# ADR-0018: Query discovery and hydration routing

- Status: Accepted
- Date: 2026-07-05
- Refines: ADR-0006 (pattern-family-first CLI), ADR-0013 (agent adoption and read
  displacement)

## Context

RepoGrammar asks coding agents to consult pattern-family context before broad
grep/read loops, but an agent often starts with a file path, symbol, framework
role, or natural-language pattern question. Requiring the agent to already know a
family id makes RepoGrammar useful only after a separate discovery step and
pushes agents back toward generic code search.

The existing fuzzy query implementation already performs bounded candidate
discovery and hydration. The public contract needs to make that loop explicit so
family ids are treated as follow-up handles, not prerequisites for the first
query.

## Decision

For `find_analogues`, `explain_deviation`, and `check_conformance`, the public
query mode is `discover -> hydrate -> compose`:

- accept the repo-relative path, symbol/member id, framework role, exact id, or
  pattern question the caller has;
- discover candidate family ids internally through bounded indexes;
- hydrate only the bounded candidate set;
- compose a family context bundle, a `PARTIAL_CONTEXT` read plan, or a typed
  `UNKNOWN` without guessing.

`show_family` remains exact-family-id only. Family ids returned by fuzzy
operations are follow-up handles for exact inspection, not required initial
inputs and not conformance evidence by themselves.

CLI and MCP lookup responses must expose source-free `query_route` metadata with
the route name, input kind, pipeline, family-id policy, candidate limit, selected
family id, candidate family ids, follow-up family ids, and selection rationale.
`selected_family_id` is present only after a supported family has actually been
selected. Candidate and follow-up family ids on `PARTIAL_CONTEXT` or `UNKNOWN`
are narrowing handles, not family claims.

## Consequences

Agent instructions should tell agents to pass the path, symbol/member id,
framework role, or pattern question they already have for fuzzy operations, use
`show_family` only for exact family ids returned earlier, and rely on
`read_plan` plus typed `UNKNOWN` boundaries before editing.

Tests and docs must treat route metadata as part of the public CLI/MCP output
contract. Telemetry remains aggregate-only and must not record raw targets,
paths, symbols, family ids, source text, or query-route payloads.

## Alternatives considered

- Require agents to call `families` first and choose a family id manually:
  rejected because it reintroduces broad inventory scanning and makes
  RepoGrammar less useful as a first context source.
- Add CodeGraph-style top-level `explore` or `impact` commands: rejected because
  ADR-0006 keeps RepoGrammar pattern-family-first.
- Infer one family from ambiguous fuzzy matches: rejected because it would turn
  insufficient evidence into a false family or conformance claim.

## Follow-up work

- Improve candidate discovery beyond exact path/role anchors while preserving
  bounded hydration and typed `UNKNOWN`s.
- Add aggregate metrics that count route outcomes without storing raw targets,
  symbols, paths, or family ids.
