# Top-20 language and third-party-library final program audit

- Date: 2026-08-01; continuation audited 2026-08-14 and 2026-08-15
- Branch: `feat/top-20-language-library-support`
- Protected baseline: `86dba38ada7fe5646b5ab8770e2ad4183e8d22d7`
- Verdict: `PARTIAL_AUDITED_PROGRESS`
- Strict Top-20 completion: **0/20**
- TypeScript extra completion: **incomplete; excluded from the denominator**
- Third-party-library platform completion: **incomplete**

The 2026-08-14 continuation ran under an added hard constraint: no new Rust
crate and no downloaded or bundled external artifact. That excludes every
provider route the earlier rounds had left as follow-up work -- Pyrefly and
Pyright, clang, Roslyn, rust-analyzer, a Go worker binary, Prism, SwiftSyntax,
libadalang, flang, LLVM MC, a MATLAB runtime, any Tree-sitter grammar, and
`sqlparser-rs`. What remained admissible was a hand-written bounded frontend in
the existing Rust core, and deepening the Python and TypeScript workers that
already ship with the package. Every per-language conclusion below is now
partitioned by whether its binding gate needs one of the excluded artifacts.

This audit closes the bounded five-round implementation campaign without
claiming that the original all-language objective was achieved. The branch
contains useful discovery, structural, dependency-inventory, persistence, and
review infrastructure, but no language satisfies all nine ADR-0020 completion
gates. A package manifest, lockfile, parser, lexical candidate, or reviewed
contract lookup is never sufficient by itself to prove language or library
behavior support.

## Mission and isolation result

The work was performed only on `feat/top-20-language-library-support` from the
protected baseline above. The campaign did not merge, rebase, cherry-pick, or
otherwise apply this work to `main`; it did not push, create a pull request,
tag, release, or publish. The saved overnight goal prompt remains an intentional
untracked operator artifact at
`docs/prompts/top-20-language-library-support-overnight-goal.md` and is excluded
from every commit.

The accepted terminal label is partial audited progress. `COMPLETE`,
`20/20_SUPPORTED`, `ALL_LIBRARIES_SUPPORTED`, and equivalent claims are false.

## Round and workstream result

| Round/workstream | Delivered evidence | Decision |
|---|---|---|
| 0 — repository and branch preflight | protected baseline, dirty-state audit, authority/skill review, dedicated branch | isolation gate passed |
| 1 — shared dependency foundation | language-neutral evidence ladder, schema-v13 persistence, npm, Python, C/C++, SwiftPM, Composer, Bundler, Go Modules, and Maven bounded inventory | useful infrastructure; no semantic-support promotion |
| 2 — N2 compiled inventory | VB.NET/NuGet, Delphi package, Ada/Alire, and Fortran/fpm discovery/configuration slices | `discovered_only`; providers and families remain open |
| 3 — data/scientific inventory | SQL metadata plus R CRAN/Bioconductor/renv inventory | `discovered_only` at the time; SQL was later advanced by ADR-0040, R still open |
| 4 — shared contract and convergence audit | exact-version reviewed-contract registry with overlap rejection; core and compiled language four-part reviews | registry has zero production packs; all reviewed languages remain incomplete |
| 5 — tail-language bounded slice | MATLAB add-on inventory, Assembly lexical candidate substrate, disconnected Scratch archive prerequisite | MATLAB incomplete; Assembly incomplete; Scratch product integration `NO_GO` |
| Final consolidation | 21 completion reviews, machine-readable summary, ecosystem/provider/manifest/UNKNOWN matrices, source-free security/performance review | `PARTIAL_AUDITED_PROGRESS` |
| 2026-08-14 R1 — Python provider abstention | one typed decision for absent/stale/conflicting provider answers, with the fixture matrix | contract only; no provider executes, Python stays 4/9 |
| 2026-08-14 R2 — SQL frontend | ADR-0040 dialect-invariance decision, bounded DDL frontend, owned units/IR, typed `UNKNOWN`s | SQL gates 2, 3, 4 close; 5/9 |
| 2026-08-14 R3 — SQL family | role registry, owned derived support, `sql.schema.table_definition`, adversarial fixture corpus | SQL gates 5, 6 close; 7/9 |
| 2026-08-14 R4 — blocker partition | per-language `blocker_class`/`blocker` in the summary JSON, stale-document sweep | every lane has a source-backed conclusion |
| 2026-08-14 R5 — SQL readiness | source-free/leakage audit across `status`, `doctor`, `stats`, `unknowns`, `families`, `files`, and MCP; corrected the dialect recovery mechanism | SQL gate 7 closes; 8/9, gate 9 left to a maintainer ruling |
| 2026-09-04 R6 — MATLAB Gate 1 selection | deterministic `.m`/`mpackage.json` selection with MATLAB-only `codegen`/`slprj`/`sccprj` generated-output exclusions, ADR-0046 D4c dialect abstention re-verified, and lane symlink/invalid-bytes discovery tests | MATLAB gate 1 closes; 8/9, gate 9 left to the completion audit |
| 2026-09-04 R7 — R obligation registry | `R_OBLIGATION_REGISTRY` in the testthat frontend declares every claim-scoped typed `UNKNOWN` (identity blocking obligations, conditional reach, description literal, parse bounds, standing callee-binding and block-NSE residuals, triggered S3/S4 dispatch and native/package-symbol unknowns) with recorded provider fallbacks; all emission routes through the table and registry/determinism/count tests pin it | R gate 4 closes; 8/9, gate 9 (final completion audit with linked prerequisite SHAs) remains |

