# Rust language completion review

- Language: Rust
- Frozen Top-20 rank: 10
- Lane: C0 current-language convergence
- Audited integration baseline: `8a4033e3eddc93787c997d13c54bba22b1df7102`
- Completion state: **Incomplete — `structural_substrate` under ADR-0020**
- Counted in Top-20 denominator: **yes**
- Counted as Top-20 complete: **no**
- Last reviewed: 2026-08-01

Rust has both RepoGrammar self-dogfood families and general framework preview
families. Neither is a language-native semantic frontend. Cargo metadata is a
real project-model provider but does not prove Rust source semantics, and it
cannot by itself satisfy ADR-0020's frontend or third-party-behavior gates.

## Scope and dialect boundary

The current source scope is exact lowercase `.rs` under Tree-sitter Rust syntax.
Exact root or nested `Cargo.toml` files are project configuration. Cargo metadata
uses the host Cargo/toolchain when available, but no single Rust edition,
toolchain, target, feature set, workspace profile, or build-script/proc-macro
environment is pinned as a completed language scope. `Cargo.lock`, generated
source, macro-expanded source, target-specific compilation, rustdoc JSON, and
IDE state are not authoritative source inputs for current families.

The family scope includes RepoGrammar-internal module roles plus exact use/FQN
gated serde, thiserror, tokio, clap, and literal axum route shapes. This does not
claim arbitrary Rust libraries or frameworks.

## Current implementation evidence

### Discovery, frontend, owned IR, and project model

- Discovery covers `.rs` and nested `Cargo.toml`, applies global limits,
  git-ignore and symlink containment, and excludes `target` and other documented
  generated/dependency paths.
- `src/rust/adapters/parsing/rust/` uses Tree-sitter Rust to emit structural
  modules, functions, traits, impls, methods, types, macro invocations, framework
  units, exact use/attribute/call anchors, module candidates, diagnostics, and
  typed `UNKNOWN`s. Its module comment and product specification explicitly say
  it is structural only.
- Parser-native nodes do not leave the adapter. Units, ranges, hashes, IR
  nodes/edges, facts, evidence, provenance, and storage records are owned by
  RepoGrammar.
- `src/rust/adapters/semantic_workers/rust.rs` runs bounded
  `cargo metadata --format-version=1 --no-deps` after same-generation manifest
  units exist. It translates workspace/package/target/feature/direct-dependency
  data into owned project facts and dependency records. It does not build,
  expand macros, execute build scripts, or resolve source symbols/types/calls.
- `rust_analyzer` is a registered provider slot but is `not_integrated`.
  `src/rust/ports/rust_provider.rs` defines future rust-analyzer/rustc/rustdoc
  request, provenance, cache, output, and unavailable-UNKNOWN shapes only.

### Package metadata and third-party-library analysis

Cargo metadata is ADR-0030's first provider consumer, but direct dependencies
remain `manifest_declared` because `--no-deps` does not prove a resolved
transitive graph. Records include package identity, requirement, scope,
optionality, directness, and manifest evidence where available. They do not
claim installed versions, source/checksum, lockfile coherence, external item
identity, feature activation, or runtime selection. The provider is recomputed
for incremental generations rather than copying stale rows.

Third-party source semantics remain incomplete. There is no isolated
rust-analyzer/rustc/rustdoc adapter, package-qualified external item identity,
reviewed library-contract registry, or controlled connection between Cargo
inventory and family support. A `serde` name in `Cargo.toml`, for example, is
not proof that a derive resolves to the intended crate or that generated trait
implementations exist. Current families use exact source-visible use/FQN anchors
and preserve macro/trait behavior as `UNKNOWN`.

### Exact families and typed `UNKNOWN`

Self-dogfood families cover repeated RepoGrammar implementation roles. General
preview families use exact same-file imports or FQNs for serde derive models,
thiserror error enums, tokio entry/tests, clap derives, and literal axum routes.
The shared family gate requires support at least three and compatible profiles;
`repogrammar-rust-derived`/`bounded_tree_sitter_anchor_v1` is structural derived
support, not provider-backed semantics.

