# CLI Specification

RepoGrammar's CLI is designed around implementation-pattern families, not
generic symbol graph navigation.

## v0.1 command surface

Project lifecycle:

- `setup`
- `init`
- `uninit`
- `index`
- `sync`
- `resync`
- `autosync`
- `prune`
- `compact`
- `storage`
- `status`
- `doctor`
- `unlock`
- `logs`

Pattern-family queries:

- `find`
- `families`
- `family`
- `member`
- `explain`
- `check`
- `files`
- `units`

Agent integration:

- `serve`
- `install`
- `uninstall`
- `instructions`

Metrics:

- `unknowns`
- `stats`
- `telemetry`

Maintenance:

- `version`
- `help`

## Help contract

`repogrammar --help`, `repogrammar -h`, and `repogrammar help` must print a
compact top-level journey of no more than 25 lines centered on `setup`, `find`,
and `doctor`, with `help --all` as the explicit complete command inventory.
`repogrammar help <command>`,
`repogrammar <command> --help`, and `repogrammar <command> -h` must print
command-specific usage, supported subcommands where applicable, accepted
options, and safety notes.

Help output is read-only discovery. It must not initialize a repository, index
source, start or stop auto-sync, configure agents, change telemetry consent, or
write receipts. Unknown help topics must fail cleanly with exit status `2`. This
applies to `serve` as well: `repogrammar serve --help`/`-h` must print serve
usage and exit `0` rather than starting the MCP loop or rejecting the flag,
even though `serve` is dispatched before the shared help handler.
For `autosync`, help must make the positional subcommand contract explicit:
`repogrammar autosync start` starts auto-sync; `repogrammar autosync --start`
is not a supported flag.

## Pattern-family commands

`repogrammar find` is the main human-facing equivalent of the MCP
`repogrammar_context` operation `find_analogues`. It must return candidate
families, target compatibility, dominant patterns, variation points, exceptions,
unknowns, and a minimal contrastive evidence set. It must not return only top-k
similar files.
The user should be able to start from the repo-relative path, symbol/member id,
framework role, or pattern question they already have. The caller does not need
to know a family id before running `find`; family ids returned by `find` are
follow-up handles for exact inspection.

`repogrammar family` is the CLI equivalent of the `show_family` operation.
It is exact-family-id only and is intended for family ids returned by earlier
`find`, `explain`, `check`, or `member` queries.

`repogrammar explain` is the CLI equivalent of the `explain_deviation`
operation.

