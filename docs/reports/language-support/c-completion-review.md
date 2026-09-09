# C language completion review

- Language / Top-20 rank: C / 2
- Review baseline: `8a4033e3eddc93787c997d13c54bba22b1df7102`
- Current product label: shared C/C++ `bounded_v0_2_preview`
- ADR-0020 completion state: Incomplete — C structural substrate under
  convergence audit
- Counted as Top-20 complete: **No**
- Last updated: 2026-08-01

The product's combined C/C++ preview label does not close C as an independent
ADR-0020 language lane. C has no C-specific exact family, completion fixture
matrix, authoritative compilation-context frontend, or linked nine-gate audit.

## Capability snapshot

| Area | Current evidence | Completion boundary |
|---|---|---|
| Discovery | `.c` and `.h` select the C language adapter; the generic discovery boundary rejects symlink escape and limits source input to 1 MiB. | `.h` is always classified as C, so a C++ header cannot be selected from compilation context. No dialect, target, standard, or toolchain profile is pinned. |
| Dialect / variant | Tree-sitter C `0.24.2` parses supplied source bytes. Shared preprocessor analysis records bounded structural evidence and uncertainty. | No C89/C99/C11/C17/C23 selection, compiler extensions, target defines, include paths, or build variant is authoritative. |
| Frontend / provider | In-process Tree-sitter syntax analysis only. | There is no Clang, clangd, libclang, compiler, preprocessor, or C-specific provider slot. Tree-sitter is not a semantic oracle. |
| Owned code units / IR | RepoGrammar emits owned module, class-like, and function units plus structural IR through the shared C/C++ scanner. | Units are syntax-derived and are not bound to a selected translation unit or compilation command. |
| Project model | Exact-root `compile_commands.json` can contribute bounded shared C/C++ project-config evidence. | Only entry count and at most 100 safe repository-relative file names are retained. Commands, arguments, working directories, flags, include paths, defines, dialects, target, and selected configuration are not modeled. |
| Package metadata | Exact-root `vcpkg.json` and `conanfile.txt` are recognized by the shared C/C++ configuration path. | They are not C-specific and record only bounded direct manifest declarations. No lockfile or installed graph is resolved. |
| Third-party analysis | vcpkg/Conan rows may provide auxiliary `manifest_declared` evidence. | No external header/symbol ownership, ABI, version selection, transitive graph, package contract, or reviewed library contract is inferred. Package presence never proves a family. |
| Exact family | None for C. | C++ test-framework families cannot be counted as a C family. A dialect/config-scoped C family with exact include/symbol anchors and support at least three is still required. |
| Typed `UNKNOWN` | Shared parsing emits uncertainty for parse degradation, macros, conditional compilation, and preprocessor variants. | The registry is not complete for compilation database selection, header language, dialect, include resolution, target defines, linkage, generated sources, or external symbols. |
| Source-free readiness | Generic source-free storage/query policy and shared C/C++ low-confidence behavior are reusable substrate. | There is no dedicated C completion matrix across CLI, MCP, status, doctor, stats, unknowns, persistence, and leakage rejection. |

Under ADR-0030, current vcpkg/Conan evidence is at most
`manifest_declared`. It must not be promoted to `lockfile_resolved`,
`provider_resolved`, external-symbol resolution, framework membership, or a
reviewed library contract.

## Four-part review record

### Correctness and bug findings

- `.h` deterministically selects C without compilation context. This is safe
  abstention substrate, not correct language selection for repositories that
  use `.h` as C++ headers.
- Structural declarations are useful candidates, but macros, typedefs,
  conditional declarations, include resolution, linkage, compiler extensions,
  and translation-unit variants are not semantically resolved.
- Shared `compile_commands.json` inventory does not preserve the command needed
  to reproduce a C translation unit. It cannot justify dialect- or
  configuration-sensitive claims.
- There is no C exact-family proof or release fixture corpus. C++ fixture and
  family evidence must not be borrowed to close the C lane.

### Security and untrusted-input handling

- The current path is non-executing: it does not invoke a compiler,
  preprocessor, build system, package manager, or project script and does not
  use network access.
- Discovery enforces repository containment and a 1 MiB per-source bound.
  Shared JSON inventory is bounded and duplicate/malformed inputs fail closed;
  unsupported Conan/vcpkg shapes become typed dependency-inventory UNKNOWNs.
- Tree-sitter still parses untrusted input in process. A future Clang-family
  provider needs a separately reviewed sandbox, bounded input/output, time and
  memory limits, descendant-process containment, and normalized diagnostics.
- Dedicated C leakage and source-free product tests remain absent, so the
  generic/shared controls are not enough to close readiness.

### Implementation completeness

