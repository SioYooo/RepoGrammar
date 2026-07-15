# ADR-0021: Go standard-library semantic worker preflight

- Status: Accepted
- Date: 2026-07-15
- Scope: Go in ADR-0020 wave N1; decision-only preflight
- Refines: ADR-0004 and ADR-0020
- Related: `docs/plans/top-20-language-expansion-plan.md`,
  `docs/specifications/semantic-workers.md`,
  `docs/specifications/indexing-pipeline.md`,
  `docs/architecture/dependency-rules.md`, and
  `docs/reports/language-support/go-completion-review.md`

## Context

ADR-0020 requires an authoritative frontend, typed uncertainty, one exact-
anchor family, safe provider execution, source-free readiness, atomic
submodules, and a final completion audit before Go can be described as
supported. Go currently has none of those runtime modules in RepoGrammar. This
decision does not add them.

The earlier N1 candidate table named `go/packages` alongside `go/parser`.
That was too imprecise for an untrusted-repository default. The official
`go/packages` documentation exposes an underlying build-system driver, and its
default driver falls back to `go list`. The official source also shows that the
driver can encounter cgo processing. Calling `go/packages.Load`, `go list`,
`gopls`, or a repository-selected `GOPACKAGESDRIVER` is therefore process and
environment interaction, not an in-memory parse operation.

Go's standard library already provides narrower building blocks that accept
supplied inputs: `go/parser.ParseFile` can parse a string, byte slice, or reader;
`go/types.Config.Check` can type-check supplied AST files under an explicitly
controlled importer; and `go/build/constraint` can parse build-constraint
lines. These APIs permit a bounded frontend without asking the target
repository's build system to run.

## Decision

### D1. Go remains not started and unsupported

This ADR is a preflight authority only. It adds no `.go` discovery, language
token, parser, worker, provider, family, fixture, dependency, CLI/MCP behavior,
or readiness state. Go remains `not_started` for the Top-20 program.

A later implementation may use the maintained Tree-sitter Go grammar only as
a version-pinned universal syntax fallback and candidate generator. The exact
crate/release, checksum or lockfile identity, license review, Rust Tree-sitter
compatibility, malformed-input behavior, and corpus proof must land with the
frontend/IR submodule. This ADR does not authorize that dependency, and
Tree-sitter facts alone must not support a Go family.

### D2. The semantic path is an explicit, opt-in, sandboxed worker

The future authoritative path is a Go standard-library worker behind the
versioned semantic-worker boundary. Its first slice must use only explicitly
supplied, in-memory request data where the standard APIs permit:

- `go/parser` for Go syntax and partial-AST parse degradation;
- `go/token` for unadjusted, source-bounded byte ranges;
- `go/types` for candidate-scoped type facts only with a RepoGrammar-controlled
  importer and package universe;
- `go/build/constraint` for syntactic build-constraint facts; and
- RepoGrammar-owned request, fact, evidence, provenance, and `UNKNOWN` types at
  every Rust boundary.

Worker execution must be explicitly enabled by the operator. A release may
ship a pinned worker binary, but analysis must never use `go run` or compile the
worker from the target repository at request time. The executable and argument
vector must be explicit, with no shell interpolation. Missing consent, missing
sandbox capability, missing or incompatible worker version, timeout, crash,
oversized output, or protocol failure produces a typed provider-unavailable or
degraded-frontend `UNKNOWN`; it never falls through to a confident structural
claim.

The first sandbox contract is fail-closed:

- no network access;
- no target-repository, user-home, credential, Go cache, toolchain cache, or
  other host-file reads; the only read allowlist is the minimum worker/runtime
  artifacts fixed by the release and sandbox profile;
- no repository or host writes and no child-process creation;
- no target-repository current working directory;
- a minimal allowlisted environment that excludes `GOPACKAGESDRIVER`, `GOENV`,
  `GOFLAGS`, `GOMOD`, `GOWORK`, `GOPATH`, `GOPROXY`, and cgo/compiler variables;
- source arrives through the bounded protocol rather than worker filesystem
  traversal;
- the controlled importer is an in-memory RepoGrammar allowlist and must not
  fall back to the ambient filesystem, export-data/module caches, or Go command;
