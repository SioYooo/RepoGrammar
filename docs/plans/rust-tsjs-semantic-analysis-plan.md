# Rust and TS/JS Semantic Analysis Plan

- Status: Active implementation plan
- Last updated: 2026-06-29
- Scope: Rust and TypeScript/JavaScript provider-backed semantic preview for
  RepoGrammar pattern-family evidence.
- Related docs: `docs/specifications/product.md`,
  `docs/specifications/domain-model.md`,
  `docs/specifications/indexing-pipeline.md`,
  `docs/specifications/semantic-workers.md`,
  `docs/specifications/unknowns.md`, `docs/roadmap.md`.

## Goal

Upgrade the current Rust self-dogfood preview and transitional TS/JS
exact-anchor preview toward engineering-usable semantic evidence for
pattern-family mining. The goal is not a general call graph tool and not a
claim of complete sound Rust, TypeScript, or JavaScript semantics.

RepoGrammar should remain sound-by-abstention:

- provider-backed facts may support a family only when they are fresh,
  same-generation, repo-relative, hash-checked, and compatible with the
  language/framework role;
- dynamic, unconfigured, stale, conflicting, tool-unavailable, unsupported, or
  heuristic-only cases become typed `UNKNOWN` for the affected claim;
- external compiler, analyzer, LSP, AST, HIR, MIR, TypeScript `Node`, CodeQL, or
  abstract-domain objects never enter core, storage, CLI, or MCP;
- build scripts, procedural macros, package-manager scripts, and repository
  code are not executed by default.

## Current Slice

The current implementation adds Rust-owned provider contract modules:

- `ports::rust_provider` for Cargo metadata, rust-analyzer, rustc, and rustdoc
  JSON backed facts.
- `ports::tsjs_provider` for TypeScript Compiler API, TypeScript Language
  Service, CodeQL, TAJS/JSAI/WALA/Closure-style analysis facts.

These ports define request scope, provenance, cache-key dimensions, output
facts, and provider-unavailable `UNKNOWN`s. They are not adapters and do not
execute tools. Rust build-variant repository blocking is now scoped to the root
`Cargo.toml`; fixture or nested manifests must not globally clear unrelated
Rust family support.

The current implementation also includes an explicit
`adapters::semantic_workers::rust` Cargo metadata provider slice. The product
indexing path now wires it into `index`/`sync`/`resync` as a default-safe Rust
project-model refresh stage after same-generation `Cargo.toml` code units are
stored. It can run `cargo metadata --format-version=1 --no-deps`, parse
workspace/package/target/feature/dependency metadata into owned
`PROJECT_CONFIG` semantic facts, and return recoverable provider `UNKNOWN`s for
unavailable Cargo, unreadable project configuration, or missing manifest
candidates. It rejects requests that claim build-script or proc-macro execution
and does not prove Rust symbol/type/call/family support.
The Tree-sitter Rust parser also uses the parser project context's discovered
`Cargo.toml` texts for bounded cfg triage: `#[cfg]` / `#[cfg_attr]`
build-variant UNKNOWNs can record simple feature predicates and whether the
nearest manifest declares them. This is not cfg evaluation and does not change
the family support gate.

The transitional TS/JS syntax path now carries a bounded project-context slice:
root `tsconfig.json`/`jsconfig.json` `paths` aliases and safe `rootDirs` entries
are parsed as structural project metadata, and unique repo-local literal
relative, path-alias, or rootDirs imports can become `STRUCTURAL`
`RESOLVED_IMPORT` facts. Direct relative resolution wins before rootDirs
fallback. Unresolved or conflicting rootDirs candidates remain typed
`UNKNOWN`; these facts are context/abstention evidence only and do not prove
TypeScript compiler-backed semantics or family support.
The checked-in TypeScript worker now accepts bounded operation requests for
module specifiers, exports, re-exports, and package entries. When a TypeScript
compiler API module is available to the worker, module-resolution facts can be
reported as `SEMANTIC` with `provider=typescript`,
`provider_resolved=true`, strict operation provenance, and config/package
hashes. Without that API, the worker uses only a dependency-free static
project-model fallback and emits `STRUCTURAL` facts or typed `UNKNOWN`s with
`provider_resolved=false`. The current slice does not bundle TypeScript, does
not run package scripts, does not claim full Program/TypeChecker semantic
coverage, and does not allow fallback facts to support family claims.

