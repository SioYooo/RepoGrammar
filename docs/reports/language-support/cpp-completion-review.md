# C++ language completion review

- Language / Top-20 rank: C++ / 3
- Review baseline: `8a4033e3eddc93787c997d13c54bba22b1df7102`
- Current product label: shared C/C++ `bounded_v0_2_preview`
- ADR-0020 completion state: Incomplete — bounded structural preview under
  convergence audit
- Counted as Top-20 complete: **No**
- Last updated: 2026-08-01

The existing exact test-framework families are meaningful bounded preview
evidence. They do not compensate for the missing compilation-context frontend,
translation-unit/header model, complete semantic obligations, fixture/readiness
matrix, or linked nine-gate audit.

## Capability snapshot

| Area | Current evidence | Completion boundary |
|---|---|---|
| Discovery | `.cc`, `.cpp`, `.cxx`, `.hh`, `.hpp`, and `.hxx` select C++; `.c` and `.h` select C. Generic discovery applies repository containment and a 1 MiB source limit. | A conventional C++ `.h` header is classified as C. Discovery is not selected from a compilation database or build target. |
| Dialect / variant | Tree-sitter C++ `0.23.4`; bounded structural handling of macros, include guards, and conditional regions. | No C++ standard, compiler extensions, target, flags, include paths, defines, generated configuration, or active preprocessor branch is authoritative. |
| Frontend / provider | In-process Tree-sitter syntax analysis only. | No clangd/libclang/Clang frontend or C++ provider slot; no compile, preprocess, build, moc, protoc, or package-manager execution. |
| Owned code units / IR | RepoGrammar-owned module, class, function, fixture, suite, and test units plus structural IR. | Syntax-derived units are not bound across authoritative translation units and do not resolve templates, overloads, ADL, macros, linkage, or external symbols. |
| Project model | Exact-root `compile_commands.json` records entry count and at most 100 safe repository-relative translation-unit paths. | Commands, arguments, directories, flags, dialects, target, staleness, and configuration selection are discarded; headers are not assigned to authoritative translation units. |
| Package metadata | Exact-root `vcpkg.json` and `conanfile.txt` yield bounded direct declarations. | No lockfile/installed/transitive graph, profiles, options, features, toolchain selection, binary identity, or selected package version. |
| Third-party analysis | vcpkg/Conan declarations are auxiliary `manifest_declared` evidence. | No package manager is run; no external symbol, API, header ownership, ABI, or reviewed library contract is resolved. |
| Exact families | Exact include-gated GoogleTest/gMock, Catch2, doctest, and Boost.Test structural evidence; product tests prove GoogleTest and Catch2 families with support at least three. | Family membership remains source/config scoped and cannot establish compilation, template expansion, generated registration, runtime discovery, or external library compatibility. |
| Typed `UNKNOWN` | Parse degradation, macro contracts, conditional/preprocessor variants, lookalikes, and low support abstain conservatively. | Translation-unit selection, templates/overloads, include resolution, generated sources, stale/conflicting configs, and provider fallbacks are not a complete registry. |
| Source-free readiness | Family detail can omit snippets; product tests cover exact, lookalike, low-support, macro, and preprocessor-variant outcomes. | The full CLI/MCP/status/doctor/stats/unknowns/persistence/leakage matrix is incomplete. |

ADR-0030 keeps the current vcpkg/Conan rows at
`manifest_declared`. They cannot become `lockfile_resolved` or
`provider_resolved`, and package presence cannot support family membership.

## Four-part review record

### Correctness and bug findings

- Exact include and macro checks reduce false positives for supported test
  frameworks, and low support/lookalikes/variants abstain rather than inflating
  family claims.
- `.h` is routed to C even when a real build treats it as C++. Without a
  selected compilation command, header-language and macro context remain
  unresolved.
- `compile_commands.json` does not retain the command-line state required to
  reproduce parsing. Multiple build configurations, stale entries, and
  generated translation units are not reconciled.
- Tree-sitter structure cannot resolve template instantiation, overloads, ADL,
  conditional includes, generated test registration, or external declarations.
  Current families must remain exact bounded structural claims.

### Security and untrusted-input handling

- Analysis is non-executing and does not invoke Clang, a build system, package
  manager, test binary, macro expander, moc, or protoc.
- Discovery checks repository containment and caps source input at 1 MiB.
  Config parsing is bounded: dependencies cap at 2,000; JSON member, key, and
  depth limits apply; duplicate or malformed JSON fails closed; unsupported
  Conan/vcpkg shapes produce typed dependency-inventory UNKNOWNs.
- Tree-sitter parses untrusted input in process. A future compiler provider
  needs OS sandboxing, filesystem/network/process containment, resource and
  output budgets, normalized diagnostics, and fail-closed provider fallback.
- The incomplete product-surface leakage matrix leaves source-free readiness
  partial even though family detail omits source snippets by default.

### Implementation completeness

Implemented work includes discovery, Tree-sitter structural IR, bounded
preprocessor/include evidence, exact test-framework families and fixtures,
unknown-reduction fixtures, dependency/config inventory, incremental storage,
and conservative no-execution behavior. Missing work includes an authoritative
translation-unit/header model, clangd/libclang provider and policy, complete
template/macro/generated-code obligations, conflict/staleness and
resolved/unresolved coverage, full product readiness/leakage proof, and the
linked final audit.

