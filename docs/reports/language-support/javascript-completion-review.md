# JavaScript language completion review

- Language: JavaScript
- Frozen Top-20 rank: 6
- Lane: C0 current-language convergence
- Audited integration baseline: `8a4033e3eddc93787c997d13c54bba22b1df7102`
- Completion state: **Incomplete — `structural_substrate` under ADR-0020**
- Counted in Top-20 denominator: **yes**
- Counted as Top-20 complete: **no**
- Last reviewed: 2026-08-01

JavaScript shares parser and optional worker infrastructure with TypeScript, but
ADR-0020 requires a separate completion conclusion. TypeScript evidence cannot
silently fill a JavaScript gate, and this report does not count the TypeScript
extra lane.

## Scope and dialect boundary

Current source discovery admits exact lowercase `.js` and `.jsx`. Exact
`package.json`, `jsconfig.json`, selected Jest/Vitest/Mocha JSON or JavaScript
config names, and selected Next config names enter the shared `tsjs-config`
lane. `.mjs` and `.cjs` are recognized only for certain named config files, not
as general JavaScript source extensions; `.mts` and `.cts` are not current
source scope. The structural parser covers bounded ECMAScript import/export,
CommonJS `require`, JSX, and framework-shaped syntax, but it does not declare a
pinned universal ECMAScript edition, Node resolution profile, browser/bundler
profile, JSX transform, or runtime environment.

The audited family scope includes exact-anchor Express, Jest/Vitest, Next.js,
Fastify, Prisma, Drizzle, Zod, NestJS, Hono, and Mocha shapes where implemented.
React roles remain unsupported for family claims.

## Current implementation evidence

### Discovery, frontend, owned IR, and project model

- Deterministic discovery, common dependency/build skips, bounded reads,
  git-ignore behavior, symlink containment, and project-config recognition are
  implemented in `src/rust/adapters/filesystem/discovery.rs`.
- The default parser in `src/rust/adapters/parsing/syntax.rs` is explicitly a
  dependency-free syntax-only bootstrap boundary. It produces RepoGrammar-owned
  code units, ranges, hashes, IR, structural facts, and diagnostics; it is not an
  authoritative JavaScript semantic frontend.
- `src/rust/adapters/parsing/tsjs/` supplies bounded import/alias/scope/project
  context and exact framework anchors. Root `package.json`, `jsconfig.json`, and
  selected configs provide static project context, not a complete Node/bundler
  program model.
- The integrated `typescript_compiler` worker slot can run bounded module,
  export, re-export, and package-entry operations when explicitly configured or
  when a trusted compiler API is available. It does not construct the required
  pinned JavaScript-mode `Program`/`TypeChecker` project model, and the default
  indexing path remains provider-deferred.

### Package metadata and third-party-library analysis

The bounded root `package.json` reader records direct npm declarations from
`dependencies`, `devDependencies`, `optionalDependencies`, and
`peerDependencies` as ADR-0030 `manifest_declared` evidence. It never invokes
Node, npm, lifecycle scripts, package code, or network resolution. Peer scope is
intentionally `unknown`; unchanged records copy forward only with unchanged
evidence.

This inventory does not establish install state, lock resolution, export maps,
conditional exports, module format, selected runtime, package-qualified
external symbols, or library behavior. The optional TypeScript worker can
resolve a narrow operation only after path/hash/range/config validation; its
dependency-free fallback is structural and cannot support a family alone. The
exact-version reviewed-contract registry exists, but production contract packs
and eligible package-qualified symbol evidence are both zero. Full third-party
analysis therefore needs a pinned JavaScript project profile, isolated provider
queries, lock/config coherence, package-qualified symbol identity, and reviewed
capability contracts above inventory.

### Exact families and typed `UNKNOWN`

The shared TS/JS family path requires support at least three, complete-link
compatibility, exact import/require or safe ambient-runner context, and owned
`repogrammar-tsjs-derived`/`bounded_exact_anchor_v1` support. Framework heuristic
facts never support a family. JavaScript-specific positive fixtures exist for
Express and Jest/Vitest, which is evidence that `.js` units can participate
without being relabeled TypeScript.

Typed `UNKNOWN` coverage includes dynamic import, conditional/non-literal
`require`, unresolved/conflicting aliases or `rootDirs`, star re-export
ambiguity, unsafe receivers and route methods, package/config absence, runtime
framework magic, DI, dynamic query builders, raw operations, stale/conflicting
facts, missing provider, and insufficient support. The current split is useful
but incomplete: JavaScript-mode provider absence/config/degradation and dynamic
runtime obligations have not been closed through a dedicated completion matrix.

