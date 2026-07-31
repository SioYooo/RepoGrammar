# R language completion review

- Status: Incomplete — `discovered_only`
- Authority: ADR-0020 and ADR-0036
- Dependency prerequisite: `82ced893f81a954b64e20546dfc4ad81043b28e5`
- Last updated: 2026-08-01

## ADR-0020 gate

- [x] Discovery/configuration — exact `.R`, `DESCRIPTION`, `NAMESPACE`, and
  `renv.lock`, R-specific exclusions, bounded dependency metadata, typed
  inventory uncertainty, persistence, and incremental behavior are covered.
- [ ] Authoritative sandboxed R source frontend and selected project/profile.
- [ ] RepoGrammar-owned R source code units and IR.
- [ ] Complete source-semantic obligation registry and provider fallback;
  inventory-only uncertainty is insufficient.
- [ ] Exact family with support at least three.
- [ ] Positive, lookalike, low-support, parse-degraded, NSE/dispatch, and
  unresolved/resolved family fixtures.
- [ ] Complete source-free readiness and leakage matrix across required public
  surfaces.
- [x] Four-part review record — this report records correctness, security,
  completeness, and performance findings; open findings remain blockers.
- [ ] Linked atomic prerequisites and final completion audit.

## Current evidence and blocker

`.R` bytes are never decoded or parsed. The metadata adapter is static and
non-executing. DESCRIPTION/NAMESPACE package declarations abstain when CRAN
versus Bioconductor cannot be proved. renv rows retain unknown directness and
scope; remote/custom/local sources are omitted without retaining their values.
No R, renv, profile, package, native code, repository script, child process, or
network action runs.

## Completion verdict

Not complete. The project-model inventory is auxiliary evidence, not R language
support. No R family, provider, support, or readiness exists.

## Final program audit fields

| Required field | Audited result |
|---|---|
| Language / rank | R / 9 |
| Dialect/version | Exact `.R` is inventory-only; no R release, platform, project, library path, profile, renv activation, native-code, or repository selection. |
| Provider/frontend/version | None; no R parser, languageserver, compiler/runtime, or provider version. |
| Discovery/config | `.R`, `DESCRIPTION`, `NAMESPACE`, and `renv.lock` with managed-library/IDE exclusions. |
| Manifest/lockfile | Bounded DESCRIPTION/NAMESPACE declarations plus exact explicit CRAN/Bioconductor `renv.lock` versions; ambiguous registries and remote/custom/local sources are omitted. |
| Owned source IR / external symbols | Both absent; package imports, S3/S4/R6 dispatch, native symbols, NSE, and generated code are unresolved. |
| Library Contracts | Registry exists, production packs = 0; inventory never creates a behavior contract. |
| Exact family / fixtures | No family. Strong DCF/JSON/resource/remote-source/leakage/incremental tests; no R source-family corpus. |
| Primary UNKNOWN cases | Repository identity, selected lock/project/profile, remote sources, package directness/scope, NSE/metaprogramming, dispatch, native code, and provider availability. |
| Source-free / security | Metadata results are source-free; `.R` is zero-read; no R, renv, package/profile script, native code, child, repository code, or network runs. |
| Completion state / counted | `discovered_only`; strict gate count `2/9`; Top-20 complete = no. |

Four-part review: correctness preserves only explicit registry evidence and
never defaults ambiguous packages to CRAN; security discards remote/path values
and executes nothing; completeness lacks source/provider/family layers;
performance is bounded for DCF/JSON inventory with no R-runtime or large-lock
benchmark. Evidence: `src/rust/adapters/languages/r.rs`,
`src/rust/adapters/parsing/r.rs`, ADR-0036, product/incremental tests, and
`82ced893f81a954b64e20546dfc4ad81043b28e5`. Exact non-claim: package
metadata does not prove installation, loading, namespace binding, dispatch,
native compatibility, or R support.
