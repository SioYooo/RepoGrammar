# Storage Specification

RepoGrammar stores repository-derived analysis state inside each analyzed
repository. It does not use a global database for code-derived family facts,
evidence, source hashes, freshness metadata, or repository paths.

The default project state directory is:

```text
.repogrammar/
```

`REPOGRAMMAR_DIR` may override the directory name for a checkout. This is
required for cases such as Windows and WSL sharing one checkout, where SQLite
and daemon locks must not be shared across OS boundaries. RepoGrammar must skip
`.repogrammar/` and `.repogrammar-*` during indexing, watching, file discovery,
token counting, and evidence selection.

## Global State Boundary

Global user state may contain only:

- installed binary and cache metadata;
- agent integration receipts;
- anonymous telemetry preference and anonymous machine id;
- downloaded grammar or runtime artifacts that are not repository-derived;
- global user preferences.

Global user state must not contain:

- source code;
- source-derived family facts;
- evidence text;
- file paths from indexed repositories;
- symbol names;
- raw prompts;
- query text;
- repository-specific SQLite indexes.

## Project State Directory

The repository-local state directory uses this normal mutable-index layout:

```text
.repogrammar/
|-- .gitignore
|-- manifest.json
|-- repogrammar.sqlite
|-- repogrammar.sqlite-wal
|-- repogrammar.sqlite-shm
|-- cache/
|   |-- tree-sitter/
|   |-- semantic-workers/
|   |-- fingerprints/
|   `-- token-counts/
|-- logs/
|   |-- daemon.log
|   |-- index.log
|   |-- mcp.log
|   `-- telemetry.log
|-- locks/
|   |-- index.lock
|   |-- daemon.lock
|   `-- sqlite.lock
|-- telemetry/
|   |-- daily-rollups.jsonl
|   `-- unsent-queue.jsonl
|-- tmp/
`-- receipts/
    |-- init.json
    `-- last-successful-index.json
```

`manifest.json` records lifecycle metadata. `repogrammar.sqlite` is the mutable
repository index database. It stores `index_generations` rows, indexed files,
code units, IR, semantic facts, family rows, evidence, derived-record
dependency metadata, and dirty-record markers for all retained generations; the
active generation is the single row with `status = 'active'`.
Normal `index`, `sync`, `resync`, `status`, `doctor`, `files`, `units`, family
queries, and `prune` operate on this top-level database and do not write a
`.repogrammar/current-generation` pointer or create `.repogrammar/generations/`
directories. Older repositories may still contain `current-generation` and
per-generation SQLite databases under `generations/`; those paths are legacy
read/prune fallback only when the mutable top-level database is absent. `cache/`
contains derived parser, semantic-worker, fingerprint, and token-count caches.
`logs/`, `locks/`, `telemetry/`, `tmp/`, and `receipts/` are repo-local
diagnostic and lifecycle state.
Status and doctor storage inspection must classify this state as `empty`,
`mutable`, `legacy`, or `mutable_with_legacy`, report whether the mutable
database and legacy generation layout are present, and expose WAL/SHM sidecar
byte counts when the mutable database exists. When mutable and legacy layouts
coexist, the mutable database remains authoritative and legacy paths are
diagnostic only.

## Git Hygiene

`repogrammar init` must not dirty the user's tracked working tree by default.
On init, RepoGrammar must add these patterns to `.git/info/exclude` unless they
are already present. Worktree-style `.git` files that point at a Git directory
must be handled without writing tracked files:

```text
.repogrammar/
.repogrammar-*/
```

RepoGrammar must also create `.repogrammar/.gitignore`:

```text
# RepoGrammar local generated state.
# This directory contains repository-local indexes, logs, caches, locks,
# telemetry rollups, and temporary files. Do not commit it.

*
!.gitignore
```

Root `.gitignore` may be modified only when the user passes
`repogrammar init --write-gitignore` or confirms an interactive prompt. If
RepoGrammar modifies root `.gitignore`, it must use a small marker-fenced
section and avoid duplicate entries. An incomplete RepoGrammar marker section
must be refused rather than repaired silently.
`repogrammar doctor` must diagnose missing or invalid generated lifecycle files,
Git exclude patterns, init receipts, and root marker sections without recreating
or rewriting them.
`repogrammar status --json` and `repogrammar doctor --json` must also expose a
source-free readiness hygiene summary for local analysis state. `.repogrammar/`
is RepoGrammar-owned local generated state: report whether it is present,
whether Git ignore policy covers it when safely detectable, and whether any
tracked entries under it are detected. If it is tracked or not covered by ignore
policy, report a low-cardinality cleanup recommendation without printing raw Git
output or file lists.

Foreign provider local state remains foreign. If `.codegraph/` is present, or
tracked entries under it are safely detected, readiness hygiene must report a
`codegraph` foreign provider entry with `managed_by_repogrammar: false`,
`path: ".codegraph/"`, `present`, `tracked_risk`, and a source-free
recommendation to keep it ignored or remove accidental tracked entries.
RepoGrammar must not create, initialize, modify, delete, or own `.codegraph/`.

