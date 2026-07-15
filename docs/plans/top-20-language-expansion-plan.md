# Top-20 Language Expansion Plan

- Status: Active implementation plan
- Last updated: 2026-07-16
- Scope: Execute ADR-0020 against the frozen TIOBE July 2026 Top-20 snapshot,
  with TypeScript tracked as an extra language.
- Authority: `docs/decisions/ADR-0020-top-20-language-expansion-gate.md`
- Related: `docs/decisions/ADR-0015-provider-backed-semantic-analysis-execution.md`,
  `docs/decisions/ADR-0019-bounded-multi-language-structural-expansion.md`,
  `docs/decisions/ADR-0021-go-standard-library-semantic-worker-preflight.md`,
  `docs/decisions/ADR-0022-ruby-prism-minitest-preflight.md`,
  `docs/decisions/ADR-0024-php-sandboxed-frontend-phpunit-preflight.md`,
  `docs/decisions/ADR-0025-swift-syntax-sourcekit-xctest-preflight.md`,
  `docs/plans/swift-n1-qualification-handoff.md`,
  `docs/plans/multi-language-expansion-plan.md`,
  `docs/plans/rust-tsjs-semantic-analysis-plan.md`,
  `docs/reports/unknown-resolution-sota-analysis.md`, and
  `docs/experiments/unknown-regression-benchmark.md`

If this plan conflicts with ADR-0020 or another accepted ADR, the ADR wins and
this plan must be updated before implementation continues.

## Goal and non-goal

Deliver one auditable, family-first, source-free-ready support slice for every
language in the frozen Top-20 list. Preserve TypeScript as a separate extra
lane. The plan coordinates work; it does not claim that an unimplemented
language, an extension-only path, or an existing structural preview has already
passed the completion gate.

The first commit for this plan is documentation and repository-governance
automation only. It changes no product runtime behavior and authorizes no new
production dependency.

## Set and wave invariant

Every language appears in exactly one row below. The current convergence lane,
TypeScript-extra lane, and new-language waves are disjoint.

| Lane | Languages | Count | Purpose |
|---|---|---:|---|
| C0 current Top-20 | Python, C, C++, Java, C#, JavaScript, Rust | 7 | Audit and converge existing paths against ADR-0020 D2 |
| X0 extra | TypeScript | 1 extra | Provider/`UNKNOWN` convergence; excluded from the Top-20 denominator |
| N1 native modern frontends | Go, PHP, Swift, Ruby | 4 | Establish first new-language frontend and family slices |
| N2 typed and legacy compiled | Visual Basic, Delphi/Object Pascal, Ada, Fortran | 4 | Pin dialect/version boundaries before frontend work |
| N3 data and technical languages | SQL, R, MATLAB | 3 | Use dialect-aware or language-native parsers and language-internal families where appropriate |
| N4 special representations | Assembly, Scratch | 2 | Pin dialect/architecture or archive-format scope and use language-internal families |

The counts are 7 current + 13 new = 20 ranked languages, plus one extra
TypeScript lane. Moving a language between waves requires updating this table
and the roadmap in the same commit; duplication is never allowed.

## Program pause checkpoint — 2026-07-16

The strict ADR-0020 terminal count is `0/20`: no ranked language yet has a
linked completion review with all nine D2 gates checked. The seven current
Top-20 paths remain convergence/audit work, and TypeScript remains a separately
reported extra lane.

Within the thirteen new-language lanes, Go, PHP, Swift, and Ruby are
`discovered_only`; the other nine lanes are not started. This `4/13` value is a
discovery-stage count only, not a support percentage. Swift preflight is commit
`d293238723c0b943d9665f05a4db948fba0f0e35`, and its reviewed discovery module
is commit `9bc1960db21e62216f2c9b85e88e32e9733390b0`. Development is intentionally
paused at that boundary. The exact next-session mission, evidence ladder,
failure taxonomy, resource policy, phases, artifacts, validation, commit
contract, and paste-ready `/goal` live in
`docs/plans/swift-n1-qualification-handoff.md`.

## Universal per-language gate

