# CodeGraph Provider Plan

- Status: Active planning artifact
- Last updated: 2026-06-25
- Scope: Optional lower-layer provider boundary

CodeGraph is a possible optional lower-layer graph and navigation provider. It
is not a RepoGrammar dependency, not a wrapper target, and not a replacement for
RepoGrammar's pattern-family product identity.

## Provider Role

A future CodeGraph provider may support:

- candidate retrieval hints;
- call and dependency context;
- graph neighborhood views;
- symbol/reference diagnostics;
- comparison against RepoGrammar-native discovery and semantic facts.

Provider facts must be translated into RepoGrammar-owned facts before entering
core or query results. They must carry provider name, provider version when
available, freshness metadata, source provenance, and conflict status.

## Product Boundary

RepoGrammar must work without CodeGraph. It must not require a `.codegraph/`
directory, CodeGraph CLI, CodeGraph MCP server, or CodeGraph runtime for
default indexing, query, tests, or MCP behavior.

CodeGraph facts can rank, enrich, or challenge candidates. They cannot by
themselves prove pattern-family membership, semantic equivalence, framework
role, conformance, or absence of risk.

RepoGrammar must not create, initialize, modify, delete, or repair
`.codegraph/`. If CodeGraph is unavailable, stale, or conflicting, RepoGrammar
should continue with native behavior and surface typed `UNKNOWN` or auxiliary
diagnostics where relevant.

## Future Configuration

Provider mode should eventually support:

- `auto`: use native RepoGrammar facts and optionally consume available provider
  facts when freshness and provenance checks pass;
- `internal`: use only RepoGrammar-native discovery, parsing, storage, and
  semantic facts;
- `codegraph`: use CodeGraph as an explicitly enabled auxiliary provider while
  keeping RepoGrammar-native evidence gates.

The exact configuration key is deferred until the provider port is specified.

## Non-goals

- no mandatory CodeGraph dependency;
- no CodeGraph wrapper product;
- no top-level `callers`, `callees`, `impact`, `affected`, `node`, or `explore`
  v0.1 commands;
- no automatic `codegraph init`;
- no bypass of Python v0.1 or other language-specific semantic-worker and
  family-evidence gates.

## Future Work

Before executable integration, define:

- provider port contract;
- unavailable-provider behavior;
- fact mapping and conflict semantics;
- freshness checks;
- provenance fields;
- security review for path and snippet exposure;
- fixture tests for present, missing, stale, and conflicting provider cases.