The bootstrap `init` implementation creates the lifecycle directories,
including `.repogrammar/telemetry/`, `.repogrammar/.gitignore`,
`manifest.json`, and `receipts/init.json`. The current full rebuild path
(`index`, `resync`, and `sync` fallback) creates or migrates
`.repogrammar/repogrammar.sqlite`, inserts a new building
`index_generations` row, stores TS/JS, Python `.py`, Rust self-dogfood, and Go
discovery metadata, plus syntax/code-unit records only for parser-supported
languages, CodeUnit-derived IR nodes, and conservative IR containment edges
under that generation id, validates the
generation, and marks it active while downgrading any previously active row to
validated. Incremental `sync` also creates a new building generation in the
same mutable database, but it computes a path delta from the readable active
generation and first rejects deltas that change parser project-context source
inventories or configuration. When that gate passes, it copy-forwards
unchanged indexed files, code units, IR nodes/edges, and non-derived semantic
fact/evidence rows whose path/hash/language/size still match, reparses added
and modified paths, omits removed paths, recomputes local derived support and
family rows, validates that no dirty or stale dependency rows remain, and then
atomically activates the new generation. Safe source-span
rendering is a query/read-plan concern and is available only through explicit
CLI/MCP opt-in. Telemetry upload queues and sent receipts are created only by
explicit `repogrammar telemetry export/upload` paths. The current storage
adapter has generation-scoped family tables and a FamilyStore port inside the
mutable database. The product full rebuild and incremental sync paths invoke a
conservative EC-MVFI-lite builder before activation, but that builder writes
family rows only when compatible framework-role candidates also have strong
same-generation `SEMANTIC` or `DATAFLOW_DERIVED` support. Current default TS/JS
and Python indexing may populate
semantic-fact/evidence rows with syntax-origin `FRAMEWORK_ROLE` records for
recognized framework-shaped code units; those rows use `FRAMEWORK_HEURISTIC`
certainty and same-generation code-unit evidence, and do not create family rows
by themselves. Default Python indexing may also populate internal semantic
fact/evidence rows from CPython `ast` parse-document structural anchors and
typed `UNKNOWN` facts after Rust-side validation; those rows use `STRUCTURAL`
or `UNKNOWN` certainty, remain unavailable through CLI/MCP query output, are
passed to the current family builder only as context or claim-scoped abstention
inputs, and do not create family rows by themselves. They cannot be support
facts and cannot prove membership. Root `pyproject.toml`, `setup.cfg`, and
`setup.py` may be indexed as `python-config` files and `project_config` code
units; only sanitized `PROJECT_CONFIG`/`STRUCTURAL` metadata or typed project-
config `UNKNOWN` records may be stored, and they are blocked from family-claim
input. `setup.py` is statically parsed with CPython `ast` and never executed.
The CLI can read the active generation for
`files`, `units`, and FamilyStore-backed pattern-family commands.
FamilyStore-backed detail reads can render metadata-only evidence and read
plans, but they still do not render source snippets or absolute paths. Raw
structural IR and semantic-fact read paths remain internal. Active-generation
detail and snapshot reads open the mutable database read-only, select the single
active `index_generations` row, validate the schema and storage health, and
recheck stored repo-relative paths, strict content hashes, languages, unit ids,
byte ranges, IR node/edge references, semantic fact kind/certainty tokens,
assumptions JSON, and same-generation evidence before returning records.
Active reads must not run `PRAGMA integrity_check`: it is a full-database
B-tree verification whose cost scales with the whole database rather than the
result, so it is reserved for generation activation, `inspect`/`doctor`, and
compaction. Full-snapshot reads still run the per-generation structural
violation scans (they are load-bearing for tamper/consistency detection), while
read-model reads run only the lighter schema/required-table/dirty-record gate.
Read-optimized inventory paths such as `stats --json` without `--unknowns`,
`families --json`, and MCP candidate discovery use bounded read-model queries
instead: they open the database read-only and do not run migrations, and they
validate schema/dirty-record state but must not hydrate
full claim snapshots, semantic facts, IR graphs, family evidence, or all family
details. Source freshness for public family detail remains enforced by the
query layer only after a bounded candidate is hydrated.
The query application layer can run an internal file-hash freshness and
claim-input readiness gate over snapshot semantic facts using the current
source-store hash boundary. That gate does not require a storage schema bump,
does not persist family claims, and does not expose semantic facts through
CLI/MCP.

The storage port and SQLite adapter can persist semantic facts together with
repo-relative evidence rows for a building generation, but only when the fact is
already validated against an indexed code unit, matching content hash, and byte
range in that same generation. Default full rebuilds and safe incremental
`sync` still do not run a semantic worker, but they may store syntax-origin
framework-role facts produced from syntax-only code-unit kinds. When
`REPOGRAMMAR_TYPESCRIPT_WORKER` names an explicit worker executable and optional
`REPOGRAMMAR_TYPESCRIPT_WORKER_ARGS_JSON` argv vector, they may record
worker-produced facts through this same-generation gate on the full rebuild
path. Incremental `sync` must fall back to full rebuild when an explicit worker
is configured. Worker unavailable, unsupported-version, timeout, crash, or
protocol-violation results fall back to syntax-only indexing with sanitized
warnings; evidence that conflicts with the building generation's indexed
path/hash/range aborts the new generation.
All generation-scoped writes for indexed files, code units, IR nodes/edges, and
semantic facts require `status = 'building'`; validated, active, or failed
generations are immutable even if a caller still holds an old generation handle.
Within a building generation, recording an indexed file for an unchanged
path/hash/size/language tuple is idempotent. Recording the same path with
changed metadata runs as one SQLite transaction: existing path-scoped code
units, IR, evidence, semantic facts, family memberships, and dependency rows are
removed through foreign-key cascades, and any derived record that depended on
that path is marked dirty before the cascade. The dirty marker is conservative;
it is not cleared by merely inserting the replacement file row, so activation
still requires a future recompute/clear step instead of silently reusing stale
support. Re-deriving a record against the current indexed content (recording the
semantic fact or family evidence again) clears its dirty marker in the same
transaction as the dependency re-derivation, so a generation that replaced or
removed a file mid-build can reach a clean, activatable state instead of being
permanently blocked. Removing an indexed path from a building generation is also
transactional and idempotent for absent paths; when the path exists, the adapter
marks dependent derived records dirty before deleting the indexed-file row and
letting path-scoped rows cascade. Generation validation transitions only
`building` to `validated`, may recheck an already `validated` generation
without changing it, and must not downgrade or reactivate an `active`
generation.
Bootstrap manifest validation parses `manifest.json` as JSON rather than
matching literal text. Field order and formatting are not meaningful, but
`schema_version`, non-empty `repogrammar_version`, `state`, `storage.status`,
and `indexing.status` must match the current bootstrap contract before status,
doctor, init repair, or index preflight treat the repository as initialized.
`repogrammar status --json` reports this manifest value as
`manifest_schema_version`, separately from SQLite's
`storage_schema_version`.

