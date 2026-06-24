# MCP API Specification

The MCP interface is not implemented yet. This file records the first planned
tool boundary without claiming a stable API.

## Default tool surface

The default v0.1 MCP surface should expose one primary tool:

```text
repogrammar_context
```

The tool carries an `operation` field. Supported v0.1 operations are:

- `find_analogues`
- `show_family`
- `explain_deviation`
- `check_conformance`

This keeps agent tool selection stable while preserving explicit internal
operation semantics. The CLI remains multi-command for human discoverability.

Advanced MCP tools may exist later, but they must be hidden by default and
enabled only by configuration or environment variable, for example:

```text
REPOGRAMMAR_MCP_TOOLS=context,find,family,check
```

## Operation intent

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

## Missing and stale indexes

MCP serving must not implicitly initialize a repository. If no project state
directory is found, the response must be a clean fallback recommendation rather
than a panic or noisy transport failure:

```text
FALLBACK_TO_CODE_SEARCH
reason: repository is not initialized
guidance: run repogrammar init
```

If an index is stale, MCP responses must include a stale warning or refuse
family claims whose evidence changed. Freshness checks must compare the active
index generation and repository state described in
`docs/specifications/storage.md`.

Typed analysis uncertainty must not be flattened into transport failure. Once
query execution exists, MCP responses must preserve `UNKNOWN` class, reason
code, affected claim, provenance, freshness status, and suggested recovery
action where available.
The current Rust query boundary has an internal semantic-fact freshness and
claim-input readiness gate for future claim builders. MCP responses must not
expose semantic-worker facts or treat them as family evidence until query
execution consumes that gate and family-evidence claim builders exist.

## Serving mode

`repogrammar serve` runs the MCP server once implemented. v0.1 serving behavior
must default to read-only and must not modify business code from pattern-family
results. MCP serving should open the active repository database read-only where
possible; indexing remains the only writer.

MCP calls must not wait on telemetry network activity.

## Agent guidance

The MCP initialize response is the canonical runtime guidance for agents.
Installer-written instruction-file content is optional, short, marker-fenced,
and owned by the installation workflow.

## Boundary rules

- MCP schema and transport errors stay in `src/rust/interfaces/mcp/`.
- Core types must not depend on MCP SDK types.
- MCP responses may include semantic-worker-derived facts only after they have
  been translated into RepoGrammar-owned evidence and certainty categories.
- Optional provider facts, including any future CodeGraph-derived facts, may
  appear only after translation into RepoGrammar-owned evidence with provider
  provenance and freshness metadata. Provider facts cannot independently prove
  pattern-family membership.
- Serialization tests are required before concrete schemas are accepted.
- Any tool name, parameter, return-shape, or error-semantics change must use
  `.agents/skills/mcp-contract-change/SKILL.md`.
