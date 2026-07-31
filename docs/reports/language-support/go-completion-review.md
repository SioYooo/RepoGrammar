# Go language completion review

- Status: Incomplete — `discovered_only`
- Authority: ADR-0020 and ADR-0021
- Integration branch baseline: `6a6f88538090ef386a8420c7424a380d97fa1f74`
- Last updated: 2026-08-01

## ADR-0020 gate

- [x] Discovery/config and dependency inventory — bounded `.go` discovery,
  root/nested `go.mod` `require` inventory, direct/indirect preservation,
  `go.work` abstention, typed malformed/conflict/resource/graph-changing
  UNKNOWNs, persistence, and incremental replacement exist without Go
  execution.
- [ ] Frontend-owned build-constraint/generated/cgo obligations.
- [ ] Authoritative frontend — blocked on a separately reviewed OS sandbox.
- [ ] RepoGrammar-owned code units and IR.
- [ ] Typed `UNKNOWN` registry and provider fallback.
- [ ] `go.testing.test_function` exact family with support at least three.
- [ ] Positive, lookalike, low-support, parse-degraded, variant, and
  unresolved/resolved fixtures.
- [ ] Source-free readiness and leakage review.
- [ ] Correctness, security, completeness, and performance review.
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