## Four-part review

### Correctness and bug findings

Positive evidence includes exact ESM/CommonJS binding checks, shadowing and
reassignment negatives, literal receiver/method gates, source-position context,
support thresholds, complete-link clustering, config/alias resolution,
provider fact freshness, and source-free query tests. The JavaScript-specific
fixture directories prevent all evidence from being inferred solely from `.ts`
examples.

Completion-blocking correctness gaps remain: the default syntax scanner is not
an authoritative parser/semantic oracle; JavaScript source extensions and
runtime profiles are narrower than the label suggests; no pinned compiler
JavaScript project mode constructs complete module/export/config context; Node
versus browser/bundler conditions and CommonJS/ESM interop remain unresolved;
and no dedicated parse-degraded JavaScript product fixture proves that damaged
anchors cannot become support. These must remain typed `UNKNOWN`.

### Security and untrusted-input handling

The shared worker protocol validates repo-relative paths, strict hashes/ranges,
operation scope, bounded input/output, timeouts, end-of-stream, sanitized error
codes, requested-file containment, and config `extends` containment. Loading a
target repository's TypeScript package is disabled by default because it would
execute dependency code; the explicit trust opt-in does not make it safe for
untrusted repositories. Static npm inventory is non-executing.

Residual risk centers on authoritative provider acquisition and JavaScript
project modeling. A completion provider must not load repository dependencies,
plugins, transforms, loaders, package scripts, or ambient global configuration;
must isolate filesystem/network/descendants and bound CPU/memory/time/output;
and must sanitize compiler diagnostics. The checked-in worker does not bundle a
locked compiler dependency, so supply-chain/version reproducibility is not yet
closed.

### Implementation completeness

Discovery, owned units/IR, project-context substrate, exact families, npm
inventory, an optional operation worker, and source-free surfaces exist. The
authoritative JavaScript frontend/project model, full provider fallback matrix,
parse-degraded completion fixtures, four-part closeout, and atomic final audit
do not. Shared TS/JS code reduces implementation cost but does not waive
language-specific readiness, fixtures, or conclusions.

### Performance and resource bounds

Discovery and parsing are bounded; worker requests share a 1 MiB envelope,
runtime adapter timeout/output constraints, deterministic operation lists, and
incremental project-context rebuild gates. Generic read/write benchmark
fixtures exist. There is no representative JavaScript completion benchmark for
large monorepos, export graphs, path aliases, JSX, or package maps under a pinned
compiler, and no measured provider cache behavior. These are open performance
evidence, not implied success.

## ADR-0020 nine-gate checklist

| Gate | Result | Audited evidence and blocker |
|---|---|---|
| 1. Discovery/config | Partial | `.js`/`.jsx`, bounded shared config, skips, limits, and symlink behavior exist; general `.mjs`/`.cjs`, runtime/profile selection, and complete JavaScript project scope are not qualified. |
| 2. Authoritative frontend | Partial | A bounded TypeScript compiler-operation worker is integrated, but default JavaScript parsing is syntax-only and the required pinned JS-mode `Program`/`TypeChecker` project model is absent. |
| 3. Owned code units/IR | Pass | Stable JavaScript tokens, units, ranges, hashes, owned IR, facts, serialization, persistence, and readback substrate exist. |
| 4. Typed `UNKNOWN` | Partial | Broad structural/provider/runtime blockers exist, but JavaScript-specific provider/project-profile, parse-degraded, conflict, and runtime closure is incomplete. |
| 5. Family-first exact anchor | Pass as substrate | JavaScript Express and runner fixtures can satisfy support >= 3 via exact anchors; shared families remain bounded and do not waive other gates. |
| 6. Fixture proof | Partial | Positive JavaScript, TS/JS negatives/lookalikes, low-support, dynamic, stale, and selected unresolved/resolved pairs exist. A dedicated JS parse-degraded and provider-state completion matrix is missing. |
| 7. Source-free readiness | Partial | CLI/MCP/stats/readiness paths and leakage tests exist, including JavaScript counts through shared language surfaces, but no linked JavaScript final readiness audit exists. |
| 8. Four-part review | Satisfied by this snapshot | This strict review records correctness, security, completeness, and performance findings. Unresolved items remain blockers or risks and are not closed by the review itself. |
| 9. Atomic delivery/audit | Fail | The principal TS/JS capability landed as an aggregate commit, not a linked D2 submodule chain, and no final JavaScript audit reran and linked all required gates. |

