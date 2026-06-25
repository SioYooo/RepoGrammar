# Project State

- Status: Bootstrap plus syntax-only TS/JS indexing substrate, Python `.py`
  discovery, CPython AST structural indexing slice, persisted internal Python
  structural anchors, path-derived module-name anchors, CPython `symtable`
  structural scope anchors, FastAPI dependency/error/request-shape anchors,
  pytest parametrize argument anchors, Pydantic field/config/member anchors, typed
  dynamic import, `sys.path` mutation, dynamic call, dynamic decorator,
  monkey-patch, and unresolved import `UNKNOWN` facts, private
  `tomllib` project-config summaries, semantic-worker-compatible project-mode
  module-level repo-local import resolution, default parser-mode repo-local
  import context from discovered `.py` inventory and sanitized root
  `pyproject.toml` source roots, default-indexed root `pyproject.toml`
  structural project-config records, structural IR storage,
  opt-in syntax-origin framework-role fact storage, semantic fact ingestion,
  bounded exact-anchor Python `DATAFLOW_DERIVED` support derivation,
  internal active claim-input snapshot reads, semantic-fact
  freshness/readiness gating, FamilyStore-backed query reads, read-only MCP
  serving, and narrow global
  explicit-target installer writes. ADR-0011 makes
  Python-first analysis the official v0.1 implementation target, and ADR-0012
  defines the claim-driven selective Python analysis cascade. The current
  Python slice persists parser-origin Python facts and structural project-config
  records but blocks them from direct family construction and claim-input
  readiness. A separate
  `repogrammar-python-derived` step can synthesize support from exact canonical
  anchors for units with one framework role, so narrow Python family rows can be
  produced without claiming provider-backed semantics. Product smoke now covers
  those exact-anchor families across CLI `member`/`find`/`explain`/advisory
  `check`, token-budget auto evidence, explicit compact/evidence/deep metadata
  modes, supported MCP operations, and stale source mutation/deletion returning
  blocking `StaleEvidence` `UNKNOWN`. Family detail output now supports
  compact/evidence/deep metadata modes shared by CLI and MCP; compact is the
  default, evidence/deep use greedy metadata coverage selection, and deep does
  not yet include source snippets. Ready Python exact-anchor families can also
  record metadata-only variation evidence when exact-compatible framework-anchor
  support targets differ. FastAPI static `response_model=...`, static
  `Depends(get_db)` dependency-target, `Depends`, `HTTPException`, and literal
  HTTPException status-code structural anchors, plus static FastAPI
  body/path/query/header/cookie request-shape anchors, remain auxiliary
  schema/context/effect metadata and are not membership support targets.
  Pytest fixture decorators are now alias-aware in same-file and `conftest.py`
  contexts. Direct parametrize arguments take precedence over same-name fixtures,
  indirect parametrize arguments stay typed `PytestFixtureInjection` `UNKNOWN`,
  and fixture-edge/parametrize-argument anchors remain context metadata rather
  than family support.
  Pydantic field, field-type, `model_config`, nested `Config`,
  computed-field, and model-validator anchors likewise remain schema/config/member
  metadata and are not membership support targets.
  FastAPI same-function service-call anchors remain handler/service context
  metadata and are not membership support targets.
  SQLAlchemy `relationship` and
  `Session.add`/`AsyncSession.add` anchors are also structural context/effect
  metadata, not family membership support. SQLAlchemy session call anchors now
  include direct `Session.commit`, `Session.rollback`, `Session.scalar`,
  `Session.scalars`, and async equivalents plus bounded propagation from
  `__init__`-assigned `self.session`/`self.db` attributes, with same-method
  receiver reassignment blocking
  canonicalization. A Rust `ports::python_provider`
  contract now exists for future candidate-scoped provider requests,
  provenance assumptions, cache-key dimensions, and recoverable
  provider-unavailable `UNKNOWN`s. The application layer can plan validated
  Pyrefly framework-identity request scopes for plausible Python candidate
  groups and skip parser-origin blocking `UNKNOWN`s for the planned claim, but
  Pyrefly/Pyright/RightTyper execution, provider fact storage, and
  provider-backed canonical evidence remain deferred. The planner can run over
  active-generation snapshots without mutating semantic facts, family rows, or
  CLI/MCP output.
