# Swift N1 qualification handoff and next-session goal

- Status: Planned; development paused after discovery/configuration
- Date: 2026-07-16
- Scope: Swift ADR-0025 stage 3 only
- Authority:
  `../decisions/ADR-0020-top-20-language-expansion-gate.md`,
  `../decisions/ADR-0025-swift-syntax-sourcekit-xctest-preflight.md`, and
  `../reports/language-support/swift-completion-review.md`
- Baseline branch: `feature/framework-wave-2`
- Baseline commits: preflight
  `d293238723c0b943d9665f05a4db948fba0f0e35`; discovery/configuration
  `9bc1960db21e62216f2c9b85e88e32e9733390b0`

## Recorded pause checkpoint

Swift is `discovered_only` and unsupported. The committed discovery slice
provides stable `swift`/`swift-config` tokens, exact source/config grammar,
Swift-only `.build`/`.swiftpm` exclusions, bounded raw-byte metadata, full and
incremental inventory lifecycle, claim-record purge, autosync fingerprinting,
and source-free CLI/product output. It performs no Swift source-store read,
parse, project evaluation, tool invocation, semantic analysis, or support
promotion.

The discovery module received independent correctness and security reviews plus
a main-session design/performance review; all were clean. Its final validation
passed formatting, Clippy with warnings denied, 1,081 library tests with one
ignored, 13 repository-guard tests, 93 product-binary tests, one doctest,
repository guard, mirrored-guide equality, diff checks, and the post-commit
documentation gate. The checkpoint was not pushed or merged.

Under the strict ADR-0020 program gate, no ranked language yet has a linked
nine-item terminal completion audit. Four of the thirteen new ranked languages
-- Go, PHP, Swift, and Ruby -- have reached `discovered_only`; the other nine
new-language lanes remain unstarted. This is a stage count, not a support
percentage or a product-readiness claim.

## Next mission contract

Produce one source-backed qualification verdict for the exact Swift frontend
path frozen by ADR-0025. The stage must decide whether SwiftSyntax 603.0.2,
the exact Swift 6.3.3 compiler differential, the proposed artifact/dependency
closure, and the native sandbox contract are sufficiently evidenced to permit
a later, separate worker/artifact-admission module.

Allowed terminal labels are:

- `QUALIFIED`: every stage-3 gate has reproducible primary evidence;
- `NO_GO`: primary evidence proves the candidate cannot satisfy the accepted
  boundary without weakening it;
- `BLOCKED`: required evidence or a native environment is unavailable and the
  gap is stated exactly; or
- `INCONCLUSIVE`: evidence conflicts or is insufficient after the bounded
  investigation.

`QUALIFIED` authorizes only consideration of ADR-0025 stage 4. It does not add
or authorize a production dependency, binary, toolchain, worker, parser,
project model, semantic fact, public reason code, family, or support state.

## Fresh-session context

Read these files before any research or command:

1. `AGENTS.md` and `CLAUDE.md`; confirm byte equality.
2. `docs/README.md` and this handoff.
3. ADR-0020, ADR-0025, and
   `docs/plans/top-20-language-expansion-plan.md`.
4. `docs/reports/language-support/swift-completion-review.md`.
5. `docs/specifications/semantic-workers.md`,
   `docs/architecture/dependency-rules.md`, and
   `docs/development/testing.md`.
6. `.agents/memories/project-state.md`.
7. Current branch, `git status --short`, recent commits, and any existing
   Swift qualification artifacts or logs.

Treat the two baseline SHAs above and their primary artifacts as the current
checkpoint. Do not reopen discovery design unless current code or tests
contradict the recorded evidence.

## Hard constraints and resource policy

- Preserve unrelated user changes. Do not rewrite history, push, merge, or
  delete branches without explicit authorization.
- Do not edit `src/`, `Cargo.toml`, `Cargo.lock`, protocol schemas, production
  configuration, or release artifacts in this stage.
- Do not install a toolchain or package system-wide. Do not add a production
  dependency or commit downloaded archives, SDKs, build trees, binaries, large
  logs, or generated corpus outputs.
- Read-only network research and exact artifact acquisition may use official
  upstream sources and authoritative package, license, signature, or advisory
  registries only. Credentialed, paid, publishing, account, or external-
  state-changing actions require explicit approval. Network or artifact
  unavailability is a valid `BLOCKED` result.
