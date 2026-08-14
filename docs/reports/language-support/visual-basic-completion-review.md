# Visual Basic .NET language completion review

- Status: Incomplete — `structural_substrate`
- Authority: ADR-0020, ADR-0030, and ADR-0031
- Dependency prerequisite: `2b5fd6d1bf32e8805798e26e53ff9f090ec22d3e`
- Last updated: 2026-08-01

## ADR-0020 gate

- [x] Discovery/configuration — exact lowercase `.vb`/`.vbproj` are VB.NET
  inventory, VB6 is excluded, and bounded literal project metadata is retained.
- [ ] Authoritative source frontend and versioned project profile.
- [ ] RepoGrammar-owned source code units and IR.
- [ ] Typed source-semantic `UNKNOWN` registry and provider fallback.
- [ ] First exact framework family with support at least three.
- [ ] Positive, lookalike, low-support, degraded, and resolved/unresolved
  fixtures for that family.
- [ ] Complete source-free readiness and leakage matrix.
- [x] Four-part review record — this report records correctness, security,
  completeness, and performance findings; open findings remain blockers.
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

Not complete. Visual Basic .NET has a bounded scanner over one attribute shape,
owned units and IR, a typed attribute-binding `UNKNOWN`, and one exact family
with support three under ADR-0043. The `.vbproj` metadata inventory remains
auxiliary and is not compiler, build, or dependency-resolution evidence. Strict
gate count is `5/9`; it must not be counted as a supported language.

One limitation is stated rather than left to inference: this frontend is a
scanner, so malformed VB does not fail — it yields fewer admitted declarations,
which is indistinguishable from a file with fewer declarations. The
parse-degraded gate stays open for that reason.

## Final program audit fields

| Required field | Audited result |
|---|---|
| Language / rank | Visual Basic .NET / 7 |
| Dialect/version | Exact lowercase `.vb`/`.vbproj` inventory is explicitly VB.NET; VB6 is excluded. No SDK, target framework, language version, compile-item, or MSBuild profile is selected. |
| Provider/frontend/version | None; no Roslyn/VB compiler provider or version. |
| Manifest/lockfile | Bounded direct literal NuGet `PackageReference` declarations from `.vbproj`; no restore graph, central versioning, assets lock, or resolved assembly. |
| Owned source IR / external symbols | Both absent; config units are not VB source IR, and types/members/assemblies are unresolved. |
| Library Contracts | Exact-version registry exists, production packs = 0; manifest-only NuGet rows are insufficient for lookup. |
| Exact family / fixtures | No exact family. Strong discovery/XML/resource/leakage/incremental tests exist, but no source-family matrix. |
| Primary UNKNOWN cases | SDK imports, properties, conditions, item transforms, central versions, analyzers/generators, target framework, assembly graph, source recovery, and provider availability. |
| Source-free / security | Inventory outputs are source-free; XML is bounded and DTD/entity-free; no compiler, `dotnet`, MSBuild, NuGet, analyzer, child, project code, or network runs. |
| Completion state / counted | `structural_substrate`; strict gate count `2/9`; Top-20 complete = no. |

Four-part review: correctness preserves only literal direct declarations;
security fails closed on dynamic XML/MSBuild semantics; completeness lacks every
claim-bearing source stage; performance is bounded by the shared XML/file/
record ceilings but has no Roslyn or solution-scale measurement. Evidence:
`src/rust/adapters/languages/visual_basic.rs`,
`src/rust/adapters/parsing/visual_basic.rs`, product/incremental tests,
ADR-0031, and `2b5fd6d1bf32e8805798e26e53ff9f090ec22d3e`. Exact non-claim:
a `PackageReference` does not prove restore, assembly identity, generated code,
framework behavior, or source support.