- Last updated: 2026-06-26
- Scope: Current implemented capability snapshot.
- Evidence: Rust code, README, roadmap, CLI/storage/indexing specs, and
  `repo-guard` checks.
- Related canonical docs: `README.md`, `docs/roadmap.md`,
  `docs/specifications/cli.md`, `docs/specifications/storage.md`,
  `docs/specifications/indexing-pipeline.md`,
  `docs/specifications/python-analysis.md`,
  `docs/decisions/ADR-0011-python-first-v0-1.md`,
  `docs/decisions/ADR-0012-python-selective-analysis-cascade.md`
- Supersedes: None
- Superseded by: None

## Context

RepoGrammar is still pre-alpha, but it is past pure skeleton bootstrap. The
current branch has repository-local lifecycle, transitional
TypeScript/JavaScript discovery, Python `.py` discovery with Python
virtualenv/cache/dependency skips, generation-scoped SQLite storage, syntax-only
TS/JS code-unit indexing, CPython AST-backed Python structural code-unit
indexing, worker-local Python structural facts for imports, decorators, class
bases, simple calls, `pytest.test` test-function anchors, alias-aware pytest
fixture decorators, same-file pytest fixture edges, and typed dynamic
decorator, dynamic call, monkey-patch, dynamic import, `sys.path` mutation, or
unresolved import `UNKNOWN` cases persisted as
internal parser-origin semantic facts. It also labels FastAPI route
`response_model`, static dependency targets, `Depends`/`HTTPException`, literal
HTTPException status codes, static FastAPI body/path/query/header/cookie
request-shape markers, literal pytest parametrize arguments, Pydantic
field/config/member declarations, and bounded FastAPI same-function service
calls as structural parser-origin anchors without upgrading them to
provider-backed semantics.
Default parser-mode indexing now also carries sanitized root `pyproject.toml`
source roots from parser/tomllib project-config facts plus bounded discovered
`conftest.py` context into the CPython parse-document request so source-rooted
repo-local import facts and parent-directory pytest fixture-edge facts can be
persisted structurally; those facts are still not default family evidence. The
semantic-worker-compatible Python project mode can also output requested
`conftest.py` fixture hierarchy edges. Root `pyproject.toml` is persisted as a
`python-config` file and
`project_config` unit with sanitized structural config metadata or typed config
`UNKNOWN`,
syntax-origin TS/JS and Python framework-role fact storage, bounded exact-anchor
Python support derivation,
CodeUnit-derived structural IR node/containment-edge storage, Rust-side
TypeScript semantic-worker
request/output protocol validation and process validation, a dependency-free
TypeScript worker stub that reports compiler analysis as unavailable, a
validated semantic-fact storage writer, opt-in command-level semantic-worker
fact ingestion through the same-generation storage gate, conservative
FamilyStore-backed query reads, and a read-only MCP `repogrammar_context` stdio
boundary. It also has narrow live installer/uninstaller writes for explicit
global Codex and Claude Code MCP targets through native agent CLIs, gated by
`--yes`, MCP self-test, and RepoGrammar-owned receipts. It also has an internal
active-generation claim-input snapshot read path for future claim builders and
an internal file-hash freshness/readiness gate that blocks stale facts,
unsupported fact kinds, weak certainty, or conflicting certainty with typed
`UNKNOWN`. It also has committed TS/JS release fixtures and a product CLI JSON
smoke gate that exercises `init`, `index`, `files`, `units`, pattern-family
queries, and `doctor` without treating syntax-only evidence as a family claim.
ADR-0011 pivots the official v0.1 implementation target to Python-first
analysis for FastAPI, pytest, SQLAlchemy, and Pydantic; ADR-0012 defines that
the implementation should use a claim-driven selective cascade rather than
running every analyzer over every file. The current Python implementation is
the first CPython AST structural slice with worker-local structural anchors and
typed dynamic/unresolved `UNKNOWN` output plus narrow exact-anchor derived
support for canonical framework targets. The current TS/JS substrate remains
useful scaffolding but must not be described as the official v0.1 target.