Use the ADR-0020 D2 nine-item gate as the acceptance checklist. A language
completion review must include the following checked matrix, with links to
source, tests, fixtures, specifications, and commit evidence:

| Gate | Minimum evidence |
|---|---|
| Discovery/config | Language and config tokens, deterministic discovery, exclusions, invalid/oversized/symlink cases |
| Frontend/parser | Pinned authoritative frontend or format parser, version/dialect, failure and parse-degraded behavior, no repository-code execution |
| Code units/IR | Stable units/ranges/hashes, RepoGrammar-owned IR and persistence/readback proof |
| Typed `UNKNOWN` | Claim registry, recoverable/irreducible split, provider fallback, stale/conflict/insufficient support handling |
| Family-first slice | One exact-anchor recurring family, support >= 3, compatibility profile, explicit non-claims; language-internal substitute when framework is not applicable |
| Fixtures | Positive, negative/lookalike, low-support, parse-degraded; plus dynamic/variant/stale/conflict and unresolved/resolved pairs where applicable |
| Source-free readiness | CLI/MCP/status/doctor/stats/unknowns inventory and leakage rejection |
| Review record | Correctness/bugs, security, implementation completeness, performance/resource bounds |
| Atomic delivery | One Conventional Commit per completed submodule with its tests/docs, followed by a completion-audit/review commit linking and verifying every prerequisite SHA |

Use `docs/reports/language-support/<language>-completion-review.md` for the
review record. A completion decision is invalid if it contains unchecked items,
unlinked evidence, or an unsupported claim that a structural grammar is a
semantic oracle.

## Lane C0 — existing Top-20 convergence

Existing code is reusable evidence, not a waiver. Audit each language against
the universal gate and close only the missing obligations.

| Language | Current starting point | Required convergence focus | Initial exact-anchor family |
|---|---|---|---|
| Python | CPython AST/`symtable`, exact framework previews, provider planning | Safe project model; Pyrefly/Pyright execution and typed fallback where needed; parse-degraded fixture and four-part review | Existing FastAPI/pytest/Pydantic/SQLAlchemy families; choose one audited slice |
| C | Tree-sitter C discovery/parser shared with C/C++ preview | Clang/clangd compilation-context frontend, preprocessor/build-variant obligations, C-specific code units and readiness rather than a C++ alias | Dialect/config-scoped C test or implementation pattern with exact include/symbol anchors |
| C++ | Tree-sitter C++ exact test-framework previews and config inventory | clangd/libclang frontend, translation-unit/header model, template/macro/variant `UNKNOWN` convergence | Existing include-gated GoogleTest/Catch2/doctest/Boost.Test family |
| Java | Tree-sitter Java Spring/test/JPA/JAX-RS previews | javac/JDT frontend, classpath/build-config scope, annotation-processor and runtime-framework boundaries | Existing exact-import JUnit or Spring family |
| C# | Tree-sitter C# ASP.NET/EF/test previews | Roslyn frontend, SDK/MSBuild declarative scope, partial/generator/dynamic obligations | Existing exact-using xUnit or ASP.NET Core family |
| JavaScript | TS/JS structural/project-context preview | TypeScript compiler JavaScript project mode, module/export binding, config and dynamic-runtime fallback | Existing exact-import Express or runner family |
| Rust | Tree-sitter Rust self-dogfood/general previews plus Cargo metadata stage | rust-analyzer/rustc-backed obligations, cfg/macro/build-script boundaries, provider isolation | Existing exact-use serde/tokio/axum or self-dogfood family |

Java C0 checkpoint (2026-07-16): the structural preview now has a bounded
same-class JUnit `@MethodSource` / TestNG `@DataProvider` linker with direct
repeatable scalar/array complete-set handling, strict non-shadowed explicit
imports or FQNs, and typed abstention outside exact unique source-visible
bindings. This reduces one
well-defined runtime-link `UNKNOWN` subset and adds tests, fixtures, and
structural replacement evidence, but it is only an intermediate Java submodule.
It does not provide javac/JDT semantics, classpath/build configuration,
inheritance, annotation processing, runtime test discovery, or the ADR-0020
completion review, so Java remains incomplete in C0.