Implemented substrate includes discovery, a Tree-sitter syntax scanner, owned
structural IR, shared preprocessor uncertainty, bounded shared C/C++ config
inventory, and conservative no-execution behavior. Missing completion work
includes a C-specific project/dialect profile, authoritative compilation
frontend, provider policy and fallback, complete obligation registry, one exact
C family, the required fixture matrix, source-free/readiness proof, and the
linked atomic completion audit.

### Performance and resource bounds

- Source input is capped at 1 MiB and the shared configuration readers apply
  bounded JSON/XML-like token/member/dependency limits where applicable.
- Tree traversal and preprocessor interval processing are bounded by supplied
  syntax size; there is no compiler or package-manager process fan-out.
- No language-specific benchmark establishes dense-macro, large-header,
  include-graph, or many-variant scaling for C. Future provider work must set
  explicit repository, translation-unit, diagnostic, wall-time, memory, and
  output budgets before admission.

## ADR-0020 nine-item completion gate

| Gate | Status | Evidence or exact gap |
|---|---|---|
| G1 Discovery/configuration | Partial | `.c`/`.h` discovery and bounded shared config inventory exist; header-language selection and a C dialect/build profile do not. |
| G2 Authoritative frontend/provider | Open | Tree-sitter only; no Clang/clangd/libclang compilation-context authority or provider slot. |
| G3 RepoGrammar-owned units/IR | Satisfied | Shared C/C++ scanning emits RepoGrammar-owned structural units and IR. This does not imply semantic completeness. |
| G4 Typed `UNKNOWN` / fallback | Partial | Parse, macro, and conditional uncertainty exists; compilation, header-language, include, linkage, and provider obligations are incomplete. |
| G5 Exact recurring family | Open | No C-specific exact-anchor family with support at least three. |
| G6 Completion fixture matrix | Open | No C-specific positive, lookalike, low-support, parse-degraded, variant, stale/conflict, and unresolved/resolved matrix. |
| G7 Source-free readiness | Partial | Shared conservative behavior exists, but no dedicated C CLI/MCP/inventory/leakage matrix closes the gate. |
| G8 Four-part review | Satisfied by this snapshot | This document records correctness, security, completeness, and performance findings. Open findings remain blockers or risks; the review is not a substitute for tests. |
| G9 Atomic delivery/final audit | Open | No linked C-specific prerequisite chain verifies all nine gates, and this review does not claim completion. |

Result: **2 satisfied, 3 partial, 4 open; not 9/9.** Because all nine gates
are not satisfied, C is not counted even though the combined runtime label uses
`bounded_v0_2_preview`.

## Prerequisite commit evidence

- `0c8b9024f5db5f86b0d64bca5923cd1b900c868a` — shared C/C++ structural preview.
- `0472e6bf0fa74a37a0bb9e43eb878f2c04185c22` — shared C/C++ dependency/config inventory.
- `9f1f657a27d45264d3b5e9b681750778234f0cb2` — bounded JSON hardening used by the shared config path.
- `9ea396b96ae11eb79ec9f80dd52b66e631c7dc1a` and
  `5ef354249b58784b1e872aa6ac3d83c7502dd4c8` — language-neutral dependency
  domain and persistence substrate.

These are substrate prerequisites, not a completed atomic C delivery chain.

## Primary evidence paths

- Authority: `docs/decisions/ADR-0019-bounded-multi-language-structural-expansion.md`,
  `docs/decisions/ADR-0020-top-20-language-expansion-gate.md`, and
  `docs/decisions/ADR-0030-language-neutral-dependency-library-semantics.md`.
- Plan/specification: `docs/plans/top-20-language-expansion-plan.md`,
  `docs/specifications/indexing-pipeline.md`,
  `docs/specifications/domain-model.md`,
  `docs/specifications/unknowns.md`, and `docs/limitations.md`.
- Implementation: `src/rust/adapters/languages/cpp.rs` and
  `src/rust/adapters/parsing/cpp/{mod.rs,preprocessor.rs,project_config.rs}`.
- Shared tests/fixtures: `src/rust/application/indexing.rs`,
  `src/rust/bin/repogrammar.rs`, and `src/fixtures/cpp/release/v0_2/`.

## Exact non-claims and remaining risks

- Do not claim C language completion, Top-20 inclusion, semantic name or type
  resolution, compilation success, dialect accuracy, include resolution,
  package resolution, ABI compatibility, or third-party library understanding.
- Do not describe `.h` classification as compilation-context-aware.
- Do not use shared C++ families or fixtures as C family evidence.
- Do not infer external symbols, APIs, or framework membership from
  vcpkg/Conan declarations.
- The highest correctness risks are header-language misclassification and
  configuration-dependent macro/include behavior; both require typed
  abstention until an authoritative bounded compilation-context path exists.

## Completion verdict

**Not complete and not counted.** C has reusable structural substrate but lacks
the independent semantic, family, fixture, readiness, and atomic-audit evidence
required for a 9/9 ADR-0020 result.