## Durable knowledge

Implemented capabilities include module boundaries, minimal domain types,
pattern-family-first CLI command parsing, safe installer dry-run planning, typed
progress and telemetry policy types, stable not-implemented behavior,
transport-neutral MCP single-tool operation boundary, read-only MCP serving,
repository guard checks,
documentation, skills, memories, CI configuration, repo-local
`init`/`uninit`/`status`/`doctor`/`unlock`/`logs`, TS/JS and Python file
discovery, hash-checked source reads, dependency-free TS/JS syntax-only
code-unit extraction, CPython AST-backed Python structural code-unit
extraction, worker-local Python structural fact payloads for import bindings,
decorator anchors, class bases, simple call targets, `pytest.test`
test-function anchors, alias-aware pytest fixture decorators, same-file pytest
fixture edges, parent-directory `conftest.py` fixture hierarchy edges, FastAPI
dependency/error/request-shape anchors, pytest parametrize argument anchors that
are not treated as fixture injection UNKNOWNs,
Pydantic field/config/member anchors, typed dynamic decorator
framework-identity `UNKNOWN`, monkey-patch call-target `UNKNOWN`, and typed
dynamic/unresolved import `UNKNOWN` cases, plus bounded same-function FastAPI
service-call anchors with reassignment invalidation,
syntax-origin
framework-role facts for recognized Express, React, Jest/Vitest, FastAPI,
pytest, Pydantic, and SQLAlchemy code-unit shapes,
root `pyproject.toml` discovery and sanitized structural project-config
records, sanitized project-config source roots reused as default parser context,
bounded `DATAFLOW_DERIVED` support facts derived only from exact canonical
Python parser anchors and a single framework role,
CodeUnit-derived structural IR nodes and
conservative containment edges, generation-scoped SQLite
migrations/storage/validation/activation, product runtime wiring for `index`
and `sync`, and the dependency-free
`src/workers/typescript/worker.js` unavailable fallback stub, plus limited
`files`/`units` reads from active file-manifest-only or syntax-only generations.
Those reads revalidate active-generation health plus stored paths, hashes,
languages, unit ids, and byte ranges before returning repo-relative metadata.
Release fixture smoke coverage copies committed TS/JS and Python fixtures into
temporary workspaces and checks product CLI JSON paths, no absolute-path
leakage, no source-snippet or parser/provider-internal leakage, and
conservative `UNKNOWN` query results by default. Python release fixtures cover
direct FastAPI, FastAPI alias, pytest, alias-aware pytest fixtures,
Pydantic model/settings, SQLAlchemy,
mixed, dynamic-unknown, and low-support examples. Positive direct FastAPI,
FastAPI alias, pytest tests, pytest fixtures, Pydantic model/settings,
SQLAlchemy model-field, and SQLAlchemy session/repository including
commit/rollback and scalar/scalars fixtures now validate the
no-worker exact-anchor derived-support family path, exact-anchor target variation metadata,
metadata-only evidence modes, MCP parity, and stale-evidence query refusal. A
separate
test-only strong FastAPI semantic-support fixture injects compatible `SEMANTIC`
facts through the
existing worker boundary to validate family reads and stale-evidence fallback
without claiming production Python semantic-provider support.
Family detail reads now use compact/evidence/deep output modes. Compact omits
evidence records; evidence/deep run deterministic greedy metadata selection
under an optional token budget and report `source_snippets_included: false`.
Family evidence records carry schema-backed `covered_claims` labels. The
current builder emits `canonical` and `support`, plus one narrow Python
`variation` label when an already-ready family has multiple exact-compatible
framework-anchor support targets. Requested exception coverage and broader
variation coverage are returned in `missing_claims` until later builders link
evidence to those roles.
`index` and `sync` acquire `.repogrammar/locks/index.lock` before discovery and
hold it through validation and activation. Partial lock metadata write failures
must remove the partial lock file. `unlock --force --yes` removes only confirmed
stale `index.lock`; active, unknown, invalid, daemon, and SQLite locks remain
in place. Status and doctor JSON use explicit manifest/storage schema-version
fields and do not expose ambiguous `schema_version` fields.
The storage port and SQLite adapter can persist already-validated semantic facts
and repo-relative evidence for building generations when they match an indexed
same-generation code unit's path, content hash, and byte range. By default
`index` and `sync` still report `semantic_worker: deferred`; when
`REPOGRAMMAR_TYPESCRIPT_WORKER` names an explicit worker executable, optional
`REPOGRAMMAR_TYPESCRIPT_WORKER_ARGS_JSON` supplies its argv vector, and accepted
worker facts may be recorded before generation validation and activation.
Worker fallback keeps indexing syntax-only, while mismatched semantic evidence
aborts the new generation.
The application query/storage boundary can load an internal active-generation
claim-input snapshot containing files, code units, IR nodes/edges, and semantic
facts after revalidating stored fact kind/certainty tokens, assumptions JSON,
repo-relative evidence, content hashes, code-unit ids, and byte ranges. This is
an internal substrate only; CLI/MCP query commands do not render semantic facts.
The query application layer can check snapshot semantic facts against current
source hashes and classify fresh supported facts as eligible inputs for future
claim builders or typed `UNKNOWN` blockers (`StaleEvidence`,
`InsufficientSupport`, or `ConflictingFacts`). Fresh eligible facts are still not
family evidence by themselves. The current EC-MVFI-lite builder can persist a
family only when repeated framework-role candidates also have fresh
same-generation `SEMANTIC` or `DATAFLOW_DERIVED` support that is compatible
with the framework role; arbitrary unrelated semantic facts remain
`InsufficientSupport`. Public CLI/MCP family reads now exact-match `family` and
`member` targets, keep fuzzy matching limited to find/explain/check style
queries, gate rendered family evidence against current source hashes, and
report stale evidence as typed `StaleEvidence` `UNKNOWN`. Syntax-origin
framework-role facts use `FRAMEWORK_HEURISTIC` certainty and remain blocked
from family-claim input as insufficient support without stronger compatible
evidence.