- Keep downloads, builds, and raw logs in an OS temporary directory or an
  existing ignored build directory. Record commands, versions, hashes, host
  platform, and retained locations in the evidence report.
- Do not point SwiftPM, SwiftSyntax, a compiler, SourceKit, Xcode, or an LSP at
  this or another untrusted repository. Do not evaluate `Package.swift`, resolve
  dependencies, build/index target modules, load macros/plugins/generators, run
  tests or target code, spawn uncontrolled descendants, or use ambient package,
  SDK, credential, editor, home, or cache state.
- Cross-compilation, an upstream CI badge, tag existence, one happy-path file,
  or a source build on one host is not native multi-platform or sandbox proof.
- No GPU, Slurm, paid service, or model/API budget is authorized or needed.

## Truth criteria and evidence ladder

Primary evidence is exact official source/archive/installer identity and hashes;
signature verification where published; toolchain-to-source mapping; complete
transitive dependency, license, advisory, build-script, C/C++ shim, generated-
code, compiler/linker, SBOM, and reproducibility review; a preregistered
SwiftSyntax/compiler differential corpus with deterministic commands and full
outcomes; five-target compile/corpus evidence; and native Linux, macOS, and
Windows sandbox/resource evidence.

Auxiliary evidence includes official documentation, upstream manifests, issue
discussions, cross-compilation, and upstream CI. It may explain a gap but cannot
alone satisfy the corresponding gate. Forbidden evidence includes regex or
filename inference, mutable branches, unpinned snapshots, recovered-tree
success, silent parser/compiler disagreement, repository builds, manifest or
macro/plugin execution, and sanitized summaries without retained reproducible
provenance.

Failures must be classified as artifact/provenance, dependency/supply-chain,
build/target, compiler differential, range/recovery, protocol/resource,
sandbox/confinement, environment/tool availability, leakage, nondeterminism,
or scope violation. Do not convert any of these into a positive claim.

## Execution phases and gates

### Phase 0: sanity and preregistration

Audit the dirty/untracked worktree, protect scratch, run `git diff --check`, and
write the candidate versions, artifact identities, platform matrix, corpus
categories, exact success gates, failure taxonomy, and stop conditions before
outcome-driven commands. Record the host OS/architecture and which native rows
are actually executable.

### Phase 1: artifact and dependency evidence

Verify the exact Swift 6.3.3, SwiftSyntax 603.0.2, SourceKit-LSP 6.3.3,
SwiftPM 6.3.3, and XCTest identities already frozen by ADR-0025. Collect
official hashes/signatures where available, map binaries to sources, and audit
the complete dependency/build/generated-code surface. Missing Windows
authenticity, mutable provenance, an incomplete closure, or an unreviewed
executable build step keeps the gate open.

### Phase 2: compiler differential evidence

Preregister a bounded corpus covering valid, malformed, recovered, Unicode,
deep, large, conditional-compilation, macro-bearing, version-sensitive, and
range-sensitive Swift. Compare exact SwiftSyntax 603.0.2 `SwiftParser` behavior
with the exact Swift 6.3.3 compiler without evaluating a target repository.
Record every disagreement, recovery, diagnostic/range mismatch, crash,
timeout, resource failure, and nondeterministic result. No partial or truncated
run counts as success.

### Phase 3: target and sandbox evidence

Attempt only the platform rows safely available. The minimum matrix is
`x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`,
`x86_64-apple-darwin`, `aarch64-apple-darwin`, and
`x86_64-pc-windows-msvc`; Linux, macOS, and Windows additionally require native
filesystem, network, descendant-process, timeout, CPU, memory, thread, and
output enforcement. Windows must cover the upstream dynamic-library path.
Record unavailable native rows as gaps rather than substituting cross-builds.

### Phase 4: decision and atomic record

Classify each gate and the overall result as `QUALIFIED`, `NO_GO`, `BLOCKED`, or
`INCONCLUSIVE`. Write
`docs/reports/language-support/swift-frontend-qualification.md` and a matching
`swift-frontend-qualification.summary.json`, update the Swift completion ledger,
this handoff, the Top-20 plan, roadmap, changelog, documentation map, and project
memory, then perform independent correctness, security, completeness/design,
and performance/resource reviews of the evidence and claims.