For C and C++, completion is reported separately even when discovery or provider
infrastructure is shared. A shared implementation must prove language-specific
tokens, units, fixtures, UNKNOWN policy, readiness, and completion reviews.

## Lane X0 — TypeScript extra convergence

TypeScript remains a current extra language and never increments the Top-20
count. Converge the existing TS/JS substrate through a pinned TypeScript
`Program`/`TypeChecker` project model, bounded package/module/export resolution,
provider provenance, cache/failure behavior, and typed dynamic/config/runtime
`UNKNOWN`s. Audit one existing exact-anchor family end-to-end with positive,
negative, low-support, parse-degraded, stale, and unresolved/resolved fixtures.

JavaScript and TypeScript may share worker infrastructure, but they require
separate readiness/inventory counts and separate completion-review conclusions.

## Wave N1 — Go, PHP, Swift, Ruby

These are candidates for the first new-language implementation wave because
they have maintained parser/frontend ecosystems and common exact source-visible
test or route anchors. Candidate choices are research inputs, not dependency
authorization.

| Language | Frontend decision to pin | Candidate first family | Required early `UNKNOWN` boundary |
|---|---|---|---|
| Go | Version-pinned Tree-sitter syntax fallback plus an explicit, opt-in, sandboxed standard-library worker using supplied inputs with `go/parser`, `go/token`, candidate-scoped `go/types`, and `go/build/constraint`; no `go/packages`, `go list`, or gopls by default | `go.testing.test_function`: top-level `_test.go` `TestXxx(*testing.T)` with exact `"testing"` import identity/alias normalization and the conservative exported-name rule | Build constraints and GOOS/GOARCH suffix selection without an explicit environment; generated files; modules/workspaces; external types; interface/dynamic dispatch; cgo; `go:generate` |
| PHP | Candidate `mago-syntax` 1.43.0 only in a separately reviewed OS-sandboxed worker; isolated official PHP 8.5.8 `php -n -l` syntax-validity oracle; isolated `nikic/PHP-Parser` 5.8.0 AST/location differential and separately qualification-gated fallback; Tree-sitter PHP 0.24.2 syntax fallback only; bounded non-executing Composer JSON/lock and PHPUnit XML project inventory | `php.phpunit.test_method`: exact supported PHPUnit test-method declaration/attribute shape under the ADR's namespace, ancestry, version, and suite gates | Parse/frontend/profile degradation; namespace and PHPUnit identity; data providers/dependencies; traits; dynamic include/autoload; runtime mutation; generated source; suite selection; resource/protocol failure |
| Swift | Candidate SwiftSyntax 603.0.2 `SwiftParser` in a separately reviewed sandboxed worker; exact Swift 6.3.3 compiler differential; exact 6.3.3 SourceKit/sourcekitd path only as a separately qualified no-build semantic identity verifier; bounded non-executing static SwiftPM profile | `swift.xctest.test_method`: direct source-visible zero-parameter no-value-return instance method under exact immediate `XCTest.XCTestCase` identity in a selected test target | Syntax/profile degradation; package root/manifest/target selection; platform SDK and module identity; conditional compilation; attributes; macros/plugins; indirect ancestry; protocol dispatch; generated source; resource/protocol failure |
| Ruby | Candidate `ruby-prism` 1.9.0 native frontend only in a separately reviewed OS-sandboxed worker over supplied bytes; exact upstream commit plus separately checksummed package artifact authority, explicit CRuby 4.0 syntax profile, bounded source-free Bundler/project inventory, and no Ruby/Bundler execution | `ruby.minitest.test_method`: exact literal `require "minitest/autorun"`, direct `class X < Minitest::Test`, and direct zero-parameter `test_*` instance methods | Parse/profile degradation, require/constant identity, aliases/inheritance/reopenings, metaprogramming and runtime mutation, dynamic loads, alternate engines, generated sources |

Exit N1 language by language. Do not wait for the other three languages before
committing a complete slice.

### Go N1 preflight status

