# Visual Basic .NET language completion review

- Status: Incomplete — `structural_substrate`
- Authority: ADR-0020, ADR-0030, and ADR-0031
- Dependency prerequisite: `2b5fd6d1bf32e8805798e26e53ff9f090ec22d3e`
- Last updated: 2026-08-01

## ADR-0020 gate

- [x] Discovery/configuration — exact lowercase `.vb`/`.vbproj` are VB.NET
  inventory, VB6 is excluded, and bounded literal project metadata is retained.
- [x] Authoritative source frontend for the declared scope — ADR-0043 D4b's
  hand-written lexer and recursive-descent parser reads only constructs every
  VB.NET language version from 10 through 17 lexes and nests identically, so the
  parse it admits does not depend on the version it cannot select. Its fidelity
  boundary is the declared subset, and its parse-degraded behaviour is a
  whole-file abstention with a typed `UNKNOWN` and a diagnostic: interpolated
  strings, XML literals, unterminated literals, unread `#` directives, and
  anything outside the grammar yield no anchor rather than a recovered one. No
  Roslyn version, target framework, or MSBuild profile is pinned, and D4c states
  why none is needed: `Option Strict`/`Explicit`/`Infer` change binding rather
  than token boundaries, and the framework and profile change reference
  resolution rather than the token stream, so neither can change what the parser
  admits. This is not a Roslyn-equivalent VB compiler frontend, and the ADR
  forbids widening the subset or the version set without a superseding decision.
- [x] RepoGrammar-owned source code units and IR — ADR-0043 emits a module unit
  per decoded `.vb` file, one unit for an admitted `TestClass`, and one per
  admitted `TestMethod`, each projected into the shared IR.
- [x] Typed source-semantic `UNKNOWN` registry and provider fallback — an
  MSTest attribute name without its import or a fully qualified namespace
  yields `UnresolvedImport` under `vb_mstest_attribute_binding`, and it blocks
  family membership.
- [x] First exact framework family with support at least three —
  `framework:vb_mstest.test_method` over the `mstest.TestMethod` anchor, gated
  at support three.
- [x] Positive, lookalike, low-support, build-variant, and parse-degraded
  fixtures exist for that family, each exercised through the product CLI.
  `mstest_build_variant` puts four attributed declarations in opposite `#If`
  branches and `mstest_parse_degraded` puts three in a file whose parse fails;
  both carry enough declarations to clear the support threshold of three if read
  naively, and both must form no family and derive no support. The degraded one
  must also surface a `parse degraded for CatalogTests.vb` index warning, which
  is the signal a scanner could not produce. Resolved/unresolved fixtures do not
  exist: there is no VB provider to resolve against.
- [x] Complete source-free readiness and leakage matrix — Visual Basic .NET is registered
  in the repo-shape language scopes, so its units and families are counted
  rather than silently reported as zero; `status`, `doctor`, `stats`,
  `unknowns`, `families`, `files`, and the MCP `inspect_readiness` and
  `find_analogues` payloads are each asserted over both an indexed positive
  workspace and an indexed unbound one to expose no identifier, literal, or
  source text and no absolute path. The assertions are non-vacuous: every
  command must exit zero and parse, the positive workspace must report
  `framework:vb_mstest.test_method`, and the unbound workspace must report the lane's typed
  `UNKNOWN` by bounded language token `visual-basic` and count.
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
custom/external entities, and invalid XML characters fail closed. Project
dependencies copy forward once, replace on modification, disappear on removal,
and never create families. Product tests prove honest
`file_manifest_only`/`syntax_only_code_units` output without source/package
leakage.

`.vb` bytes do reach the parser under ADR-0043, which supersedes the earlier
ADR-0031 property that source was discovered and never decoded. The reading is
in-process and bounded by the shared input-byte ceiling plus the parser's own
token and nesting ceilings; no source text, identifier, or literal reaches a
fact target, note, assumption, code-unit id, or any public surface.

No Roslyn, Visual Basic compiler, `dotnet`, MSBuild, NuGet, analyzer, generator,
repository program, child process, or network operation is used. A future
semantic stage — type resolution, assembly identity, cross-file `Partial`
assembly, or test discovery — still requires an independently reviewed provider
and a version/project-profile contract, neither of which the ADR-0043 subset
needs for what it claims.

