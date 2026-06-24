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
  `--no-permissions`, and `--no-telemetry`;
- validate every configured MCP integration by launching a self-test;
- store an installation receipt sufficient for precise, reversible uninstall;
- never remove configuration that was not created by RepoGrammar;
- treat instruction-file modification as optional and marker-fenced.

## Current implementation status

The bootstrap implements deterministic dry-run planning and option parsing. It
does not yet write agent configuration, install executables, run self-tests, or
write receipts.
