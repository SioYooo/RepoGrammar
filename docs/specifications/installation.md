# Installation Specification

RepoGrammar separates machine-level agent integration from repository-level
indexing.

Installation is three separate steps:

1. install the `repogrammar` CLI binary;
2. run `repogrammar install` to wire machine-level coding-agent MCP
   integration;
3. run `repogrammar init` and `repogrammar index` inside each repository that
   should have a local RepoGrammar index.

End users must not need Rust, Cargo, Node.js, npm, Docker, the SQLite CLI, a
local LLM, an embedding model, or cloud API keys to install and run the
RepoGrammar CLI. Rust/Cargo remains a contributor and source-build dependency
only. The current Python preview still requires a `python3` interpreter at
indexing time because RepoGrammar uses a bundled CPython AST worker asset; it
must not require a Python virtualenv or project dependency installation. Node.js
is needed only for TypeScript worker test development.

Agent integration may require the selected native agent CLI:

- `codex` for Codex integration;
- `claude` for Claude Code integration.

Missing agent CLIs must be non-fatal in interactive flows when other supported
choices remain available.

RepoGrammar follows a CodeGraph-style installation architecture: CLI
acquisition, machine-level agent wiring, and repository-level `init`/`index`
remain separate lifecycle layers. `repogrammar install` is the agent-wiring
orchestrator. It resolves targets through a registry, plans global versus
project-local scope, prints target MCP snippets on request, and delegates live
writes only to adapters with an implemented reversible ownership contract.

Public-preview release artifacts use these platform targets:

- `repogrammar-aarch64-apple-darwin.tar.gz`;
- `repogrammar-x86_64-apple-darwin.tar.gz`;
- `repogrammar-aarch64-unknown-linux-gnu.tar.gz`;
- `repogrammar-x86_64-unknown-linux-gnu.tar.gz`;
- `repogrammar-x86_64-pc-windows-msvc.zip`.

Every release artifact must include the `repogrammar` executable and the
bundled Python worker asset under `workers/python/worker.py`, and must have a
matching `.sha256` checksum asset.

Source checkouts may provide a dependency-light wrapper script at
`src/install/repogrammar-install.sh`. The script is a convenience TUI entrypoint
around release artifacts and the product binary: it may download a prebuilt
release artifact, verify its checksum, install or repair the user-writable
`repogrammar` command, install bundled worker assets, call
`repogrammar install`, call `repogrammar uninstall`, remove the local command
path after confirmation, display PATH guidance, or build from source only when
the user explicitly chooses the contributor source-build path. It must not
duplicate native agent configuration logic outside the Rust installer, and it
must not create or modify `.repogrammar/`.

Before GitHub Release artifacts exist, source checkouts must remain dogfoodable
through explicit contributor paths:

- `bash src/install/repogrammar-install.sh --install-cli-only --from-source --yes`;
- `bash src/install/repogrammar-install.sh --install-and-configure --from-source --yes --target all`.

The source path may require Rust/Cargo because it is a contributor workflow,
but it must install the built binary into RepoGrammar-managed user state before
refreshing the user-writable command path. It must pass
`REPOGRAMMAR_INSTALL_DIR`, `REPOGRAMMAR_COMMAND_DIR`, and
`REPOGRAMMAR_EXECUTABLE` consistently when delegating to `repogrammar install`.
It must not directly create a foreign unmanaged command path that later causes
the Rust installer to refuse ownership. If no release artifact is available and
the script is not running from a source checkout with `--from-source`, it must
fail with actionable guidance, including `REPOGRAMMAR_RELEASE_DIR` for local
artifact tests.

Windows public-preview source checkouts may provide `src/install/install.ps1`
with the same binary-download and checksum-verification boundary. Windows
source-checkout builds are deferred in this preview; Windows dogfood should use
a local release-artifact directory until a Windows source-build path is
specified and tested.

The npm package `@sioyooo/repogrammar` is a thin launcher only. Its `bin`
entrypoint lives under `src/npm/`, detects OS/architecture, downloads the
matching GitHub Release artifact, verifies the `.sha256` checksum, caches the
binary and bundled worker assets under user-local state, and execs the Rust
`repogrammar` binary with the original arguments. It must not compile Rust
source, run `cargo`, implement RepoGrammar analysis behavior in JavaScript, call
real native Codex/Claude CLIs in tests, or become the only installation path.
`npx` and global npm installs require Node/npm by definition, but they must not
require Rust/Cargo.

Before `@sioyooo/repogrammar` is published, npm dogfood may use either a local
packed package or a direct binary override:

- `npm pack` followed by
  `npm install -g ./sioyooo-repogrammar-0.1.0.tgz`;
- `REPOGRAMMAR_BINARY=/absolute/path/to/repogrammar node src/npm/repogrammar.js ...`.