## Completion verdict

Not complete. Visual Basic .NET has a bounded parser over a declared language
subset, one admitted attribute shape, owned units and IR, typed `UNKNOWN`s for
attribute binding, conditional compilation, and syntax admission, and one exact
family with support three under ADR-0043. The `.vbproj` metadata inventory
remains auxiliary and is not compiler, build, or dependency-resolution evidence.
Strict gate count is `8/9`; the linked-prerequisite and final completion audit
remains open, and it must not be counted as a supported language.

Two limitations are stated rather than left to inference. The subset is narrow,
and a file using an interpolated string or an XML literal contributes nothing at
all — that is real lost recall, chosen over reading a token stream whose
boundaries are not decidable. And the frontend proves a source-level attribute
binding, not discovery: it does not claim the MSTest assembly is referenced, the
package restored, or the test executed.

## Final program audit fields

| Required field | Audited result |
|---|---|
| Language / rank | Visual Basic .NET / 7 |
| Dialect/version | Exact lowercase `.vb`/`.vbproj` inventory is explicitly VB.NET; VB6 is excluded. No SDK, target framework, compile-item, or MSBuild profile is selected. The source frontend declares an invariance set of VB.NET language versions 10 through 17 and selects no member of it. |
| Provider/frontend/version | RepoGrammar-owned hand-written VB.NET lexer and recursive-descent parser over the ADR-0043 D4b subset, engine `repogrammar-vbnet-mstest-parser`, method `bounded_vbnet_mstest_declaration_v2`. No Roslyn/VB compiler provider, no grammar, no external artifact. |
| Manifest/lockfile | Bounded direct literal NuGet `PackageReference` declarations from `.vbproj`; no restore graph, central versioning, assets lock, or resolved assembly. |
| Owned source IR / external symbols | Owned units and IR exist for the ADR-0043 anchor only; external symbols stay absent, and types, members, and assemblies are unresolved. |
| Library Contracts | Exact-version registry exists, production packs = 0; manifest-only NuGet rows are insufficient for lookup. |
| Exact family / fixtures | One exact family, `framework:vb_mstest.test_method` over `mstest.TestMethod`, gated at support three. Positive, lookalike, low-support, and parse-degraded fixtures exist; resolved/unresolved do not, because there is no VB provider. |
| Primary UNKNOWN cases | `#If` conditional-compilation constants, unadmitted syntax (interpolated strings, XML literals, unterminated literals, unread directives, ungrammatical declarations), unbound MSTest attribute names, SDK imports, properties, conditions, item transforms, central versions, analyzers/generators, target framework, assembly graph, cross-file `Partial` assembly, source recovery, and provider availability. |
| Source-free / security | Inventory and parser outputs are source-free; the parser emits no identifier, literal, or source text and works from a fixed token vocabulary; XML is bounded and DTD/entity-free; input is bounded by the shared byte ceiling plus token and nesting ceilings; no compiler, `dotnet`, MSBuild, NuGet, analyzer, child, project code, or network runs. |
| Completion state / counted | `structural_substrate`; strict gate count `8/9`; Top-20 complete = no. |

Four-part review: correctness preserves only literal direct declarations and
only declarations the parser admits, with every refusal typed; security fails
closed on dynamic XML/MSBuild semantics and treats source as untrusted bounded
input; completeness lacks type resolution, assembly identity, and cross-file
`Partial` assembly; performance is bounded by the shared XML/file/record
ceilings plus the parser's token and nesting ceilings, but has no Roslyn or
solution-scale measurement. Evidence:
`src/rust/adapters/languages/visual_basic.rs`,
`src/rust/adapters/parsing/visual_basic.rs`,
`src/rust/adapters/parsing/visual_basic/syntax.rs`,
`src/rust/adapters/parsing/visual_basic/mstest.rs`, product/incremental tests,
ADR-0031, ADR-0043, and `2b5fd6d1bf32e8805798e26e53ff9f090ec22d3e`. Exact
non-claims: a `PackageReference` does not prove restore, assembly identity,
generated code, framework behavior, or source support; and an admitted anchor
proves a source-level attribute binding, not that the test is discovered,
referenced, or executed.
