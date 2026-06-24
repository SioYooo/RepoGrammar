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

`repogrammar init --write-gitignore` may update the root `.gitignore` with a
small marker-fenced section. Without this flag or explicit interactive
confirmation, root `.gitignore` must remain untouched.

`repogrammar uninit` removes repository-local RepoGrammar state. It is the only
command that may remove `.repogrammar/`; `repogrammar uninstall` must not remove
project indexes. `uninit` must make logs deletion explicit.

`repogrammar status` must report whether the repository is initialized, whether
the active index is fresh, the active generation, schema version, journal mode,
and relevant warning states.

`repogrammar doctor` must check database integrity, schema version, journal
mode, lock state, active generation consistency, Git hygiene, and state
directory configuration.

`repogrammar unlock` must remove only confirmed stale locks. It must inspect the
recorded process, host, OS, and advisory lock state before deletion. `--force`
must require explicit confirmation.

`repogrammar logs` reads repo-local diagnostic logs. It supports:

- `--tail`;
- `--since <duration>`;
- `--component index|daemon|mcp|telemetry`;
- `--redact`.

Logs are diagnostic state, not telemetry.

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
append explicit deferred-status text that query execution still requires a
validated pattern-family index. `status` and `doctor` may report a clean
not-initialized state without opening storage. They must not imply that real
indexing, storage activation, or MCP serving has run.

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

The bootstrap recognizes the command surface and required options. `status` and
`doctor` expose safe missing-index status without initializing storage.
Pattern-family query commands return `FALLBACK_TO_CODE_SEARCH` plus
not-implemented guidance when no validated index is available, and return a
structured fallback object when `--json` is present. Commands that would mutate
repository state, install agent configuration, run real indexing, or serve MCP
return explicit not-implemented or deferred-write errors until those
implementations are designed and tested.
