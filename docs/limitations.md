# Limitations

RepoGrammar's current source identity is `0.4.2`; that manifest value does not
prove a matching tag, GitHub Release, artifact, or npm package exists. Public
availability requires the independent registry and finalizer evidence in the
stable checklist. The MCP API and bounded analyzers remain experimental. RepoGrammar
is designed to be conservative and local-first, not a sound general static
analyzer or a production-readiness claim.

## Release Availability

- Stable artifacts are available only after the exact `v0.4.1` GitHub release
  is public and immutable and npm `@sioyooo/repogrammar@0.4.1` is independently
  verified.
- The npm wrapper is available only after that exact immutable npm version is
  approved from staged publication; source manifests do not prove availability.
- Source-checkout dogfood is the safe contributor path before release and npm
  publication exist.
- The `0.4.2` candidate rollout and recovery states are tracked in
  `release/stable-v0.4.2-release-checklist.md`. The verified `0.4.1` and earlier
  preview records remain historical evidence.

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
- Rust support is a bounded v0.2 structural/framework preview covering
  self-dogfood roles plus serde, thiserror, tokio, clap, and axum families. It
  does not claim rustc-level or general Rust semantic analysis.
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

## Static Alignment Is Not Runtime Conformance

The `check` operation (CLI `check`, MCP `check_conformance`) returns a *static
alignment* certificate. It compares a target code unit's indexed feature profile
against a pattern family's source-backed constraint profile. It does **not**
prove that the target behaves equivalently at runtime.

A `STATICALLY_ALIGNED` result means only that the target matches every required
feature the family authority derived and exhibits no deviation and no blocking
unknown. It does **not** prove:

- runtime equivalence, identical control flow, or identical side effects — the
  certificate always reports `runtime_equivalence: UNKNOWN` and carries the
  family's runtime-equivalence obligation as an unresolved obligation;
- semantic correctness — structural token alignment is not a proof of behavior,
  and Tree-sitter-derived features are a candidate-generation layer, not a
  semantic oracle;
- coverage of dynamic behavior the analyzer marked `UNKNOWN` (dynamic imports,
  monkey-patching, fixture/dependency injection, framework magic);
- that a `PARTIAL_ALIGNMENT` or `unobserved_variation` result is a defect — an
  unobserved variation is a value the family has simply never seen, not an
  illegal one.

Static alignment is deliberately conservative rather than confident:

- **Observed-profile truncation.** Observed-profile enumerations are capped, so a
  target profile that is not among a *truncated* enumeration is reported as a
  `truncated_observation` (a partial signal), not `unobserved_variation` — "never
  observed" cannot be proven from a truncated set.
- **Blocking-unknown precedence.** When a target carries a blocking unknown, an
  absence-driven required check (a required value simply missing) does not become
  a `STATIC_DEVIATION`; it is a `blocking_suppressed_requirement` routing to
  `PARTIAL_ALIGNMENT`, because the blocking unknown may itself be why the feature
  is absent from the static view. Only presence-driven violations (a prohibited or
  wrong value that is definitely present) deviate under a blocking unknown.
- **Under-specified targets abstain.** A path-only `check` target that names a
  file with more than one family-eligible code unit is ambiguous and abstains with
  `INSUFFICIENT_EVIDENCE`; `check` never certifies an arbitrary unit on the user's
  behalf. Narrow the target with a `path:line`, `path:byte-range`, or `unit:`
  locator.

Because static alignment operates on the indexed generation, a stale target must
abstain rather than align: `check` reuses the freshness machinery and returns
`INSUFFICIENT_EVIDENCE` (with a `StaleEvidence` reason) for a target whose source
changed after indexing, instead of fabricating a deviation from stale facts. A
target with no comparison family, an ambiguous family key, or an unsupported role
also abstains with `INSUFFICIENT_EVIDENCE` and never surfaces a selected family.