ADR-0021 accepts the Go architecture/security decision, and the bounded
discovery/config module now advances Go to `discovered_only`; Go remains
unsupported. Default indexing inventories `.go` as `go` and root/nested
`go.mod`/`go.work` as `go-config` without parser-facing source-store reads, parsing, units, facts,
IR, families, or readiness promotion. Its pure normalized-path classifier
records Go-tool exclusions, `_test.go`, and a dated Go 1.26.5 GOOS/GOARCH
suffix shape without selecting a configuration. Source marker scanning is
explicitly deferred rather than guessed from text. While these tokens remain
inventory-only and absent from `ParserProjectContext`, add/modify/delete deltas
stay incremental, count zero Go parser attempts, retain warnings from the whole
manifest, and purge claim-bearing records for Go paths. Frontend/IR must restore
token-based project-context invalidation when it adds cross-file Go semantics.

The future semantic path must be opt-in and sandboxed, consume supplied
source/config bytes where possible, and fail to a
typed `UNKNOWN` when consent, sandboxing, worker compatibility, or bounded
resources are unavailable. The safe default must not run repository code,
`go test`, `go build`, `go generate`, cgo, `go/packages`, `go list`, or gopls.
Tree-sitter Go remains a future version-pinned candidate layer, not a semantic
oracle and not an authorized dependency from the preflight alone.
The current process adapter is not the required Go sandbox. The next module is
frontend/IR, but implementation stops until a separately reviewed OS sandbox
proves filesystem, network, descendant-process, timeout, CPU, and memory
isolation. Whole-file parse-error blocking and one authoritative claim-impact
classifier remain required before any family promotion; the discovery path
classifier alone is not that claim-impact authority.

The Go module sequence is preflight; discovery/config; frontend/IR;
`UNKNOWN`/provider; family/fixtures; product wiring; and post-module review plus
completion audit. Each completed module must be an atomic Conventional Commit
with tests and documentation. The current unchecked evidence and four-part
post-decision risk review live in
`docs/reports/language-support/go-completion-review.md`.

The missing total repository file-count and aggregate-byte budget is a separate
cross-language P2 resource-hardening item. Discovery already enforces per-file
bounds; the aggregate budget must land in its own atomic module rather than be
silently folded into Go support or its completion percentage.

### PHP N1 preflight and discovery status

ADR-0024 accepts the PHP architecture/security decision, and the bounded
discovery/configuration module advances PHP to `discovered_only`; PHP remains
unsupported. Stable `php`/`php-config` tokens inventory exact case-sensitive
`.php` paths and exact root/nested `composer.json`, `composer.lock`,
`phpunit.xml`, and `phpunit.xml.dist` basenames. One pure normalized-path
classifier gives configuration precedence, applies PHP-only `.composer` and
`.phpunit.cache` exclusions with `language_specific_exclusion`, and does not
globally hide other languages; exact `vendor` remains globally excluded.

Indexing persists only bounded repo-relative path, raw-byte SHA-256, size, and
token before any source-store or parser dispatch. PHP-only generations are
`file_manifest_only`; mixed generations remain syntax-only; warnings are
path-free and emitted once per accepted token. PHP inventory deltas stay
incremental, and copy-forward purges legacy PHP claim records while retaining
metadata. No configuration is decoded or parsed, and no unit, IR, fact,
`UNKNOWN`, family, project model, readiness, PHP/Composer/PHPUnit execution, or
support behavior is added. Custom `vendor-dir` and PHPUnit cache-directory
selection remain unresolved until the bounded project-model stage.

`mago-syntax` 1.43.0 remains the production candidate only behind a separately
reviewed OS-sandboxed worker and the full dependency, artifact, malformed-input,
resource, five-target, and native-runtime gates.
Official PHP 8.5.8 `php -n -l` is the isolated syntax-validity oracle;
`nikic/PHP-Parser` 5.8.0 is the isolated AST/location differential and
separately qualification-gated fallback. Tree-sitter PHP 0.24.2 may generate
syntax candidates only.

