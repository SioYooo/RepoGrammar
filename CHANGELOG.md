# Changelog

## Unreleased

### Added

- Added an optional `against` input to the two-sided static-alignment operations
  `explain_deviation` / `check_conformance` (CLI `explain` / `check`, `--against`).
  It names the COMPARISON family and pins it to exactly one fresh ready family (an
  exact `family:` id, framework role, or pattern); an ambiguous or unmatched
  `against` abstains with `INSUFFICIENT_EVIDENCE`, `selected_family_id: null`, and
  bounded candidate handles, never a false selection. `against` is additive to the
  closed input schema (every existing `target`-only call is byte-compatible) and is
  rejected — never silently ignored — on any other operation/command. See ADR-0029
  (Phase 4 note).

- Added an optional `target`/`within` (a directory/module scope) to the MCP
  `inspect_readiness` operation and a matching `--target`/`--within` to
  `repogrammar doctor`, returning a bounded, source-free SCOPED queryability
  report over just that scope: a `summary` token, a `queryability` verdict
  (`queryable`/`partial_context`/`degraded`/`not_indexed`/`not_ready`/
  `cannot_verify`), a `scope` object (safe-prefix count, indexed-file count and
  coverage bucket, truncation flag, languages present, count of families whose
  evidence occupies the scope, and freshness), `providers`, and one `recovery`
  action. It reuses the shared target-resolution vocabulary and the bounded
  directory-scope read/family-mapping ports, but only COUNTS: it hydrates no
  family, reads no source content, and records no family-query telemetry. Every
  field is a low-cardinality enum/count/language token — no raw target, path, or
  symbol appears. The no-target `inspect_readiness`/`doctor` output is unchanged
  (the whole-checkout `readiness` object and the scoped `scoped_readiness` object
  are mutually exclusive and carried under distinct keys), and the response stays
  on `product-schemas.v1` (see ADR-0029). A bare single-segment token that carries
  no `/` or `.` is not a path-like scope and reads to an empty scope.
- Added an additive top-level `resolution` object to `find_analogues` /
  `explain_deviation` (CLI `find`/`explain`) responses that resolve a directory or
  composite scope. It projects the candidate-set cardinality
  (`one`/`many`/`none`/`truncated`) plus bounded, source-free `{family_id, summary}`
  candidate handles, so a multi-family scope surfaces every real in-scope family
  instead of collapsing into a generic `UNKNOWN`. The cardinality token is
  telemetry-safe and the candidate summaries are projected from the committed
  family search-summary projection (never a hydrated deep family, never raw
  source). No new top-level status token was introduced — the response stays on
  `product-schemas.v1` (see ADR-0029). `resolution` renders at `standard`/`full`
  and is dropped at `minimal`, where the candidate `family_id`s remain available as
  narrowing handles on `query_route.follow_up_family_ids`.

### Changed

- Changed `explain_deviation` (CLI `explain`) from a `find_analogues` alias into a
  real deviation projection: it now runs the same two-sided static-alignment
  resolution as `check_conformance`, resolving the subject to exactly one code unit
  and one comparison family, and always reports a real `target_relationship`
  (`MEMBER` / `LEGAL_VARIATION` / `NEAR_MISS` / `EXCEPTION` / `BLOCKED_UNKNOWN` /
  `OUT_OF_SCOPE` / `INCOMPATIBILITY`; `COMPETING_PATTERN` stays reserved) with
  `runtime_equivalence: "UNKNOWN"`. A target that cannot be pinned to one unit + one
  family (ambiguous, stale, unindexed, out of scope, or family-less) now abstains
  with a typed `INSUFFICIENT_EVIDENCE`/`UNKNOWN` and `selected_family_id: null`
  instead of returning fuzzy family context. `find_analogues` is unchanged. This is
  a user-visible contract change; affected fixtures were regenerated deliberately.
- Changed a directory/composite scope that resolves to more than one in-scope
  pattern family (or to a truncated bounded read) from a typed `UNKNOWN` to a
  `PARTIAL_CONTEXT` outcome carrying `resolution.cardinality: "many"` /
  `"truncated"`. A single proven in-scope family remains `FOUND`
  (`resolution.cardinality: "one"`) and a resolved-but-familyless scope remains
  `PARTIAL_CONTEXT` (`resolution.cardinality: "none"`). A `many`/`none`/`truncated`
  resolution never carries a `selected_family_id` — committed family precision is
  unchanged (a family is selected only when exactly one high-confidence family
  resolves). Non-scope outcomes are byte-unchanged.

## 0.4.0 — 2026-07-20 stable channel

RepoGrammar `0.4.0` is the forward-only Build Week stable candidate. The
retained `v0.3.2` tag and 11-asset private draft are bound to the earlier
`26ce59e` source; its release workflow was cancelled before the protected npm
staging job ran. The tag, draft, and candidate bytes remain historical audit
evidence and are not moved, replaced, published, or reused. Current source
changes therefore advance to the unoccupied `0.4.0` identity.

This source record does not prove that the `v0.4.0` tag, immutable GitHub
Release, npm package, provenance, dist-tags, or public finalizer exist. Those
facts must be independently recorded only after the canonical stable checklist
completes. The release makes no production-readiness, 1.0 API-stability,
sound-analysis, runtime-equivalence, Windows-support, hallucination-prevention,
or measured-token-savings claim.

### Added

- Added source-free, all-language query-outcome accounting and all-scope
  estimated potential token-savings breakdowns. The atomic v2 cohort keeps each
  savings event paired with its query denominator and serializes concurrent
  writers without importing legacy unpaired evidence.
- Added qualified concept-phrase routing for specific family queries while
  retaining conservative `UNKNOWN` and `PARTIAL_CONTEXT` behavior.
- Added a judge-first no-Rust/Cargo path and a pinned real-repository recording
  runbook that make bounded read obligations, static-alignment limits, typed
  abstention, and the mechanics-only `0/4` proactive MCP-adoption result
  explicit.

### Changed

- Made standalone `repogrammar init` start its repo-local auto-sync daemon after
  the default resync succeeds. `--no-autosync` provides an explicit CI,
  experiment, and one-shot opt-out; `--autosync` remains a compatible explicit
  spelling, and `--state-only` remains daemon-free.
- Made managed agent instructions precision-first so covered repository
  contract lookups use RepoGrammar before broad source search.
- Reused the Python parse-interface hash across compatible indexing work and
  retained the active generation when an incremental sync proves that source
  and project context are unchanged.
- Advanced Cargo, npm, release-source, installer, workflow, finalizer, and
  stable-checklist authority to `0.4.0`. The historical
  `preview=0.2.0-preview.0` dist-tag remains unchanged.
- Python remains the official v0.1 family scope. TypeScript/JavaScript, Rust,
  Java, C#, and C/C++ remain bounded preview scopes; their source-free readiness
  and query-savings records are reported separately rather than promoted to an
  official support claim.

### Fixed

- Kept top-level `stats` `family_count` inside the Python official-family scope
  instead of combining an all-language family total with Python-only eligible
  units and coverage. Per-language preview counts and all-scope savings remain
  available under `stats --json`.
- Made source installation verify the running MCP contract and accept the
  current Claude Code absence probe without weakening ownership checks.
- Removed stale public-release evidence that incorrectly associated the
  `v0.2.2` finalizer run `29591027524` with the unpublished `v0.3.2`
  candidate.
- Pinned every `dtolnay/rust-toolchain` workflow step to reviewed commit
  `4cda84d5c5c54efe2404f9d843567869ab1699d4`, kept the requested toolchain
  explicit, and added a repository-guard regression so a mutable third-party
  action ref cannot silently return.

## 0.3.2 — 2026-07-19 unpublished stable candidate

RepoGrammar `0.3.2` is the second verifier-fix patch-forward, succeeding the
retained, unpublished `0.3.0` and `0.3.1` candidates. Its product behavior is
identical to both; the only substantive change is the release-gate assertion
fix below, plus the version advance and the release-mechanism pins. It makes no
production-readiness, 1.0 API-stability, stable-MCP-API, sound-analysis,
measured-token-savings, runtime-equivalence, Windows-support, or
expanded-language-support claim. Registry availability must be verified
independently; this source record does not prove that GitHub or npm publication
completed.

### Fixed

- Updated the installer test's regex-escaped release assertions. The `v0.3.1`
  tag run's Verify release gate failed because
  `src/install/repogrammar-install.test.sh` still pinned the stable staging
  command, the finalizer verify markers, and the npm package reference with
  regex-escaped `0\.2\.2` literals that both prior version advances'
  plain-text audits could not match; no draft or npm stage was produced. The
  four escaped assertions now pin `0.3.2`, and the release-source audit
  procedure covers regex-escaped version forms.

### Changed

- Advanced the stable release authority, finalizer, and install/documentation
  pins to `v0.3.2`, and registered the retained, unpublished `v0.3.1`
  candidate among the failed stable versions that must never appear in the
  registry inventory alongside `0.2.0`, `0.2.1`, and `0.3.0`.

## 0.3.1 — 2026-07-19 unpublished stable candidate

RepoGrammar `0.3.1` is the verifier-fix patch-forward that succeeds the retained,
unpublished `0.3.0` candidate. Its `v0.3.1` tag run failed at the release-gate
verifier's own installer test and was never published; the tag is retained and
non-reusable, and `0.3.2` is the patch-forward (see
`docs/release/stable-v0.3.1-release-checklist.md`).
Its product behavior is identical to `0.3.0`; the
only substantive change is the release-gate verifier fix below, plus the version
advance and the release-mechanism pins. It makes no production-readiness, 1.0
API-stability, stable-MCP-API, sound-analysis, measured-token-savings,
runtime-equivalence, Windows-support, or expanded-language-support claim.
Registry availability must be verified independently; this source record does not
prove that GitHub or npm publication completed.

### Fixed

- Made the end-to-end payload-measure smoke self-build the product `repogrammar`
  binary from the current workspace — `cargo build --bin repogrammar`, using the
  executable path cargo itself reports — instead of requiring a sibling artifact
  next to the test harness. The `0.3.0` tag run's Verify release gate panicked
  under a fresh CI `cargo test --locked --workspace --all-features`, which does
  not emit a standalone `target/<profile>/repogrammar` reachable from the
  deps-hashed harness path, so the test's precondition never held and no draft
  or npm stage was ever produced. Self-building always measures fresh current
  source and closes the recurring stale/missing sibling-binary hazard.

### Changed

- Advanced the stable release authority, finalizer, and install/documentation
  pins to `v0.3.1`, and registered the retained, unpublished `v0.3.0` candidate
  among the failed stable versions that must never appear in the registry
  inventory. The published `0.2.2` release and the retained `v0.2.0`/`v0.2.1`
  candidate tags are unchanged.

## 0.3.0 — 2026-07-19 unpublished stable candidate

RepoGrammar `0.3.0` is the stable-channel candidate that succeeds the published
`0.2.2`, intended to become the next published stable-channel pre-1.0 release.
Its `v0.3.0` tag run failed at the release-gate verifier and was never published;
the tag is retained and non-reusable, and `0.3.1` is the patch-forward (see
`docs/release/stable-v0.3.0-release-checklist.md`). It makes no
production-readiness, 1.0 API-stability, stable-MCP-API, sound-analysis,
measured-token-savings, runtime-equivalence, Windows-support, or
expanded-language-support claim. Registry availability must be verified
independently; this source record does not prove that GitHub or npm publication
completed.

### Changed

- Synchronized Cargo, Cargo lockfile, npm manifest, installers, launchers, and
  current install documentation on `0.3.0`. Historical `0.2.0-preview.0`
  evidence remains historical and the npm `preview` dist-tag must stay on that
  immutable prerelease.
- Advanced the stable release authority and finalizer to `v0.3.0`. The published
  `0.2.2` release and the retained `v0.2.0`/`v0.2.1` candidate tags are
  unchanged, and neither failed unpublished version may appear in the stable
  registry inventory.

### Fixed

- Made the Python context-budget gate's escape headroom a provable upper
  bound. JSON-escaping expands a source byte by at most 6x (a
  short-escape-less control character becomes `\uXXXX`), so the
  incremental-sync budget check now multiplies manifest sizes by 6
  instead of 2 — sealing the channel where control-character-dense
  Python sources could pass the gate, exceed the frontend request cap,
  and silently drop whole-project context on the incremental path. The
  sync-equivalence oracle grows a fourteenth scenario that edits an
  escape-heavy module across the budget boundary and was adversarially
  proven to fail under the old 2x coefficient. Projects whose aggregate
  Python context estimate stays under about 170 KiB (the vast majority)
  see no change; estimates between 170 KiB and 512 KiB now fall back to
  a full rebuild on interface-stable Python edits, a performance-only
  trade for soundness.

- Stopped the Equal constraint axis from fabricating a certain deviation
  out of uncertainty. A conformance target carrying a strict subset of a
  multi-value Equal constraint's expected values (missing values, no
  offending value present) is now classified as absence-driven, so under
  a blocking unknown it downgrades to `blocking_suppressed_requirement`
  and `PARTIAL_ALIGNMENT` instead of a fabricated `required_mismatch`
  `STATIC_DEVIATION` — matching the MustContain axis and the invariant
  that an UNKNOWN never becomes a definite verdict. Any offending value
  outside the expected set still deviates, with or without blocking
  unknowns. Abstaining alignment certificates (`UNKNOWN`,
  `INSUFFICIENT_EVIDENCE`) are additionally guaranteed by construction
  to carry no selected family and no computation body.

- Aligned the autosync change fingerprint with the manual discovery
  manifest. The fingerprint now evaluates gitignore through one batched
  git check-ignore subprocess per pass (an ad-hoc local measurement
  observed roughly ten milliseconds for six hundred candidates, a small
  fraction of the default poll interval, but this is not a committed,
  reproducible benchmark) with the same warning fallback as discovery,
  and git-ignored supported files no longer trigger spurious syncs or
  count toward the
  fingerprint's accepted-limit ceilings, so autosync and manual sync
  agree on whether a repository fits the accepted limits. Skipped-path
  counts are logged as bounded path-free counters, and the remaining
  metadata-only blind spot (same-size same-mtime edits) is documented
  explicitly.

### Added

- Hardened cross-version compatibility around locks, daemons, and
  schema versions. A preview-era index lock whose record lacks the
  `host` field is now classified as a distinct legacy state (not
  undecidable): `unlock --force --yes` may remove it, printing every
  recoverable provenance field plus a prominent warning when a process
  with the recorded pid still appears alive (host-less ownership can
  never be soundly proven dead), while doctor reports
  `INDEX_LOCK_LEGACY` with the exact recovery command and the same
  caveat; modern locks with undecidable owners remain refused, and
  acquire never auto-removes any lock. The autosync daemon stamps its
  version into the run state additively and steps down when a strictly
  newer version has written the state after its own startup — a
  best-effort early exit, honestly documented as advisory (index-write
  mutual exclusion stays with the index lock), with the startup
  reclaiming the stamp so a deliberate binary downgrade is never
  permanently locked out. Databases from a future schema version are
  proven fail-closed by new tests: the read gate reports an invalid
  state rather than the recoverable-outdated path, and the
  mutable-store rebuild never deletes them. The MCP spec and
  limitations now document recovery from consumer-side response
  truncation: responses are deterministic and stateless for a fixed
  active generation, so re-issuing the identical call recovers a
  truncated result, with `follow_up_family_ids` as the persistent
  handle — re-resolve handles if a resync or background autosync
  activated a new generation between calls.

- Added a `verbosity` request parameter (`minimal | standard | full`,
  default `standard`) to the CLI query flags and the MCP
  `repogrammar_context` input, additive under `product-schemas.v1` and
  orthogonal to `--mode`: `mode` selects how much evidence the
  resolver gathers, `verbosity` selects the response field density the
  serializers emit. `standard` (the default) and `full` are
  byte-identical to this development line's pre-precision response
  (golden tests pin them to captured pre-change bytes on both surfaces)
  — not to v0.2.2, because that pre-precision baseline was captured
  after the inline-member cap of 20 (a declared v0.2.2 default-shape
  change; see its own CHANGELOG entry). Relative to v0.2.2 the only
  default-tier changes are that member cap plus purely additive new
  fields. Precision slices suppress demoted diagnostic fields only at
  `minimal`. Invalid values are explicit protocol errors, never a
  silent fallback.

- Realized the lean `verbosity: minimal` tier across the query
  serializers, additive under `product-schemas.v1` with `standard` and
  `full` proven byte-stable by the payload-measure harness (44 of 44
  standard/full matrix rows byte-identical before vs after). At
  `minimal`: the `query_route` envelope keeps only `route` and
  `follow_up_family_ids` (a normalized superset of the candidate and
  selected ids, so no handle is lost; `candidate_family_ids` stays on
  `PARTIAL_CONTEXT`/`UNKNOWN`/conformance abstentions as a narrowing
  recovery handle); `resolved_target` drops the `original_target` input
  echo, normalizer residue, and pinned-target candidate echoes while
  retaining candidates whenever resolution was ambiguous; the alignment
  certificate drops the duplicate `alignment_status` token (the
  top-level `selected_family_id` is kept at every tier as the
  authoritative selected-family carrier, and `runtime_equivalence:
  "UNKNOWN"` is never removed); the read plan gains honest `truncated`
  and `item_count` flags, items whose content is already rendered into
  `source_spans` collapse to a back-reference, and the empty
  `source_spans` stub disappears. Deviation and variation lists on
  alignment computations are capped at 24 entries at every tier with
  `_truncated`/`_count` flags on actual truncation only. Measured on
  the committed payload fixture: the minimal tier is 16,351 bytes
  (14.2%) smaller than standard across the matrix, with abstention
  responses 58-67% smaller; savings figures cite the two-run
  payload-measure byte table.

- Added a `repo-guard payload-measure` subcommand and a committed
  deterministic payload fixture (a 31-member FastAPI family plus small
  SQLAlchemy/Pydantic/pytest/Express families and a below-support file)
  that indexes in an isolated temporary workspace and emits a stable
  per-operation, per-verbosity response-byte table (67 matrix rows,
  including source-span variants). Two runs are byte-identical by
  construction and by test; any payload-savings claim must cite the
  diff of two runs.

