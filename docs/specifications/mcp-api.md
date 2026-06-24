# MCP API Specification

The MCP interface is not implemented yet. This file records the first planned
tool boundaries without claiming a stable API.

## Tool intent

### find_analogues

Find source-backed implementations closest to a target and return conservative
classification evidence.

CLI equivalent: `repogrammar find`.

### show_family

Show the canonical template, variation points, exceptions, representative
implementations, and provenance for a known family.

CLI equivalent: `repogrammar family`.

### explain_deviation

Explain whether a target differs from a family as a legal variation, exception,
incompatibility, or `UNKNOWN`.

CLI equivalent: `repogrammar explain`.

### check_conformance

Check whether a target conforms to a selected family or abstain with a reason.

CLI equivalent: `repogrammar check`.

## Serving mode

`repogrammar serve` runs the MCP server once implemented. v0.1 serving behavior
must default to read-only and must not modify business code from pattern-family
results.

MCP calls must not wait on telemetry network activity.

## Boundary rules

- MCP schema and transport errors stay in `src/rust/interfaces/mcp/`.
- Core types must not depend on MCP SDK types.
- MCP responses may include semantic-worker-derived facts only after they have
  been translated into RepoGrammar-owned evidence and certainty categories.
- Serialization tests are required before concrete schemas are accepted.
- Any tool name, parameter, return-shape, or error-semantics change must use
  `.agents/skills/mcp-contract-change/SKILL.md`.
