# ADR-0017: Optional semantic provider capability registry

- Status: Accepted
- Date: 2026-07-04
- Refines: ADR-0015 (provider-backed semantic analysis execution), the
  provider-platform "Phase 1" that is buildable without a real analyzer.
- Related: ADR-0016 (semantic obligation refinement),
  `docs/specifications/unknowns.md`

## Context

ADR-0015 opened a staged, consent-gated program to execute optional
language-native semantic providers. Its platform layer — a capability registry
and an honest availability report — is buildable and useful **before** any real
analyzer is integrated, and without bundling one. RepoGrammar already carries
provider *ports* (a TypeScript worker that emits `provider_resolved=true` facts;
Python/Rust provider traits) but had no first-class, source-free way to answer
"which optional accelerators exist, what would they resolve, and are they
available here?" Without that, a missing analyzer reads as an opaque gap rather
than an optional enhancement that is simply absent.

## Decision

Add a source-free optional-provider capability registry and surface it in
`doctor --json`. It executes no analyzer and introduces no external dependency;
it reports capability and availability only.

### D1. Fixed, source-free slot vocabulary

`SemanticProviderSlot` is a closed enum: `typescript_compiler` (integrated),
`python_type_provider` and `rust_analyzer` (documented, not yet integrated). Each
slot carries its language and the stable required-mechanism buckets it would
resolve (linking to the UNKNOWN inventory's `by_required_mechanism`). No slot
carries repository-specific text.

### D2. Honest availability states

`ProviderAvailability` is `configured` (integrated and its configuration signal
is present), `available_bundled` (integrated and not configured, but RepoGrammar
ships a worker for it and that worker's runtime is present on the host, so it can
be enabled here right now — still opt-in, never auto-launched), `not_configured`
(integrated but not trivially runnable here), or `not_integrated` (RepoGrammar
has no adapter for this slot yet). Detection reads configuration signals and a
runtime `PATH` scan only (for TypeScript, the existing
`REPOGRAMMAR_TYPESCRIPT_WORKER` and whether `node` — the bundled
`src/workers/typescript/worker.js` runtime — is on `PATH`); it never executes an
analyzer or the worker. A non-integrated slot is always `not_integrated`
regardless of any stray signal or present runtime, so a provider that does not
exist can never be falsely reported as present. `available_bundled` respects the
ADR-0015 D1 consent boundary: it reports that the provider *can* be enabled, but
nothing is launched until the operator sets the configuration signal.

### D3. Missing providers are never fatal

The registry is informational. It is reported alongside `doctor`'s `checks`, not
inside them, so an absent optional provider never becomes a doctor failure and
never affects health status. The baseline product continues to work with every
provider absent, reporting provider-dependent gaps as typed `UNKNOWN`s.

### D4. Runtime provider statuses are deferred

Runtime-only statuses (timeout, conflict, unsupported query) are **not** modeled
here, because no producer exists until an analyzer actually executes. Adding them
now would be speculative. They belong to a later adapter-execution ADR, together
with the provider fact contract and freshness/conflict handling from ADR-0015.

## Alternatives considered

- **A new top-level `providers` command:** rejected for now — the v0.1 CLI is
  kept minimal/pattern-family-first; `doctor` already inspects environment health
  and is the natural home for optional-provider availability.
- **Map the Python worker override to a type provider:** rejected —
  `REPOGRAMMAR_PYTHON_WORKER` configures only the syntax worker, not a type
  checker, so treating it as a `python_type_provider` signal would be false.
- **Model runtime statuses (timeout/conflict) now:** rejected as speculative —
  no analyzer executes, so there is no producer for those states.

## Consequences

- Agents can see, source-free, which optional accelerators exist, what each would
  resolve, and whether it is present — turning "analyzer missing" into an
  actionable, non-fatal optional-enhancement signal.
- When a real adapter is integrated (ADR-0015 D4), flipping a slot's
  `is_integrated` and wiring its detection is a localized change, and runtime
  provider statuses can be added with their producer.