Typed `UNKNOWN`s cover macro/proc-macro expansion, cfg/build variants,
build-script presence, unresolved/conflicting modules and external imports,
trait-object dispatch, framework magic, derive-without-use, dynamic/untraceable
axum receivers/routes, middleware/extractor semantics, stale evidence,
conflicts, provider unavailability, and insufficient support. Root build-variant
ambiguity can block affected repository families; nested fixture manifests are
kept package-scoped. The recoverable/irreducible governance is useful, but
without rust-analyzer/rustc the recoverable symbol/type/trait obligations cannot
be discharged in production.

## Four-part review

### Correctness and bug findings

Positive evidence includes stable unit kinds, module candidate resolution,
exact use/FQN gates, same-file binding rules, cfg feature context, build-variant
blocking, support thresholds, complete-link compatibility, low-support
abstention, family-specific compatibility profiles, and unresolved/resolved
fixtures for modules and serde anchors.

Completion-blocking correctness gaps remain: Tree-sitter is the only source
frontend; Cargo metadata is not a source semantic oracle; editions/targets/
features are not selected as one authoritative build model; macro expansion,
trait resolution, method dispatch, external crate item identity, and generated
code remain unresolved; and parse-degraded Rust family non-formation is not
represented by a dedicated ADR-0020 completion fixture. These gaps must not be
guessed from imports, attributes, or manifest names.

### Security and untrusted-input handling

Current positive boundaries include repo-root validation, absolute manifest
path containment, bounded JSON parsing/output, timeout supervision, `--no-deps`,
no builds/tests/runs, no build-script or proc-macro execution, build-script
sentinel tests, source-free output, and sanitization through owned types.

Cargo is nevertheless an external executable using ambient toolchain state;
the metadata-only safety contract must stay narrow. A future Rust semantic
provider must pin the toolchain/provider, isolate filesystem/network/child
processes, disable builds/build scripts/proc macros by default, bound CPU/memory/
time/output, hash cfg/profile/Cargo inputs, constrain sysroot/cache access, and
sanitize diagnostics. No rust-analyzer/rustc adapter has passed that review.

### Implementation completeness

Discovery, structural source units/IR, bounded Cargo project metadata, exact
self-dogfood/general family substrate, typed unknowns, fixtures, and source-free
read surfaces exist. Missing items are the authoritative source semantic
frontend, project/profile resolution, provider-backed unknown discharge,
package-qualified external symbols/library contracts, parse-degraded completion
fixtures, final resource review, and an atomic D2 submodule chain.

### Performance and resource bounds

Tree-sitter parsing, bounded manifest reads, output limits, provider timeout,
file-local incremental paths, Cargo metadata recomputation, and generic read/
write benchmark fixtures provide architectural bounds. No representative
rust-analyzer/rustc completion benchmark covers workspaces, macro-heavy crates,
feature matrices, target variants, cache invalidation, or large external
dependency graphs because the provider is absent. Cargo process startup and
workspace metadata costs are not a substitute for semantic-provider
measurements. Performance closure therefore fails.

## ADR-0020 nine-gate checklist

| Gate | Result | Audited evidence and blocker |
|---|---|---|
| 1. Discovery/config | Partial | `.rs`, nested `Cargo.toml`, exclusions, limits, symlinks, and Cargo project metadata exist; edition/toolchain/target/feature/profile selection and lock/build context are not closed. |
| 2. Authoritative frontend | Fail | Source analysis is Tree-sitter-only. Cargo metadata is an authoritative project-format tool, not a Rust source semantic frontend; rust-analyzer/rustc/rustdoc is not integrated. |
| 3. Owned code units/IR | Pass | Stable Rust units, ranges, hashes, owned IR/facts/evidence/provenance, persistence, and readback exist. |
| 4. Typed `UNKNOWN` | Partial | Broad cfg/macro/module/dispatch/provider/stale/conflict handling exists, but the authorized provider needed to resolve recoverable obligations is absent and degraded syntax is not fully audited. |
| 5. Family-first exact anchor | Pass as substrate | Self-dogfood and general exact-anchor families meet support >= 3 and compatibility gates; they remain structural. |
| 6. Fixture proof | Partial | Positive, lookalike, low-support, macro/cfg/build, module/serde unresolved-resolved, and build-script sentinel fixtures exist. A dedicated parse-degraded/provider stale-conflict completion matrix is missing. |
| 7. Source-free readiness | Partial | Rust counts, unknowns, CLI/MCP family paths, and leakage behavior have tests, but no final Rust-specific provider/readiness audit links all surfaces. |
| 8. Four-part review | Fail | This review records unresolved source correctness, provider security, completeness, and representative performance evidence. |
| 9. Atomic delivery/audit | Fail | The principal Rust support landed as one aggregate commit, not independently complete D2 modules, and no final linked audit reran all required gates. |