The future exact first family is `php.phpunit.test_method`. Composer JSON/lock
and PHPUnit XML remain bounded data inputs only; the safe path must not execute
Composer, PHPUnit, autoloaders, plugins, scripts, repository PHP, or target
dependencies. The normative obligation registry, resource/protocol contract,
atomic module sequence, and all unchecked evidence live in ADR-0024 and
`docs/reports/language-support/php-completion-review.md`. No completion
percentage or supported-language count may include PHP before its final audit.

### Swift N1 preflight and discovery status

ADR-0025 accepts the Swift architecture/security preflight, and the bounded
discovery/configuration module advances Swift to `discovered_only`; Swift
remains unsupported and absent from every readiness/support count. Stable
`swift`/`swift-config` tokens inventory exact case-sensitive `.swift` paths and
exact root/nested `Package.swift`, `Package.resolved`, `.swift-version`, and
complete ASCII `Package@swift-M[.m[.p]].swift` basenames. One pure
normalized-path classifier gives configuration precedence and applies exact
Swift-only `.build`/`.swiftpm` exclusions without globally hiding other
languages. Invalid version-manifest lookalikes with an exact `.swift` suffix
remain ordinary Swift source inventory.

Indexing persists only bounded repo-relative path, raw-byte SHA-256, size, and
token before any source-store or parser dispatch. Swift-only generations are
`file_manifest_only`; mixed generations retain their parser-capable mode;
warnings are path-free and emitted once per accepted token. Swift inventory
deltas stay incremental, and copy-forward purges legacy Swift claim records.
No configuration is decoded or evaluated, and no dependency, toolchain,
worker, project model, parser, unit, IR, fact, typed `UNKNOWN`, family, or
readiness behavior is added.

The production syntax candidate is exact SwiftSyntax 603.0.2 `SwiftParser`
inside a separately reviewed OS-sandboxed worker, differentially qualified
against the exact Swift 6.3.3 compiler. Syntax is not the semantic oracle.
SourceKit-LSP/sourcekitd at the exact Swift 6.3.3 tag is only a separately
qualified semantic identity candidate; it must operate over synthesized
supplied inputs without opening the repository, evaluating `Package.swift`,
building/indexing modules, resolving dependencies, loading macros/plugins, or
using ambient toolchain state. If exact XCTest identity requires those actions,
the N1 semantic path is `NO_GO`.

The next permitted module is documentation/evidence-only artifact, compiler-
differential, dependency, supply-chain, five-target, and native sandbox
qualification. `.swift-version` remains uninterpreted toolchain-selector
metadata, not package dialect or build evidence.
`docs/plans/swift-n1-qualification-handoff.md` is the execution authority for
that future stage. It permits a source-backed `QUALIFIED`, `NO_GO`, `BLOCKED`,
or `INCONCLUSIVE` result and explicitly forbids production admission in the
same commit.

The future first family is `swift.xctest.test_method`. It requires a selected
static SwiftPM test target, clean exact syntax, exact platform XCTest module
and immediate-superclass identity, and a direct source-visible instance method
with the narrow XCTest signature. It does not claim build, runtime selection,
execution, or pass/fail. Swift Testing is deferred because `@Test` is a compiler
macro. The obligation registry, resource/protocol contract, eleven-stage atomic
sequence, and all unchecked evidence live in ADR-0025 and
`docs/reports/language-support/swift-completion-review.md`.

### Ruby N1 preflight and discovery status

ADR-0022 accepts the Ruby decision preflight, and the bounded discovery/config
module advances Ruby to `discovered_only`; Ruby remains unsupported. Stable
`ruby`/`ruby-config` tokens inventory exact `.rb` source and root/nested
`Gemfile`, `Gemfile.lock`, `gems.rb`, `gems.locked`, `.ruby-version`, and
`*.gemspec` paths. One pure normalized-path classifier gives configuration
precedence, uses `language_specific_exclusion` for Ruby candidates below exact
`.bundle`/`.ruby-lsp` components, and does not globally hide other languages.
Indexing stores bounded path/hash/size/token metadata with no parser-facing
source read, unit, IR, fact, `UNKNOWN`, family, or readiness promotion. Ruby-only
generations are `file_manifest_only`; mixed generations remain syntax-only;
inventory deltas stay incremental with one path-free warning per token and
claim-record purge. Autosync preserves its generic Git-independent fingerprint
behavior. This path does not evaluate project files or invoke Ruby, Bundler,
RubyGems, Rake, Rails, tests, generators, child processes, or network access.

