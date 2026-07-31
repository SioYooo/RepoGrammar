# Java language completion review

- Language / Top-20 rank: Java / 4
- Review baseline: `8a4033e3eddc93787c997d13c54bba22b1df7102`
- Required Maven prerequisite: `9d3a0ba7afb21f46c716f496a71f34cc7cfc939b`
- Current product label: `bounded_v0_2_preview`
- ADR-0020 completion state: Incomplete — bounded structural preview under
  convergence audit
- Counted as Top-20 complete: **No**
- Last updated: 2026-08-01

The Maven prerequisite adds bounded direct declaration inventory. The Maven
effective model, classpath, and authoritative Java provider are still
incomplete. Therefore neither Maven data nor existing exact framework families
close Java's ADR-0020 completion gate.

## Capability snapshot

| Area | Current evidence | Completion boundary |
|---|---|---|
| Discovery | `.java` selects Java. Exact-root and nested `pom.xml` files are discovered as `java-config`; lookalikes and build/output directories are filtered by current rules. | No selected JDK, Java language level, module graph, source set, generated-source root, Maven profile, or reactor build is authoritative. |
| Dialect / variant | Tree-sitter Java `0.23.5` parses supplied Java bytes. | No `--release`/source level, preview features, compiler flags, module-path/classpath, or annotation-processor configuration is pinned. |
| Frontend / provider | In-process Tree-sitter syntax analysis with exact import/FQN checks. | No javac, JDT, Maven, Gradle, annotation processor, or Java provider slot is admitted or executed. |
| Owned code units / IR | RepoGrammar-owned structural units and IR for classes, methods, tests, routes, entities, repositories, and selected same-class test-data helpers. | No authoritative overload/type resolution, inherited/generated members, external classpath symbols, reflection, runtime wiring, or inter-module binding. |
| Project model | Bounded supplied-byte `pom.xml` reader emits a project-config unit/IR node and direct dependency rows. | The Maven effective model is not implemented: no parent inheritance, properties, dependency management/BOMs, profiles, reactor modules, exclusions, plugins, type/classifier, repositories, or model interpolation. |
| Package metadata | Literal direct `groupId:artifactId` declarations, requirement text, optional flag, and default/compile/runtime/test scope are retained at `manifest_declared`. | No selected or installed version, lockfile, transitive graph, artifact/JAR identity, checksum, classpath, or repository resolution. |
| Third-party analysis | Exact source imports/FQNs support bounded Spring MVC/components/data, JUnit 4/5, TestNG, JPA/Jakarta, and JAX-RS structural families; Maven declarations are auxiliary context only. | No dependency-to-import binding, external symbol ownership, API compatibility, method contract, reviewed library contract, or runtime framework behavior is resolved. |
| Exact families | Product evidence exists for Spring MVC, JUnit 5, JPA, and JAX-RS with support at least three; same-class MethodSource/DataProvider has bounded structural resolution. | Family claims remain exact source-scope previews. Lombok/generated members, DI, inherited endpoints/tests, custom composed annotations, classpath types, and runtime discovery stay unresolved. |
| Typed `UNKNOWN` | Parse degradation, lookalikes/low support, same-class data-provider uncertainty, Lombok/generated members, and unsupported/malformed/bounded Maven inputs abstain through typed facts. | Obligations are incomplete for effective-model conflicts, classpath/module-path selection, overloads, processors/generated sources, stale configs, external symbols, reflection, and provider fallback. |
| Source-free readiness | Maven tests prove supplied-byte parsing, no Maven execution, bounded storage, incremental replacement/removal, and rejection of raw XML/package/version leakage; product family tests cover exact/negative outcomes. | A full Java CLI/MCP/status/doctor/stats/unknowns/family/persistence leakage matrix and authoritative-provider fallback proof are incomplete. |

Under ADR-0030, direct POM rows remain `manifest_declared`. A declared artifact
must not be described as resolved, installed, selected on the classpath, or
semantically linked to source. Reviewed library contracts are a separate,
currently absent evidence type.

## Four-part review record

### Correctness and bug findings

- Exact import/FQN checks and support thresholds reduce false-positive framework
  families; lookalikes and low-support cases abstain.
- Maven inventory is intentionally literal. Because parent inheritance,
  properties, profiles, dependency management, BOM imports, reactor modules,
  exclusions, and plugin/source-set effects are absent, it is not the Maven
  effective model.
- There is no authoritative classpath or module path. Imports and Maven
  coordinates are not joined, external symbols are not resolved, and source
  claims cannot depend on an assumed artifact version.
- Tree-sitter cannot replace javac/JDT for overloads, inherited types,
  annotation processors, generated members, module accessibility, or erroneous
  source recovery. Lombok and similar generation remain explicit uncertainty.

### Security and untrusted-input handling

- Java source and POM analysis is non-executing: RepoGrammar does not run Maven,
  Gradle, javac/JDT, processors, plugins, tests, application code, or network
  resolution.
- POM parsing uses supplied bytes, rejects DTD/entity/CDATA and unsupported
  element-prefix shapes, rejects malformed/duplicate attributes, and applies a
  1 MiB file limit, 2,000-dependency limit, 50,000 XML-token limit, depth 128,
  bounded names/fields/coordinates, and fail-closed UNKNOWN behavior.
- Tests protect source-free Maven outputs against raw XML, raw package/version
  text, and path leakage and cover incremental replacement/removal.
- Tree-sitter remains an in-process parser for untrusted input. Any javac/JDT
  admission needs an OS-sandbox review, filesystem/network/process isolation,
  time/memory/output limits, normalized diagnostics, and typed fallback.