## File Discovery Exclusions

Indexing must respect repository ignore rules and default generated-artifact
exclusions. At minimum, RepoGrammar must skip:

- `.repogrammar/` and `.repogrammar-*`;
- dependency, build, cache, coverage, virtual environment, and generated output
  directories such as `node_modules`, `vendor`, `dist`, `build`, `target`,
  `.venv`, `Pods`, `.next`, and `coverage`;
- files ignored by Git or by nested `.gitignore` rules, including parent
  worktree ignore rules when the project path is below the Git top-level;
- files larger than the configured size limit, with 1 MB as the default limit.

Explicit project configuration may opt in additional files, but ignored
third-party and generated artifacts must not enter family evidence by accident.

The current discovery substrate enforces these defaults for `.ts`, `.tsx`,
`.js`, `.jsx`, `.py`, `.java`, `.cs`, `.c`/`.h`, `.cc`/`.cpp`/`.cxx`/`.hh`/
`.hpp`/`.hxx`, `.go`, `.php`, `.rb`, and `.rs` files (C# discovery skips the MSBuild
`obj/` output directory and stores the `csharp` language token; C/C++ discovery
skips the CLion `cmake-build-debug`/`cmake-build-release` directories and stores
the `c`, `cpp`, and `cpp-config` language tokens; no schema change is required for a new
language token). It returns repo-relative
metadata and skip reasons, and `index`, `resync`, and `sync` store the current discovered file
manifest in the mutable SQLite database under the next building generation id.
Go uses the distinct `go` token for `.go` and `go-config` for root or nested
`go.mod`/`go.work`. Those inventory-only records store only path, strict hash,
size, and token; the indexing loop skips source reads and parsing and therefore
stores no Go code units, IR, facts, or families. A Go-only or empty active
generation is `file_manifest_only`; mixed generations with parser-capable
tokens remain `syntax_only_code_units`. Parser-attempt and `reparsed_files`
counts measure actual parser dispatches, so Go inventory contributes zero. The full rebuild path stores
syntax-only `code_units` containing repo-relative path, language, kind,
start/end byte range, and content hash only for parser-supported discovered
files. Incremental `sync` copy-forwards those records for unchanged active
paths and reparses added or modified paths only when the project-context gate
passes. While `go` and `go-config` are inventory-only and absent from
`ParserProjectContext`, their add/modify/delete deltas remain incremental and
the copy path filters every unit, IR record, fact, derived-support input, and
family associated with current Go inventory paths. The frontend must restore
token-based context invalidation when it adds Go project semantics.

Ruby uses `ruby` for exact `.rb` paths and `ruby-config` for the accepted
root/nested `Gemfile`, `Gemfile.lock`, `gems.rb`, `gems.locked`,
`.ruby-version`, and `.gemspec`-suffix paths. Those tokens follow the same
inventory-only persistence contract: path, strict raw-byte hash, size, and token
only, with zero source-store/parser dispatch and no unit, IR, fact, `UNKNOWN`, or
family. Ruby-only active generations are `file_manifest_only`; mixed generations
remain `syntax_only_code_units`. Ruby inventory deltas remain incremental while
the tokens are absent from `ParserProjectContext`, and copy-forward filters all
claim-bearing records for current Ruby inventory paths. The frontend must add
its Ruby context and restore token-based invalidation before cross-file semantic
records exist.

PHP uses `php` for exact `.php` paths and `php-config` for exact root/nested
`composer.json`, `composer.lock`, `phpunit.xml`, and `phpunit.xml.dist`
basenames. Those tokens persist only path, strict raw-byte hash, size, and token,
with zero source-store/parser dispatch and no unit, IR, fact, `UNKNOWN`, family,
or project-model record. PHP-only active generations are `file_manifest_only`;
mixed generations remain `syntax_only_code_units`. PHP inventory deltas remain
incremental while the tokens are absent from `ParserProjectContext`, and copy-
forward filters claim-bearing records for current PHP inventory paths. Exact
`.composer`/`.phpunit.cache` exclusions are PHP-only; exact `vendor` remains
globally excluded. A later bounded project model must add context invalidation
and apply validated custom vendor/cache exclusions before semantic records
exist.

Swift uses `swift` for exact `.swift` paths and `swift-config` for exact
root/nested `Package.swift`, `Package.resolved`, `.swift-version`, and complete
ASCII `Package@swift-M[.m[.p]].swift` basenames. Those tokens persist only
bounded path, strict raw-byte hash, size, and token, with zero source-store/
parser dispatch and no unit, IR, fact, `UNKNOWN`, family, or project-model
record. Swift-only active generations are `file_manifest_only`; mixed
generations remain `syntax_only_code_units`. Swift inventory deltas remain
incremental while the tokens are absent from `ParserProjectContext`, and copy-
forward filters all claim-bearing records for current Swift inventory paths.
Exact `.build`/`.swiftpm` exclusions are Swift-only and do not globally prune
other languages. A later bounded project model must add context invalidation
before cross-file semantic records exist.

