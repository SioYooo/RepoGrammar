# TypeScript language completion review

- Language: TypeScript
- Frozen Top-20 rank: **not ranked; explicit extra language**
- Lane: X0 extra-language convergence
- Audited integration baseline: `8a4033e3eddc93787c997d13c54bba22b1df7102`
- Completion state: **Incomplete extra — `structural_substrate` under ADR-0020**
- Counted in Top-20 denominator: **no**
- Counted as Top-20 complete: **never; tracked separately**
- Last reviewed: 2026-08-01

TypeScript follows ADR-0020's provider and `UNKNOWN` convergence discipline but
is not one of the frozen twenty languages. It shares infrastructure with
JavaScript while requiring its own inventory/readiness counts, fixtures, and
completion conclusion. A future complete TypeScript lane would remain an extra
result and would not change the Top-20 numerator or denominator.

## Scope and dialect boundary

Current source discovery admits exact lowercase `.ts` and `.tsx`. Exact
`package.json`, `tsconfig.json`, selected Jest/Vitest/Mocha config names, and
selected Next config names enter the shared `tsjs-config` lane. General `.mts`
and `.cts` source are not current scope. The structural parser accepts bounded
TypeScript/TSX syntax, but the repo does not pin a complete TypeScript language
version, JSX mode, module-resolution strategy, project-reference graph, target,
lib set, path mapping, or package profile as a completed dialect.

The worker specification identifies TypeScript 6 public compiler API as a
candidate versioned path and treats TypeScript 7.0 API instability as requiring
a CLI/LSP compatibility path or `UNKNOWN`; that policy is not a bundled locked
provider implementation. The current worker directory has no package-manager
lockfile or bundled compiler dependency.

## Current implementation evidence

### Discovery, frontend, owned IR, and project model

- Deterministic `.ts`/`.tsx` and shared config discovery, skips, resource limits,
  git-ignore behavior, and symlink containment exist.
- `src/rust/adapters/parsing/syntax.rs` and
  `src/rust/adapters/parsing/tsjs/` emit owned units, ranges, hashes, IR,
  structural project context, import/alias candidates, exact framework anchors,
  diagnostics, and typed `UNKNOWN`s. This default path is explicitly syntax-only
  bootstrap substrate.
- The `typescript_compiler` slot is integrated. The checked-in Node worker and
  Rust host support bounded `resolve_module_specifier`, `resolve_export`,
  `resolve_reexport`, and `resolve_package_entry` operations with validated
  evidence and a structural/UNKNOWN fallback.
- When an official compiler module is available in the trusted worker
  environment, a narrow operation may emit provider-resolved facts. The worker
  does not construct the X0-required pinned whole-project `Program`/`TypeChecker`
  model, and default indexing still reports the semantic worker as deferred.
- Bounded root config parsing and `extends` containment exist, but project
  references, composite builds, all compiler options, resolution modes,
  conditional exports, declaration packages, ambient libs, and monorepo profile
  selection are not a complete project model.

### Package metadata and third-party-library analysis

The root `package.json` reader records bounded direct npm declarations at
`manifest_declared` evidence without executing Node, npm, scripts, or dependency
code. It distinguishes production/development/optional scopes and preserves peer
scope as `unknown`. These records are evidence-bound and cannot produce a family
claim.

The worker can establish narrow repo-local module/export facts when compiler API
evidence, config/package hashes, source ranges, and operation scope validate. It
does not yet produce the general package-qualified external-symbol identities
required by ADR-0030, resolve a lockfile graph, prove installed versions, or
match the exact-version registry because production contract packs are absent.
Loading the target repository's own
TypeScript package is disabled by default because doing so executes dependency
code; the explicit trust opt-in is not acceptable for untrusted-default
completion. Arbitrary third-party TypeScript-library behavior therefore remains
inventory-visible but semantically `UNKNOWN` unless bounded source anchors and
future reviewed evidence agree.

### Exact families and typed `UNKNOWN`

TypeScript units participate in the conservative TS/JS families for Express,
Jest/Vitest, Next.js, Fastify, Prisma, Drizzle, Zod, NestJS, Hono, Mocha, and
Playwright.
Families require support at least three, complete-link compatibility, exact
bindings/conventions, and owned `repogrammar-tsjs-derived` support. Optional
worker facts can cross-check bounded Next/Express/Fastify/Prisma/Drizzle binding
obligations after freshness and evidence validation; heuristic facts and
structural fallback cannot support a family alone.

