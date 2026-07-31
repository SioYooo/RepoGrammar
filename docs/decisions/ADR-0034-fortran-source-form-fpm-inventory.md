# ADR-0034: Fortran source-form discovery and fpm inventory

- Status: Accepted
- Date: 2026-08-01
- Scope: Fortran discovery/configuration and dependency inventory only
- Refines: ADR-0020 and ADR-0030
- Related: `docs/reports/language-support/fortran-completion-review.md`

## Context

Fortran filename suffixes can select fixed/free source form and preprocessing.
GNU Fortran documents lowercase `.f`, `.for`, and `.ftn` as fixed form without
preprocessing, and lowercase `.f90`, `.f95`, `.f03`, and `.f08` as free form
without preprocessing. Uppercase equivalents and `.fpp` trigger preprocessing.
RepoGrammar cannot safely run a preprocessor or guess compiler flags, include
paths, macros, source form, or dialect from a broad suffix list.

The Fortran Package Manager specifies exact `fpm.toml`, root `[dependencies]`
and `[dev-dependencies]`, registry namespace/version shapes, git/path sources,
and target-specific dependency tables. Only a small direct string subset has a
package identity and scope that can be proven without selecting targets,
resolving registries, reading local dependency paths, or running fpm.

Primary sources retrieved 2026-08-01:

- GNU Fortran file suffixes and preprocessing:
  <https://gcc.gnu.org/onlinedocs/gcc-11.1.0/gfortran/GNU-Fortran-and-GCC.html>
  and <https://gcc.gnu.org/onlinedocs/gfortran/Preprocessing-Options.html>
- GNU Fortran source-form options:
  <https://gcc.gnu.org/onlinedocs/gfortran/Fortran-Dialect-Options.html>
- fpm manifest specification:
  <https://fpm.fortran-lang.org/spec/manifest.html>
- Flang parser architecture and driver:
  <https://flang.llvm.org/docs/Parsing.html>,
  <https://flang.llvm.org/docs/Overview.html>, and
  <https://flang.llvm.org/docs/FlangDriver.html>

## Decision

### D1. Fortran remains `discovered_only`

The stable tokens are `fortran` and `fortran-config`. The exact accepted source
suffix set is:

- fixed form: `.f`, `.for`, `.ftn`;
- free form: `.f90`, `.f95`, `.f03`, `.f08`.

Only lowercase normalized repo-relative paths are admitted. Uppercase variants,
`.fpp`, `.fi`, `.fii`, suffix lookalikes, and absolute/traversing paths remain
unrecognized. This is a frozen source-free inventory boundary, not a language-
dialect, source-form-flag, preprocessing, include, buildability, or compiler
acceptance claim.

Fortran source bypasses SourceStore and parser dispatch and may persist only
repo-relative path, strict raw-byte hash, size, and token. Exact root/nested
`fpm.toml` becomes `fortran-config` and may enter the bounded static parser.

### D2. The fpm subset is bounded and declaration-only

The parser caps a manifest at 1 MiB, 50,000 lines, 4,096 bytes per line, 2,000
dependencies, 128 bytes per package name, and 256 bytes per constraint. Tests
cover exact and limit-plus-one bytes plus record and text failure behavior.

Only ASCII, unescaped, nonempty string assignments in exact root
`[dependencies]` and `[dev-dependencies]` tables are admitted. Bounded lowercase
package names may use digits, underscore, and hyphen after the first letter.
The original bounded constraint is retained without resolution. Runtime table
rows use `scope=runtime`; development rows use `scope=development`. Both use
`ecosystem=fpm`, `directness=direct`, `optional=false`,
`evidence_level=manifest_declared`, and no resolved version.

Dotted registry namespace/version keys, inline git/path tables, target-specific
dependency tables, non-string values, conflicts, non-ASCII/control text, and
resource overflow are omitted or fail closed with fixed source-free UNKNOWNs
scoped only to `fortran_dependency_inventory`. Local paths, URLs, credentials,
package constraints, and source text do not enter UNKNOWN notes.

Nothing invokes fpm, gfortran, flang, preprocessors, linkers, project tools,
repository/dependency code, tests, plugins, child processes, or the network. No
new production dependency is admitted.

### D3. Flang is `NO_GO` for this production slice

Flang's documented parser is reusable and reentrant, but its prescanner expands
`INCLUDE` and performs preprocessing before parsing; its driver exposes broader
compiler phases. That is not the current supplied-bytes-only, no-preprocessor,
no-project-execution boundary, and no pinned artifact/dependency/sandbox/
differential matrix was qualified in this round.

The Round-4 qualification verdict is `NO_GO` for production admission and
default dispatch. RepoGrammar adds no Flang/f18 dependency, binary, process, or
fallback. A future isolated frontend proposal must first freeze source-form and
dialect selection, include/preprocessor policy, artifact provenance, dependency
closure, five-target builds, sandboxing, resource limits, differential oracles,
and typed obligations.

### D4. Incremental behavior is evidence-bound

fpm parsing is file-local and absent from `ParserProjectContext`. Unchanged
config units/facts/dependencies copy forward exactly once; changed manifests
replace rows; removed manifests retain none. Fortran source deltas remain zero-
read metadata and purge seeded claim-bearing legacy records. Source-only
generations are `file_manifest_only`; a generation containing parsed `fpm.toml`
is `syntax_only_code_units`.

## Consequences

- Arbitrary simple direct fpm package identities become available to the shared
  internal dependency inventory without claiming resolution, installation,
  behavior, source semantics, or family support.
- Preprocessing, includes, macros, source-form flags, compiler dialects, modules,
  submodules, generics, interfaces, overloads, coarrays, linking, target
  selection, namespace resolution, and runtime behavior remain unavailable or
  typed UNKNOWN.
- Fortran is not a supported language. Completion still requires every ADR-0020
  frontend, IR, obligation, family, fixture, readiness, review, and audit gate.

## Alternatives rejected

- **Accept uppercase and `.fpp` inputs.** Rejected because their documented
  default includes preprocessing.
- **Infer free/fixed form from content or ambient compiler settings.** Rejected
  because that reads deferred source or selects an unrecorded environment.
- **Run fpm/compiler metadata commands.** Rejected because resolution, local
  paths, preprocessors, build scripts, and project/dependency execution are not
  qualified.
- **Admit Flang now.** Rejected because its prescanner boundary and artifact/
  isolation evidence do not satisfy this slice.