## Research Sources

Primary sources and high-quality papers reviewed on 2026-06-29:

- Cargo `metadata` command reference for workspace members, packages, targets,
  dependencies, features, and feature-resolution output:
  <https://doc.rust-lang.org/cargo/commands/cargo-metadata.html>
- rustc dev guide for HIR, name resolution, type checking, trait resolution,
  MIR, and borrow checking:
  <https://rustc-dev-guide.rust-lang.org/hir.html>,
  <https://rustc-dev-guide.rust-lang.org/name-resolution.html>,
  <https://rustc-dev-guide.rust-lang.org/type-checking.html>,
  <https://rustc-dev-guide.rust-lang.org/traits/resolution.html>,
  <https://rustc-dev-guide.rust-lang.org/mir/index.html>,
  <https://rustc-dev-guide.rust-lang.org/borrow_check.html>
- Rust RFC 2094 for non-lexical lifetimes:
  <https://rust-lang.github.io/rfcs/2094-nll.html>
- rust-analyzer architecture, including CrateGraph/HIR/Salsa-style incremental
  architecture:
  <https://rust-analyzer.github.io/book/contributing/architecture.html>
- rustdoc JSON unstable output boundary:
  <https://doc.rust-lang.org/rustdoc/unstable-features.html#--output-format-output-format-render-documentation-in-a-different-format>
  and rustdoc JSON type documentation:
  <https://doc.rust-lang.org/nightly/nightly-rustc/rustdoc_json_types/>
- Polonius rules and origin/loan model:
  <https://rust-lang.github.io/polonius/>
- RustBelt formal foundations for Rust safety:
  <https://people.mpi-sws.org/~dreyer/papers/rustbelt/paper.pdf>
- TypeScript Compiler API and Language Service API:
  <https://github.com/microsoft/TypeScript/wiki/Using-the-Compiler-API>,
  <https://github.com/microsoft/TypeScript/wiki/Using-the-Language-Service-API>
- TypeScript module resolution reference, including `node16`, `nodenext`,
  `bundler`, `paths`, package `exports`/`imports`, and extension substitution:
  <https://www.typescriptlang.org/docs/handbook/modules/reference.html>,
  <https://www.typescriptlang.org/tsconfig/moduleResolution.html>
- CodeQL JavaScript/TypeScript data-flow and taint-tracking guide:
  <https://codeql.github.com/docs/codeql-language-guides/analyzing-data-flow-in-javascript-and-typescript/>
- TAJS project and publications for abstract interpretation of JavaScript:
  <https://www.brics.dk/TAJS/>
- JSAI and abstracting abstract machines work for JavaScript analysis:
  <https://dl.acm.org/doi/10.1145/2384616.2384663>
- Approximate call graph construction for JavaScript IDE services:
  <https://doi.org/10.1109/ICSE.2013.6606581>
- WALA JavaScript analysis infrastructure:
  <https://github.com/wala/WALA>
- Google Closure Compiler documentation as a practical typed JavaScript
  compiler baseline:
  <https://developers.google.com/closure/compiler>

## Rust Provider Design

### Project Model

The first Rust provider slice should ingest Cargo metadata into a
RepoGrammar-owned project model:

- workspace root and workspace members;
- package names, manifest paths, and repo-relative package scope;
- targets, target kinds, crate roots, editions, doctest/test/bench/bin/lib
  distinctions;
- dependency edges, dependency kinds, renamed dependencies, platform filters,
  optional dependencies, features, and selected feature sets;
- cfg, target triple, build profile, and toolchain fingerprints;
- build-script/proc-macro presence and execution status.

Cargo metadata is only a project-model source. It does not prove symbol
resolution, type identity, trait dispatch, borrow facts, or runtime behavior.
If metadata cannot be read without executing repository code or without
unexpected network/package resolution, the provider must emit recoverable
`UNKNOWN` scoped to package/crate/project-model claims.

### Semantic Sources

Rust semantic facts need a tiered provider strategy:

- rust-analyzer is the preferred default semantic model for resolved symbols,
  module/import resolution, types, impls, and method dispatch where its HIR
  model can answer without executing build scripts or proc macros. It should be
  treated as an incremental semantic oracle with explicit crate graph, cfg, and
  proc-macro configuration provenance.
