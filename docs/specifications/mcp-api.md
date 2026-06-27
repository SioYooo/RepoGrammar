# MCP API Specification

The MCP interface is implemented as a minimal pre-alpha read-only stdio server.
This file records the concrete bootstrap tool boundary and the current
Codex/Claude Code installer wiring without claiming a final stable API.

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

The current input schema is intentionally small:

- required `operation`: one of the four operation strings above.
- optional `target`: non-empty string, at most 8192 bytes, with no control
  characters.
- optional `token_budget`: positive integer up to 200000 used to cap selected
  evidence metadata. Supplying it implies `mode: evidence` unless an explicit
  mode is provided.
- optional `mode`: one of `compact`, `evidence`, or `deep`.
- optional `include_variations` and `include_exceptions`: booleans requesting
  those evidence-coverage labels. Current output reports them as missing unless
  stored family evidence explicitly covers them.
- optional `include_source_spans`: boolean, default `false`. When true,
  RepoGrammar may render bounded line-numbered source spans selected from the
  `read_plan` after content-hash validation. This is an explicit source-output
  opt-in and is not implied by `mode: deep`.

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
The target is an exact family id. `show_family` must not resolve arbitrary
path, framework-role, classification, or substring targets.

CLI equivalent: `repogrammar family`.

### explain_deviation

Explain whether a target differs from a family as a legal variation, exception,
incompatibility, or `UNKNOWN`.

CLI equivalent: `repogrammar explain`.

### check_conformance

Check whether a target conforms to a selected family or abstain with a reason.
In the current v0.1 candidate, matched family context is not proof of runtime
equivalence. `check_conformance` must therefore return top-level
`CONTEXT_ONLY` or typed `UNKNOWN` when conformance is unproven, and its nested
check result must remain advisory `UNKNOWN`.

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
The same freshness gate applies to public MCP family detail and conformance
responses: stale family evidence must become typed `StaleEvidence` `UNKNOWN`
rather than a top-level successful claim.

Typed analysis uncertainty must not be flattened into transport failure. MCP
responses preserve `UNKNOWN` class, reason code, affected claim, and suggested
recovery action where available.
The current Rust storage/query boundary has an internal active-generation
claim-input snapshot, semantic-fact freshness/readiness gate, and conservative
EC-MVFI-lite family read model. MCP responses must not expose semantic-worker
facts, raw snapshot contents, or treat framework heuristics as family evidence.
The MCP call handler reuses the same application query preflight and
FamilyStore-backed lookup path as the CLI rather than inventing a parallel
contract.
Matched family responses use the same output selection contract as the CLI:
`compact` is the default and returns family summary, members, variation slots,
unknowns, output metadata, and a `read_plan` without evidence records;
`evidence` adds budgeted repo-relative evidence metadata selected by
deterministic greedy marginal coverage per estimated token cost; `deep` is
accepted only as an explicit detail request and remains metadata-first unless
`include_source_spans: true` is also provided. The `read_plan` is returned for
the existing `find_analogues`, `show_family`, `explain_deviation`, and
`check_conformance` operations. It contains suggested target, canonical,
support, and variation/exception spans by repo-relative path, strict content
hash, byte range, estimated token cost, and purpose. When source spans are not
requested, line ranges may remain `null` and `source_snippets_included` remains
`false`. When `include_source_spans: true` is requested, RepoGrammar renders
only selected read-plan spans after source-store path and content-hash
validation, fills line ranges for rendered spans, and returns line-numbered
text under a separate `source_spans` object. The read plan never includes
absolute paths and does not imply source edits are safe outside listed ranges.
Output metadata includes the selection strategy, estimated evidence tokens,
estimated read-plan tokens, `estimated_potential_token_savings` with
`measurement_kind: ESTIMATED`, covered claim labels, missing claim labels, and
whether the rough budget was satisfied. `estimated_potential_token_savings` is
a potential read-displacement estimate for the returned RepoGrammar metadata
shape; it is not measured token savings and must carry a caveat saying so.
Stored family evidence carries
schema-backed `covered_claims` labels from the allowlist `canonical`,
`support`, `variation`, and `exception`; selectors must consume those labels
rather than infer coverage from notes or record order. The current family
builder emits `canonical` and `support`, plus a narrow Python `variation` label
when an already-ready family has multiple exact-compatible framework-anchor
support targets. It may also emit metadata-only variation slots when
parser-context profiles differ inside an already-supported Python family, but
those slots do not imply variation evidence coverage. Requested exception
coverage and broader variation coverage must be reported as missing until later
builders explicitly link evidence to those claims. Family detail unknowns
identify runtime-equivalence gaps with the concrete
`<family_id>:runtime_equivalence` affected claim. MCP responses must report
whether source snippets were included. Stale, missing, hash-mismatched,
unsupported, dynamic, insufficient, or conflicting evidence must omit rendered
spans and preserve typed `UNKNOWN` or omission guidance telling the agent to
use normal Read/Grep for the affected case.

