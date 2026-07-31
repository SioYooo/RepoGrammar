# Go language completion review

- Status: Incomplete — `discovered_only`
- Authority: ADR-0020 and ADR-0021
- Reviewed baseline: `86dba38ada7fe5646b5ab8770e2ad4183e8d22d7`
- Last updated: 2026-08-01

## ADR-0020 gate

- [ ] Discovery/config — bounded `.go`, `go.mod`, and `go.work` inventory exists,
  but frontend-owned build-constraint/generated/cgo obligations are open.
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

Discovery is deterministic and inventory-only. Default indexing does not read
Go source into a parser and emits no Go units, facts, UNKNOWNs, or families. The
current generic process adapter is not the sandbox required by ADR-0021. Before
frontend work, isolation must prove filesystem, network, descendant-process,
wall-time, CPU, memory, and output containment. If that cannot be enforced, the
provider remains unavailable.

The safe path may use supplied bytes with `go/parser`, `go/token`, bounded
`go/types`, and `go/build/constraint`. It must not run `go test`, `go build`,
`go generate`, cgo, `go/packages`, `go list`, or gopls. Tree-sitter alone cannot
support a family claim.

## Completion verdict

Not complete. No completion percentage or supported-language count may include
Go until every checkbox is linked to current-branch evidence.
