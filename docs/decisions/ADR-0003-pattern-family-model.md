# ADR-0003: Pattern-family repository representation

- Status: Accepted
- Date: 2026-06-24

## Context

RepoGrammar should help agents understand repository conventions. Call graphs
and nearest-neighbor similarity alone are not enough because agents need common
skeletons, legal variation points, exceptions, contrastive evidence, and
uncertainty.

## Decision

Represent repositories through pattern families with canonical templates,
variation points, exceptions, companion families, and source evidence. Results
must distinguish dominant patterns, variations, exceptions, and unknowns.

## Alternatives considered

- Call graph only: useful for flow but insufficient for implementation
  convention inference.
- Textual similarity only: easy to produce but weak for structural evidence.
- Automatic code modification from family matches: deferred because first-version
  results must be advisory and conservative.

## Consequences

The core domain model prioritizes evidence, provenance, freshness, compatibility,
and abstention. Static analysis facts that cannot be proven must remain
`UNKNOWN`.

## Follow-up work

Define canonical templates, anti-unification, clustering thresholds, support
metrics, and counterexample storage before implementing full mining.