- stdout and stderr are drained concurrently, bounded, and sanitized; and
- if the host cannot enforce the declared process, filesystem, network, time,
  and memory limits, the semantic capability is unavailable rather than run
  unsandboxed.

The first test-function anchor does not require `go/types`: exact decoded
`"testing"` import identity plus the source-visible AST signature is sufficient
for that deliberately narrow contract. It must not synthesize a `testing.T`
package object or label structural evidence as semantic. `go/types` may be used
only by later facts that actually need it, after the controlled importer and
package universe are implemented and independently reviewed.

The worker must not execute or invoke target-repository code or tooling. In
particular, the default and first supported mode must not run:

- `go test`, `go build`, `go run`, `go generate`, or any other `go` command;
- `go list` or `go/packages.Load`;
- `gopls` or an alternate `GOPACKAGESDRIVER`;
- cgo, a C/C++ compiler, assembler, linker, or `pkg-config`; or
- generators, build scripts, test binaries, plugins, analyzers selected from
  the repository, or arbitrary subprocesses.

Any later trusted-repository mode that uses `go/packages`, `go list`, or gopls
requires a successor decision with separate consent, acquisition, environment,
network, module-cache, toolchain, cgo, subprocess, timeout, and provenance
gates. It must not silently replace this safe default.

### D3. First exact-anchor family is standard-library test functions

The first family candidate is a Go standard-library test-function declaration.
Each anchor must satisfy all of these source-visible conditions:

1. the repo-relative filename ends in `_test.go`;
2. the declaration is a top-level function, not a method and not a function
   literal;
3. the function has no type parameters, exactly one parameter by Go AST field-
   name semantics, and no result (`ast.Field.Names` counts declared parameters,
   so one unnamed field is one parameter and a field with multiple names is
   not);
4. the name is `TestXxx`, where `Xxx` is non-empty and its first Unicode
   character satisfies Go's exported-identifier uppercase-letter rule;
5. the parameter type is exactly a pointer selector `*alias.T`; and
6. `alias` is the default or explicit non-blank, non-dot import binding for the
   exact standard-library import path `"testing"` in that file.

The RepoGrammar target token is `go.testing.test_function`; it denotes the
top-level Go test-function contract with signature `func(*testing.T)`, not a
symbol named `testing.Test` (no such standard-library API exists). Default and
explicit aliases are normalized to the same import identity; dot imports,
blank imports, local `testing` lookalikes, methods, generic functions, variadic
parameters, extra parameters, results, non-pointer `testing.T`, and other
testing types do not anchor the first family.

The official `go test` rule accepts `TestXxx` when `Xxx` does not start with a
lowercase letter. This first slice deliberately uses the narrower exported-
identifier uppercase rule, so names beginning with digits, underscores, title-
case characters outside Unicode category `Lu`, or other non-lowercase
characters, plus the official empty-suffix name `Test`, are non-claims rather
than guessed positives.

A family still requires at least three fresh, same-language, exact anchors that
pass the existing complete-link compatibility and support gates, with no
claim-relevant blocking Go `UNKNOWN`. Three declarations are not sufficient if
they are configuration-incompatible, stale, conflicting, or from an
environment whose file selection is unresolved.

### D4. Evidence ladder and non-claims

The evidence order for the first slice is:

1. **Primary:** fresh output from the opted-in pinned standard-library worker,
   over supplied bytes, proving the declaration/import/signature anchor and
   carrying path/hash/range, worker/protocol/Go versions, request configuration,
   and sandbox profile.
2. **Claim-supporting derived fact:** a RepoGrammar-owned exact-anchor fact
   synthesized only after the primary fact matches the canonical test identity
   and the relevant `UNKNOWN` gates.
3. **Auxiliary:** version-pinned Tree-sitter Go syntax, extension/file-name
   discovery, unselected build-constraint expressions, `go.mod`/`go.work`
   inventory, and multi-configuration candidates. These may prioritize work or
   explain uncertainty but do not support a family by themselves.