- rustc can provide higher-authority checks for HIR, type checking, trait
  solving, MIR, borrow/NLL, and call/effect-like facts only behind an adapter
  that records the exact toolchain and rejects or marks `UNKNOWN` for build
  variants that require executing build scripts/proc macros by default.
- rustdoc JSON can help with exported public item inventories and paths, but it
  is not enough for private items, local dataflow, body-level call/effect facts,
  macro-expanded behavior, or trait dispatch inside function bodies.
- Polonius/RustBelt/NLL literature informs abstention policy for ownership,
  loans, aliasing, and unsafe boundaries; RepoGrammar should not reimplement a
  full borrow checker for certainty claims.

### Rust Facts

Provider output should translate to RepoGrammar-owned facts such as:

- `PROJECT_CONFIG`: Cargo workspace/package/target/feature/cfg facts.
- `RESOLVED_IMPORT`: module, `use`, extern prelude, re-export, and crate edge
  facts when resolved.
- `SYMBOL`: item identity, visibility, associated item, impl, trait, type,
  generic, and bound facts.
- `RESOLVED_CALL`: inherent method, trait method, function, constructor, async
  poll/desugar boundary, and operator target facts only when the provider can
  prove a target under the recorded cfg/feature/toolchain profile.
- `DATAFLOW_DERIVED` or `SEMANTIC` support facts for family evidence only after
  compatibility checks against Rust family tables.
- `UNKNOWN`: macro/proc-macro expansion, build variant, cfg ambiguity, dynamic
  dispatch, trait solver limits, unsafe/FFI effects, generated code, missing
  dependencies, tool unavailability, or conflicting analyzer answers.

### Rust Minimum Implementation Phases

1. Cargo metadata ingestion with repo-relative manifest/target/crate scope and
   fixture coverage for workspace packages, target-specific dependencies,
   features, build scripts, and proc-macro declarations. The first slice parses
   workspace packages, targets, features, and dependencies into owned
   `PROJECT_CONFIG` facts through the default-safe `index`/`sync`/`resync`
   project-model stage.
2. rust-analyzer-backed worker or sidecar that accepts bounded candidates and
   returns owned resolved item/type/import facts or recoverable `UNKNOWN`.
3. Optional rustc-backed cross-checks for claim-upgrading facts, limited by
   recorded toolchain/cfg/feature/build-script/proc-macro status.
4. MIR-like CFG/call/effect summaries for bounded family evidence, never as a
   complete whole-program Rust call graph.
5. Family builder compatibility tables for Rust provider-backed roles beyond
   RepoGrammar self-dogfood, with complete-link clustering over role, support
   family, item kind, cfg/feature, dispatch kind, effect profile, and unsafe
   boundary features.

## TS/JS Provider Design

### Project Model

The first TS/JS provider slice should build a TypeScript project model:

- discovered `tsconfig.json`/`jsconfig.json`, `extends`, composite project
  references, and project root sets;
- `allowJs`/`checkJs`, JSX mode, decorators, emit/module/moduleResolution
  settings, `baseUrl`, `paths`, `rootDirs`, and type acquisition boundaries;
- `package.json` `type`, dependencies/devDependencies, `exports`, `imports`,
  `types`, `main`, and package self-name;
- module resolution mode: `node16`, `nodenext`, `bundler`, and legacy modes
  only when explicitly supported;
- generated/bundler-only aliases as typed `UNKNOWN` unless a configured adapter
  proves them.

### Semantic Sources

- TypeScript Compiler API `Program`/`TypeChecker` is the primary provider for
  project files, symbols, aliases, exports, types, signatures, call-like
  expressions, JSX symbol identity, and compiler module resolution.
- TypeScript Language Service can provide incremental project model and
  resolved semantic answers for editor-like workflows, but cache keys must
  include compiler version, project config hash, file hashes, and module
  resolution settings.
- CodeQL can provide local/global dataflow and taint facts where its JS/TS
  libraries support a source/sink/path claim. CodeQL facts must remain separate
  from TypeScript symbol facts and must carry query identity/version.
