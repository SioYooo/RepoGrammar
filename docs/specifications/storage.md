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

The bootstrap `init` implementation creates the lifecycle directories,
`.repogrammar/.gitignore`, `manifest.json`, and `receipts/init.json`. The current
`index` and `sync` implementation creates SQLite generations from TS/JS discovery
metadata plus syntax-only code-unit records and activates
`.repogrammar/current-generation` after validation. It does not yet create a
top-level `.repogrammar/repogrammar.sqlite`, telemetry queues, families,
freshness manifests, or family-evidence query read paths. The CLI can read the
active generation for `files` and `units` inventory/debugging output only. That
read path opens the active generation read-only, requires a regular
`current-generation` pointer, validates the generation schema and health, and
rechecks stored repo-relative paths, strict content hashes, languages, unit ids,
and byte ranges before returning records.

The storage port and SQLite adapter can persist semantic facts together with
repo-relative evidence rows for a building generation, but only when the fact is
already validated against an indexed code unit, matching content hash, and byte
range in that same generation. This is a storage substrate for future semantic
worker integration; the current `index` and `sync` commands do not launch a
worker or record semantic facts.

## File Discovery Exclusions

Indexing must respect repository ignore rules and default generated-artifact
exclusions. At minimum, RepoGrammar must skip:

- `.repogrammar/` and `.repogrammar-*`;
- dependency, build, cache, coverage, virtual environment, and generated output
  directories such as `node_modules`, `vendor`, `dist`, `build`, `target`,
  `.venv`, `Pods`, `.next`, and `coverage`;
- files ignored by Git or by nested `.gitignore` rules;
- files larger than the configured size limit, with 1 MB as the default limit.

Explicit project configuration may opt in additional files, but ignored
third-party and generated artifacts must not enter family evidence by accident.

The current discovery substrate enforces these defaults for `.ts`, `.tsx`,
`.js`, and `.jsx` files only. It returns repo-relative metadata and skip
reasons, and `index`/`sync` store the discovered file manifest in a
generation-scoped SQLite database. The current index path also stores
syntax-only `code_units` containing repo-relative path, language, kind,
start/end byte range, and content hash. Source snippets, absolute paths,
families, and semantic-worker-produced facts are not stored by `index`/`sync`.
The active indexed-file and code-unit rows are exposed only through the limited
`files`/`units` read path and must not be treated as family evidence. Active
`units` reads must revalidate stored code-unit ids against their repo-relative
paths before rendering output so tampered generation databases cannot smuggle
absolute paths through unit ids.

Experimental Python discovery, once accepted, must record its support level and
must not be stored or reported as official v0.1 production support. Optional
provider facts, including future CodeGraph-derived facts, must carry provider
provenance and freshness metadata before they can participate in family
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
raw path. Every family and evidence row must carry file content hash, source
range, generation id, and repository revision metadata.
Unknown facts must retain reason code, affected claim, freshness status, and
recovery guidance where applicable.

## SQLite Responsibilities

RepoGrammar uses repository-local SQLite databases. SQLite and SQL migration
logic belong only in persistence adapters. The current substrate creates one
database per generation under `.repogrammar/generations/<generation>/` and
records the active generation in `.repogrammar/current-generation`. The
top-level `.repogrammar/repogrammar.sqlite` active database path remains the
target read path for later family-evidence query integration. Current CLI
`files` and `units` reads open the active generation database directly after
validating the active pointer and schema, and expose only repo-relative metadata
and code-unit rows.

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
`sync` command path populates only indexed files and syntax-only code units.
The storage port also exposes a semantic-fact writer for future frontend
integration; it accepts only building generations and requires evidence to
match an indexed code unit's repository-relative path, content hash, and byte
range in the same generation. Generation validation must reject malformed
semantic evidence rows before activation. The storage adapter enforces foreign
keys, repo-relative paths at the Rust port boundary, matching file/code-unit
content hashes, code-unit byte ranges bounded by indexed file size, and
validation before activation.
The current storage schema version is `2`. Existing pre-release schema `1`
generation databases are treated as stale and must be rebuilt rather than
silently upgraded in place.
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

`repogrammar status` must show journal mode when an active generation exists.
`repogrammar doctor` must run SQLite integrity checks, verify schema version,
verify active generation consistency, report missing storage layout without
recreating it, and report lock state.

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
must contain:

```json
{
  "kind": "index",
  "pid": 12345,
  "hostname_hash": "sha256:...",
  "os": "darwin",
  "started_at": "2026-06-24T00:00:00Z",
  "repogrammar_version": "0.1.0",
  "generation": "gen-000003"
}
```

`repogrammar unlock` must not be a blind delete command. It must:

- check that the lock file exists;
- check whether the recorded process is still alive;
- check whether the lock belongs to the same OS and host;
- try to acquire an advisory lock;
- delete only confirmed stale locks;
- refuse to delete an active process lock;
- output the reason a lock was removed;
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
