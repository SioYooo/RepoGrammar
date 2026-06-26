# Installation Specification

RepoGrammar separates machine-level agent integration from repository-level
indexing.

## Commands

- `repogrammar install`
- `repogrammar uninstall`

## Scope

Installer commands support global and project-local scopes. Project-local
installation must not impose RepoGrammar's own mirrored `AGENTS.md` and
`CLAUDE.md` policy on consuming repositories.

`repogrammar install` and `repogrammar uninstall` configure agent integration
only. They must not create, update, or delete `.repogrammar/`, and they must not
remove project indexes, logs, caches, locks, or repository-local receipts.

Repository lifecycle state is owned by `repogrammar init`,
`repogrammar index`, `repogrammar sync`, and `repogrammar uninit`.

## Safety requirements

The installer must:

- detect supported coding agents;
- prefer native agent configuration commands where available;
- preserve all unknown configuration fields;
- never overwrite malformed configuration by default;
- create a backup before approved repair;
- use atomic writes and reparse the result after writing;
- install the RepoGrammar executable in a user-writable directory;
- store an absolute executable path in MCP configuration where supported;
- avoid sudo or administrator privileges;
- support `--dry-run`, `--print-config`, `--target`, `--scope`, `--yes`,
  `--no-permissions`, `--telemetry`, and `--no-telemetry`;
- validate every configured MCP integration by launching a self-test;
- store an installation receipt sufficient for precise, reversible uninstall;
- never remove configuration that was not created by RepoGrammar;
- treat instruction-file modification as optional and marker-fenced.

## Global installation state

Global user state may contain only installation and user-preference data:

- installed binary and cache metadata;
- agent integration receipts;
- anonymous telemetry preference and anonymous machine id;
- downloaded grammar or runtime artifacts that are not repository-derived;
- global user preferences.

Anonymous telemetry is off by default. Live `install --yes` must not imply
telemetry consent. `--telemetry` is the explicit opt-in flag for install-time
planning, receipts, and live preference persistence after agent installation
succeeds. `--no-telemetry` remains accepted as an explicit disable and
backward-compatible flag. `REPOGRAMMAR_TELEMETRY=0`, `DO_NOT_TRACK=1`, and CI force the
effective install-time telemetry decision to disabled. Users can also change
actual telemetry preference with `repogrammar telemetry on` and
`repogrammar telemetry off`.

It must not contain source-derived family facts, evidence text, source paths,
symbol names, query text, raw prompts, or repository-specific SQLite indexes.

## Instruction-file integration

The MCP initialize response is the canonical runtime guidance for agents.
Installer-written instruction-file content is optional and must be short,
preferably no more than 30 lines.

When writing to files such as `AGENTS.md`, `CLAUDE.md`, or `GEMINI.md`,
RepoGrammar must use this exact marker fence:

```text
<!-- BEGIN REPOGRAMMAR MANAGED SECTION -->
...
<!-- END REPOGRAMMAR MANAGED SECTION -->
```

The installer must not overwrite unrelated user instructions. `uninstall` may
remove only the managed section. If a file has a malformed or incomplete managed
section, the installer must stop and direct the user to a repair workflow such
as `repogrammar doctor --repair-instructions`.

Consuming repositories must not be forced to mirror RepoGrammar's own
`AGENTS.md` and `CLAUDE.md` policy.

## Current implementation status

The bootstrap implements deterministic dry-run planning and option parsing.
Live `install` and `uninstall` writes are intentionally narrow:

- live writes require `--yes`;
- live `--target all` is deferred to avoid partial multi-agent writes;
- `--target codex --scope global` uses the native `codex mcp add/remove`
  commands;
- `--target claude-code --scope global` uses the native `claude mcp add/remove`
  commands with `user` scope;
- live project-local writes remain deferred until ownership, receipt, and native
  config semantics are specified for each supported agent;
- install runs a read-only MCP self-test before native agent configuration, with
  a bounded timeout that kills and reaps a hanging self-test process;
- install writes a RepoGrammar-owned receipt under the user install data
  directory after native configuration succeeds and rolls back the native entry
  if receipt writing fails;
- uninstall removes only targets with a matching RepoGrammar receipt.
- live install persists the final anonymous telemetry preference after
  successful agent configuration; `--yes` alone persists disabled telemetry, and
  environment/CI disablement overrides `--telemetry`.

The installer still does not copy executables, edit instruction files, repair
malformed native agent config, or touch `.repogrammar/`.
