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

The bootstrap `init` implementation creates the lifecycle directories,
including `.repogrammar/telemetry/`, `.repogrammar/.gitignore`,
`manifest.json`, and `receipts/init.json`. The current `index` and `sync`
implementation creates or migrates `.repogrammar/repogrammar.sqlite`, inserts a
new building `index_generations` row, stores TS/JS and Python `.py` discovery
metadata plus syntax-only code-unit records, CodeUnit-derived IR nodes, and
conservative IR containment edges under that generation id, validates the
generation, and marks it active while downgrading any previously active row to
validated. Safe source-span rendering is a query/read-plan concern and is
available only through explicit CLI/MCP opt-in. Telemetry upload queues and
sent receipts are created only by explicit `repogrammar telemetry export/upload`
paths. The current storage adapter has generation-scoped family tables and a
FamilyStore port inside the mutable database. The product `index` and `sync`
path now invokes a conservative EC-MVFI-lite builder before activation,
but that builder writes family rows only when compatible framework-role
candidates also have strong same-generation `SEMANTIC` or `DATAFLOW_DERIVED`
support. Current default TS/JS and Python indexing may populate
semantic-fact/evidence rows with syntax-origin `FRAMEWORK_ROLE` records for
recognized framework-shaped code units; those rows use `FRAMEWORK_HEURISTIC`
certainty and same-generation code-unit evidence, and do not create family rows
by themselves. Default Python indexing may also populate internal semantic
fact/evidence rows from CPython `ast` parse-document structural anchors and
typed `UNKNOWN` facts after Rust-side validation; those rows use `STRUCTURAL`
or `UNKNOWN` certainty, remain unavailable through CLI/MCP query output, are
passed to the current family builder only as context or claim-scoped abstention
inputs, and do not create family rows by themselves. They cannot be support
facts and cannot prove membership. Root `pyproject.toml` may be indexed as a
`python-config` file and
`project_config` code unit; only sanitized `PROJECT_CONFIG`/`STRUCTURAL`
metadata or typed project-config `UNKNOWN` records may be stored, and they are
blocked from family-claim input. The CLI can read the active generation for
`files`, `units`, and FamilyStore-backed pattern-family commands.
FamilyStore-backed reads can render metadata-only evidence and read plans, but
they still do not render source snippets or absolute paths. Raw structural IR
and semantic-fact read paths remain internal. Active-generation reads open the
mutable database read-only, select the single active `index_generations` row,
validate the schema and storage health, and recheck stored repo-relative paths,
strict content hashes, languages, unit ids, byte ranges, IR node/edge
references, semantic fact kind/certainty tokens, assumptions JSON, and
same-generation evidence before returning records. They also reject active
dirty records and derived-record dependency rows whose stored path/hash no
longer matches the active indexed-file row, so dirty or dependency-mismatched
records cannot support family or semantic-fact output.
The query application layer can run an internal file-hash freshness and
claim-input readiness gate over snapshot semantic facts using the current
source-store hash boundary. That gate does not require a storage schema bump,
does not persist family claims, and does not expose semantic facts through
CLI/MCP.

The storage port and SQLite adapter can persist semantic facts together with
repo-relative evidence rows for a building generation, but only when the fact is
already validated against an indexed code unit, matching content hash, and byte
range in that same generation. Default `index` and `sync` still do not run a
semantic worker, but they may store syntax-origin framework-role facts produced
from syntax-only code-unit kinds. When `REPOGRAMMAR_TYPESCRIPT_WORKER` names an
explicit worker executable and optional
`REPOGRAMMAR_TYPESCRIPT_WORKER_ARGS_JSON` argv vector, they may record
worker-produced facts through this same-generation gate. Worker unavailable,
unsupported-version, timeout, crash, or protocol-violation results fall back to
syntax-only indexing with sanitized warnings; evidence that conflicts with the
building generation's indexed path/hash/range aborts the new generation.
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
support. Removing an indexed path from a building generation is also
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
`.js`, `.jsx`, and `.py` files. It returns repo-relative metadata and skip
reasons, and `index`/`sync` store the discovered file manifest in the mutable
SQLite database under the building generation id. The current index path also
stores syntax-only `code_units` containing repo-relative path, language, kind,
start/end byte range, and content hash. Source snippets, absolute paths,
families, and pattern-family evidence are not stored by default syntax-only
`index`/`sync` runs.
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
  "repogrammar_version": "0.1.0",
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
`files` and `units` reads use the validated active generation and expose only
repo-relative metadata and code-unit rows. The internal claim-input snapshot uses
the same active generation and validation rules, but remains unavailable through
CLI/MCP.

Required PRAGMAs:

```text
PRAGMA journal_mode=WAL;
PRAGMA synchronous=NORMAL;
PRAGMA foreign_keys=ON;
PRAGMA busy_timeout=5000;
PRAGMA temp_store=MEMORY;
```