The current frontend candidate is `ruby-prism` 1.9.0 using the exact upstream
commit linked by the release plus the separately checksummed package artifact,
not mutable hosted rustdoc. It is a C99/FFI native dependency and is not
authorized by the preflight. A documentation/evidence-only dependency and
sandbox qualification must pass before production dependency/artifact
admission; frontend/IR follows only through that accepted worker boundary. The
first exact profile accepts only a sole repository-root `.ruby-version` with
exact `4.0.6` plus optional LF; all other version scopes/shapes must become
`UNKNOWN` after the registry lands, and semantic capability is unavailable
before then. The worker receives only bounded `.rb` bytes plus normalized
validated profile metadata, never executable config bytes. The first exact
family, authoritative future `UNKNOWN` obligations, proposed ceilings, ten
atomic stages, and incomplete semantic four-part review are normative in
ADR-0022 and `docs/reports/language-support/ruby-completion-review.md`.

## Wave N2 — Visual Basic, Delphi/Object Pascal, Ada, Fortran

Dialect/version scope is a mandatory preflight gate for this wave.

| Language | Frontend decision to pin | Candidate first family | Scope/non-claim requirement |
|---|---|---|---|
| Visual Basic | Decide VB.NET via Roslyn Visual Basic versus any separately scoped classic VB format before token design | MSTest/NUnit/xUnit-compatible test declarations for VB.NET | VB.NET support must not imply VB6/classic Visual Basic support |
| Delphi/Object Pascal | Version-profiled Delphi/Object Pascal parser; evaluate Free Pascal tooling with compatibility limits | DUnit/DUnitX test declarations | One accepted dialect profile must not imply every Delphi/FPC extension |
| Ada | Libadalang/GNAT frontend with project-file inventory | AUnit tests or package/body implementation pairs | Generic instantiation, representation clauses, build configuration and generated code stay scoped |
| Fortran | Version-pinned flang/LFortran/fparser-class frontend | pFUnit tests or module/procedure families | Fixed/free form, preprocessing, modules and compiler extensions must be explicit |

## Wave N3 — SQL, R, MATLAB

Framework-family language may be not applicable only when the implementation
substitutes an exact language-internal recurring-pattern family.

| Language | Frontend/format decision to pin | Candidate first family | Scope/non-claim requirement |
|---|---|---|---|
| SQL | One initial SQL dialect and its authoritative/maintained dialect parser; migration/config discovery | DDL migration/table definitions or compatible query-shape families | One dialect never implies universal SQL; dynamic SQL and stored-language bodies remain typed |
| R | R-native parser/frontend and package/project metadata | `testthat::test_that` tests or exact Shiny declarations | Non-standard evaluation, formula semantics, dynamic package loading and native extensions remain typed |
| MATLAB | Version-profiled MATLAB parser/frontend and project/package metadata | `matlab.unittest` tests or function/class method families | Dynamic workspace/eval, path mutation, toolboxes, code generation and Simulink are separate claims |

## Wave N4 — Assembly, Scratch

These languages require explicit representation boundaries before discovery is
counted as support.

| Language | Frontend/format decision to pin | Candidate language-internal family | Security-critical boundary |
|---|---|---|---|
| Assembly | One architecture, object format, syntax dialect and parser frontend (for example an LLVM MC or GNU-syntax parsing boundary) | Exact directive/label/call-anchored procedure families | Never assemble or execute input; directives, macros, includes, self-modifying/runtime control flow remain typed |
| Scratch | `.sb3` ZIP/container and `project.json` schema/version parser | Exact event-hat plus opcode-stack script families | Bound archive size/count/ratio/depth, reject path traversal and malformed JSON, never execute projects/extensions |