`repogrammar check` is the CLI equivalent of the `check_conformance` operation.
It is a source-backed *static-alignment* check, not a runtime-conformance
verdict. It resolves the target to a specific indexed code unit, selects a
comparison pattern family (the unit's own family when it is a member, otherwise
the single fresh ready family of the unit's `(language, kind, role)` key), and
compares the target's indexed feature profile against that family's constraint
profile. The top-level `status` is one of the static-alignment tokens
`STATICALLY_ALIGNED`, `STATIC_DEVIATION`, `PARTIAL_ALIGNMENT`,
`INSUFFICIENT_EVIDENCE`, or `UNKNOWN` — never `PASS`/`FAIL`/`CONFORMS` and never
the legacy `CONTEXT_ONLY` advisory. Every certificate carries
`runtime_equivalence: "UNKNOWN"`; static alignment never proves runtime
equivalence. A stale, ambiguous, unindexed, or family-less target abstains with
`INSUFFICIENT_EVIDENCE` and never surfaces a selected family.

All query commands must support:

- `--project <path>`
- `--token-budget <n>` where `n` is positive and no greater than 200000
- `--mode compact|evidence|deep`
- `--verbosity minimal|standard|full`
- `--json`
- `--include-variations`
- `--include-exceptions`
- `--include-source-spans`

Successful `find`, `family`, `member`, `explain`, and `check` outputs include
metadata-only `estimated_potential_token_savings` diagnostics with
`measurement_kind: ESTIMATED`. This field estimates potential read displacement
from selected family evidence, read-plan metadata, and optional source-span
token estimates. It must not be described as actual token savings and it must
carry a caveat saying it is not measured token savings.

## Long-running commands

All long-running commands must support:

- `--progress auto|always|never`
- `--json`
- `--quiet`
- `--verbose`

Long-running commands include setup, repository initialization, indexing, sync,
resync, `autosync run`, and MCP serving.

For `init`, `index`, `sync`, and `resync`, human progress is emitted to stderr
when `--progress always` is set, or when `--progress auto` detects an
interactive stderr. Known work renders an ASCII progress bar, exact integer
percentage, and completed/total counts; unknown work remains indeterminate and
does not display a percentage. `--quiet` and `--progress never` suppress
progress. Interactive TTY progress rewrites a single terminal line and finishes
with one newline; noninteractive plain-log progress remains append-only with one
line per event. Final human or JSON results remain on stdout. `--json` affects
only the final stdout result; progress on stderr remains human progress-bar
output so terminal users are not flooded with machine progress events.

## Repository state commands

`repogrammar setup [--project <path>] [--target
auto|codex|claude-code] [--yes] [--dry-run] [--no-autosync] [--json]
[--progress auto|always|never]` is the primary user-facing onboarding
orchestrator. It composes the existing machine-level installation and
repository lifecycle boundaries; it does not replace either boundary or invoke
the product CLI recursively. The default project is the current directory, the
default target is `auto`, and successful indexing starts auto-sync unless
`--no-autosync` is present.

Live setup performs one final interactive confirmation. Noninteractive live
setup requires `--yes`; `--dry-run` is zero-write and may be combined with
`--yes` by wrappers that forward a common argument list. Missing supported
agent CLIs must not block repository-only initialization, indexing, or the
product-binary MCP self-test. Setup never changes the anonymous telemetry
preference: telemetry is off by default, but a preference that was explicitly
enabled before setup remains enabled. Setup JSON therefore reports
`telemetry_changed: false` and `telemetry_enabled_by_setup: false`; it does not
claim the current global preference is false.

Setup planning and execution must use the application-layer setup orchestrator.
The plan distinguishes machine-owned agent integration, repository
initialization, repository indexing, background auto-sync, and read-only
self-test stages. Human plans and results must label repository initialization
and repository indexing separately; they must not collapse both into one
ambiguous repository-index label. Agent inspection distinguishes unmanaged,
`OwnedCurrent`, `OwnedOutdated`, foreign, and malformed state. Current owned
state is skipped, obsolete-but-internally-consistent owned state is safely
refreshed through the install service, unmanaged state may be configured, and
foreign, malformed, or receipt/native-drifted state is preserved rather than
overwritten. Rollback may remove only machine-level writes and receipts newly
created by the current setup run. A refreshed pre-existing owned integration is
tracked separately from a newly configured target and must survive every later
repository, auto-sync, or MCP failure. Setup also preserves pre-existing
repository state and any active generation that was successfully built before a
later auto-sync or MCP failure. Human failures expose one typed, sanitized next
action without absolute paths or raw internal errors; JSON emits structured
stage, failure-class, preservation, rollback, and recovery fields.
Failure-mode human output has exactly one sanitized line for completed stages,
retained resources, rollback status (including rollback failure), the primary
failed stage/class, and the single next action. It never renders raw errors or
paths.

Setup readiness is factored, not inferred from one aggregate `ready` string.
JSON must expose these stable low-cardinality fields on every setup result:

- `ready_agent_targets`: detected native targets whose current integration is
  verified ready after execution;
- `blocked_agent_targets`: inspected supported targets not in the ready set;
- `product_self_test_state`: `passed`, `failed`, `planned`, or `not_run`;
- `agent_query_ready`: true only when the product self-test passed and at least
  one native agent target is ready;
- `repository_index_ready`: whether a fresh active index existed or indexing
  completed;
- `autosync_ready`: whether requested auto-sync was already ready or completed
  its readiness handshake;
- `family_evidence_state`: `available`, `available_zero`, `unknown`, or
  `not_applicable`;
- `limitations`: the complete list of low-cardinality limitations, not only the
  first limitation.

The `index` object keeps `indexed_files`, nullable `pattern_groups`, and its own
`family_evidence_state`: `available_zero` means a successful inventory query
returned zero, `available` means it returned a positive count, and `unknown`
means the inventory could not be obtained. An unavailable inventory must not be
serialized or rendered as zero supported pattern groups.

Human output likewise renders every limitation. It may print “Ask your coding
agent” and JSON may set `suggested_question` only when `agent_query_ready` is
true. Repository-only success must instead say that the RepoGrammar CLI/index
is available while coding-agent MCP wiring is inactive, with
`suggested_question: null`.

An active index is skipped only when status inspection reports it as fresh.
Stale or unverifiable active state is refreshed through the existing resync
boundary before setup can report completion. A requested auto-sync start is
complete only after a bounded startup handshake proves that the spawned child
owns the expected daemon lock via its PID and startup nonce and that the child
is still alive. Immediate child exit, lock refusal, and timeout are distinct
typed start failures; process creation or a PID alone cannot be synthesized into
running or ready state.
If indexing finds zero supported pattern groups, setup reports
`ready_with_limitations`, states that no supported pattern groups were
verified, and recommends the conservative source fallback instead of printing
a strong ready claim. On a fresh active-index rerun, setup reads the actual
family inventory before skipping indexing; index query-readiness alone is not
family evidence. A fresh generation with zero family rows therefore remains
`ready_with_limitations` with the conservative source fallback.

A successful native-agent probe whose configuration cannot be recognized is a
preserved malformed integration, not a fatal repository inspection error.
Setup blocks agent writes, continues repository-only setup, and recommends
`repogrammar doctor`. An actual native probe failure remains fatal because the
configuration state could not be inspected safely.

The authoritative recovery formatter maps initial setup, status, doctor, query,
and MCP missing-repository guidance to `repogrammar setup`. It preserves the
more specific `repogrammar resync` and `repogrammar autosync start` actions for
stale/missing index and auto-sync recovery respectively.

`repogrammar init` creates repository-local state under `.repogrammar/` by
default, or under `REPOGRAMMAR_DIR` when that environment variable is set. It
must not modify tracked repository files by default. It must write
`.repogrammar/` and `.repogrammar-*/` to `.git/info/exclude` when Git is
available, and it must create `.repogrammar/.gitignore` as a second defense.
`REPOGRAMMAR_DIR` is a repo-local directory-name override only; empty values,
absolute paths, traversal, nested paths, symlink state directories, file
conflicts, and names outside `.repogrammar` or `.repogrammar-*` must be
rejected.
After successful initialization, human and JSON `init` output must report the
current repository storage and indexing status from the same status inspection
contract used by `repogrammar status`. Re-running `init` in a repository with a
readable active generation must therefore report `storage: available` and the
active indexing mode such as `syntax_only_code_units`, rather than replaying
bootstrap manifest placeholder values.
`repogrammar init --yes` is accepted as an agent-safe noninteractive
confirmation flag. It does not broaden `init` writes and does not make root
`.gitignore` writes unless `--write-gitignore` is also present, but it still
builds or refreshes the active index by default.

`repogrammar init` is the default one-command repository bootstrap for users or
agents that have permission to create repo-local analysis state. It must run the
normal init path, then run the same static-analysis path as
`repogrammar resync`. `--resync` remains accepted as an explicit spelling of
that default. After the resync path produces a readable active generation,
`init` starts repo-local auto-sync by default. `--no-autosync` explicitly keeps
the successful active index without starting a background daemon; `--autosync`
remains accepted as an explicit compatibility spelling of the default. The two
auto-sync preference flags are mutually exclusive and must fail cleanly before
creating state when combined. `--state-only` preserves lifecycle-only repair
behavior: it creates or repairs repo-local state, must not run indexing, and
must not start auto-sync. `init --state-only --resync` and `init --state-only
--autosync` must fail cleanly before creating state; `init --state-only
--no-autosync` is accepted as a redundant safe opt-out.

JSON output for bootstrap must preserve the existing top-level init fields and
include `resync` and `autosync` sub-results where applicable. If indexing fails
after state initialization, the error must preserve repo-local state, preserve
any previously valid active generation, and guide the user to fix the indexing
issue and run `repogrammar resync`. If auto-sync start fails after a successful
resync, the JSON error must preserve the valid `resync` sub-result and must not
roll back the active generation. Because auto-sync is the default, this partial
failure contract applies to an unflagged `init` as well as explicit
`init --autosync`.

`repogrammar init --write-gitignore` may update the root `.gitignore` with a
small marker-fenced section. Without this flag or explicit interactive
confirmation, root `.gitignore` must remain untouched.

`repogrammar uninit` removes repository-local RepoGrammar state. It is the only
command that may remove `.repogrammar/`; `repogrammar uninstall` must not remove
project indexes. `uninit` must make logs deletion explicit.

`repogrammar prune` removes old inactive index generations from
`.repogrammar/repogrammar.sqlite` by deleting inactive `index_generations` rows
and their cascading generation-scoped records while preserving the single active
generation row. The default retention policy is active plus the newest 2
inactive generations. `--keep <n>` overrides the inactive generation count and
may be `0`. Destructive prune runs require `--yes`; `--dry-run` reports
candidates without deleting. Human and JSON output must report the active
generation, retained inactive generation IDs, candidate generation IDs, deleted
generation IDs, `dry_run`, and `keep_inactive` without exposing absolute paths.
Prune must refuse unhealthy storage and concurrent active-generation changes.
When only a legacy `.repogrammar/current-generation` pointer and
`.repogrammar/generations/` directory exist, prune may use the legacy directory
fallback; that fallback must refuse missing or corrupt active-generation
pointers, symlinked generation directories, and generation entries that are not
directories.
After a destructive mutable-database prune commits, the storage adapter runs
bounded SQLite maintenance with `PRAGMA optimize` and a passive WAL checkpoint.
This maintenance must not run blocking `VACUUM` and must not remove active
mutable records.

`repogrammar compact` explicitly compacts the repo-owned mutable SQLite index
database. `compact --dry-run --json` must acquire the repository-local index
lock, validate the active generation and storage sidecars, perform no writes,
and report database, WAL, SHM, total before/after bytes, `dry_run`, active
generation, status, and reclaimed bytes without exposing absolute paths.
Mutating `compact --yes` must require the same index lock and storage-health
preflight as `prune`, refuse unsafe database states such as dirty active
records, refuse missing mutable storage, and report the same before/after size
metadata after running explicit SQLite `VACUUM` and a truncating WAL checkpoint.
It must not delete source files, user files, or legacy generation directories.

`repogrammar storage clean` is the one-command repository-local maintenance
path for human users and agents. The only implemented subcommand is `clean`.
It must run the same initialized-state, lifecycle-subdirectory, storage-health,
and repository-local index-lock preflight as `prune` and `compact`. A mutating
run requires `--yes`; `--dry-run` reports candidates and size metadata without
removing files, pruning rows, or compacting. The clean sequence is:

1. verify that mutable SQLite storage is present and authoritative;
2. remove diagnostic legacy layout files
   `.repogrammar/current-generation` and `.repogrammar/generations/` only when
   mutable SQLite is present;
3. prune inactive mutable generations with `keep_inactive = 0`;
4. run explicit mutable SQLite compaction;
5. report legacy-layout bytes, prune candidates/deletions, compact
   before/after bytes, total before/after bytes, and reclaimed bytes without
   exposing absolute paths.

`storage clean` must refuse legacy-only repositories instead of deleting their
only index; users should run `repogrammar resync` first to create mutable
SQLite storage. `storage clean --dry-run --json` must preserve the same
machine-readable fields as the human report and mark `status: dry_run`.

`repogrammar status` must support human and `--json` output. It must report
whether the repository is initialized, manifest status, the active generation,
manifest schema version, storage schema version, journal mode,
storage layout, mutable-database presence, legacy generation-layout presence,
mutable WAL/SHM sidecar byte counts, active derived dependency count, active
dirty-record count, storage/indexing implementation status, missing
subdirectories, and relevant warning states. Status JSON must use
`manifest_schema_version` and `storage_schema_version` for the index and storage
schema versions, and must never collapse them into the top-level `schema_version`
field. The top-level `schema_version` field is reserved for the shared product
payload schema token (`product-schemas.v1`); see the product schema versioning
paragraph below. When storage is wired, it must also
report SQLite integrity status and unhealthy storage states without exposing
absolute paths. When mutable and legacy layouts coexist, status must report the
mixed layout while retaining the mutable database as the active read source.
Manifest status must be based on parsed JSON fields, not literal text layout,
so valid reordered manifests are accepted and malformed required fields are
reported as corrupted.
Status and doctor must compare the inspected storage schema with the running
binary's schema authority before reporting storage available or queries ready.
An older stored schema is unhealthy with `repogrammar resync` guidance from the
current binary. A stored schema newer than the binary is also unhealthy, but its
guidance is to upgrade RepoGrammar and explicitly not to resync with the older
binary; an old executable must never destroy a newer derived index while
pretending to repair it.

Status JSON must include a source-free `readiness` object so humans and agents
can decide whether RepoGrammar can answer repository-local queries right now.
Stable readiness states are `not_initialized`, `state_only_no_active_index`,
`ready_active_index`, `active_index_unhealthy`,
`active_index_stale_or_unreadable`, `autosync_recommended`,
`autosync_active`, `storage_unhealthy`, and `unknown`. The object reports
`query_ready`, `active_generation_available`, `recommended_next_command`,
`requires_user_permission`, an `autosync` object (`configured`, `running`,
`recommended`), and `local_state_hygiene`. `local_state_hygiene` reports
whether `.repogrammar/` is present, ignored by Git policy when safely
detectable, at risk of being tracked when safely detectable, and any
source-free recommendation. It also reports foreign provider local state such
as `.codegraph/` only when present or tracked-risk is detected, with
`managed_by_repogrammar: false`. Readiness output must not expose source text,
absolute paths, repo names, file lists, raw Git output, or raw errors.

Status and doctor JSON must also include a source-free `product_readiness`
object: the shared decomposed capability model that replaces reliance on the
single optimistic `query_ready` boolean. It reports a deterministic
low-cardinality `summary` token (`ready`, `degraded`, or `not_ready`) plus
independently truthful dimensions — `repository_state`, `active_index` (with
`available`, `active_generation`, `manifest_schema_version`, `schema_current`),
`family_evidence` (`state` and `fresh_count`/`stale_count`/`cannot_verify_count`
from the bounded freshness machinery, plus `evidence_unreadable`),
`family_prevalence` (counts by classification, or `null` when the family store is
unreadable), `query_retrieval` (exact and term-retrieval modes and the
`vocabulary_version`), `static_alignment` (`available`/`unavailable`/
`not_applicable`, or `cannot_verify` when the family store is unreadable),
`providers` (per-slot integration and availability), `autosync`, and
`measurement` (the NOT_MEASURED token-saving discipline). It also carries
`top_blocking_unknowns` (the bounded top-five required-mechanism buckets, `null`
when that inventory is unreadable versus `[]` for genuinely none) and one
`recovery` object from the shared recovery classifier (`action`, `reason`,
`recommended_command` — null unless the action is an executable RepoGrammar
command — `guidance`, and `executable`). A store-read error must yield no definite
dimension token: prevalence and the top-unknown list report unreadable (`null`)
and static alignment `cannot_verify`, never a false zero/not-applicable.

The summary is a pure projection of the one combined recovery decision, derived
from the same authoritative repository recovery the query preflight consumes (so
it already folds in the repository dirty-record freshness signal) layered with the
hash-checked family-evidence freshness; it is never more optimistic than the query
path. An unservable index is `not_ready`; a servable index that is stale (family
evidence stale/unverifiable, or the repository index carries dirty derived
records) or whose autosync is recommended-but-stopped is `degraded` with the stale
count visible; only a fully clean, fresh servable index is `ready`. A
stale-families-while-`query_ready` checkout therefore reports `degraded`, never a
bare `ready`, and `summary: ready` guarantees `query_ready` is true in the same
payload. Assembling the block performs bounded stats-scale reads (a family-evidence
freshness scan plus an unknown-inventory read), so `status`/`doctor`/
`inspect_readiness` are readiness-triage commands, not routine per-query loops.
Because the classic `readiness` object and `product_readiness` are computed from
independent repository-status snapshots within one invocation, they can describe
slightly different points in time under concurrent modification. The block is
present only when the runtime can assemble it (it is absent/null for deferred
runtimes).

`repogrammar doctor` must support human and `--json` output. It must check
manifest status, required lifecycle subdirectories, storage/indexing
implementation status, lock state, Git hygiene, and state directory
configuration. Once SQLite exists, it must also check database integrity,
schema version, journal mode, active generation consistency, storage layout,
and mutable WAL/SHM sidecar state.
Doctor must validate generated lifecycle hygiene without mutating state:
`.repogrammar/.gitignore`, `receipts/init.json`, `.git/info/exclude`, and root
`.gitignore` RepoGrammar marker sections must be reported as missing or invalid
rather than silently repaired. JSON output must expose this as
`checks.lifecycle_hygiene`. JSON output must expose index-lock diagnostics as
`checks.locks` with `pass`, `warning`, `fail`, or `not_applicable`.
Doctor JSON must use `checks.manifest_schema_version` and
`checks.storage_schema_version`; it must not expose an ambiguous
`checks.schema_version` field. The product payload schema token stays at the
top-level `schema_version`, never inside `checks`. When storage can be inspected,
doctor JSON also
reports `checks.dependency_records` and `checks.dirty_records` so stale/dirty
storage diagnostics are machine-readable. It must also report
`checks.storage_layout`, `checks.mutable_database_present`,
`checks.legacy_generation_layout_present`, `checks.wal_bytes`, and
`checks.shm_bytes`. Legacy-only storage and mixed mutable-plus-legacy storage
must produce explicit doctor findings without treating the legacy files as
authoritative when a mutable database is present.
Doctor JSON must include the same `readiness` and `product_readiness` objects as
status JSON. Doctor may recommend commands such as `repogrammar init`,
`repogrammar resync`, `repogrammar doctor`, or `repogrammar autosync start`, but
it must not perform those actions implicitly. Doctor and status human output must
lead with the actionable capability summary and the one canonical next action from
the recovery classifier (a `capability:` line and a `next_action:` line). Family
evidence counts (`stale_family_evidence`, `unverifiable_family_evidence`) are
rendered as facts only: the single command shown is the classifier's `next_action`,
and callers must not infer a second command from raw freshness counts. Internal
mechanism ids stay in the JSON as follow-up handles.

Every primary structured CLI payload — the pattern-family query commands (`find`,
`family`, `member`, `explain`, `check`, which share the same stamped serializers),
plus `families`, `status`, `doctor`, and `stats` JSON, including their fallback and
lifecycle-error payloads — carries a top-level `schema_version` product payload
token (`product-schemas.v1`), shared with the MCP result objects
(`docs/specifications/mcp-api.md`). This is the wire contract version and is
distinct from the index/storage `manifest_schema_version` and
`storage_schema_version`. The pre-1.0 compatibility policy is additive: fields may
be added within a version; removing or renaming a field, or changing its meaning,
requires a version bump and a CHANGELOG entry. Consumers must ignore unknown
fields.
During the current syntax-only phase, `doctor` is wired to SQLite storage health
for the active generation. It must still distinguish file-manifest-only,
syntax-only code-unit, and future family-evidence indexing.

`repogrammar index`, `repogrammar sync`, and `repogrammar resync` require an
initialized repository-local state directory. `index` and `resync` run a full
rebuild: they perform TS/JS, bounded TS/JS project-config, Python `.py`, and
Rust self-dogfood discovery, read source through a repo-relative hash-checked
boundary, store repo-relative file metadata and syntax/code-unit records plus
owned semantic facts in a new building generation inside
`.repogrammar/repogrammar.sqlite`, validate the generation, and atomically mark
that generation active while downgrading any previously active row to
validated. Human and JSON output must report the authoritative generation mode,
actual `parser_attempted_files`, `indexed_units`, and `semantic_facts` counts,
`semantic_worker`, and `mining: deferred`. A generation containing only `go`,
`go-config`, `php`, `php-config`, `ruby`, `ruby-config`, `swift`, and/or
`swift-config` inventory tokens,
or no accepted source/configuration tokens, reports
`indexing: file_manifest_only` and
`parser: deferred`. A generation containing any parser-capable language token
reports `indexing: syntax_only_code_units` and `parser: syntax_only`, including
unchanged mixed-repository incremental rounds with zero parser attempts. The
CLI emits at most one truthful unsupported/inventory-only warning per accepted
manifest token, not one warning per file. By default, `semantic_worker` is
`deferred`.
During the current TS/JS and Python framework-role slices, `semantic_facts` may
be greater than zero even when `semantic_worker` is `deferred`; those records
are syntax-origin `FRAMEWORK_ROLE` facts with `FRAMEWORK_HEURISTIC` certainty,
Python parser-origin structural/approved graph-derived/`UNKNOWN` facts, root
`pyproject.toml`/`setup.cfg`/`setup.py` `PROJECT_CONFIG` or config-`UNKNOWN`
records, or TS/JS project-config `PROJECT_CONFIG`/config-`UNKNOWN` records. The
only Python
parser-origin `DATAFLOW_DERIVED` facts accepted here are repo-local import graph
or pytest fixture graph facts with explicit `provider_resolved=false` provenance;
Python and conservative TS/JS exact-anchor derivation may also add separate
`DATAFLOW_DERIVED` support facts without running a semantic worker. These are
bounded RepoGrammar support/context facts, not compiler/provider-backed facts.
`sync` attempts a path-level incremental rebuild when the active generation is
readable, mutable, schema-compatible, and dirty-free, no explicit semantic
worker is configured, and the delta passes the project-context gate. Safe
content-only edits of Rust and TS/JS sources and interface-stable Python modules
may reparse only the changed paths; source-set changes and project-config,
`conftest.py`, or unverifiable/interface-changing Python edits fall back as
specified by the indexing pipeline. After those preconditions pass, a zero
delta retains the existing validated active generation and reports zero
copied-forward files, reparses, and family recomputations; it does not create a
redundant generation. For a non-empty safe delta, sync copies unchanged active
file/code-unit/IR/semantic records into a new building generation without
reparsing those paths, reparses added and modified paths, omits removed paths,
recomputes local derived support and families over the new generation, validates
dirty/dependency state, and then activates the new generation. If a safe
precondition is not met, `sync` must
fall back to the full rebuild path and report `sync_mode:
full_rebuild_fallback` with a `fallback_reason`.
Inventory-only `go`, `go-config`, `php`, `php-config`, `ruby`, `ruby-config`,
`swift`, and `swift-config` deltas are an explicit token-based exception while
those tokens are absent from
`ParserProjectContext`: only bounded file metadata is added, modified, removed,
or copied, claim-bearing legacy records for Go, PHP, Ruby, and Swift paths are purged,
and parser-attempt/reparse counts remain zero. Ruby discovery records the stable
`language_specific_exclusion` skip token for `.bundle` and `.ruby-lsp` path
components; PHP uses the same token for exact `.composer` and `.phpunit.cache`
components. Neither language-specific policy globally hides those directories
from other language discovery, and exact `vendor` remains globally excluded.
Swift uses the same stable skip token for exact `.build` and `.swiftpm`
components without globally hiding other-language files below them.
`resync` is the explicit full-rebuild command: it is available for any
initialized repository, rebuilds a new active generation through the full
static-analysis path, and uses the invoked command name in CLI output. Public
fallback and MCP guidance should prefer `resync` for missing, stale, or
intentionally refreshed analysis because it names the user intent to rebuild
static-analysis facts.
Rust self-dogfood indexing may likewise add Tree-sitter-origin structural
anchors, Cargo manifest inventory, Rust typed `UNKNOWN`s, and bounded internal
`DATAFLOW_DERIVED` support facts for RepoGrammar-owned implementation roles.
Those facts are not Cargo/rustc-backed semantics and do not imply general Rust
target-language support.
During a non-quiet run, `init` emits repository-state initialization progress.
`index`, `sync`, and `resync` emit progress for project discovery, file
scanning, syntax parsing, local support-fact recording, semantic-worker
deferred/running state, candidate/family construction, and persistence
validation. Known work uses exact completed/total counts and exact integer
percentages. Inventory-only progress must say that work was deferred or
inventoried; it must not label Go, PHP, Ruby, or Swift metadata traversal as parsed
source.
Unknown work must remain explicit and must not display fabricated
percentages or ETAs.
Progress events must not include source snippets, source paths, content hashes,
symbols, raw targets, or repository-identifying absolute paths.
The product runtime also runs the default safe Rust Cargo metadata project-model
substage during full `index`, full `resync`, and full-rebuild `sync` fallback
when same-generation `Cargo.toml` code units exist. Incremental `sync` reuses
unchanged provider facts only when their evidence path/hash remains unchanged;
`Cargo.toml` or `Cargo.lock` changes force full-rebuild fallback. Provider
metadata may add provider-backed `PROJECT_CONFIG` facts and typed provider
`UNKNOWN`s, but package/crate/target/feature/dependency metadata cannot prove
symbol/type/call semantics or family membership.
When `REPOGRAMMAR_TYPESCRIPT_WORKER` is set to an explicit worker executable,
`index`, full `resync`, and full-rebuild `sync` fallback may run that worker
after syntax-only code units are stored for the building generation.
`REPOGRAMMAR_TYPESCRIPT_WORKER_ARGS_JSON` may supply an optional JSON array of
non-blank string arguments. This is an argv contract, not shell parsing; worker
arguments without `REPOGRAMMAR_TYPESCRIPT_WORKER` are invalid. Worker facts must
pass the same-generation storage gate before they are recorded. Worker
unavailable, unsupported-version, timeout, crash, or protocol-violation
failures must fall back to syntax-only indexing with a typed
`semantic_worker: fallback_*` status and sanitized warnings. A worker fact that
conflicts with the indexed code-unit path, content hash, or range must abort the
new generation rather than silently dropping or accepting stale evidence. If
storage health is already unhealthy, index, sync, and resync must refuse and
direct the user to `repogrammar doctor` rather than masking the corruption with
a new generation. Before discovery, source reads, generation preparation,
validation, and activation, all three commands acquire
`.repogrammar/locks/index.lock` and hold it through validation and activation.
`REPOGRAMMAR_STRICT_GITIGNORE=true` makes unavailable Git ignore checks a hard
index/sync/resync error; otherwise discovery keeps the warning fallback and
continues.
Discovery aggregate-resource failures are deterministic invalid-input errors
(exit 2) with the stable resource token, inclusive limit, first exceeded value,
and narrowing/exclusion guidance. They never return partial index success or
change the successful index/sync/resync JSON schema. Because discovery runs
before generation preparation, an over-limit repository cannot activate a new
generation. During `init`, the same failure remains an initialization
`failed_step: "resync"`; state initialization may have succeeded, but the
`resync` sub-result is null and autosync is not started.
Autosync polling evaluates Git ignore with the same accepted-manifest policy as
manual discovery, batching every supported candidate through one
`git check-ignore -z --stdin` subprocess per fingerprint pass (about 10 ms for a
few hundred paths, roughly one percent of the default 1000 ms poll) rather than
one process per file. Supported Git-ignored candidates are excluded before the
aggregate fingerprint file/byte ceilings are charged, so `autosync run` and a
manual Git-aware `sync` no longer disagree about whether a repository is within
accepted limits. When Git is absent or errors, the pass falls back to safe
no-ignore filtering exactly as discovery does. The fingerprint stays
metadata-only, so a same-size, same-modification-time edit is invisible to
polling until another change or a manual `sync`; `sync` remains the
authoritative Git-aware indexing operation and always recomputes content hashes.
Each pass counts the Git-ignored supported files it excluded and records that
bounded, path-free count with the Git-ignore status to the daemon log on change;
surfacing it through `autosync status --json` is a deferred follow-up.
The lock records process id, host when available, OS, start time, and
RepoGrammar version. Active or unknown lock ownership is refused with guidance
to run `repogrammar doctor`; confirmed stale same-host locks may be replaced
during acquisition. Same-host liveness checks must use native process probes on
Windows as well as Unix so a dead nonzero PID does not remain permanently
unknown. Successful runs remove only the lock content they wrote.

`repogrammar autosync` manages optional repository-local automatic sync. It
supports subcommands:

- `status`
- `enable`
- `start`
- `stop`
- `disable`
- `run`

With no subcommand, `autosync` is equivalent to `autosync status`. `start`
enables auto-sync for the current repository if needed and launches a
background `repogrammar autosync run` worker. Normal `repogrammar init` already
invokes this start path after a successful resync; the standalone command is
the recovery path after a stop, reboot, or daemon failure, and remains
available for repositories initialized with `--no-autosync`. Before enabling
or launching a new background worker, `start` validates the inherited semantic-worker
environment and reports invalid worker argv configuration synchronously rather
than claiming the daemon started. The child first publishes a `starting`
daemon-lock phase, then validates repository/config state, validates its own
semantic-worker environment, computes the initial repository fingerprint,
initializes daemon log/startup state, and completes one immediate service
heartbeat. That heartbeat revalidates the exact `starting` lock owner,
repository readiness, and a second repository fingerprint; the second
fingerprint becomes the polling baseline. Only after those fallible steps
succeed may the same lock owner atomically transition its exact
PID-plus-startup-nonce record to `ready` and enter the polling loop. The
transition quarantines the owned `starting` record and installs `ready` with a
no-overwrite link, so a non-cooperating replacement is preserved and readiness
fails closed rather than overwriting another owner. The parent reports
`running: true` only while its child
handle remains alive and the exact expected PID, nonce, current lock owner, and
`ready` phase all match. A `starting` record is never running. Failed startup
persists only one sanitized low-cardinality code:
`worker_environment_invalid`, `repository_fingerprint_failed`,
`repository_state_unavailable`, `daemon_lock_refused`,
`child_exited_before_ready`, `startup_timeout`, or
`first_heartbeat_failed`; raw worker errors, paths, source, environment values,
credentials, nonces, and daemon internals are excluded. The worker polls a lightweight
supported-file metadata fingerprint, debounces changes, and calls the existing
`sync` implementation when indexed files are added, removed, or modified. The
daemon records a sanitized `repository fingerprint failed` previous-attempt
error and remains alive when a later polling fingerprint transiently fails;
such a post-ready runtime failure is not retroactively presented as an
unverified startup success.
lightweight detector must skip RepoGrammar state directories, default excluded
directories, unsupported extensions, oversized files, symlinks, Git-ignored
supported files (with the same fallback discovery uses when Git is unavailable),
and paths outside the repository; the following `sync` remains the authoritative
content-hash, Git-ignore, parsing, semantic-fact, and generation-activation
step, whether it completes incrementally or reports a full-rebuild fallback. It
must not scan repositories that have not explicitly run `init`, and it must not
be started by `install`, `serve`, MCP queries, or agent wiring. `run` is the
foreground worker entrypoint used by `start`; it writes diagnostics to
`.repogrammar/logs/daemon.log` when started in the background. After every sync
the daemon always records a one-line summary to that log. Incremental-aware
summaries include sync mode, added/modified/removed counts, unchanged and
copied-forward counts, reparsed files, units indexed, elapsed milliseconds, and
the activated generation (`autosync: incremental sync +A ~M -R unchanged U
copied C reparsed P file(s), N unit(s) in Xms (generation G)`). Older or
non-sync outcomes may use the legacy summary (`autosync: synced N file(s), U
unit(s) in Xms (generation G)`). Failures record the first failure with elapsed
time, then summarize consecutive identical failures by repeat count instead of
writing one line per failed loop. A different error records a transition
summary for the previous repeated error; a later successful sync records one
recovery summary before the normal success line. If the repository-local state
precondition becomes unavailable, for example required lifecycle subdirectories
are missing, the daemon records a stop reason and exits rather than retrying
forever. These daemon log writes are independent of `--quiet`, which only
suppresses the interactive "watching"/"change detected" chatter. `stop`
terminates the recorded daemon process and removes
`.repogrammar/locks/daemon.lock`; terminating an already-exited daemon is not an
error, and the lock is always removed so the next `start` is never blocked by a
stale lock. `disable` requires the daemon to be stopped
first and removes
`.repogrammar/autosync.json`.

Daemon liveness must not be inferred from the recorded PID alone. After an
unclean daemon exit the operating system can reuse that PID for an unrelated
process; treating it as live would let `stop` signal a stranger and permanently
block `start`. A lock's PID is therefore reported as `running` only when the PID
both exists and is confirmed to be a RepoGrammar `autosync run` daemon.
When the platform can prove process existence but cannot inspect its command,
daemon liveness is `unknown`, not stopped: status renders
`daemon_state: unknown`, and stale cleanup, replacement, disable, and stop
preserve the lock instead of starting another daemon or signaling an
unverified process.

After each sync attempt the daemon records a best-effort run state in
`.repogrammar/autosync-run.json` (last sync time, result, synced generation,
and any error). This historical record is not the outcome of the current
`start` or `status` command. Human output therefore labels it as the
`previous_autosync_attempt` time/result plus optional generation/error. JSON
retains the preview-compatible `last_run` object and adds the explicit
`previous_autosync_attempt` alias. Run-state writes and both renderers accept
only the fixed errors `repository fingerprint failed`, `repository state is
unavailable`, and `repository sync failed`; any other legacy or malformed
error is preserved on disk but rendered as `previous autosync attempt failed`.
Synced generations are rendered only when they are canonical positive `gen-N`
ids with exactly six ASCII digits; invalid legacy values are omitted. Both
output modes separately report current
`startup_state`, current `daemon_state`, current `repository_ready`, and an
optional `startup_failure_code`; `running: true` can therefore be distinguished
from both startup readiness and a previously completed sync. A failed startup
belongs to the current state only while the same live lock nonce owns it. Once
that owner exits, or when a concurrent different nonce failed, current
`startup_state` is not rewritten as failed; the sanitized code is exposed only
as `previous_startup_failure_code`. A missing or
unreadable run-state file is reported as absent rather than failing the status
read, and historical errors remain preserved.

`autosync` supports `--project <path>`, `--path <path>`, `--json`, `--quiet`,
`--progress auto|always|never` for long-running command compatibility,
`--poll-ms <n>` where `n` is 100 through 600000, and `--debounce-ms <n>` where
`n` is 0 through 60000. `REPOGRAMMAR_STRICT_GITIGNORE=true` applies to
auto-sync discovery and sync just as it does for manual `index` and `sync`;
semantic worker environment variables are inherited by the foreground worker.
Auto-sync output must not include source snippets, absolute paths, content
hashes, symbols, raw targets, or repository-identifying details.

`repogrammar unlock` must remove only confirmed stale locks. It must inspect the
recorded process, host, OS, and advisory lock state before deletion. `--force`
must require explicit confirmation. Without `--force --yes`, unlock is
inspection-only. With `--force --yes`, it may remove only a confirmed stale
`index.lock`; active, unknown, invalid, daemon, and SQLite locks must remain in
place with a stable refusal reason.

`repogrammar logs` reads repo-local diagnostic logs from
`.repogrammar/logs/<component>.log`. It supports:

- `--tail [n]`, defaulting to 100 lines and bounded by implementation limits;
- `--since <duration>`;
- `--component index|daemon|mcp|telemetry`;
- `--redact`.

The default component is `daemon`. Missing, unreadable, non-file, symlinked, or
malformed log files must return a clean unavailable report rather than panic.
`--since` is accepted for contract stability but may return the bounded tail
with a message that duration filtering is not implemented. Logs are diagnostic
state, not telemetry. JSON output includes the selected `component` and the raw
`component_filter` option value. `logs` redacts by default and
machine-readable output must not include source snippets or absolute paths.

## Managed instruction commands

`repogrammar instructions` provides a separate explicit-file lifecycle for the
short RepoGrammar pre-flight block:

```text
repogrammar instructions status --file <path> [--json]
repogrammar instructions sync --file <path> [--dry-run] [--yes] [--json]
repogrammar instructions remove --file <path> [--dry-run] [--yes] [--json]
```

`--file` is required. Relative paths resolve from the current directory; no
agent-specific file path is guessed. `status` is read-only. Live `sync` and
`remove` require `--yes`, while `--dry-run` reports the planned action without
writes. The command must never create repository state, initialize an index,
reconfigure native MCP integration, or mirror another instruction filename.
Selecting `AGENTS.md` does not authorize a `CLAUDE.md` write, and vice versa.

The content contract is versioned independently from the product version.
Status reports `missing`, `current`, `outdated`, `foreign`, or `malformed`.
The deployed unversioned global legacy block reports detected content version
`0`; the exact later legacy body is classified as logical content version `1`
even though it had no embedded version marker; current content reports `2`.
Sync may create or append a missing block and may refresh either
exact known legacy body. A complete marker pair containing modified or unknown
content is `foreign`, not owned. Foreign, partial, duplicated, or malformed
sections are preserved and refused for sync/remove. Remove strips only an exact recognized
current or legacy block and preserves unrelated content.

The block's trigger semantics are canonical in the installation and MCP
specifications. In particular, schema/protocol/API/prompt-output or Meaning
Contract conformance and drift work is RepoGrammar-first even when it names an
exact file or YAML prompt; pure operational or single-fact inspection remains
outside the gate when no repository contract or implementation comparison is
required. `instructions sync` installs this shared contract but never executes
the read-only pre-flight itself. After a successful live sync, human output
recommends starting a new coding-agent session because an already-running
session may retain an earlier instruction snapshot. JSON reports the same
guidance through `session_restart_recommended`; status, dry-run, and refused
operations report `false`.

JSON output is low-cardinality and path-free. It includes command/operation,
status, before/after state, detected and expected content versions, file-existed,
dry-run/would-change/changed booleans, action, repairability, and a nullable
refusal code, plus the low-cardinality `session_restart_recommended` boolean.
Refusal codes are `confirmation_required`,
`foreign_managed_section`, and `malformed_managed_section`. Filesystem failures
use the sanitized `instruction_file_unavailable` reason without raw paths or OS
details. Live replacement uses a uniquely created sibling temporary file,
refuses preoccupied candidates without following or removing them, preserves
the existing file mode, owner uid, and group gid, and checks the open temporary
file's identity immediately before activation. Failure cleanup retains that
open handle through a device/inode/link-count identity check and removes only
the still-matching operation-owned pathname. This is not a cross-process
compare-and-swap; the unsupported same-directory hostile pathname race is
defined in the installation specification.

## Installer commands

`install` and `uninstall` must support:

- `--target`
- `--scope global|project`
- `--location global|local` as a `--scope` alias
- `--dry-run`
- `--yes`
- `--print-config`
- `--telemetry`
- `--no-telemetry`
- `--no-permissions`

Installer commands configure agents and machine-level integration only. They do
not create, delete, or rewrite `.repogrammar/`, and they do not run `init`,
`index`, or `sync`. The installer follows a CodeGraph-style target-registry
state machine while preserving RepoGrammar's safety boundaries. `--target`
accepts `auto`, `all`, `none`, single concrete ids, and comma-separated
concrete target lists. Recognized concrete ids are `codex`, `claude-code`
(`claude` alias), `cursor`, `opencode`, `hermes`, `gemini`, `antigravity`, and
`kiro`. `repogrammar install` with no flags launches a simple TUI-style text
wizard when running in an interactive terminal. The wizard supports multi-select
Codex and Claude Code in one run, shows existing RepoGrammar-managed receipts,
uses `a` as the default automatic selection, selects only detected
not-yet-managed agents through that default, reports a no-op when that set is
empty, and lets users explicitly add missing supported agents on later runs.

Noninteractive live writes require `--yes`. `install --yes`, `install
--dry-run`, and explicit `--target ... --yes` must never prompt. The current
implementation supports `--target codex --scope global` through the native
Codex MCP CLI, `--target claude-code --scope global` through the native Claude
Code MCP CLI, and safe `--target all --scope global --yes` through the same
all-or-rollback transaction. In the interactive wizard, anonymous telemetry
consent remains default-no, while the final reviewed install-plan confirmation
is default-yes. `all` and `auto` resolve to the current first-class live targets
for safe noninteractive writes. Registry targets without a live writer must fail
before command-path, receipt, or native config writes and direct the user to
`--dry-run` or `--print-config`. Project-local writes remain deferred.

`install` places the `repogrammar` command in a user-writable command directory
when possible, runs a read-only MCP self-test before native configuration,
writes one managed receipt per configured target, and rolls back all changes
from the same run if any selected agent install, native verification, receipt
write, or final product `tools/list` self-test fails. Before any command-path or
native write, the installer uses the selected agent's bounded, read-only native `mcp get`
operation. Only an exact target-specific not-found response is absent; unknown
or malformed probe output fails closed and is not echoed. A same-name native
entry without a RepoGrammar receipt is foreign. A receipt whose native entry is
missing, has a different scope or executable, or does not use exactly the
`serve` argument is drifted. Foreign and drifted states are preserved and block
automatic repair. After configuration, both the exact native entry and the
installed product's exact-one-tool MCP surface must be verified before success.
Re-running `install` refreshes only a RepoGrammar-managed command path and
skips native agent add commands for already managed target receipts. When the
selected managed binary or managed command copy already exists, refresh stages
the new file, removes the previous RepoGrammar-managed file, and then activates
the new file. If the previous managed file cannot be removed because a running
coding agent or MCP process still holds it, install must fail with guidance to
exit that agent and rerun the install or build command. When the selected
command path is the same executable currently running the installer, such as a
local Cargo-installed `repogrammar.exe` on PATH, the installer may copy that
executable into RepoGrammar-managed user state and continue without overwriting
that currently executing command path in the same run. Existing unrelated
foreign command paths must still be refused rather than adopted silently.
`uninstall` removes only receipt-owned managed entries. `uninstall --target all
--scope global --yes` removes every owned first-class agent receipt it finds,
but refuses unmanaged or foreign receipts.
Dry-run install output reports the native MCP command shape for supported
global Codex and Claude Code targets and deferred MCP snippet guidance for
registry targets without live writers. `--print-config <target>` prints a
target-specific MCP configuration snippet and exits without requiring a live
write confirmation, creating install state, running an MCP self-test, or
delegating native writes. Live `install --yes` must not prompt for telemetry;
if neither `--telemetry` nor `--no-telemetry` is provided, telemetry remains
disabled. `--yes` itself never implies telemetry consent.
Interactive telemetry prompts are allowed only in the default TUI-style
installer, only when no telemetry flag was supplied, and the default is no.
Install does not upload telemetry or run paired token-saving experiments.

## Metrics commands

`repogrammar unknowns` reports aggregate persisted semantic `UNKNOWN` inventory
for initialized repositories with an active readable generation. It does not
claim to count every query-time, family-store, preflight, or storage fallback
`UNKNOWN`. With `--json`, it must return a parseable object with
`implemented: true`, `status: ok`, and an `unknown_inventory` object containing
`inventory_scope: persisted_semantic_unknowns`, the active generation, total
counts for `blocking_unknowns`, `non_blocking_unknowns`, `recoverable_unknowns`,
and `irreducible_unknowns`, plus rollups named:

- `by_language`
- `by_language_detail`
- `by_reason_code`
- `by_required_mechanism`
- `by_framework_role`
- `by_role_state`
- `by_blocks_support`
- `by_recovery_code`

The inventory is diagnostic and source-free. It must not include source
snippets, query text, repository names, absolute paths, code-unit ids, or fact
ids by default. `by_recovery_code` is a stable low-cardinality code bucket,
never the free-text recovery guidance stored on individual facts. Recovery
codes include `run_sync`, `add_project_config`, `enable_provider`,
`not_implemented_in_current_version`, `resolve_import_graph`,
`resolve_fixture_graph`, `resolve_dependency_metadata`,
`runtime_trace_required`, `manual_review_required`, and `unknown`.
`enable_provider` names only mechanisms an integrated optional provider (today
the TypeScript compiler slot) can resolve; mechanisms a registered-but-not-
integrated slot or a future provider would resolve report
`not_implemented_in_current_version`. `by_role_state` uses
`none`, `single`, or `ambiguous`; ambiguous framework roles are reported as
support-risk because they block confident family-support interpretation.
`by_language_detail` is a source-free readiness-scoped language rollup. It may
combine raw TypeScript/JavaScript variants into `typescript/javascript`, and it
includes only counts plus top low-cardinality reason and required-mechanism
buckets.
Unknown-rate changes are not quality claims unless false certainty is also
controlled. If repository state or the active index is missing,
`unknowns --json` uses exit code 2 with the same missing-index fallback shape as
implemented inventory commands, keeps `implemented: true`, and includes
`inventory_available: false`; this means the inventory is not ready, not that
the command crashed internally.

`repogrammar stats` reports Python-family repo-shape diagnostics for initialized
repositories with an active readable generation. With `--json`, it must return
a parseable object with `implemented: true`,
`official_family_scope: python_v0_1`,
`repo_shape_scope: python_family_eligible_units`, the active generation,
source-free repository readiness state, and source-free indexed inventory
counts for indexed files, indexed code units, and semantic facts. Indexed
inventory counts describe the active read model and may be nonzero even when
the official Python family `eligible_code_units` count is `0`,
metric-kind vocabulary, top-level `token_saving_readiness`, `blocking_reasons`,
`measurement_kind`, and `caveat` fields,
`null` values for measured `token_savings`, `token_savings_ratio`,
`measurement_source`, and `context_compression_ratio` unless a comparable local
paired token experiment exists, and diagnostic metrics:

- `local_pattern_density`
- `family_support_coverage`
- `abstention_rate`
- `external_dependency_signal`
- `thin_wrapper_risk`
- `token_saving_risk`

These values are product diagnostics, not causal token-saving claims. If data
is insufficient, individual values must be `null` or `unknown` rather than
guessed. Without `--unknowns`, `stats --json` must use the active read-model
aggregate path and must not hydrate family evidence, semantic facts, IR graphs,
full claim-input snapshots, or per-family detail. `stats` may report the
repo-local aggregate
`estimated_potential_token_savings` with event count, estimated baseline and
returned token totals, `measurement_kind: ESTIMATED`, and a not-measured caveat.
These values and `total_queries` come from one `family-query-metrics.v2` atomic
cohort with an explicit epoch, start timestamp, and producer version. Legacy v1
savings and query-outcome files are excluded because their events are unpaired.
This aggregate is all-scope: it sums savings events across every indexed
language and every context-delivering outcome shape (found, PARTIAL_CONTEXT, and
committed/partial alignment certificates), not only Python found families.
`stats --json` must also include an `all_scope_token_savings` block with the
same totals, `measurement_kind: ESTIMATED`, the caveat, the honest
`savings_events` / `total_queries` denominator, and `by_outcome_shape` and
`by_language` breakdown objects (each key mapping to `event_count` plus the
estimated baseline/returned/potential token counts). All existing Python-scoped
repo-shape fields are unchanged and remain the official-scope subset; the block
carries a note pointing to that relationship. It must also include
`query_outcome_rollup`, the denominator side of that same local-only source-free cohort,
with `rollup_scope: local_query_outcomes`, aggregate event count, status,
entrypoint, CLI command/MCP operation category, lookup-mode, typed UNKNOWN
class/reason/mechanism/recovery buckets, read-plan count buckets, and
source-span request/inclusion/omission buckets. `query_outcome_rollup` counts
every outcome (including `UNKNOWN`, `PARTIAL_CONTEXT`, and fallback) and is the
`total_queries` denominator; its counts are a distinct metric from
`estimated_potential_token_savings` events and are not presented as successful
family hits. The human `stats` output leads with a concise summary (readiness,
indexed inventory, family coverage, the all-scope savings headline, and the
scope note) and moves the full per-metric detail behind `--json`; no `--json`
field is dropped.
Measured `token_savings` remains `null` unless a comparable paired experiment
exists. When only estimates exist, top-level `measurement_kind` must be
`ESTIMATED`, `blocking_reasons` must include `no_paired_experiment`, and
`caveat` must say the value is estimated potential only, not measured token
savings. If repository shape is not ready for useful read-displacement
estimates, `blocking_reasons` must also name concrete causes such as
`no_supported_units`, `no_families`, or `low_pattern_density`. The output must
not include source snippets, query text, repository names, or absolute paths.
Stats JSON must also include a top-level source-free `by_language` readiness
section with `language`, `language_scope`, `indexed_file_count`,
`indexed_code_unit_count`, `eligible_code_units`, `family_count`,
`family_member_count`, `family_support_coverage`, `blocking_unknowns`,
`top_required_mechanisms`, `top_reason_codes`, `support_risk`, `preview_status`,
and `unknown_inventory_available`. It must also include source-free
`scope_explanations` that state `official_family_scope`,
`repo_shape_scope`, why `eligible_code_units: 0` can be expected for
non-Python or unsupported-family repositories, and the current unsupported
React/RN status. When TS/JS code units are indexed but supported TS/JS family
count is zero, `scope_explanations` must include
`tsjs_indexed_context_available: true`,
`tsjs_family_support: none_or_unsupported`,
`react_rn_family_support: unsupported`, and
`recommended_next_action: use repogrammar find/check with exact repo-relative paths for PARTIAL_CONTEXT read plans`.
The required language scopes are
`python`/`official_v0_1`, `typescript/javascript`/`bounded_v0_2_preview`,
`rust`/`bounded_v0_2_preview`, `java`/`bounded_v0_2_preview`,
`csharp`/`bounded_v0_2_preview`, and `c/cpp`/`bounded_v0_2_preview`. Python
top-level repo-shape readiness — including `family_count` — remains separate
from multi-language preview readiness. These diagnostics must not be described
as React/RN family support, React/RN conformance, or measured token savings.
With `--unknowns --json`, stats must embed the same source-free
persisted semantic `unknown_inventory` object produced by
`repogrammar unknowns --json`; without `--unknowns`, that object must be
omitted. The `repo_shape_scope` label and the inventory's `inventory_scope`
label must both remain present, and `query_outcome_rollup` must keep its
separate `rollup_scope`, so Python-family readiness, multi-language persisted
semantic unknowns, and query-time outcome observability are not conflated.
If repository state or the active index is missing, `stats --json` uses the
same missing-index fallback shape as implemented inventory commands, keeps
`implemented: true`, and still reports `token_saving_readiness: unknown`,
`measurement_kind: ESTIMATED`, a not-measured caveat, blocking reasons,
`official_family_scope: python_v0_1`,
`repo_shape_scope: python_family_eligible_units`,
`readiness_available: false`, unavailable indexed inventory counts, and an empty
`by_language` array.
When `--unknowns` was requested, the fallback must also include
`inventory_available: false`.

`repogrammar telemetry` owns anonymous product telemetry consent, explicit
upload, research trace consent, and local paired token experiment recording.
Telemetry is disabled by default. `REPOGRAMMAR_TELEMETRY=0`,
`DO_NOT_TRACK=1`, and CI force effective telemetry off and prevent upload
network activity. Supported subcommands are:

- `telemetry status [--json] [--project <path>]`
- `telemetry on [--json] [--project <path>]`
- `telemetry off [--json] [--project <path>]`
- `telemetry export [--json] [--project <path>]`
- `telemetry upload [--json] [--dry-run] [--yes] [--endpoint <url>] [--project <path>]`
- `telemetry purge [--json] --yes [--project <path>]`
- `telemetry research-status|research-on|research-off|research-export|research-purge [--json] [--yes] [--project <path>]`
- `telemetry experiment-start|experiment-record|experiment-stop|experiment-report|experiment-export|experiment-purge`

`--project <path>` selects the repository root for anonymous telemetry and
research diagnostics only. Experiment subcommands use machine-local experiment
state and accept only their dedicated options shown by `repogrammar help
telemetry`; they reject `--project` instead of silently ignoring it.

Upload uses `REPOGRAMMAR_TELEMETRY_ENDPOINT` when `--endpoint` is not supplied.
Endpoints must be HTTPS except localhost test endpoints. No endpoint configured
returns a parseable not-uploaded result. `upload --dry-run` validates and
prints the exact allowlisted payload without opening a network connection.
Non-dry-run upload requires `--yes`.
`telemetry status --json` reports anonymous and research preferences, effective
environment/CI disablement, rollup/queue/sent counts, endpoint configuration,
and whether an explicit upload would open a network connection.
`telemetry export --json` is inspect-only and does not create a queue or
rollup. `stats --json` never uploads; when anonymous telemetry is effectively
enabled it may update a local allowlisted passive-diagnostics rollup without
creating an upload queue.

Paired token measurements are local only unless the user also opts into
anonymous telemetry upload of aggregate buckets. Actual token savings are:

```text
baseline_total_tokens - treatment_total_tokens
```

They are reported only when comparable baseline and treatment sessions share a
measurement source. Accepted sources are `host_reported`, `user_entered`, and
`documented_tokenizer`.
Experiment start requires explicit confirmation. In non-interactive use,
`--yes` confirms recording. In interactive product runs without `--yes`,
`experiment-start` prompts with default-no `[y/N]`; empty input, `n`, or `no`
does not create an experiment record, and only `y` or `yes` proceeds.
`--experiment-mode record-existing` records counts from already performed
sessions and usually does not increase token usage. `--experiment-mode
controlled-pair` records comparable baseline/treatment measurements and warns
that users may spend additional time, tokens, and provider cost if they choose
to run separate sessions. `--session baseline|treatment` identifies the
measurement side. `experiment-record` accepts either explicit
`--input-tokens`, `--output-tokens`, optional `--tool-tokens`, and `--success`
flags, or `--usage-json <path>` pointing at a redacted local usage file.
Usage-import files may contain only token counts, optional success, and
optional test outcome, either at the top level or under `usage`; supported
count names are `input_tokens`/`prompt_tokens`,
`output_tokens`/`completion_tokens`, optional `tool_tokens`, and optional
`total_tokens` for deriving tool tokens. Command-line values override imported
values, and `tool_tokens` defaults to zero when neither a separate nor
derivable tool-token count is present. Unsupported usage-import fields are
rejected so raw prompt, message, source, path, symbol, patch, or query payloads
cannot be accepted as token experiment input. If treatment correctness fails,
reports keep the raw token delta but mark the result invalid for product
token-saving claims.
`experiment-export --json` is redacted by default and must not include the
user-provided experiment name, session ids, raw token counts, prompts, paths,
repository names, symbols, or source.

## Disallowed top-level graph commands

The following CodeGraph-style names must not be added as top-level v0.1
commands:

- `callers`
- `callees`
- `impact`
- `affected`
- `node`
- `explore`

If call-graph functionality is later needed, it must live under a secondary
namespace such as `repogrammar graph callers` and must not be presented as the
primary value proposition.

An optional lower-layer provider such as CodeGraph must not change this command
surface. Provider facts may enrich future pattern-family evidence only after
translation into RepoGrammar-owned evidence with provenance and freshness.

## Missing or stale indexes

Query commands must check project initialization and freshness before making
family claims. If `.repogrammar/` is missing, the command must return clean
fallback guidance rather than panic or implicitly initialize the repository:

```text
FALLBACK_TO_CODE_SEARCH
reason: repository is not initialized
guidance: run repogrammar init --yes
```

During the bootstrap, pattern-family query commands use this fallback shape only
when repository state or an active index generation is missing or unreadable.
`status` and `doctor` may report a clean not-initialized state without opening
storage. Stored syntax-only code units are not family evidence; stored
syntax-origin framework-role facts remain insufficient support unless stronger
compatible semantic/dataflow evidence exists. Exact-anchor Python
`DATAFLOW_DERIVED` support may produce narrow EC-MVFI-lite family rows when the
family support threshold is met, but query commands must not imply that
provider-backed Python v0.1 analysis, TypeScript compiler analysis, full
mining, or broad production family evidence has run. The
`files` and `units` commands are a limited exception: when an active
file-manifest-only or syntax-only generation exists, they may read and return
repo-relative indexed-file metadata and code-unit records for inventory/debugging
only. Their accepted option surface is limited to `--project`/`--path` and
`--json`; the query-only options (`--mode`, `--token-budget`, `--include-*`) and
a positional target do not apply and are rejected rather than silently ignored.
The query application layer now owns a shared preflight contract so pattern
family commands enter the active FamilyStore read path once a readable active
generation exists, while `files` and `units` are implemented inventory commands
whose fallback means an active inventory index precondition is missing or
unreadable. Semantic-fact freshness/readiness checks remain internal and must
not introduce semantic-fact CLI output. The presence of FamilyStore tables is
not by itself a strong claim: the query layer must return typed `UNKNOWN` when
support, compatibility, or evidence is insufficient. MCP serving uses the same
query preflight and family lookup boundary rather than a separate claim path.

With `--json`, query fallback output must use exit status `2` and write a
stable JSON object to `stderr` rather than the human text block:

```json
{
  "status": "FALLBACK_TO_CODE_SEARCH",
  "reason": "repository is not initialized",
  "guidance": "run repogrammar init --yes",
  "command": "find",
  "implemented": false
}
```

For `files` and `units`, the command itself is implemented even when its active
index precondition is not met. Their missing/unreadable-index JSON fallback must
therefore set `implemented: true`; pattern-family query commands set
`implemented: false` only for missing/unreadable active index fallback and set
`implemented: true` when they return typed `UNKNOWN` from a readable active
generation.

If the index is stale, the command must warn or refuse claims whose evidence has
changed.

Analysis uncertainty must be reported as typed `UNKNOWN` with a reason code and
affected claim. Missing-index fallback, deferred stronger query evidence,
stale-index refusal, and typed `UNKNOWN` are separate states and must not be
collapsed into one error string.

If repository-local state exists but no active generation exists, query commands
must keep the same `FALLBACK_TO_CODE_SEARCH` marker with `reason: no active
index generation`, not `repository is not initialized`. If a readable active
generation exists but no supported family evidence has been built, query
commands must return typed `UNKNOWN` with `InsufficientSupport` instead of the
fallback marker.
If repository status cannot be read safely, the fallback must direct the user to
`repogrammar doctor` instead of masking corrupted state as an uninitialized
repository.
For `files` and `units`, initialized state with no active generation must keep
the fallback marker but use `reason: no active index generation`, guidance to
run `repogrammar index`, and `implemented: true` in JSON. Corrupt or unreadable
state must direct users to `repogrammar doctor`. Once an active
file-manifest-only or syntax-only generation exists, `files --json` must return
`status: ok`, `implemented: true`, the active generation, an `indexing` value of
either `file_manifest_only` or `syntax_only_code_units`, and a `files` array of
repo-relative paths, languages, sizes, and strict content hashes. `units --json`
must return the active generation, the same `indexing` contract,
`semantic_worker: deferred`, `mining: deferred`, and a `units` array of
repo-relative unit ids, paths, languages, kinds, byte ranges, and strict content
hashes. Neither command may include source snippets or absolute paths.

For active pattern-family commands, `families --json` returns `status: ok` and a
`families` array when family rows exist; otherwise it returns `status: UNKNOWN`,
`implemented: true`, and a typed `InsufficientSupport` unknown on stdout.
`families --json` verifies evidence freshness before serving the listing. It
reads one bounded projection of the active generation's family evidence, then
hash-verifies each distinct evidence path at most once (never once per family),
so the number of source reads is bounded by the distinct evidence paths rather
than the sum over families. Each family entry carries a `freshness` field with
one of three values — `fresh` (every evidence path verified with a matching
hash), `stale` (at least one evidence path is missing or its hash changed), or
`cannot_verify` (no stale path, but at least one path failed verification for a
non-content reason, or the family has zero evidence rows). The report adds
`fresh_count`, `stale_count`, and `cannot_verify_count`. Stale and
`cannot_verify` families remain listed but must not read as unqualified usable
claims: the human surface leads with the counts and qualifies them distinctly,
and the JSON carries the `freshness` field and counts verbatim. When at least
one family is stale, the report also carries one low-cardinality report-level
`StaleEvidence` unknown with recovery `run repogrammar resync`; a partially
stale listing is never turned into `status: UNKNOWN`. The freshness-free
`list_families` variant (used by internal callers that explicitly do not want
freshness) omits the `freshness` field and the counts.
`family`, `member`, `find`, `explain`, and `check` accept the first positional
operand as their target. `family <target>` is an exact family-id lookup.
`member <target>` is an exact code-unit/member-id lookup. `find`, `explain`,
and `check` use an internal `discover -> hydrate -> compose` loop over the
target the caller already has: repo-relative paths or suffixes, exact
member/code-unit ids, exact member roles, exact family ids, and supported
query-safe pattern text. These commands may accept family ids, but family ids are
not required initial inputs; returned family ids are follow-up handles for exact
inspection. They must not treat short substrings such as a framework name,
classification label, or directory fragment as a successful family match. When a
fuzzy path or path suffix matches evidence in multiple families, the command must
return typed `UNKNOWN` instead of selecting the first stored family. That unknown
uses `InsufficientSupport`, affected claim
`query target ambiguity`, and recovery guidance that tells the caller to narrow
the target to an exact family id or member id while naming the candidate family
ids. Fuzzy role/path candidate discovery must be bounded; if the candidate set
is truncated or exceeds the configured cap, the family probe blocks with typed
`UNKNOWN` (affected claim `query target candidate set`) and candidate
ids/recovery guidance instead of hydrating all families. These fuzzy family
blocks — no candidate, a too-broad or truncated candidate set, or several
competing families — still route through the local-context fallback below, so a
target that resolves to exactly one indexed path or code unit earns
`PARTIAL_CONTEXT` while the family stays unguessed, and a target that remains
ambiguous or unresolvable keeps the typed `UNKNOWN`.
Query targets must be non-empty, at most 8192 bytes, and free of
control characters. The deterministic target resolver recognizes exact
repo-relative indexed paths, exact member/code-unit ids, embedded indexed paths
inside longer text, unique indexed path suffixes, `path:line`, and
`path:start-end` byte-range forms. It records the raw target, resolved kind,
repo-relative path, optional line, optional byte range, optional family/member
ids, symbol hints, residue terms, candidate paths/ids, confidence, and match
kind. It must prefer exact indexed paths over suffixes and must return ambiguity
instead of choosing the first storage-order match.
When `find`, `explain`, or `check` can deterministically resolve a fuzzy target
to exactly one indexed repo-relative path or code unit in the active generation
but no family evidence supports a claim for that target, the command returns
`status: PARTIAL_CONTEXT` instead of pretending a family was found.
`PARTIAL_CONTEXT` is metadata-only local context: it includes `query_route`, the
resolved target, a single target read-plan item, output metadata, and a typed
`InsufficientSupport` unknown for `pattern family evidence for resolved target`.
It is not family evidence, not conformance evidence, and not safe to treat as a
supported pattern claim. A `PARTIAL_CONTEXT` response also carries the
`estimated_potential_token_savings` block (never omitted): `outcome_shape:
partial_context`, the resolved file's `language` scope, the estimated
baseline/returned/potential token counts, `ESTIMATED` kind, and the not-measured
caveat. The baseline is the estimated whole-file read the read plan displaces,
taken from the indexed file inventory's stored size; when that size is
unavailable the block reports null counts with an `unavailable_reason` rather
than a guessed number. Alignment (`check`) certificates carry the same block
with `outcome_shape: alignment`; an abstaining certificate reports the null
block.

When a `find`/`explain` target names a **directory / composite scope**, the
command reports the resolved candidate-set cardinality through an additive
top-level `resolution` object (byte-parallel with the MCP shape; see
`docs/specifications/mcp-api.md` and ADR-0029). No new top-level status token is
added — the response stays on `product-schemas.v1`:

```json
"resolution": { "cardinality": "none|one|many|truncated", "candidates": [ { "family_id": "family:...", "summary": "python fastapi.route · DOMINANT_PATTERN" } ] }
```

A single proven in-scope family is `status: ok` + `resolution.cardinality: one`;
several in-scope families are `PARTIAL_CONTEXT` + `many` (bounded candidate
summaries, **never** a `selected_family_id`); a resolved-but-familyless scope is
`PARTIAL_CONTEXT` + `none` (empty candidates); a bounded read that may hide
families is `PARTIAL_CONTEXT` + `truncated`. Each candidate `summary` is projected
from the family search-summary projection (never a hydrated deep family, never raw
source). `resolution` is an additive `standard`/`full` field, dropped at
`--verbosity minimal`; for `many`/`truncated` the candidate `family_id`s are also
carried on `resolved_target.candidate_family_ids` and
`query_route.follow_up_family_ids`, which are retained at `minimal`, so no
narrowing handle is lost. Exact `family` and `member` lookups continue to return
typed `UNKNOWN` when their exact ids are missing. `family`, `member`, `find`,
`explain`, and `check` JSON outputs must include `query_route` with `route`,
`input_kind`, `pipeline`, `family_id_policy`, `candidate_limit`,
`selected_family_id`, `candidate_family_ids`, `follow_up_family_ids`, and
`why_selected`. Candidate/follow-up family ids are narrowing handles; only
`selected_family_id` on a matched family response is a supported family claim.
At `--verbosity minimal` this object is reduced to `route` and
`follow_up_family_ids`; the duplicate `candidate_family_ids` is dropped on a
matched family but kept as a recovery handle on `PARTIAL_CONTEXT`, `UNKNOWN`, and
conformance abstentions, and the remaining diagnostic fields appear only at
`standard`/`full`.
When a natural-language, synonym, or framework-plus-concept target is resolved by
the deterministic term-retrieval fallback, `query_route` additionally carries
`hydrated_family_count`, `retrieval_stage_count`, and a source-free
`term_retrieval` object: `route` (`term_retrieval_hydrate` |
`term_retrieval_unknown`), `retrieved_summary_count`, `ranked_candidate_count`,
`hydrated_candidate_count`, `retrieval_stage_count`, raw `top_score`/`margin`,
`top_score_bucket`/`margin_bucket`, `truncated`, `matched_signals`, and
`abstention_reason`. The `abstention_reason` vocabulary is `no_candidate`,
`below_min_score`, `unsupported_target`, `margin_too_close`, `truncated_tie`,
`stale_candidates`, and `hydration_ambiguous` (null when a family was found).
These fields are null for exact/role/path routes. The human-readable output adds
`query_term_route`, and either `query_term_matched_signal` (on a match) or
`query_term_abstention_reason` with the bounded candidate family ids (on an
abstention). See `docs/specifications/query-resolution.md`.
Matched family output defaults to `--mode compact`: family
id, classification, support, members, variation slots, a metadata-only
`constraint_profile`, typed unknowns, selected output metadata, a `read_plan`,
and no evidence records or source snippets. The inline `members` array is
bounded: outside `--mode deep` it is capped at the first 20 members in unchanged
deterministic order, and the response always carries the true `member_count` and
a `members_truncated` flag so a large family (family identity is metadata-first)
never inflates a single response. `--mode deep` restores the full member list.
A matched found-family response carries its estimate as scalar fields nested in
the `output` object, not as a separate block: `estimated_evidence_tokens`,
`estimated_read_plan_tokens`, `estimated_baseline_tokens`,
`estimated_returned_tokens`, `estimated_potential_token_savings`,
`estimated_potential_token_savings_kind` (`ESTIMATED`), and
`estimated_potential_token_savings_caveat`. The found `output` block does not
carry `outcome_shape` or `language`; those attribution tokens are recorded only
in the local savings rollup, and the standalone
`estimated_potential_token_savings` block that does surface `outcome_shape` and
`language` appears only on `PARTIAL_CONTEXT` and alignment (`check`) responses.
The `constraint_profile` object is
the family's hydrated source-backed specification (`required_equal_features`,
`allowed_variations`, `prohibited_or_blocking_features`, and
`unresolved_obligations`, each a typed token or count; see
`docs/specifications/domain-model.md`), or `null` when the active generation
persisted none.
`--mode evidence` adds budgeted repo-relative evidence metadata:
evidence id, family id, code-unit id, path, content hash, byte range, note,
estimated token cost, and covered claim labels. The shared read plan is present
in compact, evidence, and deep outputs and contains suggested source spans to
inspect before acting on the family result. Each read-plan item includes
purpose, repo-relative path, strict content hash, byte range, optional line
range, estimated token cost, a short reason, whether source is required before
editing, and whether a source snippet was included for that item.
`--include-source-spans` is the only CLI source-output opt-in. When absent,
RepoGrammar still attempts hash-checked metadata-only line-range enrichment for
read-plan items and keeps `source_snippets_included` false. Fresh source hashes
should therefore produce `start_line` and `end_line` without returning source
text. Stale, missing, hash-mismatched, too-large, non-UTF-8, unavailable, or
invalid ranges must preserve the read-plan item and add `line_range_omissions`
guidance telling the user to use normal Read/Grep for the affected span. When
`--include-source-spans` is present, RepoGrammar renders only selected
read-plan spans through the hash-checked source-store boundary, fills line
ranges for rendered spans, and places line-numbered text under a separate
`source_spans` block. Stale, missing, hash-mismatched, too-large, unsupported,
dynamic, insufficient, or conflicting cases must omit rendered spans and tell
the user to use normal Read/Grep for the affected file or claim. Read-plan items
are ordered by purpose priority so a budget-truncated plan keeps the most
decision-critical prefix. At `--verbosity minimal` the read plan adds an honest
`truncated` flag and `item_count`; a span rendered into `source_spans` is left
as a `{purpose, path, rendered: true}` back-reference rather than a repeated full
item, because the rendered `source_spans` entry is the single source of truth and
is treated as already read; and the empty `source_spans` stub is omitted when
spans are not requested. `standard` and `full` keep the full items and the stub
unchanged, byte-identical to the pre-precision shape. The read plan
must never include absolute paths or a claim that editing is safe outside
listed ranges.
`--token-budget <n>` validates a positive bounded integer and implies
`--mode evidence` unless an explicit mode is provided. Evidence mode
uses deterministic greedy marginal coverage per estimated token cost. Stored
family evidence carries schema-backed `covered_claims` labels from the
allowlist `canonical`, `support`, `contrast`, `variation`, and `exception`; the
selector must consume those labels rather than inferring coverage from note text
or storage order. The family builder assigns labels by coverage, not storage
order: the canonical medoid carries `canonical`, every member carries `support`,
the farthest-from-medoid support witness additionally carries `contrast`, and one
representative per observed variation profile carries `variation` (the canonical
medoid is excluded from `contrast` and variation witnesses). Hydration re-sorts
evidence by path, so the medoid-first write order is not the carrier — the
`contrast` label is. See the Representative selection rule in
`docs/specifications/domain-model.md`. When the hydrated `constraint_profile`
enumerates variation dimensions, evidence-mode selection covers one witness per
dimension plus the anchor-target dimension when its slot exists; otherwise a
single variation witness is requested from the variation-slot signal. Read-plan
purposes follow the same labels: `canonical_evidence` names the medoid,
`support_evidence` prefers the `contrast`-labelled witness (falling back to the
first distinct-path `support` member), and `variation_guard` names a variation
witness. `--include-exceptions` still reports missing coverage; a variation
dimension can only be missing under a real budget shortfall, since the canonical
covers any dimension it solely represents. `exception` evidence remains unlinked
in this slice.
`--mode deep` is accepted as an explicit detail request, but it remains
metadata-first and does not imply source output without `--include-source-spans`.
`--verbosity minimal|standard|full` selects response field density and is
orthogonal to `--mode`, which selects evidence detail; the two never interact.
It defaults to `standard`, the current byte-stable structured payload, and is
additive under `product-schemas.v1`: `minimal` opts into the lean shape and
`full` retains every diagnostic field. `standard` and `full` emit byte-identical
output (equal to this development line's pre-precision response, which already
carries the inline-member cap — byte-stable against the pre-precision shape, not
identical to v0.2.2); each precision slice suppresses its demoted fields only at
`minimal`, and every removal is a demotion `full` restores. An unrecognized value
is rejected rather than silently defaulted. At
`minimal` the `query_route` object keeps only `route` and `follow_up_family_ids`
(dropping the duplicate `candidate_family_ids` on a matched family, retaining it
as a recovery handle on `PARTIAL_CONTEXT`/`UNKNOWN`/conformance abstentions) and
suppresses the diagnostic routing fields; the `read_plan`/`source_spans`
reductions (honest truncation flags, read-plan/span dedup, and the dropped empty
`source_spans` stub) also apply only at `minimal`, and further per-field
reductions are documented with their output contracts. This CLI flag stays
byte-parallel with the MCP `verbosity` request field.
None of these modes may include absolute paths. `check` returns a static
alignment certificate. Its top-level `status` is the alignment status token
(`STATICALLY_ALIGNED`, `STATIC_DEVIATION`, `PARTIAL_ALIGNMENT`,
`INSUFFICIENT_EVIDENCE`, or `UNKNOWN`) and the JSON carries these fields:

- `alignment_status` — the same token as `status`; dropped at
  `verbosity: minimal` as a duplicate, retained byte-for-byte at `standard`/`full`.
- `runtime_equivalence` — always the literal `"UNKNOWN"`; static alignment
  never proves runtime equivalence. This invariant is emitted at every verbosity
  and is never suppressed.
- `target_relationship` — `MEMBER`, `NEAR_MISS`, `BLOCKED_UNKNOWN`,
  `OUT_OF_SCOPE`, or `EXCEPTION` (null while abstaining before a family is
  compared). `COMPETING_PATTERN` is a reserved token that no current path emits
  (a member always compares against its own family).
- `selected_family_id` and `query_route.selected_family_id` — the comparison
  family, present only when one was confidently selected; `null` for every
  abstaining outcome. The top-level `selected_family_id` is the authoritative
  carrier of the selected-family handle and is retained at every tier, including
  `minimal`; the `query_route.selected_family_id` copy is the one suppressed at
  `minimal` (by the route lane), so the certificate top-level copy is what keeps
  the selection determinable in the lean shape.
- `target` — the resolved code-unit locator (id, path, byte range). It shares the
  `resolved_target` serializer, so at `verbosity: minimal` it drops the input echo
  (`original_target`), normalizer internals (`residue_terms`), and the
  `candidate_*` lists that only echo an already-resolved locus, while retaining
  those candidate lists when resolution stayed genuinely ambiguous.