Result: **not 9/9; Rust must not increment the ADR-0020 completed-language
count.**

## Evidence paths

- Authority: `docs/decisions/ADR-0020-top-20-language-expansion-gate.md`,
  `docs/decisions/ADR-0030-language-neutral-dependency-library-semantics.md`
- Specifications: `docs/specifications/semantic-workers.md`,
  `docs/specifications/indexing-pipeline.md`,
  `docs/specifications/unknowns.md`, `docs/specifications/product.md`,
  `docs/development/testing.md`
- Discovery/parser: `src/rust/adapters/filesystem/discovery.rs`,
  `src/rust/adapters/parsing/rust/`
- Project/provider: `src/rust/adapters/semantic_workers/rust.rs`,
  `src/rust/ports/rust_provider.rs`, `src/rust/core/model/provider.rs`,
  `src/rust/application/providers.rs`
- Family/pipeline: `src/rust/adapters/frameworks/rust_general.rs`,
  `src/rust/application/indexing.rs`, `src/rust/application/family.rs`
- Fixtures: `src/fixtures/rust/release/v0_2/`,
  `src/fixtures/unknown_reduction/rust_module_unresolved/`,
  `src/fixtures/unknown_reduction/rust_module_resolved/`,
  `src/fixtures/unknown_reduction/rust_serde_unresolved/`,
  `src/fixtures/unknown_reduction/rust_serde_resolved/`

## Evidence-bearing historical SHAs

These ancestor commits identify current substrate, not a qualifying D2/G9
prerequisite chain:

- `6561987ef2a01d5b920315a9afe97be92e57853b` — shared discovery, Cargo provider,
  storage, indexing, query, and lifecycle substrate.
- `f52c001c4b8eacfdb4b06de7e9c519f9e0595cea` — aggregate Rust self-dogfood,
  general framework, UNKNOWN, fixture, and family landing.
- `9ea396b96ae11eb79ec9f80dd52b66e631c7dc1a` — language-neutral dependency
  evidence model and Cargo consumer contract.
- `5ef354249b58784b1e872aa6ac3d83c7502dd4c8` — dependency persistence.

## Required next closure sequence

1. Pin the initial edition/toolchain/target/feature/profile boundary and approve
   an isolated rust-analyzer or rustc/rustdoc provider contract with explicit
   build-script/proc-macro abstention.
2. Add candidate-scoped module, item, type, trait, and external-crate identity
   operations with provenance, cache, stale/conflict/failure handling, and
   controlled unresolved-to-resolved fixtures.
3. Add package-qualified external-symbol and reviewed library-contract layers;
   keep Cargo manifest presence out of family support.
4. Add dedicated parse-degraded and provider absent/present/stale/conflicting
   product fixtures, then audit one exact serde/tokio/axum or self-dogfood family
   end-to-end with leakage and resource measurements.
5. Land coherent submodule commits, run the full required gates, and link exact
   SHAs in the final Rust completion audit.

## Risks and non-claims

- This report does not claim rustc-equivalent parsing, name/type/trait
  resolution, borrow checking, macro expansion, target/feature completeness,
  external-crate item identity, or runtime behavior.
- Cargo `manifest_declared` rows do not prove lock resolution, installation,
  selected feature activation, buildability, or library behavior.
- Exact `use` paths and attributes are bounded source anchors, not proof of
  generated implementations or trait dispatch.
- Self-dogfood evidence is RepoGrammar-specific and cannot be generalized to all
  Rust repositories.
- Build scripts and procedural macros remain intentionally unexecuted; their
  affected facts must stay `UNKNOWN`.

## Completion verdict

**Incomplete — `structural_substrate`; Top-20 counted = yes; Top-20 complete =
no.** Rust has strong structural and Cargo project-model substrate, but the
authoritative semantic provider, complete fixtures/review, and linked atomic
audit required for ADR-0020 `bounded_preview` are absent.