Completion for Assembly or Scratch is bounded to the declared dialect/format
version. README wording must include that qualifier.

## Per-language implementation sequence

1. **Preflight decision:** create/update the language completion review with
   dialect/version, frontend/provider candidate, dependency/acquisition policy,
   exact-anchor family, `UNKNOWN` obligations, threat model, and performance
   limits. No production dependency lands before architecture/decision sync.
   Land a decision-only commit when this establishes new durable authority.
2. **Discovery/config slice:** tokens, safe discovery, config inventory, skips,
   source-free counts, deterministic tests, and the corresponding documentation
   record; land an atomic Conventional Commit for this completed submodule.
3. **Frontend and IR slice:** adapt the authoritative frontend or format parser
   behind ports; normalize owned code units/IR; preserve parse degradation and
   provider unavailability as typed outcomes. Land the implementation, tests,
   dependency/architecture update, and review notes atomically.
4. **UNKNOWN/provider slice:** register claim-scoped obligations and mechanisms;
   add unresolved/resolved pairs without deleting irreducible uncertainty. Land
   provider behavior, fallback tests, and documentation as one atomic commit;
   split provider acquisition from behavior only when each commit is coherent.
5. **Family slice:** add one exact-anchor family, support and compatibility
   gates, positive/negative/low-support/parse-degraded fixtures, family/readiness
   tests, and exact non-claims in one atomic family/fixtures commit.
6. **Review:** audit bugs/correctness, security, implementation completeness,
   and performance/resource bounds; resolve findings or record them as risks and
   `UNKNOWN`s. Correct findings in scoped atomic commits with matching
   regression tests and documentation.
7. **Completion audit/review and documentation:** update the final completion
   review, support matrix, specifications, roadmap, module map, testing policy,
   CHANGELOG, active plan, and durable memory. The atomic completion-audit
   Conventional Commit must link every prerequisite SHA, verify each universal
   gate, and only then advance the language's support state.
8. **Validation and delivery discipline:** before every submodule commit, run
   the targeted tests and relevant quality gates, explicitly stage paths, and
   inspect the staged diff. Before the completion-audit commit, run format,
   clippy, all workspace tests, repo guard, diff check, mirror comparison,
   targeted leakage checks, and provider-specific deterministic tests. Do not
   push unless explicitly authorized.

Intermediate submodule commits are required for completed module boundaries;
they must be independently coherent and must not claim language completion.
The final completion audit/review commit closes the language only after linking
and verifying their exact SHAs. A single mega commit for the whole language
does not satisfy this plan.

## Parallel ownership and integration

- Use one dedicated major-feature branch per language.
- Assign exclusive ownership for the language adapter, fixtures, review record,
  and language-specific docs while work is active.
- Integrate shared language enums, provider registries, readiness tables,
  persistence whitelists, and test dispatch sequentially; do not let parallel
  agents edit the same registry concurrently.
- Preserve one authoritative classifier for family-affecting `UNKNOWN`s and one
  authoritative support-target registry per language.
- Review changed logic and completion evidence before merging agent output.
- A blocked language does not block independent languages in the same wave. It
  remains explicitly incomplete with its blocker and next recovery mechanism.

## Required validation

Every language completion runs, at minimum:

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo run --quiet --bin repo-guard -- check
git diff --check
cmp -s AGENTS.md CLAUDE.md
```

Provider tests remain deterministic, local, bounded, and offline by default.
Network acquisition, system-wide installation, or repository build/runtime
execution is not implied by this plan.

## Program reporting

The roadmap may report each language only as `not_started`, `discovered_only`,
`structural_substrate`, `bounded_preview`, or a stronger provider-backed state.
Only a linked completion review with all ADR-0020 D2 evidence may advance a
language to `bounded_preview` or stronger for this program.

At the end of each wave, publish a source-free summary of completed languages,
provider availability, recoverable and irreducible `UNKNOWN` mechanisms,
validation results, commit SHAs, and remaining risks. Keep the seven current
Top-20 languages, TypeScript extra, and thirteen additions in separate totals.
