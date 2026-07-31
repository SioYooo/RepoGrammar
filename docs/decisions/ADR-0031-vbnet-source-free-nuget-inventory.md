# ADR-0031: VB.NET source-free discovery and NuGet inventory

- Status: Accepted
- Date: 2026-08-01
- Scope: Visual Basic in ADR-0020 wave N2; discovery/configuration only
- Refines: ADR-0020 and ADR-0030
- Related: `docs/specifications/indexing-pipeline.md`,
  `docs/specifications/unknowns.md`, and
  `docs/reports/language-support/visual-basic-completion-review.md`

## Context

The Top-20 plan requires a dialect decision before the Visual Basic token can
be stable. Microsoft documents `.vbproj` as the Visual Basic .NET MSBuild
project extension and documents `PackageReference` as a direct/top-level NuGet
declaration inside an MSBuild project. Those facts support a bounded VB.NET
inventory slice. They do not support classic Visual Basic/VB6, whose `.vbp`,
`.frm`, `.bas`, and `.cls` formats and runtime model are different.

MSBuild project files are XML but are not passive manifests in the general
case. SDK references imply imports, explicit `.props`/`.targets` imports can add
items, conditions select items, properties compute identities and versions,
and `Update`/`Remove` can alter an item set. Evaluating a target project would
cross the no-execution boundary. NuGet restore similarly computes the full
dependency closure and may access package sources. Neither operation is needed
or permitted for manifest inventory.

Primary format evidence reviewed for this decision:

- Microsoft Learn, [Understanding the project file](https://learn.microsoft.com/en-us/aspnet/web-forms/overview/deployment/web-deployment-in-the-enterprise/understanding-the-project-file), identifies `.vbproj` as a Visual Basic .NET MSBuild project file;
- Microsoft Learn, [Use the MSBuild XML schema to control builds](https://learn.microsoft.com/en-us/visualstudio/msbuild/msbuild?view=vs-2022), documents XML projects, implicit SDK imports, properties, items, and conditions;
- Microsoft Learn, [Item element](https://learn.microsoft.com/en-us/visualstudio/msbuild/item-element-msbuild?view=visualstudio), documents direct `ItemGroup` children and `Include`, `Update`, `Remove`, `Condition`, and child/attribute metadata; and
- Microsoft Learn, [PackageReference in project files](https://learn.microsoft.com/en-us/nuget/consume-packages/package-references-in-project-files), defines `PackageReference` declarations, version metadata, conditions, central/implicit behavior, and the distinction between top-level declarations and the restore-time transitive closure.

## Decision

### D1. The lane is VB.NET only

The stable discovery tokens are `visual-basic` for exact lowercase `.vb`
source and `visual-basic-config` for exact lowercase `.vbproj` configuration.
This decision explicitly excludes VB6. `.vbp`, `.frm`, `.bas`, and `.cls` are
unsupported extensions, not aliases for either token. A `.vb` path proves only
bounded inventory presence; it does not prove a compiler version, target
framework, project membership, buildability, semantic support, or family.

Exact `bin` and `.vs` components are Visual-Basic-only exclusions. `obj`
remains covered by the existing global MSBuild generated-output exclusion.
The language-specific exclusions must not hide unrelated-language files.

### D2. Source is never read or parsed

`visual-basic` files persist only normalized repository-relative path, raw-byte
SHA-256, byte size, and token. Full and incremental indexing route them around
the source store and parser. Add/modify/remove operations remain file-local and
metadata-only. No Roslyn, `vbc`, `dotnet`, MSBuild, NuGet, project target,
analyzer, source generator, build task, package script, repository executable,
child process, or network operation is invoked.

### D3. `.vbproj` admits one static subset

An exact `.vbproj` is read only after generic discovery limits and source-store
hash validation. RepoGrammar's own bounded, non-validating XML reader rejects
DTD declarations, external/custom entities, processing instructions other
than the XML declaration, malformed nesting, duplicate attributes, and
depth/node/attribute/name/value overflows. It does not resolve schemas,
XInclude, imports, SDKs, properties, conditions, tasks, or targets.

The parser creates one `visual-basic-config` `project_config` unit and accepts a
NuGet dependency row only for an exact `PackageReference` that:

1. is a direct child of an `ItemGroup` that is a direct child of the exact
   `Project` root;
2. has one bounded literal `Include` package identity;
3. is not an `Update` or `Remove` item; and
4. has zero or one mutually consistent literal `Version` attribute/child and
   no effective `VersionOverride` in the accepted subset.

The resulting ADR-0030 row uses ecosystem `nuget`, evidence
`manifest_declared`, directness `direct`, unknown scope, no resolved version,
and the literal version as a requirement when available. PrivateAssets and
asset-flow metadata do not change that inventory identity. Package identities
are deduplicated case-insensitively. Conflicting requirements omit that
identity rather than choosing one.

An SDK reference, `Import`, condition, property/item expression, `Update`,
`Remove`, `VersionOverride`, invalid entry, conflict, malformed XML, or resource
overflow produces source-free `visual_basic_dependency_inventory` uncertainty.
Safe independent rows survive a partial/dynamic document. The parser never
claims that the static rows are restored, installed, authenticated, selected,
complete after MSBuild evaluation, transitively closed, or behaviorally
understood.

### D4. Evidence and status stay conservative

The evidence ladder is:

1. primary inventory evidence: exact repo-relative path, raw-byte hash, size,
   and stable language/config token;
2. primary static dependency evidence: the admitted literal
   `PackageReference` field in supplied `.vbproj` bytes;
3. diagnostic evidence: scoped typed UNKNOWN for omitted/dynamic metadata;
4. forbidden evidence: extension-only support claims, MSBuild/NuGet output,
   ambient SDK/toolchain state, package presence as framework behavior, or any
   VB6-to-VB.NET equivalence.

Visual Basic remains `discovered_only` and unsupported. There is no VB source
frontend, IR, framework role, semantic provider, family, readiness promotion,
or support-state change. A repository with only `.vb` files is
`file_manifest_only`; an admitted `.vbproj` owns a project-config unit and is
`syntax_only_code_units`. Neither mode means language support.

### D5. Limits, tests, and follow-up

The XML reader permits at most depth 64, 16,384 elements, 64 attributes per
element, 256 bytes per name, and 65,536 decoded bytes per value. The static
dependency inventory permits 2,000 unique identities. Exact limits succeed;
the first plus-one emits a resource UNKNOWN or fails the enclosing bounded XML
parse without partial over-limit rows. Shared discovery limits, symlink
refusal, and ADR-0023's still-open concurrent replacement limitation remain
unchanged.

Required tests cover exact token/case/VB6 separation, normalized paths,
language-specific exclusions, binary source, source-store/parser bypass,
DTD/entity refusal, direct/condition/property/update/conflict cases, exact and
plus-one resource behavior, persistence/copy-forward/replacement/removal,
source-free output, CLI indexing mode, and zero families.

Future source analysis requires a separately reviewed Roslyn/Visual Basic
frontend and project-profile decision. `packages.config`,
`Directory.Packages.props`, `packages.lock.json`, imports, generated items,
analyzers, and source generators remain outside this slice.

## Consequences

- VB.NET repositories gain deterministic source-free inventory and bounded
  direct NuGet declaration metadata without a new production dependency.
- VB6 remains explicitly unsupported instead of being silently relabelled.
- Dynamic MSBuild behavior is visible as typed uncertainty, so the inventory
  cannot be overclaimed as a resolved package graph.
- The new language remains incomplete and cannot contribute family evidence.