Other source-inventory or config changes still fall back to a full rebuild.
Source snippets and absolute paths are not stored by default syntax-only
`index`/`sync`/`resync` runs; family rows
are stored only by the family builder after same-generation support checks.
Syntax-origin framework-role facts may be stored in semantic-fact/evidence rows
for the same generation, but they remain internal framework-heuristic facts and
must not create family rows, family-bound evidence, or query success by
themselves. Family rows may be created only when the family builder receives
stronger compatible semantic/dataflow support.
File metadata size checks are an optimization, not the safety boundary:
discovery hashing and transient source-store reads must open regular files and
read at most `max_file_bytes + 1` bytes, accepting exactly `max_file_bytes` and
classifying observed limit-plus-one content as oversized without allocating the
full file.
Discovery additionally fails the whole operation at the first aggregate
resource-limit plus-one observation: 100,000 accepted files, 512 MiB accepted
bytes, 100,000 reported skipped paths, 250,000 visited directory entries, or
directory depth 256. The error is path/source-free invalid input. No partial
file manifest is returned, and indexing must not prepare, validate, or activate
a generation after this failure. Exact-limit inputs remain admissible. The
autosync metadata fingerprint uses the same accepted-file, accepted-byte,
visited-entry, and depth budgets before retaining fingerprint records; it has
no skip-report budget because it persists no skipped-path collection.
ADR-0023 additionally requires the future fingerprint walker to enumerate and
open relative to retained directory handles and to derive file type, size, and
modified time from the opened no-follow file handle. The fingerprint remains a
point-in-time change hint, not a snapshot. This handle migration must land with
discovery and source-store migration; the accepted preflight does not change
the current fingerprint implementation or close its tree-swap race.
Semantic-worker-produced facts are stored only when an explicit worker is
configured and the facts pass same-generation evidence validation. Syntax-origin
framework-role facts use the same storage validation path but do not imply that
a TypeScript compiler worker ran.
The active indexed-file and code-unit rows are exposed through the limited
`files`/`units` CLI read path and must not be treated as family evidence. Active
`units` reads must revalidate stored code-unit ids against their repo-relative
paths before rendering output so tampered generation databases cannot smuggle
absolute paths through unit ids. Active semantic-fact reads are internal and
must likewise revalidate fact/evidence rows before any future family builder
consumes them.

Python discovery is now part of the official v0.1 substrate for `.py` files. It
records language provenance and source hashes with the same repo-relative
storage discipline as the existing TS/JS substrate. Python support level,
repo-local module/import facts, provider provenance, and richer freshness
metadata remain deferred until provider-backed evidence is implemented.
Optional provider facts, including future CodeGraph-derived facts, must carry
provider provenance and freshness metadata before they can participate in family
evidence.

## Project Configuration

Optional shared project configuration lives at the repository root:

```text
repogrammar.json
```

The file may configure language enablement, custom file extensions,
include/exclude patterns, framework adapters, family thresholds, and telemetry
inheritance. Missing configuration means zero-config defaults. Malformed
configuration must produce a warning and fall back to safe defaults; it must not
crash indexing.

Local-only configuration belongs under `.repogrammar/` or global user config,
not in `repogrammar.json`.

## Manifest and Freshness

`.repogrammar/manifest.json` must include enough metadata to decide whether
family claims are fresh for the current repository state. It must include at
least:

```json
{
  "schema_version": 1,
  "repogrammar_version": "0.2.2",
  "created_at": "2026-06-24T00:00:00Z",
  "last_indexed_at": "2026-06-24T00:00:00Z",
  "repository": {
    "root_canonical_hash": "sha256:...",
    "git_head": "abc123",
    "worktree_hash": "sha256:...",
    "dirty": true
  },
  "storage": {
    "database": "repogrammar.sqlite",
    "journal_mode": "wal",
    "active_generation": "gen-000002"
  },
  "languages": {
    "typescript": {
      "status": "supported",
      "syntax_frontend": "tree-sitter",
      "semantic_worker": "typescript",
      "semantic_worker_version": "6.x"
    },
    "python": {
      "status": "planned"
    }
  },
  "index": {
    "files_indexed": 4812,
    "code_units": 36881,
    "families": 624,
    "unknown_facts": 203,
    "abstained_groups": 91
  }
}
```

Database source paths must be repository-relative. Manifest and telemetry may
store a hash of the canonical repository root, but telemetry must not upload the
raw path. Every family and evidence row must carry generation id; every evidence
row must carry file content hash, source range, and schema-backed
`covered_claims` labels from the allowlist `canonical`, `support`, `variation`,
and `exception`. Full repository revision metadata for family evidence remains
deferred until repository/worktree freshness metadata is implemented.
Repository-relative storage paths are lexical, slash-separated, non-empty
paths. They must reject absolute paths, Windows drive prefixes, backslashes,
URI-like text, control characters, `.`/`..` traversal segments, and empty path
segments before activation or readback.
Unknown facts must retain reason code, affected claim, freshness status, and
recovery guidance where applicable.

## SQLite Responsibilities

RepoGrammar uses repository-local SQLite databases. SQLite and SQL migration
logic belong only in persistence adapters. The current substrate uses the
top-level `.repogrammar/repogrammar.sqlite` database as the normal mutable index
store. Each rebuild inserts a new `index_generations` row, generation-scoped
tables carry that `generation_id`, and active reads select the single active
generation row. Legacy per-generation databases under
`.repogrammar/generations/<generation>/` and `.repogrammar/current-generation`
are read/prune fallback only when the mutable database is absent. Current CLI
inventory reads such as `files`, `units`, and internal semantic-fact inventory
use targeted active-generation queries and expose only repo-relative metadata,
code-unit rows, or validated fact metadata needed by that command. They must
not load the full active claim-input snapshot merely to count files, list code
units, or list fact inventory. The internal claim-input snapshot uses the same
active generation and validation rules, but remains reserved for family and
freshness gates and unavailable through CLI/MCP.