`REPOGRAMMAR_BINARY` is a local dogfood bypass only. It must be an absolute
path to an existing file and must not change the release-artifact default for
published npm use.

## Commands

- `repogrammar install`
- `repogrammar uninstall`

## Scope

Installer commands support global and project-local scopes. `--location` is
accepted as an alias for `--scope` in the CLI. Project-local installation must
not impose RepoGrammar's own mirrored `AGENTS.md` and `CLAUDE.md` policy on
consuming repositories.

The target registry recognizes these CodeGraph-style target ids for planning
and `--print-config` output:

- `codex`;
- `claude-code` / `claude`;
- `cursor`;
- `opencode`;
- `hermes`;
- `gemini`;
- `antigravity`;
- `kiro`.

It also accepts `auto`, `all`, `none`, and comma-separated concrete target
lists such as `codex,claude-code`. In the current public preview, live writes
are implemented only for global Codex and global Claude Code. Other registry
targets are configuration-preview/deferred targets until their idempotent
writer, ownership receipt, uninstall inverse, and tests are implemented.

`repogrammar install` and `repogrammar uninstall` configure agent integration
only. They must not create, update, or delete `.repogrammar/`, and they must not
remove project indexes, logs, caches, locks, or repository-local receipts.
They must not run `init`, `index`, or `sync`.

Repository lifecycle state is owned by `repogrammar init`,
`repogrammar index`, `repogrammar sync`, and `repogrammar uninit`.

## Safety requirements

The installer must:

- install from prebuilt release artifacts for end users;
- verify release artifact checksums before installing a downloaded binary;
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
- accept `--location` as a `--scope` alias and accept `auto`, `all`, `none`,
  and comma-separated target lists;
- make `--print-config <target>` a no-write path that prints the selected
  target's MCP snippet and exits without requiring HOME, installing the command,
  running the MCP self-test, or delegating native writes;
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
telemetry consent and must not prompt for telemetry. When live `install --yes`
runs without `--telemetry` or `--no-telemetry`, telemetry remains disabled.
`--telemetry` is the explicit opt-in flag for install-time planning, receipts,
and live preference persistence after agent installation succeeds.
`--no-telemetry` remains accepted as an explicit disable and
backward-compatible flag. Interactive telemetry prompts are allowed only in the
default TUI-style installer, only when no telemetry flag was supplied, and the
default answer is no. `REPOGRAMMAR_TELEMETRY=0`, `DO_NOT_TRACK=1`, and CI force
the effective install-time telemetry decision to disabled and skip prompting.
Users can also change actual telemetry preference
with `repogrammar telemetry on` and `repogrammar telemetry off`.

It must not contain source-derived family facts, evidence text, source paths,
symbol names, query text, raw prompts, or repository-specific SQLite indexes.
Machine-level integration receipts may contain the configured RepoGrammar
executable path, native agent command arguments, and the resolved
instruction-file path plus the instruction action taken, because they are
required for precise uninstall; they must not contain paths discovered from an
indexed repository, source evidence paths, prompts, or query targets. Each
receipt records `target`, `scope`, `mcp_server`, `executable_path`,
`native_program`, `native_args`, `instruction_file_path` (null when deferred),
`instruction_action`, `telemetry_enabled`, and `created_unix_seconds` only.

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

The installer must not overwrite unrelated user instructions. `uninstall`
reverses only RepoGrammar's own managed write. If a file has a malformed or
incomplete managed section, the installer must stop and direct the user to a
repair workflow such as `repogrammar doctor --repair-instructions`.

The managed instruction writer is implemented as a reversible, idempotent
operation:

- it creates the file with the managed section when the file is absent;
- it appends the managed section, preserving prior content, when the file exists
  without markers;
- it replaces the section in place when a single complete managed section exists;
- it reports an unchanged result when the section is already byte-equivalent;
- it refuses to modify a file with malformed, partial, or duplicated markers;
- it writes atomically through a sibling temp file plus rename and re-reads the
  file to verify the managed section before reporting success;
- `uninstall` and rollback reverse exactly the recorded `instruction_action`:
  they remove the managed section and preserve unrelated user content; when
  RepoGrammar created the file (`instruction_action: "created"`) and stripping
  the section leaves it empty, they also delete the file so no empty artifact is
  left behind, but a file that pre-existed the install or gained user content
  after creation keeps its remaining content and is never deleted.

Because real Codex/Claude global instruction-file locations are not yet verified,
live instruction writing is deferred by default. The installer resolves a
target's instruction-file path only from the explicit environment override
`REPOGRAMMAR_INSTRUCTION_FILE_<TARGET>` (for example
`REPOGRAMMAR_INSTRUCTION_FILE_CODEX`) and only when it resolves to an absolute
path. When no path is resolved, the receipt records `instruction_action:
"deferred"` and no file is written. RepoGrammar never guesses an instruction-file
path.