## Strict language matrix

“Gates” is the number of ADR-0020 gates currently evidenced out of nine. It is
not a percentage or support score.

“Blocked by” partitions the remaining work under the zero-external-dependency
constraint. `zero_dependency_excluded` means the binding gate needs an artifact
the constraint forbids, named in the JSON blocker text.
`open_under_zero_dependency` means no artifact is required and the remaining
work is a separate multi-module effort this program did not attempt. `no_go`
means a recorded ADR decision closed the lane.

| Rank | Language | Program state | Gates | Blocked by | Third-party-library boundary | Complete |
|---:|---|---|---:|---|---|---|
| 1 | Python | `structural_substrate` | 4/9 | `zero_dependency_excluded` | bounded PyPI manifest declarations only | no |
| 2 | C | `structural_substrate` | 2/9 | `zero_dependency_excluded` | shared vcpkg/Conan manifest declarations only | no |
| 3 | C++ | `structural_substrate` | 3/9 | `zero_dependency_excluded` | vcpkg/Conan manifest declarations only | no |
| 4 | Java | `structural_substrate` | 3/9 | `zero_dependency_excluded` | direct literal Maven declarations only | no |
| 5 | C# | `structural_substrate` | 3/9 | `zero_dependency_excluded` | bounded `.csproj` NuGet reader landed at the parser layer; discovery admission pending | no |
| 6 | JavaScript | `structural_substrate` | 3/9 | `open_under_zero_dependency` | root npm manifest declarations only | no |
| 7 | Visual Basic .NET | `structural_substrate` | 8/9 | `zero_dependency_excluded` | literal `.vbproj` NuGet declarations only | no |
| 8 | SQL | `structural_substrate` | 8/9 | `open_under_zero_dependency` | no SQL extension dependency consumer | no |
| 9 | R | `structural_substrate` | 8/9 | `open_under_zero_dependency` | bounded CRAN/Bioconductor manifest/renv evidence | no |
| 10 | Rust | `structural_substrate` | 3/9 | `zero_dependency_excluded` | Cargo manifest/project-model declarations only | no |
| 11 | Delphi/Object Pascal | `structural_substrate` | 8/9 | `open_under_zero_dependency` | literal Delphi package declarations only | no |
| 12 | Scratch | `not_started`; prerequisite `NO_GO` | 1/9 | `no_go` | no product dependency inventory | no |
| 13 | Go | `discovered_only` | 2/9 | `open_under_zero_dependency` | bounded `go.mod` declarations only | no |
| 14 | PHP | `discovered_only` | 1/9 | `open_under_zero_dependency` | Composer manifest plus lock rows, without coherence | no |
| 15 | Swift | `discovered_only` | 1/9 | `open_under_zero_dependency` | SwiftPM schema-2/3 lock pins only | no |
| 16 | Ada | `structural_substrate` | 8/9 | `open_under_zero_dependency` | unconditional literal Alire declarations only | no |
| 17 | Assembly | `structural_substrate` | 1/9 | `zero_dependency_excluded` | no native/system dependency consumer | no |
| 18 | MATLAB | `structural_substrate` | 8/9 | `open_under_zero_dependency` | bounded R2024b+ add-on manifest declarations only | no |
| 19 | Fortran | `discovered_only` | 2/9 | `open_under_zero_dependency` | literal root fpm declarations only | no |
| 20 | Ruby | `discovered_only` | 2/9 | `open_under_zero_dependency` | direct `Gemfile.lock` declaration inventory only | no |
| extra | TypeScript | `structural_substrate` | 3/9 | `open_under_zero_dependency` | root npm manifest declarations only | no |

