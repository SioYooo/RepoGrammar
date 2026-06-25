# Domain Model Specification

This specification defines the initial domain vocabulary. The Rust-core bootstrap
implements only minimal types and placeholders.

## CodeUnit

A `CodeUnit` is a repository-owned analyzable source unit such as a function,
class, module, or test case. It carries a language, kind, source range, and
provenance. It must not contain Tree-sitter nodes or transport-specific types.

Current syntax-only indexing can persist transitional TS/JS `CodeUnit` records
for modules, functions, assigned arrow functions, classes, methods, React
function components, custom hooks, Express route calls, and Jest/Vitest suite or
test blocks. The first Python v0.1 slice can also persist CPython
`ast`-derived records for modules, functions, async functions, classes, methods,
FastAPI route-shaped functions, pytest tests and fixtures, Pydantic model-shaped
classes, SQLAlchemy model-shaped classes, and SQLAlchemy repository
method-shaped functions. Root `pyproject.toml` may be represented as a
`python-config` language file with a `project_config` code unit so the config
artifact can share the same generation, hash, and evidence validation boundary.
These records are structural candidates only. They are
not semantic facts, resolved symbols, framework-equivalence claims, or
pattern-family membership evidence. A separate syntax-origin `SemanticFact` may
be derived from some framework-shaped code units, but that fact remains
framework-heuristic evidence for future claim builders, not proof of semantic
equivalence or family membership.

## Unified IR

The unified IR is a lightweight RepoGrammar representation derived from parser
and semantic-worker output. It supports structural comparison while hiding
parser-specific AST, compiler, LSP, and SDK details from the core domain.

Current syntax-only indexing persists only the bootstrap IR subset: one IR node
per code unit and conservative `contains` edges from modules to contained units
and from classes to methods. IR node payloads remain `{}` until typed IR
attributes are designed. This structural IR is not semantic certainty or
pattern-family membership evidence.

## SemanticFact

A `SemanticFact` records language-native or derived information such as resolved
calls, imports, symbols, types, or framework roles. Facts include origin,
certainty, evidence, and assumptions.

Certainty levels are semantic, dataflow-derived, structural,
framework-heuristic, conflicting, and unknown. Structural certainty is not
enough to prove family membership. Framework-heuristic certainty is also not
enough to prove family membership.
The current Rust domain includes an internal claim-input readiness gate for
semantic facts: fresh supported fact kinds with `SEMANTIC` or
`DATAFLOW_DERIVED` certainty may become inputs to future family claim builders,
while stale evidence, conflicting facts, structural certainty, framework
heuristics, unknown certainty, and `UNKNOWN` fact kind are blocked with typed
`UNKNOWN`. Readiness is not itself a family classification.
Current default TS/JS indexing can store syntax-origin `FRAMEWORK_ROLE` facts
for recognized Express, React, and Jest/Vitest code-unit shapes. These facts
carry repo-relative code-unit evidence, `FRAMEWORK_HEURISTIC` certainty, and
explicit unresolved-binding assumptions; they do not resolve TypeScript symbols,
framework runtime behavior, or family membership.
Current default Python indexing can store syntax-origin `FRAMEWORK_ROLE` facts
for FastAPI route-shaped units, pytest tests/fixtures, Pydantic models,
SQLAlchemy models, and SQLAlchemy repository methods. These facts also use
`FRAMEWORK_HEURISTIC` certainty and unresolved-binding assumptions; they do not
resolve imports, decorator targets, fixture bindings, SQLAlchemy mappings, or
family membership. Current default Python indexing can also persist CPython
`ast` parse-document structural facts for import bindings, decorator anchors,
class bases, simple calls, FastAPI static response-model/dependency-target/error
anchors, and typed dynamic/unresolved `UNKNOWN` cases. These facts remain
`STRUCTURAL` or `UNKNOWN`, are blocked from family-claim input, and are not fed
to the current family builder as raw facts. A separate
application-layer derivation step may synthesize `DATAFLOW_DERIVED` support
facts from those validated structural anchors only when the unit has exactly
one Python framework role, evidence stays in the same code-unit path/hash/range,
and the target exact-matches the canonical Python compatibility table. Those
derived facts use `provider_resolved=false`; they are bounded support for
EC-MVFI-lite, not provider-backed semantic identity or runtime-equivalence
proof. It can also persist
`PROJECT_CONFIG` facts for sanitized root `pyproject.toml` project name, safe
source roots, and recognized tool sections, or typed config `UNKNOWN`s when
`tomllib` or valid TOML is unavailable. `PROJECT_CONFIG` facts are structural
context only and are blocked from claim-input readiness even if a future bug
marks them with stronger certainty. Future Python facts should follow the
same owned model for FastAPI, pytest, SQLAlchemy, and Pydantic evidence; parser,
type-checker, LSP, or Python runtime objects must not enter the core domain.

