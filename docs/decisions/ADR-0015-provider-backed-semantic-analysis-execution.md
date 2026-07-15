# ADR-0015: Provider-backed semantic analysis execution program

- Status: Accepted (explicit maintainer direction, 2026-07-11)
- Date: 2026-07-03 (proposed); accepted 2026-07-11
- Refines: ADR-0004 (Rust core with language-native workers),
  ADR-0012 (Python selective analysis cascade)
- Related: `docs/reports/unknown-resolution-sota-analysis.md`,
  `docs/specifications/python-analysis.md`,
  `docs/plans/rust-tsjs-semantic-analysis-plan.md`,
  `docs/specifications/unknowns.md`, `docs/roadmap.md`

This ADR is a decision proposal only. It adds no production dependency, wires no
adapter, and executes no external tool. It exists so a maintainer can approve
(or reject) the program before any provider adapter, dependency, or execution
path is implemented, per the repository rule that a production dependency
requires an accompanying decision update.

## Context

`docs/reports/unknown-resolution-sota-analysis.md` inventories every typed
`UNKNOWN` RepoGrammar emits across Python, Rust, TypeScript/JavaScript, and Java
and classifies each as (a) already recovered by a bounded structural mechanism,
(b) **recoverable only by a language-native semantic provider**, or (c)
**irreducible** by static analysis under the no-execution rule.

The structural slice has taken class (a) as far as it soundly goes without a
provider: repo-local import graphs, the pytest fixture graph, `cargo metadata`
project model, bounded `tsconfig` resolution, and exact Spring anchors. The
large remaining class (b) — canonical framework identity, decorator/annotation
target resolution, cross-crate/workspace/`.d.ts` module resolution, trait/method
dispatch, Spring DI/component/Data models, and the third-party interface tail —
cannot be resolved by more structural parsing. It requires executing a
language-native analyzer (a type checker / language server), which the roadmap
and ADR-0012 designed but explicitly deferred at the execution level. Java has
no provider boundary at all today.

Class (c) is not a gap to close. `MonkeyPatch`, non-literal `DynamicImport`,
`eval`/`exec`, Rust macro/proc-macro expansion and build-script output, dynamic
`dyn` dispatch under an open world, TS/JS dynamic `import()`/proxy/prototype
mutation/bundler transforms, and Java reflection/runtime component scan/AOP
proxy are runtime-defined. Resolving them would require executing repository
code or manufacturing certainty, both forbidden. They must remain first-class
typed `UNKNOWN`.

The decision this ADR resolves: do we open a staged program that **executes
language-native semantic analyzers** (not repository runtime code) behind
consent and soundness boundaries, to convert the recoverable class (b) UNKNOWNs
into source-backed facts, while leaving class (c) as `UNKNOWN`?

## Decision

Adopt a staged, consent-gated, provider-backed semantic analysis execution
program. It turns only the recoverable UNKNOWN categories into
RepoGrammar-owned, source-backed facts, in the expected-value order from the
report, and it never weakens the UNKNOWN contract.

### D1. Two distinct execution classes, two consent levels

- **Analyzer execution (this program enables, opt-in):** running a
  language-native static analyzer / language server that parses and type-checks
  source without running the repository's runtime behavior — Pyrefly, Pyright,
  rust-analyzer, `rustc --emit=metadata`-style checks, the TypeScript
  `Program`/`TypeChecker`, and `javac`/Eclipse JDT. These read source and
  resolve names/types; they do not run application entrypoints. They may be
  enabled per adapter through explicit configuration, never during a default
  `index`/`sync` unless the adapter is configured.
- **Runtime execution (still forbidden by default):** build scripts,
  procedural-macro expansion, package/install scripts, annotation processors,
  Spring runtime wiring, pytest, and RightTyper-style observed tracing run
  repository or generated code. These stay off by default. Observed-runtime
  evidence remains a separate opt-in tier (ADR-0012) with its own certainty
  token and consent boundary and must never generalize beyond the observed
  execution.

An analyzer that can only answer a query by triggering runtime execution (for
example proc-macro-dependent name resolution, or build-script-generated modules)
must return a recoverable `UNKNOWN` for that claim rather than executing.

### D2. Dependency and isolation policy

- Providers are invoked as **external tools over subprocess/CLI/LSP
  boundaries**, at pinned versions, with provider internals (HIR, MIR, TS
  `Node`, JDT AST, abstract domains) never entering RepoGrammar core, storage,
  CLI, or MCP. Only RepoGrammar-owned facts cross the boundary.
- Each provider is a **separate, optional adapter with its own follow-up
  decision** for how it is obtained (system-detected binary, pinned toolchain,
  or — only where unavoidable, e.g. the TypeScript compiler — an explicitly
  bundled dependency). No provider is a hard production dependency of the core
  binary. When a provider is unavailable, stale, version-mismatched, or
  configured differently from the indexed generation, its claims become
  recoverable `UNKNOWN`.

### D3. Soundness, provenance, and conflict rules (unchanged)

- Provider facts may support a family only when fresh, same-generation,
  repo-relative, hash-checked, and compatible with the language/framework role.
  They carry provider name + pinned version, config hash, environment
  fingerprint, source content hash, source range, query operation, and freshness
  (per ADR-0012 provenance dimensions).
