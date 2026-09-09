# Third-party ecosystem coverage matrix

Date: 2026-08-01. Branch: `feat/top-20-language-library-support`.

This matrix audits the closed `DependencyEcosystem` vocabulary against actual
consumers. “Inventory” means bounded repository evidence only; it never means
installed, selected, importable, compatible, or behaviorally understood.

| Ecosystem token | Language lanes | Current reader/evidence | Current level | Principal gap |
|---|---|---|---|---|
| `pypi` | Python | bounded `pyproject.toml`, `setup.cfg`, static literal `setup.py` declarations | `manifest_declared` | common lockfiles, distribution metadata, `.pyi`/`py.typed`, import-to-distribution binding |
| `npm` | JavaScript, TypeScript | bounded root `package.json` dependency sections | `manifest_declared` | npm/yarn/pnpm locks, workspaces, exports/imports, installed graph, package-qualified symbols |
| `maven` | Java | bounded exact root/nested `pom.xml` direct dependencies | `manifest_declared` | effective model, parent/properties/BOM/profiles/reactor, Gradle, classpath/JAR symbols |
| `nuget` | Visual Basic .NET; C# missing | bounded `.vbproj` literal `PackageReference` | `manifest_declared` | C# `.csproj`, central versions, restore/assets graph, target frameworks, assemblies |
| `cargo` | Rust | bounded Cargo project records from the metadata lane | `manifest_declared` | `Cargo.lock`, full feature/target graph, external item identity, proc-macro/build semantics |
| `go_modules` | Go | bounded `go.mod` `require`; direct/`// indirect` retained | `manifest_declared` | `go.work` selection, module graph, `go.sum` authenticity, package/type identity |
| `composer` | PHP | bounded `composer.json` declarations and `composer.lock` package pins | manifest plus `lockfile_resolved` | coherence/effective profile, virtual/platform packages, autoload and PHP symbol binding |
| `rubygems` | Ruby | unique top-level `Gemfile.lock` `DEPENDENCIES` rows | `manifest_declared` | resolved `GEM` specs join, groups, executable Gemfile/gemspec, require/constant identity |
| `swift_package_manager` | Swift | bounded schema-2/3 `Package.resolved` pins | `lockfile_resolved` | executable manifest selection, directness, authenticity, module/SDK/XCTest identity |
| `cran` | R | explicit CRAN DESCRIPTION/NAMESPACE/renv evidence | manifest or `lockfile_resolved` | selected repository/project/profile, source package/native identity, namespace binding |
| `bioconductor` | R | explicit Bioconductor renv evidence | `lockfile_resolved` where exact | repository release compatibility, project selection, namespace/native identity |
| `delphi_package` | Delphi | bounded `.dproj` `DCC_UsePackage` names | `manifest_declared` | directness, version suffixes, auto-added packages, FPC/Lazarus models, unit symbols |
| `alire` | Ada | unconditional literal `[[depends-on]]` rows | `manifest_declared` | conditional tables, pins, internal lock schema, GPR/toolchain selection, Ada symbols |
| `fpm` | Fortran | root literal `fpm.toml` dependency tables | `manifest_declared` | target/git/path shapes, graph/lock, compiler/source form, modules/ABI |
| `matlab_add_on` | MATLAB | exact R2024b+ `resources/mpackage.json` dependency entries | `manifest_declared` | installed toolbox graph, release/project selection, MATLAB symbols and licensed provider |
| `sql_extension` | SQL | no production consumer | none | dialect, extension manifest/source, catalog and version identity |
| `scratch_extension` | Scratch | no product consumer; disconnected archive prerequisite only | none | binary port, deflate, extension opcode identity/version/provenance |
| `vcpkg` | C, C++ | bounded root `vcpkg.json` names and `version>=` | `manifest_declared` | registries/baselines/features/platform expressions, selected triplet, headers/symbols |
| `conan` | C, C++ | bounded exact Conan 2 `[requires]` references | `manifest_declared` | ranges/revisions/user-channel/lock/profile, installed packages, headers/ABI |
| `native_system` | C/C++/Assembly/Fortran and others | no production consumer | none | target-specific system packages, ABI, linker inputs, include/library search paths |

## Platform layers

| Layer | Result | Evidence boundary |
|---|---|---|
| Package identity | Implemented | ecosystem plus bounded canonicalized package text; unknown ecosystems are rejected |
| Dependency snapshots | Implemented | deterministic records, typed directness, evidence level, source provenance, schema-v14 persistence |
| Generic no-contract packages | Implemented at inventory level | arbitrary admitted names remain visible internally; behavior stays `UNKNOWN` |
| External symbols | Model only / incomplete consumers | `ExternalSymbolId` exists, but no cross-language complete package-qualified resolver |
| Reviewed contracts | Registry implemented, packs absent | exact-version sets, revision, capabilities, duplicate/overlap rejection; production pack count 0 |
| Behavior/family use | Not platform-complete | still requires exact source anchor, qualified symbol, compatible resolved version, contract, freshness, and no blocking UNKNOWN |

Verdict: broad bounded package inventory exists, but
`LIBRARY_ANALYSIS_PLATFORM_COMPLETE=false`. The largest common gap is the safe
join from project/lock evidence to package-qualified external symbols, followed
by reviewed contract packs with provenance and source-free product projection.