Consuming repositories must not be forced to mirror RepoGrammar's own
`AGENTS.md` and `CLAUDE.md` policy.

## Current implementation status

The current implementation supports deterministic dry-run planning,
noninteractive live writes, and a dependency-light text wizard:

- public-preview release packaging is defined by the release workflow for
  macOS arm64/x86_64, Linux arm64/x86_64, and Windows x86_64 preview, each with
  a bundled Python worker asset and `.sha256` checksum. Real GitHub
  prerelease artifacts are not available until a preview tag is published, so
  source-checkout dogfood remains the supported pre-release path;
- `src/install/repogrammar-install.sh` is the macOS/Linux installer wrapper. By
  default it downloads a prebuilt release artifact instead of requiring Cargo,
  verifies the checksum, installs the CLI and bundled worker asset, and can then
  launch agent wiring or uninstall flows. In a source checkout, its interactive
  menu makes the contributor source-build path first-class, and its
  noninteractive `--from-source` mode supports dogfood before release artifacts
  exist;
- `src/install/install.ps1` is the Windows preview installer wrapper for the
  Windows x86_64 artifact. Windows source-checkout dogfood builds remain
  deferred;
- `repogrammar install` with no flags launches a TUI-style wizard when running
  in an interactive terminal;
- the wizard presents Codex and Claude Code, supports multi-select in one run,
  detects existing RepoGrammar-owned receipts, and skips already managed agents
  by default;
- the installer has a target registry for Codex, Claude Code, Cursor,
  opencode, Hermes, Gemini, Antigravity, and Kiro, exposed through a per-target
  adapter contract (`TargetAdapter`) that consolidates scope support, live-writer
  status, the no-write config preview, and the native MCP plus instruction-file
  plan lines (`describe_paths`). The current registry exposes deferred targets
  through dry-run and `--print-config` snippets only; live writes remain
  implemented for global Codex and global Claude Code;
- re-running the wizard can add missing supported agents later or refresh the
  RepoGrammar-managed command path even when all selected agents are already
  managed, without re-running native agent add commands for already managed
  receipts;
- noninteractive live writes still require `--yes`;
- `install --yes`, `install --dry-run`, and explicit `--target ... --yes`
  never prompt;
- `--target all --scope global --yes` is safe because multi-agent install is
  all-or-rollback;
- `--target codex --scope global` uses the native `codex mcp add/remove`
  commands;
- `--target claude-code --scope global` uses the native `claude mcp add/remove`
  commands with `user` scope;
- live project-local writes remain deferred until ownership, receipt, and native
  config semantics are specified for each supported agent;
- install places the `repogrammar` command in a user-writable command directory
  when possible and points agent MCP entries at the installed command binary;
- the `@sioyooo/repogrammar` launcher supports `npx @sioyooo/repogrammar ...`
  after package publication by downloading and caching the matching prebuilt
  release artifact, then delegating all behavior to the Rust binary;
- install runs a read-only MCP self-test before native agent configuration, with
  a bounded timeout that kills and reaps a hanging self-test process;
- install writes one RepoGrammar-owned receipt per configured target under the
  user install data directory after native configuration succeeds, recording the
  resolved instruction-file path and instruction action;
- the managed instruction-file writer (create/append/replace/idempotent/remove,
  atomic temp+rename with re-read verification, malformed-marker refusal) is
  implemented and tested, but live instruction writes stay deferred unless
  `REPOGRAMMAR_INSTRUCTION_FILE_<TARGET>` resolves to an absolute path;
- if any selected agent install or receipt write fails, receipts created during
  that run are removed and native entries configured during that run are
  removed in reverse order;
- `uninstall --target all --scope global --yes` removes all matching
  RepoGrammar-owned first-class agent receipts it finds and ignores missing
  receipts only when at least one owned receipt is removed;
- explicit single-target uninstall still refuses missing or foreign receipts
  rather than removing unmanaged configuration;
- live install persists the final anonymous telemetry preference after
  successful agent configuration; non-interactive `--yes` alone persists
  disabled telemetry, interactive install without telemetry flags prompts
  default-no, and environment/CI disablement overrides `--telemetry`.
- dry-run output names the native MCP command shape for Codex and Claude Code
  global installs and clearly marks deferred registry targets/scopes, while
  project-local live writes remain deferred unless separately specified and
  tested.
- default tests must not invoke real `codex` or `claude` binaries. Native agent
  integration coverage uses dry-run output, command-vector construction, fake
  configurators, and receipt behavior; any real native-CLI integration test must
  be explicitly ignored or feature-gated outside default CI.

By default the installer does not edit instruction files: live instruction
writing stays deferred unless an explicit `REPOGRAMMAR_INSTRUCTION_FILE_<TARGET>`
override resolves to an absolute path. The installer still does not repair
malformed native agent config, upload telemetry, run paired experiments, or
touch `.repogrammar/`.
