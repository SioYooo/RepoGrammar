# Swift language completion review

- Status: Incomplete — `discovered_only`
- Authority: ADR-0020 and ADR-0025
- Dependency prerequisite: `9f1f657a27d45264d3b5e9b681750778234f0cb2`
- Last updated: 2026-08-01

## ADR-0020 gate

- [ ] Discovery/config — bounded Swift/SwiftPM inventory exists and
  `Package.resolved` schema 2/3 pins are decoded without execution; manifest,
  target, and toolchain selection remain unresolved.
- [ ] Authoritative frontend — stage-3 qualification is incomplete.
- [ ] RepoGrammar-owned code units and IR.
- [ ] Typed `UNKNOWN` registry and provider fallback.
- [ ] `swift.xctest.test_method` exact family with support at least three.
- [ ] Positive, lookalike, low-support, parse-degraded, variant, and
  unresolved/resolved fixtures.
- [ ] Source-free readiness and leakage review.
- [x] Four-part review record — this report records correctness, security,
  completeness, and performance findings; open findings remain blockers.
- [ ] Linked atomic prerequisite commits and final completion audit.

## Current evidence and blocker

Discovery inventories `.swift`, `Package.swift`, `Package.resolved`,
`.swift-version`, and valid version-specific manifests. A bounded static reader
now admits unique-member schema-2/3 `Package.resolved` JSON, persists exact
semantic-version pins with `scope=unknown` and `directness=unknown`, and emits
typed `swift_dependency_inventory` uncertainty for malformed, unsupported,
conflicting, or over-budget input. It never retains package locations or
revisions and never invokes SwiftPM. This is dependency infrastructure, not a
Swift frontend or completion-gate pass.

The next permitted semantic stage is documentation/evidence-only qualification
of SwiftSyntax 603.0.2, Swift 6.3.3 compiler differential behavior, dependency
closure, five targets, and native OS sandboxes. Current arm64 macOS tools prove
only local availability, not the full qualification matrix.

Stage 3 may conclude `QUALIFIED`, `NO_GO`, `BLOCKED`, or `INCONCLUSIVE` but must
not add a production dependency, worker, parser, IR, UNKNOWN, or family. If exact
XCTest identity requires opening the target repository, evaluating
`Package.swift`, building/indexing modules, resolving dependencies, loading
macros/plugins, or ambient SDK state, the N1 semantic path is `NO_GO`.

## Completion verdict

Not complete. No completion percentage or supported-language count may include
Swift until every checkbox is linked to current-branch evidence.

## Final program audit fields

| Required field | Audited result |
|---|---|
| Language / rank | Swift / 15 |
| Dialect/version | No selected tools version, Swift language mode, target triple, SDK, deployment target, manifest profile, or macro set. |
| Provider/frontend/version | None integrated. SwiftSyntax 603.0.2 and Swift 6.3.3 are evidence-stage candidates only; no provider version is active. |
| Discovery/config | `.swift`, `Package.swift`, valid versioned manifests, `Package.resolved`, and `.swift-version` inventory. |
| Manifest/lockfile | Bounded schema-2/3 `Package.resolved` exact pins with unknown scope/directness; executable manifests are not evaluated. |
| Owned source IR | Absent; lock config units do not count as Swift source IR. |
| External symbols | Absent; module, SDK, XCTest, macro, and package symbol identity are unresolved. |
| Library Contracts | Registry infrastructure exists, production packs = 0, and lock identity alone cannot satisfy a contract anchor. |
| Exact-anchor family | Absent; proposed `swift.xctest.test_method` is not implemented. |
| Fixtures | Lock schema, duplicate, resource, leakage, incremental, and no-execution coverage; no admitted source/family matrix. |
| Primary UNKNOWN cases | Tools/language version, project root/manifest selection, package directness/authenticity, SDK/XCTest identity, macros/plugins, provider availability, and external symbols. |
| Source-free result | Pass for lock inventory; full Swift claim-bearing readiness is absent. |
| Completion state / counted | `discovered_only`; strict gate count `1/9`; Top-20 complete = no. |

## Four-part review

- Correctness: the lock reader reports only exact recorded fields and never
  turns a pin into directness, authenticity, availability, buildability, or
  runtime selection.
- Security: package manifests, SwiftPM, Xcode, compiler, macros, plugins,
  project code, children, and network are never executed. Toolchain sandbox and
  supply-chain qualification remain blockers.
- Completeness: source frontend/IR, selected project profile, provider/UNKNOWN
  closure, XCTest family, completion fixtures, and final audit are missing.
- Performance: static JSON ceilings are deterministic; no toolchain, SourceKit,
  SDK-scale, macro, or package-graph performance evidence exists.

Evidence paths are `src/rust/adapters/languages/swift.rs`,
`src/rust/adapters/parsing/swift.rs`, ADR-0025, product/incremental tests, and
prerequisite `9f1f657a27d45264d3b5e9b681750778234f0cb2`. Exact non-claim: a
`Package.resolved` pin does not establish a usable module, XCTest identity,
source semantics, API compatibility, or family.