## Serving mode

`repogrammar serve` runs a newline-delimited JSON-RPC stdio loop for
`initialize`, `notifications/initialized`, `tools/list`, `tools/call`, and
`shutdown`. v0.1 serving behavior defaults to read-only for source, index,
family, and business-code state and must not modify business code from
pattern-family results. MCP serving uses a read-only analysis runtime facade
that can only request repository status and pattern-family lookup. Indexing
remains the only writer for repository analysis state. The only allowed MCP
side effect is the local aggregate metric described below; it is not source,
index, family, or agent configuration state.

`tools/list` returns exactly one default tool, `repogrammar_context`.
`tools/call` wraps the RepoGrammar JSON payload in a standard MCP text content
item:

```json
{
  "content": [
    {
      "type": "text",
      "text": "{\"status\":\"UNKNOWN\"}"
    }
  ],
  "isError": false
}
```

Missing state, missing active indexes, and typed analysis uncertainty are normal
tool results. Unknown JSON-RPC methods, unknown tool names, invalid operations,
blank, oversized, or control-character-containing targets, oversized token
budgets, and malformed argument types are transport/schema errors.

MCP calls must not wait on telemetry network activity and must not trigger
telemetry upload. Anonymous telemetry upload is only attempted by explicit
`repogrammar telemetry upload` after consent and endpoint validation.
Successful family-context MCP calls may best-effort update the repo-local
aggregate `.repogrammar/telemetry/local-metrics/estimated_potential_token_savings.json`.
That local file stores only aggregate estimated token counts, event count,
`ESTIMATED` kind, and caveat text; it must not store operation targets, paths,
content hashes, prompts, source, evidence text, symbols, or raw errors.
Claude Code and Codex integrations both point at this same read-only MCP server;
agent installation and uninstallation are machine-level configuration workflows
and must not change MCP tool semantics, initialize a repository, index code,
touch `.repogrammar/`, upload telemetry, or enable telemetry by themselves.
Interactive multi-agent install and `--target all --scope global --yes` are
installer transactions around this same read-only MCP server, not additional
MCP tools.

## Agent guidance

The MCP initialize response is the canonical runtime guidance for agents.
Installer-written instruction-file content is optional, short, marker-fenced,
and owned by the installation workflow.
In initialized repositories, agents should call `repogrammar_context` before
grep/find/manual reads for implementation-pattern analogues, family
conformance, deviations, or repeated framework behavior. Agents must fall back
to normal Read/Grep when RepoGrammar returns `UNKNOWN`, stale/omitted spans, or
insufficient support.

## Boundary rules

- MCP schema and transport errors stay in `src/rust/interfaces/mcp/`.
- Core types must not depend on MCP SDK types.
- MCP responses may include semantic-worker-derived facts only after they have
  been translated into RepoGrammar-owned evidence and certainty categories.
- Future Python provider facts from Pyrefly, Pyright, or RightTyper-style
  observed runs may appear in MCP only after translation into RepoGrammar-owned
  facts with provider provenance, freshness metadata, and current supported
  certainty tokens. MCP must not expose raw provider graphs, Python AST nodes,
  LSP payloads, or runtime traces as product results.
- Optional provider facts, including any future CodeGraph-derived facts, may
  appear only after translation into RepoGrammar-owned evidence with provider
  provenance and freshness metadata. Provider facts cannot independently prove
  pattern-family membership.
- Serialization tests are required before concrete schemas are accepted.
- Any tool name, parameter, return-shape, or error-semantics change must use
  `.agents/skills/mcp-contract-change/SKILL.md`.
