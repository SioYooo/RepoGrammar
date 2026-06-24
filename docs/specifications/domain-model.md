# Domain Model Specification

This specification defines the initial domain vocabulary. The Rust-core bootstrap
implements only minimal types and placeholders.

## CodeUnit

A `CodeUnit` is a repository-owned analyzable source unit such as a function,
class, module, or test case. It carries a language, kind, source range, and
provenance. It must not contain Tree-sitter nodes or transport-specific types.

Current syntax-only indexing can persist `CodeUnit` records for modules,
functions, assigned arrow functions, classes, methods, React function
components, custom hooks, Express route calls, and Jest/Vitest suite or test
blocks. These records are structural candidates only. They are not semantic
facts, resolved symbols, framework-equivalence claims, or pattern-family
membership evidence.

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
enough to prove family membership.
The current Rust domain includes an internal claim-input readiness gate for
semantic facts: fresh `SEMANTIC` and `DATAFLOW_DERIVED` facts may become inputs
to future family claim builders, while stale evidence, conflicting facts,
structural certainty, framework heuristics, and unknown certainty are blocked
with typed `UNKNOWN`. Readiness is not itself a family classification.

## PatternFamily

A `PatternFamily` will group related code units that share an implementation
pattern. The bootstrap only defines `FamilyId` and classification vocabulary.

## CanonicalTemplate

A `CanonicalTemplate` will represent the shared implementation skeleton for a
family. It is not implemented in the bootstrap.

## VariationPoint

A `VariationPoint` is an allowed slot where implementations can differ while
remaining inside the same family.

## Evidence

Evidence links a conclusion to a code unit, source range, provenance record, and
note. Every future family conclusion must carry auditable source evidence.

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
