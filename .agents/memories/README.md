# Repository Memories

- Status: Active
- Last updated: 2026-06-24
- Scope: Memory lifecycle for RepoGrammar repository-local agent context.
- Evidence: Repository bootstrap documentation and mirrored agent contract.
- Related canonical docs: `docs/README.md`, `docs/development/documentation-policy.md`
- Supersedes: None
- Superseded by: None

## Context

Memories store durable context that future agents should know but that should
not be treated as mandatory specification.

## Durable knowledge

Use memories for current project phase, known local constraints, repeated traps,
verified compatibility issues, and open questions. Do not store secrets,
credentials, full specifications, temporary chat logs, or easily derived code
facts.

## Implications

If a memory conflicts with normative docs, the normative doc wins. Update,
supersede, or delete the stale memory in the same commit that exposes the
conflict.

## Revalidation conditions

Revalidate memories when architecture, specifications, storage, MCP contracts,
or repository governance rules change.
