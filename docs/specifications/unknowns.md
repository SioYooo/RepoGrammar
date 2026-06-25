# UNKNOWN Governance Specification

RepoGrammar treats `UNKNOWN` as a first-class typed analysis output. It is not a
panic, not an empty result, and not something to guess away.

## Purpose

The purpose of `UNKNOWN` is to prevent weak, stale, conflicting, unsupported, or
dynamic evidence from becoming false certainty. A family classification may
still be useful when some supporting facts are unknown, but the unknown facts
must remain visible and must block only the claims they actually affect.

## Unknown Classes

- `blocking_unknown`: prevents a specific claim or classification from being
  emitted because required evidence is unavailable or contradictory.
- `non_blocking_unknown`: records missing or weak evidence that does not affect
  the emitted claim under the current query.
- `recoverable_unknown`: may be resolved by additional evidence such as a
  semantic worker, user hint, optional provider, project config, dependency
  install, or bounded runtime trace.
- `irreducible_unknown`: cannot be resolved safely by static analysis in the
  current configuration, usually because behavior is runtime-defined or
  intentionally dynamic.

## Reason Codes

Reason codes are stable protocol-facing values. They may apply to Python v0.1,
transitional TypeScript/JavaScript substrate, future C/C++, provider facts, or
storage freshness:

- `DynamicImport`
- `MonkeyPatch`
- `PytestFixtureInjection`
- `RuntimeDependencyInjection`
- `UnresolvedImport`
- `MissingProjectConfig`
- `MissingDependency`
- `FrameworkMagic`
- `MacroOrPreprocessor`
- `BuildVariantAmbiguity`
- `ConflictingFacts`
- `StaleEvidence`
- `InsufficientSupport`

Future reason codes must be documented here before they become public CLI, MCP,
storage, or metrics output.

## Claim Rules

Structural evidence may generate candidates but cannot independently prove
semantic family membership. Compiler-native semantic facts, framework roles,
CFG/dataflow/effect summaries, API usage, and repository context may strengthen
claims only when their provenance and freshness checks pass.

The Rust domain currently exposes stable typed `UNKNOWN` classes and reason
codes and uses them in the internal semantic-fact claim-input readiness gate.
Fresh supported fact kinds with `SEMANTIC` or `DATAFLOW_DERIVED` certainty may
be eligible inputs for future claim builders, but that eligibility is not a
pattern-family claim. Stale or missing source evidence blocks the affected
semantic-fact claim input with `StaleEvidence`; `UNKNOWN` fact kind plus
`STRUCTURAL`, `FRAMEWORK_HEURISTIC`, and `UNKNOWN` certainty block with
`InsufficientSupport`; `CONFLICTING` certainty blocks with `ConflictingFacts`.

If views disagree, RepoGrammar must preserve the conflict as
`ConflictingFacts`, produce `CONFLICTING` internally where appropriate, and
abstain or emit `UNKNOWN` for affected claims. Do not average or vote away
conflicts when the losing fact would change behavior, security, persistence,
authorization, transactionality, idempotency, async lifecycle, error mapping, or
external side effects.

Some unknowns block only specific claims:

- unresolved auth middleware may block a conformance claim about authorization
  while still allowing a syntax-only route-shape classification;
- a stale dependency graph may block call-context evidence while preserving
  source-range evidence from the current file hash;
- Python fixture injection may block test-behavior equivalence
  while still allowing structural pytest test discovery.

## Recovery Actions

Recovery suggestions may include:

- run or configure a language-native semantic worker;
- add missing project configuration;
- install or point to missing dependencies;
- enable an optional provider such as a future CodeGraph provider;
- provide explicit user hints;
- run an optional bounded runtime trace when that feature exists and is enabled;
- re-run `repogrammar sync` when evidence is stale.

Recovery actions are suggestions, not automatic mutation permission. They must
not create indexes, install dependencies, execute runtime traces, or initialize
provider state without an explicit command and documented consent boundary.

## CLI, MCP, Storage, and Metrics

Human and JSON CLI output must distinguish:

- missing-index fallback;
- stale-index refusal;
- not-yet-implemented query execution;
- typed `UNKNOWN` from a real analysis result.

MCP serialization must preserve unknown class, reason code, affected claim,
evidence attempted, freshness status, and suggested recovery action.

Storage records for families, facts, and evidence must retain enough provenance
to explain why a fact is unknown or why it was non-blocking for a claim.

Metrics may count unknowns by language, framework, adapter, reason, and stage.
Unknown-rate reduction must not be reported as quality improvement unless false
certainty is also measured or controlled.

## Test Expectations

New analyzers, providers, query paths, and serializers should include positive,
negative, stale, conflicting, unsupported, and dynamic cases. Tests must prove
that unknowns are not silently collapsed into dominant patterns, variations, or
exceptions.
