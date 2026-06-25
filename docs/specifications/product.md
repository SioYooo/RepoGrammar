# Product Specification

RepoGrammar is a local tool for helping coding agents understand recurring
implementation patterns inside a repository.

## Product goal

RepoGrammar should return pattern-family evidence rather than only call graphs
or similarity search results. A result should be able to describe:

- common implementation skeletons;
- high-support repository conventions;
- legitimate variation slots;
- exceptions and counterexamples;
- closest matching implementations;
- contrastive examples that cover key differences;
- source evidence for every conclusion;
- `UNKNOWN` when static analysis cannot support a claim.

The v0.1 technical narrative is Evidence-Constrained Multi-View Family
Induction (EC-MVFI): syntax, compiler semantics, framework role, CFG/dataflow
and effect, API usage, and repository-context views may propose and validate
families, but claims are emitted only when evidence and compatibility gates
support them. Otherwise the result must remain `UNKNOWN`.

The CLI and MCP surfaces must preserve this identity. Human-facing commands are
organized around implementation-pattern families, not generic symbol graph
navigation.

## Intended users

- Local coding agents preparing implementation changes.
- Maintainers reviewing whether a proposed change matches repository norms.
- Developers seeking representative examples inside a large codebase.

## MVP scope

RepoGrammar v0.1 is Python-first. The official v0.1 implementation target is
local Python analysis for recurring repository pattern families in:

- FastAPI;
- pytest;
- SQLAlchemy;
- Pydantic.

The first Python implementation phase follows the claim-driven selective
cascade in `docs/decisions/ADR-0012-python-selective-analysis-cascade.md`.
The implemented slice covers CPython `ast` structural candidates, path-derived
module anchors, CPython `symtable` structural scope anchors, private
`tomllib` project-config summaries, semantic-worker-compatible project-mode
module-level repo-local import resolution, default parser-mode repo-local
import context from discovered `.py` inventory, and framework-role heuristics
only, plus committed Python release fixtures and product smoke tests that
preserve no-claim defaults unless compatible test-injected `SEMANTIC` fixture
evidence is present. Subsequent slices should persist safe project-config facts,
add selective Pyrefly provider queries for plausible family candidates, Pyright
cross-checks only for claim-upgrading facts, bounded framework-role
propagation, target-centered call recovery, EC-MVFI-lite family induction, and
typed `UNKNOWN` governance. The canonical algorithm contract is
`docs/specifications/python-analysis.md`.

Existing TypeScript/JavaScript discovery, syntax extraction, framework-role
facts, TypeScript worker protocol scaffolding, and release fixtures are
transitional substrate from the earlier bootstrap. They may remain useful, but
they are no longer the official v0.1 language target. Production-quality TS/JS
family evidence is deferred until after the Python v0.1 checkpoint unless a
later ADR changes the sequence again.

Django, C/C++, whole-program Python call graphs, sound full Python semantic
analysis, and default runtime tracing are deferred.

## Non-goals

- No cloud service dependency.
- No local LLM, embedding model, vector database, or remote API.
- No global database for repository-derived family facts, evidence, source
  hashes, freshness metadata, or repository paths.
- No automatic modification of user business code from pattern-family results.
- No top-level v0.1 `callers`, `callees`, `impact`, `affected`, `node`, or
  `explore` commands.
- No production-readiness or token-savings claims until measured evidence
  exists.
- No mandatory CodeGraph dependency. CodeGraph may be considered only as an
  optional lower-layer provider, not as RepoGrammar's product identity.

## Result discipline

RepoGrammar must distinguish `DOMINANT_PATTERN`, `VARIATION`, `EXCEPTION`, and
`UNKNOWN`. Low confidence, competing families, incompatible targets, and dynamic
runtime behavior must lead to abstention rather than certainty.

Structural similarity may generate candidates, but it must not by itself prove
semantic family membership. Language-native semantic facts take precedence over
framework heuristics and syntax-only fingerprints. Syntax-origin framework-role
facts can record that a code unit has a recognizable framework role shape, but
`FRAMEWORK_HEURISTIC` certainty is not enough to prove family membership,
resolved handler identity, runtime lifecycle equivalence, or conformance.
Freshness is a required gate before semantic facts can become inputs to future
family claim builders. A fresh supported fact kind is still only eligible input;
it is not a `DOMINANT_PATTERN`, `VARIATION`, `EXCEPTION`, or conformance result
until EC-MVFI support, compatibility, and contrastive evidence checks are
implemented.
The current EC-MVFI-lite implementation is deliberately narrow: it can only
store a `DOMINANT_PATTERN` family when repeated compatible framework-role
candidates also have strong same-generation `SEMANTIC` or `DATAFLOW_DERIVED`
non-framework evidence. That support must be role-compatible: an arbitrary
semantic fact for an unrelated package, API, or framework cannot prove an
FastAPI, pytest, SQLAlchemy, Pydantic, Express, React, Jest, or Vitest family.
Otherwise family queries must return typed `UNKNOWN` rather than upgrading
syntax/framework heuristics into claims.

`UNKNOWN` is a typed result with reason codes and affected claims, not an
implementation failure by default. Some unknowns block specific semantic,
security, persistence, or conformance claims while still allowing weaker
structural observations. The canonical taxonomy lives in
`docs/specifications/unknowns.md`.

## Installation and telemetry boundaries

Machine-level `install` and `uninstall` are separate from repository-level
`init`, `index`, and `sync`. Installer behavior must be reversible, scoped, and
dry-run friendly.

Repository-derived analysis state belongs in the current repository's
`.repogrammar/` state directory, or the directory named by `REPOGRAMMAR_DIR`.
Global user state may contain installation receipts, binary/cache metadata,
anonymous telemetry preference, anonymous machine id, and non-repository-derived
runtime artifacts only.

Anonymous telemetry and research trace collection are separate consent
decisions. Context compression metrics are not actual token savings unless a
comparable baseline and treatment token measurement exist.