- `alignment` — the computation, or `null` when abstaining: `outcome_reason`,
  `required_features_matched[]` (`prefix`, `semantics`, `expected_summary`,
  `satisfied_summary`), `static_deviations[]` (`prefix`, `kind`,
  `semantics_token`, `expected_summary`, `observed_summary`),
  `legal_observed_variations[]`, `blocking_unknowns[]`, and
  `unresolved_runtime_obligations[]` (always non-empty — it carries the
  runtime-equivalence obligation verbatim). As a scale guard,
  `static_deviations[]` and `legal_observed_variations[]` are capped at a fixed
  bound in every tier; a target that exceeds the cap truncates the array to the
  bound and the computation gains an honest `<name>_truncated: true` flag and a
  `<name>_count` total. Below the cap the full arrays are emitted with no
  truncation metadata.
- `read_plan` — the comparison family's evidence read plan, leading with the
  contrast witness.

Deviation `observed_summary` and `expected_summary` values are RepoGrammar
feature TOKENS, never repository source text. The certificate must not contain
proof-like fields such as `pass`, `conforms`, or `fail_on`, and must not contain
the legacy `check` advisory object. The `static_deviations[].kind` vocabulary is
`required_mismatch`, `must_be_empty_violation`, `missing_required_core`, and
`prohibited_presence` (required-feature *violations* that force
`STATIC_DEVIATION`), plus three non-violating partial-alignment signals:
`unobserved_variation` (a value never observed among an untruncated enumeration —
explicitly *unobserved*, never *illegal*), `truncated_observation` (not among an
enumeration that was truncated at the cap, so "never observed" is unprovable), and
`blocking_suppressed_requirement` (an absence-driven required check that a blocking
unknown plausibly suppressed). Under a target blocking unknown, absence-driven
required checks route to `PARTIAL_ALIGNMENT`, while presence-driven violations
(prohibited presence, a present-but-wrong value, a must-be-empty value) still
deviate. Resolution is locator-first: a path-only target that names a file with
more than one family-eligible unit is ambiguous and abstains with
`INSUFFICIENT_EVIDENCE` and candidate unit ids; narrow it with a `path:line`,
`path:byte-start-byteend`, or `unit:` locator. The static-alignment computation is
the single authority for this decision (`application::conformance`); `explain`
remains on the shared fuzzy family-resolution surface and presents family
context, while `check` is the operation that emits the alignment certificate.