The machine-readable authority for these rows is
`top-20-program-summary.json`. Each row links its own completion review,
evidence paths, exact prerequisite commits, provider state, library-analysis
state, and highest-impact `UNKNOWN` mechanisms.

## Third-party-library platform audit

The cross-language foundation now has:

- a closed 20-token `DependencyEcosystem` vocabulary;
- bounded package identities and three-state directness;
- an evidence ladder of `manifest_declared`, `lockfile_resolved`, and
  `provider_resolved` without promoting lower evidence;
- schema-v14 generation persistence and active readback;
- package-qualified external-symbol and reviewed-contract domain types; and
- an exact-version contract registry that rejects duplicate ids, overlapping
  package/version/capability claims, versionless queries, and manifest-only
  lookup.

The platform is still incomplete:

- only 17 of 20 ecosystem tokens have any generic dependency consumer;
- `sql_extension`, `scratch_extension`, and `native_system` have none;
- no language has a complete package-manager/effective-project graph;
- no language has a complete package-qualified external-symbol resolver;
- the production reviewed-contract pack count is zero;
- a contract match would still require exact source anchors, a qualified
  external symbol, compatible exact version, freshness, and no blocking
  `UNKNOWN`; and
- public source-free dependency projection is not complete.

The authoritative detail is split across:

- `third-party-ecosystem-coverage-matrix.md`;
- `dependency-manifest-lockfile-matrix.md`;
- `provider-tool-version-license-matrix.md`;
- `unknown-resolution-matrix.md`; and
- `source-free-security-performance-review.md`.

## Exact branch commit ledger

These are the campaign commits between the protected baseline and the
pre-audit branch head. Integration commits are retained because the independent
agent workstreams were reviewed and integrated without rewriting history.

| Commit | Purpose |
|---|---|
| `9ea396b96ae11eb79ec9f80dd52b66e631c7dc1a` | dependency evidence model |
| `5ef354249b58784b1e872aa6ac3d83c7502dd4c8` | dependency persistence |
| `f516aefb014efe1bc780be73a4ff0a11fb745b84` | npm inventory |
| `afc472bc2bf23abe63c24e1a501f1bc354b5b24e` | Python dependency inventory |
| `0472e6bf0fa74a37a0bb9e43eb878f2c04185c22` | C/C++ dependency inventory |
| `9f1f657a27d45264d3b5e9b681750778234f0cb2` | SwiftPM lock inventory |
| `f27812bf79c1eb3c5eca8179b17285794b344491` | Composer inventory |
| `6a6f88538090ef386a8420c7424a380d97fa1f74` | Bundler inventory |
| `abda602fe38db8549e4f318bd4638ddb94df7282` | Go Modules inventory |
| `2b5fd6d1bf32e8805798e26e53ff9f090ec22d3e` | VB.NET and Delphi inventory |
| `9d3a0ba7afb21f46c716f496a71f34cc7cfc939b` | Maven inventory |
| `5e8fda053122fb0cfd093b28767b9479bd7ddc80` | Ada and Fortran inventory |
| `82ced893f81a954b64e20546dfc4ad81043b28e5` | SQL and R metadata inventory |
| `72248cd1ab11eab6c213d66f834e1bd62410bdde` | VB.NET/Delphi integration |
| `fd85c23595333dcb1b0727843f09a2d1fcc638bc` | Ada/Fortran integration |
| `8a4033e3eddc93787c997d13c54bba22b1df7102` | SQL/R integration |
| `4e4d0de6d6866065818d955f4719e247b745089a` | exact-version reviewed-contract registry |
| `a994f5b7eb46d4b4a22c8e5ce5656495400fee06` | MATLAB, Assembly, and Scratch prerequisite slice |
| `b175bc7b7b2341cd5521498657e0d2edce664901` | core-language completion reviews |
| `7ffa93df75983de86780a2750d3560ff2dd56869` | C/C++/Java/C# completion reviews |
| `cdfb76061aecae56b74eb0958b345b13a3539c2a` | tail-language integration |
| `617be0480c785abab8bfef2066a704dafd01b43c` | core-review integration |
| `4cc8cc0a27e8fde61a01d506066abdc300ca7d1c` | compiled-review integration |