Tree-sitter integration, TypeScript compiler API integration,
provider-backed Python project-configuration semantics, Pyrefly/Pyright
provider execution, provider-backed canonical framework evidence,
command-level full repository/worktree freshness metadata, typed IR attributes
beyond the structural bootstrap graph, resolved framework semantics, full
family mining, broad installer writes, project-local installer writes,
instruction-file integration, and telemetry network transport are not
implemented.

Pattern-family query commands and MCP tool calls still use stable fallback
behavior before an active index and typed `UNKNOWN` when active evidence is
insufficient. Advisory `check`/`check_conformance` responses may return matched
family context as `CONTEXT_ONLY`, but conformance remains nested `UNKNOWN`
because runtime equivalence is unproven. `files` and `units` can return active
file-manifest-only or syntax-only index metadata, but stored syntax-only units
must not be described as query-ready family evidence.

## Implications

Future agents must not claim compiler-backed TypeScript analysis,
provider-backed Python semantic analysis, full pattern-family mining,
freshness-validated semantic claims, installer writes
beyond explicit Codex/Claude MCP registration, or stable MCP API support until
those capabilities are implemented and tested.
Agents also must not restart repo-local lifecycle, SQLite generation, opt-in
semantic-worker ingestion, or Rust-side worker process validation work from
scratch. Do not restart structural IR storage or active semantic-fact/evidence
read-path work from scratch either; extend the existing lifecycle, storage,
worker stub, query read path, and worker boundary substrates through the
canonical specs.

## Revalidation conditions

Update this memory after provider-backed project-configuration semantics,
Pyrefly/Pyright provider integration, Tree-sitter fallback, TypeScript compiler
API integration, full family-claim gates, broader installer writes, production
family evidence, or stable MCP API support lands.