- Extended estimated-potential token-savings accounting to every
  context-delivering outcome. A single estimator authority records an
  ESTIMATED event for found, partial-context, and committed-alignment
  responses (savings = max(0, baseline - returned) under the existing
  bytes/4 heuristic and caveat); abstentions and targets with no stored
  size record no event rather than a guess. The local telemetry rollup
  gains additive `by_outcome_shape` and `by_language` breakdowns behind
  closed vocabularies (out-of-vocabulary languages coerce explicitly to
  `unknown`, never dropped silently), `stats --json` reports an
  `all_scope_token_savings` block with an honest
  events-over-total-queries coverage ratio, and partial-context and
  alignment responses now carry the estimate block on CLI and MCP —
  with an explicit null and `unavailable_reason` when no estimate is
  possible. Measured-savings claims remain exclusive to paired
  experiments; nothing in this accounting can produce one.

- Extended dependency-aware incremental sync to Python through stored
  module interface hashes. Each build records a deterministic interface
  hash per non-conftest Python module (schema v10
  `python_module_interfaces` table, `extract_interface` worker mode),
  and a content-only Python module edit now reparses just the touched
  files when every modified module's interface hash is unchanged.
  Interface-affecting edits, conftest/config changes, adds/removes,
  missing stored hashes (`python_interface_unverified`), and any
  manifest whose estimated whole-project context payload approaches the
  frontend request cap on either the base or current side
  (`python_context_budget`, a conservative sizes-only bound with 2x
  escaping headroom) fall back to a full rebuild, so copied-forward
  facts can never diverge from a clean rebuild through the
  context-omission channel. The interface probe stops at the first
  unverified module, and the sync-equivalence oracle grows to 13
  scenarios covering both gate directions.

- Decomposed product readiness and stamped schema versions on every
  structured payload. A single readiness authority reports
  independently-truthful dimensions - repository state, active index,
  family evidence freshness counts, prevalence counts, query retrieval,
  static alignment, providers, autosync, and measurement status (always
  NOT_MEASURED without a paired experiment) - plus the top blocking
  UNKNOWN mechanisms and exactly one recovery action from the shared
  classifier. The summary token (ready, degraded, not_ready) is a pure
  projection of the same recovery authority the query path consumes, so
  a ready summary provably implies query-preflight readiness (a
  property test enforces it, closing the baseline defect where status
  reported query_ready while every family claim was stale-blocked);
  unreadable stores yield null dimensions rather than definite tokens.
  The MCP surface gains an inspect_readiness operation on the existing
  single tool, and CLI find/family/families/member/explain/check/
  status/doctor/stats plus all MCP results carry
  schema_version product-schemas.v1 under a documented additive
  pre-1.0 compatibility policy.

### Changed

- Bounded inline family member lists and condensed the human stats
  output. find/family/check responses now cap the inline `members`
  array at 20 entries in deterministic order outside `--mode deep`,
  with an honest `member_count` total and `members_truncated` flag
  (`--mode deep` restores the full list) — the prior unbounded form
  returned 75 KB for a 123-member family. The human `stats` panel
  shrinks to a ~10-line lead block (readiness, inventory, family
  coverage, the all-scope savings headline, scope note, and a `--json`
  pointer); every previously emitted field remains available under
  `stats --json`.

- Narrowed the incremental-sync full-rebuild gate for content-only Rust
  and TypeScript/JavaScript source edits. Non-Python parser contexts
  carry no cross-file source text (path sets, nearest Cargo.toml
  features, and root TS/JS config only), so a content-only modification
  of a .rs/.ts/.tsx/.js/.jsx source now takes the incremental path with
  exactly one file reparsed, decided by the discovered-language
  classifier; adds, removes, renames, config files, and every Python
  change keep falling back to the full rebuild, and a configured
  semantic worker still forces a full rebuild. The sync-equivalence
  oracle grew to eleven scenarios - content-only Rust and TS edits
  prove EQUAL against clean full rebuilds, and source additions prove
  the fallback - all passing.

- Rewrote the storage write lifecycle around generation write sessions.
  A generation build now uses one writer connection with bounded
  2000-row immediate transactions and explicit phase checkpoints (after
  the file/unit/IR phase, the semantic-fact phase, and the family
  phase, each committing the open batch and running a passive WAL
  checkpoint), replacing the per-record connection-open pattern. Every
  per-record validation is preserved on the session connection and
  generation validation gains a set-wide code-unit/file conformance
  scan; whole-database integrity checking runs once per sync instead of
  twice; abandoned builds that committed at least one batch are stamped
  with the terminal failed status while post-finish validation or
  activation failures truthfully leave a building generation for prune;
  maintenance errors after sealing are non-fatal and finish after
  abandon is a typed error. Measured at fixture scale with real
  instrumentation: one connection open instead of one per record, and
  a 6,200-record build dropping from 5.2 s on the granular per-record
  API to 0.26 s in the session, with crash and fault injection tests
  proving the active generation stays readable and unchanged.

### Added

- Upgraded check/check_conformance from an advisory no-op into
  source-backed static alignment certificates. The target resolves to
  exactly one code unit (unit ids and path:line / byte-range locators
  pin the innermost containing unit; a path-only target with several
  eligible units abstains with candidate unit ids; there is no silent
  canonical fallback), its freshness is verified before any
  certificate, and its features are extracted by the same authority
  family induction uses. Certificates compare the target against the
  family constraint profile per semantics token and report
  STATICALLY_ALIGNED, STATIC_DEVIATION, PARTIAL_ALIGNMENT,
  INSUFFICIENT_EVIDENCE, or UNKNOWN with matched constraints,
  deviations, legal observed variations, blocking unknowns, unresolved
  runtime obligations, and a target relationship (member, near-miss,
  exception on source-backed negative evidence, blocked-unknown,
  out-of-scope); absence-driven requirement failures under a blocking
  unknown report PARTIAL_ALIGNMENT rather than a fabricated deviation,
  truncated variation enumerations never claim a profile was never
  observed, abstaining certificates never surface a selected family,
  and runtime equivalence remains the literal UNKNOWN in every
  certificate.

### Fixed

- Closed two incremental-sync correctness gaps and added the
  sync-equivalence oracle that proves incremental builds equal a clean
  full rebuild. Mocha runner configs (`.mocharc.json/.jsonc/.cjs/.yml/
  .yaml`) now force the full project-context rebuild their global
  runner-flag effect requires, and sync preflight compares the base
  generation's recorded engine version, falling back to a full rebuild
  with an explicit `engine_version_changed` reason after an upgrade.
  The new `repo-guard sync-equivalence` harness compares an incremental
  sync against an independent clean rebuild across ten canonicalized
  surfaces (files, units, IR graph, families, deep family evidence,
  unknown inventory, repo-shape stats, local and provider semantic
  facts) with per-scenario expected outcomes and fallback reasons, so a
  regressed gate or misfiring preflight fails the suite instead of
  passing vacuously; all eight committed scenarios pass, proving the
  currently-incremental java/csharp paths equivalent.

### Added

- Wired constraint-profile persistence into production indexing and
  replaced storage-order representative selection with deterministic
  contrastive selection. The canonical member is now the cluster medoid
  (minimum summed symmetric-difference distance over the member feature
  map, ties broken by a path-free feature fingerprint so two-member
  families no longer flip canonical on renames), the
  farthest-from-medoid member carries a new `contrast` covered-claim
  label that the read plan's support span prefers, and variation
  witnesses cover every observed profile per constraint dimension plus
  the Python anchor-target dimension. Query-time evidence selection
  requires one witness per variation dimension when a constraint
  profile is hydrated, letting the canonical satisfy a dimension it
  solely represents, and family detail on CLI and MCP exposes a
  metadata-only `constraint_profile` object.

- Derived source-backed family constraint profiles at storage schema v9.
  Every emitted family now records a FamilyConstraintProfile - required
  features under a four-token semantics discriminator (Equal,
  EqualEmpty for equal-but-empty dimensions, MustContain for the
  support-family core, ProhibitedPresence on the prohibited axis with
  read-and-write axis validation), observed-only variation constraints
  that agree dimension-for-dimension with the co-persisted variation
  slots, and the claim's typed unknown obligations - derived exclusively
  from the existing compatibility authorities with drift-guard tests on
  both hand-maintained mirrors. Profiles persist through a dedicated
  constraint-profile store into a validated deterministic column, are
  hydration-validated including tampering rejection, and are not yet
  exposed on query or interface surfaces; production wiring lands with
  the alignment slice.

- Added evaluation run conditions and an honest naive control to the
  product-core harness: `repo-guard product-eval` accepts `--condition
  <token>` and `--baseline token-overlap`, results carry explicit
  `condition` and `baseline` fields, and a new
  `selected_on_abstention_gold` safety counter reports selections on
  queries whose gold outcome is abstention for every condition.
  Reciprocal rank now scores the committed answer only - abstentions
  contribute zero for all conditions - and candidate metrics evaluate at
  most the first five candidates, so cross-condition MRR comparisons no
  longer credit uncommitted candidate lists. Measured on the corpus
  after integration: product hit@1 21/42 with mrr 0.500 and zero
  abstention-gold selections; the token-overlap control reaches hit@1
  11/42 with 4 abstention-gold selections.

### Changed

- Stabilized semantic family identity (pre-1.0 breaking change to the family-id
  format for multi-cluster keys). A `FamilyKey` with two or more ready clusters
  now gives every cluster a `v{hex}` suffix derived from the cluster's
  characteristic profile (the feature values the role requires equal across
  members), so no cluster holds the bare base id and adding a path-earlier file
  can no longer silently re-point it; single-ready-cluster keys keep their bare
  base id unchanged. Sibling clusters that share a characteristic profile are
  distinguished by their shared support-family core, then by a deterministic
  positional ordinal (recorded as classification-independent metadata) for
  genuinely indistinguishable clusters, and lossy `stable_token` base-id
  collisions across distinct keys are disambiguated deterministically with
  uniqueness asserted at build time. The legacy membership-union `cluster_...`
  suffix is replaced. Run `repogrammar resync` after upgrading so stored family
  ids for multi-cluster keys are rewritten to the new format. Sync and resync
  JSON now also report `families_added` and `families_removed` (bounded, sorted
  samples plus counts) by diffing the base and new generations' family-id sets;
  they are `null` when there is no base generation or the sync did not recompute
  families.
- Separated family support from dominance. Minimum support no longer implies a
  `DOMINANT_PATTERN` label: every emitted family now carries an evidence-backed
  `FamilyPrevalence` record (eligible/blocked/unsupported peer counts, coverage
  ratio, competing ready-family counts, and a deterministic reason) and is
  classified with the four-token prevalence vocabulary `DOMINANT_PATTERN`,
  `SUPPORTED_PATTERN`, `MINORITY_PATTERN`, and `UNKNOWN_PREVALENCE`, decided on
  exact integer thresholds. The prevalence object is exposed on the families
  list, family detail (CLI JSON, MCP, and human), and find/check payloads.
- Bumped the storage schema to version `8`: the `families` table gains the
  prevalence columns and its classification `CHECK` moves to the four-token
  vocabulary. Reads against a pre-`8` database now fail with a typed
  schema-outdated error whose recovery is `repogrammar resync`, and the
  full-rebuild path recreates the repo-local mutable database (deleting only
  `.repogrammar/repogrammar.sqlite` and its WAL/SHM sidecars) because the
  create-if-not-exists DDL cannot add columns in place.

### Added

- Routed the deterministic term-retrieval substrate into the production fuzzy
  lookup path so natural-language, synonym, and framework-plus-concept queries now
  resolve to a fresh family with calibrated abstention — no LLM, embedding, or
  network dependency. Term retrieval runs **only** when the exact authority layers
  (exact family id, `unit:` member id, exact role, exact `//`-suffix path) and the
  role/evidence fuzzy layer produce **no candidate at all** (a single `query
  target` `InsufficientSupport` block with empty candidate ids) for a target that
  is not path-locator-shaped; exact-layer candidate-set and ambiguity abstentions
  keep their own claim, candidate ids, and narrowing recovery verbatim. A target
  is path-shaped only when a whitespace token contains `/` or ends in a known
  source-file extension, so prose with an interior-dotted word (`fastapi.Depends`,
  `0.100`, `e.g.`) still reaches retrieval. Normalization scores the source-free
  family search projection and applies named abstention gates
  (`MIN_RETRIEVAL_SCORE = 10`, `MIN_RETRIEVAL_MARGIN = 1`, hydration bounded by the
  defensive `MAX_RETRIEVAL_HYDRATIONS = 5`): a selection requires the top candidate
  to clear the additive absolute floor and carry a pattern-concept signal, and to
  beat any competing family that also clears the floor. Bare frameworks, bare
  concepts, typos, truncated ties, stale candidates, and genuinely ambiguous
  targets abstain with a typed `UNKNOWN` and a low-cardinality `abstention_reason`
  (`no_candidate`, `below_min_score`, `unsupported_target`, `margin_too_close`,
  `truncated_tie`, `stale_candidates`, `hydration_ambiguous`). Calibrated on the
  79-query product-eval corpus (42 retrieval + 25 abstention + 12 context), this
  raises hit@1 to 21/42 (from the pre-routing 17/43) while holding zero
  false-family selections, 25/25 correct abstentions, 4/4 unsupported rejections,
  6/6 ambiguity precision, and 13/14 candidate recall with no regression among
  previously-matching exact/context queries. The committed retrieval vocabulary
  also treats `repository` as a data-access concept (previously shadowed by a
  stopword) and `model` as both a validation-model and a data-access concept, so
  repository-worded queries resolve and bare `models` stays genuinely ambiguous.
  The `query_route` report (CLI JSON, MCP, and human) gains source-free
  term-retrieval metadata (`hydrated_family_count`, `retrieval_stage_count`, and a
  `term_retrieval` object with route token, counts, bucketed scores, matched
  signals, truncation flag, and abstention reason), and anonymous telemetry gains
  the enum-only `by_abstention_reason` rollup dimension (kept in lockstep with the
  reason enum by a build-time equality test). No raw target text is ever persisted.
- Committed the deterministic product-core evaluation harness: a report-only
  `repo-guard product-eval` command that indexes committed fixtures in isolated
  temporary workspaces and drives the product binary through
  `src/fixtures/evaluation/query-corpus-v1.json`, writing machine-readable
  output. The Phase 2 corpus expands to 73 gold-labeled queries (later grown to
  79 with 12 context queries as alignment and context coverage landed; the
  committed corpus file is authoritative) over python-v0_1,
  typescript-v0_2, and a zero-family fixture, each tagged with a measurement
  `intent` (`retrieval`/`abstention`/`context`) and, where relevant, a
  `candidates_include` gold set; new coverage adds `path:line`/`path:start-end`
  local-context locators, bare framework names, concept synonyms, natural-language
  paraphrases (including TypeScript framework questions), ambiguous route/test
  questions, unsupported-language questions, and unsafe typo inputs. The corpus
  schema stays backward-compatible `product-eval-corpus.v1` (new optional fields);
  the results schema bumps to `product-eval-results.v2`, adding retrieval metrics
  (`hit_at_1`, `candidate_recall`, `mrr`, `correct_abstention_rate`,
  `false_family_rate`, `unsupported_rejection_rate`, `ambiguity_precision`) with
  audit-friendly numerator/denominator counts, per-intent totals, and per-query
  `intent`/`reciprocal_rank` plus null-tolerant `hydrated_family_count` and
  `retrieval_stage_count` placeholders for a later wave. The recorded baseline at
  `docs/experiments/product-core-baseline.md` shows every exact
  id/member/path/role/locator query and every abstention/context query matching
  gold (47/73, zero false family selections, `hit@1` 17/43, `mrr` 0.395,
  `candidate_recall` 10/13, correct abstention 24/24) while all retrieval-intent
  natural-language and synonym questions abstain — the measured retrieval gap
  this baseline freezes for Phase 2. No production behavior changed.
- Added the deterministic metadata-retrieval substrate for term-based family
  discovery, with no LLM, embedding, or network dependency. A new
  `application::query_terms` module folds a raw target into a typed, bounded,
  total `NormalizedQuery` (disjoint language/framework/concept/residue buckets)
  using small committed vocabulary tables — a stopword set, a singular/plural
  table, language aliases (including the `c#`/`c++` compound forms), framework
  aliases, and a concept-alias table — every entry justified by a
  `framework_role` the index can produce. A new source-free
  `list_active_family_search_summaries` store projection (one bounded,
  generation-consistent read of identity, language, code-unit kind, framework
  role, prevalence classification, support count, prevalence, and bounded
  repo-relative evidence-path components) backs `score_family_candidates`, which
  ranks families with explainable additive integer weights, hard
  language/framework exclusion, a deterministic total order, and a retained-K
  cap. This substrate is fully tested and documented in
  `docs/specifications/query-resolution.md` but is NOT yet routed into the
  production fuzzy lookup path; wiring and abstention calibration land in the
  next change. No production behavior changed.

### Fixed

- Aligned `UNKNOWN` recovery guidance with the optional-provider registry so it
  never tells an agent to enable a provider that does not exist. A single
  cross-check authority (`application::providers::provider_recovery_code`) now
  decides every provider-related recovery code against the registry instead of
  hard-coded mechanism lists in the query use case. Mechanisms an *integrated*
  provider slot resolves recover via `enable_provider` (today the TypeScript
  compiler slot's `typescript_module_resolver`, `typescript_paths_resolver`,
  `typescript_package_entry_model`, and `typescript_export_graph`, which moved
  off `resolve_import_graph`). Mechanisms a *registered-but-not-integrated* slot
  would resolve (the python and rust slots) and framework/dependency-injection/
  build models only a future provider could resolve now recover via the new
  low-cardinality code `not_implemented_in_current_version` instead of an
  unexecutable `enable_provider`. Genuine import-graph mechanisms in no provider
  bucket keep `resolve_import_graph`. `stats --unknowns` and `unknowns` output
  therefore shows `not_implemented_in_current_version` buckets where it
  previously showed `enable_provider`; `resolve_dependency_metadata` stays a
  reserved telemetry code but is no longer emitted. `run_sync` keeps its spelling
  for metric-bucket continuity even though its operator action is
  `repogrammar resync`.