4. **Forbidden for the claim:** extension-only recognition, text or regex
   matching, unpinned grammar output, `go/packages`/gopls output obtained without
   the separate trusted execution contract, executed test results, generated
   output, and structural similarity without exact import/signature identity.

The first slice must emit typed, claim-scoped `UNKNOWN` or abstain for:

- `//go:build` and legacy `+build` selection until the evaluation environment
  is explicit;
- GOOS/GOARCH filename suffix selection until GOOS, GOARCH, Go release/toolchain,
  compiler, cgo state, and any relevant build tags are explicit;
- generated files, even when they contain the conventional generated-code
  comment;
- `go.mod`/`go.work` module, workspace, replacement, vendor, and package-graph
  resolution;
- external dependency or non-standard-library type facts;
- interface, method-set, reflection, generic-instantiation, and dynamic dispatch;
- cgo and `import "C"` semantics;
- `//go:generate` directives and all generated artifacts; and
- stale, conflicting, parse-degraded, worker-unavailable, or insufficient-
  support evidence.

`go/build/constraint.Parse` is only an expression parser, not a file-selection
oracle. Constraint inventory must accept directives only in the valid header
before the package clause. Multiple `//go:build` lines, invalid expressions,
conflicting or unconfirmed legacy `+build` lines, and misplaced directives are
typed degraded/configuration `UNKNOWN`s. Filename selection must follow the Go
command's order: strip `.go`, then optional `_test`, then recognize only the
official `_GOOS`, `_GOARCH`, or `_GOOS_GOARCH` suffix forms. Selection remains
unresolved until the complete explicit configuration includes relevant GOOS,
GOARCH, compiler, cgo, release, architecture-feature, alias, `unix`, and custom
tag dimensions.

`go/parser` intentionally accepts a superset of valid Go syntax. Any parser or
candidate-scoped `go/types` error that can affect package, import, declaration,
name, type parameter, parameter, result, or signature identity blocks the
anchor. Import paths must be decoded as valid Go string literals before exact
comparison with `testing`; duplicate/conflicting aliases also block. Generated
markers must use `parser.ParseComments` plus the official `ast.IsGenerated`
rule, never a substring search.

The first implementation is stricter: any `go/parser.ParseFile` error blocks
every confident anchor in that file. It may retain source-free candidates and a
typed degraded-file `UNKNOWN`, but neither partial Go AST nor Tree-sitter output
may bypass the whole-file gate. Relaxing this requires a later proven error-
locality classifier and adversarial fixtures.

Every fact range must be derived from the request's `token.File` with
`token.File.Offset(pos)` and satisfy `0 <= start <= end <= len(source)`. If a
line/column is needed, the worker may use only unadjusted
`FileSet.PositionFor(pos, false)`. Adjusted `Position.Filename` or positions
affected by `//line` directives must never enter facts, diagnostics, logs, or
provenance; the repo-relative request path remains the sole path authority.

Inventory may record generated-file markers, module/workspace files, build
constraints, and source suffixes as bounded structural facts. It must not
pretend to evaluate them. Multi-configuration and multi-workspace facts remain
structural candidates unless an exact, explicit precedence and environment
resolution exists. An arbitrary chosen configuration must never erase the
other configurations' uncertainty.

Go file discovery and Go build eligibility are separate policies. Inventory may
retain bounded `.go` candidates, but files or directories beginning with `.` or
`_`, `vendor`/`testdata` paths, non-test files, platform-suffixed files without
a complete selected environment, and other Go-tool-excluded shapes cannot
support the first family.

Discovery-stage amendment (2026-07-16): while `go` and `go-config` are
inventory-only and absent from `ParserProjectContext`, their add/modify/remove
deltas may use incremental file-manifest persistence. The application must make
that decision from the authoritative discovery language token, perform zero Go
source-store/parser calls, purge any legacy or tampered claim-bearing records
for current inventory-only paths, and preserve only file metadata. This is not
an incremental semantic-context proof. The frontend/IR module must add Go
inputs to `ParserProjectContext` and restore project-context invalidation by
language token before it emits any Go unit, IR, fact, derived support, or family.
Future Go configuration inputs follow the same rule.