The initial schema stores schema metadata, generation rows, indexed files,
syntax-only code-unit records, IR nodes and edges, semantic facts, families,
family members, variation slots, evidence links, derived-record dependency
rows, and dirty-record markers. The current `index` and `sync` command path
populates indexed files, syntax-only code units,
CodeUnit-derived IR nodes, conservative IR containment edges, optional
semantic-worker facts, and exact-anchor Python `DATAFLOW_DERIVED` support facts
derived in the application layer. The storage ports also expose semantic-fact
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
family-evidence presence for non-`UNKNOWN` family classifications, and
validation before activation.
Path replacement in a building generation is transactional and fail-closed:
unchanged file metadata is treated as an idempotent no-op, while changed file
metadata marks dependent derived records dirty before replacing the file row and
letting path-scoped rows cascade. Dirty markers intentionally block activation
until a later bounded recomputation path can remove or rebuild the affected
derived records. Path removal follows the same fail-closed rule: absent paths
are a no-op, existing paths are deleted through the indexed-file row, and any
derived records that depended on the removed path are marked dirty before the
cascade.
The current storage schema version is `6`. Existing pre-release schema `1`,
`2`, `3`, `4`, and `5` generation databases are treated as stale and must be
rebuilt rather than silently upgraded in place.
The database must later store repository revision, worktree hash, language
adapter versions, freshness metadata, canonical templates, exception records,
and richer provenance once those producers exist. Searchable source evidence
may use FTS5 where useful after source-snippet retention rules are finalized.

Schema work should keep separate tables for:

- `schema_migrations`;
- `index_generations`;
- repository metadata;
- indexed files and content hashes;
- code units and source ranges;
- unified IR summaries;
- fingerprints and candidate groups;
- pattern families and templates;
- variations, exceptions, and counterexamples;
- derived record dependencies and dirty-record markers;
- source evidence.

The production SQLite dependency is `rusqlite` with bundled SQLite enabled.
Only `src/rust/adapters/persistence/` may depend on it directly; application and
domain code must use RepoGrammar-owned storage port types.

`repogrammar status` must show journal mode when an active generation exists
and must distinguish the bootstrap manifest schema from the active SQLite
storage schema in both human and JSON output. Status should also report active
derived dependency and dirty-record counts when storage can be inspected.
`repogrammar doctor` must run SQLite integrity checks, verify schema version,
verify active generation consistency, report missing storage layout without
recreating it, and report lock state. Doctor JSON must distinguish
`checks.manifest_schema_version` from `checks.storage_schema_version` and must
not emit an ambiguous `checks.schema_version` field. Doctor JSON should expose
the same dependency and dirty-record counts under `checks` for reviewer
diagnostics.

## Index Generations

Repository initialization and indexing must build a new index generation without
overwriting the previous valid index. The new generation becomes active only
after persistence validation succeeds. Cancellation or failure must preserve the
previous valid generation.

Full rebuilds must write to a new generation and atomically activate it only
after validation. All family and evidence records must bind to a
`generation_id`. MCP serving should open the active database read-only where
possible. Indexing is the only writer.

Generation retention may remove inactive generation rows after an active
generation is readable and storage health checks pass. The default retention
policy is active plus the newest 2 inactive generations; CLI callers may
override the inactive count with `--keep <n>`. Retention must never remove the
active generation row, must rely on foreign-key cascades for generation-scoped
records, and must recheck active generation state before destructive deletion.
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
  "repogrammar_version": "0.1.0",
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
Unix so a dead nonzero PID can become a confirmed stale lock. PID values that
cannot represent a positive process id on the current OS must not be passed to
the native probe as a live owner. Malformed, cross-host, cross-OS, or otherwise
unknown locks are refused and surfaced by `doctor`.

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
first setup, users and agents may run `repogrammar init --yes --resync --autosync`.
When enabled with `repogrammar autosync start`, RepoGrammar stores
`.repogrammar/autosync.json`, uses `.repogrammar/locks/daemon.lock` for the
running worker, and writes diagnostics to `.repogrammar/logs/daemon.log`.
Daemon lock acquisition writes complete lock metadata to a temporary file and
publishes it atomically when supported, falls back to create-new semantics when
needed, and removes stale daemon locks only when the on-disk bytes still match
the inspected stale record. The current worker detects
changed lightweight supported-file metadata fingerprints and calls the existing
full `sync` path after a debounce interval. The detector is only a low-cost
change trigger; the full sync remains responsible for content hashes,
Git-ignore enforcement, parsing, semantic facts, and atomic generation
activation. Incremental changed-unit reparsing, affected-family stale marking,
and lazy query-time recomputation remain future work.

## Migration Strategy

Migration execution logic belongs under `src/rust/adapters/persistence/`.
Migrations must be deterministic, versioned, tested, and documented before
storage is implemented.

## Non-goals

RepoGrammar does not use a vector database in the first version. Embeddings are
not part of the bootstrap architecture.
