# Visual Basic .NET language completion review

- Status: Incomplete — `discovered_only`
- Authority: ADR-0020, ADR-0030, and ADR-0031
- Reviewed baseline: `abda602fe38db8549e4f318bd4638ddb94df7282`
- Last updated: 2026-08-01

## ADR-0020 gate

- [x] Dialect-qualified discovery/config — exact lowercase `.vb` and `.vbproj`
  are VB.NET inventory; VB6 formats are excluded.
- [x] Bounded non-executing project metadata — literal direct
  `PackageReference` rows and scoped inventory uncertainty are persisted.
- [ ] Authoritative source frontend and versioned project profile.
- [ ] RepoGrammar-owned source code units and IR.
- [ ] Typed source-semantic `UNKNOWN` registry and provider fallback.
- [ ] First exact framework family with support at least three.
- [ ] Positive, lookalike, low-support, degraded, and resolved/unresolved
  fixtures for that family.
- [ ] Source-free readiness, completeness, correctness, security, and
  performance completion review.
- [ ] Linked semantic prerequisite commits and final completion audit.

## Current evidence and boundary

The current slice classifies exact `.vb` source as unread inventory and exact
`.vbproj` files as bounded project metadata. It rejects VB6 extensions and
keeps `bin`/`.vs` exclusions language-specific. The project parser accepts only
literal direct-root `PackageReference` declarations through a bounded
RepoGrammar-owned XML reader. Safe rows use ADR-0030 NuGet identity, direct
directness, unknown scope, manifest-declared evidence, an optional literal
requirement, and no resolved version.

SDK imports, explicit imports, properties, conditions, updates, removals,
overrides, invalid/conflicting entries, malformed XML, and resource exhaustion
produce source-free `visual_basic_dependency_inventory` uncertainty. DTDs,
custom/external entities, and invalid XML characters fail closed. Full and
incremental tests prove that `.vb` bytes never enter the source store/parser;
project dependencies copy forward once, replace on modification, disappear on
removal, and never create families. Product tests prove honest
`file_manifest_only`/`syntax_only_code_units` output without source/package
leakage.

No Roslyn, Visual Basic compiler, `dotnet`, MSBuild, NuGet, analyzer, generator,
repository program, child process, or network operation is used. A future
semantic stage requires an independently reviewed Visual Basic frontend,
version/project-profile contract, isolated malformed/resource corpus, and
framework-specific family evidence.

## Completion verdict

Not complete. Visual Basic .NET remains `discovered_only`; the current metadata
inventory is not compiler, framework, build, dependency-resolution, or support
evidence. It must not be counted as a supported language.