Python framework compatibility must use typed canonical identities rather than
free-text matching. Python facts should map provider output into owned records
such as resolved symbol, subclass, decorator binding, call target, and fixture
binding facts with canonical fully qualified names. A framework claim must be
checked against an explicit compatibility table for FastAPI, pytest, Pydantic,
and SQLAlchemy. Do not infer Python framework compatibility from substrings in
fact kind, engine, method, target, assumptions, path, or note fields.

Future Python provider states such as cross-checked static facts or observed
runtime facts require explicit domain/protocol/storage changes before becoming
public certainty tokens. Until then, cross-check status and observed provenance
must stay in owned assumptions/provenance and use the current certainty
vocabulary.

## PatternFamily

A `PatternFamily` will group related code units that share an implementation
pattern. The current storage substrate can persist generation-scoped family
records, family members, variation slots, and family-bound evidence. The
current EC-MVFI-lite builder can populate those records only for repeated
compatible candidates backed by strong semantic/dataflow support; syntax-only
framework-role facts produce typed `UNKNOWN` instead of a family claim. Full
template induction, exception mining, and MCP family responses remain deferred.

## CanonicalTemplate

A `CanonicalTemplate` will represent the shared implementation skeleton for a
family. It is not implemented in the bootstrap.

## VariationPoint

A `VariationPoint` is an allowed slot where implementations can differ while
remaining inside the same family.

## Evidence

Evidence links a conclusion to a code unit, source range, provenance record, and
note. Every future family conclusion must carry auditable source evidence.
Family evidence storage must remain linked to a family and same-generation code
unit and must carry explicit covered-claim labels. The current allowlist is
`canonical`, `support`, `variation`, and `exception`. Current builders emit
`canonical` and `support`, and they may emit one narrow Python `variation`
evidence label when an already-ready family has multiple exact-compatible
framework-anchor support targets. Exception evidence and broader medoid,
template, or counterexample evidence links remain deferred. Semantic-fact
evidence must not be treated as family evidence by itself.

## Provenance

Provenance contains the source path, content hash, and repository revision used
for a conclusion. Content hashes use the exact `sha256:<64 hex chars>` form;
empty, placeholder, or non-SHA-256 values are not auditable evidence. Stale
provenance must not be treated as fresh evidence.

## Counterexample

A counterexample is a source-backed implementation that resembles a family but
violates a meaningful rule. Counterexample storage is deferred.

## CompatibilityResult

Compatibility expresses whether a target can be compared to a family:
compatible, incompatible with reason, or unknown with reason.

## AbstentionReason

Abstention prevents weak evidence from becoming a false claim. Reasons include
low confidence, competing families, dynamic runtime behavior, and unsupported
targets.

## UNKNOWN governance

`UNKNOWN` is a typed analysis result. It can be blocking, non-blocking,
recoverable, or irreducible depending on which claim is affected and what
evidence could resolve it. Reason codes include dynamic imports, monkey
patching, pytest fixture injection, runtime dependency injection, unresolved
imports, missing project configuration, missing dependencies, framework magic,
macro or preprocessor ambiguity, build variant ambiguity, conflicting facts,
stale evidence, and insufficient support.

The canonical taxonomy and propagation rules live in
`docs/specifications/unknowns.md`. New domain behavior that emits or consumes
unknowns must update that file when it introduces a public reason code, class,
or recovery action.

## Freshness

Freshness connects evidence to content hashes and repository revisions. Unknown
or stale freshness must be represented explicitly.
The current implementation has a file-hash freshness policy for active semantic
facts. It compares stored fact evidence hashes with current source reads before
allowing a fact to become eligible input for future claim builders. Missing or
changed source becomes a blocking `UNKNOWN` with reason `StaleEvidence` and a
`run repogrammar sync` recovery suggestion. Repository-revision and worktree-wide
freshness are still deferred.

## Classification vocabulary

- `DOMINANT_PATTERN`: a high-support family pattern with sufficient evidence.
- `VARIATION`: a known allowed slot inside a family.
- `EXCEPTION`: a source-backed deviation or counterexample.
- `UNKNOWN`: insufficient evidence, competing families, dynamic behavior, or
  unsupported target.