One authoritative Go obligation classifier must feed the existing cross-
language family-`UNKNOWN` classifier. `go_file_selection`,
`go_test_declaration_identity`, and `go_generated_origin` block the affected
test-function claim. Package buildability, module/workspace/vendor context,
external types, dispatch, cgo, and `go:generate` are separate subclaims and do
not block a source-local test declaration unless the authoritative classifier
records a direct impact. Foreign-provenance `UNKNOWN`s never block a Go family.
Callers must not rederive these decisions from raw assumptions.

### D5. Determinism, resource, and complexity budget

The first worker implementation must stay candidate-scoped and offline. Its
initial hard ceilings are:

- at most 256 files per request;
- at most 1 MiB per file and 8 MiB aggregate supplied source/config bytes;
- at most 10,000 emitted facts/unknowns and 1 MiB each of captured stdout and
  stderr;
- a five-second wall-clock deadline per worker request; and
- a 256 MiB process-memory ceiling plus no more than one worker request per
repository indexing operation, when the host sandbox can enforce it.

When stdout is the protocol channel, its one-mebibyte ceiling applies to the
entire response. Truncation or overflow invalidates the response; partial facts
must never be accepted.

The protocol must separately bound encoded wire bytes and decoded source bytes;
the eight-mebibyte decoded limit is not a JSON-wire limit. Rust must reject
wire/file/aggregate overflow before spawning the worker, and boundary tests must
cover 256/257 files plus one-byte-below/exact/one-byte-above per-file,
aggregate, response, and fact-count ceilings.

Exceeding a ceiling is a typed bounded-resource `UNKNOWN`, not partial confident
output. The frontend/IR implementation may lower these limits. Raising one
requires measured fixture evidence and an ADR/plan update before the change.

Discovery, lexical build-constraint collection, parsing, exact-anchor scanning,
normalization, and serialization must be linear in supplied bytes plus emitted
AST/fact size, apart from documented `go/types` behavior. The first test-family
slice must prefilter `_test.go` candidates and must not type-check a repository-
wide dependency graph. The worker must use stable sorted input and output,
content-addressed cache keys, deterministic identifiers, and no timestamps,
filesystem enumeration order, ambient environment, or network state in claim
facts. The cache and provenance identity must include the stable ordered
`(repo-relative path, content hash)` manifest, worker artifact digest (not only
a version string), protocol/parser mode, Go version, explicit configuration,
controlled-importer policy, sandbox profile, and operation. Identical normalized
inputs and those identities must produce byte-identical fact payloads.

### D6. Atomic module sequence and acceptance gates

Go is delivered through these independently coherent commits; no commit may
claim Go completion early:

1. **Preflight decision:** this ADR, the preflight review, and synchronized
   plan/index/roadmap/changelog/memory references. No code or dependency.
2. **Discovery/config:** safe `.go` and configuration inventory, exclusions,
   invalid/oversized/symlink cases, path-shape classification, deterministic
   source-free counts, inventory-only incremental deltas, claim-record purge
   tests, and docs. Source-backed marker extraction is outside this module.
3. **Frontend/IR:** the separately authorized and pinned Tree-sitter fallback,
   the opt-in sandboxed standard-library worker, RepoGrammar-owned code units/
   IR, parser-backed generated/build-constraint/cgo/`go:generate` markers,
   partial-parse behavior, leakage tests, protocol tests, restored Go
   project-context invalidation, and dependency/architecture documentation.
4. **`UNKNOWN`/provider:** one authoritative Go claim-obligation registry,
   unresolved/resolved fixture pairs, provider capability/fallback, freshness,
   conflict, environment, and cache/provenance tests.
5. **Family:** the exact `go.testing.test_function` family, positive,
   alias-positive, negative/lookalike, Unicode-name, low-support, parse-degraded,
   generated,
   build-variant, and multi-configuration fixtures; support of at least three
   exact compatible anchors; and exact non-claims.
6. **Product wiring:** status/doctor/stats/unknowns, CLI/MCP inventory and
   readiness, persistence/readback, synchronization, and tests rejecting
   source, absolute paths, raw diagnostics, ambient configuration, and high-
   cardinality leakage.