Schema migrations are versioned by `schema_migrations`. Before writing a new
generation, the adapter must refuse to open a database whose stored maximum
schema version is newer than the running build supports, so an older binary
cannot corrupt an index written by a newer one. A stored version *older* than
the current build is handled explicitly rather than silently reused: read paths
return a typed schema-outdated error whose recovery is `run repogrammar resync`
(routed through the recovery classifier vocabulary, `RecoveryAction::Resync`),
and the read leaves the active generation untouched. Because the DDL batch is
`CREATE TABLE IF NOT EXISTS` and cannot add columns to an existing table, the
full-rebuild path (`init`, `index`, `resync`) recreates the repo-local mutable
database when the stored version is older than current: it deletes only the
`repogrammar.sqlite` file and its `-wal`/`-shm` sidecars under `.repogrammar`,
then rebuilds a fresh current-version database. Nothing outside the mutable
database is removed.

Required PRAGMAs:

```text
PRAGMA journal_mode=WAL;
PRAGMA synchronous=NORMAL;
PRAGMA foreign_keys=ON;
PRAGMA busy_timeout=5000;
PRAGMA temp_store=MEMORY;
```

After successful generation activation and after mutating mutable-database
retention, the SQLite adapter runs bounded post-commit maintenance with
`PRAGMA optimize` and `PRAGMA wal_checkpoint(PASSIVE)`. This happens outside
the write transaction after the committed state is durable, so readers never see
partial writes and a passive checkpoint reports reader contention instead of
blocking them. Normal `index`, `sync`, `resync`, and `prune` must not run
automatic blocking `VACUUM`.

Full database compaction is available only through `repogrammar compact`. The
command uses the repository-local index lock before dry-run or mutating
compaction. `compact --dry-run` opens the mutable database read-only, validates
the active generation and storage sidecars, and reports database, WAL, SHM, and
total byte counts without mutating the database. `compact --yes` repeats the
active-generation validation, runs explicit SQLite `VACUUM` against
`.repogrammar/repogrammar.sqlite`, requires a successful truncating WAL
checkpoint, then reports before/after size metadata. Compact must refuse missing
mutable storage, dirty active records, malformed sidecar paths, busy WAL
checkpoint state, and unhealthy repository-local state. It must not remove
source files, user files, or legacy generation directories.

`repogrammar storage clean` composes the safe maintenance operations intended
for users who want to finish the mutable-storage migration and minimize local
disk use. It first verifies that the top-level mutable SQLite database is
present and authoritative. Only then may it remove diagnostic legacy paths
`.repogrammar/current-generation` and `.repogrammar/generations/`, prune all
inactive mutable generations with `keep_inactive = 0`, and run explicit
compaction. `storage clean --dry-run` reports legacy-layout bytes, prune
candidates, compact size metadata, total before/after bytes, and reclaimed
bytes without mutating storage. A mutating run requires `--yes`. Legacy-only
repositories must be refused rather than deleted; users and agents should run
`repogrammar resync` first to create mutable SQLite storage.

The initial schema stores schema metadata, generation rows, indexed files,
syntax-only code-unit records, IR nodes and edges, semantic facts, families,
family members, variation slots, evidence links, derived-record dependency
rows, and dirty-record markers. The full rebuild command path populates indexed
files, syntax-only code units, CodeUnit-derived IR nodes, conservative IR
containment edges, optional semantic-worker facts, and exact-anchor
`DATAFLOW_DERIVED` support facts derived in the application layer. Incremental
`sync` copy-forwards unchanged base records into a new building generation,
reparses changed paths, omits removed paths, and recomputes derived support and
family records before validation. The storage ports also expose semantic-fact
and family evidence writers for frontend and claim-builder integration. These
writers accept only building generations and require evidence to match an
indexed code unit's repository-relative path, content hash, and byte range in
the same generation. They also write `derived_record_dependencies` rows for
semantic facts, semantic evidence, family evidence, and the supported family
record that depends on each family-evidence path/hash. Family evidence rows must
be linked to a family and must declare covered family-claim labels explicitly;
semantic fact evidence rows must remain unlinked to a family. Generation
validation must reject malformed semantic or family evidence rows, mismatched
derived dependencies, or active dirty records before activation. The storage
adapter enforces foreign keys, repo-relative paths at the Rust port boundary,
matching file/code-unit content hashes, code-unit byte ranges bounded by indexed
file size, IR node references to same-generation code units, IR edge references
to same-generation IR nodes, family/member/slot/evidence generation binding,
family-evidence presence for every emitted family (all four prevalence
classifications are evidence-backed), and validation before activation.
The lifecycle report's `dirty_records_cleared` count covers persisted dirty
marker rows actually cleared in the building generation. Incremental
generation-by-replacement omission of claim-bearing records is a copy-forward
filter, not dirty-marker cleanup, so purging legacy Go, PHP, Ruby, or Swift
claims from inventory-only paths leaves that count at zero.
Path replacement in a building generation is transactional and fail-closed:
unchanged file metadata is treated as an idempotent no-op, while changed file
metadata marks dependent derived records dirty before replacing the file row and
letting path-scoped rows cascade. Dirty markers intentionally block activation
until a later bounded recomputation path can remove or rebuild the affected
derived records. Path removal follows the same fail-closed rule: absent paths
are a no-op, existing paths are deleted through the indexed-file row, and any
derived records that depended on the removed path are marked dirty before the
cascade.
The current storage schema version is `9`. Existing pre-release schema `1`
through `8` generation databases are treated as stale: reads refuse them with a
typed schema-outdated error recommending `repogrammar resync`, and the
full-rebuild path recreates the mutable database rather than upgrading it in
place.
Schema `9` adds the `family_constraint_profiles` table, which persists one
`FamilyConstraintProfile` per family (see the domain model). Each row is
family-keyed and generation-scoped with `PRIMARY KEY (generation_id, family_id)`
and cascades from both `index_generations` and `families`:

- `generation_id` `TEXT NOT NULL`;
- `family_id` `TEXT NOT NULL`;
- `profile_json` `TEXT NOT NULL` (`CHECK <> ''`) — a validated, deterministic,
  source-free structured column. The adapter serializes the typed profile
  (required-equal features, allowed variations, prohibited/blocking features, and
  unresolved obligations) with a stored `version` tag; hydration re-validates
  every value (source-free, known origin/class/reason tokens, matching version)
  and rejects any malformed row. Object keys serialize in sorted order, so the
  encoding is deterministic. The production indexing pipeline persists profiles
  in a later slice, so a valid generation may legitimately carry no rows yet.

Schema `8` replaces the legacy classification vocabulary with the four
prevalence classifications and adds the `FamilyPrevalence` columns to the
`families` table:

- `classification` `CHECK` now accepts exactly `DOMINANT_PATTERN`,
  `SUPPORTED_PATTERN`, `MINORITY_PATTERN`, and `UNKNOWN_PREVALENCE`.
- `eligible_peer_count`, `supported_member_count`,
  `competing_ready_family_count`, `largest_competing_support`,
  `blocked_peer_count`, and `unsupported_peer_count` are `INTEGER NOT NULL`
  (each `CHECK >= 0`).
- `coverage_ratio` is a nullable `REAL`.
- `classification_reason` is `TEXT NOT NULL` (`CHECK <> ''`).

Schema `7` adds bounded read-path indexes for agent-loop queries:

- `idx_evidence_generation_family_order` on
  `(generation_id, family_id, path, start_byte, end_byte, code_unit_id,
  evidence_id)`;
- `idx_family_members_generation_code_unit` on
  `(generation_id, code_unit_id, family_id)`;
- `idx_evidence_generation_path_family` on
  `(generation_id, path, family_id, start_byte, end_byte, evidence_id)`.

The SQLite migration path is idempotent and runs `PRAGMA optimize` after
creating or confirming these indexes. Query-plan tests must prove family
evidence, member lookup, and path candidate queries use the intended indexes.
The database must later store repository revision, worktree hash, language
adapter versions, freshness metadata, canonical templates, exception records,
and richer provenance once those producers exist. Searchable source evidence
may use FTS5 where useful after source-snippet retention rules are finalized.

Future storage-size optimization should remain a SQLite schema and access-path
problem, not a database-engine replacement. Candidate schema changes include
normalizing repeated low-cardinality strings into dictionary tables, packing
enum-like reason/mechanism/certainty fields behind stable integer ids,
deduplicating repeated JSON assumption shapes where that preserves the public
source-free contract, and adding covering indexes for targeted active-read
paths before adding broader materialized snapshots. Any such change requires a
schema bump, migration or rebuild policy, storage-size benchmarks on real
repositories, and query regression tests proving that fail-closed `UNKNOWN`,
path/hash validation, and source-free output are unchanged.

Schema work should keep separate tables for:

- `schema_migrations`;
- `index_generations`;
- repository metadata;
- indexed files and content hashes;
- code units and source ranges;
- unified IR summaries;
- fingerprints and candidate groups;
- pattern families and templates;
- family constraint profiles;
- variations, exceptions, and counterexamples;
- derived record dependencies and dirty-record markers;
- source evidence.

The production SQLite dependency is `rusqlite` with bundled SQLite enabled.
Only `src/rust/adapters/persistence/` may depend on it directly; application and
domain code must use RepoGrammar-owned storage port types.

`repogrammar status` must show journal mode when an active generation exists
and must distinguish the bootstrap manifest schema from the active SQLite
storage schema in both human and JSON output. Status should also report storage
layout, mutable database presence, legacy generation layout presence, mutable
WAL/SHM sidecar byte counts, and active derived dependency and dirty-record
counts when storage can be inspected.
`repogrammar doctor` must run SQLite integrity checks, verify schema version,
verify active generation consistency, report missing storage layout without
recreating it, report legacy-only or mixed mutable-plus-legacy layouts, and
report lock state. Doctor JSON must distinguish
`checks.manifest_schema_version` from `checks.storage_schema_version` and must
not emit an ambiguous `checks.schema_version` field. Doctor JSON should expose
the same layout, sidecar, dependency, and dirty-record counts under `checks` for
reviewer diagnostics.

## Index Generations

Repository initialization and indexing must build a new index generation without
overwriting the previous valid index. The new generation becomes active only
after persistence validation succeeds. Cancellation or failure must preserve the
previous valid generation.

Full rebuilds must write to a new generation and atomically activate it only
after validation. All family and evidence records must bind to a
`generation_id`. MCP serving must not write source files or family/index
content; it may rely on the same idempotent SQLite schema/index migration path
used by read-model queries when an initialized mutable database predates the
current storage schema. Indexing is the only writer of repository analysis
records.

### Generation Write Session

A build persists a generation through a single **generation write session**, not
through one connection per record. The session is opened once against the
`building` generation, owns exactly one writable SQLite connection with the
write pragmas applied a single time, and routes every `record_*` call through
bounded-batch transactions on that connection. The build finishes the session
(commit and seal) before the generation is validated and activated through the
unchanged `building -> validated -> active` state machine, so validation and
activation always observe fully committed data on their own connections. The
granular per-record store methods remain for tests and the storage boundary and
delegate to one one-shot session each; production builds open exactly one
session for the whole build.