### Implementation completeness

Implemented work includes Java discovery, Tree-sitter owned structural IR,
exact import/FQN framework slices, same-class data-provider handling, release
and unknown-reduction fixtures, bounded Maven direct-declaration inventory,
dependency persistence, and conservative no-execution behavior. Missing work
includes the Maven effective model, selected source-set/project/module model,
classpath, javac/JDT provider, processor/generated-code and runtime-framework
obligations, complete fixture/readiness matrices, and final linked audit.

### Performance and resource bounds

- Java source is capped at 1 MiB. Maven input is also capped at 1 MiB with
  50,000 XML tokens, depth 128, 2,000 dependencies, name length 256, field
  length 1,024, coordinate-part length 255, and version length 256.
- Current analysis is in-process and non-executing, avoiding compiler/build
  process fan-out and remote dependency graph expansion.
- No benchmark closes annotation-dense sources, deeply nested types, large
  reactors, profile explosions, processor output, or classpath-scale symbol
  resolution. A future provider must preregister source/classpath/module,
  diagnostic, time, memory, process, and output ceilings.

## ADR-0020 nine-item completion gate

| Gate | Status | Evidence or exact gap |
|---|---|---|
| G1 Discovery/configuration | Partial | `.java` and bounded POM discovery/inventory exist; effective model, source sets, profiles, reactor/module graph, language level, and classpath selection do not. |
| G2 Authoritative frontend/provider | Open | Tree-sitter only; no admitted javac/JDT provider or authoritative classpath/module path. |
| G3 RepoGrammar-owned units/IR | Satisfied | Java structural code/framework units and IR are RepoGrammar-owned. |
| G4 Typed `UNKNOWN` / fallback | Partial | Parse, generated/Lombok, data-provider, and Maven abstentions exist; effective-model/classpath/provider obligations are incomplete. |
| G5 Exact recurring family | Satisfied | Exact Spring MVC, JUnit 5, JPA, and JAX-RS families have support-at-least-three product evidence. |
| G6 Completion fixture matrix | Partial | Exact, lookalike, low-support, data-provider, and unknown-reduction fixtures exist; the complete version/config/stale/conflict/provider matrix does not. |
| G7 Source-free readiness | Partial | Strong Maven no-execution/leakage and family tests exist; the full Java public-surface and provider-fallback matrix is incomplete. |
| G8 Four-part review | Satisfied by this snapshot | This document records findings and bounds; unresolved findings remain blockers/risks and require tests. |
| G9 Atomic delivery/final audit | Open | Exact substrate SHAs exist, but no completion audit verifies a chain satisfying all nine gates. |

Result: **3 satisfied, 4 partial, 2 open; not 9/9.** Java is not included in
the Top-20 completion count.

## Prerequisite commit evidence

- `707b78c51141f46bbadd10205bc11d824ac608c7` — Java bounded structural preview and exact structural family substrate.
- `9d3a0ba7afb21f46c716f496a71f34cc7cfc939b` — required bounded Maven direct-declaration inventory prerequisite.
- `9ea396b96ae11eb79ec9f80dd52b66e631c7dc1a` and
  `5ef354249b58784b1e872aa6ac3d83c7502dd4c8` — language-neutral dependency
  domain and persistence substrate.

These exact SHAs are ancestors of the reviewed baseline. They are prerequisites,
not a completed Java delivery chain. No non-ancestor or alternative-branch Java
commit is accepted as current-baseline evidence.

## Primary evidence paths

- Authority: `docs/decisions/ADR-0019-bounded-multi-language-structural-expansion.md`,
  `docs/decisions/ADR-0020-top-20-language-expansion-gate.md`, and
  `docs/decisions/ADR-0030-language-neutral-dependency-library-semantics.md`.
- Plan/specification: `docs/plans/top-20-language-expansion-plan.md`,
  `docs/specifications/indexing-pipeline.md`,
  `docs/specifications/product.md`, `docs/specifications/storage.md`,
  `docs/specifications/semantic-workers.md`,
  `docs/specifications/unknowns.md`, and `docs/limitations.md`.
- Implementation: `src/rust/adapters/languages/java.rs`,
  `src/rust/adapters/parsing/java/{mod.rs,junit.rs,jpa.rs,jaxrs.rs,spring.rs,test_data.rs,maven.rs,tests.rs}`,
  and `src/rust/adapters/frameworks/java.rs`.
- Tests/fixtures: `src/fixtures/java/release/v0_2/`,
  `src/fixtures/unknown_reduction/java_*`,
  `src/rust/application/{indexing.rs,family.rs}`, and
  `src/rust/bin/repogrammar.rs`.

## Exact non-claims and remaining risks

- Do not claim Java completion, Top-20 inclusion, javac/JDT semantics,
  compilation success, effective Maven model, authoritative classpath or module
  path, annotation-processor output, or runtime framework behavior.
- Do not describe a POM declaration as a selected/installed version or infer
  external symbols, API contracts, library compatibility, or family membership
  from dependency presence.
- Do not extend exact framework families beyond their import/FQN,
  compatibility, and structural source scopes.
- The highest correctness risks are effective-model divergence, classpath and
  source-set ambiguity, annotation/generated code, overload/inheritance, and
  runtime/reflection wiring. They remain typed uncertainty or non-claims until
  authoritative bounded evidence exists.

## Completion verdict

**Not complete and not counted.** The Maven prerequisite is real and bounded,
but effective-model, classpath, and provider support remain incomplete. Java
does not satisfy the 9/9 ADR-0020 gate.