Language-specific reviews also link older prerequisite SHAs already contained
in the baseline when they are necessary to interpret existing structural paths.
Every SHA in the ledger above was checked as an ancestor of the branch head at
the 2026-08-01 audit. That check has not been rerun for the 2026-08-14
continuation commits, which are recorded in the delivery summary of the session
that produced them rather than here, because a commit cannot contain its own
SHA.

## Correctness, security, completeness, and performance decision

- Correctness: evidence levels, directness, provider status, and contract
  status remain separate. The status/doctor JSON count that used to be named
  `dependency_records` — while counting `derived_record_dependencies` — is now
  named `derived_record_dependencies`. The authoritative active dependency read
  model still has no public projection; that remains an open follow-up rather
  than a misnamed surface.
- Security: new manifest/config/archive readers are bounded and non-executing.
  No package manager, build system, compiler, plugin, generator, repository
  code, dependency code, child process, network resolver, database, MATLAB,
  Swift, Go, Ruby, PHP, or Scratch VM is invoked by the new inventory lanes.
- Completeness: all 20 ranked languages plus TypeScript extra have explicit
  completion reviews, but none passes nine gates and arbitrary third-party
  library behavior remains unsupported.
- Performance: deterministic file-local parsing, aggregate limits, exact/+1
  tests, and bounded registries exist. Representative provider latency/memory,
  large dependency graph, deflate, catalog, and production-corpus benchmarks
  do not; those remain completion blockers.

## Final validation gate

The following commands passed on the integration branch before the completion-
audit commit. The final commit SHA is reported in the delivery summary because
a commit cannot truthfully contain its own SHA; `check-diff` is rerun against
that committed head.

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features --quiet
cargo run --quiet --bin repo-guard -- check
cargo run --quiet --bin repo-guard -- check-diff --base origin/main --head HEAD
git diff --check
cmp -s AGENTS.md CLAUDE.md
node --test src/workers/typescript/worker.test.js
python3 src/workers/python/worker.test.py
jq program-summary structural assertions
```

Observed results:

- format: passed;
- Clippy: passed with `-D warnings`;
- Rust tests: library 1,738 passed/2 ignored, repository guard 105 passed, CLI
  127 passed, doctest 1 passed, with zero failures;
- repository guard and pre-commit `origin/main..HEAD` diff gate: passed;
- TypeScript worker: 1 Node test target passed;
- Python worker: exited successfully;
- JSON: all 21 unique records, exact 20-language denominator, ranks 1–20,
  `complete_count=0`, no complete row, and zero production contract packs;
- `git diff --check` and guide byte comparison: passed; and
- `main` and local `origin/main` still resolved to the protected baseline.

## Remaining risks and next highest-EV action

Principal blockers are authoritative project/environment selection, safe
version-pinned provider admission, package-qualified external-symbol identity,
reviewed contract packs, source-free projection, conflict/freshness policy, and
language-specific family evidence. Native, licensed, compiler, archive, and
runtime-provider lanes also require separate supply-chain and OS-sandbox
qualification before admission.

Under the zero-external-dependency constraint the partition above is the
operative risk statement. Eight lanes are excluded outright: Python, C, C++,
Java, C#, Visual Basic .NET, Rust, and Assembly each need an artifact the
constraint forbids, so no amount of in-repo work closes their binding gate.
Scratch is a recorded `NO_GO`. The twelve remaining lanes are open in principle
and unattempted, and SQL is the only one this continuation advanced -- from
`discovered_only` 2/9 to `structural_substrate` 7/9 -- which is also the
evidence that the open-lane label is not merely optimistic.

SQL now stands at 8/9. Its last gate is not an engineering task but a ruling:
ADR-0020 G9 lists frontend/IR and `UNKNOWN`/provider as separate submodules, and
for SQL they landed together because they are inseparable -- a semantic
`UNKNOWN` requires code-unit evidence, so the abstentions cannot precede the
frontend that produces the units, and no provider exists to form the other half.
The chain is otherwise four independently coherent commits with their own tests
and documentation. Deciding whether that satisfies G9 is the single highest-EV
next action under the constraint: it either produces the program's first 9/9
language or states exactly what a completion chain must look like, and every
other open lane copies whichever answer comes back.

If the constraint is lifted, the single highest-EV next action reverts to
finishing one end-to-end Python vertical:
select and pin the authoritative Python project/type provider under an isolated
supplied-input boundary, join one PyPI distribution and exact external symbol to
a reviewed exact-version FastAPI/pytest/Pydantic/SQLAlchemy contract, preserve
blocking `UNKNOWN`s, expose source-free evidence, and close all nine gates for
one narrowly scoped family before copying the mechanism to another language.