- **Transaction boundaries.** Writes accumulate in a batch opened with
  `BEGIN IMMEDIATE`; the batch commits when it reaches a bounded row capacity
  (2000 rows) and at explicit pipeline phase checkpoints. Both production build
  pipelines checkpoint after the file/code-unit/IR write phase, after the
  semantic-fact phase, and after the family phase; each checkpoint commits the
  open batch and passively checkpoints the write-ahead log. `BEGIN IMMEDIATE`
  takes the write lock up front, and the session re-reads the generation status
  under that lock at every batch open, so a status flip landing between batches
  is rejected before the next write. The row-capacity bound targets transaction
  size and lock-hold time rather than partial-work durability: a `building`
  generation is never readable and never resumed, so a crash discards the open
  batch and the next build supersedes the leftover.
- **Referential validation.** Each record reproduces the field-level and
  referential checks of the historical per-record path, reading referenced
  files, code units, and families on the session's own connection. That
  connection sees both committed batches and the current open batch, so the
  checks are at least as strong as the previous per-record reads against the
  committed database. Statements are issued directly (`execute`/`query_row`);
  per-statement prepared-statement caching is a deferred optimization gated on
  enabling the SQLite driver's statement-cache feature and is not required for
  the single-connection win.
- **Terminal `failed` status.** The `failed` stamp is written only for a build
  abandoned **before the session is sealed** and only when it already committed
  at least one batch — a record error propagated out of the pipeline while the
  session is still open, an explicit `abandon`, or a dropped unsealed session —
  which rolls back the open batch and stamps the generation `failed`. A session
  that committed nothing (for example a lone field- or referential-validation
  rejection through a granular store method) rolls back and leaves a pristine,
  reusable `building` row instead of stamping `failed`. Errors raised **after**
  the session is sealed — a generation-validation rejection or an activation
  failure — do not stamp anything: the fully committed generation stays
  `building` (unchanged from the historical behavior) and is reclaimed by prune,
  exactly like the previous active generation which remains readable throughout.
  The schema already permits `failed`, validation refuses it, and retention
  deletes any non-active generation, so both a `failed` and a leftover
  `building` row are inert and reclaimable. Finishing a session after it was
  abandoned (or finishing twice) is a typed error, so a build that gave up on
  one path can never silently seal and activate on another.
- **Validation once per activation.** Activation validates the generation
  exactly once. The pipeline validates immediately before activating under the
  held index lock, leaving the generation `validated`; activation then accepts
  that status without re-running the whole-database integrity check, so the
  full-database `PRAGMA integrity_check` runs once per sync rather than twice.
  Only a not-yet-validated (`building`) generation handed straight to activation
  is validated inline. The generation-scoped violation scans — family evidence,
  semantic evidence, derived dependencies, dirty records, the IR graph, and a
  code-unit/indexed-file conformance scan that re-proves every code unit's hash
  and byte range against its file — run during validation and make the
  activation gate a strict superset of per-record enforcement.
- **WAL behavior and maintenance.** Batch commits alone do not truncate the
  write-ahead log, so WAL growth is bounded by the phase checkpoints — each runs
  a passive `wal_checkpoint`, holding the WAL to roughly one phase rather than
  the whole build. Sealing runs `PRAGMA optimize` plus a passive WAL checkpoint
  once for a whole-build `finish`; it is sealed **before** that best-effort
  maintenance so a transient maintenance failure over already-committed rows can
  never fail or `failed`-stamp the build. The granular one-shot record methods
  run no post-commit maintenance (matching the historical per-record path), with
  the sole exception of `remove_indexed_file`, which retains its historical
  maintenance.

Generation retention may remove inactive generation rows after an active
generation is readable and storage health checks pass. The default retention
policy is active plus the newest 2 inactive generations; CLI callers may
override the inactive count with `--keep <n>`. Retention must never remove the
active generation row, must rely on foreign-key cascades for generation-scoped
records, and must recheck active generation state before destructive deletion.
Successful mutable-database deletion runs the same bounded post-commit
`PRAGMA optimize` and passive WAL checkpoint as index activation, but it must
not run automatic `VACUUM`.
If no active generation is readable, the active row is corrupt, or the active
generation changes during pruning, retention must fail without deleting. When
only the legacy directory layout exists, retention may remove inactive
generation directories after the same health checks; that fallback must refuse
missing or corrupt active-generation pointers, symlinked generation directories,
and generation entries that are not directories. A dry run must report the same
candidates without mutating storage.

## Logs

RepoGrammar uses repo-local diagnostic logs:

```text
.repogrammar/logs/daemon.log
.repogrammar/logs/index.log
.repogrammar/logs/mcp.log
.repogrammar/logs/telemetry.log
```

Logs must not contain source snippets, raw prompts, secrets, environment
variables, raw error dumps, or unredacted absolute paths by default.
Repo-local logs may include repo-relative paths; telemetry must not upload them.
`repogrammar logs` reads these files through a bounded tail interface, defaults
to `daemon.log`, redacts by default, and returns clean unavailable reports for
missing, malformed, symlinked, or unreadable log files. `--since` is accepted
for contract stability but may return the bounded tail with an unsupported
filtering message until duration filtering is implemented.

Supported log levels are `error`, `warn`, `info`, `debug`, and `trace`.
`debug` and `trace` must not be enabled by default. Logs must rotate with a
default maximum size of 10 MB per file and 5 retained files. `doctor --bundle`
may generate a diagnostic bundle, but it must be redacted by default.

Telemetry off does not disable local diagnostic logs. Local logs and telemetry
are separate controls.

## Locks and Unlock

RepoGrammar uses explicit lock files under `.repogrammar/locks/`. An index lock
is acquired before discovery, source reads, generation preparation, validation,
and activation, so the current lock metadata does not include a generation id
yet. The lock must contain:

```json
{
  "kind": "index",
  "pid": 12345,
  "host": "workstation-name",
  "os": "darwin",
  "started_unix_seconds": 1782200000,
  "repogrammar_version": "0.2.2",
  "token": "12345-1782200000000000000-1"
}
```