### Performance and resource bounds

- Source files are capped at 1 MiB. Dependency inventory caps at 2,000 entries;
  JSON parsing caps members at 8,192, key length at 256, and depth at 128.
- Structural preprocessing and test-framework extraction traverse bounded
  syntax; Boost suite tracking uses a depth-bounded stack. Project config keeps
  at most 100 safe translation-unit paths.
- No benchmark closes dense macro/conditional input, large header fan-out,
  template-heavy sources, or multi-configuration scaling. A future provider
  must add explicit translation-unit, diagnostic, time, memory, process, and
  output ceilings.

## ADR-0020 nine-item completion gate

| Gate | Status | Evidence or exact gap |
|---|---|---|
| G1 Discovery/configuration | Partial | C++ extensions and bounded shared config inventory exist; authoritative dialect, header, build variant, and configuration selection do not. |
| G2 Authoritative frontend/provider | Open | Tree-sitter only; no admitted clangd/libclang/Clang provider or translation-unit authority. |
| G3 RepoGrammar-owned units/IR | Satisfied | Structural C++ code/test units and IR are RepoGrammar-owned. |
| G4 Typed `UNKNOWN` / fallback | Partial | Parse/macro/preprocessor uncertainty exists; template, include, generated, config-conflict/staleness, and provider obligations are incomplete. |
| G5 Exact recurring family | Satisfied | Exact include-gated GoogleTest and Catch2 families have product evidence with support at least three; doctest and Boost.Test structural support is also present. |
| G6 Completion fixture matrix | Partial | Exact, lookalike, low-support, macro, preprocessor, and unknown-reduction fixtures exist; the full variant/stale/conflict/provider resolved-unresolved matrix does not. |
| G7 Source-free readiness | Partial | Conservative product tests and snippet-free family detail exist; the complete public-surface leakage matrix is not closed. |
| G8 Four-part review | Satisfied by this snapshot | This document records findings and bounds; open findings remain blockers/risks and require tests. |
| G9 Atomic delivery/final audit | Open | Substrate commits exist, but no completion audit links a chain satisfying all nine gates. |

Result: **3 satisfied, 4 partial, 2 open; not 9/9.** C++ remains outside the
Top-20 completion count.

## Prerequisite commit evidence

- `0c8b9024f5db5f86b0d64bca5923cd1b900c868a` — shared C/C++ structural preview and exact structural families.
- `0472e6bf0fa74a37a0bb9e43eb878f2c04185c22` — C/C++ project/dependency inventory.
- `9f1f657a27d45264d3b5e9b681750778234f0cb2` — bounded JSON hardening for configuration input.
- `9ea396b96ae11eb79ec9f80dd52b66e631c7dc1a` and
  `5ef354249b58784b1e872aa6ac3d83c7502dd4c8` — language-neutral dependency
  domain and persistence substrate.

These exact SHAs are prerequisites, not proof of an ADR-0020-complete atomic
delivery chain.

## Primary evidence paths

- Authority: `docs/decisions/ADR-0019-bounded-multi-language-structural-expansion.md`,
  `docs/decisions/ADR-0020-top-20-language-expansion-gate.md`, and
  `docs/decisions/ADR-0030-language-neutral-dependency-library-semantics.md`.
- Plan/specification: `docs/plans/top-20-language-expansion-plan.md`,
  `docs/specifications/indexing-pipeline.md`,
  `docs/specifications/product.md`, `docs/specifications/unknowns.md`, and
  `docs/limitations.md`.
- Implementation: `src/rust/adapters/languages/cpp.rs`,
  `src/rust/adapters/parsing/cpp/{mod.rs,preprocessor.rs,test_framework.rs,project_config.rs}`,
  and `src/rust/adapters/frameworks/cpp.rs`.
- Tests/fixtures: `src/fixtures/cpp/release/v0_2/`,
  `src/fixtures/unknown_reduction/cpp_gtest_resolved/`,
  `src/fixtures/unknown_reduction/cpp_gtest_unresolved/`,
  `src/rust/application/{indexing.rs,family.rs}`, and
  `src/rust/bin/repogrammar.rs`.

## Exact non-claims and remaining risks

- Do not claim C++ completion, Top-20 inclusion, compilation success, full
  language semantics, name/type/overload/template resolution, or authoritative
  build-variant selection.
- Do not claim that `.h` files are correctly classified from project context.
- Do not infer installed packages, selected versions, external symbols, ABI/API
  compatibility, or library contracts from vcpkg/Conan declarations.
- Do not extend current framework families beyond their exact include/macro and
  compatibility profiles or claim runtime test discovery.
- The main correctness risks are configuration-dependent parsing, header/TU
  ownership, macro variants, and template/generated behavior. They remain
  structural uncertainty until an authoritative bounded provider is admitted.

## Completion verdict

**Not complete and not counted.** C++ has the strongest exact-family evidence
of these four lanes, but it does not meet the independent 9/9 ADR-0020 gate.
