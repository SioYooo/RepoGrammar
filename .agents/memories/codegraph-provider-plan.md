# CodeGraph Provider Plan

- Status: Active
- Last updated: 2026-06-25
- Scope: Durable cautions for CodeGraph-adjacent integration planning.
- Evidence: `docs/decisions/ADR-0010-optional-codegraph-provider.md`
- Related canonical docs: `docs/decisions/ADR-0006-pattern-family-cli.md`, `docs/specifications/mcp-api.md`
- Supersedes: None
- Superseded by: None

## Durable knowledge

- RepoGrammar must not become generic graph navigation. Its identity is
  pattern-family evidence, variations, exceptions, and `UNKNOWN`.
- Do not add CodeGraph-style top-level commands: `callers`, `callees`,
  `impact`, `affected`, `node`, or `explore`.
- A future CodeGraph bridge, if accepted, should be optional and auxiliary. It
  must translate provider output into RepoGrammar-owned facts with provenance
  and freshness metadata.
- The default MCP plan remains one `repogrammar_context` tool with explicit
  operations.
- The repo's `.codegraph/` directory is a development aid for agents, not a
  product dependency or proof of RepoGrammar query support.

## Revalidation conditions

Update if an ADR accepts a provider bridge implementation, graph namespace, or
advanced MCP tool exposure.