7. **Post-module review and completion audit:** review correctness/bugs,
   security, design/completeness, and performance; fix findings in scoped atomic
   commits; link every prerequisite SHA; run all required checks; and only then
   decide whether the evidence reaches `bounded_preview` or a stronger state.

Every module commit includes its tests and documentation. The completion audit
must satisfy all ADR-0020 D2 gates, the limits in this ADR, and the checked
matrix in `docs/reports/language-support/go-completion-review.md`. Until then,
Go must remain `not_started`, `discovered_only`, or `structural_substrate` as
the actual evidence warrants.

The current TypeScript process boundary does not enforce the filesystem,
network, descendant-process, CPU, or memory sandbox required here and must not
launch the Go worker. A separately reviewed OS sandbox capability is a hard
frontend/IR prerequisite. Unsupported platforms report provider unavailable;
they never run the worker with weaker isolation.

## Alternatives considered

- **Use `go/packages` by default:** rejected because its driver boundary can
  execute an external build-system driver and normally falls back to `go list`.
- **Use gopls by default:** rejected because it adds a toolchain/project-model
  process with module, environment, cache, and workspace effects not authorized
  for untrusted repositories.
- **Use Tree-sitter Go as the semantic oracle:** rejected because tolerant
  syntax and candidate generation cannot resolve build selection, imports,
  types, or dispatch.
- **Run `go test` to discover tests:** rejected because it compiles and executes
  repository tests and dependencies.
- **Treat every `Test*` declaration as a test family:** rejected because file,
  top-level declaration, exact import identity, signature, name, configuration,
  and support gates are all necessary.
- **Resolve one ambient GOOS/GOARCH configuration silently:** rejected because
  it makes output machine-dependent and hides other valid build variants.

## Consequences

- The N1 plan now has a safe Go frontend authority and no longer suggests
  implicit `go/packages` use.
- A future Go worker can use the standard library without importing Go AST or
  type-checker objects into the Rust core.
- Default indexing remains safe when Go tooling is absent or untrusted: it may
  eventually discover structural candidates, but confident Go families require
  the explicit worker and all gates.
- Build, module/workspace, generated-code, cgo, generator, dependency, and
  dispatch facts remain honest `UNKNOWN`s until a later scoped mechanism
  resolves the exact obligation.
- No production dependency or runtime behavior changes in this decision.

## Primary sources verified

The following live primary sources were checked on 2026-07-15:

- Go standard-library [`go/parser`](https://pkg.go.dev/go/parser) documentation,
  including supplied `ParseFile` input and partial AST behavior;
- Go standard-library [`go/token`](https://pkg.go.dev/go/token) documentation,
  including byte offsets and adjusted versus unadjusted positions;
- Go standard-library [`go/types`](https://pkg.go.dev/go/types) documentation;
- Go standard-library [`go/build/constraint`](https://pkg.go.dev/go/build/constraint)
  documentation;
- official [`go/packages`](https://pkg.go.dev/golang.org/x/tools/go/packages)
  documentation and the official x/tools
  [`defaultDriver`](https://github.com/golang/tools/blob/master/go/packages/packages.go)
  / [`go list` driver](https://github.com/golang/tools/blob/master/go/packages/golist.go)
  source;
- official [`go test` testing-function contract](https://pkg.go.dev/cmd/go#hdr-Testing_functions)
  and [`testing`](https://pkg.go.dev/testing) package documentation;
- Go specification [exported-identifier rule](https://go.dev/ref/spec#Exported_identifiers);
  and
- maintained [Tree-sitter Go grammar](https://github.com/tree-sitter/tree-sitter-go)
  project page.

These sources constrain the decision; they do not prove RepoGrammar has
implemented any Go support.

## Follow-up work

- Execute the atomic sequence in D6 on a dedicated Go major-feature branch.
- Before the frontend/IR commit, decide the exact Go toolchain/worker and
  Tree-sitter dependency pins, distribution checksums, licenses, and sandbox
  implementation against current platform capabilities.
- Keep the completion review incomplete until its evidence links and module
  SHAs are real.
- Write a successor ADR before enabling any `go/packages`, `go list`, gopls,
  cgo, module download, repository build/test/generate, or trusted-repository
  mode.