Typed `UNKNOWN`s cover dynamic import/require, ambiguous exports/re-exports,
missing/untrusted/unsupported compiler API, invalid config and out-of-root
`extends`, unresolved aliases/rootDirs, ambient or bundler-only globals, dynamic
decorators/prototypes/proxies/eval, framework DI/runtime behavior, stale or
conflicting facts, insufficient support, worker timeout/crash/protocol failure,
and resource bounds. The classification is substantial but not a complete X0
provider/project-model and parse-degraded fixture closure.

## Four-part review

### Correctness and bug findings

Positive evidence includes operation-scoped compiler facts, strict request-fact
matching, exact hash/range/path checks, unsupported-version rejection,
dependency-free fallback separation, alias/re-export ambiguity, framework
shadowing/reassignment negatives, support thresholds, complete-link clustering,
and stale/conflict readiness behavior.

Completion-blocking gaps are explicit: there is no bundled and pinned compiler;
no complete `Program`/`TypeChecker` construction; the default scanner is not an
authoritative TypeScript frontend; project references, declaration resolution,
compiler-option profiles, full module/export/package identity, decorators,
dynamic runtime, and ambient types are incomplete; and no dedicated
parse-degraded TypeScript product corpus proves family non-formation. A narrow
successful worker operation must not be overclaimed as whole-project semantics.

### Security and untrusted-input handling

The worker/host boundary validates protocol, absolute project root, requested
repo-relative paths, traversal/URI/backslash rejection, symlink containment,
hashes, ranges, work counts, facts, end-of-stream, bounded request/output, timeout
and inherited-pipe behavior, and sanitized errors. Config `extends` cannot escape
the project root. Compiler loading from the target repository is opt-in only and
off by default. npm inventory is static and non-executing.

Residual security gaps block completion: a pinned compiler acquisition and
supply-chain policy is absent; Node and compiler modules execute trusted worker
code in-process; no completion audit proves filesystem/network/descendant/CPU/
memory containment for a full project provider; and trust-project mode must not
serve as the default untrusted path. Future package/type providers must also
prevent plugins, transformers, loaders, scripts, and dependency code execution.

### Implementation completeness

Discovery, owned units/IR, project-context substrate, exact family slices,
operation-scoped optional compiler evidence, npm inventory, provider capability
reporting, and source-free surfaces exist. The pinned full project model,
package-qualified symbols/contracts, complete provider/UNKNOWN and fixture
matrix, performance evidence, and atomic completion chain do not. TypeScript's
separate report closes a documentation gap only; it cannot pass missing runtime
gates.

### Performance and resource bounds

The runtime adapter bounds request size, operation lists, output, time, and
worker protocol. Incremental indexing uses project-context rebuild gates, and
generic read/write benchmark fixtures exist. There is no representative pinned
compiler benchmark across large projects, project references, alias/export
graphs, declaration packages, TSX, or monorepos, and no measured provider cache
or memory ceiling. The dependency-free fallback's low cost cannot be used as
evidence for the absent authoritative provider.

## ADR-0020 nine-gate checklist

| Gate | Result | Audited evidence and blocker |
|---|---|---|
| 1. Discovery/config | Partial | `.ts`/`.tsx`, bounded shared configs, skips, limits, and symlinks exist; `.mts`/`.cts`, version/compiler-option/project-reference/profile scope is not fully qualified. |
| 2. Authoritative frontend | Partial | An integrated bounded compiler-operation worker exists, but no pinned bundled compiler or full `Program`/`TypeChecker` project model backs the default path. |
| 3. Owned code units/IR | Pass | Stable TypeScript tokens, units, ranges, hashes, owned IR/facts/evidence/provenance, serialization, storage, and readback exist. |
| 4. Typed `UNKNOWN` | Partial | Extensive structural, dynamic, provider, stale, conflict, and fallback outcomes exist; full provider/project-model obligation closure and controlled discharge matrix do not. |
| 5. Family-first exact anchor | Pass as substrate | Multiple exact TypeScript families can meet support >= 3 and compatibility rules; structural or fallback evidence alone cannot support them. |
| 6. Fixture proof | Partial | Positive, lookalike, low-support, dynamic, stale, and selected unresolved/resolved fixtures exist. Dedicated parse-degraded and full provider absent/present/stale/conflicting X0 fixtures are missing. |
| 7. Source-free readiness | Partial | CLI/MCP/stats/readiness, provider availability, and leakage tests exist, but no linked TypeScript-only final readiness audit covers the completed provider contract. |
| 8. Four-part review | Satisfied by this snapshot | The four required dimensions are recorded. Open correctness, provider security, completeness, and representative performance gaps remain blockers. |
| 9. Atomic delivery/audit | Fail | The main TS/JS capability landed as an aggregate commit and no TypeScript-extra final audit links independently complete D2 modules and full gates. |

