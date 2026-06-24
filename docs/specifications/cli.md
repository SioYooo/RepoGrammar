# CLI Specification

RepoGrammar's CLI is designed around implementation-pattern families, not
generic symbol graph navigation.

## v0.1 command surface

Project lifecycle:

- `init`
- `uninit`
- `index`
- `sync`
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

Metrics:

- `stats`
- `telemetry`

Maintenance:

- `version`
- `help`

## Pattern-family commands

`repogrammar find` is the main human-facing equivalent of the MCP
`repogrammar_context` operation `find_analogues`. It must return candidate
families, target compatibility, dominant patterns, variation points, exceptions,
unknowns, and a minimal contrastive evidence set. It must not return only top-k
similar files.

`repogrammar family` is the CLI equivalent of the `show_family` operation.

`repogrammar explain` is the CLI equivalent of the `explain_deviation`
operation.

`repogrammar check` is the CLI equivalent of the `check_conformance` operation.

All query commands must support:

- `--project <path>`
- `--token-budget <n>`
- `--json`
- `--include-variations`
- `--include-exceptions`

## Long-running commands

All long-running commands must support:

- `--progress auto|always|never`
- `--json`
- `--quiet`
- `--verbose`

Long-running commands include repository initialization, indexing, sync, and MCP
serving.

## Repository state commands

`repogrammar init` creates repository-local state under `.repogrammar/` by
default, or under `REPOGRAMMAR_DIR` when that environment variable is set. It
must not modify tracked repository files by default. It must write
`.repogrammar/` and `.repogrammar-*/` to `.git/info/exclude` when Git is
available, and it must create `.repogrammar/.gitignore` as a second defense.
`REPOGRAMMAR_DIR` is a repo-local directory-name override only; empty values,
absolute paths, traversal, nested paths, symlink state directories, file
conflicts, and names outside `.repogrammar` or `.repogrammar-*` must be
rejected.

`repogrammar init --write-gitignore` may update the root `.gitignore` with a
small marker-fenced section. Without this flag or explicit interactive
confirmation, root `.gitignore` must remain untouched.

`repogrammar uninit` removes repository-local RepoGrammar state. It is the only
command that may remove `.repogrammar/`; `repogrammar uninstall` must not remove
project indexes. `uninit` must make logs deletion explicit.

`repogrammar status` must support human and `--json` output. It must report
whether the repository is initialized, manifest status, the active generation,
schema version, journal mode, storage/indexing implementation status, missing
subdirectories, and relevant warning states. When storage is wired, it must also
report SQLite integrity status and unhealthy storage states without exposing
absolute paths.

`repogrammar doctor` must support human and `--json` output. It must check
manifest status, required lifecycle subdirectories, storage/indexing
implementation status, lock state, Git hygiene, and state directory
configuration. Once SQLite exists, it must also check database integrity,
schema version, journal mode, and active generation consistency.
During the current syntax-only phase, `doctor` is wired to SQLite storage health
for the active generation. It must still distinguish file-manifest-only,
syntax-only code-unit, and future family-evidence indexing.

`repogrammar index` and `repogrammar sync` currently require an initialized
repository-local state directory. They run TS/JS discovery, read source through a
repo-relative hash-checked boundary, store repo-relative file metadata and
syntax-only code-unit records in a new generation-scoped SQLite database,
validate the generation, and atomically activate
`.repogrammar/current-generation`. Human and JSON output must report
`indexing: syntax_only_code_units`, the actual `indexed_units` count,
the actual `semantic_facts` count, `parser: syntax_only`, `semantic_worker`,
and `mining: deferred`. By default, `semantic_worker` is `deferred`.
When `REPOGRAMMAR_TYPESCRIPT_WORKER` is set to an explicit worker executable,
`index` and `sync` may run that worker after syntax-only code units are stored
for the building generation.
`REPOGRAMMAR_TYPESCRIPT_WORKER_ARGS_JSON` may supply an optional JSON array of
non-blank string arguments. This is an argv contract, not shell parsing; worker
arguments without `REPOGRAMMAR_TYPESCRIPT_WORKER` are invalid. Worker facts must
pass the same-generation storage gate before they are recorded. Worker
unavailable, unsupported-version, timeout, crash, or protocol-violation
failures must fall back to syntax-only indexing with a typed
`semantic_worker: fallback_*` status and sanitized warnings. A worker fact that
conflicts with the indexed code-unit path, content hash, or range must abort the
new generation rather than silently dropping or accepting stale evidence. If
storage health is already unhealthy, index and sync must refuse and direct the
user to `repogrammar doctor` rather than masking the corruption with a new
generation.

`repogrammar unlock` must remove only confirmed stale locks. It must inspect the
recorded process, host, OS, and advisory lock state before deletion. `--force`
must require explicit confirmation. During bootstrap, unlock is inspection-only
and must report known lock files without deleting them.

`repogrammar logs` reads repo-local diagnostic logs. It supports:

- `--tail`;
- `--since <duration>`;
- `--component index|daemon|mcp|telemetry`;
- `--redact`.

Logs are diagnostic state, not telemetry. During bootstrap, `logs` exposes
redacted metadata only; machine-readable output must not include source
snippets or absolute paths.

## Installer commands

`install` and `uninstall` must support:

- `--target`
- `--scope global|project`
- `--dry-run`
- `--yes`
- `--print-config`
- `--no-telemetry`
- `--no-permissions`

Installer commands configure agents and machine-level integration only. They do
not create, delete, or rewrite `.repogrammar/`.

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
guidance: run repogrammar init
```

During the bootstrap, pattern-family query commands use this fallback shape and
append explicit deferred-status text that query execution still requires stored
pattern-family evidence. `status` and `doctor` may report a clean
not-initialized state without opening storage. Stored syntax-only code units are
not family evidence; stored semantic facts are not family evidence until
freshness and claim gates exist. Query commands must not imply that TypeScript
compiler analysis, mining, family-query execution, or MCP serving has run. The
`files` and `units` commands are a limited exception: when an active syntax-only
generation exists, they may read and return repo-relative indexed-file metadata
and code-unit records for inventory/debugging only.

With `--json`, query fallback output must use exit status `2` and write a
stable JSON object to `stderr` rather than the human text block:

```json
{
  "status": "FALLBACK_TO_CODE_SEARCH",
  "reason": "repository is not initialized",
  "guidance": "run repogrammar init",
  "command": "find",
  "implemented": false
}
```

For `files` and `units`, the command itself is implemented even when its active
index precondition is not met. Their missing/unreadable-index JSON fallback must
therefore set `implemented: true`; pattern-family query commands that still lack
family evidence must set `implemented: false`.

If the index is stale, the command must warn or refuse claims whose evidence has
changed.

Once query execution exists, analysis uncertainty must be reported as typed
`UNKNOWN` with a reason code and affected claim. Missing-index fallback,
not-yet-implemented query execution, stale-index refusal, and typed `UNKNOWN`
are separate states and must not be collapsed into one error string.

If repository-local state exists but pattern-family query execution is still
unimplemented or no family evidence has been built, query commands must keep the
same `FALLBACK_TO_CODE_SEARCH` marker but use a reason such as `query execution
requires pattern-family evidence`, not `repository is not initialized`.
If repository status cannot be read safely, the fallback must direct the user to
`repogrammar doctor` instead of masking corrupted state as an uninitialized
repository.
For `files` and `units`, initialized state with no active generation must keep
the fallback marker but use `reason: no active index generation`, guidance to
run `repogrammar index`, and `implemented: true` in JSON. Corrupt or unreadable
state must direct users to `repogrammar doctor`. Once an active syntax-only
generation exists, `files --json` must return `status: ok`, `implemented: true`,
`indexing: syntax_only_code_units`, the active generation, and a `files` array
of repo-relative paths, languages, sizes, and strict content hashes. `units
--json` must return the active generation, `semantic_worker: deferred`, `mining:
deferred`, and a `units` array of repo-relative unit ids, paths, languages,
kinds, byte ranges, and strict content hashes. Neither command may include
source snippets or absolute paths.

## Current implementation status

The bootstrap recognizes the command surface and required options. `init`
creates safe repo-local lifecycle state, `.repogrammar/.gitignore`, required
lifecycle subdirectories, a bootstrap manifest, `receipts/init.json`, and Git
ignore hygiene. `uninit --yes` removes only the resolved RepoGrammar state
directory. `status`, `doctor`, `unlock`, and `logs` expose human and JSON-safe
repo-local lifecycle information without claiming parser/mining support.
`index` and `sync` create syntax-only SQLite generations from the TS/JS file
discovery substrate and dependency-free structural extractor. Their JSON output
includes `generation_id`, `discovered_files`, `stored_files`, the actual
`indexed_units` count, the actual `semantic_facts` count, `indexing:
syntax_only_code_units`, `parser: syntax_only`, `semantic_worker`, and `mining:
deferred`. By default they do not launch a semantic worker and report
`semantic_worker: deferred`. When `REPOGRAMMAR_TYPESCRIPT_WORKER` names an
explicit executable, optional
`REPOGRAMMAR_TYPESCRIPT_WORKER_ARGS_JSON` supplies the worker argv vector. The
commands pass the discovered repo-relative TS/JS file set to that worker, record
only worker facts that match the active building-generation code-unit
path/hash/range gate, and still make no family or query claims.
They do not store source snippets, absolute paths, families, or pattern-family
evidence.
`files` and `units` now read only active syntax-only index metadata and code-unit
records. Pattern-family query commands return `FALLBACK_TO_CODE_SEARCH` plus
not-implemented guidance when no family evidence is available, and return a
structured fallback object when `--json` is present. Commands that install agent
configuration or serve MCP return explicit not-implemented or deferred-write
errors until those implementations are designed and tested.
