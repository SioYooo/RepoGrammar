# Swift language completion review

- Status: Incomplete — `discovered_only`
- Authority: ADR-0020 and ADR-0025
- Reviewed baseline: `86dba38ada7fe5646b5ab8770e2ad4183e8d22d7`
- Last updated: 2026-08-01

## ADR-0020 gate

- [ ] Discovery/config — bounded Swift/SwiftPM inventory exists; no manifest or
  target evaluation occurs.
- [ ] Authoritative frontend — stage-3 qualification is incomplete.
- [ ] RepoGrammar-owned code units and IR.
- [ ] Typed `UNKNOWN` registry and provider fallback.
- [ ] `swift.xctest.test_method` exact family with support at least three.
- [ ] Positive, lookalike, low-support, parse-degraded, variant, and
  unresolved/resolved fixtures.
- [ ] Source-free readiness and leakage review.
- [ ] Correctness, security, completeness, and performance review.
- [ ] Linked atomic prerequisite commits and final completion audit.

## Current evidence and blocker

Discovery inventories `.swift`, `Package.swift`, `Package.resolved`,
`.swift-version`, and valid version-specific manifests. The next permitted stage
is documentation/evidence-only qualification of SwiftSyntax 603.0.2, Swift 6.3.3
compiler differential behavior, dependency closure, five targets, and native OS
sandboxes. Current arm64 macOS tools prove only local availability, not the full
qualification matrix.

Stage 3 may conclude `QUALIFIED`, `NO_GO`, `BLOCKED`, or `INCONCLUSIVE` but must
not add a production dependency, worker, parser, IR, UNKNOWN, or family. If exact
XCTest identity requires opening the target repository, evaluating
`Package.swift`, building/indexing modules, resolving dependencies, loading
macros/plugins, or ambient SDK state, the N1 semantic path is `NO_GO`.

## Completion verdict

Not complete. No completion percentage or supported-language count may include
Swift until every checkbox is linked to current-branch evidence.
