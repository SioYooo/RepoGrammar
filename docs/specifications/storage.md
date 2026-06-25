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

The repository-local state directory should use this layout once storage is
implemented:

```text
.repogrammar/
|-- .gitignore
|-- manifest.json
|-- repogrammar.sqlite
|-- repogrammar.sqlite-wal
|-- repogrammar.sqlite-shm
|-- current-generation
|-- generations/
|   |-- gen-000001/
|   |   |-- repogrammar.sqlite
|   |   `-- manifest.json
|   `-- gen-000002/
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

`manifest.json` records the active index metadata. `repogrammar.sqlite` is the
active database. `generations/` supports atomic rebuild and rollback. `cache/`
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
`.repogrammar/.gitignore`, `manifest.json`, and `receipts/init.json`. The current
`index` and `sync` implementation creates SQLite generations from TS/JS and
Python `.py` discovery metadata plus syntax-only code-unit records,
CodeUnit-derived IR nodes, and conservative IR containment edges, then activates
`.repogrammar/current-generation` after validation. It does not yet create a
top-level `.repogrammar/repogrammar.sqlite`, telemetry queues, freshness
manifests, or family-evidence query execution. The current storage adapter has
generation-scoped family tables and a FamilyStore port. The product `index` and
`sync` path now invokes a conservative EC-MVFI-lite builder before activation,
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
not passed to the current family builder, and do not create family rows by
themselves. Root `pyproject.toml` may be indexed as a `python-config` file and
`project_config` code unit; only sanitized `PROJECT_CONFIG`/`STRUCTURAL`
metadata or typed project-config `UNKNOWN` records may be stored, and they are
blocked from family-claim input. The CLI can
read the active generation for `files`, `units`, and FamilyStore-backed
pattern-family commands. Raw structural IR and semantic-fact read paths remain
internal. Active-generation reads
open one generation read-only, require a regular `current-generation` pointer,
validate the generation schema and health, and recheck stored repo-relative
paths, strict content hashes, languages, unit ids, byte ranges, IR node/edge
references, semantic fact kind/certainty tokens, assumptions JSON, and
same-generation evidence before returning records.
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
Generation validation transitions only `building` to `validated`, may recheck an
already `validated` generation without changing it, and must not downgrade or
reactivate an `active` generation.
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
reasons, and `index`/`sync` store the discovered file manifest in a
generation-scoped SQLite database. The current index path also stores
syntax-only `code_units` containing repo-relative path, language, kind,
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
row must carry file content hash and source range. Full repository revision
metadata for family evidence remains deferred until repository/worktree
freshness metadata is implemented.
Repository-relative storage paths are lexical, slash-separated, non-empty
paths. They must reject absolute paths, Windows drive prefixes, backslashes,
URI-like text, control characters, `.`/`..` traversal segments, and empty path
segments before activation or readback.
Unknown facts must retain reason code, affected claim, freshness status, and
recovery guidance where applicable.

## SQLite Responsibilities

RepoGrammar uses repository-local SQLite databases. SQLite and SQL migration
logic belong only in persistence adapters. The current substrate creates one
database per generation under `.repogrammar/generations/<generation>/` and
records the active generation in `.repogrammar/current-generation`. The
top-level `.repogrammar/repogrammar.sqlite` active database path remains the
target read path for later family-evidence query integration. Current CLI
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
family members, variation slots, and evidence links. The current `index` and
`sync` command path populates indexed files, syntax-only code units,
CodeUnit-derived IR nodes, conservative IR containment edges, and optional
semantic-worker facts. The storage ports also expose semantic-fact and family
evidence writers for future frontend and claim-builder integration. These
writers accept only building generations and require evidence to match an
indexed code unit's repository-relative path, content hash, and byte range in
the same generation. Family evidence rows must be linked to a family; semantic
fact evidence rows must remain unlinked to a family. Generation validation must
reject malformed semantic or family evidence rows before activation. The storage
adapter enforces foreign keys, repo-relative paths at the Rust port boundary,
matching file/code-unit content hashes, code-unit byte ranges bounded by indexed
file size, IR node references to same-generation code units, IR edge references
to same-generation IR nodes, family/member/slot/evidence generation binding,
family-evidence presence for non-`UNKNOWN` family classifications, and
validation before activation.
The current storage schema version is `4`. Existing pre-release schema `1`,
`2`, and `3` generation databases are treated as stale and must be rebuilt
rather than silently upgraded in place.
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
- source evidence.

The production SQLite dependency is `rusqlite` with bundled SQLite enabled.
Only `src/rust/adapters/persistence/` may depend on it directly; application and
domain code must use RepoGrammar-owned storage port types.

`repogrammar status` must show journal mode when an active generation exists
and must distinguish the bootstrap manifest schema from the active SQLite
storage schema in both human and JSON output.
`repogrammar doctor` must run SQLite integrity checks, verify schema version,
verify active generation consistency, report missing storage layout without
recreating it, and report lock state. Doctor JSON must distinguish
`checks.manifest_schema_version` from `checks.storage_schema_version` and must
not emit an ambiguous `checks.schema_version` field.

## Index Generations

Repository initialization and indexing must build a new index generation without
overwriting the previous valid index. The new generation becomes active only
after persistence validation succeeds. Cancellation or failure must preserve the
previous valid generation.

Full rebuilds must write to a new generation and atomically activate it only
after validation. All family and evidence records must bind to a
`generation_id`. MCP serving should open the active database read-only where
possible. Indexing is the only writer.

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
`index` and `sync` implementation creates `index.lock` with atomic
create-new semantics before discovery, holds it through validation and
active-generation pointer update, and removes only the lock content it wrote.
If lock metadata creation fails after the file is created, RepoGrammar must
remove the partial `index.lock` before returning the write error. Stale-lock
replacement also requires the lock bytes to match the inspected stale bytes
before deletion, so a concurrently replaced lock is rechecked instead of
removed. A live same-host lock is refused. A lock whose same-host process is
confirmed dead may be replaced during acquisition; malformed, cross-host,
cross-OS, or otherwise unknown locks are refused and surfaced by `doctor`.

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
guidance: run repogrammar init
```

If the index is stale, RepoGrammar must either return a stale warning or refuse
family claims whose evidence changed.

The current implementation performs only an internal semantic-fact
claim-input readiness check over the active claim-input snapshot against current
source content hashes. Changed or missing source blocks the affected semantic
fact with typed `StaleEvidence`.
Repository-revision, worktree-wide, and persisted family freshness remain
deferred.

Auto-sync is optional in v0.1. The v0.1 baseline is `init`, `index`, `sync`,
freshness warnings in `status`, and freshness checks before MCP claims. A future
watcher may reparse changed units, mark affected families stale, and lazily
recompute on query, but it should not eagerly recompute the whole repository by
default.

## Migration Strategy

Migration execution logic belongs under `src/rust/adapters/persistence/`.
Migrations must be deterministic, versioned, tested, and documented before
storage is implemented.

## Non-goals

RepoGrammar does not use a vector database in the first version. Embeddings are
not part of the bootstrap architecture.
