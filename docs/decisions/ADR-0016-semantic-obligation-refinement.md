# ADR-0016: Semantic obligation refinement for typed UNKNOWNs

- Status: Accepted
- Date: 2026-07-04
- Refines: ADR-0015 (provider-backed semantic analysis execution), specifically
  the "absorb" point that a semantic obligation should be a first-class object
  rather than only an `UNKNOWN` reason field.
- Related: `docs/specifications/unknowns.md`,
  `docs/reports/unknown-resolution-sota-analysis.md`,
  `docs/experiments/unknown-regression-benchmark.md`

## Context

RepoGrammar already models typed `UNKNOWN`s along two axes: a reason code
(`DynamicImport`, `FrameworkMagic`, `MissingDependency`, …) and a recoverability
class (`recoverable_unknown` vs `irreducible_unknown`, the ADR-0015 class-b/c
line). It also derives an ad-hoc `required_mechanism` string (`python_import_graph`,
`fastapi_dependency_graph`, …) naming which analyzer/provider could resolve each
`UNKNOWN`.

What was missing is a typed, first-class notion of *what kind of semantic
question* an `UNKNOWN` poses — its obligation. ADR-0015's absorb points called
for making the obligation a first-class object (type identity, symbol binding,
dispatch target, framework identity, build variant, macro expansion, external
dependency), under the hard constraint that it must be a **refinement** of the
`UNKNOWN` contract and must never weaken it: residual/unproven questions still
fall back to a typed `UNKNOWN`.

## Decision

Introduce a first-class `SemanticObligation` domain enum and expose it as a
source-free aggregate (`by_obligation`) in the existing `unknowns --json` /
`stats --unknowns --json` inventory. The obligation is a deterministic,
source-free refinement of an already-typed `UNKNOWN`; it is purely additive
annotation.

### D1. Vocabulary (fixed, source-free)

`type_identity`, `symbol_binding`, `dispatch_target`, `framework_identity`,
`build_variant`, `macro_expansion`, `external_dependency`, `runtime_irreducible`,
`governance`. This is a closed enum with stable protocol tokens and carries no
path, symbol, target, or repository-specific text.

### D2. It never weakens the UNKNOWN contract

The obligation classifier does not resolve any `UNKNOWN`, change whether it
blocks a family claim, alter any support gate, or emit family evidence. Every
`UNKNOWN` still blocks or abstains exactly as before. Obligation is orthogonal
to the `recoverable`/`irreducible` class axis (which is unchanged). Runtime-
defined reasons (`MonkeyPatch`, dynamic execution call targets) map to
`runtime_irreducible` and stay `UNKNOWN` by design (ADR-0015 class c). Quality
states (`StaleEvidence`, `ConflictingFacts`, `InsufficientSupport`) map to
`governance` and are explicitly **not** semantic obligations.

### D3. Deterministic derivation

The obligation is derived deterministically from the already-typed reason code
plus the same language/claim/framework-role/assumption context used to pick the
required mechanism — not from free text or structural similarity. One input maps
to exactly one obligation. The genuinely ambiguous reasons
(`RuntimeDependencyInjection`, `FrameworkMagic`) are disambiguated with the
existing role/claim/assumption signals (framework role → framework identity;
bare untyped DI → type identity; `rust_trait_dispatch` assumption → dispatch
target; `python_call_target` claim → runtime irreducible).

### D4. Source-free exposure only

`by_obligation` is an aggregate count bucket subject to the same redaction rules
as the other inventory buckets: no paths, code-unit ids, fact ids, snippets,
repository names, raw targets, or free-text guidance. It does not introduce a
new storage schema or a new certainty token, and it is not family evidence.

## Alternatives considered

- **Leave obligation implicit in mechanism strings:** rejected — mechanism names
  the tool, not the question kind, and the recoverable/irreducible split does not
  capture the obligation taxonomy ADR-0015 asked to make first-class.
- **Replace the reason code or recoverability class with obligation:** rejected —
  that would collapse orthogonal axes and risk weakening the `UNKNOWN` contract.
  Obligation is additive.
- **Infer obligation from naming/structural similarity:** rejected — forbidden
  by the engineering standards; derivation uses only already-typed inputs.

## Consequences

- Agents can see how many `UNKNOWN`s are provider-resolvable questions vs
  runtime-irreducible residuals vs governance states, source-free.
- Future provider adapters (ADR-0015 D4) can be measured against a typed
  obligation baseline in the UNKNOWN regression benchmark without weakening any
  gate.
- Any new `UnknownReasonCode` must extend the deterministic obligation mapping
  and its tests; the closed obligation vocabulary changes only with a matching
  spec and test update.
