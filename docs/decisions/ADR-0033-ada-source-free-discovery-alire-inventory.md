# ADR-0033: Ada source-free discovery and Alire inventory

- Status: Accepted
- Date: 2026-08-01
- Scope: Ada discovery/configuration and dependency inventory only
- Refines: ADR-0020 and ADR-0030
- Related: `docs/reports/language-support/ada-completion-review.md`

## Context

Ada project naming, preprocessing, configuration, and library resolution cannot
be inferred from a broad filename list. GNAT documents `.ads` as the default
specification suffix and `.adb` as the default body suffix, while GPR project
configuration can define alternative source naming. Treating `.ada` or a
case-insensitive suffix as Ada without selecting and evaluating that project
model would create false inventory and later false semantic candidates.

Alire's catalog format documents exact root `alire.toml` manifests and
`[[depends-on]]` dependency tables whose direct entries map a crate name to a
version constraint. Conditional `depends-on.'case(...)'` tables and `[[pins]]`
carry selection or override semantics that a file-local static reader cannot
resolve. Current Alire documentation explicitly describes `alire/alire.lock` as
internal state, regenerated from scratch and not intended for user inspection.
RepoGrammar therefore cannot freeze an undocumented lock schema or label its
contents resolved dependency evidence.

Primary sources retrieved 2026-08-01:

- GNAT compilation model:
  <https://docs.adacore.com/gnat_ugn-docs/html/gnat_ugn/gnat_ugn/the_gnat_compilation_model.html>
- GPR project manager:
  <https://docs.adacore.com/gprbuild-docs/html/gprbuild_ug/gnat_project_manager.html>
- Alire catalog format:
  <https://alire.ada.dev/docs/catalog-format-spec>
- Alire lockfile guidance: <https://alire.ada.dev/docs/>
- Libadalang introduction and core concepts:
  <https://docs.adacore.com/live/wave/libadalang/html/libadalang_ug/introduction.html>
  and
  <https://docs.adacore.com/live/wave/libadalang/html/libadalang_ug/core_concepts.html>

## Decision

### D1. Ada remains `discovered_only`

The stable tokens are `ada` and `ada-config`. Only normalized repo-relative
lowercase `.ads` and `.adb` paths become Ada source inventory. Exact lowercase
`.gpr`, `alire.toml`, and `alire.lock` basenames become configuration inventory.
`.ada`, case variants, suffix lookalikes, absolute/traversing paths, and other
project-selected names remain unrecognized. This is a conservative default-name
boundary, not proof that GNAT or a selected GPR project would accept the file.

Ada source and GPR are inventory-only before SourceStore access. They may persist
only repo-relative path, strict raw-byte hash, size, and language token. Binary
bytes remain admissible metadata. They create no code unit, IR, semantic fact,
dependency, framework role, family, readiness, or support record.

### D2. The Alire subset is bounded, static, and declaration-only

Exact `alire.toml` and `alire.lock` may be read through the normal hash-checked
SourceStore boundary. The parser caps a document at 1 MiB, 50,000 lines, 4,096
bytes per line, 2,000 dependencies, 128 bytes per crate name, and 256 bytes per
constraint. Tests cover every inclusive byte boundary and limit-plus-one.

Only ASCII, unescaped, nonempty string assignments in an exact unconditional
`[[depends-on]]` section are admitted. Crate names use a bounded lowercase Ada-
identifier-compatible subset. The original bounded constraint is retained as a
requirement without resolving or interpreting its Boolean/version meaning. Each
row is `ecosystem=alire`, `scope=runtime`, `directness=direct`,
`optional=false`, `evidence_level=manifest_declared`, and has no resolved
version.

Conditional dependency tables are omitted as build-variant ambiguity. Pins are
not dependencies and remain unresolved overrides. Conflicting names omit the
identity; malformed or resource-bounded input fails the dependency inventory
closed. `alire.lock` always emits an internal-schema UNKNOWN and no dependency
row. UNKNOWN notes and assumptions are fixed source-free tokens scoped only to
`ada_dependency_inventory`; crate constraints, paths, URLs, credentials, and
source text do not enter them.

Nothing invokes GNAT, gcc, gprbuild, alr, project tools, repository/dependency
code, tests, plugins, child processes, or the network. No new production
dependency is admitted.

### D3. Libadalang is `NO_GO` for this production slice

Libadalang is a capable syntax/semantic candidate with error recovery, but its
documented model is not a full Ada legality checker, is coupled to a compatible
GNAT toolchain, and its project provider can read GPR/source files to construct
project context. Those properties are useful research inputs but do not prove a
source-free, supplied-bytes-only, cross-platform production boundary.

The Round-4 qualification verdict is `NO_GO` for production admission and
default dispatch. RepoGrammar adds no Libadalang dependency, artifact, process,
or fallback. A future proposal may reconsider a pinned isolated worker only
after artifact/provenance, dependency closure, five-target build, sandbox,
project-model, differential, resource, and typed-obligation gates are specified
and independently reviewed.

### D4. Incremental behavior is evidence-bound

Alire config parsing is file-local and absent from `ParserProjectContext`.
Unchanged config units/facts/dependencies copy forward exactly once; changed
files replace their rows; removed files retain none. Ada source/GPR deltas remain
metadata-only and filter any seeded claim-bearing legacy records. A generation
with only source/GPR inventory is `file_manifest_only`; one with a parsed Alire
document is `syntax_only_code_units`.

## Consequences

- Arbitrary direct Alire crate identities become visible to the internal shared
  dependency model without claiming installation, resolution, library behavior,
  source semantics, or family support.
- Alternative Ada naming, GPR selection, conditional Alire dependencies, pins,
  lock resolution, preprocessing, generics, overload resolution, dispatch,
  contracts, and runtime behavior remain unavailable or typed UNKNOWN.
- Ada is not a supported language. Completion still requires the full ADR-0020
  frontend, IR, obligation, family, fixture, readiness, review, and audit gates.

## Alternatives rejected

- **Accept `.ada` and case variants.** Rejected because GPR naming is project-
  selected and default GNAT suffixes are precise.
- **Run alr/GNAT/gprbuild to discover dependencies or sources.** Rejected because
  package/build/project tools can resolve remote state or execute untrusted code.
- **Reverse-engineer `alire.lock`.** Rejected because official documentation
  identifies it as internal state without a stable consumer contract.
- **Admit Libadalang now.** Rejected because qualification and isolation evidence
  is incomplete and the dependency is unnecessary for this source-free slice.