Before public pattern-family detail or target-specific claim output is returned,
stored family evidence must be fresh against the current repository source
hashes. If an evidence source is missing or its content hash no longer matches
the active generation, public `family`, `member`, `find`, `explain`, and
`check` output must refuse or omit the stale claim and return typed
`StaleEvidence` `UNKNOWN` guidance instead of rendering stale family detail.
Human and JSON output must preserve the stale reason, affected claim, and
recovery guidance. `families` also verifies evidence freshness, but instead of
refusing it keeps every family listed and qualifies each with a per-family
`fresh`/`stale`/`cannot_verify` verdict plus report-level counts; a stale family
additionally raises one low-cardinality report-level `StaleEvidence` unknown.

## Current implementation status

The bootstrap recognizes the command surface and required options. `init`
creates safe repo-local lifecycle state, `.repogrammar/.gitignore`, required
lifecycle subdirectories, a bootstrap manifest, `receipts/init.json`, and Git
ignore hygiene. `uninit --yes` removes only the resolved RepoGrammar state
directory. `prune --yes` removes only old inactive generation rows from the
mutable database after storage health and active-generation checks; when only
legacy generation directories exist, it falls back to pruning those directories.
`prune --dry-run` reports the same candidates without writes. Successful
mutable index activation and destructive mutable prune run bounded SQLite
maintenance through `PRAGMA optimize` and passive WAL checkpointing. There is
no automatic `VACUUM`; full database compaction is available only through the
explicit confirmation-gated `repogrammar compact --yes` command, with
`compact --dry-run --json` for non-mutating size inspection. `storage clean`
wraps safe legacy-layout cleanup, `prune --keep 0`, and `compact` behind the
same confirmation gate so users do not need to compose destructive maintenance
steps manually.
`status` and `doctor` expose storage layout diagnostics, mutable/legacy
presence, mutable sidecar byte counts, and active dirty/dependency counts.
`unlock` and `logs` expose
human and JSON-safe repo-local lifecycle information without claiming
parser/mining support; `logs` returns a bounded redacted tail for selected
repo-local component logs.
`index` and `resync` create full syntax-only mutable SQLite generations from
the TS/JS file discovery substrate, bounded TS/JS project-config inventory,
Python `.py` discovery/CPython AST structural extraction, and Rust
self-dogfood syntax extraction. `sync` creates the same generation shape, but
uses path-level incremental copy-forward when safe and full-rebuild fallback
otherwise. Their JSON output includes `generation_id`, `active_generation`,
`discovered_files`, `stored_files`, the actual `parser_attempted_files`,
`indexed_units`, and `semantic_facts` counts, the authoritative `indexing` and
`parser` modes described above, `semantic_worker`, and `mining: deferred`.
`sync --json` also
includes `sync_mode`, `fallback_reason`, `base_generation`, `added_files`,
`modified_files`, `removed_files`, `unchanged_files`, `copied_forward_files`,
`reparsed_files`, `families_recomputed`, `dirty_records_cleared`,
`families_added`, and `families_removed`.
`reparsed_files` is the actual number of parser dispatches in the generation,
not the number of changed or discovered inventory-only files.
For the zero-delta fast path, `generation_id` equals `base_generation`,
`sync_mode` remains `incremental`, `unchanged_files` reports the manifest size,
and `copied_forward_files`, `reparsed_files`, and `families_recomputed` are all
zero.
`families_added` and `families_removed` report the cross-generation family
identity change: each is either `null` (when the sync had no base generation to
diff against, or did not recompute families) or an object `{ "count": <n>,
"sample": [<family id>, ...] }` whose bounded `sample` lists up to twenty sorted
ids. They are computed by diffing the base generation's family-id set against
the new generation's; family ids are deterministic follow-up handles, so a
re-clustered family surfaces as one removed and one added id rather than a
rename (see `specifications/domain-model.md`, "Family identity"). The
`dirty_records_cleared` field counts persisted dirty-marker rows actually
cleared while building the new generation. Claim-bearing records deliberately
omitted during generation-by-replacement copy-forward, including legacy Go
records on inventory-only paths, are not dirty markers and therefore do not
increment this field. The
structural extractors can also
produce syntax-origin
framework-role fact records for recognized Express, React, Jest/Vitest, Next.js,
Fastify, Prisma, Drizzle, FastAPI, pytest, Pydantic, and SQLAlchemy code-unit
shapes; these may increase `semantic_facts` while `semantic_worker: deferred`
remains true. Python
parser-origin structural facts, root `pyproject.toml`/`setup.cfg`/`setup.py`
project-config records, TS/JS project-config records, TS/JS exact-anchor support
facts, and TS/JS typed
`UNKNOWN` records for dynamic/unsafe receiver, runner, route, client, or query
boundaries may also increase `semantic_facts` without changing
`semantic_worker: deferred`.
Exact-anchor Python `DATAFLOW_DERIVED` support facts may also be stored in this
default path. By default the
commands do not launch a semantic worker and report
`semantic_worker: deferred`. When
`REPOGRAMMAR_TYPESCRIPT_WORKER` names an explicit executable, optional
`REPOGRAMMAR_TYPESCRIPT_WORKER_ARGS_JSON` supplies the worker argv vector. The
commands pass the discovered repo-relative TS/JS file set to that worker, record
only worker facts that match the active building-generation code-unit
path/hash/range gate.
They do not store source snippets or absolute paths. The product indexing path
now runs a conservative EC-MVFI-lite family builder before activation. That
builder can write family records only when compatible framework-role candidates
also have strong same-generation `SEMANTIC` or `DATAFLOW_DERIVED` support; the
default syntax-origin framework-role facts and raw parser facts alone still
produce no family rows.
`files` and `units` now read only active file-manifest-only or syntax-only index
metadata and, when present, code-unit records. Pattern-family query commands
return missing-index fallback before an active generation exists, typed
`UNKNOWN` when active family evidence is insufficient, and stored family detail
when EC-MVFI-lite has written supported family rows. Stored family detail now
uses compact/evidence/deep output modes. Compact is the default and omits
evidence records; all modes include a read plan; evidence and deep remain
metadata-first, include default read-plan line ranges when hashes are fresh,
and include source snippets only when `--include-source-spans` is explicitly
requested. `families` uses the active family summary read model plus one bounded
family-evidence projection to hash-verify each distinct evidence path once for
freshness; it does not hydrate full per-family evidence detail. `stats --json` without `--unknowns` uses the active
repo-shape read model and does not load evidence, semantic facts, IR, full
claim snapshots, or family detail. `stats` reports local pattern density,
family support coverage, abstention rate,
thin-wrapper/token-saving risk, readiness/blocking reasons, and estimated
potential read displacement without reporting measured token savings.
`serve` runs the read-only MCP
`repogrammar_context` stdio boundary and reuses the same query preflight and
FamilyStore-backed lookup path. Commands that install or uninstall agent
configuration now support narrow explicit-target live writes after MCP
self-test. The CLI now includes the first Python structural indexing slice, but
Pyrefly/Pyright provider evidence, richer repo-local module resolution, broad
Python family mining beyond the current framework set, React TS/JS support, and
TypeScript compiler-backed analysis remain deferred. Narrow exact-anchor Python
family rows and conservative TS/JS Express/Jest/Vitest/Next/Fastify/Prisma/
Drizzle family rows may exist when EC-MVFI-lite has enough derived support.
Explicit `--include-source-spans` is implemented for bounded hash-checked spans;
default output remains source-free.
Unsupported live target/scope combinations return explicit deferred errors;
dry-run planning remains available for all targets and scopes.