- **Directory scope reads are bounded.** A directory-scope target resolves through
  a bounded, generation-consistent prefix read (a fixed cap on child files). A
  directory holding more files than the bound reports truncation explicitly, and
  because unseen files might belong to other families the resolver never claims a
  single family under truncation — it surfaces the seen families as candidate
  handles instead. Narrow to a smaller directory or a specific pattern family id.
  See `docs/specifications/query-resolution.md` for the full scope-resolution
  semantics.
- **A bare single-segment directory resolves only as a last-resort fallback, and
  only when it is a real indexed directory.** A multi-segment token (`backend/app`)
  is a directory scope directly. A bare bareword (`backend`, `src`) is kept a
  natural-language target for ranking; it resolves as a directory only through a
  fallback-time probe that runs **last**, after the exact/role/evidence layers and
  term retrieval have already abstained, and **only** when a bounded index read
  finds real files under it. A bareword that names no indexed directory — including
  every single-word concept or natural-language query — reads to zero files and
  stays `UNKNOWN`: the natural-language interpretation is never hijacked and no
  family is ever selected that the directory resolver cannot prove. This is a
  query-path fallback only; scoped readiness (below) still rejects barewords.

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

## Readiness Is Capability, Not Correctness

The `product_readiness` model (on `status`/`doctor` JSON and the MCP
`inspect_readiness` operation) reports only whether RepoGrammar can operate in
the current checkout. A `ready` summary means the active index is servable with
fresh family evidence; it does not prove that any family, static-alignment, or
runtime-equivalence claim is correct, that a query will find a supported family,
or that token savings are real. Its `measurement` dimension stays `NOT_MEASURED`.
The `static_alignment` dimension reports only that a fresh, ready family exists
to align against, never that an alignment holds at runtime. The
`top_blocking_unknowns` are triage buckets, not resolved analysis. A `degraded`
summary with a stale count is an honest freshness caveat, not an error.

- **Scoped readiness is bounded and path-like.** `repogrammar doctor
  --target/--within` and the MCP `inspect_readiness` scoped operation report a
  bounded, source-free queryability report for one directory/module scope. Its
  counts come from the same bounded directory-scope reads as the query path, so a
  scope holding more files than the bound reports `coverage: truncated` and the
  file/family counts are lower bounds. The scope must be path-like: a bare
  single-segment token (e.g. `pkg`) that carries no `/` or `.` is rejected by the
  shared path-safety authority and reads to an empty scope (`not_indexed`). Unlike
  the query path — whose bare-directory fallback probe resolves a bareword that
  turns out to be a real indexed directory — scoped readiness does **not** probe
  barewords; it keeps the strict path-like requirement. Scoped readiness never
  hydrates a family, reads no source content, and is capability-only in exactly the
  same sense as the whole-checkout readiness above.

## Token Savings

`estimated_potential_token_savings` is an estimated local read-displacement
diagnostic. It is not measured token savings and not a causal claim. Every value
uses a coarse bytes/4 token heuristic over indexed metadata and stored file
sizes; it is a rough potential estimate, not a tokenizer-accurate count.

The estimate is all-scope: it covers every indexed language and every
context-delivering outcome shape (found families, PARTIAL_CONTEXT read plans, and
committed or partial alignment certificates), not only Python found families.
Abstentions — typed `UNKNOWN`, out-of-scope certificates, and any resolved target
whose stored file size is unavailable — deliver no read displacement and record
no savings event; they are never counted as savings and never produce negative
accounting. They are counted only in the query denominator, so the stats panel
reports `savings_events / total_queries` honestly rather than implying every
query saved tokens.

Measured token-saving claims require paired baseline/treatment evidence with a
comparable measurement source and valid treatment correctness. The all-scope
estimate adds no new path to a measured claim; the paired-experiment recorder
remains the only source of measured savings.

## Agent-adoption evidence