Validate the JSON, relative links, `git diff --check`, formatting, Clippy, the
full workspace tests, repository guard, and mirrored guides. Stage explicit
paths, review the staged diff, and create one atomic Conventional Commit. Do not
push. A valid negative or blocked evidence commit is preferable to an unsupported
positive conclusion.

## Stop conditions

Stop without weakening the gate when an exact artifact or source mapping cannot
be verified; a candidate requires repository/project evaluation, dependency
resolution, macros/plugins, ambient state, or uncontrolled network/processes;
the differential is nondeterministic or cannot bound recovery/ranges/resources;
the sandbox cannot contain malformed inputs; output leaks source, paths,
credentials, or host state; required native rows are unavailable; or a complete
dependency/supply-chain review cannot be reproduced.

This is a single qualification pass, not an optimization campaign. Retry a
failed acquisition or command only when the failure class has a concrete safe
remedy; do not tune the corpus or thresholds to manufacture a positive result.

## Final report contract

Report the terminal label; what was attempted and why; a gate-by-gate table;
exact artifact identities, commands, host/platform coverage, and evidence paths;
all failure classes and unavailable native rows; committed and untracked
artifacts; validation results; commit SHA; active external jobs, if any; and the
single next highest-value action. State explicitly whether stage 4 remains
blocked. Never describe Swift as supported from a stage-3 result.

## Paste-ready next-session prompt

```text
/goal Produce a source-backed Swift N1 stage-3 qualification verdict without adding runtime behavior.

Repository: /Users/sioyoo/code/RepoGrammar. Start from feature/framework-wave-2 at or after d293238723c0b943d9665f05a4db948fba0f0e35 and 9bc1960db21e62216f2c9b85e88e32e9733390b0. Read AGENTS.md, CLAUDE.md, docs/README.md, ADR-0020, ADR-0025, docs/plans/swift-n1-qualification-handoff.md, the Top-20 plan, the Swift completion review, semantic-workers.md, dependency-rules.md, testing.md, and project-state.md. Treat the handoff as the detailed execution authority.

Objective: decide whether exact SwiftSyntax 603.0.2 plus the exact Swift 6.3.3 compiler differential, artifact/dependency closure, five-target evidence, and native Linux/macOS/Windows sandbox contract qualify a later separate worker-admission stage. Allowed outcomes are QUALIFIED, NO_GO, BLOCKED, or INCONCLUSIVE; a positive verdict authorizes no dependency or runtime by itself.

Hard constraints: preserve user changes; no src/Cargo/protocol edits; no production dependency, system-wide install, push, merge, repository build/evaluation, Package.swift execution, dependency resolution, macros/plugins, target code/tests, ambient home/cache/SDK discovery, credentialed or paid action, or weak-evidence support claim. Official upstream and authoritative registry research plus exact temporary artifact acquisition are allowed; keep large outputs untracked and provenance-bound. Cross-compilation and upstream CI are auxiliary, never native proof.

Phases: (0) audit git state and preregister versions, matrix, corpus, gates, failures, and stops; (1) verify exact artifacts, hashes/signatures, source mapping, transitive dependencies, licenses/advisories, build/generated/C++ surface, SBOM and reproducibility; (2) run a preregistered valid/malformed/recovered/Unicode/deep/large/conditional/macro/version/range differential corpus against exact SwiftSyntax and compiler, recording every disagreement and resource failure; (3) collect only genuinely native target/sandbox evidence and leave unavailable rows open; (4) classify every gate, run four-part evidence review, and atomically record the result.

Artifacts: docs/reports/language-support/swift-frontend-qualification.md plus .summary.json; synchronized Swift ledger, handoff, Top-20 plan, roadmap, changelog, docs map, and project memory. Validate JSON, links, git diff --check, fmt, clippy -D warnings, full tests, repo-guard, and AGENTS/CLAUDE equality. Commit explicitly staged documentation/evidence with Conventional Commits; do not push.

Final report: terminal label, gate table, exact artifacts/commands/hosts, failures and missing native rows, evidence paths, untracked large outputs, validation, commit SHA, active jobs, whether stage 4 is blocked, and one next action. Never describe Swift as supported.
```
