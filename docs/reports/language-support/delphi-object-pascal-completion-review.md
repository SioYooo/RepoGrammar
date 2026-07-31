# Delphi/Object Pascal language completion review

- Status: Incomplete — `discovered_only`
- Authority: ADR-0020, ADR-0030, and ADR-0032
- Reviewed baseline: `abda602fe38db8549e4f318bd4638ddb94df7282`
- Last updated: 2026-08-01

## ADR-0020 gate

- [x] Dialect-safe discovery — `.pas`/`.dpr`/`.dpk` remain generic
  `object-pascal`; only `.dproj` is Delphi-qualified configuration.
- [x] Bounded non-executing Delphi metadata — literal runtime-package rows and
  scoped inventory uncertainty are persisted with unknown directness.
- [ ] Evidence-pinned Delphi source frontend and version/project profile.
- [ ] Separately qualified Free Pascal/Lazarus lane; no dialect equivalence.
- [ ] RepoGrammar-owned source code units and IR.
- [ ] Typed source-semantic `UNKNOWN` registry and provider fallback.
- [ ] First exact framework family with support at least three.
- [ ] Positive, lookalike, low-support, degraded, and resolved/unresolved
  fixtures for that family.
- [ ] Source-free readiness, completeness, correctness, security, and
  performance completion review.
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

Not complete. The Delphi-qualified metadata slice and generic Object Pascal
inventory remain `discovered_only`. They are not source semantics, compiler
compatibility, resolved package relationships, framework evidence, or support.
Free Pascal/Lazarus remains separately deferred and must not be counted as
implemented by this lane.