The mechanics-only N=2 pilot is not an impact study. In its four treatment
runs, the RepoGrammar MCP server connected successfully but the small headless
model made `0/4` proactive `repogrammar_context` calls. This is an adoption
finding, not a product correctness failure or savings result. Current recording
instructions explicitly tell the agent to use RepoGrammar and must not be
described as spontaneous adoption. A larger, isolated, correctness-gated study
remains deferred.

## Telemetry

Anonymous telemetry is off by default. Telemetry must not include source code,
paths, repository names, symbols, prompts, query text, evidence text,
credentials, raw errors, diffs, or patches. Upload is explicit and separate
from local query diagnostics.

## Known Engineering Limitations

These are intentional current behaviors or tracked deferrals, not defects:

- **Found-family member lists are bounded.** Outside `--mode deep`, the inline
  `members` array in find/family responses (CLI JSON and MCP) is capped at the
  first 20 members in unchanged deterministic order to keep a large family (a
  recorded case had 123 members) from inflating a single response and amplifying
  compaction truncation. Family identity is metadata-first: the response always
  reports the true `member_count` and a `members_truncated` flag, and `--mode
  deep` returns the full list.
- **Consumer-side context compression is a recovery contract, not a prevention
  guarantee.** RepoGrammar cannot stop a client from truncating a response when
  it compacts its own context window. What it guarantees is a deterministic way
  back: the MCP server is read-only and stateless across calls (no cursor,
  session, or continuation token), and for a fixed active generation every
  `repogrammar_context` result is deterministic, so re-issuing the identical
  call returns the same bytes and recovers a truncated response. The recovery
  stays exact because `follow_up_family_ids` is a persistent, precise handle
  retained at every verbosity tier (including `minimal`); the 20-member inline
  cap and `verbosity: minimal` shrink the exposed surface so a single response
  is less likely to be truncated at all. If a `resync` or a background autosync
  `sync` activated a new generation between calls, handles may have changed and
  must be re-resolved rather than assumed byte-stable across generations. See
  `docs/specifications/mcp-api.md`.
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
  Autosync polling evaluates Git ignore with the same accepted-manifest policy as
  manual discovery (one batched `git check-ignore` subprocess per fingerprint
  pass), so Git-ignored supported files no longer count toward its file/byte
  ceilings and polling and manual `sync` agree on whether a repository is within
  limits.
  Narrow the repository root or exclude generated, dependency, build, and cache
  content when a ceiling is reached.
- **Autosync change detection is metadata-only, not content-hashed.** Idle
  polling fingerprints each supported file's path, size, modification time, and
  language rather than hashing its bytes, to keep the ~1s poll cheap. An edit
  that leaves both the size and the modification time unchanged is therefore
  invisible to polling and does not trigger a background `sync` until another
  change moves the fingerprint. This blind spot is intentional; a manual
  `repogrammar sync` (or any later size/mtime-visible change) recomputes content
  hashes authoritatively, so freshness is never silently claimed from the
  fingerprint alone.
- **Autosync is a repo-local detached process, not a global OS service.** Each
  repository needs its own initialized state and daemon. A reboot, explicit
  stop, crash, or managed command runner that reclaims detached descendants can
  leave the repository index readable but auto-sync stopped; use
  `repogrammar autosync status --project <path>` and restart it from a persistent
  user terminal when needed. CI and managed runners should normally initialize
  with `--no-autosync`.