- The `families` listing now actually verifies evidence freshness. Previously
  `families --json` accepted a freshness request and source store but ignored
  both, serving the family inventory with zero freshness qualification even when
  the same runtime's single-family lookups returned `StaleEvidence`. The listing
  now reads one bounded projection of the active generation's family evidence and
  hash-verifies each distinct evidence path at most once (never once per family),
  so source reads stay bounded by the distinct evidence paths. Each family entry
  gains a three-state `freshness` verdict — `fresh`, `stale`, or `cannot_verify`
  (a family with zero evidence rows abstains as `cannot_verify`) — and the report
  gains `fresh_count`/`stale_count`/`cannot_verify_count`. Stale and
  `cannot_verify` families stay listed but are qualified distinctly (the human
  surface leads with the counts; JSON carries the fields verbatim), and a stale
  listing raises one low-cardinality report-level `StaleEvidence` unknown
  recovering via `run repogrammar resync` without collapsing the whole listing
  into `UNKNOWN`. The freshness-free `list_families` variant is unchanged.
- Isolated every public npm launcher finalizer lane in its own external working
  directory. Post-public finalizer run `29587973589` verified the immutable
  GitHub release, public npm metadata and provenance, the packaged native
  product, and the public installer, but its launcher step ran from the
  checked-out repository root. npm treated that root's same-name
  `package.json` as the current package without injecting the public package's
  `repogrammar` bin, so the run failed before emitting `STABLE_RELEASE_READY`.
  The corrected workflow is dispatched from `main`, checks out the immutable
  `v0.2.2` source, and remains bound to candidate run `29586694524`, attempt 1.
  Follow-up finalizer run `29589865164` again failed in the launcher step. A
  matching isolated reproduction proved that the public version command worked
  and setup then returned typed `repository_initialization_failed` because the
  tool-only PATH omitted `git`, which repository initialization requires. The
  finalizer now includes `git` in that bounded PATH without widening release
  permissions or changing published artifacts. Finalizer run
  [`29591027524`](https://github.com/SioYooo/RepoGrammar/actions/runs/29591027524)
  then passed every public evidence and smoke gate and emitted
  `STABLE_RELEASE_READY`.

## 0.2.2 — 2026-07-17 stable channel

RepoGrammar `0.2.2` is the patch-forward first published stable-channel pre-1.0
release. It makes no
production-readiness, 1.0 API-stability, stable-MCP-API, sound-analysis,
measured-token-savings, Windows-support, or expanded-language-support claim.
Finalizer run
[`29591027524`](https://github.com/SioYooo/RepoGrammar/actions/runs/29591027524),
bound to candidate run `29586694524`, attempt 1, verified the immutable GitHub
release, npm integrity and provenance, native archive, public installer,
pinned/latest/preview launchers, repository setup, indexing, and product MCP
self-test before emitting `STABLE_RELEASE_READY`. The earlier verifier-only
failures remain recorded under Unreleased.

### Changed

- Synchronized Cargo, Cargo lockfile, npm manifest, installers, launchers, and
  current install documentation on `0.2.2`. Historical
  `0.2.0-preview.0` evidence remains historical and the npm `preview` dist-tag
  must stay on that immutable prerelease.
- Advanced the stable release authority to `v0.2.2`. The retained `v0.2.0` and
  `v0.2.1` candidate tags remain non-reusable, and neither failed unpublished
  version may appear in the stable registry inventory.

### Fixed

- Corrected the npm staging package spec to
  `./npm-candidate/sioyooo-repogrammar-0.2.2.tgz`. The explicit `./` makes the
  argument an unambiguous local tarball instead of GitHub shorthand.

## 0.2.1 — 2026-07-17 failed publication candidate

The retained annotated `v0.2.1` tag points to
`22956a2d5dc8ef19241ae86cefbe42c6709b05a5`. Its tag workflow run
`29582156611`, attempt 1, completed all artifact and private-draft gates. The
expected 11-asset private GitHub draft, id `355686885`, remained unpublished.
npm staging then failed before registry staging because the bare
`npm-candidate/sioyooo-repogrammar-0.2.1.tgz` argument was parsed as GitHub
shorthand instead of a local package file. The npm stage inventory remained
empty. GitHub `v0.2.1` and npm `@sioyooo/repogrammar@0.2.1` were never
published. The tag and private draft are retained and must not be moved,
replaced, published as another version, or reused; `0.2.2` is the required
patch-forward publication candidate.

### Added

- Added a marker-fenced RepoGrammar instruction preflight contract and a safe,
  explicit refresh path. After authority docs, initialized-repository work that
  needs a local contract/convention, repeated implementation, framework role,
  or analogue comparison attempts compact `repogrammar_context`, consumes its
  read plan, and records the typed reason before falling back on unavailable,
  `UNKNOWN`, `FALLBACK`, stale, omitted, or insufficient evidence. Root-cause
  repair and schema/protocol/API/prompt-output or Meaning Contract conformance
  and drift are explicitly covered, even with exact YAML or file targets; pure
  operational or single-fact inspection remains outside the gate. Instruction
  writes remain consent-gated, reversible, and fail closed for malformed or
  duplicated managed markers.
- Added a stable-release proof matrix covering the exact npm tarball, native
  packaged artifacts, isolated setup/index/autosync/Pydantic smoke, GitHub
  immutable releases, npm Trusted Publisher OIDC, non-public staged
  publication, human 2FA approval, provenance, and final public verification.

### Changed

- Made successful live `instructions sync` output recommend a new coding-agent
  session, with the same path-free guidance exposed as the
  `session_restart_recommended` JSON boolean. This avoids implying that an
  already-running agent hot-reloads newly synchronized instructions.
- Synchronized Cargo, Cargo lockfile, npm manifest, installers, launchers, and
  the candidate source documentation on `0.2.1`. Historical
  `0.2.0-preview.0` evidence remains historical and the npm `preview` dist-tag
  must stay on that immutable prerelease.
- Generalized the release policy from preview-only to fail-closed preview and
  stable channels. Preview remains a GitHub prerelease plus npm `preview`;
  stable is a normal GitHub release plus npm `latest`, with manual workflow
  dispatch remaining build-only.

### Fixed

- Corrected the GitHub draft-collision query used by tag publication. The
  failed `v0.2.0` candidate combined `gh api --paginate --slurp` with `--jq`,
  which the runner CLI rejects before draft creation. The patch-forward uses
  per-page `--jq` output without `--slurp`, and the repository guard rejects
  that incompatible option combination while requiring the exact collision
  query and refusal branch to remain present. Stable verification also fails
  closed if the never-published `0.2.0` candidate appears in the registry
  inventory.
- Kept the creating Unix file handle alive and included its link count in the
  identity used to validate and clean an operation-owned managed-instruction
  temporary file. Unlinking changes the live handle's link-count identity, and
  retaining that handle prevents ordinary inode reuse until cleanup completes.
  The documented same-directory concurrent pathname race remains unsupported
  rather than becoming a claimed compare-and-swap guarantee.
- Corrected the npm dist-tag release gate for a package whose only published
  version is a prerelease. npm requires `latest` and may map it to that sole
  version even when publication used `--tag preview`; the gate now verifies the
  exact `preview` target and accepts a published prerelease `latest` only while
  no stable version exists, keeping exact-version or `@preview` installation
  mandatory. This does not
  establish a stable release, and mismatched or malformed registry state still
  fails closed.

## 0.2.0 — 2026-07-17 failed publication candidate

The retained annotated `v0.2.0` tag points to
`981eb9a0ab21e5cb7ea503feead4b2a350bf0471`. Its tag workflow run
`29571508953`, attempt 1, failed before creating a GitHub draft or npm stage
because the workflow used a runner-incompatible `gh api --slurp --jq`
combination. GitHub `v0.2.0` and npm `@sioyooo/repogrammar@0.2.0` were never
published. The tag is retained and must not be moved, deleted, or reused.
`0.2.1` was the next patch-forward candidate but also remained unpublished;
`0.2.2` subsequently became the first published stable-channel release.

## 0.2.0-preview.0 — 2026-07-17 public preview

This is the first public-preview prerelease.
`Cargo.toml` and `package.json` carry `0.2.0-preview.0` so a
`v0.2.0-preview.0` tag produces matching artifacts. This source release record
does not by itself establish registry availability: verify the exact npm
version and matching GitHub assets, and use the source-checkout path when
either check fails.

### Added

- Corrected telemetry help option scope after public-repository dogfood exposed
  a help/parser mismatch. `--project` is now documented only for anonymous
  telemetry and research diagnostics; experiment subcommands explicitly keep
  their dedicated options and reject `--project`. The historical dogfood
  evidence remains unchanged and still reports no paired token measurement.
- Fixed two real-repository Python indexing blockers found during public-preview
  dogfood. Per-name module-scope alias/assignment event histories and cached AST
  byte ranges remove repeated large-module rescans without quadratic full-map
  snapshots; the bounded Rust response envelope now admits valid metadata up to
  2 MiB. The Rust Python frontend concurrently writes stdin and drains bounded
  stdout under a 30-second wall-clock deadline, kills and waits for a timed-out
  child, and returns a typed payload-free timeout. The Rust parser also accepts
  the exact seven-assumption `fastapi_include_router` context contract while
  rejecting malformed fields and raw route literals; prefix metadata is
  reduced to low-cardinality segment shapes, and dynamic prefix/binding
  outcomes retain their typed affected-claim tokens. The bundled worker can now
  parse its own source across the process boundary without weakening fact-count,
  path, hash, typed `UNKNOWN`, or source-free validation.
- Added the zero-friction `repogrammar setup` onboarding path. One reviewed
  application-layer plan can detect Codex or Claude Code, reuse the existing
  reversible machine integration service, initialize and index the current
  repository, optionally start auto-sync, and run the product-binary MCP
  self-test. Interactive setup confirms once; `--yes` is noninteractive;
  `--dry-run` performs no writes; missing agents preserve repository-only setup;
  setup never changes telemetry preference; and rollback is limited to machine
  writes created by that run. Active stale or unverifiable indexes are
  refreshed, false auto-sync starts cannot become ready, zero supported pattern
  groups remain an explicit limitation, and sanitized failure output reports
  completed/retained/rollback/failed/next state. Missing-repository recovery is
  consistently `repogrammar setup`, while resync and auto-sync recovery retain
  their specific commands. Fresh active-index reruns inspect the real family
  inventory so zero-family repositories remain limited, and successful but
  unrecognized native probes preserve the configuration while repository-only
  setup continues with a doctor recommendation. Default help is now a <=25-line primary journey with `help --all`
  for the full inventory. Default `families` and query human output is compact
  and hides internal cluster and query-routing fields while JSON stays
  canonical. Status, doctor, setup, and query preflight now consume one
  authoritative recovery classifier.
- Hardened public-preview setup truthfulness and lifecycle reconciliation.
  Native agent inspection now distinguishes `OwnedCurrent` from
  `OwnedOutdated`: matching current ownership is skipped, matching obsolete
  RepoGrammar ownership is refreshed through the reversible install service,
  and foreign, malformed, or owned-but-drifted state remains preserved. Setup
  separately tracks newly configured and reconfigured targets, so a downstream
  repository or MCP failure rolls back only integrations created by that setup
  run and never deletes a pre-existing owned integration. Auto-sync startup now
  requires a bounded PID-plus-startup-nonce daemon-lock handshake while the
  spawned child remains alive. The child first publishes `starting` ownership
  and advances to `ready` only after repository-state validation, worker
  environment preflight, initial fingerprinting, log initialization, and a
  successful first heartbeat. Worker-environment, fingerprint, repository-
  state, lock-refusal, early-child-exit, timeout, and first-heartbeat failures
  persist as low-cardinality startup codes without raw paths or environment
  values. Status output separates current daemon/startup/repository readiness
  from `previous_autosync_attempt`; a previous sync error is not rendered as
  the current daemon state. Daemon lifecycle mutations are serialized before
  exact-record lock cleanup so stop cannot delete a successor lock. Family
  inventory is now `Available(0)`, `Available(N)`, or `Unknown` instead of
  turning query errors into zero families. Setup human and JSON output
  separately report product self-test state, ready/blocked agent targets,
  agent-query readiness,
  repository-index readiness, auto-sync readiness, family-evidence state, and
  all limitations; repository-only success has no suggested coding-agent
  question and initialization/indexing retain distinct stage labels.
- Corrected two native release-gate false failures. Unix lock-owner liveness
  now tolerates only the one-second rounding window of the `ps etimes` probe,
  preventing a healthy autosync daemon from becoming `running=false` at a
  wall-clock boundary while still rejecting later PID reuse. The Windows
  source-only installer contract now returns explicit success after verifying
  its intentionally failing delegated-install case, so that expected child
  status cannot leak into the CI job result.
- Versioned the private CPython parse-document boundary independently of the
  public semantic-worker protocol. Requests and normal responses now require
  the exact `protocol_version=1, contract_revision=1` tuple. A newer worker
  rejects an older host with one path- and source-free mismatch envelope; a
  newer Rust host maps an older worker's bounded rejection, or a missing/wrong
  response revision, to typed `PythonFrontendContractMismatch` recovery that
  tells the user to rebuild or reinstall the binary and bundled worker from the
  same release. Regression coverage uses the exact committed
  `pydantic-basic` fixture through the direct worker, Rust parser adapter, full
  indexing, and unchanged incremental copy-forward. Its `field_validator`
  remains a structural member fact while the validator body call remains a
  non-blocking typed `FrameworkMagic` UNKNOWN; neither is promoted to
  unsupported semantic certainty.
- Hardened release truth gates without claiming publication. Manual workflow
  dispatch is explicitly build-only even from a tag ref. Only a pushed tag may
  enter credential or publication jobs; tag verification requires containment
  in `origin/main` plus accepted npm credentials before publication. Exact packaged archives are unpacked and
  exercised through `version`, `setup --dry-run --json`, a live product MCP
  self-test, `find`, and advisory `check`; GitHub prerelease assets are staged
  before npm publication; and a missing/disappearing npm token fails visibly
  instead of producing a green skipped-publish result. The preview matrix and
  npm launcher now admit exactly macOS/Linux x64/arm64, reject Windows, and no
  longer publish a Windows archive or `install.ps1`. The source-tree PowerShell
  wrapper removes its release-download branch and refuses all install actions
  unless `-FromSource` is explicit. The canonical release
  checklist separates build-only candidate evidence, tag publication, fresh
  HOME verification, and external publication proof. Linux downloads now fail
  closed before write for musl, unknown libc, glibc below 2.35 on x86_64, or
  glibc below 2.39 on arm64; pinned builders and imported-symbol inspection
  hold those ABI floors. The npm launcher also preserves a concurrent first-
  install winner after rename collision. A real temporary npm tarball is
  inspected, installed offline, and executed in tests, while packed README
  links remain valid outside the repository. Python 3.10+ is the explicit
  packaged runtime minimum. Native CI runs the PowerShell source-only contract
  on Windows without publishing a Windows artifact. Preview npm publication
  uses dist-tag `preview`, and local workflow changes alone remain no proof of
  external publication.
- ADR-0025 now records the Swift N1 architecture/security preflight plus bounded
  discovery/configuration inventory without adding a dependency, toolchain,
  worker, parser, project model, code unit, IR, fact, typed `UNKNOWN`, family,
  or support behavior. Stable `swift`/`swift-config` classification inventories
  exact `.swift`, `Package.swift`, `Package.resolved`, `.swift-version`, and
  complete ASCII `Package@swift-M[.m[.p]].swift` basenames. Swift-only
  `.build`/`.swiftpm` exclusions do not globally prune other languages. Full
  and incremental indexing persist only bounded path/raw-byte hash/size/token
  metadata, bypass source-store/parser work, emit one path-free warning per
  accepted token, report Swift-only generations as `file_manifest_only`, keep
  inventory deltas incremental, purge legacy claim records, and retain generic
  Git-independent autosync fingerprinting. Swift advances only to
  `discovered_only` and remains unsupported. The production syntax candidate
  is exact SwiftSyntax 603.0.2 `SwiftParser` in a separately reviewed
  OS-sandboxed worker, differentially qualified against the exact Swift 6.3.3
  compiler. Exact 6.3.3 SourceKit-LSP/sourcekitd is only a separately qualified
  no-build semantic identity candidate. Neither path may open or build the
  target repository, evaluate `Package.swift`, resolve dependencies, enable
  background indexing, load target modules/macros/plugins, run tests or
  generators, spawn descendants, or use network/ambient toolchain state. The
  next permitted module is documentation/evidence-only artifact, differential,
  dependency, supply-chain, and native sandbox qualification. The future first
  family is the narrow direct `swift.xctest.test_method`; it does not claim
  build, runtime selection, execution, or pass/fail. Swift Testing is deferred
  because `@Test` is a compiler macro. All artifact, supply-chain, five-target,
  native sandbox, project, frontend/IR, obligation, family, semantic-product,
  cross-module review, and completion-audit gates remain open. Preflight commit
  `d293238723c0b943d9665f05a4db948fba0f0e35` and discovery commit
  `9bc1960db21e62216f2c9b85e88e32e9733390b0` form the paused baseline;
  `docs/plans/swift-n1-qualification-handoff.md` records the exact next-session
  evidence contract and forbids combining qualification with production
  admission.
- ADR-0024 now records the PHP N1 frontend/security preflight plus bounded
  discovery/configuration inventory without adding PHP semantic runtime
  behavior, support, or a production dependency. Stable `php`/`php-config`
  classification inventories exact `.php` paths and exact root/nested
  `composer.json`, `composer.lock`, `phpunit.xml`, and `phpunit.xml.dist`
  basenames. PHP-only `.composer`/`.phpunit.cache` exclusions do not globally
  prune other languages; exact `vendor` remains globally excluded. Full and
  incremental indexing persist only repo-relative path, raw-byte SHA-256, size,
  and token, bypass the source store and parser, aggregate one path-free warning
  per accepted token, report PHP-only generations as `file_manifest_only`, keep
  inventory deltas incremental, and purge legacy claim-bearing PHP records.
  Autosync keeps its generic Git-independent fingerprint policy. PHP advances
  only to `discovered_only` and remains unsupported.
  The production candidate is `mago-syntax` 1.43.0 only in a separately
  reviewed OS-sandboxed worker. Official PHP 8.5.8 `php -n -l` is the isolated
  syntax-validity oracle; `nikic/PHP-Parser` 5.8.0 is the isolated AST/location
  differential and separately qualification-gated fallback. Tree-sitter PHP
  0.24.2 may generate syntax candidates only. The future first family is the
  exact `php.phpunit.test_method` slice. Composer 2.10.2 pins future lock-
  content-hash semantics; raw project configuration stays in a separate non-
  executing parser, and PHP frontend admission is blocked until its normalized
  profile and custom exclusions are applied. Exact PHPUnit `Test` attributes
  must be zero-argument and non-repeated. Composer JSON/lock and PHPUnit XML
  inputs remain bounded, non-executing data. Composer, PHPUnit, autoloaders,
  plugins, scripts, repository PHP, and target
  dependencies must not execute. The dependency, artifact, sandbox, protocol,
  resource, malformed-input, five-target, three-OS runtime, family, product,
  and completion-audit gates all remain open. No source text, config parse,
  project model, unit, IR, fact, typed `UNKNOWN`, family, or readiness claim is
  added; custom `vendor-dir` and PHPUnit cache-directory handling remain
  unresolved until the later project-model stage.
- ADR-0023 accepts a decision-only preflight for the known concurrent
  canonicalize-then-reopen filesystem confinement gap. The required future
  invariant pins the repository root, performs discovery/source-store/autosync
  access one validated path component at a time relative to retained directory
  handles, refuses final symlink/reparse components, rejects special-file swaps
  without blocking, and uses one opened regular-file handle for metadata and
  bounded content. Exact `cap-std`/`cap-fs-ext` 4.0.2 package checksums and
  candidate APIs are recorded, but no dependency or runtime change is
  authorized. Full transitive/advisory/build-script review, five-target compile
  proof, native Linux/macOS/Windows runtime proof including Unix FIFOs and
  Windows junctions/relevant special objects, simultaneous three-consumer
  migration, and a final completion audit remain mandatory. The current P2
  limitation stays open.
- ADR-0022 now records the Ruby N1 preflight plus bounded discovery/configuration
  implementation without adding Ruby runtime support or a production
  dependency. Stable `ruby`/`ruby-config` classification, Ruby-specific
  `.bundle`/`.ruby-lsp` exclusions, source-store/parser bypass, one warning per
  manifest token, file-manifest/mixed-mode reporting, incremental metadata
  deltas, legacy-claim purge, and Git-aware discovery versus Git-independent
  autosync fingerprinting advance Ruby only to `discovered_only`. The exact
  direct `ruby.minitest.test_method` family remains staged behind an immutable
  `ruby-prism` artifact, explicit CRuby
  4.0 syntax profile, future authoritative typed obligations, support >= 3,
  source-free readiness, review, and completion audit. A documentation/evidence-only
  dependency and sandbox qualification must pass before production artifact
  admission. The 1.9.0 candidate wraps native C99 Prism through FFI and may not
  run in the primary process on untrusted source; checksum/vendor,
  license/supply-chain, five-target malformed-corpus/fuzz,
  deterministic-range/diagnostic, CPU/thread/memory, and separately reviewed
  OS-sandbox gates remain mandatory. Default analysis must never evaluate
  Gemfiles/gemspecs or invoke Ruby, Bundler, RubyGems, Rake, Rails, tests,
  generators, installed gems, repository tooling, or network access. The first
  profile accepts only the sole repository-root `.ruby-version` with exact
  `4.0.6` plus optional LF, and the worker receives only bounded `.rb` bytes plus
  normalized profile metadata. The Minitest anchor requires a lexically earlier
  unconditional program-body require and a source-visibly public method. Ruby
  discovery persists only path/hash/size/token metadata and stores no source
  text, code unit, IR, fact, typed `UNKNOWN`, family, project model, or support
  claim; Ruby remains unsupported and all later qualification gates remain
  open.
- Fixed aggregate filesystem discovery ceilings now bound accepted supported
  files (100,000), accepted bytes (512 MiB), reported skips (100,000), visited
  entries (250,000), and directory depth (256), with inclusive exact-boundary
  behavior and typed path/source-free plus-one errors. Discovery consumes
  `read_dir` incrementally before sorting only bounded children, deduplicates
  Git-unavailable warnings while walking, and aborts before preparing or
  activating a generation. The autosync metadata fingerprint moved from the
  composition root into the filesystem adapter and shares accepted-file/byte,
  visited-entry, and depth admission, so its preflight cannot bypass the next
  index's aggregate content-size boundary for the same candidates. Polling does
  not evaluate Git ignore, so supported Git-ignored candidates deliberately
  count and can make autosync stricter than manual discovery. Successful CLI
  JSON remains unchanged; failures are invalid input (exit 2), and `init` retains
  `failed_step: resync`. `FileDiscoveryError::ResourceLimitExceeded` is an
  additive public pre-1.0 enum variant, so downstream exhaustive matches must
  be updated.
- Go discovery/config inventory with stable `go` and `go-config` tokens for
  bounded `.go` plus root or nested `go.mod`/`go.work`. A normalized-path
  classifier records dot/underscore, `vendor`, `testdata`, `_test.go`, and the
  Go 1.26.5 known GOOS/GOARCH filename suffix snapshot without selecting a
  build environment or becoming family authority. Default full and incremental
  indexing skip parser-facing source-store reads and parsing for both tokens, persist only source-
  free file metadata, aggregate one deterministic path-free unsupported warning
  per token from the whole manifest, and emit zero units, IR, semantic facts,
  or families. Go-only and empty generations now report `file_manifest_only`
  with `parser: deferred`; mixed parsed generations remain syntax-only even
  when an incremental round performs zero parser dispatches. While `go` and
  `go-config` remain absent from `ParserProjectContext`, `.go`, `go.mod`, and
  `go.work` additions, removals, and modifications stay incremental with exact
  metadata deltas and zero Go reparses. Copy-forward purges legacy/tampered Go
  units, IR, facts, derived support, and families while retaining file metadata.
  Generic-policy fixtures retain dot/underscore, `testdata`, and platform-
  suffix inventory but keep `vendor` globally excluded. Build-tag/generated/cgo/
  `go:generate` marker scanning remains deliberately deferred instead of using
  regex guesses and now belongs to frontend/IR by ADR amendment; Go advances
  only to `discovered_only` and remains unsupported.
- Bounded Java JUnit/TestNG test-data-link UNKNOWN reduction. Exact imported/FQN
  same-class JUnit `@MethodSource` direct-repeatable scalar/array literal sets
  (including blank/omitted same-name convention) and TestNG literal
  `dataProvider` names may resolve only as complete unique source-visible sets
  and emit structural replacement evidence. Link identity requires an FQN or
  one unambiguous explicit import; wildcard/colliding imports, local shadows,
  malformed imports, nested annotations, and parse-open inventories abstain.
  External/signature/provider-class, type-level/inherited, explicit-container/
  meta, `PER_CLASS` non-static, overloaded/duplicate, dynamic, partial-positive,
  mixed-framework, missing, unknown-identity, invalid non-parameterized, and
  nested-boundary cases remain typed `UNKNOWN` or conflict. The
  adapter builds one registry per class-like body, never executes javac/build
  tools/test engines/processors/repository code, never promotes the link to
  family support, and does not claim Java Top-20 completion.
- ADR-0021 accepts the decision-only Go N1 preflight without adding Go runtime
  support or a production dependency. The future authoritative path is an
  explicit, opt-in, sandboxed standard-library worker over supplied inputs with
  `go/parser`, candidate-scoped `go/types`, and `go/build/constraint`; a later
  version-pinned Tree-sitter Go grammar may generate syntax candidates only.
  The safe default forbids `go/packages`, `go list`, gopls, repository build/
  test/generate execution, cgo, child processes, and network access. The first
  exact family candidate is a top-level `_test.go`
  `TestXxx(*testing.T)` declaration with exact `"testing"` import identity and
  alias normalization, the conservative exported-name rule, support >= 3, and
  explicit build/config/generated/module/dispatch/cgo/generator `UNKNOWN`s.
  The preflight also fixes module boundaries, resource/determinism limits, an
  atomic seven-stage delivery sequence, and an incomplete post-decision review;
  the future target token is `go.testing.test_function` (signature
  `func(*testing.T)`, not a nonexistent `testing.Test` API). Unadjusted
  `go/token` ranges, valid-header/file-selection constraint rules, whole-file
  parser fail-closed gates, filesystem-read denial, a controlled in-memory
  importer, and path/hash plus worker-digest cache provenance are mandatory.
  The existing generic worker boundary is not an adequate Go sandbox. Go
  is now followed by the separate discovery/config module described above; the
  preflight commit itself added no runtime behavior.
- ADR-0020 is Accepted per explicit maintainer direction and freezes the
  official TIOBE July 2026 Top-20 list as a planning snapshot. It defines a
  per-language completion gate covering discovery/configuration, an
  authoritative frontend or format parser, code units/IR, typed `UNKNOWN`, one
  family-first exact-anchor or explicit language-internal slice, positive/
  negative/low-support/parse-degraded fixtures, source-free readiness, a four-
  part correctness/security/completeness/performance review, atomic submodule
  commits with tests/docs, and a final SHA-linked completion audit. The active
  plan keeps the seven current ranked languages,
  TypeScript as an uncounted extra, and thirteen additions in disjoint waves.
  This documentation/governance change adds no product runtime support or
  production dependency; extension-only recognition remains insufficient.
- UNKNOWN governance now has internal orthogonal `ClaimImpact` and
  `ResolutionClass` axes behind the unchanged public `UnknownClass` tokens and
  schemas. Family suppression and `blocks_support` use only claim impact;
  recovery uses only resolution plus the registered mechanism. Exact runtime
  and execution assumptions keep data-dependent calls/imports, proxies,
  reflection/runtime binding, Rust procedural macros, and Cargo build scripts
  irreducible, while declarative Rust macros, fixed-command C/C++ preprocessing,
  and cfg/target selection remain recoverable. Legacy class counters and JSON
  projections remain compatible and are not reinterpreted as an axis cross-tab.
  The public Rust `ClaimUnknown` record retains its existing `pub class` field
  and struct-literal compatibility; recoverable/irreducible legacy values are
  never interpreted as family impact.
- ADR-0015 (provider-backed semantic analysis execution program) is now
  Accepted per explicit maintainer direction. This authorizes the staged,
  consent-gated analyzer-execution program (Pyrefly/Pyright, rust-analyzer,
  TypeScript `Program`/`TypeChecker`, `javac`/JDT — analyzers only, never
  repository runtime code) in the ADR's D4 expected-value order. No adapter is
  wired by the acceptance itself: each provider still requires its own
  dependency-acquisition follow-up decision, benchmark fixture pair, and
  consent boundary before execution, and every irreducible UNKNOWN class stays
  typed `UNKNOWN`.
- C/C++ bounded structural preview (Wave C1, realizing ADR-0019 for C/C++):
  default indexing discovers `.c`/`.h` (parsed with Tree-sitter C) and
  `.cc`/`.cpp`/`.cxx`/`.hh`/`.hpp`/`.hxx` (parsed with Tree-sitter C++) files,
  skipping the CLion `cmake-build-debug`/`cmake-build-release` output
  directories, and derives `repogrammar-cpp-derived` /
  `bounded_tree_sitter_c_cpp_anchor_v1` `DATAFLOW_DERIVED` support only for
  registration-macro shapes corroborated by a lexically parsed `#include`:
  GoogleTest `TEST`/`TEST_F`/`TEST_P`/`TYPED_TEST` and `::testing::Test`
  fixtures (via `gtest/gtest.h` or `gmock/gmock.h`), Catch2 `TEST_CASE`/
  `SCENARIO` (via `catch2/` or `catch.hpp`), doctest `TEST_CASE` (via
  `doctest/doctest.h`), and Boost.Test `BOOST_AUTO_TEST_CASE`/
  `BOOST_FIXTURE_TEST_CASE`/`BOOST_AUTO_TEST_SUITE` (via `boost/test/`). Both the
  function-definition-with-macro-declarator and the call-expression parse shapes
  are handled. Families require at least three complete-link-compatible support
  facts with present-and-equal `test_framework`/`test_macro` evidence and no
  claim-relevant blocking `UNKNOWN`. Registration macros without include
  evidence and `TEST_CASE` matching both Catch2 and doctest are blocking
  `UnresolvedImport`/`ConflictingFacts` `cpp_test_framework_identity` `UNKNOWN`s;
  `#if`/`#ifdef`/`#ifndef` regions overlapping a unit are blocking
  `cpp_build_variant` `UNKNOWN`s (standard include guards and `#pragma once` are
  excluded); Tree-sitter ERROR-node regions are blocking `cpp_macro_boundary`
  `UNKNOWN`s. Qt `Q_OBJECT`/moc output, string-based SIGNAL/SLOT connect,
  function-pointer dispatch, and `compile_commands.json`/`vcpkg.json`/
  `conanfile.txt` project configuration remain non-blocking subclaims or
  source-free `PROJECT_CONFIG` structural inventory. `stats`/`doctor`
  `by_language`, `unknowns --json` readiness detail, and repo-shape scopes gain a
  bounded `c/cpp` scope, and `repo-guard` now guards the `.cxx`/`.hh`/`.hxx`
  extensions. RepoGrammar never runs a build, compiler, preprocessor, or
  moc/protoc, never expands macros, never executes a compilation database, and
  performs no points-to or class-hierarchy dispatch analysis; this resolves no
  UNKNOWN and changes no official v0.1 scope claim by itself.
- Rust general framework preview (Wave E1, realizing ADR-0019 for Rust): beyond
  the existing self-dogfood roles, default indexing now recognizes exact
  source-visible serde/thiserror/tokio/clap derive and attribute shapes plus
  axum literal `Router::new().route(...)` segments in any repository, each gated
  by same-file `use`-path evidence or an inline fully-qualified path, and
  promotes them through the existing `repogrammar-rust-derived` /
  `bounded_tree_sitter_anchor_v1` `DATAFLOW_DERIVED` support path under support>=3
  complete-link family gates. New code-unit kinds are `serde_model`,
  `thiserror_error_enum`, `tokio_entry`, `tokio_test`, `clap_parser`, and
  `axum_route`; supported targets are `serde.Serialize`/`serde.Deserialize`,
  `thiserror.Error`, `tokio.main`, `tokio.test`,
  `clap.Parser`/`clap.Subcommand`/`clap.Args`, and `axum.routing.route`. serde
  compatibility requires a present-and-equal trait/target profile
  (Serialize-only, Deserialize-only, and both never merge) and axum requires a
  present-and-equal HTTP method and literal path shape. Derive/attribute macro
  expansion stays a non-blocking `rust_derive_expansion` subclaim, and axum tower
  middleware ordering and handler extractor resolution stay non-blocking
  `rust_axum_middleware_semantics`/`rust_axum_extractor_semantics` subclaims;
  derive tokens without use-path evidence
  (`UnresolvedImport`/`rust_framework_attribute_binding`), non-literal or
  untraceable axum routes (`rust_axum_route_identity`), and `#[cfg]` on a
  framework unit (`rust_build_variant`) are blocking typed `UNKNOWN`s. The Rust
  readiness scope becomes `bounded_v0_2_preview`/`bounded_preview` (self-dogfood
  families still form), the `axum_route_model` mechanism joins the telemetry
  vocabulary, and `stats`/`unknowns` scopes plus the repo-shape kind whitelist
  gain the new kinds. RepoGrammar never expands derive/attribute macros, resolves
  traits, or performs points-to analysis, and this resolves no UNKNOWN and
  changes no official v0.1 scope claim by itself.
- Java framework deepening (Wave J1, realizing ADR-0019 D3/D4 for Java): the
  `.java` preview grows beyond Spring with the same exact-import/FQN gate and
  `repogrammar-java-derived` / `bounded_tree_sitter_java_anchor_v1`
  `DATAFLOW_DERIVED` support. New exact anchors and families: JUnit 5
  `@Test`/`@ParameterizedTest`, JUnit 4 `@Test`, and TestNG `@Test` methods;
  JPA/Jakarta Persistence `@Entity`/`@MappedSuperclass`/`@Embeddable` under dual
  `jakarta.persistence`/`javax.persistence` roots (jakarta and javax entities
  never cluster together); and JAX-RS/Jakarta REST `@Path` resource classes with
  `@GET`/`@POST`/`@PUT`/`@DELETE`/`@PATCH`/`@HEAD`/`@OPTIONS` resource methods
  under dual `jakarta.ws.rs`/`javax.ws.rs` roots. Mockito annotations attach
  `mockito_context` metadata to enclosing tests, Lombok `lombok.*` annotations
  emit only a non-blocking `MacroOrPreprocessor` generated-members `UNKNOWN`, and
  Spring Data derived-query method names attach structural metadata (never a
  support target). Test/JPA/JAX-RS annotation lookalikes without exact
  imports (`java_test_annotation_binding`, `java_jpa_entity_identity`,
  `java_jaxrs_resource_identity`), a JAX-RS verb outside a `@Path` class, and
  mixed JUnit 4/5 `@Test` bindings (`ConflictingFacts`) are blocking typed
  `UNKNOWN`s, while external `@MethodSource`, TestNG data providers, Mockito
  runtime mocks, JPA runtime mapping, and Lombok generated members remain
  non-blocking subclaims. New required mechanisms `java_test_annotation_model`,
  `jpa_entity_model`, `jaxrs_resource_model`, `java_annotation_processor_boundary`,
  and `java_mockito_runtime_mock_model` are registered, `stats`/repo-shape scopes
  gain the eight new Java unit kinds, and the parser splits into a
  `parsing/java/` module (`mod.rs` core + `spring.rs`/`junit.rs`/`jpa.rs`/
  `jaxrs.rs`) with the blocking-claim and copied-assumption policy tables hoisted
  into one authoritative registry. RepoGrammar never executes Maven/Gradle/javac,
  never runs annotation processors or Lombok, never generates Mockito mocks,
  never parses `testng.xml`/`orm.xml`, and does not validate derived-query
  property paths; this resolves no `UNKNOWN` and changes no official v0.1 scope
  claim by itself.
- Python bounded framework preview (Wave E1, realizing ADR-0019 for Python):
  the CPython AST worker now classifies Django models
  (`django.db.models.Model` bases with bucketed field-count and `class Meta`
  variation context), literal `urlpatterns` `path()`/`re_path()` routes,
  `django.test.TestCase` classes, Flask routes decorated by a same-file
  `Flask(__name__)`/`Blueprint(...)` receiver (including Flask 2 method
  shortcuts), stdlib `unittest.TestCase` `test_*` methods with `setUp`/
  `tearDown` fixture-shape context, click/typer command decorators with
  bucketed parameter-count context, and Celery `@shared_task`/`@app.task`
  tasks. Each maps 1:1 to a `framework:{django,flask,unittest,click,typer,
  celery}.*` role and forms families through the existing
  `repogrammar-python-derived` / `bounded_ast_anchor_v1` `DATAFLOW_DERIVED`
  support with support>=3 and no claim-relevant blocking `UNKNOWN`. Framework
  identity requires the base or decorator receiver to resolve to an exact
  framework import binding; name lookalikes without the import stay `UNKNOWN`.
  Non-literal Django routes (`python_django_url_identity`), imported-external
  Django model bases (`python_django_model_identity`), unresolvable Flask/
  typer/Celery receivers (`python_flask_route_identity`,
  `python_cli_command_identity`, `python_celery_task_identity`) are blocking
  typed `UNKNOWN`s, while settings-driven behavior
  (`python_django_settings_behavior` → `django_settings_model`), string URL
  dispatch/`include()` (`python_django_string_dispatch`), `unittest.mock.patch`
  targets (`python_unittest_patch_target`), and Celery runtime routing
  (`python_celery_runtime_routing`) remain non-blocking subclaims. New recovery
  mechanisms `django_project_model`, `django_settings_model`, and
  `flask_app_model` join the telemetry vocabulary. RepoGrammar never evaluates
  `settings.py`, reverses URLs, models middleware order, or resolves task-queue
  routing, and this changes no official v0.1 FastAPI/pytest/SQLAlchemy/Pydantic
  scope claim by itself.
- C# bounded structural preview (Wave CS1, realizing ADR-0019 for C#): default
  indexing discovers `.cs` files (skipping the MSBuild `obj/` output directory),
  parses them with Tree-sitter C#, and derives `repogrammar-csharp-derived` /
  `bounded_tree_sitter_csharp_anchor_v1` `DATAFLOW_DERIVED` support only for
  exact lexical-scope `using`/FQN-gated framework anchors: ASP.NET Core
  `[ApiController]`/`ControllerBase` controllers and `[Http*]` actions inside a
  controller, literal minimal-API `MapGet/MapPost/MapPut/MapDelete/MapPatch`
  routes whose receiver traces in-file to
  `WebApplication.CreateBuilder(...).Build()`, EF Core `DbContext`/`DbSet<T>`
  entity sets, and xUnit `[Fact]`/`[Theory]`, NUnit `[Test]`/`[TestCase]`, and
  MSTest `[TestMethod]` (inside `[TestClass]`) tests. Families require at least
  three complete-link-compatible support facts with matching `http_method` and
  `route_template_shape` evidence and no claim-relevant blocking `UNKNOWN`.
  Lookalike attributes without exact usings (`csharp_attribute_binding`), route
  attributes outside a controller (`csharp_controller_identity`), unresolvable
  minimal-API receivers (`csharp_minimal_api_receiver`), MSTest methods without a
  `[TestClass]` (`csharp_test_class_identity`), and `#if` regions overlapping a
  unit (`csharp_build_variant`) are blocking typed `UNKNOWN`s, while runtime DI,
  the filter pipeline, nonliteral route templates, convention routing,
  partial-class/source-generator boundaries and `dynamic` binding remain
  non-blocking subclaim `UNKNOWN`s. xUnit `MemberData` now discharges only its
  same-class source-link UNKNOWN when a direct identifier string names a unique
  unconditional `public static` field, property, or zero-argument method in a
  closed non-generic class; it never evaluates the provider or claims row
  compatibility, and every open/dynamic/ambiguous form remains typed UNKNOWN.
  C# exact-using evidence is now Tree-sitter lexical-scope aware, so comments,
  strings, and sibling namespaces cannot corroborate attributes; immutable
  member inventories are shared across traversal contexts instead of copied.
  `stats`/`doctor`
  `by_language`, `unknowns --json` readiness detail, and repo-shape scopes gain a
  bounded `csharp` scope, and `repo-guard` now guards the `.cs` extension.
  RepoGrammar never executes MSBuild, Roslyn, source generators, or the ASP.NET
  Core runtime, never evaluates preprocessor conditions, does not analyze
  Razor/Blazor, and this changes no official v0.1 scope claim by itself.
- ADR-0019 accepts a bounded multi-language structural expansion program:
  C# and C/C++ join Java as bounded v0.2 preview language slices, Java gains
  a framework-deepening wave (JUnit 5/4, TestNG, Mockito context, JPA,
  JAX-RS, Spring Data derived-query metadata, Lombok as typed
  `MacroOrPreprocessor` UNKNOWN), and Rust/Python/TS-JS gain
  framework-widening waves, all under the existing exact-anchor,
  support>=3, typed-UNKNOWN contract with no analyzer or build execution
  (ADR-0015 remains Proposed and unactivated). The execution plan, wave
  tables, UNKNOWN reason-code mappings for the C preprocessor and C#
  source generators, and sound no-execution project-config parsing scope
  (`compile_commands.json`, `vcpkg.json`, `conanfile.txt`, SDK-style
  `.csproj`/`Directory.*.props`, root `pom.xml`) are documented in
  `docs/plans/multi-language-expansion-plan.md`. This changes no official
  v0.1 scope claim and resolves no UNKNOWN by itself.

- TS/JS v0.2 preview widening (ADR-0019 Wave E1): conservative exact-anchor
  family support for Zod schema builders (`z.object`/`z.union`/
  `z.discriminatedUnion`/`z.enum`/`z.array` under an exact `zod`/`zod/v4`
  import), NestJS `@Controller`/`@Get`/`@Injectable`/`@Module` decorators bound
  to `@nestjs/common` (routes anchor only inside an exact-import controller), and
  Hono literal `app.get`/`post`/`put`/`delete`/`patch` routes on a `new Hono()`
  receiver. Mocha and `node:test` exact named imports are aliased onto the
  existing Jest/Vitest suite/test surface with a required-equal `runner_kind`,
  so mocha, node:test, jest, and vitest families never merge. New non-blocking
  subclaims (`tsjs_nest_di_resolution`, `tsjs_nest_dynamic_module`,
  `tsjs_zod_runtime_refinement`) and blocking claims
  (`tsjs_nest_controller_identity`, `tsjs_hono_receiver`) map onto the new
  `nestjs_di_model` and `hono_receiver_model` recovery mechanisms. RepoGrammar
  does not model the NestJS DI graph, dynamic modules, or Zod runtime
  refinement, and React stays excluded from all family claims.
- `status --json` and `doctor --json` now include a source-free `readiness`
  object for repository setup and local-analysis hygiene. It distinguishes
  not-initialized, state-only/no-active-index, ready active index,
  unhealthy/stale active index, and autosync-recommended states; reports
  recommended next commands without running them; and surfaces `.repogrammar/`
  ignored/tracked-risk hygiene plus `.codegraph/` as foreign unmanaged provider
  state with `managed_by_repogrammar: false`.
- `doctor --json` now reports a source-free `optional_providers` capability
  registry (ADR-0017): the known optional semantic provider slots
  (`typescript_compiler`, `python_type_provider`, `rust_analyzer`), the stable
  required-mechanism buckets each would resolve, and an honest availability state
  — `configured`, `available_bundled` (RepoGrammar ships a worker for it and its
  runtime is on `PATH`, so it can be enabled here right now — still opt-in),
  `not_configured` (integrated but not trivially runnable here), or
  `not_integrated` (no adapter wired yet). Detection reads configuration signals
  and a runtime `PATH` scan only (e.g. `REPOGRAMMAR_TYPESCRIPT_WORKER` and whether
  `node` — the bundled TypeScript worker's runtime — is present) and executes no
  analyzer or worker; a non-integrated slot is always `not_integrated` regardless
  of any stray signal. Missing providers are optional accelerators reported
  alongside `checks`, never doctor failures, so the baseline product keeps working
  with every provider absent. Runtime provider statuses (timeout/conflict) are
  intentionally deferred until an adapter executes.

### Changed

- `repogrammar init` now builds or refreshes the active index by default.
  `--state-only` preserves lifecycle-only initialization, `--resync` remains an
  accepted explicit spelling of the default index build, and `--autosync` starts
  only after that first index succeeds.
- `repogrammar stats` / `stats --json` now distinguish official Python-family
  readiness from active indexed inventory. Output includes
  `official_family_scope: python_v0_1`, indexed file/code-unit/semantic-fact
  counts, source-free readiness state, by-language indexed inventory, and
  unsupported-scope guidance for TS/JS repositories where indexed context exists
  but no supported families are available. React/RN remains explicitly
  unsupported; stats recommends exact-path `find`/`check` for source-free
  `PARTIAL_CONTEXT` read plans instead of creating React/RN family or
  conformance claims.
- The typed `UNKNOWN` inventory (`unknowns --json` / `stats --unknowns --json`)
  now exposes a source-free `by_obligation` bucket naming the kind of semantic
  question each `UNKNOWN` poses — its obligation — as a first-class refinement of
  the `UNKNOWN` contract (ADR-0016, realizing the ADR-0015 absorb point). The
  fixed, low-cardinality vocabulary is `type_identity`, `symbol_binding`,
  `dispatch_target`, `framework_identity`, `build_variant`, `macro_expansion`,
  `external_dependency`, `runtime_irreducible` (runtime-defined residuals that
  stay `UNKNOWN` by design), and `governance` (stale/conflicting/insufficient
  states, which are not semantic obligations). Obligation is derived
  deterministically from the already-typed reason plus the same
  language/claim/role context used for the required mechanism; it never resolves
  an `UNKNOWN`, changes whether it blocks, weakens any gate, or becomes family
  evidence, and is orthogonal to the recoverable/irreducible class axis. The same
  obligation is also rolled up (source-free) on the `stats --json`
  `query_outcome_rollup` `by_obligation` bucket for query-time UNKNOWNs, tolerating
  its absence in rollup files written before it existed.
- Rust trait-object dispatch UNKNOWNs now record the syntactic dispatched trait
  name(s) (`rust_trait_dispatch_trait=<name>`) as bounded context. This names
  which trait is dispatched without resolving the concrete target: a sound
  candidate/impl set would require name and type resolution and impl coherence
  that a Tree-sitter substring scan cannot provide, so the dispatch claim itself
  stays a typed `UNKNOWN`. Full resolution remains provider-gated (rust-analyzer).
- Python `importlib.import_module` resolution now applies sound intra-scope
  string-constant propagation: a target read from a single-static local string
  constant (for example `name = "pkg.mod"; importlib.import_module(name)`)
  resolves to the repo-local module like a literal, instead of a
  `DynamicImport` `UNKNOWN`. The constant is used only when the name is bound
  exactly once to a string literal in the scope; a reassigned, parameter-bound,
  or otherwise ambiguously bound name stays typed `UNKNOWN`, and data-dependent
  imports remain `UNKNOWN`. This is a no-dependency, no-execution stage of the
  ADR-0015 provider program that shrinks the statically-determinable slice of
  `DynamicImport`.
- Python `__import__` resolution now mirrors `importlib.import_module`: a literal
  or single-static-constant absolute name resolves to the repo-local module
  instead of a `DynamicImport` `UNKNOWN`. Because `__import__(name, ..., level)`
  performs a relative import when `level` is nonzero, a call abstains to a typed
  `UNKNOWN` whenever a nonzero or non-literal `level`, or a positional/keyword
  splat that could hide `level`, might make the argument relative; reassigned,
  parameter-bound, and data-dependent targets also stay `UNKNOWN`. Another
  no-dependency, no-execution stage of the ADR-0015 provider program shrinking
  the statically-determinable slice of `DynamicImport`.
- Python project-config parsing now also reads root `setup.cfg` (INI) with the
  standard-library `configparser`, in addition to `pyproject.toml`. Sanitized
  project name and repo-relative source roots from `[tool:pytest]` test paths and
  `[options.packages.find] where` are merged into the same project-config context
  used for repo-local import resolution; unsafe paths are dropped and a malformed
  `setup.cfg` yields a typed `MissingProjectConfig` `UNKNOWN`. This widens config
  coverage for setup.cfg-based projects without executing `setup.py`, resolving
  dependencies, or proving any family claim, and is a no-dependency,
  no-execution stage of the ADR-0015 provider program.
- Python project-config parsing now also reads root `setup.py`, discovered as a
  Python config file and parsed with the standard-library `ast` module — it is
  **never executed**. Only complete literal source-root evidence is extracted (a
  unique string-to-string `package_dir` dict and one literal positional-or-
  keyword `where` for `find_packages` / `find_namespace_packages`) from direct,
  aliased, or module-qualified imports lexically traced to `setuptools`, with no
  recognized name, attribute, or namespace mutation. Only a direct
  unconditional zero-positional module-body `setup(...)` without keyword
  unpacking is authoritative, and finder roots must be its direct `packages=`
  value. Duplicate/overridable relevant keywords, partial/dynamic mappings,
  ambiguous finder arguments, explicit builtins mutation, and setup calls after
  an unconditional top-level `raise` fail closed. A recognized but incomplete
  call yields typed `MissingProjectConfig`; `setup()` remains valid empty config.
  Exactly one authoritative call is required, with multiple calls producing
  `ConflictingFacts`. Roots across all three config formats form only a
  deduplicated structural candidate union, not a setuptools-precedence proof.
  This widens source-root coverage for legacy `setup.py`-based projects without
  executing `setup.py`, running `find_packages`, resolving dependencies, or
  proving any family claim, and is another no-dependency, no-execution stage of
  the ADR-0015 provider program.
- Python pytest analysis now resolves a bounded allowlist of distinctive
  well-known plugin fixtures (for example pytest-mock `mocker`, pytest-asyncio
  `event_loop`, pytest-freezer `freezer`) to `pytest.plugin_fixture.*` external
  context anchors instead of `PytestFixtureInjection` `UNKNOWN`. Repo-local and
  `conftest.py` fixtures still resolve first, the context is structural metadata
  only (never family support, treated like built-in fixture context), and plugin
  fixtures outside the allowlist stay typed `UNKNOWN`. This is the first landed
  stage of the ADR-0015 provider program (the no-dependency, no-execution
  subset).
- The `PARTIAL_CONTEXT` local-context fallback now covers every fuzzy family
  block, not only the no-candidate case. When the fuzzy family probe blocks with
  a too-broad or truncated candidate set (`query target candidate set`) or
  several competing families (`query target ambiguity`), a target that still
  resolves to exactly one indexed repo-relative path or code unit earns a
  bounded read plan with a typed `InsufficientSupport` unknown, while the family
  stays unguessed. Exact family/member lookups and still-ambiguous or
  unresolvable targets keep the typed `UNKNOWN`, and no source is returned by
  default.
- Storage now uses the top-level mutable `.repogrammar/repogrammar.sqlite`
  database as the normal index store, with active generation state held in
  `index_generations` rows and legacy `current-generation`/`generations/`
  support retained only as fallback. Query commands now return
  `PARTIAL_CONTEXT` for a uniquely resolved indexed target that lacks family
  evidence, preserving typed `InsufficientSupport` instead of inflating it into
  a family claim. The partial-context resolver now preserves embedded paths,
  `path:line`, `path:start-end`, symbol hints, residue terms, candidates, and
  advisory `check` metadata without proof-like conformance fields. The mutable
  storage schema now records derived-record dependencies and dirty-record
  markers, reports their active counts through status/doctor, and refuses active
  family or semantic reads when dirty rows or dependency/hash mismatches are
  present. Re-recording an unchanged indexed file is now idempotent, while
  replacing a changed path removes stale path-scoped records and marks derived
  dependents dirty in the same SQLite transaction. Removed indexed paths now
  use the same fail-closed path-scoped cascade and dirty-marker behavior.
  Successful mutable index activation and mutating mutable prune operations now
  apply bounded SQLite maintenance with `PRAGMA optimize` and a passive WAL
  checkpoint without running automatic `VACUUM`. Explicit
  `repogrammar compact --dry-run --json` and `repogrammar compact --yes`
  commands now report mutable SQLite database/WAL/SHM size metadata, require the
  index lock, validate the active generation, and reserve full `VACUUM` for
  confirmation-gated compaction with a truncating WAL checkpoint only. Status
  and doctor now also report storage layout (`empty`, `mutable`, `legacy`, or
  `mutable_with_legacy`), mutable-database presence, legacy layout presence, and
  WAL/SHM sidecar byte counts when a mutable database exists.
- `repogrammar sync` now performs path-level incremental updates when the active
  mutable generation is readable, schema-compatible, and dirty-free. It
  copy-forwards unchanged file/code-unit/IR/non-derived semantic records into a
  new generation, reparses added or modified paths, drops removed paths,
  recomputes local derived support and families, and reports delta counters in
  `sync --json`. Project-context changes, configured semantic workers, unsafe
  storage layouts, or dirty active records now report `sync_mode:
  full_rebuild_fallback`; `index` and `resync` remain full rebuilds.
- Public-preview hardening now blocks React-shaped TypeScript semantic-worker
  facts from forming unsupported JS/TS family claims, applies safe
  `tsconfig.json` / `jsconfig.json` `baseUrl` prefixes to JSON path aliases,
  documents that Jest/Vitest script configs are metadata/typed `UNKNOWN` only,
  and recommends explicit preview tags instead of GitHub's `latest` redirect for
  prerelease installer usage.
- Installer launchers now validate release archive entry names before
  extraction and reject unsafe or unexpected paths even when the checksum
  matches. The npm launcher also validates release tags/cache containment and
  stages binary+worker cache updates before swapping them into place.
- Installer wrapper stale-PATH cleanup now exits nonzero when a requested prune
  cannot remove an outdated `repogrammar` copy, instead of reporting a
  successful install or prune with the stale executable still ahead on PATH.
- Python indexing now builds a bounded static repo-local import and pytest
  fixture graph from CPython `ast` context. Unique local module imports, direct
  imported top-level symbols, static package re-exports, literal `__all__` star
  imports, same-file/conftest fixture edges, and literal
  `request.getfixturevalue("name")` lookups are persisted as source-tied
  `DATAFLOW_DERIVED` graph facts with `provider_resolved=false`; ambiguous,
  dynamic, plugin, unsafe star, external, or unresolved cases remain typed
  `UNKNOWN`.
- FastAPI module-level `include_router(router, prefix="...")` calls now record
  exact local or repo-local imported router-prefix context without treating it
  as route-family support. Dynamic prefixes, unresolved/external routers, and
  router factories remain typed `UNKNOWN`.
- SQLAlchemy `relationship("LocalModel")` calls now record same-module literal
  relationship-target context without treating it as model-family support.
  Dynamic or nonlocal relationship targets remain typed `UNKNOWN`.
- SQLAlchemy `Base = declarative_base()` assignments now propagate bounded
  declarative base context to direct `class Model(Base)` declarations when the
  imported helper is not shadowed.
- Pydantic model fields assigned with imported `Field(...)` now record bounded
  field-metadata context without using metadata arguments as model-family
  support.
- Dynamic Pydantic `model_config = ConfigDict(...)` values now preserve typed
  `FrameworkMagic` UNKNOWNs instead of upgrading dynamic config behavior into
  model-family support.
- Pydantic runtime validator body calls now preserve
  `pydantic_validator_side_effects` UNKNOWNs as non-blocking subclaim metadata
  instead of treating validator behavior as static model support.
- Pydantic- or SQLAlchemy-shaped classes that inherit imported external bases
  now preserve framework-identity UNKNOWNs instead of proving base semantics
  from local syntax.
- Python SQLAlchemy typed `Session` / `AsyncSession` query calls now preserve a
  claim-scoped `sqlalchemy_query_shape` UNKNOWN for direct raw SQL strings or
  imported `text(...)` SQL text while keeping receiver-call context.
- SQLAlchemy repository methods that call untyped runtime-injected
  `self.session`-style receivers now preserve `RuntimeDependencyInjection`
  UNKNOWNs instead of canonicalizing them as exact session calls.
- Local custom SQLAlchemy query wrapper calls now preserve framework-magic
  UNKNOWNs instead of treating the wrapper call as static repository support.
- SQLAlchemy `event.listen(...)` and `event.listens_for(...)` listener hooks now
  preserve framework-magic UNKNOWNs instead of counting runtime event behavior as
  static model or repository support.
- SQLAlchemy dynamic model class factories such as `type(..., (Base,), ...)`
  now preserve framework-magic UNKNOWNs when the dynamic base resolves to a
  SQLAlchemy declarative base.
- SQLAlchemy repository-method exact anchors now include typed
  `Session.get(...)` and `AsyncSession.get(...)` receiver calls. Plain `.get`
  calls without a typed SQLAlchemy session receiver remain non-SQLAlchemy
  context.
- TS/JS project context now records safe JSON `rootDirs` from root
  `tsconfig.json` / `jsconfig.json` and uses them as a bounded fallback for
  repo-local relative import resolution. Unique rootDirs targets become
  `STRUCTURAL` `RESOLVED_IMPORT` context facts, while unresolved or conflicting
  rootDirs candidates remain typed `UNKNOWN`.
- Fastify exact-anchor parsing now records exact local
  `register(plugin, { prefix: "..." })` plugin/prefix context as
  `fastify.plugin.register` without adding it to Fastify route support targets.
  Dynamic prefixes, imported plugins, missing plugins, and plugin side effects
  remain typed `UNKNOWN` or context only.
- TS/JS exact-anchor binding now accepts CommonJS destructuring aliases from
  exact supported framework packages, covering Express routers, Fastify
  factories, Prisma clients, and Drizzle table/db factories without treating
  custom wrappers or injected clients as support.
- TypeScript semantic-worker requests now carry bounded operation scopes for
  module specifiers, exports, re-exports, and package entries. The checked-in
  Node worker can use the TypeScript compiler API for provider-resolved module
  facts when a TypeScript module is available, and otherwise emits only
  structural fallback facts or typed `UNKNOWN`s with `provider_resolved=false`.
  Rust-side validation now matches returned facts against the requested
  path/hash/code-unit/range and operation provenance before storage.
  `export * from "<specifier>"` parser UNKNOWNs now carry bounded re-export
  operation provenance as `<specifier>#*`, letting configured workers return
  matching `resolve_reexport` context without turning fallback or unresolved
  re-export facts into family support.
- Configured TypeScript workers now receive `resolve_export` operations for
  exact Next.js file-convention route/page/layout/API anchors. When the
  TypeScript compiler API proves the matching export name for the same
  path/hash/code-unit/range, the application layer records a
  provider-resolved TS/JS-derived support fact with `provider=typescript` and
  `query_operation=resolve_export`. Dependency-free fallback export facts still
  carry `provider_resolved=false` and remain context only.
- Prisma TS/JS anchors can now represent relative repo-local named shared-client
  imports such as `import { prisma } from "./db"` as provider-required
  candidates. Configured TypeScript workers receive a bounded
  `resolve_reexport` operation like `./db#prisma`; only matching
  TypeScript-provider facts with `provider_resolved=true` can produce
  provider-resolved Prisma support. External shared clients, fallback worker
  facts, callback transactions, dynamic operations, and raw SQL remain
  non-supporting context or typed `UNKNOWN`.
- Express and Fastify shorthand routes now treat relative repo-local named
  handler imports as provider-required candidates. Configured TypeScript
  workers must prove a matching `resolve_reexport` operation such as
  `./handlers#listUsers` before those imported-handler routes contribute
  support. External handler imports, missing handlers, fallback worker facts,
  and unresolved worker facts remain non-supporting context or typed `UNKNOWN`.
- Express route anchors now record exact literal `app.use("/prefix", router)`
  mounts on subsequent exact router routes as `route_prefix_shape` and
  `effective_route_path_shape` context. Dynamic prefixes, middleware side
  effects, and wrapper routers remain outside support.
- Drizzle query anchors now treat relative repo-local named `db` and table
  imports as provider-required candidates. Configured TypeScript workers must
  prove every required `resolve_reexport` binding, such as `./db#db` and
  `./schema#users`, for the same path/hash/code-unit/range before those
  imported Drizzle query shapes contribute support. External imports, missing
  proofs, fallback worker facts, dynamic builders, and raw SQL remain
  non-supporting context or typed `UNKNOWN`.
- Rust cfg/cfg_attr build-variant UNKNOWNs now carry bounded Cargo feature
  context, including the nearest discovered `Cargo.toml`, feature predicate
  names, and whether each feature is declared there. These assumptions improve
  `cargo_feature_cfg_model` triage and family UNKNOWN recovery text without
  evaluating cfgs or converting the UNKNOWN into family support.
- `repogrammar stats --json` now includes source-free `by_language` readiness
  buckets that keep official Python v0.1 diagnostics separate from bounded
  TS/JS, Rust self-dogfood, and Java preview scopes. `stats --unknowns --json`
  and `unknowns --json` also include readiness-scoped language UNKNOWN detail
  without exposing paths, code-unit ids, fact ids, snippets, or free-text
  recovery guidance.

### Fixed

- `repo-guard check` now recognizes direct `.claude/worktrees/` children only
  when their bounded regular `.git` pointer resolves under this repository's
  linked-worktree metadata, so parallel-agent checkouts no longer appear as
  nested guides or source outside `src/`; unlinked files and ordinary
  directories named `worktrees` remain under all repository rules.
- Root `setup.py` now reaches the real `RepoGrammarSourceParser` project-config
  path instead of being discovered as `python-config` and then dropped as
  `UnsupportedLanguage`. The product parser continues to use CPython `ast`
  without executing the file: complete literal `package_dir`/finder roots are
  sanitized only when their calls are lexically import-bound to `setuptools`
  without a recognized binding or module-attribute mutation; same-leaf
  local functions, unrelated helpers, standalone finder calls, conditional/dead
  setup calls, and import bindings conditionally rebound, deleted, or explicitly
  mutated through attributes/namespaces before a call abstain. Builtins-qualified
  mutation helpers are covered too. Positional or unpacked setup calls,
  duplicate relevant keywords, partial/dynamic/duplicate `package_dir` entries,
  ambiguous or unpacked finder arguments, lookalike finders, and setup after a
  definite top-level `raise` emit `MissingProjectConfig` without forged roots;
  `setup()` remains a valid empty config. Multiple authoritative calls become
  typed `ConflictingFacts`, malformed syntax stays `MissingProjectConfig`, and
  similar or nested `.py` paths are not misrouted as
  root config. Project-config fact provenance now truthfully distinguishes
  `tomllib`, `configparser`, and `cpython_ast`, and source-root extraction accepts
  only the method that matches the exact config path. The application unions
  roots from coexisting config formats only as structural candidate context; it
  does not infer setuptools precedence or turn config conflicts into strong
  claim evidence. Real product-parser and discovery regressions replace the
  false confidence previously provided by a RecordingParser-only setup.py
  context test.
- Codebase-wide review remediation:
  - Security: the TypeScript worker no longer loads the analyzed repository's own
    `typescript` package by default (it was arbitrary code execution on an
    untrusted repo); loading the project's TypeScript is now opt-in behind
    `REPOGRAMMAR_TSJS_TRUST_PROJECT_TYPESCRIPT=1`, and TypeScript config parsing
    is bounded to the project root. The Python worker fails safe on adversarial
    input (a deeply chained expression that overflows recursion becomes a typed
    `worker_error`, not a crash + truncated stream) and enforces an aggregate
    source-byte budget. The shell installer rejects non-regular-file archive
    members (symlink/hardlink) before extraction. MCP request lines are read
    under a bounded 1 MiB read; the TypeScript worker boundary drains
    stdout/stderr concurrently while writing stdin (deadlock fix); the
    `cargo metadata` provider bounds captured output and adds a timeout.
  - Robustness: auto-sync daemon liveness now verifies process identity (a
    reused PID no longer causes `stop` to signal a stranger or block `start`),
    and `stop` is resilient to an already-exited daemon; blocking family
    `UNKNOWN`s are gated on each language's anchor engine; an evidence-less
    family is no longer served as fresh; incremental `sync` recomputes
    provider-resolved TS/JS support so it matches a full rebuild.
  - Storage: active reads no longer run `PRAGMA integrity_check` (kept at
    activation/doctor/compaction); read-model reads use a read-only connection
    and no longer write migrations; re-derived records clear their dirty markers;
    `apply_migrations` rejects a newer-than-supported schema version.
  - CLI/parsing: `serve --help` prints usage; both binaries tolerate a broken
    pipe instead of panicking; `files`/`units` reject inapplicable flags;
    `install` option parsing guards against a following flag; structural
    candidate classification is tightened (arrow-vs-callback, Next.js
    `src/pages/`, Rust method-by-receiver, `#[path]` attribute, escape-aware
    import specifiers).
  - Installer/npm/telemetry: the source-only `install.ps1` retains its local
    contributor guards; the npm launcher bounds redirects and rejects Windows
    before artifact or override use; telemetry state files are written
    owner-only (`0600`) and a negative token-savings ratio uses a distinct
    `negative` bucket.
- The TypeScript worker now performs path-alias wildcard substitution with
  literal all-occurrence replacement and rejects multi-wildcard path aliases as
  typed `UNKNOWN`s, avoiding incomplete replacement behavior flagged by CodeQL.

### Added

- Public-preview growth assets now include quickstarts, limitations, a
  FastAPI/pytest example, launch-kit copy, contribution guidance, issue/PR
  templates, and a growth-readiness report that separates source-checkout
  dogfood from unpublished release/npm paths.
- UNKNOWN regression benchmark coverage now pins release-fixture
  `unknowns --json` language, reason-code, and required-mechanism buckets for
  Python dynamic behavior, TS/JS framework negative cases, and Rust macro/cfg
  boundaries, while also asserting those negative fixtures do not silently form
  families. It also includes positive unresolved/resolved fixture pairs for
  bounded Python FastAPI, TS/JS Express, TS/JS Prisma, and Rust module
  reductions that require source-backed replacement facts. The protocol is
  documented under `docs/experiments/`.
- Release-readiness documentation now records the source-checkout smoke matrix
  for fresh checkout product tests, npm pack dry-run, `REPOGRAMMAR_BINARY`
  launcher dogfood, repository lifecycle JSON paths, MCP `tools/list`,
  installer/uninstaller dry-run plans, and optional source-tree secret scans.
  Installer dry-runs are documented as human-readable plans rather than JSON,
  and unavailable optional scanners or missing npm lockfiles must be reported
  instead of treated as passing evidence.
- Conservative Java/Spring structural-preview support. RepoGrammar now discovers
  `.java` files, uses Tree-sitter Java for structural Java/Spring code units,
  emits exact imported/FQN Spring MVC/stereotype/Spring Boot/Spring Data
  anchors, derives bounded `repogrammar-java-derived` support under exact target
  and safe-origin gates, and preserves typed `UNKNOWN`s for lookalike
  annotations, nonliteral route paths, DI/proxy/component-scan/runtime/classpath
  behavior, Maven/Gradle/javac/annotation-processor semantics, and low support.
- Conservative TS/JS structural-preview adapters for Next.js, Fastify, Prisma,
  and Drizzle. The new adapter registry adds role-compatible exact-anchor
  promotion with framework-specific `derived_from=tsjs_<framework>_structural_anchors`
  provenance, bounded Next App/Pages conventions including async const route
  handlers with dynamic segment/route-group context assumptions, Fastify
  shorthand and `fastify.route.route` full declarations, Prisma allowlisted
  model operations and array transactions, and Drizzle table/query anchors
  including `db.query.<table>.findMany/findFirst`. Package-only evidence,
  framework-role heuristics, React components/hooks, Next middleware/server
  actions/re-exports, Fastify dynamic/imported plugin registrations, Prisma
  bulk/injected/raw clients, and Drizzle raw/dynamic builders remain `UNKNOWN`
  or non-supporting context.
  New v0.2 fixtures cover positive Next/Fastify/Prisma/Drizzle families and
  negative package/dynamic/raw/bulk/shadowed cases.
- Rust structural self-dogfood indexing for RepoGrammar's own implementation
  families. The new Tree-sitter Rust adapter discovers `.rs` files and
  `Cargo.toml`, extracts structural Rust units, emits typed UNKNOWNs for
  cfg/build variants, macro/proc-macro syntax, unresolved modules, and
  trait-object dispatch, and derives bounded internal `DATAFLOW_DERIVED`
  support only for RepoGrammar-owned roles. Product fixtures under
  `src/fixtures/rust/release/v0_2` prove support>=3 positive families across
  family gates, parser adapters, installer actions, and product tests;
  low-support, macro/cfg, trait-dispatch, conflicting-module, Cargo build-script,
  and unsafe-path abstention; bounded Cargo target-dependency inventory;
  metadata-only default output; stale source refusal; build-script non-execution;
  and explicit source-span opt-in. This is not general Rust semantic analysis.
- CLI family query output, MCP `repogrammar_context` family responses, and
  `repogrammar stats` now surface `estimated_potential_token_savings` as an
  `ESTIMATED` local potential-read-displacement diagnostic. Successful family
  context responses update a repo-local aggregate under
  `.repogrammar/telemetry/local-metrics/` without adding anonymous upload queue
  entries, source text, paths, hashes, prompts, query text, or evidence text.
- `repogrammar autosync` now provides optional repository-local automatic sync
  management. `autosync start` enables and starts a background worker that
  polls the existing discovery fingerprint, debounces file saves, and runs the
  normal delta-aware `sync` path; `status`, `stop`, `disable`, and foreground
  `run` manage the worker. The feature is explicit per repository and is not
  started by MCP serving, agent installation, or queries.
- Public-preview readiness documentation now includes an explicit support matrix
  for Python v0.1, conservative JS/TS v0.2 exact-anchor support, unsupported
  React/broad TS/JS semantics, source-span opt-in, token-saving claim limits, and
  installer platform boundaries. A readiness report and real-repo dogfood
  protocol were added under `docs/reports/` and `docs/experiments/`.
- Release/install readiness tests now verify the exact four macOS/Linux npm and
  release targets, explicit Windows/unsupported-architecture rejection,
  required bundled Python worker assets, Bash installer state-boundary
  behavior, exact packaged `setup`/`find`/`check` smoke, and `install.sh`
  checksum publication.
- Additional v0.2 JS/TS fixtures cover JavaScript Jest/Vitest family support,
  exact Next/Fastify/Prisma/Drizzle positives, and React/package-only/dynamic/raw
  lookalikes that must not form public family rows.
- Conservative TS/JS exact-anchor family support for Express route handlers,
  Jest/Vitest suites/tests, and structural-preview Next.js, Fastify, Prisma, and
  Drizzle adapters. The syntax parser emits `STRUCTURAL` anchors only for exact
  local framework bindings and file conventions; reassigned, shadowed,
  dynamic-receiver or dynamic-method, custom-wrapper, conditional-import, raw,
  object-literal, and framework-magic lookalikes stay `UNKNOWN`. The application
  layer promotes those anchors to `DATAFLOW_DERIVED` support facts (engine
  `repogrammar-tsjs-derived`, method `bounded_exact_anchor_v1`), and the loose
  substring compatibility gate is replaced by an exact target whitelist plus a
  safe-origin check. TS/JS family construction now requires at least three
  compatible support facts, uses complete-link clustering over conservative
  route/test/component/query feature profiles, records variation slots, and
  keeps project-config inventory
  (`package.json`, `tsconfig.json`, `jsconfig.json`, Jest/Vitest config files)
  as structural context or typed config `UNKNOWN` only. React components/hooks
  remain `UNKNOWN`. CLI/MCP `find`/`check`/`family` and the source-span renderer
  work for JS/TS fixtures; default output stays source-free and
  `--include-source-spans` / `include_source_spans=true` returns bounded
  hash-checked line-numbered spans. New fixtures live under
  `src/fixtures/typescript/release/v0_2`. This is a token-saving foundation, not
  full TS/JS semantic analysis or an official v0.1 target change.
- Bounded TS/JS project context now feeds parser-mode indexing: discovered
  TS/JS module paths, JSON `tsconfig.json`/`jsconfig.json` path aliases, and
  package/config Jest/Vitest context are passed into the syntax parser. Unique
  literal relative imports and unique path-alias imports can be persisted as
  `STRUCTURAL` `RESOLVED_IMPORT` context facts, while dynamic imports,
  non-literal or conditional `require`, unresolved or conflicting aliases, and
  star re-exports persist typed `UNKNOWN`s. Ambient Jest/Vitest globals now
  require package/config test-runner context; test-file location alone is not
  enough to form support.
- The installer target registry is now exposed through a per-target adapter
  contract (`TargetAdapter`) that consolidates scope support, live-writer
  status, the no-write config preview, and `describe_paths` planning. Dry-run
  output now reports a per-target instruction-file plan line that names the
  `REPOGRAMMAR_INSTRUCTION_FILE_<TARGET>` override and its deferred default.
  Live writes still cover only global Codex and Claude Code, and `--target all`
  still installs only live-supported targets all-or-rollback.
- The installer now has a reversible, idempotent managed instruction-file
  writer using the exact markers `<!-- BEGIN REPOGRAMMAR MANAGED SECTION -->`
  and `<!-- END REPOGRAMMAR MANAGED SECTION -->`. It creates, appends, replaces,
  or leaves unchanged the managed section, refuses malformed or partial markers,
  writes atomically with re-read verification, and on uninstall or rollback
  removes that section, deleting a file RepoGrammar created when removal leaves
  it empty while preserving any pre-existing or user-added content. Receipts now
  record `instruction_file_path` and
  `instruction_action`. Live instruction writing stays deferred unless
  `REPOGRAMMAR_INSTRUCTION_FILE_<TARGET>` resolves to an absolute path, because
  real Codex/Claude instruction-file locations are not yet verified.
- `repogrammar index` and `repogrammar sync` now emit progress while they run.
  Human progress uses stderr and exact completed/total counts when known;
  `--json --progress always` keeps the final JSON result on stdout while
  rendering progress-bar output on stderr.
- CLI and MCP family queries now support explicit bounded source-span rendering
  (`--include-source-spans` / `include_source_spans: true`) over hash-checked
  read-plan spans. Default output remains metadata-only, and stale or omitted
  spans carry Read/Grep fallback guidance.
- Source-checkout installer dogfood now works before GitHub prerelease assets
  or npm publication exist. The Bash wrapper can install the built contributor
  binary through explicit `--from-source` flows, writes the command through the
  same managed install layout used by the Rust installer, delegates agent
  wiring to `repogrammar install`, backs up and replaces older unmanaged
  `repogrammar` command files during explicit CLI installation, and reports
  actionable missing-release guidance. The npm launcher now has a tested
  `REPOGRAMMAR_BINARY` local dogfood bypass while keeping release artifacts as
  the published default.
- Interactive `repogrammar install` now provides a dependency-light text wizard
  for machine-level Codex and Claude Code MCP wiring. The wizard supports
  multi-select in one run, skips already managed RepoGrammar receipts by
  default, keeps telemetry default-off, and does not initialize or index
  repositories. Noninteractive `--target all --scope global --yes` uses the
  same all-or-rollback multi-agent transaction, and `uninstall --target all`
  removes only RepoGrammar-owned first-class agent receipts.
- Re-running `repogrammar install` now still installs or repairs the
  user-writable `repogrammar` command when selected agents are already managed.
- Managed installs now place bundled Python worker assets under install state
  that the managed executable can discover, and refresh rollback restores the
  previous managed executable and command copy if self-test or native agent
  configuration fails.
- The installer now has a CodeGraph-style target registry for planning and
  configuration previews. `--target` accepts `auto`, `all`, `none`, aliases,
  and comma-separated concrete target lists; `--location` aliases `--scope`;
  and `--print-config <target>` prints no-write MCP snippets for known targets
  such as Cursor, opencode, Hermes, Gemini, Antigravity, and Kiro while live
  writes remain gated to global Codex and Claude Code.
- Source checkouts now include `src/install/repogrammar-install.sh`, a
  dependency-light TUI wrapper for downloading and verifying a prebuilt release
  binary, installing or repairing the command, configuring or uninstalling
  Codex/Claude Code integrations, removing the local command path after
  confirmation, or explicitly choosing a contributor source build.
- Release automation now builds exactly four prebuilt `repogrammar` artifacts
  with checksum assets for macOS arm64/x86_64 and Linux arm64/x86_64, bundles
  the current Python worker asset, and publishes `install.sh` plus its checksum
  for tagged preview releases. Windows and `install.ps1` are outside this
  preview publication set. Real
  downloadable prerelease artifacts are available only after a preview tag is
  published.
- Added the `@sioyooo/repogrammar` npm package manifest and thin `npx`
  launcher. The launcher downloads and verifies the same prebuilt release
  artifacts, caches the binary and bundled worker asset, and delegates all
  behavior to the Rust CLI without requiring Cargo.
- Repository bootstrap for the RepoGrammar Rust-core package layout.
- Layered architecture skeleton for core, ports, application, interfaces, and
  adapters.
- Language-native semantic worker boundary and TypeScript worker protocol
  placeholder.
- Semantic worker v1 protocol tokens, message schemas, and NDJSON fixtures for
  TypeScript semantic facts and unsupported-version fallback.
- Semantic worker v1 request schema and TypeScript request fixture for the
  Rust-to-worker stdin contract.
- Rust-side TypeScript semantic-worker process adapter that writes request JSON
  over stdin, enforces a timeout, validates bounded NDJSON v1 stdout, converts
  fact messages into RepoGrammar-owned semantic facts, and sanitizes
  unavailable, unsupported-version, crash, timeout, and protocol-violation
  failures.
- Dependency-free TypeScript worker executable stub that validates the v1 stdin
  request contract and emits sanitized NDJSON `worker_error` plus
  `end_of_stream` fallback output when compiler-backed semantic analysis is
  unavailable.
- CPython AST Python worker with private parse-document JSON output for the
  Rust parser adapter and semantic-worker-compatible NDJSON framework-role
  heuristic smoke output, without running repository code or provider tools.
- Python worker structural fact output for import bindings, decorator anchors,
  class bases, simple call targets, `pytest.test` test-function anchors,
  same-file pytest test and fixture dependency edges, and typed
  dynamic/unresolved `UNKNOWN` cases, now including path-derived module-name
  anchors, CPython `symtable`
  structural scope anchors, and a private
  `tomllib` project-config summary mode. Its semantic-worker-compatible
  project mode now resolves only unique repo-local module imports as
  `STRUCTURAL` facts, resolves requested-project `conftest.py` fixture names
  through pytest's directory hierarchy as structural fixture-edge facts, and
  reports ambiguous/missing repo-local imports, unsafe/unresolved literal
  dynamic imports, `__import__`, `locals()[...]`, `eval`, `exec`, `compile`, or
  `sys.path` mutation as typed `UNKNOWN`. Default parser-mode indexing now
  passes discovered repo-relative `.py` inventory and bounded discovered
  `conftest.py` contents into private parse-document requests so source-tied
  repo-local import facts, same-file fixture dependency facts, and
  parent-directory pytest fixture-edge facts can be persisted without launching
  a Python semantic worker; oversized context
  payloads fall back to contextless parsing. The worker now performs file-local
  simple FastAPI router/app alias propagation with same-name reassignment
  invalidation, emits structural FastAPI `response_model`, `Depends`, and
  static `Depends(get_db)` dependency-target anchors plus `HTTPException`
  call and literal status-code anchors, treats literal
  `pytest.mark.parametrize` arguments as parametrize facts rather than
  fixture-injection UNKNOWNs, and labels Pydantic field, field-type,
  model-config, nested Config, computed-field, validator, and model-validator
  anchors as structural model metadata.
  SQLAlchemy parser anchors now include `Mapped[...]`, `mapped_column(...)`,
  typed `Session`/`AsyncSession` call targets, and bounded propagation from
  `__init__`-assigned `self.session`/`self.db` attributes into repository
  methods with same-method receiver reassignment invalidation.
  FastAPI route parser anchors now include bounded same-function service-call
  context for import-resolved static local forms such as `service = UserService();
  service.list_users()` and `runner = run_query; runner()`, with reassignment
  invalidation and dynamic `getattr(...)` calls preserved as typed `UNKNOWN`.
  Static FastAPI `Body`, `Path`, `Query`, `Header`, and `Cookie` route
  parameter markers now produce structural request-shape anchors; those anchors
  remain context metadata and do not become family support.
  Dynamic decorator factories now produce typed `FrameworkMagic` UNKNOWNs for
  `python_framework_identity`, and `setattr(...)` monkey-patching produces typed
  `MonkeyPatch` UNKNOWNs for `python_call_target`; neither path becomes family
  evidence. Assigned aliases of dynamic import/execution/namespace/lookup and
  monkey-patch functions now remain typed `UNKNOWN` as well, while uniquely
  repo-local literal `importlib.import_module(...)` can still become structural
  import context. Dynamic FastAPI dependency target expressions such as
  `Depends(make_dependency())` now produce typed `RuntimeDependencyInjection`
  UNKNOWNs for the dependency-target sub-claim instead of silently disappearing.
  Literal pytest fixture `name=` aliases now define the fixture binding name,
  while dynamic or unsafe `name=` values remain typed `PytestFixtureInjection`
  UNKNOWNs instead of falling back to the implementation function name.
  Dynamic Pydantic `create_model(...)` factories now remain typed
  `FrameworkMagic` UNKNOWNs instead of becoming static model-family support.
  Bare unresolved decorators now also produce framework-identity
  `FrameworkMagic` UNKNOWNs while local decorators and native `property`,
  `classmethod`, and `staticmethod` remain structural metadata.
  Duplicate applicable pytest `conftest.py` fixture names now produce typed
  `ConflictingFacts` UNKNOWNs for fixture binding, known pytest built-in
  fixtures such as `tmp_path` and `capsys` become metadata-only
  `pytest.builtin_fixture.*` context, and plugin-style fixture names remain
  `PytestFixtureInjection` UNKNOWN without an allowlist or provider.
  The `dynamic-unknown` release fixture now covers dynamic import, `sys.path`
  mutation, dynamic FastAPI dependency targets, pytest fixture-binding
  ambiguity/plugin UNKNOWNs, dynamic call target, dynamic decorator, and
  monkey-patch boundaries through the product indexing/query path.
  A dedicated `pytest-dynamic-fixture-name` release fixture verifies dynamic
  pytest fixture `name=` values stay UNKNOWN through the product path without
  producing fixture-binding support.
  No-worker release smoke now covers direct FastAPI, FastAPI alias, pytest,
  Pydantic model/settings, SQLAlchemy model-field, and SQLAlchemy
  session/repository exact-anchor derived-support family paths without claiming
  provider-backed Python semantics. It also covers exact-anchor `member`,
  `find`, `explain`, advisory `check`, token-budget auto evidence, explicit
  compact/evidence/deep metadata modes, MCP parity for supported operations,
  and stale source mutation/deletion returning `StaleEvidence` `UNKNOWN`.
  Matched CLI and MCP family reads now also include metadata-only read plans
  with repo-relative paths, strict content hashes, byte ranges, purpose labels,
  estimated token costs, and `source_snippets_included: false`. Read plans mark
  target source as required before edits, keep line ranges `null` until safe
  source-span rendering exists, and are suppressed when stale or insufficient
  evidence returns typed `UNKNOWN`. Non-blocking supported-member subclaim
  UNKNOWNs, such as unresolved FastAPI dependency targets, are preserved in
  family detail/query metadata with the concrete family id instead of being
  silently dropped from confident route-family reads.
  `repogrammar stats --json` now reports repo-shape diagnostics for local
  pattern density, family support coverage, abstention rate, and
  thin-wrapper/token-saving risk without reporting measured token savings or
  context compression ratios.
- Anonymous telemetry now has state-backed `status`, `on`, `off`, `export`,
  `upload`, and `purge` commands, remains disabled by default, honors
  environment opt-outs, validates a versioned allowlist payload schema, and
  keeps research trace consent separate. The anonymous payload no longer carries
  a repository instance id, includes only coarse external-dependency risk, and
  treats export as inspect-only rather than queue creation. Enabled
  `stats --json` now writes only an allowlisted local rollup and still performs
  no network upload or queue creation. Telemetry status now reports rollup
  counts, CI disablement, and whether explicit upload would open a network
  connection. Live install now keeps telemetry consent independent from agent
  configuration: `--yes` alone does not enable telemetry, `install --yes`
  without telemetry flags does not prompt and keeps telemetry disabled, and
  dry-run output names the planned native Codex/Claude Code MCP command shape.
  Local paired token
  experiments can now record baseline/treatment token counts through explicit
  `record-existing` or `controlled-pair` confirmation so `stats --json` reports
  measured token savings only when comparable measurements exist. The product
  binary prompts for experiment recording with default-no `[y/N]` when `--yes`
  is absent; controlled-pair prompts warn about possible extra
  token/time/provider cost. Anonymous telemetry payloads include only coarse
  experiment aggregate categories, experiment export is redacted by default,
  and failed treatment correctness invalidates product token-saving claims even
  when a raw token delta is available. Uninstall now refuses missing managed
  receipts instead of silently succeeding.
  FastAPI exact-anchor regression coverage now spans all supported
  FastAPI/APIRouter HTTP route methods (`delete`, `get`, `head`, `options`,
  `patch`, `post`, and `put`) and keeps `api_route`/WebSocket decorators
  deferred. pytest regression coverage now distinguishes the support-eligible
  `pytest.mark.parametrize` decorator from context-only parametrize argument
  anchors.
  Default indexing validates and persists
  parser-origin
  `STRUCTURAL`/`UNKNOWN` facts while keeping them out of support derivation
  and CLI/MCP family evidence. The family builder may consume them only as
  context features or claim-scoped abstention inputs.
- Default Python indexing discovers root `pyproject.toml` as `python-config`,
  reads it through the Rust source-store path/hash boundary, and persists a
  `project_config` unit with sanitized `PROJECT_CONFIG`/`STRUCTURAL` metadata
  or typed config `UNKNOWN` facts; these records stay out of family support and
  claim-input readiness.
- Default Python parser context now reuses sanitized root `pyproject.toml`
  source roots from the existing parser/tomllib project-config facts, alongside
  discovered `.py` inventory and bounded `conftest.py` context, so repo-local
  import facts can reflect configured source roots without adding a second TOML
  parser or provider-backed semantics.
- Bounded Python exact-anchor support derivation: validated CPython structural
  anchors can now produce separate `DATAFLOW_DERIVED` support facts when their
  target exact-matches the Python framework compatibility table for a unit with
  one framework role. Raw parser facts and framework heuristics remain
  insufficient, project-config facts stay blocked, and Python still requires
  three compatible support members before the EC-MVFI-lite family builder writes
  a family.
- Python EC-MVFI-lite family construction now applies bounded complete-link
  clustering over support-family feature groups. Bridge members can no longer
  single-link incompatible Python support families into one confident claim, and
  multiple ready clusters inside one coarse bucket receive stable sanitized
  cluster ids without exposing source snippets or absolute paths.
- Python family construction now also consumes parser-origin context facts in
  its complete-link compatibility check and applies claim-scoped blocking
  `UNKNOWN`s at the final family-builder boundary. Dynamic import/import
  resolution UNKNOWNs block affected family membership, pytest fixture-binding
  UNKNOWNs block pytest family support, and FastAPI dependency-target UNKNOWNs
  remain scoped to the dependency subclaim rather than route-family membership.
- Ready Python families now record metadata-only variation slots when
  parser-context profiles such as FastAPI effect markers or service-call shapes
  differ inside an already-supported family. These slots do not expose source
  snippets or promote parser context into support evidence.
- pytest family compatibility now requires non-builtin fixture dependency
  profiles to match; known builtin fixture-context differences can remain as
  metadata-only variation/context.
- The CPython AST worker now resolves import aliases and module-level dynamic
  UNKNOWN propagation by source position. A route or model before a top-level
  shadowing assignment can still use exact framework imports, later units
  cannot, local `@client.get(...)` no longer becomes a FastAPI route, local
  `BaseModel`/`Base` classes no longer become Pydantic/SQLAlchemy support, and
  bare `locals()`/`globals()` calls now produce typed call-target `UNKNOWN`s.
- Product-path regression coverage now indexes local framework lookalikes and
  proves `@client.get(...)`, user-defined `BaseModel`, and user-defined
  SQLAlchemy-shaped `Base` classes stay out of family support and query claims.
- Query input hardening now shares target and token-budget validation between
  CLI and MCP. MCP schema exposes `target` max length and `token_budget`
  maximum, and both interfaces reject oversized or control-character targets.
- File discovery now supports explicit strict gitignore mode via
  `REPOGRAMMAR_STRICT_GITIGNORE=true`, which treats unavailable Git ignore
  checks as an error. Non-strict discovery keeps the previous warning fallback.
  Gitignore regression coverage now includes Python files in root and parent
  worktree project layouts.
- Narrow Python exact-anchor variation metadata: when an already-ready Python
  family has multiple exact-compatible framework-anchor support targets, the
  family builder records a dedicated variation slot and one metadata-only
  `variation` evidence label. This does not add provider-backed semantics,
  source snippets, exception mining, or runtime-equivalence claims.
- Python family claim-boundary regression coverage now proves FastAPI
  `response_model`, static dependency-target, `Depends`, `HTTPException`, and
  literal HTTPException status-code structural anchors do not create membership
  support or alter exact-anchor support targets. Family detail UNKNOWN output
  now scopes runtime-equivalence gaps to the concrete family id, and human
  `families` output preserves typed stale-evidence UNKNOWN details.
- Product-path Python auxiliary-context regression coverage now proves FastAPI
  request body/path/query/header/cookie anchors and SQLAlchemy
  `relationship`/`Session.add` anchors are persisted as CPython structural
  facts, blocked from claim-input readiness with `InsufficientSupport`, and
  absent from derived family-support facts.
- CPython AST worker SQLAlchemy structural anchors now include
  `sqlalchemy.orm.relationship` and `Session.add`/`AsyncSession.add` effect
  calls. These anchors remain structural context and are explicitly excluded
  from family membership support.
- SQLAlchemy repository-method exact anchors now include direct
  `Session.scalar`/`Session.scalars` and async session equivalents, with a
  release smoke fixture proving derived family support without source snippets.
- SQLAlchemy transaction-boundary exact anchors now have derivation and product
  release-smoke coverage for sync/async `Session.commit` and
  `Session.rollback`, while keeping transaction equivalence unclaimed and
  `Session.add` as context/effect metadata only.
- CPython AST worker pytest fixture detection is now alias-aware for same-file
  and `conftest.py` contexts. Direct parametrize arguments take precedence over
  same-name fixtures, indirect parametrize arguments stay typed
  `PytestFixtureInjection` `UNKNOWN`, and fixture-edge/parametrize-argument
  anchors remain excluded from family support.
- CPython AST worker Pydantic model-member structural anchors now include
  fields, field annotation targets, `model_config`, nested `Config`,
  `computed_field`, validators, and `model_validator`. These anchors remain
  schema/config/member context and are explicitly excluded from family
  membership support. Pydantic validator anchors are no longer accepted as
  exact-anchor family support; v0.1 Pydantic family support is limited to
  compatible model/settings base targets.
- CPython AST worker FastAPI service-call structural anchors now recover only
  bounded same-function static call targets and remain handler/service context,
  explicitly excluded from route-family membership support.
- Rust `ports::python_provider` contract for future candidate-scoped
  Pyrefly/Pyright/RightTyper provider requests, provenance assumptions,
  cache-key dimensions, and recoverable provider-unavailable `UNKNOWN`s. This
  does not execute provider tools or add production provider-backed Python
  semantics.
- Application-layer Pyrefly framework-identity request planning for plausible
  Python candidate groups. The planner validates future-provider request scopes
  from in-memory facts or active-generation snapshots and skips
  claim-blocking parser `UNKNOWN`s without executing Pyrefly, storing provider
  facts, changing CLI/MCP output, or upgrading family claims. Planner tests now
  cover import-resolution, framework-identity, and pytest fixture-binding
  blockers.
- Exact-anchor Python support derivation is now sound-by-abstention for bounded
  framework-family claims: a parser-origin blocking `UNKNOWN` prevents the
  affected unit from contributing `DATAFLOW_DERIVED` family support, while
  FastAPI dependency-target UNKNOWNs stay scoped to dependency-target subclaims.
- CPython exact-anchor derivation now ignores framework imports shadowed by
  later same-name top-level definitions or assignments, treats only top-level
  imports as file-level framework aliases, and copies module-level dynamic
  import or `sys.path` mutation into unit-scoped blocking `UNKNOWN`s for later
  family-shaped units instead of letting those units contribute support.
- Compact/evidence/deep family output modes for CLI and MCP family detail.
  Compact is now the default and omits evidence records; evidence/deep return
  selected repo-relative evidence metadata under an optional token budget and
  explicitly report that source snippets are not included.
- Greedy family evidence selection metadata for CLI and MCP. Evidence/deep
  output now reports the selector strategy, rough budget satisfaction, covered
  claim labels, and missing requested variation/exception coverage instead of
  preserving raw storage order or inferring unsupported coverage from notes.
- Schema-backed family evidence coverage labels. The pre-release storage schema
  v5 now persists validated `covered_claims` labels for family evidence, and
  query selection consumes those labels rather than inferring claim coverage
  from notes or storage order.
- Python v0.1 release fixture smoke coverage for FastAPI, pytest, Pydantic,
  SQLAlchemy, mixed, dynamic-unknown, and low-support examples, plus a test-only
  strong FastAPI semantic-support fixture that validates family reads, stale
  evidence fallback, leakage guards, and a no-worker exact-anchor FastAPI
  positive path without claiming production Python semantic-provider support.
  The no-worker release path now also exercises the committed `stale-evidence`
  fixture for source mutation/deletion and the FastAPI/APIRouter route-method
  variation fixture across `delete`, `get`, `head`, `options`, `patch`, `post`,
  and `put`.
- Python discovery regression coverage now explicitly covers `.venv`, `venv`,
  `env`, `.tox`, `.nox`, `__pycache__`, `.pytest_cache`, `.mypy_cache`,
  `.ruff_cache`, `build`, `dist`, `site-packages`, and nested Python runtime
  cache/dependency directory segments as default exclusions.
  Dynamic release smoke now asserts each dynamic boundary is persisted as typed
  `UNKNOWN`, blocked from claim-input readiness, and absent from derived
  support. Worker and parser regression tests now also distinguish safe literal
  `importlib.import_module(...)` anchors from unsafe/nonliteral dynamic imports,
  cover `sys.path.insert`, and prove plain `getattr(...)` assignments do not
  become dynamic call-target evidence. They also pin generic Python
  `module`/`function`/`async_function`/`class`/`method` code-unit output apart
  from framework-specialized units.
- Metadata-only algorithm paper archive for syntax, semantics, retrieval,
  graph fingerprints, alignment, anti-unification, clustering, evidence
  selection, evaluation, and installer supply-chain references.
- Parallel-agent implementation and post-implementation logic-review
  requirements in the mirrored agent contract.
- Historical TypeScript/JavaScript-first MVP language policy, now superseded by
  ADR-0011's Python-first v0.1 target.
- Pattern-family-first CLI command surface, with CodeGraph-style graph commands
  rejected as top-level v0.1 commands.
- Stable `stats --json` output that exposes metric-kind vocabulary and
  repo-shape diagnostics without reporting measured token savings or source
  snippets.
- Safe contracts for agent installation, initialization progress, metrics, and
  telemetry consent.
- Repo-local lifecycle implementation for `init`, `uninit`, `status`,
  `doctor`, `unlock`, and `logs`, with parser/mining behavior kept deferred.
- TS/JS file discovery substrate with repo-relative metadata, strict SHA-256
  content hashes, default generated-directory skips, Git ignore checks,
  symlink-escape rejection, size-limit handling, and deterministic skip
  reasons.
- Python `.py` discovery with repo-relative metadata and default skips for
  common Python virtualenv, cache, and dependency directories.
- SQLite storage substrate behind a port, including generation-scoped
  migrations, WAL and foreign-key PRAGMAs, required-table validation,
  repository-relative indexed-file records, active-generation pointer
  activation, and rollback preservation when validation fails.
- Semantic-fact/evidence storage substrate that records validated facts only for
  building generations when evidence matches an indexed code unit, content hash,
  repository-relative path, and byte range.
- Syntax-only `index` and `sync` integration that runs TS/JS discovery, reads
  source through a repo-relative hash-checked boundary, stores repo-relative file
  metadata and structural code-unit records in a new SQLite generation, validates
  it, and atomically activates `.repogrammar/current-generation` without claiming
  semantic-worker-derived facts, mining, query, or family evidence.
- CodeUnit-derived structural IR storage for syntax-only indexing, with one IR
  node per code unit, conservative containment edges, empty IR payloads, and
  same-generation SQLite validation without introducing family claims.
- CPython AST-backed Python structural code-unit extraction for modules,
  functions, async functions, classes, methods, FastAPI route-shaped functions,
  pytest tests/fixtures, Pydantic model-shaped classes, SQLAlchemy model-shaped
  classes, and SQLAlchemy repository method-shaped functions.
- Lightweight TS/JS framework-role fact storage for syntax-origin Express,
  React, and Jest/Vitest code-unit shapes. Stored facts use
  `FRAMEWORK_HEURISTIC` certainty and unresolved-binding assumptions, and do not
  enable pattern-family query commands.
- Lightweight Python framework-role fact storage for CPython AST-origin
  FastAPI, pytest, Pydantic, and SQLAlchemy code-unit shapes. Stored facts use
  `FRAMEWORK_HEURISTIC` certainty and unresolved-binding assumptions, and do not
  enable pattern-family query commands.
- Internal CPython AST parser fact storage for Python import, decorator,
  class-base, call, fixture-edge, and typed dynamic/unresolved `UNKNOWN`
  anchors. Stored facts are source-snippet-free, same-generation validated, and
  blocked from claim-input readiness as insufficient support.
- Opt-in semantic-worker fact ingestion for `index` and `sync` when
  `REPOGRAMMAR_TYPESCRIPT_WORKER` names an explicit worker executable, with
  optional argv supplied by `REPOGRAMMAR_TYPESCRIPT_WORKER_ARGS_JSON`. Accepted
  facts are recorded only through the same-generation code-unit path/hash/range
  storage gate; worker fallback remains syntax-only, and stale or mismatched
  semantic evidence aborts the new generation.
- Active `files` and `units` read paths that return repo-relative
  file-manifest-only or syntax-only indexed-file metadata and code-unit records
  from the validated active generation without source snippets, absolute paths,
  semantic facts, mining, or family evidence claims; the read path opens the
  active generation read-only and revalidates stored paths, hashes, languages,
  unit ids, and byte ranges before returning records.
- Active semantic-fact/evidence read path for future claim builders, with
  read-only active-generation access and validation of stored fact
  kind/certainty tokens, assumptions JSON, repo-relative evidence paths,
  content hashes, code-unit ids, and byte ranges. This remains internal and does
  not expose semantic facts through CLI/MCP query commands or make them
  freshness-validated family evidence.
- Internal active-generation claim-input snapshot over files, code units, IR
  nodes/edges, and semantic facts for future claim builders. It uses the same
  read-only active generation and validation rules, remains unavailable through
  CLI/MCP, and does not create family evidence.
- Internal semantic-fact freshness and claim-input readiness gate that checks
  active fact evidence against current source content hashes, blocks stale or
  missing evidence with typed `StaleEvidence` `UNKNOWN`, and keeps structural,
  framework-heuristic, conflicting, or unknown certainty and `UNKNOWN` fact kind
  out of future family claim inputs.
- Conservative EC-MVFI-lite family builder that groups compatible
  framework-role candidates but writes `DOMINANT_PATTERN` family records only
  when each supporting member has strong same-generation `SEMANTIC` or
  `DATAFLOW_DERIVED` non-framework support.
- FamilyStore-backed query read path for `families`, `family`, `member`,
  `find`, `explain`, and `check`, including stable typed `UNKNOWN` output when
  a readable active generation lacks sufficient family evidence.
- Application-level query preflight contract that keeps pattern-family query
  commands in fallback until a readable active generation exists, then lets the
  query layer return family evidence or typed `UNKNOWN`; `files` and `units`
  remain implemented inventory commands whose missing-index fallback is an
  active-index precondition failure.
- Read-only MCP `repogrammar_context` stdio boundary for `initialize`,
  `tools/list`, `tools/call`, and `shutdown`, reusing the same pattern-family
  query preflight and FamilyStore-backed lookup path without enabling installer
  writes.
- Narrow live `install`/`uninstall` execution for explicit Codex and Claude Code
  MCP targets through native agent CLIs, gated by `--yes`, MCP self-test, and
  RepoGrammar-owned receipts while keeping broad target and unsupported scope
  writes deferred.
- v0.1 TS/JS release fixture corpus and product CLI smoke gate that runs
  `init`, `index`, `files`, `units`, pattern-family query commands, and
  `doctor` JSON paths without upgrading syntax-only evidence into family
  claims.
- Storage-aware `status` and `doctor` reporting for active generation id, schema
  version, WAL journal mode, integrity check, and unhealthy active-generation
  pointer cases.
- Regression coverage for semantic-fact/evidence storage, including fact-token
  validation, sanitized text fields, same-generation code-unit path/hash/range
  evidence, building-only writes, malformed evidence rejection before
  activation, and atomic rollback of failed fact writes.
- v0.1 parallel development planning artifacts for repo-local lifecycle,
  adapter/provider abstraction, Python-first analysis, optional
  CodeGraph provider boundaries, typed UNKNOWN governance, family compression,
  query/MCP, installer, and release-smoke phases.
- Historical experimental Python dogfooding plan and ADR, now superseded by
  ADR-0011.
- Python-first v0.1 analysis specification, implementation plan, ADR, and
  durable memory for FastAPI, pytest, SQLAlchemy, and Pydantic family evidence.
- Python selective analysis cascade ADR and documentation, defining CPython
  `ast`/`symtable`/`tomllib` as the primary frontend, Pyrefly as the future
  primary static provider, Pyright as a selective claim-upgrading cross-check,
  typed canonical framework identities, RightTyper-style observed evidence as
  explicit opt-in only, and precision-first evidence compression.
- Optional CodeGraph provider plan and ADR that allow future auxiliary provider
  evidence without making CodeGraph a dependency or product wrapper.
- UNKNOWN governance specification with typed unknown classes, reason codes,
  claim-blocking semantics, and recovery-action guidance.
- Mirrored `AGENTS.md` and `CLAUDE.md` governance contract.
- Documentation system covering architecture, specifications, development
  workflow, ADRs, roadmap, skills, and memories.
- `repo-guard` repository governance binary with guide sync, source-location,
  skill front matter, required-document, and diff-documentation checks.
- CI workflow for formatting, clippy, tests, repository guard checks, and pull
  request diff documentation gating.

### Changed

- Tightened provenance and semantic-worker evidence docs around strict
  `sha256:<64 hex>` content hashes.
- Replaced the bootstrap SHA-256 implementation with the standard `sha2` crate
  while preserving strict `sha256:<64 hex>` content-hash output and vector
  tests.
- Replaced CLI and progress JSON string assembly with `serde_json` builders for
  parseable machine output without changing human output.
- Centralized lexical repo-relative path validation across source reads,
  storage, SQLite validation, semantic-worker boundaries, schemas, and protocol
  tests.
- Centralized native Git context resolution for discovery ignore checks and
  repo-local `.git/info/exclude` hygiene instead of manually parsing `.git`
  files.
- Bound discovery hashing and source-store reads to `max_file_bytes + 1` bytes
  so oversized files are classified without allocating the full file.
- Changed TS/JS discovery to resolve parent Git worktrees before running
  `check-ignore`, so subdirectory project roots honor parent `.gitignore`
  rules while reports stay project-relative.
- Changed bootstrap manifest validation to parse JSON fields instead of relying
  on literal string layout.
- Documented JSON-parsed semantic-worker protocol fixture tests without
  claiming a running TypeScript worker or runtime indexing integration.
- Documented request-side semantic-worker fixture validation without claiming a
  bundled Node or TypeScript compiler worker.
- Documented that the checked-in TypeScript worker is an unavailable fallback
  stub, not compiler-backed TypeScript analysis, and added its Node smoke test
  to CI.
- Aligned semantic-worker schemas with fixture validation by rejecting blank
  string `target` values.
- Documented `repo-guard` required-document coverage for ADR-0008.
- Documented progress `WorkUnits` constructor validation and CLI missing-index
  fallback/deferred implementation status, including structured `--json`
  fallback output for query commands.
- Documented safe repo-local lifecycle behavior, including state directory
  override validation, Git ignore hygiene, bootstrap manifest status, and
  conservative lock/log handling.
- Documented that discovery-to-storage syntax-only code-unit generations and
  Rust-side semantic-worker process validation are implemented while TypeScript
  compiler worker execution, pattern-family query execution, and family
  evidence remain deferred.
- Documented default `semantic_worker: deferred` index/sync behavior plus
  explicit-worker fallback statuses and `semantic_facts` reporting.
- Documented and tested optional semantic-worker argv configuration through
  `REPOGRAMMAR_TYPESCRIPT_WORKER_ARGS_JSON`, keeping worker startup free of
  shell parsing and PATH-dependent shebang assumptions.
- Documented `rusqlite` as the first production dependency, constrained to the
  persistence adapter for repository-local SQLite storage.
- Documented `serde_json` as a production dependency for runtime
  semantic-worker NDJSON validation in adapter code.
- Hardened Rust-side TypeScript semantic-worker process handling around
  canonical project roots, request size limits, inherited-pipe timeout handling,
  unsupported semantic TypeScript versions, sorted/deduplicated changed-file
  requests, field-name redaction, and source/path-like text rejection.
- Aligned the Rust-side TypeScript semantic-worker request guard with the
  checked-in worker stub's 1 MiB stdin envelope, including the terminating
  newline written after the JSON request object.
- Hardened the Rust-side TypeScript semantic-worker process boundary so timeout
  handling does not wait on descendant-held pipes after killing the worker, and
  empty changed-file requests cannot accept worker facts as repository-wide
  scope.
- Hardened semantic-worker protocol validation so worker errors must still close
  with `end_of_stream`, evidence paths are schema-constrained to repo-relative
  forms, fixture validation rejects unsafe evidence paths and source-like text,
  and the worker stub rejects Windows drive-prefix changed-file paths without
  echoing request data.
- Bumped the pre-release storage schema to version 4 for IR node code-unit
  linkage, semantic-fact/evidence constraints, and family-bound evidence
  constraints; stale schema 1, 2, and 3
  generation databases must be rebuilt rather than silently treated as
  compatible.
- Added the FamilyStore storage substrate for generation-scoped family records,
  members, variation slots, and family-bound evidence without enabling
  pattern-family query commands yet.
- Updated roadmap, product, CLI, MCP, indexing, semantic-worker, storage, and
  domain-model docs to align Python-first v0.1 analysis, optional provider, and
  UNKNOWN boundaries with the current transitional TS/JS indexing baseline.
- Hardened generation-scoped storage writes so indexed files, code units, IR
  nodes/edges, and semantic facts can only be recorded while a generation is
  still building, and active generations cannot be downgraded by stale
  validation or activation handles.
- Hardened `index` and `sync` generation updates with
  `.repogrammar/locks/index.lock`, including active-lock refusal, confirmed
  stale-lock replacement during acquisition, lock acquisition before discovery,
  cleanup of partial metadata writes, cleanup of successful runs, and doctor
  lock-state reporting.
- Implemented `unlock --force --yes` for confirmed stale `index.lock` removal
  while preserving active, unknown, invalid, daemon, and SQLite locks.
- Expanded `repo-guard` required-document coverage to include v0.1 planning
  artifacts, the Python analysis specification and ADR-0011, the substrate
  hardening checkpoint, ADR-0009/ADR-0010, typed UNKNOWN governance, and the
  matching durable memory mirrors.
- Hardened `doctor` lifecycle diagnostics so missing or invalid generated state
  `.gitignore`, Git exclude patterns, init receipts, and root `.gitignore`
  markers are reported without mutating repository state.
- Split `status` schema reporting into explicit `manifest_schema_version` and
  `storage_schema_version` human/JSON fields, removing the ambiguous status
  JSON `schema_version` field.
- Split `doctor` JSON schema reporting into explicit
  `checks.manifest_schema_version` and `checks.storage_schema_version`, removing
  the ambiguous `checks.schema_version` field.
- Tightened EC-MVFI-lite support compatibility so arbitrary semantic facts do
  not prove Express, React, Jest, or Vitest framework-role families.
- Tightened pattern-family query matching so `family` and `member` use exact
  ids, while fuzzy matching remains limited to `find`, `explain`, and `check`
  and rejects short-substring false positives.
- Changed advisory `check` and MCP `check_conformance` success contexts to
  report top-level `CONTEXT_ONLY` with nested advisory `UNKNOWN` instead of
  implying conformance is proven.
- Added public family-evidence freshness checks so stale source hashes block or
  omit CLI/MCP family detail with typed `StaleEvidence` `UNKNOWN`.
- Hardened MCP install self-tests with a bounded timeout that kills and reaps
  hanging self-test processes before native agent configuration.

### Fixed

- `repogrammar index`/`sync` `semantic_facts` totals (surfaced by `index --json`
  and `status`) now include TS/JS-derived support facts. The reported count had
  omitted `repogrammar-tsjs-derived` facts even though they were recorded and
  fed family construction, undercounting the total for Express/Jest/Vitest
  repositories; the reported total now equals the facts actually stored in the
  active generation.
