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
During the current file-manifest-only phase, `doctor` is wired to SQLite storage
health for the active generation. It must still distinguish metadata-only
indexing from parser/code-unit/family indexing.

`repogrammar index` and `repogrammar sync` currently require an initialized
repository-local state directory. They run TS/JS discovery, store repo-relative
file metadata in a new generation-scoped SQLite database, validate the
generation, and atomically activate `.repogrammar/current-generation`. Human and
JSON output must report `file_manifest_only`, `indexed_units: 0`, `parser:
deferred`, and `mining: deferred`. If storage health is already unhealthy, they
must refuse and direct the user to `repogrammar doctor` rather than masking the
corruption with a new generation.

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
not-initialized state without opening storage. They must not imply that parser,
semantic-worker execution, mining, query execution, or MCP serving has run.

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

If the index is stale, the command must warn or refuse claims whose evidence has
changed.

## Current implementation status

The bootstrap recognizes the command surface and required options. `init`
creates safe repo-local lifecycle state, `.repogrammar/.gitignore`, required
lifecycle subdirectories, a bootstrap manifest, `receipts/init.json`, and Git
ignore hygiene. `uninit --yes` removes only the resolved RepoGrammar state
directory. `status`, `doctor`, `unlock`, and `logs` expose human and JSON-safe
repo-local lifecycle information without claiming parser/mining support.
`index` and `sync` create metadata-only SQLite generations from the TS/JS file
discovery substrate. Their JSON output includes `generation_id`,
`discovered_files`, `stored_files`, `indexed_units: 0`, `indexing:
file_manifest_only`, `parser: deferred`, and `mining: deferred`; they do not
store source snippets, absolute paths, parser facts, code units, families, or
evidence.
Pattern-family query commands return `FALLBACK_TO_CODE_SEARCH` plus
not-implemented guidance when no validated index is available, and return a
structured fallback object when `--json` is present. Commands that install agent
configuration or serve MCP return explicit not-implemented or deferred-write
errors until those implementations are designed and tested.
