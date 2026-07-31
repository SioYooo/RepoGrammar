# Go language completion review

- Status: Incomplete — `discovered_only`
- Authority: ADR-0020 and ADR-0021
- Dependency prerequisite: `abda602fe38db8549e4f318bd4638ddb94df7282`
- Last updated: 2026-08-01

## ADR-0020 gate

- [x] Discovery/config and dependency inventory — bounded `.go` discovery,
  root/nested `go.mod` `require` inventory, direct/indirect preservation,
  `go.work` abstention, typed malformed/conflict/resource/graph-changing
  UNKNOWNs, persistence, and incremental replacement exist without Go
  execution.
- [ ] Authoritative frontend and its build-constraint/generated/cgo obligations
  — blocked on a separately reviewed OS sandbox.
- [ ] RepoGrammar-owned code units and IR.
- [ ] Typed `UNKNOWN` registry and provider fallback.
- [ ] `go.testing.test_function` exact family with support at least three.
- [ ] Positive, lookalike, low-support, parse-degraded, variant, and
  unresolved/resolved fixtures.
- [ ] Source-free readiness and leakage review.
- [x] Four-part review record — this report records correctness, security,
  completeness, and performance findings; open findings remain blockers.
- [ ] Linked atomic prerequisite commits and final completion audit.

## Current evidence and blocker

Discovery remains deterministic and `.go` source remains inventory-only: default
indexing does not read or parse Go source and emits no Go-source unit, fact,
UNKNOWN, role, or family. A bounded in-process static project-model reader now
reads only discovered `go.mod`/`go.work` bytes. It records valid `require`
declarations as language-neutral `go_modules` dependencies at
`manifest_declared` evidence, preserves direct versus `// indirect`, and emits
claim-scoped typed UNKNOWNs for malformed/conflicting/resource-bounded or
module-graph-changing input. It does not read `go.sum`, resolve a graph, claim a
selected/installed version, or create language/library support.

The current generic process adapter is still not the sandbox required by
ADR-0021. Before source frontend work, isolation must prove filesystem, network,
descendant-process, wall-time, CPU, memory, and output containment. If that
cannot be enforced, the provider remains unavailable.

The safe path may use supplied bytes with `go/parser`, `go/token`, bounded
`go/types`, and `go/build/constraint`. It must not run `go test`, `go build`,
`go generate`, cgo, `go/packages`, `go list`, or gopls. Tree-sitter alone cannot
support a family claim.

## Completion verdict

Not complete. This dependency inventory is auxiliary project-model evidence, not
Go language support. No completion percentage or supported-language count may
include Go until every checkbox is linked to current-branch evidence.

## Final program audit fields

| Required field | Audited result |
|---|---|
| Language / rank | Go / 13 |
| Dialect/version | No selected Go release, GOOS, GOARCH, build tags, toolchain, generated-code, or cgo profile; only conservative lowercase `.go` and module metadata inventory. |
| Provider/frontend/version | None integrated. The proposed supplied-bytes Go standard-library frontend and OS sandbox remain unqualified; no provider version is claimed. |
| Discovery/config | `.go`, `go.mod`, and `go.work` with documented exclusions and bounded path/size handling. |
| Manifest/lockfile | Bounded `go.mod` direct/`// indirect` declaration inventory; `go.work` abstains; `go.sum` is not promoted to a lockfile. |
| Owned source IR | Absent; config-only project units do not count as Go source IR. |
| External symbols | Absent; imports, module graph, selected packages, and type identity are unresolved. |
| Library Contracts | Registry infrastructure exists at `4e4d0de`, but production contract packs = 0 and no Go package version/symbol can match one. |
| Exact-anchor family | Absent; proposed `go.testing.test_function` is not implemented. |
| Fixtures | Strong manifest/resource/incremental/no-execution coverage; no authoritative source-family positive/lookalike/degraded/resolved matrix. |
| Primary UNKNOWN cases | Workspace selection, graph-changing directives, malformed/conflicting requirements, build constraints, generated/cgo boundaries, target profile, provider availability, and external symbols. |
| Source-free result | Pass for inventory outputs and errors; no claim-bearing Go readiness surface exists. |
| Completion state / counted | `discovered_only`; strict gate count `2/9`; Top-20 complete = no. |

## Four-part review

- Correctness: module declarations are deterministic and directness is retained,
  but no selected module graph, import binding, type relation, or source meaning
  is inferred. `go.sum` and `go.work` deliberately cannot close those gaps.
- Security: supplied config bytes are bounded and no Go command, package,
  generator, cgo path, child process, or network action runs. The missing native
  sandbox is a hard blocker for any future frontend.
- Completeness: discovery and one manifest subset are real; source frontend,
  owned source IR, semantic obligations, exact family, full fixtures, provider,
  and final completion audit remain absent.
- Performance: byte, line, and record ceilings bound the static reader; no
  representative Go frontend, workspace, or large-module benchmark exists.

Evidence is rooted in `src/rust/adapters/languages/go.rs`,
`src/rust/adapters/parsing/go.rs`, `src/rust/application/indexing.rs`,
`src/rust/bin/repogrammar.rs`, ADR-0021, and prerequisite
`abda602fe38db8549e4f318bd4638ddb94df7282`. Exact non-claim: package presence
does not prove selection, installation, buildability, symbol identity, testing
semantics, or runtime behavior.
