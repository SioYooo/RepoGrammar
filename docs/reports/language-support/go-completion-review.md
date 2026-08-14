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
- [x] Authoritative frontend for the declared scope — ADR-0041's bounded
  in-process scanner reads `*_test.go` bytes only and recognizes one declaration
  shape. It is string- and comment-aware, so prose is never a declaration. It is
  not a Go parser, not `go/parser`, and not a type checker; the OS sandbox
  ADR-0021 requires is unreached rather than waived, because no process runs.
- [x] RepoGrammar-owned code units and IR — a module unit per admitted file, one
  `go_function` or `go_test_function` unit per package-level declaration, source
  ranges, content hashes, IR nodes, and containment edges.
- [x] Typed `UNKNOWN` registry — dot and blank testing imports block the test
  declaration claim they scope, because neither yields a resolvable signature; a
  build constraint rides along as a non-blocking subclaim, because the file's
  declarations are what they are whether or not the target platform compiles it;
  the scanner limit abstains. No provider exists and none is authorized.
- [x] `go.testing.test_function` exact family with support at least three — the
  role, the fixed `go.testing.T` target, the owned `repogrammar-go-derived`
  origin, and a minimum support of three rather than the shared default of two.
- [ ] Positive, lookalike, low-support, parse-degraded, variant, and
  unresolved/resolved fixtures — positive, lookalike, and low-support fixtures
  exercise the product CLI. The parse-degraded leg is open and is a real gap
  rather than a formality: this frontend is a scanner, so malformed Go does not
  fail, it simply yields fewer declarations, which is indistinguishable from a
  file that has fewer declarations. Nothing yet detects that difference.
- [ ] Source-free readiness and leakage review.
- [x] Four-part review record — this report records correctness, security,
  completeness, and performance findings; open findings remain blockers.
- [ ] Linked atomic prerequisite commits and final completion audit.

## Current evidence and blocker

Discovery remains deterministic and ordinary `.go` source remains inventory-only.
Under ADR-0041 exactly one filename class is admitted: `*_test.go` bytes reach a
bounded in-process scanner, and every other `.go` byte is still never decoded.
The filename is part of the anchor's meaning, since `go test` compiles only
those files as tests, so the same signature elsewhere is not a test. A bounded in-process static project-model reader now
reads only discovered `go.mod`/`go.work` bytes. It records valid `require`
declarations as language-neutral `go_modules` dependencies at
`manifest_declared` evidence, preserves direct versus `// indirect`, and emits
claim-scoped typed UNKNOWNs for malformed/conflicting/resource-bounded or
module-graph-changing input. It does not read `go.sum`, resolve a graph, claim a
selected/installed version, or create language/library support.

The current generic process adapter is still not the sandbox required by
ADR-0021. Before any Go **provider or worker** runs, isolation must prove
filesystem, network, descendant-process, wall-time, CPU, memory, and output
containment. If that cannot be enforced, the provider remains unavailable. That
obligation is scoped to routes that execute something: ADR-0041's scanner runs
in process, starts nothing, and therefore does not reach it. An earlier wording
here gated "source frontend work" generally, which read as a bar on frontends
with nothing to isolate.

A future deeper frontend may use supplied bytes with `go/parser`, `go/token`,
bounded `go/types`, and `go/build/constraint` once ADR-0021's sandbox is proven.
It must not run `go test`, `go build`, `go generate`, cgo, `go/packages`,
`go list`, or gopls. Tree-sitter alone cannot support a family claim. None of
that is required by the current scanner, whose whole claim is one declaration
shape.

## Completion verdict

Not complete. Go now has a bounded frontend over one filename class and one
declaration shape, owned units and IR, typed `UNKNOWN`s, and one exact family
with support three. It has no parse-degraded fixture, no audited source-free
readiness matrix, no final audit, and no provider. Strict gate count is `6/9`;
Go is `structural_substrate` and must not be counted as supported.

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
| Exact-anchor family | `go.testing.test_function` over the fixed `go.testing.T` target, minimum support three, owned derived origin required. Benchmarks, fuzz targets, `TestMain`, and `t.Run` subtests are named non-members. |
| Fixtures | Strong manifest/resource/incremental/no-execution coverage; no authoritative source-family positive/lookalike/degraded/resolved matrix. |
| Primary UNKNOWN cases | Workspace selection, graph-changing directives, malformed/conflicting requirements, build constraints, generated/cgo boundaries, target profile, provider availability, and external symbols. |
| Source-free result | Pass for inventory outputs and errors; no claim-bearing Go readiness surface exists. |
| Completion state / counted | `structural_substrate`; strict gate count `6/9`; Top-20 complete = no. |

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