`host` may be `null` when no local host identifier is available. The current
`index`, `sync`, and `resync` implementation creates `index.lock` before
discovery, preferably by writing the complete metadata to a temporary file and
publishing it atomically into place. On filesystems where that publish step is
unavailable, it falls back to create-new semantics and still removes a partial
`index.lock` if metadata writing fails. The lock is held through validation and
active-generation pointer update, and successful runs remove only the lock
content they wrote. Stale-lock replacement also requires the lock bytes to
match the inspected stale bytes before deletion, so a concurrently replaced
lock is rechecked instead of removed. A live same-host lock is refused. A lock
whose same-host process is confirmed dead may be replaced during acquisition;
same-host process checks must use native liveness probes on Windows as well as
Unix so a dead nonzero PID can become a confirmed stale lock. On Unix, when the
platform can expose a live process start time, a same-host process that started
after the lock's `started_unix_seconds` is treated as PID reuse rather than as
the lock owner; if the start time cannot be inspected, the lock remains
conservatively live or unknown instead of being deleted. PID values that cannot
represent a positive process id on the current OS must not be passed to the
native probe as a live owner. Malformed, cross-host, cross-OS, or otherwise
unknown locks are refused and surfaced by `doctor`.
Index lock classification, autosync daemon-lock inspection, and repository
readiness must consume the same application liveness policy instead of
reimplementing PID checks independently.

`repogrammar unlock` must not be a blind delete command. It must:

- check that the lock file exists;
- check whether the recorded process is still alive;
- check whether the lock belongs to the same OS and host;
- try to acquire an advisory lock;
- delete only a confirmed stale `index.lock`;
- refuse to delete active, unknown, invalid, daemon, or SQLite locks;
- output the reason a lock was removed or refused;
- support `--force` only with explicit confirmation;
- run or recommend `repogrammar doctor --storage` after removal.

If RepoGrammar detects a network filesystem, Windows/WSL shared mount, or
unsupported locking behavior, it must warn and recommend a separate
`REPOGRAMMAR_DIR` such as `.repogrammar-linux` or `.repogrammar-win`.

## Freshness and Sync

Every query must check index freshness against the current repository state. If
the index is missing, RepoGrammar must return clean fallback guidance rather
than failing noisily:

```text
FALLBACK_TO_CODE_SEARCH
reason: repository is not initialized
guidance: run repogrammar init --yes
```

If repository-local state exists but the active generation is missing or stale,
RepoGrammar should guide users and agents to run `repogrammar resync`. If the
index is stale, RepoGrammar must either return a stale warning or refuse family
claims whose evidence changed.

The current implementation performs only an internal semantic-fact
claim-input readiness check over the active claim-input snapshot against current
source content hashes. Changed or missing source blocks the affected semantic
fact with typed `StaleEvidence`.
Repository-revision, worktree-wide, and persisted family freshness remain
deferred.

Auto-sync is optional. The baseline remains explicit repo-local bootstrap,
freshness warnings in `status`, and freshness checks before MCP claims. For
first setup, users and agents may run `repogrammar init`; agents may add
`--yes`, and either may add `--autosync` when auto-sync should start after the
first active index succeeds.
When enabled with `repogrammar autosync start`, RepoGrammar stores
`.repogrammar/autosync.json`, uses `.repogrammar/locks/daemon.lock` for the
running worker, and writes diagnostics to `.repogrammar/logs/daemon.log`.
Daemon lock acquisition writes complete lock metadata to a temporary file and
publishes it atomically when supported, falls back to create-new semantics when
needed, and removes stale daemon locks only when the on-disk bytes still match
the inspected stale record. A start attempt also passes a bounded, non-secret
hexadecimal startup nonce to the child. The child writes that nonce into
`daemon.lock` only after acquiring the lock. The parent reports `running: true`
only after a bounded poll observes the expected child PID and nonce in the lock
and separately confirms that the spawned child is still alive. Process creation
or PID allocation alone is not readiness. Immediate child exit, an active or
unreadable conflicting lock, and readiness timeout are typed failures; polling
is finite and failure output must not expose the nonce, environment values,
credentials, or internal paths.

Outside that startup handshake, a daemon lock is considered running only when
the recorded PID is live and, on Unix, its command line can be confirmed as
`repogrammar autosync run`; PID existence alone is not enough to signal or
preserve the lock as active in either `autosync status` or repository
readiness. Start acquisition, stop, disable, and stale-lock replacement first
acquire one short-lived create-new lifecycle record. While that record is held,
no cooperating successor can publish a daemon lock; cleanup additionally
requires the daemon-lock bytes to equal the inspected or owned record. A failed
stop signal preserves the owner lock and returns an error, while successful
stop waits only for a bounded shutdown interval. The current worker detects changed lightweight
supported-file metadata fingerprints and calls the normal `sync` path after a
debounce interval. The detector is only a low-cost change trigger; `sync`
remains responsible for content hashes, Git-ignore enforcement, parsing,
semantic facts, full-rebuild fallback decisions, and atomic generation
activation. The daemon must recheck its repository-local state preconditions
while running; if required lifecycle state becomes unavailable, it records a
single stop reason and exits instead of retrying forever. Consecutive identical
sync failures must be summarized by repeat count in `daemon.log`, with a
transition, recovery, or terminal summary when the error changes, a sync
succeeds, or the daemon stops.

## Migration Strategy

Migration execution logic belongs under `src/rust/adapters/persistence/`.
Migrations must be deterministic, versioned, tested, and documented before
storage is implemented.

## Non-goals

RepoGrammar does not use a vector database in the first version. Embeddings are
not part of the bootstrap architecture.
