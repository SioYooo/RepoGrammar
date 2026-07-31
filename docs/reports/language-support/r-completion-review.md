# R language completion review

- Status: Incomplete — `discovered_only`
- Authority: ADR-0020 and ADR-0036
- Integration baseline: `abda602fe38db8549e4f318bd4638ddb94df7282`
- Last updated: 2026-08-01

## ADR-0020 gate

- [x] Discovery/configuration — exact `.R`, `DESCRIPTION`, `NAMESPACE`, and
  `renv.lock` classification with R-specific generated-library exclusions.
- [x] Bounded dependency inventory — official DESCRIPTION/NAMESPACE fields are
  classified conservatively; only explicit CRAN/Bioconductor renv identities
  and versions enter the shared model.
- [x] Typed inventory UNKNOWN, exact/+1 resources, source-read boundaries,
  leakage checks, CLI mode, persistence, copy-forward, replacement, and removal.
- [ ] Authoritative sandboxed R source frontend and selected project/profile.
- [ ] RepoGrammar-owned R source code units and IR.
- [ ] Source-semantic obligation registry and provider fallback.
- [ ] Exact family with support at least three.
- [ ] Source-free readiness plus independent correctness/security/performance review.
- [ ] Linked final completion audit.

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
