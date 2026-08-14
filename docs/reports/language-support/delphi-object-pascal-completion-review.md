# Delphi/Object Pascal language completion review

- Status: Incomplete — `structural_substrate`
- Authority: ADR-0020, ADR-0030, ADR-0032, and ADR-0044
- Dependency prerequisite: `2b5fd6d1bf32e8805798e26e53ff9f090ec22d3e`
- Last updated: 2026-08-15

## ADR-0020 gate

- [x] Discovery/configuration — `.pas`/`.dpr`/`.dpk` remain generic
  `object-pascal`, only `.dproj` is Delphi-qualified, and bounded literal
  runtime-package metadata is retained.
- [ ] Evidence-pinned Delphi frontend and version/project profile; Free
  Pascal/Lazarus requires a separate qualification and is not equivalent.
  ADR-0044's scanner is bounded to one attribute shape and pins no version, so
  this gate stays open.
- [x] RepoGrammar-owned source code units and IR — ADR-0044 emits a module
  unit per decoded `.pas`, a fixture unit, and a test-procedure unit, each
  projected into the shared IR.
- [x] Typed source-semantic `UNKNOWN` registry and provider fallback — an
  unbound DUnitX attribute name yields `UnresolvedImport` under
  `delphi_dunitx_attribute_binding`, and it blocks family membership.
- [x] First exact framework family with support at least three —
  `framework:dunitx.test_procedure` over the `dunitx.Test` anchor, gated at
  support three.
- [ ] Positive, lookalike, and low-support fixtures exist for that family;
  degraded and resolved/unresolved do not, because a scanner has no parse
  failure and there is no Delphi provider to resolve against.
- [ ] Complete source-free readiness and leakage matrix.
- [x] Four-part review record — this report records correctness, security,
  completeness, and performance findings; open findings remain blockers.
- [ ] Linked semantic prerequisite commits and final completion audit.

## Current evidence and boundary

The current slice inventories exact lowercase `.pas`, `.dpr`, and `.dpk`
without reading them and deliberately assigns the dialect-neutral
`object-pascal` token. Exact `.dproj` is the only `delphi-config` input.
Free Pascal/Lazarus `.pp`, `.lpr`, `.lpi`, and `.lpk` inputs are not admitted,
because the reviewed evidence did not establish a stable normative project/
package model equivalent to Delphi. `__history`/`__recovery` exclusions remain
language-specific.

The bounded project parser accepts literal semicolon-separated
`DCC_UsePackage` names only from direct-root property groups. Rows use the
ADR-0030 Delphi-package ecosystem, runtime scope, manifest-declared evidence,
unknown directness, and no version. Imports, conditions, property chains,
paths, compiled-package suffixes, malformed XML, and resource exhaustion
produce source-free `delphi_dependency_inventory` uncertainty. Every nonempty
list explicitly retains directness uncertainty because the official project
list may contain automatically added packages.

Full and incremental tests prove that Object Pascal bytes never enter the
source store/parser; `.dproj` dependencies copy forward once, replace on
modification, disappear on removal, and never create families. Product tests
prove honest indexing mode without source/package leakage. No Delphi or Free
Pascal compiler, RAD Studio, Lazarus, MSBuild, package manager, package loader,
repository program, child process, or network operation is used.

## Completion verdict

Not complete. Delphi/Object Pascal has a bounded scanner over one attribute
shape, owned units and IR, a typed attribute-binding `UNKNOWN`, and one exact
family with support three under ADR-0044. The `.dproj` metadata inventory
remains auxiliary and is not compiler, build, or package-resolution evidence.
Strict gate count is `5/9`; it must not be counted as a supported language.

Two limitations are stated rather than left to inference. This frontend is a
scanner, so malformed Object Pascal does not fail — it yields fewer admitted
declarations, which is indistinguishable from a file with fewer declarations;
the parse-degraded gate stays open for that reason. And the dialect evidence is
`uses DUnitX.TestFramework`, never the `.pas` suffix, which ADR-0032 forbids as
a dialect oracle: a `.pas` unit that does not import DUnitX is still
dialect-neutral Object Pascal, and Free Pascal/Lazarus remains separately
deferred and must not be counted as implemented by this lane.

## Final program audit fields

| Required field | Audited result |
|---|---|
| Language / rank | Delphi/Object Pascal / 11 |
| Dialect/version | Source suffixes remain dialect-neutral Object Pascal; only `.dproj` metadata is Delphi-qualified. No RAD Studio/Delphi version or Free Pascal equivalence is selected. |
| Provider/frontend/version | None; no Delphi/FPC parser, compiler, LSP, or version. |
| Manifest/lockfile | Bounded literal `.dproj` `DCC_UsePackage` rows with runtime scope, unknown directness, and no version; `.lpi`/`.lpk`/`fpmake` remain deferred. |
| Owned source IR / external symbols | Both absent. Package/unit/class/member identity is unresolved. |
| Library Contracts | Registry exists, production packs = 0; versionless unknown-directness rows cannot match a reviewed contract. |
| Exact family / fixtures | No family. Strong discovery/XML/resource/leakage/incremental tests exist, but no source-family corpus. |
| Primary UNKNOWN cases | Compiler dialect, project selection, MSBuild properties/conditions/imports, automatically added packages, version suffixes, unit search paths, generated forms/resources, and provider availability. |
| Source-free / security | Source/config outputs are source-free; bounded XML rejects active constructs; no compiler, IDE, MSBuild, package loader, child, repository code, or network runs. |
| Completion state / counted | `discovered_only`; strict gate count `2/9`; Top-20 complete = no. |

Four-part review: correctness retains explicit direct-root text while preserving
unknown directness; security is non-executing and fail-closed; completeness has
no source semantics, provider, family, or readiness; performance is bounded for
XML inventory but unmeasured for compiler/project-scale work. Evidence:
`src/rust/adapters/languages/object_pascal.rs`,
`src/rust/adapters/parsing/delphi.rs`, ADR-0032, product/incremental tests, and
`2b5fd6d1bf32e8805798e26e53ff9f090ec22d3e`. Exact non-claim: Delphi
metadata does not implement Free Pascal/Lazarus, resolve packages, or prove any
source/runtime behavior.