- TAJS, JSAI, ACG, WALA, and Closure-style analyses are useful research
  baselines for abstract interpretation, call-graph precision, and typed
  JavaScript tradeoffs. They should inform adapters and tests, not become a
  blanket claim of sound dynamic JavaScript semantics.

### TS/JS Facts

Provider output should translate to owned facts such as:

- `PROJECT_CONFIG`: project references, compiler options, package metadata, and
  module-resolution mode.
- `RESOLVED_IMPORT`: imports, exports, re-exports, CJS `require`, package
  exports/imports, path aliases, and repo-local module targets when resolved by
  the provider.
- `SYMBOL`: declarations, aliases, type symbols, JSX components, decorators,
  namespace/object member identities, and exported names.
- `RESOLVED_CALL`: call targets only where TypeChecker or a dataflow provider
  can prove a stable target under the recorded project model.
- dataflow/taint/effect facts for claim-specific adapters.
- `UNKNOWN`: `eval`, dynamic import, non-literal/conditional `require`,
  prototype mutation, decorators with runtime rewriting, proxies, ambient
  globals without project context, bundler-only aliases, unresolved package
  exports/imports, dynamic property access, framework wrappers, and conflicting
  analyzer answers.

### TS/JS Minimum Implementation Phases

1. Project discovery for tsconfig/jsconfig references plus package metadata.
2. Bounded TypeScript worker operation slice for module/import/export/package
   facts with strict path/hash/range provenance. This is implemented for
   operation-scoped requests and optional compiler module resolution; broader
   Program construction remains future work.
3. TypeScript worker that builds a `Program`, uses the compiler's module
   resolver and `TypeChecker`, and emits owned module/import/export/symbol
   facts.
4. Language Service cache mode for repeated indexing when project hashes match.
5. Resolved call-target facts only where TypeChecker can prove target identity.
6. CodeQL or bounded abstract-interpretation dataflow/taint facts for
   claim-specific families.
7. Framework adapters for Express/Fastify/Jest/Vitest/Next/Prisma/Drizzle/React
   only when provider facts and compatibility tables support the exact claim.

## Family Builder Requirements

- Rust and TS/JS need explicit compatibility tables by language, framework
  role, code-unit kind, support target family, dispatch/resolution mode, and
  dynamic boundary.
- Support requires fresh same-generation `SEMANTIC` or `DATAFLOW_DERIVED`
  evidence from provider-backed or derivation-safe origins. Framework
  heuristics, syntax-only facts, and package metadata do not prove membership.
- Complete-link clustering must include semantic compatibility features so one
  bridge member cannot connect incompatible families.
- Source spans remain opt-in and hash-checked. Provider facts must never render
  source snippets by default.
- Analyzer disagreement must become `ConflictingFacts` or claim-scoped
  `UNKNOWN`, not majority voting.

## Fixture Matrix

Rust fixtures should cover workspace packages, feature-gated modules, target
cfg, macros, proc macros, trait methods, inherent methods, async functions,
generics, `impl Trait`, `dyn Trait`, build scripts, re-exports, module
resolution, unsafe/FFI, and generated-code boundaries. A fixture `Cargo.toml`
must not globally block unrelated root Rust families; root manifest
build-variant ambiguity may still block repository-wide Rust self-dogfood
claims.

TS/JS fixtures should cover `node16`, `nodenext`, and `bundler`; path aliases;
package `exports`/`imports`; CJS/ESM interop; re-exports; JSX; decorators;
dynamic `require`/`import`/`eval`; framework route/test/db adapters; ambient
globals; package self-name imports; and unsupported bundler-only or runtime
rewriting cases.

## Explicit Unsupported Claims

RepoGrammar must not claim:

- complete sound general Rust semantics;
- complete Rust trait solving, borrow checking, unsafe/FFI effect modeling, or
  macro/proc-macro expansion when the configured provider did not prove it;
- complete sound JavaScript semantics;
- complete TypeScript/JavaScript call graphs under dynamic property access,
  prototype mutation, decorators, proxies, eval, bundler transforms, or runtime
  dependency injection;
- React, Next, Prisma, Drizzle, Express, Fastify, or test framework runtime
  equivalence without role-specific provider facts and compatibility gates;
- provider-backed semantic support when providers are unavailable, stale,
  mismatched to current source hashes, or configured with different project
  options than the indexed generation.
