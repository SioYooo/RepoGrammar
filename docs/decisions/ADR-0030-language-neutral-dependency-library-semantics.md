# ADR-0030: Language-neutral dependency inventory and reviewed library semantics

- Status: Accepted
- Date: 2026-08-01
- Refines: ADR-0015, ADR-0017, ADR-0020

## Context

The frozen Top-20 program needs consistent third-party-library analysis across
languages with incompatible package managers, lockfiles, compilers, and runtime
loading rules. Existing adapters can emit selected project-config facts (for
example Cargo dependencies or `package.json` fields), but there is no shared
package identity, dependency evidence ladder, external-symbol identity, or
reviewed library-contract model.

“Support every third-party library” cannot truthfully mean that RepoGrammar has
hand-authored runtime semantics for every package. Package universes are open,
versions change, dependency graphs can be environment-dependent, and importing
or building repository dependencies can execute untrusted code. A scalable
design must inventory arbitrary packages while refusing to infer behavior that
has not been resolved or reviewed.

## Decision

RepoGrammar adopts one language-neutral four-layer model:

1. **Dependency inventory** records package ecosystem, package name, version
   requirement or resolved version when present, dependency scope, directness,
   optionality, evidence level, and repository evidence. Static manifest and
   lockfile readers must be bounded and non-executing.
2. **Provider resolution** may upgrade a candidate to a package-qualified
   external symbol only through a pinned, isolated, candidate-scoped provider.
   Provider unavailability, conflict, staleness, or insufficient context remains
   typed `UNKNOWN`; it never falls back to guessed identity.
3. **Reviewed library contracts** are explicit, versioned RepoGrammar-owned
   contract packs. A contract names one package and a bounded capability set
   such as symbol identity, framework role, configuration model, call semantics,
   or dataflow effect. Inventory never creates behavior contracts automatically.
4. **Family evidence** may use third-party behavior only when exact source
   anchors and the required resolved symbol/contract evidence agree. A package
   name in a manifest is context, not family support.

The core owns `DependencyEcosystem`, `PackageIdentity`, `DependencyVersion`,
`DependencyScope`, `DependencyEvidenceLevel`, `DependencyRecord`,
`DependencySnapshot`, `ExternalSymbolId`, `LibraryContractId`,
`LibraryCapability`, and `LibraryContract`. External SDK, compiler, package
manager, and wire-protocol types must be translated at adapter boundaries.

The evidence ladder is strict:

`manifest_declared < lockfile_resolved < provider_resolved < reviewed contract`

The final step is deliberately not represented as another dependency evidence
level: a reviewed contract is a separate artifact and still requires compatible
package/version and source evidence at use time.

Unknown ecosystems may not be silently mapped to a nearby ecosystem. A new
stable ecosystem token requires a reviewed model change. Arbitrary package and
symbol text is treated as untrusted, bounded, control-free data and must not be
recorded in telemetry. Public source-free surfaces may expose bounded package
counts and low-cardinality ecosystem/evidence tokens; raw package/symbol text
requires an explicitly reviewed product contract.

The first production consumer is the existing Cargo metadata adapter. It emits
generic `manifest_declared` dependency records alongside its legacy project
facts. `cargo metadata --no-deps` does not provide a resolved transitive graph,
so those records must not be labeled lockfile- or provider-resolved.

## Provider and package-manager policy

- Manifest/lockfile parsers consume supplied bounded bytes and do not execute
  repository files, package-manager scripts, plugins, builds, test runners, or
  dependency code.
- Language-native semantic providers are optional accelerators. Each language
  requires its own exact tool/version, isolation, resource, provenance, cache,
  and failure contract before integration.
- Package managers and build tools are never used merely to discover the graph
  if doing so can execute project code. A safe metadata-only mode must be proven
  per tool, as with the bounded Cargo `--no-deps` path.
- Lockfiles prove only the fields their pinned format actually contains. They do
  not prove that an environment installed the package or that a runtime selected
  it.
- Packages without a reviewed contract remain inventory-visible. Their behavior
  is `UNKNOWN`, not unsupported package identity and not guessed semantics.

## Top-20 application

The initial ecosystem mapping is an implementation target, not completion
evidence: PyPI, npm, Maven, NuGet, Cargo, Go modules, Composer, RubyGems, Swift
Package Manager, CRAN/Bioconductor, Delphi package metadata, Alire, fpm, MATLAB
add-ons, SQL extensions, Scratch extensions, and native/system dependencies.
C/C++ project manifests such as vcpkg and Conan map to package identities only
after their exact bounded schemas are qualified.

Languages without a conventional package manager still use the same model:
Assembly can inventory explicitly declared native/system dependencies, SQL can
inventory dialect extension declarations, and Scratch can inventory extension
opcodes from a bounded `.sb3` project. Absence of a package manager is not
permission to infer runtime behavior.

## Consequences

- Third-party coverage can scale to arbitrary package names without an
  impossible global behavior database.
- Exact package/version/symbol semantics remain auditable and conservative.
- Existing string-only dependency facts can migrate incrementally; they are not
  retroactively promoted to resolved identities.
- Persistence, source-free product projections, generic provider ports, and
  per-ecosystem manifest adapters remain follow-up modules. This ADR and the
  first Cargo consumer do not complete any ADR-0020 language gate.
- Library contracts require explicit review, versioning, fixtures, provenance,
  and invalidation tests. They must not become hard-coded benchmark answers.

## Alternatives considered

- **Treat every import spelling as an external symbol.** Rejected because local
  shadowing, aliases, classpaths, workspaces, and dynamic loaders make spellings
  non-authoritative.
- **Run each package manager/build tool to obtain a complete graph.** Rejected
  because those tools may execute untrusted scripts, plugins, builds, macros, or
  repository code and may use ambient state or network access.
- **Hand-code semantics for a popular-library allowlist only.** Rejected as the
  only layer because it leaves arbitrary packages invisible. Reviewed contracts
  are retained as an optional behavior layer above generic inventory/resolution.
- **Use one universal language server.** Rejected because no provider covers the
  frozen language set or shares one trustworthy sandbox, version, and semantic
  contract.

## Follow-up work

1. Add a language-neutral dependency-model provider port and persistence
   round-trip with freshness and conflict behavior.
2. Migrate bounded existing manifest readers to generic records, beginning with
   Cargo and then npm/Python/C++ project metadata.
3. Add package-qualified external-symbol queries to the TypeScript, Python, and
   Rust provider lanes before adding new framework contracts.
4. Specify a reviewed library-contract registry, cache invalidation, and
   source-free product projection.
5. Apply each language's ADR-0020 gate independently; shared infrastructure is
   reusable evidence, never a completion waiver.
