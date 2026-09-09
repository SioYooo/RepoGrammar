# Dependency manifest and lockfile matrix

Date: 2026-08-01. `Static` means supplied repository bytes with bounded,
non-executing parsing. `Inventory-only` means the path is discovered but is not
interpreted as dependency evidence.

| Language | Static manifest/config coverage | Lock/resolution coverage | Deliberately deferred or rejected |
|---|---|---|---|
| Python | `pyproject.toml`, `setup.cfg`, literal authoritative `setup.py` dependency fields | none | poetry/pdm/uv/pip-tools locks, installed distribution metadata, `.pyi`, `py.typed`, dynamic setup backends |
| C | shared root `vcpkg.json`, `conanfile.txt`; compile-command candidates | none | C-specific header language, CMake effective model, vcpkg lock/registries, Conan lock/profile, pkg-config/system libs |
| C++ | root `vcpkg.json`, `conanfile.txt`; bounded `compile_commands.json` and CMake config context | none | effective CMake/targets, vcpkg registries/features/platforms, Conan ranges/revisions/lock/profile, headers/ABI |
| Java | direct exact root/nested Maven `pom.xml` dependencies | none | Maven effective model, Gradle, wrapper/toolchains, classpath/module path, JAR metadata, repositories/checksums |
| C# | none | none | `.csproj`, MSBuild evaluation, central package versions, `packages.lock.json`, `project.assets.json`, NuGet/assembly metadata |
| JavaScript | root `package.json` dependency sections and bounded shared TS/JS project config | none | npm/yarn/pnpm locks, workspaces, conditional exports/imports, installed `node_modules`, `.d.ts` package ownership |
| Visual Basic .NET | bounded `.vbproj` direct literal `PackageReference` | none | SDK/import/property/condition evaluation, central versions, NuGet restore/assets/lock, assemblies/analyzers/generators |
| SQL | migration/schema/catalog/generic artifact path inventory only | none | dialect extension manifests, migration tool config/effective order, live catalogs/databases |
| R | `DESCRIPTION`, `NAMESPACE`, explicit repository metadata | exact explicit CRAN/Bioconductor `renv.lock` versions | ambiguous registry, remote/custom/local/URL sources, selected renv project/library, installed package metadata |
| Rust | `Cargo.toml` declarations via bounded Cargo project-model path | none | `Cargo.lock` graph, features/targets/profile selection, registry/git/path authenticity, proc macro/build-script results |
| Delphi/Object Pascal | exact `.dproj` literal `DCC_UsePackage` | none | `.lpi`, `.lpk`, `fpmake`, MSBuild effective properties/conditions, package versions/directness, compiled packages |
| Scratch | no product discovery or dependency records | none | `.sb3` binary port, deflate, asset index, extension opcode/package identity, VM metadata |
| Go | exact root/nested `go.mod` `require` | none (`go.sum` is not a lockfile) | `go.work` graph selection, replaces/excludes/toolchain graph semantics, module cache/checksum authenticity |
| PHP | exact `composer.json` direct runtime/dev declarations | `composer.lock` exact package version rows | content-hash/project-profile coherence, virtual/platform packages, custom repositories, autoload graph |
| Swift | executable `Package.swift` is inventory-only | schema-2/3 `Package.resolved` exact semantic-version pins | manifest execution/static model, version-selected manifest, directness, location/authenticity, plugins/macros/build tools |
| Ada | exact unconditional Alire `[[depends-on]]` string rows; GPR inventory-only | internal `alire.lock` schema deliberately not interpreted | conditional cases, pins, GPR effective project/naming/scenarios, toolchain/target/package resolution |
| Assembly | no dependency manifest; `.s` source scanner only | none | includes/incbin, target triple/object format/build/link metadata, native/system packages/libraries |
| MATLAB | exact R2024b+ `resources/mpackage.json` dependencies | none | MATLAB project files, toolbox archives, installation state, Simulink, path/startup execution |
| Fortran | root literal `fpm.toml` dependency/dev tables | none | target/git/path/namespace shapes, lock/graph, compiler project files, includes/native/system libraries |
| Ruby | executable Gemfile/gemspec are inventory-only; unique `Gemfile.lock` `DEPENDENCIES` is treated as direct manifest declarations | resolved `GEM` spec join not implemented | groups/scope, source-specific entries, git/path/plugin sources, `.ruby-version` project selection, RBS/gem metadata |
| TypeScript extra | same root `package.json` and TS/JS configs as JavaScript | none | npm/yarn/pnpm locks, workspaces/project references, package exports/typesVersions, installed `.d.ts`, package-qualified symbols |

## Evidence semantics

- `manifest_declared` may carry a bounded requirement but no installed or
  selected version claim.
- `lockfile_resolved` carries only the exact version recorded by the admitted
  format. It does not prove authenticity, availability, installation, runtime
  selection, directness, or compatibility unless that field is separately
  proven.
- `provider_resolved` exists in the shared model, but this program does not
  produce a complete package-qualified external-symbol graph for any language.
- Executable build/package DSLs are never run merely to discover dependencies.
  Unsupported dynamic portions remain `UNKNOWN` or inventory-only.
