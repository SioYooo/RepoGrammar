# Ruby language completion review

- Status: Incomplete — `discovered_only`
- Authority: ADR-0020 and ADR-0022
- Dependency prerequisite: `6a6f88538090ef386a8420c7424a380d97fa1f74`
- Last updated: 2026-08-01

## ADR-0020 gate

- [x] Discovery/config — bounded Ruby/Bundler inventory exists; project files
  are not executed.
- [ ] Authoritative frontend — stage-3 dependency and sandbox qualification is
  incomplete.
- [ ] RepoGrammar-owned code units and IR.
- [ ] Typed `UNKNOWN` registry and provider fallback.
- [ ] `ruby.minitest.test_method` exact family with support at least three.
- [ ] Positive, lookalike, low-support, parse-degraded, variant, and
  unresolved/resolved fixtures.
- [ ] Source-free readiness and leakage review.
- [x] Four-part review record — this report records correctness, security,
  completeness, and performance findings; open findings remain blockers.
- [ ] Linked atomic prerequisite commits and final completion audit.

## Current evidence and blocker

Discovery inventories `.rb`, Bundler manifests/locks, `.ruby-version`, and
gemspec paths without running Ruby or Bundler. A pure Rust parser inventories
only strict direct entries from exact `Gemfile.lock` `DEPENDENCIES` as
`rubygems` manifest declarations. Executable Gemfile/gemspec DSLs, unsupported
lock sources and variants, malformed/conflicting input, and resource exhaustion
fail closed as `ruby_dependency_inventory` `UNKNOWN`; no resolved-version,
framework, family, or support claim follows. Dependency rows persist across an
unrelated incremental source edit, while Ruby source remains unread and
inventory-only.

The next permitted semantic stage is
documentation/reproducibility qualification of `ruby-prism` 1.9.0, its exact
artifact/upstream source, native dependency closure, CRuby 4.0.6 differential
behavior, malformed/resource corpora, five targets, and native OS sandboxes.
The current host Ruby 2.6.10 is not the required oracle.

Native Prism must remain outside the main process. Failure to pin or contain the
candidate, native crashes on malformed input, uncertain ranges, or any need to
execute Gemfile, gemspec, Ruby, Bundler, RubyGems, Rake, Rails, or tests is
`NO_GO`, not permission to claim semantics from spelling.

## Completion verdict

Not complete. No completion percentage or supported-language count may include
Ruby until every checkbox is linked to current-branch evidence.

## Final program audit fields

| Required field | Audited result |
|---|---|
| Language / rank | Ruby / 20 |
| Dialect/version | No selected engine or project profile. CRuby 4.0.6 is a future differential target; ambient Ruby 2.6.10 is not evidence. |
| Provider/frontend/version | None integrated. `ruby-prism` 1.9.0 is a qualification candidate only. |
| Discovery/config | `.rb`, Bundler manifests/locks, gemspecs, and `.ruby-version` inventory with Ruby-specific exclusions. |
| Manifest/lockfile | Direct root rows from exact `Gemfile.lock` `DEPENDENCIES`; no resolved-spec join, group/scope, or executable Gemfile/gemspec evaluation. |
| Owned source IR | Absent. |
| External symbols | Absent; require/load/autoload, constants, refinements, metaprogramming, and gem ownership are unresolved. |
| Library Contracts | Registry infrastructure exists, production packs = 0; no Ruby contract can match manifest-only dependency rows. |
| Exact-anchor family | Absent; proposed Minitest family is not implemented. |
| Fixtures | Strong Bundler inventory/no-execution/resource/incremental tests; no qualified Prism source/family matrix. |
| Primary UNKNOWN cases | Engine/version/root selection, executable DSLs, non-registry sources, resolved specs, require graph, dynamic definitions, native extensions, provider and sandbox availability. |
| Source-free result | Pass for inventory; full Ruby readiness is absent. |
| Completion state / counted | `discovered_only`; strict gate count `2/9`; Top-20 complete = no. |

## Four-part review

- Correctness: the direct `DEPENDENCIES` subset is intentionally not a resolved
  gem graph and cannot prove source ownership, loading, or Minitest semantics.
- Security: Ruby, Bundler, RubyGems, gemspec/Gemfile DSLs, Rake, Rails, tests,
  native extensions, children, and network never run. Native Prism isolation
  and malformed-input qualification are not complete.
- Completeness: discovery plus one static subset exists; source frontend/IR,
  provider obligations, exact family, completion fixtures, and audit do not.
- Performance: bounded text/record parsing is covered; no Prism, native parser,
  large gem graph, metaprogramming, or incremental-source benchmark exists.

Evidence paths are `src/rust/adapters/languages/ruby.rs`,
`src/rust/adapters/parsing/ruby.rs`, ADR-0022, product/incremental tests, and
prerequisite `6a6f88538090ef386a8420c7424a380d97fa1f74`. Exact non-claim:
Bundler declaration inventory is not installed/resolved gem evidence, Ruby
source semantics, Minitest identity, or behavior support.
