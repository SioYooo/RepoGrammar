# Ruby language completion review

- Status: Incomplete — `discovered_only`
- Authority: ADR-0020 and ADR-0022
- Reviewed baseline: `86dba38ada7fe5646b5ab8770e2ad4183e8d22d7`
- Last updated: 2026-08-01

## ADR-0020 gate

- [ ] Discovery/config — bounded Ruby/Bundler inventory exists; project files
  are not executed.
- [ ] Authoritative frontend — stage-3 dependency and sandbox qualification is
  incomplete.
- [ ] RepoGrammar-owned code units and IR.
- [ ] Typed `UNKNOWN` registry and provider fallback.
- [ ] `ruby.minitest.test_method` exact family with support at least three.
- [ ] Positive, lookalike, low-support, parse-degraded, variant, and
  unresolved/resolved fixtures.
- [ ] Source-free readiness and leakage review.
- [ ] Correctness, security, completeness, and performance review.
- [ ] Linked atomic prerequisite commits and final completion audit.

## Current evidence and blocker

Discovery inventories `.rb`, Bundler manifests/locks, `.ruby-version`, and
gemspec paths without running Ruby or Bundler. The next permitted stage is
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