Result: **not 9/9; JavaScript must not increment the ADR-0020 completed-language
count.**

## Evidence paths

- Authority: `docs/decisions/ADR-0020-top-20-language-expansion-gate.md`,
  `docs/decisions/ADR-0030-language-neutral-dependency-library-semantics.md`
- Specifications: `docs/specifications/semantic-workers.md`,
  `docs/specifications/indexing-pipeline.md`,
  `docs/specifications/unknowns.md`, `docs/specifications/product.md`,
  `docs/development/testing.md`
- Discovery/parser: `src/rust/adapters/filesystem/discovery.rs`,
  `src/rust/adapters/parsing/syntax.rs`,
  `src/rust/adapters/parsing/tsjs/`
- Worker/provider: `src/workers/typescript/worker.js`,
  `src/workers/typescript/worker.test.js`,
  `src/rust/adapters/semantic_workers/typescript.rs`,
  `src/rust/ports/tsjs_provider.rs`, `src/rust/core/model/provider.rs`
- Family/pipeline: `src/rust/application/indexing.rs`,
  `src/rust/application/family.rs`, `src/rust/adapters/frameworks/tsjs/`
- JavaScript fixtures:
  `src/fixtures/typescript/release/v0_2/javascript_exact_routes/`,
  `src/fixtures/typescript/release/v0_2/javascript_jest_vitest_exact_tests/`
- Shared negative/unknown fixtures:
  `src/fixtures/typescript/release/v0_2/framework_adapter_negative_cases/`,
  `src/fixtures/typescript/release/v0_2/unsupported_framework_lookalikes/`,
  `src/fixtures/unknown_reduction/tsjs_express_unresolved/`,
  `src/fixtures/unknown_reduction/tsjs_express_resolved/`

## Evidence-bearing historical SHAs

These ancestor commits identify current substrate, not a qualifying D2/G9
prerequisite chain:

- `6561987ef2a01d5b920315a9afe97be92e57853b` — shared discovery, parser,
  persistence, query, and worker-host substrate.
- `2568c330c626d8cca73fe0dd1340e1c8440a3c5e` — aggregate TS/JS exact-anchor,
  family, fixture, and worker slice.
- `9ea396b96ae11eb79ec9f80dd52b66e631c7dc1a` — language-neutral dependency
  evidence model.
- `5ef354249b58784b1e872aa6ac3d83c7502dd4c8` — dependency persistence.
- `f516aefb014efe1bc780be73a4ff0a11fb745b84` — bounded npm manifest inventory.

## Required next closure sequence

1. Pin a trusted TypeScript compiler/API version and construct a bounded
   JavaScript-mode project model with explicit ECMAScript, JSX, Node/browser,
   module, package, alias, and config boundaries.
2. Add candidate-scoped package-qualified module/export/symbol resolution with
   provenance, cache, isolation, conflict, stale, timeout, and absent-provider
   behavior; keep dependency-free fallback non-supporting.
3. Add versioned reviewed library contracts above npm inventory and prove one
   JavaScript family end-to-end without executing dependency code.
4. Add dedicated JS positive, negative, low-support, parse-degraded,
   stale/conflicting, and unresolved/resolved product fixtures and leakage
   tests.
5. Land coherent submodule commits, run the full required gates, and link exact
   SHAs from a final JavaScript completion audit.

## Risks and non-claims

- This report does not claim general ECMAScript, Node, browser, bundler, JSX,
  CommonJS/ESM interop, dynamic dispatch, DI, framework lifecycle, or runtime
  semantics.
- npm declarations do not prove installation, selected versions, export maps,
  import identity, or library behavior.
- A configured TypeScript operation worker is not a complete JavaScript
  `Program`/`TypeChecker` implementation.
- TypeScript fixture or provider evidence cannot be counted as JavaScript
  completion without JavaScript-specific proof.
- Source-visible framework anchors are bounded family evidence, not support for
  every third-party JavaScript library.

## Completion verdict

**Incomplete — `structural_substrate`; Top-20 counted = yes; Top-20 complete =
no.** JavaScript has real exact-anchor and optional-provider substrate but lacks
the authoritative JS project model, full fixtures/review closure, and atomic
completion audit required to advance to ADR-0020 `bounded_preview`.
