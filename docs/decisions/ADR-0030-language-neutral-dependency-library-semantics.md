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
`DependencyScope`, `DependencyDirectness`, `DependencyEvidenceLevel`, `DependencyRecord`,
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

The first provider consumer is the existing Cargo metadata adapter. It emits
generic `manifest_declared` dependency records alongside its legacy project
facts. `cargo metadata --no-deps` does not provide a resolved transitive graph,
so those records must not be labeled lockfile- or provider-resolved. The first
static-manifest consumers are the bounded root `package.json` parser and the
Python project-config frontend. The npm path records valid names from
`dependencies`, `devDependencies`,
`optionalDependencies`, and `peerDependencies`; peer scope remains `unknown`
because the shared scope vocabulary does not invent a runtime/build meaning for
that npm-specific relation. The Python path records normalized PyPI identities
from PEP 621/build-system/dependency-group metadata when `tomllib` is available,
from `setup.cfg` requirement sections, and from literal dependency fields on a
lexically authoritative static `setup.py` call. Dynamic dependency expressions
remain typed `UNKNOWN`, and URL/path requirement suffixes are omitted so local
paths or credentials do not enter the record. The accepted suffix is ASCII and
byte bounded; the revisioned private response is capped at 2 MiB and becomes a
typed resource-limit outcome instead of crossing the host boundary.
Parser-origin records can never claim `provider_resolved`.

The first C/C++ static-manifest consumers are the existing root `vcpkg.json`
and `conanfile.txt` readers. They use distinct `vcpkg` and `conan` ecosystem
tokens so equal package spellings from different registries never collapse into
one identity. vcpkg inventory accepts only bounded names under the official
lowercase/digit/hyphen grammar, either as string entries or objects; a bounded
official `version>=` string is retained as a minimum requirement. It does not
interpret features, host/build selection, registries, baselines, top-level
overrides, or platform expressions. Conan inventory accepts only direct exact
Conan 2 lowercase `name/version` entries in `[requires]`.
Ranges, recipe revisions, user/channel references, malformed references,
duplicate package names with conflicting requirements, uninterpreted vcpkg
object fields, malformed Conan section syntax, and record-limit overflow become
`cpp_dependency_inventory` typed `UNKNOWN`. Valid names remain inventoried when
only wider semantics are unknown. Both paths use `scope=unknown`, execute no
package-manager or project code, and preserve their legacy `PROJECT_CONFIG`
facts as non-family context.

The C/C++ project-config provenance method is
`bounded_cpp_project_inventory_v2`. Before vcpkg decoding, a non-executing JSON
member scanner rejects duplicate object keys, nesting beyond 128 levels, more
than 8,192 object members, or decoded keys above 256 bytes. This prevents JSON
map overwrite from turning ambiguous manifests into apparently complete
inventory. The admission scanner is now a shared project-metadata helper used by
every qualified JSON manifest lane; the normal `serde_json` parse remains the
syntax authority after that gate.

The qualified format snapshot is fixed for auditability:

- Microsoft Learn `vcpkg.json` Reference, retrieved 2026-08-01. The accepted
  subset uses its lowercase package-name grammar and dependency `name` plus
  bounded `version>=` fields. Other dependency-object fields remain inventory
  context with `UNKNOWN` semantics.
- Conan 2.31.1 package-reference grammar and `conanfile.txt` `[requires]`,
  retrieved 2026-08-01. Only lowercase 2–101 byte `name/version` references are
  admitted. Version ranges, revisions, user/channel references, and other
  requirement sections remain outside this subset.

References: <https://learn.microsoft.com/en-us/vcpkg/reference/vcpkg-json>,
<https://docs.conan.io/2/reference/conanfile/attributes.html>, and
<https://docs.conan.io/2/reference/conanfile_txt.html>.

Dependency directness is a closed three-state contract: `direct`, `transitive`,
or `unknown`. A lockfile that enumerates pins without proving which ones were
declared by the root manifest must use `unknown`; it must not encode unknown as
boolean false and thereby mislabel a pin as transitive.

The first consumer of that third state is SwiftPM `Package.resolved` schema 2
and 3. A bounded unique-member JSON reader accepts only lowercase source-free
package identities and exact semantic versions, records them as
`lockfile_resolved`, and leaves scope and directness `unknown`. Malformed pins,
branch or revision-only state, conflicting identities, unsupported schema, and
resource overflow emit `swift_dependency_inventory` typed `UNKNOWN`. The reader
does not retain locations, URLs, revisions, or origin hashes, and it does not
prove install state, authenticity, buildability, runtime selection, or direct
root declaration. `Package.swift` and version-specific manifests remain
executable Swift: indexing never evaluates them.

The qualified behavior is based on Swift Package Manager documentation,
retrieved 2026-08-01, which describes `Package.resolved` as the recorded result
of dependency resolution and documents that SwiftPM commands can resolve or
update dependencies. RepoGrammar consumes only supplied lock bytes and invokes
none of those commands. References:
<https://docs.swift.org/swiftpm/documentation/packagemanagerdocs/resolvingdependencyfailures/>
and <https://docs.swift.org/package-manager/PackageDescription/PackageDescription.html>.

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
C/C++ vcpkg names and bounded Conan exact references now map to distinct package
identities under the qualified subset above. Wider vcpkg and Conan schemas
remain unqualified.

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
- Schema v13 persistence and an internal active-generation read model now
  preserve generic dependency records with source evidence. Cargo records are
  recomputed by their provider on incremental sync; unchanged static-manifest
  records are copied only with their unchanged evidence unit. Source-free
  public projections, generic provider ports, and remaining per-ecosystem
  manifest adapters remain follow-up modules. This ADR, persistence slice, and
  the Cargo/npm/Python/vcpkg/Conan/SwiftPM consumers do not complete any
  ADR-0020 language gate.
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

1. Extend the schema v13 persistence round-trip with explicit cross-provider
   conflict reporting when multiple qualified providers disagree; path
   freshness and generation replacement are already fail-closed through
   derived-record dependencies.
2. Continue migrating bounded existing manifest readers to generic records.
   Cargo, root npm `package.json`, bounded standard Python project formats, and
   the qualified vcpkg/Conan subsets are complete at manifest-declaration level;
   SwiftPM schema-2/3 pins are complete at bounded lockfile level. Wider C/C++,
   Swift manifest declarations, and additional Python tool-specific schemas
   remain open.
3. Add package-qualified external-symbol queries to the TypeScript, Python, and
   Rust provider lanes before adding new framework contracts.
4. Specify a reviewed library-contract registry, cache invalidation, and
   source-free product projection.
5. Apply each language's ADR-0020 gate independently; shared infrastructure is
   reusable evidence, never a completion waiver.
