# Limitations

RepoGrammar is a pre-alpha public preview. It is designed to be conservative
and local-first, not a sound general static analyzer.

## Release Availability

- Public release artifacts are available only after a preview tag runs the
  release workflow and the assets are verified.
- The npm wrapper is available only after `@sioyooo/repogrammar` is published.
- Source-checkout dogfood is the safe contributor path before release and npm
  publication exist.
- The three-phase rollout gate (source dogfood, then verified prerelease
  artifacts, then verified npm) is tracked in
  `release/public-preview-release-checklist.md`.

## Language And Framework Scope

- Python v0.1 support is bounded to FastAPI, pytest, Pydantic, and SQLAlchemy
  implementation families.
- Python claims are source-backed framework-family claims, not full Python
  semantic analysis.
- JS/TS support is a conservative v0.2 exact-anchor preview for Express,
  Jest/Vitest, Next.js, Fastify, Prisma, and Drizzle.
- React, full JS/TS semantic analysis, dynamic wrappers, broad re-export
  analysis, executable config semantics, and general runtime behavior are not
  supported.
- Rust support is internal self-dogfood only and does not claim general Rust
  semantic analysis.
- Java/Spring support is structural preview only and does not execute classpath,
  build, DI, proxy, or generated repository semantics.
- Go is discovered-only and unsupported: `.go`, `go.mod`, and `go.work` may
  appear in source-free file inventory, but RepoGrammar does not read them for
  parsing or emit Go units, facts, IR, families, or readiness claims. Go-only
  generations are file-manifest-only; incremental inventory does not imply Go
  project-context or semantic support.

## UNKNOWN Is Expected

`UNKNOWN` is a first-class result. It protects users from false certainty when
evidence is stale, dynamic, ambiguous, low-support, unsupported, conflicting,
or unavailable.

Do not "fix" an `UNKNOWN` by weakening support gates, guessing from naming
conventions, or promoting package/config presence into family evidence. Improve
an `UNKNOWN` only with source-backed positive and negative evidence that keeps
false certainty controlled.

## Source Text

RepoGrammar returns metadata by default. Source text is opt-in through:

```text
--include-source-spans
```

or MCP:

```text
include_source_spans=true
```

When source spans are requested, RepoGrammar renders only bounded spans selected
from the read plan after hash and freshness checks. Stale or omitted spans mean
the agent should use normal source reads for the affected case.

## Token Savings

`estimated_potential_token_savings` is an estimated local read-displacement
diagnostic. It is not measured token savings and not a causal claim.

Measured token-saving claims require paired baseline/treatment evidence with a
comparable measurement source and valid treatment correctness.

## Telemetry

Anonymous telemetry is off by default. Telemetry must not include source code,
paths, repository names, symbols, prompts, query text, evidence text,
credentials, raw errors, diffs, or patches. Upload is explicit and separate
from local query diagnostics.

## Known Engineering Limitations

These are intentional current behaviors or tracked deferrals, not defects:

- **File-discovery excludes are basename-based.** Common build/output/cache
  directory names (for example `generated`, `out`, `cache`, `env`, `build`,
  `dist`) are skipped at any depth. A real source directory that happens to use
  one of those names is not indexed. This is a conservative default; rely on
  `.gitignore` and repository layout rather than these names for source you want
  indexed.
- **Aggregate discovery limits are fixed safety ceilings.** Discovery and the
  autosync preflight have no CLI/environment knobs for the 100,000 accepted
  files, 512 MiB accepted bytes, 250,000 visited entries, or depth-256 ceilings;
  discovery additionally caps reported skips at 100,000. An exact boundary is
  accepted, while plus one fails the operation without a partial generation.
  Autosync polling does not evaluate Git ignore, so supported Git-ignored files
  count toward its file/byte ceilings and may make polling reject a repository
  that manual Git-aware discovery would accept.
  Narrow the repository root or exclude generated, dependency, build, and cache
  content when a ceiling is reached.
- **Concurrent filesystem tree replacement remains a confinement gap.** The
  aggregate bounds cap traversal and retained output, and current walkers reject
  observed symlinks and canonical paths outside the repository, but a concurrent
  tree swap can occur between canonicalization and reopen/metadata use. Closing
  that pre-existing cross-platform TOCTOU gap requires the shared no-follow,
  handle-relative traversal and same-open-file metadata/read invariant accepted
  by ADR-0023 for discovery, source reads, and autosync fingerprinting. Its
  candidate dependency still lacks complete transitive/advisory, five-target
  compile, one-component descendant-open, nonblocking/special-file-safe open,
  and three-OS runtime proof, and no implementation has landed. The intended
  closure is limited to covered symlink/reparse/pathname replacement
  redirection; mounted/bind-mounted descendants, hard-linked physical origin,
  and concurrent mount-topology changes remain outside it. The ADR, aggregate
  bounds, and existing symlink tests do not claim concurrent filesystem safety;
  the completion review remains incomplete.
- **Source-inventory incremental sync is whole-project.** Any Python, TS/JS, or
  Rust source change forces a full-rebuild `sync` because import, fixture, and
  module inventories are project context. The incremental copy-forward path is
  reserved for deltas that pass that project-context gate.
- **Token-saving readiness caps at partial.** The `token_saving_readiness`
  signal reports at most `partial` in the v0.1 preview; a dedicated `ready`
  band is deferred.
- **Release checksums provide integrity, not authenticity.** Installers verify a
  `.sha256` fetched from the same release endpoint as the artifact. Signing and
  signature verification (or pinned digests) are deferred.
- **Single-writer connection reuse for indexing is deferred.** Each record write
  currently opens its own SQLite connection. Reusing one writer connection per
  generation build (and, further out, batching multiple rows per transaction) is
  a tracked storage-write-lifecycle change that must preserve per-record
  crash-consistency and mid-build cross-connection reads.
- **Local metrics opt-out and retention are partial.** Local aggregate,
  source-free query-outcome and token-savings rollups do not yet re-check the
  `DO_NOT_TRACK`/`REPOGRAMMAR_TELEMETRY`/`CI` environment kill-switch, and the
  telemetry `queue/`, `sent/`, and daily-rollup directories are not yet capped.
  These paths write no PII; the gate and retention caps are tracked follow-ups.