- **Cross-version autosync step-down is best-effort, not a single-writer
  guarantee.** After a binary upgrade, a still-running older autosync daemon
  observes the newer engine's version stamp in the run state and steps down on
  its next poll. This is advisory and eventual, not immediate or exclusive:
  a newer daemon writes its version stamp only after its first successful sync,
  an older daemon can overwrite that stamp, and a same-poll race, an unparseable
  stamp, or a schema-gated stamp is invisible to the check. A daemon started
  explicitly (including a deliberate downgrade) reclaims the stamp to its own
  version so a persistent high-water version never locks it out. The authoritative
  mutual exclusion on index writes is the index lock acquired per build, not the
  version stamp; the stamp only reduces needless cross-version churn.
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
- **Source-inventory incremental sync is content-only for Rust and TS/JS, and
  interface-hash-gated for Python.** A content-only edit of a Rust or TS/JS source
  file (same path, changed hash, no add/remove) takes the incremental copy-forward
  path: those parsers consume only their own discovered path set plus root
  configuration, never another file's text, so exactly the edited file is
  reparsed. A content-only edit of a `.py` module also takes that fast path when
  its interface projection (top-level symbols, literal `__all__`, `__init__`
  re-exports) is unchanged — the interface is the only channel by which a module's
  text reaches another module's parse. The preflight decides this by comparing a
  freshly probed interface hash against a per-module hash persisted at build time
  (schema v10 `python_module_interfaces`). A changed interface falls back with
  `python_interface_changed`; an unverifiable one (worker error/timeout, or a
  build-time probe failure left no stored hash) falls back with
  `python_interface_unverified`; and a Python edit whose whole-project context
  payload approaches the worker's per-request byte cap on either the base or the
  current manifest falls back with `python_context_budget`, since the worker would
  silently drop that context and change how sibling modules parse. That budget is
  estimated from manifest sizes alone (no file reads) against a provable 6x
  worst-case JSON-escape bound, so even a control-char-dense module — whose bytes
  each escape to a six-byte `\uXXXX` sequence — cannot slip a real request past the
  cap; the conservative bound trades a wider full-rebuild band for soundness. Any
  add/remove/rename of a Python, TS/JS, or Rust source file (it changes the
  language's path set), any `conftest.py` edit (it
  alters ancestor fixture context), and any add/edit/remove of a project-config
  file still force a full-rebuild `sync`. That gate also forces a full rebuild when
  a Mocha runner config (`.mocharc.json/.jsonc/.cjs/.yml/.yaml`) changes, since
  these flip the global TS/JS test-runner flag, and when the base generation's
  stored engine version differs from the running binary, so a post-upgrade `sync`
  never copies forward facts produced by an older engine. A configured semantic
  worker still forces a full rebuild every run; the fast paths apply to worker-less
  operation. Every gate rule is guarded by the `repo-guard sync-equivalence`
  oracle. Java, C#, and C/C++ file-local edits and inventory-only tokens also take
  the incremental path (their parsers ignore project context). Two further items
  remain. First, this is the sound conservative core of the Python gate: an
  interface *change* falls back to a full rebuild rather than computing a
  reverse-import invalidation closure that would reparse only the affected
  importers. Second, a full build issues one extra bounded worker call per `.py`
  module to compute and store its interface hash, so a full rebuild's Python
  worker-spawn count is higher than before; folding this into the existing
  parse-document response is a future optimization. Adds/removes that only touch
  an isolated path also remain future work.
- **Token-saving readiness caps at partial.** The `token_saving_readiness`
  signal reports at most `partial` in `0.4.2`; a dedicated `ready`
  band is deferred.
- **Release checksums provide integrity, not authenticity.** Installers verify a
  `.sha256` fetched from the same release endpoint as the artifact. Signing and
  signature verification (or pinned digests) are deferred.
- **Indexing uses one write session per build; two follow-ups remain.** A build
  now persists a generation through a single write session: one connection with
  the write pragmas applied once and bounded-batch transactions, so connection
  opens drop from one-per-record to one-per-build. Two smaller items are still
  open. First, abandoned builds that committed rows are stamped terminal
  `failed`, but reclamation of `failed`, stale `building`, and old `validated`
  generations still requires manual `prune`/`compact`; the sync path does not
  auto-prune. Second, statements are issued directly rather than through a
  reused prepared-statement cache — per-statement caching is gated on enabling
  the SQLite driver's statement-cache feature and is a deferred optimization
  that the single-connection change does not require. The granular per-record
  store methods remain (each a one-shot session) for tests and the storage
  boundary.