- Provider disagreement becomes `ConflictingFacts` or claim-scoped `UNKNOWN`;
  never majority-voted away when the losing fact would change behavior,
  security, persistence, authorization, transactionality, async lifecycle, error
  mapping, or external effects.
- Structural or fallback facts with `provider_resolved=false` remain context and
  never suppress an UNKNOWN or support a family.

### D4. Per-language sequencing (EV order; refines existing plans)

Each stage ships behind its own adapter follow-up and the UNKNOWN regression
benchmark (`docs/experiments/unknown-regression-benchmark.md`): an
unresolved→resolved fixture pair must reduce a named required-mechanism bucket
or blocking UNKNOWN *and* prove a source-backed replacement fact, while the
unresolved side still forms no family and the dynamic tail stays `UNKNOWN`.

1. **Python — Pyrefly primary** (`ports::python_provider` already exists).
   Resolves the recoverable part of `FrameworkMagic` (decorator/canonical
   identity), untyped `RuntimeDependencyInjection`, and the third-party tail of
   `UnresolvedImport`/`MissingDependency` via type resolution.
2. **Python — Pyright cross-check** for claim-upgrading facts only; enables a
   future `CROSS_CHECKED_SEMANTIC` tier (ADR-0012) without weakening gates.
3. **Python — bounded no-provider wins** shippable ahead of Pyrefly and inside
   the current no-execution rule: a spec-sanctioned pytest **plugin-fixture
   allowlist** (reduces `PytestFixtureInjection`) and widened safe project-config
   parsing for `setup.cfg`/`requirements.txt` (reduces `MissingProjectConfig`).
4. **Rust — rust-analyzer worker** on top of the `cargo metadata` model.
   Resolves the `UnresolvedImport` glob/alias/cross-crate tail, `ConflictingFacts`
   module precedence, and the statically-monomorphizable part of
   `rust_trait_dispatch` — without executing build scripts or proc-macros.
5. **Rust — bounded cfg/feature product model** to reduce `BuildVariantAmbiguity`
   for a pinned feature/target set (variability analysis over declared features).
6. **TS/JS — TypeScript `Program`/`TypeChecker` worker** (behind the Python-first
   lock / a scope ADR) for the `UnresolvedImport` ambient/workspace/type-only
   tail and star-re-export `ConflictingFacts`.
7. **Java — `javac`/Eclipse JDT provider + Spring model** (new provider
   boundary; preview only, behind its own ADR): FQN/classpath resolution for
   `java_spring_annotation_binding`, and declarative `spring_di_model` /
   `spring_component_scan_model` / `spring_data_repository_model` (derived-query
   grammar) for the statically declared Spring subset.

### D5. Irreducible set stays UNKNOWN

This program does not target and must not "resolve" the class (c) irreducible
set (report §7). Those remain typed, first-class `UNKNOWN`. Any adapter that
appears to resolve them is presumed to be manufacturing false certainty and must
be rejected.

## Alternatives considered

- **Stay structural-only (no provider execution):** rejected — it permanently
  leaves the recoverable class (b) UNKNOWNs unresolved and caps family evidence
  density below the product goal.
- **Execute all analyzers (and runtime) by default during `index`/`sync`:**
  rejected — violates the no-execution and consent boundaries, risks running
  repository code, and adds cost/noise without per-claim need.
- **Bundle every provider as a hard core dependency:** rejected — providers must
  stay optional adapters; the core must work (as recoverable `UNKNOWN`) without
  them.
- **Use LLM or neural inference to "resolve" UNKNOWNs (including the irreducible
  set):** rejected — not auditable provenance; would convert dynamic behavior
  into false certainty.

## Consequences

- Each adopted provider adds an **opt-in** dependency/toolchain requirement,
  gated by its own follow-up decision, with a recoverable-`UNKNOWN` fallback when
  absent. The core binary gains no hard dependency from this ADR.
- The validation surface grows: each adapter needs positive/negative/stale/
  conflicting/dynamic fixtures and must extend the UNKNOWN regression benchmark
  baselines.
- New certainty tiers (`CROSS_CHECKED_SEMANTIC`, `OBSERVED_SEMANTIC`) may be
  introduced only with matching Rust domain, schema, storage, CLI, MCP, and test
  updates (ADR-0012). Until an adapter ships and its benchmark pair proves a
  source-backed replacement, documentation must not claim the corresponding
  UNKNOWN is resolved.
- "Resolve all UNKNOWNs" remains false as stated: this program resolves the
  recoverable class only; the irreducible class stays `UNKNOWN` by design.

## Follow-up work

- One adapter, one follow-up ADR/plan update, one benchmark pair at a time, in
  the D4 order. Start with the D4.3 no-provider Python wins (allowlist +
  config widening) since they need neither new dependencies nor execution.
- Add per-adapter dependency-acquisition decisions (system binary vs pinned
  toolchain vs bundled) before wiring each provider.
- Update `docs/roadmap.md`, `docs/specifications/python-analysis.md`, and
  `docs/plans/rust-tsjs-semantic-analysis-plan.md` as each stage lands.
- Extend `docs/experiments/unknown-regression-benchmark.md` baselines whenever a
  stage reclassifies an UNKNOWN, keeping dynamic/ambiguous/stale/external cases
  typed `UNKNOWN`.
- Add a Java provider boundary decision before any Java `javac`/JDT work.
