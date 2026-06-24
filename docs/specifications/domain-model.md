# Domain Model Specification

This specification defines the initial domain vocabulary. The Rust-core bootstrap
implements only minimal types and placeholders.

## CodeUnit

A `CodeUnit` is a repository-owned analyzable source unit such as a function,
class, module, or test case. It carries a language, kind, source range, and
provenance. It must not contain Tree-sitter nodes or transport-specific types.

## Unified IR

The unified IR is a lightweight RepoGrammar representation derived from parser
and semantic-worker output. It supports structural comparison while hiding
parser-specific AST, compiler, LSP, and SDK details from the core domain.

## SemanticFact

A `SemanticFact` records language-native or derived information such as resolved
calls, imports, symbols, types, or framework roles. Facts include origin,
certainty, evidence, and assumptions.

Certainty levels are semantic, dataflow-derived, structural,
framework-heuristic, conflicting, and unknown. Structural certainty is not
enough to prove family membership.

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
for a conclusion. Stale provenance must not be treated as fresh evidence.

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

## Freshness

Freshness connects evidence to content hashes and repository revisions. Unknown
or stale freshness must be represented explicitly.

## Classification vocabulary

- `DOMINANT_PATTERN`: a high-support family pattern with sufficient evidence.
- `VARIATION`: a known allowed slot inside a family.
- `EXCEPTION`: a source-backed deviation or counterexample.
- `UNKNOWN`: insufficient evidence, competing families, dynamic behavior, or
  unsupported target.
