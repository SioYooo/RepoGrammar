# ADR-0041: Bounded Go test-declaration frontend

- Status: Accepted
- Date: 2026-08-15
- Scope: Go in ADR-0020; admits a family-bearing frontend over one declaration shape
- Refines: ADR-0021 (adds a route it did not consider; supersedes none of it)
- Related: ADR-0040, ADR-0019,
  `docs/reports/language-support/go-completion-review.md`,
  `docs/plans/multi-language-expansion-plan.md`

## Context

Go source is discovered and never read. Framework support for Go is therefore
not blocked on framework work at all — it is blocked on the absence of any
frontend, because no anchor of any kind is visible in bytes nobody decodes.

ADR-0021 examined this and produced a preflight, not a frontend. Its reasoning
is about *executing the Go toolchain*: `go/packages.Load` falls back to
`go list`, a repository can select its own `GOPACKAGESDRIVER`, and cgo
processing can be reached, so those calls are "process and environment
interaction, not an in-memory parse operation". It concluded that an OS sandbox
proving filesystem, network, descendant-process, wall-time, CPU, memory, and
output containment must land before a provider is admitted. It separately
declined to authorize a Tree-sitter Go grammar as a dependency.

Both conclusions stand. Neither reaches the route taken here.

Reference: the `testing` package documentation,
<https://pkg.go.dev/testing>, which specifies the test-function shape this ADR
admits and the `_test.go` filename rule that gives it meaning.

## Decision

### D1. A hand-written scanner is not the thing ADR-0021 gated

ADR-0021's sandbox requirement is bound to a premise: that reading Go requires
running Go. A bounded scanner written in the existing Rust core starts no
process, opens no network socket, spawns no descendant, consults no
`GOPACKAGESDRIVER`, and never touches cgo — there is nothing to contain, because
nothing runs. The containment obligation is not waived here; it is unreached.

The same distinction ADR-0040 drew for SQL applies: a premise that correctly
forbids one route does not forbid every route. ADR-0021 keeps full force over
any future `go/parser`, `go/types`, `go/packages`, or gopls integration, and
over the Tree-sitter grammar it declined. This ADR authorizes neither.

`docs/reports/language-support/go-completion-review.md` currently reads "Before
source frontend work, isolation must prove filesystem, network,
descendant-process, wall-time, CPU, memory, and output containment." That
sentence is about ADR-0021's worker; read literally it gates every frontend,
including one with nothing to isolate. The review is corrected in the same
commit that lands the scanner.

### D2. Admission is `*_test.go` only

Discovery keeps every `.go` path as inventory. Only files whose normalized
repository-relative name ends in `_test.go` are decoded and scanned. Every other
`.go` byte remains unread.

This is not merely a small blast radius. The filename is *part of the anchor's
meaning*: `go test` compiles only `_test.go` files as tests, so a function with
a test's exact signature in an ordinary file is not a test. Admitting the whole
language and then filtering inside the parser would make the frontend claim
authority over source it has no reason to read.

### D3. The exact anchor

One shape is admitted, and it is admitted whole or not at all:

```go
func TestXxx(t *testing.T)
```

Every condition is required. The declaration is at package level, not a method
and not nested. The name begins `Test` and the following character is not a
lowercase letter, so `Testify` is not a test while `TestFoo` and `Test_foo` are.
There is exactly one parameter and no result list. The parameter type is a
pointer to `T` in the package imported as `"testing"`, resolved through the
file's own import aliases: `import tt "testing"` makes `*tt.T` the anchor
spelling and `*testing.T` meaningless in that file.

Two import forms defeat resolution rather than bending it. A dot import
(`import . "testing"`) puts `T` in file scope with no qualifier, and a blank
import (`import _ "testing"`) binds no name at all. Neither yields a resolvable
parameter spelling, so a file containing one emits a typed `UNKNOWN` for the
import form and anchors nothing in that file.

`BenchmarkXxx(b *testing.B)`, `FuzzXxx(f *testing.F)`, `TestMain`, table-driven
subtests via `t.Run`, and helper functions are out of scope. They are follow-up
entries, not silent omissions.

### D4. The scanner is string- and comment-aware

A declaration is recognized only from source text that is actually source. Text
inside a raw or interpreted string literal, a line or block comment, or a build
directive line never contributes to an anchor.

This is stated as a decision because it is exactly the defect class this
repository has now hit twice: a Rust attribute matched inside a function body,
and a TS/JS runner matched inside a member call. A scanner that answers "does
this text appear" rather than "is this a declaration" produces anchors for prose.

### D5. What the anchor claims, and what stays UNKNOWN

The anchor claims that a file compiled by `go test` declares a function with the
shape the `testing` package requires of a test. It claims nothing else.

Build constraints are not evaluated, so a file excluded by `//go:build` on the
target platform still anchors. Generated code is not distinguished. The package
clause is recorded but no module, package graph, or import resolution is
performed, and no cross-file identity is established. Subtests, parallelism,
fixtures, helper registration, and skips are runtime behavior. Each is a typed
`UNKNOWN` or an explicit non-claim, never an inference.

### D6. No dependency, and no gate is granted by this ADR

No Rust crate, no grammar, no toolchain, no downloaded artifact. Go still has no
provider and no project model. This ADR authorizes a frontend for one
declaration shape; whether the resulting evidence satisfies any ADR-0020 gate is
decided by the completion review against the delivered code, not here.

## Alternatives considered

- Wait for the ADR-0021 sandbox: rejected because the sandbox gates a route this
  one does not take, and waiting leaves Go with no anchor of any kind
  indefinitely.
- Adopt a Tree-sitter Go grammar: rejected — ADR-0021 already declined the
  dependency, and the zero-external-dependency constraint excludes it.
- Scan all `.go` files and filter tests inside the parser: rejected under D2.
- Match `func Test` textually without resolving the parameter type: rejected
  because it would claim any function whose name starts with Test, which is the
  overclaim the exactness gate exists to prevent.

## Consequences

- Go moves from `discovered_only` toward a frontend, owned units, typed
  `UNKNOWN`s, and one exact family, for one declaration shape and no more.
- The security posture changes in one specific way: `_test.go` bytes now cross
  the source-store boundary. No process, network, toolchain, or module
  resolution is added.
- ADR-0021 remains in force for every route it describes.
- Widening the admitted shape, the admitted filename set, or the claim surface
  requires a superseding ADR.