- **Local metrics opt-out and retention are partial.** Local aggregate,
  source-free query-outcome and token-savings rollups do not yet re-check the
  `DO_NOT_TRACK`/`REPOGRAMMAR_TELEMETRY`/`CI` environment kill-switch, and the
  telemetry `queue/`, `sent/`, and daily-rollup directories are not yet capped.
  These paths write no PII; the gate and retention caps are tracked follow-ups.
- **Family ids are follow-up handles, not durable identities.** A family id is
  deterministic for a fixed input set and stable under unrelated file changes,
  but a cluster that is re-clustered under a different characteristic profile is
  reported as one removed and one added id, not an in-place rename. Multi-cluster
  keys carry a `v{hex}` characteristic-profile suffix, and genuinely
  indistinguishable sibling clusters fall back to a deterministic positional
  ordinal. Consumers must resync and re-resolve handles rather than persist an id
  as a permanent membership reference; sync/resync JSON surfaces the change via
  `families_added`/`families_removed`.
- **The `families` listing hash-verifies evidence at query time, not
  continuously.** `families` reads one bounded projection of the active
  generation's family evidence and hash-verifies each distinct evidence path once
  per invocation, marking each family `fresh`, `stale`, or `cannot_verify`. It
  does not re-mine, re-cluster, or repair anything: a `stale` family stays listed
  with its verdict and a report-level `StaleEvidence` signal that recovers via
  `run repogrammar resync`, rather than being recomputed in place.
- **Natural-language resolution uses a bounded, deterministic vocabulary.**
  Natural-language, synonym, and framework-plus-concept targets now resolve to a
  fresh family through deterministic term retrieval (a committed alias/concept
  vocabulary with no LLM, embedding, or network dependency); they no longer always
  resolve to zero candidates. Resolution requires the top candidate to clear an
  absolute score floor (in practice a framework filter plus a pattern concept, or
  a concept plus enough evidence-token matches) and carry a pattern-concept signal,
  and to be a single family clearly ahead of any competitor that also clears the
  floor. A bare framework name, a bare concept, an unrecognised token (including
  typos), a genuinely ambiguous target, or an unsupported concept still abstains
  with a typed `UNKNOWN` and a low-cardinality route reason. Because scoring is a
  pure function of the normalized query, targets that normalize identically share
  one outcome, so some natural-language phrasings deliberately abstain rather than
  risk a false family. See `docs/specifications/query-resolution.md`.
- **Term retrieval is skipped for path-locator-shaped targets, judged before
  normalization.** A target is treated as a file locator — and routed to the
  exact/local-context path instead of term retrieval — when a whitespace token
  contains `/` or ends in a known indexed source-file extension (`.py`, `.ts`,
  `.js`, `.rs`, `.java`, `.cs`, `.go`, `.cpp`, …). A single interior-dotted word
  in prose (`fastapi.Depends`, a version like `0.100`, or `e.g.`) is not treated
  as a locator, so such phrasings still reach term retrieval. Conversely, a bare
  filename such as `app.py` is a locator and never reaches term retrieval even
  when phrased as a question.
- **Constraint-profile hydration is not pinned to the family's generation.** A
  family lookup loads the matched family (evidence, members, slots) on one active-
  generation store read and then hydrates its `constraint_profile` on a second,
  independent active-generation read (`show_family_constraint_profile` takes no
  generation id). If a resync activates a new generation between the two reads,
  the detail can pair one generation's evidence with another generation's profile;
  the mismatch is self-limiting — representative ids that match no evidence simply
  yield zero per-dimension variation mapping and the profile is treated as
  absent-shaped — but it is a real time-of-check/time-of-use window. This matches
  the repository's existing list-then-show multi-open pattern; a same-snapshot
  profile read (pinning hydration to `ActiveFamily.generation_id`) is a tracked
  follow-up.