Result: **not 9/9; TypeScript remains an incomplete extra and never contributes
to the Top-20 count.**

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
- Worker/provider: `src/workers/typescript/README.md`,
  `src/workers/typescript/worker.js`,
  `src/workers/typescript/worker.test.js`,
  `src/rust/adapters/semantic_workers/typescript.rs`,
  `src/rust/ports/tsjs_provider.rs`, `src/rust/core/model/provider.rs`,
  `src/rust/application/providers.rs`
- Family/pipeline: `src/rust/application/indexing.rs`,
  `src/rust/application/family.rs`, `src/rust/adapters/frameworks/tsjs/`
- Fixtures: `src/fixtures/typescript/release/v0_1/`,
  `src/fixtures/typescript/release/v0_2/`,
  `src/fixtures/unknown_reduction/tsjs_express_unresolved/`,
  `src/fixtures/unknown_reduction/tsjs_express_resolved/`,
  `src/fixtures/unknown_reduction/tsjs_prisma_unresolved/`,
  `src/fixtures/unknown_reduction/tsjs_prisma_resolved/`

## Evidence-bearing historical SHAs

These ancestor commits identify current substrate, not a qualifying D2/G9
prerequisite chain:

- `6561987ef2a01d5b920315a9afe97be92e57853b` — shared discovery, parser,
  persistence, query, and worker-host substrate.
- `2568c330c626d8cca73fe0dd1340e1c8440a3c5e` — aggregate TS/JS exact-anchor,
  worker, family, and fixture landing.
- `9ea396b96ae11eb79ec9f80dd52b66e631c7dc1a` — language-neutral dependency
  evidence model.
- `5ef354249b58784b1e872aa6ac3d83c7502dd4c8` — dependency persistence.
- `f516aefb014efe1bc780be73a4ff0a11fb745b84` — bounded npm manifest inventory.

## Required next closure sequence

1. Pin a compiler/API version and acquisition policy, then build the bounded X0
   `Program`/`TypeChecker` project model with explicit TypeScript/TSX, module,
   JSX, config, project-reference, package, and version boundaries.
2. Prove untrusted-default provider isolation and deterministic provenance,
   cache, timeout, unsupported-version, stale, conflict, and failure behavior
   without loading repository dependencies.
3. Add package-qualified external symbols and versioned reviewed library
   contracts above npm inventory; audit one exact family end-to-end.
4. Add TypeScript-specific positive, negative, low-support, parse-degraded,
   stale/conflicting, and controlled unresolved/resolved product fixtures plus
   leakage and representative resource measurements.
5. Land coherent submodule commits and a final TypeScript-extra audit linking
   exact SHAs and full required gates, while leaving the Top-20 count unchanged.

## Risks and non-claims

- This report does not claim full TypeScript compiler, language-service,
  project-reference, declaration, decorator, ambient-type, build, or runtime
  semantics.
- npm inventory does not prove installation, selected versions, type packages,
  module exports, external symbol identity, or library behavior.
- Structural fallback and syntax exact anchors are not compiler-backed facts.
- Trust-project TypeScript loading is not safe-default evidence for untrusted
  repositories.
- JavaScript and TypeScript may share implementation, but neither language's
  completion can be inferred from the other's fixtures or label.
- Even a future 9/9 TypeScript result remains outside the frozen Top-20 count.

## Completion verdict

**Incomplete extra — `structural_substrate`; Top-20 counted = no.** TypeScript
has the only integrated optional compiler slot, but it lacks the pinned full
project model, complete fixture/review closure, and linked atomic audit required
for a standalone ADR-0020 completion result.
