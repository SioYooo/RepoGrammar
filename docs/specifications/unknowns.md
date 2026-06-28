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

Future provider agreement or runtime observation states, such as Python
cross-checks or RightTyper-style observed traces, must not become public
certainty tokens until the Rust domain, protocol schemas, storage, CLI, MCP,
and tests define those tokens together. Until then, agreement and observation
details remain provenance or assumptions, while disagreements still surface as
`ConflictingFacts` or typed `UNKNOWN`.

Some unknowns block only specific claims:

- unresolved auth middleware may block a conformance claim about authorization
  while still allowing a syntax-only route-shape classification;
- a stale dependency graph may block call-context evidence while preserving
  source-range evidence from the current file hash;
- Python fixture injection may block test-behavior equivalence
  while still allowing structural pytest test discovery;
- dynamic Python Pydantic model factories may block static model-family
  identity without blocking unrelated model classes in the same file;
- dynamic or unsafe Python pytest fixture `name=` aliases may block fixture
  binding without changing the structural fact that the function is decorated
  as a pytest fixture;
- duplicate applicable Python `conftest.py` fixture names may block fixture
  binding with `ConflictingFacts`, while known pytest built-in fixtures may
  remain non-supporting external context;
- dynamic FastAPI dependency target expressions may block the dependency-target
  sub-claim while still allowing a route family when route membership has enough
  independent exact-anchor support;
- dynamic TS/JS route methods, unsafe or unresolved Express/Fastify receivers,
  unsafe or unresolved Jest/Vitest runner bindings, unsupported Next.js
  route-convention magic, Prisma injected/raw/bulk/dynamic clients, and Drizzle
  unresolved/raw/dynamic builders may block the affected `tsjs_receiver_binding`,
  `tsjs_runner_binding`, `tsjs_support_target`, or adapter-specific claim while
  other exact-anchor units in the same repository can still form a family when
  they have enough independent compatible support.
- TS/JS dynamic imports, conditional or non-literal `require`, unresolved
  repo-local imports, unresolved or conflicting path aliases, ambiguous star
  re-exports, and missing ambient test-runner project context must remain typed
  `UNKNOWN`. These unknowns may be blocking only when they affect framework
  identity, runner/receiver binding, support target, or another emitted family
  claim; otherwise they remain context/read-plan guard evidence and must not be
  guessed away.
- The TS/JS parser maps granular v0.2 cases onto the stable reason-code set:
  dynamic `import(...)` is `DynamicImport`; non-literal or conditional
  `require`, dynamic route/test calls, Next server-client/middleware/server
  action/re-export magic, Fastify dynamic route options or full routes without
  literal `url`/`path`, Prisma callback, raw, bulk, or dynamic operations, and
  Drizzle raw/dynamic builders are
  `FrameworkMagic` or `BuildVariantAmbiguity`; exact local Next dynamic
  segments, route groups, and parallel routes are stored as context assumptions
  on accepted anchors rather than UNKNOWNs by themselves; unresolved relative
  imports, unresolved path aliases,
  unresolved Express/Fastify receivers, unresolved Prisma clients, unresolved
  Drizzle db/table bindings, and missing ambient runner or Next package context
  are `UnresolvedImport` or `MissingProjectConfig` as applicable; reassigned or
  shadowed receivers, unsafe test runner bindings, conflicting path aliases, and
  ambiguous star re-exports are `ConflictingFacts`. These mappings are
  intentionally conservative and do not create new public reason codes for every
  syntax shape.
- Rust self-dogfood maps unresolved external modules to `UnresolvedImport`,
  `#[cfg]` / `#[cfg_attr]`, target-specific Cargo sections, and Cargo build
  scripts to `BuildVariantAmbiguity`, macro/proc-macro syntax to
  `MacroOrPreprocessor`, trait-object dispatch to `FrameworkMagic`, and stale
  Rust source evidence to `StaleEvidence`.
- Rust UNKNOWNs block only the affected claim. `rust_build_variant`,
  `rust_macro_expansion`, `rust_trait_dispatch`, `rust_module_resolution`, and
  `rust_family_membership` block the relevant internal RepoGrammar family
  claim; a Cargo.toml build-script or target-specific `rust_build_variant`
  UNKNOWN blocks Rust self-dogfood family emission for the repository until the
  build variant is resolved. Unrelated optional call-shape details may remain
  non-blocking metadata. Rust UNKNOWNs must not be guessed away by naming
  convention or path similarity.

When a family is emitted with a non-blocking unknown, the affected claim should
name the concrete family and claim whenever possible, such as
`<family_id>:runtime_equivalence`, so downstream agents do not treat the
unknown as repository-global. Current Python family detail/query output also
preserves supported-member non-blocking subclaim unknowns, such as
`<family_id>:fastapi_dependency_target`, through metadata-only family output;
the route family can remain confident while the dependency-target subclaim
stays `UNKNOWN`.

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

When public family output is blocked by `StaleEvidence`,
`InsufficientSupport`, or another claim-relevant `UNKNOWN`, CLI and MCP output
must not present stale or unsupported read-plan spans as authoritative. Matched
fresh families may include read plans and, under explicit opt-in, bounded
source spans. Stale, missing, hash-mismatched, unsupported, dynamic,
insufficient, or conflicting evidence must omit source spans and keep the
recovery guidance explicit instead of implying that source ranges are safe to
trust.

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
