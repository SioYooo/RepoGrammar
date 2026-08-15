# R language completion review

- Status: Incomplete — `structural_substrate`
- Authority: ADR-0020 and ADR-0036
- Dependency prerequisite: `82ced893f81a954b64e20546dfc4ad81043b28e5`
- Last updated: 2026-08-01

## ADR-0020 gate

- [x] Discovery/configuration — exact `.R`/`.r`, `DESCRIPTION`, `NAMESPACE`,
  and `renv.lock`, R-specific exclusions, bounded dependency metadata, typed
  inventory uncertainty, persistence, and incremental behavior are covered.
- [x] Authoritative frontend for the declared scope — ADR-0042's bounded
  in-process scanner reads `tests/testthat/test-*.R` only and recognizes one
  call shape. It is comment-, string-, and raw-string-aware. ADR-0036 carries no
  evidence ladder and no prohibition on this route; its restrictions all name
  executing R, and nothing here runs. No selected project model or profile
  exists, and none is claimed.
- [x] RepoGrammar-owned R source code units and IR — a module unit per admitted
  file, one `r_test_that_block` unit per admitted call, source ranges, content
  hashes, IR nodes, and containment edges.
- [ ] Complete source-semantic obligation registry and provider fallback;
  inventory-only uncertainty is insufficient.
- [x] Exact family with support at least three — `testthat.test_that` over the
  fixed `testthat.test_that` target, minimum support three rather than the
  shared default of two, and the owned `repogrammar-r-derived` origin.
- [x] Positive, lookalike, low-support, and parse-degraded family fixtures. An
  unclosed quote or bracket is a decidable well-formedness violation and reports
  a degraded parse. NSE/dispatch and unresolved/resolved fixtures do not exist:
  there is no R provider to resolve against.
- [ ] Complete source-free readiness and leakage matrix across required public
  surfaces.
- [x] Four-part review record — this report records correctness, security,
  completeness, and performance findings; open findings remain blockers.
- [ ] Linked atomic prerequisites and final completion audit.

## Current evidence and blocker

`.R` and `.r` bytes are never decoded or parsed. The metadata adapter is static
and non-executing. DESCRIPTION/NAMESPACE package declarations abstain when CRAN
versus Bioconductor cannot be proved. renv rows retain unknown directness and
scope; remote/custom/local sources are omitted without retaining their values.
No R, renv, profile, package, native code, repository script, child process, or
network action runs.

## Completion verdict

Not complete. R has a bounded scanner over one path class and one call shape,
owned units and IR, a typed identity `UNKNOWN`, and one exact family with
support three. It has no audited source-free readiness matrix, no final audit,
and no provider. Strict gate count is `6/9`;
R is `structural_substrate` and must not be counted as supported.

One limitation is worth stating rather than leaving to inference: this frontend
is a scanner, so malformed R does not fail the way a parsed language does. It
reports a degraded parse when its own well-formedness invariant is violated, and
outside that invariant fewer admitted calls remain indistinguishable from a file
with fewer calls.

## Final program audit fields

| Required field | Audited result |
|---|---|
| Language / rank | R / 9 |
| Dialect/version | Exact `.R`/`.r` is inventory-only; no R release, platform, project, library path, profile, renv activation, native-code, or repository selection. |
| Provider/frontend/version | None; no R parser, languageserver, compiler/runtime, or provider version. |
| Discovery/config | `.R`, `.r`, `DESCRIPTION`, `NAMESPACE`, and `renv.lock` with managed-library/IDE exclusions. |
| Manifest/lockfile | Bounded DESCRIPTION/NAMESPACE declarations plus exact explicit CRAN/Bioconductor `renv.lock` versions; ambiguous registries and remote/custom/local sources are omitted. |
| Owned source IR / external symbols | Owned units exist for the ADR-0042 anchor only, and the IR abstains on their kind because a testthat block is a call, not a declaration; external symbols stay absent, and package imports, S3/S4/R6 dispatch, native symbols, NSE, and generated code are unresolved. |
| Library Contracts | Registry exists, production packs = 0; inventory never creates a behavior contract. |
| Exact family / fixtures | One exact family, `framework:testthat.test_that` over the testthat block anchor, gated at support three. Positive, lookalike, low-support, and parse-degraded fixtures exist; resolved/unresolved do not, because there is no R provider. |
| Primary UNKNOWN cases | Repository identity, selected lock/project/profile, remote sources, package directness/scope, NSE/metaprogramming, dispatch, native code, and provider availability. |
| Source-free / security | Metadata results are source-free; `.R`/`.r` is zero-read; no R, renv, package/profile script, native code, child, repository code, or network runs. |
| Completion state / counted | `structural_substrate`; strict gate count `6/9`; Top-20 complete = no. |

Four-part review: correctness preserves only explicit registry evidence and
never defaults ambiguous packages to CRAN; security discards remote/path values
and executes nothing; completeness lacks source/provider/family layers;
performance is bounded for DCF/JSON inventory with no R-runtime or large-lock
benchmark. Evidence: `src/rust/adapters/languages/r.rs`,
`src/rust/adapters/parsing/r.rs`, ADR-0036, product/incremental tests, and
`82ced893f81a954b64e20546dfc4ad81043b28e5`. Exact non-claim: package
metadata does not prove installation, loading, namespace binding, dispatch,
native compatibility, or R support.
