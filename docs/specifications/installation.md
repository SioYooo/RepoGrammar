# Installation Specification

RepoGrammar separates machine-level agent integration from repository-level
indexing.

The ownership model has three separate layers:

1. install the `repogrammar` CLI binary;
2. run `repogrammar install` to wire machine-level coding-agent MCP
   integration;
3. run `repogrammar init` inside each repository that should have a local
   RepoGrammar index, or `repogrammar init --autosync` when agent-session
   auto-sync should start after the first index succeeds.

`repogrammar setup` is the primary onboarding journey after the CLI has been
acquired. It composes layers 2 and 3 in one reviewed plan and one confirmation,
but it does not merge their ownership or rollback rules. The npm launcher and
release installers remain responsible only for acquiring the product binary and
bundled worker, then forwarding setup arguments unchanged. Setup delegates
machine-level writes to the existing install service and receipts, delegates
repository state/index/auto-sync to the existing lifecycle services, and ends
with a read-only product-binary MCP self-test. It never downloads itself and
does not make `repogrammar install` initialize or delete `.repogrammar/`.

`setup --target auto` selects only detected targets with a live writer. If none
are available, setup still initializes and indexes the repository and reports a
single install-agent limitation. Before planning a machine write, setup performs
a bounded, read-only native `mcp get` probe and correlates its parsed scope,
executable, and exact `serve` argument with the RepoGrammar-owned receipt and
the current managed executable authority.

The resulting ownership states are intentionally distinct:

- `Unmanaged`: neither native entry nor receipt exists; setup may configure it;
- `OwnedCurrent`: native entry and receipt match each other and the current
  managed authority; setup skips it;
- `OwnedOutdated`: native entry and receipt match each other but name an
  obsolete authority; setup delegates safe refresh to the install service;
- `Foreign`: a native entry exists without an owned receipt; preserve and
  refuse automatic overwrite;
- `OwnedDrifted`: a receipt is paired with a missing or mismatched native entry;
  preserve and refuse automatic repair;
- `Malformed`: the receipt or successful native response cannot be recognized;
  preserve and refuse automatic overwrite.

An exact native not-found response means absent. A failed probe with unexpected
output remains unknown and blocks setup because native state could not be
inspected safely. Setup may continue repository-only for a successfully probed
but unrecognized malformed configuration and recommends `repogrammar doctor`.
The install service snapshots an `OwnedOutdated` integration before removing
and re-adding it, and restores the exact native entry, receipt, receipt backup,
and managed instruction state if refresh fails. Setup reports newly configured
and reconfigured targets separately. Its outer rollback may uninstall only
newly configured targets; a later repository, auto-sync, or MCP failure must
not delete a refreshed integration that existed before setup. Existing machine
and repository state is preserved. Setup never enables telemetry.

End users must not need Rust, Cargo, Node.js, npm, Docker, the SQLite CLI, a
local LLM, an embedding model, or cloud API keys to install and run the
RepoGrammar CLI. Rust/Cargo remains a contributor and source-build dependency
only. The bounded Python analyzer still requires a Python 3.10 or newer `python3` interpreter at
indexing time because RepoGrammar uses a bundled CPython AST worker asset; it
must not require a Python virtualenv or project dependency installation. Node.js
is needed only for TypeScript worker test development.

Agent integration may require the selected native agent CLI:

- `codex` for Codex integration;
- `claude` for Claude Code integration.

Missing agent CLIs must be non-fatal in interactive flows when other supported
choices remain available.
In the interactive wizard, pressing Enter uses the `a` selection. The `a`
selection must include only detected, not-yet-managed agent integrations by
default; if that automatic set is empty, the installer must report a no-op
rather than selecting undetected agents. Undetected unmanaged agents are shown
for explicit selection, but they must not be selected implicitly. When every
supported concrete agent is already managed by RepoGrammar, the default may
select those already managed agents so the installer can refresh the
RepoGrammar-managed command path without rerunning native agent configuration.

RepoGrammar follows a CodeGraph-style installation architecture: CLI
acquisition, machine-level agent wiring, and repository-level `init`/`index`
remain separate lifecycle layers. `repogrammar install` is the agent-wiring
orchestrator. It resolves targets through a registry, plans global versus
project-local scope, prints target MCP snippets on request, and delegates live
writes only to adapters with an implemented reversible ownership contract.

Stable and preview release artifacts use these platform targets:

- `repogrammar-aarch64-apple-darwin.tar.gz`;
- `repogrammar-x86_64-apple-darwin.tar.gz`;
- `repogrammar-aarch64-unknown-linux-gnu.tar.gz`;
- `repogrammar-x86_64-unknown-linux-gnu.tar.gz`.

The Linux archives are glibc-only: x86_64 requires glibc 2.35 or newer and
arm64 requires glibc 2.39 or newer. The npm launcher and shell installer must
prove the matching runtime family and minimum version before download; musl,
older glibc, and unknown libc fail closed. The x86_64 builder is pinned to
Ubuntu 22.04 and the arm64 builder to Ubuntu 24.04, and each build records the
highest imported GLIBC symbol version. Every native build exercises post-build
ABI inspection, but only the tag-run build can become publication evidence.

These four macOS/Linux archives are the complete stable and preview platform
set. Windows is not a release or npm platform while its local index lifecycle
remains unsupported; the workflow must not build, smoke, upload, or imply a
Windows archive.

Every release artifact must include the `repogrammar` executable and the
bundled Python worker asset under `workers/python/worker.py`, and must have a
matching `.sha256` checksum asset.
The release build must unpack and execute each exact archive on its native
runner before upload. The packaged binary smoke enforces Python 3.10+ and runs
`version`, isolated `setup --dry-run --json`, isolated live setup through the
product MCP self-test, full and incremental sync of the committed Pydantic
release fixture, and its bounded `find` plus advisory `check` path. The `check`
result must remain `CONTEXT_ONLY` with `advisory_status: UNKNOWN` rather than
being promoted to runtime proof. Exercising only a source-tree binary is
insufficient. The published `install.sh` asset must also have a matching
`.sha256` checksum asset. `install.ps1` is not published for either channel.
Installers must fail instead of silently installing an artifact that omits the
bundled Python worker.
Manual release-workflow dispatch is build-only and cannot publish, even when a
tag is selected as its ref. It is a rehearsal only: its artifacts are not
publication candidates. Only a pushed tag is a publication event and the tag
run is the sole source of candidate bytes. Before candidate creation, complete
Git history must prove the tag is the exact version tag at the current
`origin/main` commit and Cargo, Cargo lockfile, and npm manifest versions must
match. Preview and stable tags both use the same stage-only npm Trusted
Publisher, protected `npm-release` GitHub environment, and one exact package
tarball produced and smoked in that tag run. They do not use a traditional npm
write token. The tag run uploads a private GitHub draft and privately stages
npm; a human reviews those exact candidates before making either registry
public. The GitHub draft contains exactly four archives, their four checksums,
`install.sh`, its checksum, and `npm-candidate-manifest.json` (11 assets). A
full tag-run rerun refuses any existing release or draft instead of replacing
candidate files; rerunning only failed staging jobs remains available. Stable
publication then publishes the complete GitHub draft as an immutable normal
release, requires human 2FA to approve the npm stage, and runs a separate
read-only finalizer with the exact tag-run id and attempt. Because the two
registries cannot publish atomically, any failure must remain visibly partial
until GitHub immutability, npm integrity/provenance, and public product smokes
are independently verified. Workflow success or local packaging never proves
that either registry publication occurred.
Preview documentation must use an explicit preview tag such as
`v0.2.0-preview.0` rather than relying on GitHub's `latest` redirect. Stable
documentation should pin `v0.2.0` for reproducible acquisition even after the
normal release becomes GitHub `latest`. When a `latest` or explicit artifact
lookup fails, installers must report that the release artifact was not found,
suggest the exact `--version <release-tag>`, and mention
`REPOGRAMMAR_RELEASE_DIR` for local artifact testing.
Installers must validate archive entry names before extraction: absolute paths,
Windows absolute paths, traversal components, URI-like names, backslashes, and
unexpected files are rejected even when the archive checksum matches. Entry
types must also be validated: only regular files and directories are allowed,
so a symlink or hardlink member — which could redirect extraction outside the
temp directory on older `tar` — is rejected before extraction, and extracted
paths are re-verified as regular files (not symlinks) before they are copied
into place.

Source checkouts may provide a dependency-light wrapper script at
`src/install/repogrammar-install.sh`. The script is a convenience TUI entrypoint
around release artifacts and the product binary: it may download a prebuilt
release artifact, verify its checksum, install or repair the user-writable
`repogrammar` command, install bundled worker assets, call
`repogrammar install`, call `repogrammar uninstall`, remove the local command
path after confirmation, clean up stale `repogrammar` copies found on PATH,
display PATH guidance, or build from source only when the user explicitly
chooses the contributor source-build path. It must not
duplicate native agent configuration logic outside the Rust installer, and it
must not create or modify `.repogrammar/`.

Before GitHub Release artifacts exist, source checkouts must remain dogfoodable
through explicit contributor paths:

- `bash src/install/repogrammar-install.sh --install-cli-only --from-source --yes`;
- `bash src/install/repogrammar-install.sh --install-and-configure --from-source --yes --target all`.
- `powershell -ExecutionPolicy Bypass -File src/install/install.ps1 -InstallCliOnly -FromSource -Yes`;
- `powershell -ExecutionPolicy Bypass -File src/install/install.ps1 -InstallAndConfigure -FromSource -Yes -Target all`.

The source path may require Rust/Cargo because it is a contributor workflow,
but it must install the built binary into RepoGrammar-managed user state before
refreshing the user-writable command path. It must pass
`REPOGRAMMAR_INSTALL_DIR`, `REPOGRAMMAR_COMMAND_DIR`, and
`REPOGRAMMAR_EXECUTABLE` consistently when delegating to `repogrammar install`.
First-party installers must resolve the same default install data directory as
the Rust installer (`$XDG_DATA_HOME/repogrammar`, otherwise
`~/.local/share/repogrammar`), so release installs, source builds, and direct
`repogrammar install` runs converge on one managed executable authority under
that data directory's `bin`, and agent MCP entries written by any of them point
at the same managed executable. `cargo install` and the npm launcher install to
their own toolchain-managed locations; they are contributor and bypass
acquisition paths, not the managed authority. See ADR-0014.
Repeated CLI installation must replace existing RepoGrammar-managed installed
executables and managed command copies rather than failing only because the
destination file already exists. Replacement must stage the new file, remove
the previous RepoGrammar-managed file, and then activate the staged file. If the
previous file cannot be removed because an active coding agent or MCP process
is using it, the install path must fail and tell the user to exit that agent
before rerunning the install or build command; it must not keep an alternate
new binary beside the old active one.
After a successful CLI install/update action, first-party wrappers must scan the
current PATH for additional `repogrammar` command copies and compare each copy's
SHA256 with the managed installed executable authority. Copies whose hash
differs from the authority are stale and must be removed after confirmation, or
without prompting when the noninteractive `--yes` / `-Yes` flag is present. The
managed authority and any matching command copy must be preserved. Explicit
verify/prune commands may remain available for diagnosis and manual repair, but
normal install/update paths must not require users to run a second cleanup
command. If a requested stale-copy cleanup cannot remove one or more stale
copies, the wrapper must exit nonzero with actionable guidance and leave the
unremoved paths visible for manual repair.
If the user-writable command path already contains an unmanaged
`repogrammar` — a file the wrapper did not install — replacing it is a trust
boundary and must not happen on `--yes` / `-Yes` alone. The wrapper must refuse
with actionable guidance unless the user passes an explicit opt-in
(`--replace-unmanaged-command` for the shell wrapper, `-ReplaceUnmanagedCommand`
for `install.ps1`); only with that opt-in may it back the file up with an
`.unmanaged-backup` suffix and install the managed command. It must never
silently delete the old file, and it must still refuse unsafe paths such as
directories regardless of that opt-in. It must not directly create a foreign
unmanaged command path that later causes the Rust installer to refuse
ownership. If no release artifact is available and the shell wrapper is not
running from a source checkout with `--from-source`, it must fail with
actionable guidance, including `REPOGRAMMAR_RELEASE_DIR` for local artifact
tests. When a source wrapper runs
without an explicit `REPOGRAMMAR_SOURCE_BINARY` or `-SourceBinary` override, it
must run `cargo build --release` before copying `target/release/repogrammar`
or `target\release\repogrammar.exe`, even if a previous release binary already
exists, so source-checkout one-step installs do not silently reuse stale
binaries.
This command-path backup behavior is specific to first-party CLI acquisition
wrappers after the user invokes install/update; it does not relax
`repogrammar install` ownership rules for agent configuration, receipts, or
unrelated foreign paths.

Windows source checkouts retain `src/install/install.ps1` only as a contributor
and local-dogfood path. It is not a tagged release asset and contains no
release-download installation path. Every CLI install action must fail before
network access or filesystem writes unless the user passes `-FromSource`
explicitly from a RepoGrammar source checkout. `-FromSource` may build or copy
a local `repogrammar.exe`, install bundled worker assets, refresh the
user-writable command path, and delegate to `repogrammar install`. It supports
`REPOGRAMMAR_SOURCE_BINARY` / `-SourceBinary` for deterministic local dogfood
tests with an already built binary, but `-SourceBinary` alone must not bypass
the explicit `-FromSource` gate. This source path does not establish Windows
product support.

The npm package `@sioyooo/repogrammar` is a thin launcher only. Its `bin`
entrypoint lives under `src/npm/`, detects OS/architecture, downloads the
matching GitHub Release artifact, verifies the `.sha256` checksum, caches the
binary and bundled worker assets under user-local state, and execs the Rust
`repogrammar` binary with the original arguments. It must not compile Rust
source, run `cargo`, implement RepoGrammar analysis behavior in JavaScript, call
real native Codex/Claude CLIs in tests, or become the only installation path.
Its artifact download follows a bounded number of HTTP redirects and resolves
relative `Location` headers against the current URL, so a redirect loop cannot
recurse until the process stack overflows.
`npx` and global npm installs require Node/npm by definition, but they must not
require Rust/Cargo.
`package.json` admits only `darwin` and `linux`, with `x64` and `arm64` for the
four release targets above. A root npm `libc` field cannot encode Linux-only
glibc while leaving Darwin applicable: npm also evaluates that field on Darwin,
where the detected libc is undefined. Therefore the cross-platform package
must omit root `libc`; the launcher is the fail-closed Linux libc/version
authority before download. The manifest also carries canonical
repository, homepage, and issue URLs because the packed README links to
unbundled project documentation through absolute GitHub URLs. The npm launcher
must reject every Windows
architecture with an explicit macOS/Linux-only release boundary before using a
release artifact or a local binary override. On Linux it must also reject musl,
an unknown libc, or glibc below the architecture-specific minimum before using
an override or downloading an artifact.

The npm launcher activates a staged cache directory without deleting an
existing destination after a rename conflict. If another process completed the
same first install, the loser accepts that complete install and removes only
its own staging state. An incomplete or foreign conflicting destination is
preserved and reported; rollback may restore or remove only this process's own
backup.

Npm dogfood uses either a local packed package or a direct binary override:

- `npm_config_cache=/tmp/repogrammar-npm-cache npm pack --dry-run` for the
  package-content smoke;
- `npm pack` followed by
  `npm install -g ./sioyooo-repogrammar-0.2.0.tgz`;
- `REPOGRAMMAR_BINARY=/absolute/path/to/repogrammar node src/npm/repogrammar.js ...`.

`REPOGRAMMAR_BINARY` is a local dogfood bypass only. It must be an absolute
path to an existing file and must not change the release-artifact default for
published npm use.

The deterministic package gate must create the real `.tgz` in a temporary
directory, inspect its exact file set and metadata, install it into an isolated
prefix offline, and execute the installed `repogrammar` wrapper against local
fake release assets. Temporary tarballs must never remain in the repository.
Preview publication stages the exact already-smoked tarball through the same
OIDC trusted-publisher and 2FA approval boundary as stable publication, with npm
dist-tag `preview`; it never uses a traditional write token or relies on the
default `latest`. npm requires a `latest` dist-tag; while the package has
no stable published version, the registry may map `latest` to the exact bounded
preview version. That preview-only state does not promote the package to stable
or authorize unversioned installation. Preview instructions must pin the exact
prerelease or `@preview`. Stable publication stages the exact already-smoked
tarball with dist-tag `latest`, preserves `preview=0.2.0-preview.0`, and requires
the public finalizer to prove both tags. Once a stable version exists, any
prerelease-valued `latest` fails closed.

The npm Trusted Publisher identity is exact: owner `SioYooo`, repository
`RepoGrammar`, workflow `release.yml`, and GitHub environment `npm-release`.
That environment has a required human reviewer and a deployment-branch rule
restricted to tags matching `v*`; the npm publisher permits staged publication
only (`--allow-stage-publish`). The workflow records raw stage output only in
the protected job log. Maintainers record the tag-run id, successful run
attempt, and npm stage id in the private release record without claiming those
values are a public artifact.

The stable finalizer treats the public
`npm-candidate-manifest.json` GitHub asset as candidate authority. Before
running any public npm package or launcher it compares the registry-fetched pack
metadata and SRI with that manifest. It then verifies the public Linux archive,
the public shell installer, pinned and unversioned live repository-only setup,
and the preserved preview version. Native coding-agent integration and a fresh
agent's instruction behavior require separate isolated pre-release/manual
evidence; the read-only finalizer does not claim to exercise either one.

Contributor release-readiness smoke may run `repogrammar install --target all
--scope global --dry-run` and `repogrammar uninstall --target all --scope
global --dry-run` to verify planner boundaries without writing agent
configuration or repository indexes. These dry-run planner commands currently
produce human-readable output only; callers must not pass `--json` unless a
future installer contract explicitly adds JSON output.

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
lists such as `codex,claude-code`. In the current stable/preview product, live writes
are implemented only for global Codex and global Claude Code. Other registry
targets are configuration-preview/deferred targets until their idempotent
writer, ownership receipt, uninstall inverse, and tests are implemented.

`repogrammar install` and `repogrammar uninstall` configure agent integration
only. They must not create, update, or delete `.repogrammar/`, and they must not
remove project indexes, logs, caches, locks, or repository-local receipts.
They must not run `init`, `index`, `sync`, or `resync`.
Agent-safe bootstrap is explicit and per repository: after machine-level
installation, an agent may run `repogrammar init --yes` only when the user has
allowed repo-local analysis state. That command does not broaden `init` writes,
does not start auto-sync, and does not imply telemetry consent, but it builds or
refreshes the active index by default. Agents may add `--autosync` when the user
wants coding-agent edits to enter RepoGrammar results without repeated manual
`resync`. Use `--state-only` only for lifecycle repair that must not index.

Repository lifecycle state is owned by `repogrammar init`,
`repogrammar index`, `repogrammar sync`, `repogrammar resync`, and
`repogrammar uninit`.

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
- verify the native entry is present and still points at the expected installed
  executable with exactly the `serve` argument after configuration;
- run the product binary's bounded `tools/list` self-test again after native
  configuration and receipt creation, before reporting success;
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

The current managed content version is `2`. The block and MCP initialize
guidance share one authoritative pre-flight contract. After mandatory
repository authority and instruction documents have been read, the gate applies
when `.repogrammar/` exists and an implementation, fix, refactor, test, or
diagnosis requires a repository-local contract or convention, repeated
implementation, framework role, or analogue comparison. Covered work explicitly
includes root-cause repair and schema, protocol, API, prompt-output, or Meaning
Contract qualification, conformance, or drift. File type and an exact target do
not exempt a mixed task, such as a YAML-generated prompt that must conform to a
repeated Meaning Contract.

For covered work the agent calls `repogrammar_context` exactly once before
CodeGraph or source search/read, with `operation: "find_analogues"`, a target
built from the concrete repo-relative path, symbol/member id, framework role,
or code-work question in the task, and `mode: "compact"`. It consumes the
returned `read_plan` and may fall back when the tool is unavailable or the
result explicitly reports `UNKNOWN`, `FALLBACK`, stale, omitted, or
insufficient evidence. The agent records that fallback reason before
proceeding and does not repeat an identical context call unless the target or
indexed evidence changed. CodeGraph may then supply exact source or call-path
detail not supplied by RepoGrammar.

Instruction-file synchronization does not guarantee hot reload for agent
sessions that are already running. After a successful live `repogrammar
instructions sync`, the CLI recommends starting a new coding-agent session; an
older session may retain the instruction snapshot it loaded at startup.

The gate is skipped for pure prose documentation; operational release, Git,
environment, or credential inspection; syntax-only YAML or configuration
validation; and an exact one-symbol, file, or call-path lookup only when no
repository contract, convention, repeated implementation, framework role,
analogue comparison, code-behavior diagnosis, or implementation decision is
involved. These exceptions do not override a covered contract-conformance
subtask.

The installer must not overwrite unrelated user instructions. `uninstall`
reverses only RepoGrammar's own managed write. If a file has a malformed or
incomplete managed section, the installer must stop and direct the user to a
manual repair workflow. A complete marker pair alone is not proof of ownership:
only the exact current content or an exact previously shipped legacy body is
recognized. A modified or unknown body inside the reserved markers is
`foreign`; it is preserved and refused for automatic refresh or removal. The
managed block also treats returned family ids as follow-up handles, uses
`show_family` only with an exact returned id, avoids `include_source_spans` and
`repogrammar stats` by default, and never silently initializes, resyncs, or
starts autosync.

The managed instruction writer is implemented as a reversible, idempotent
operation:

- it creates the file with the managed section when the file is absent;
- it appends the managed section, preserving prior content, when the file exists
  without markers;
- it replaces the section in place only when the body is an exact known older
  RepoGrammar content version;
- it reports an unchanged result when the section is already byte-equivalent;
- it refuses to modify a file with malformed, partial, duplicated, modified, or
  foreign marker content;
- it writes atomically through a sibling temp file plus rename and re-reads the
  file to verify the managed section before reporting success;
- on Unix it restores the pre-existing file mode, owner uid, and group gid on
  the open temporary handle before activation, or fails without activating it;
- on Unix any failure cleanup keeps that creating handle open while comparing
  the sibling pathname's device, inode, and link-count identity and unlinking
  only the matching operation-owned temporary file. Unlinking the original
  pathname changes the live handle's link count, while retaining the handle
  prevents ordinary inode reuse until the cleanup decision is complete;
- `uninstall` and rollback reverse exactly the recorded `instruction_action`:
  they remove the managed section and preserve unrelated user content; when
  RepoGrammar created the file (`instruction_action: "created"`) and stripping
  the section leaves it empty, they also delete the file so no empty artifact is
  left behind, but a file that pre-existed the install or gained user content
  after creation keeps its remaining content and is never deleted.

This is crash-safe replacement, not a cross-process compare-and-swap. A hostile
or independently authorized same-directory process can still replace the
destination or sibling temporary pathname between validation and rename. That
concurrent-mutation scenario is unsupported; callers must not run competing
instruction writers, and a release must not claim stronger confinement without
the native evidence required by ADR-0023.

Because real Codex/Claude global instruction-file locations are not yet verified,
live instruction writing is deferred by default. The installer resolves a
target's instruction-file path only from the explicit environment override
`REPOGRAMMAR_INSTRUCTION_FILE_<TARGET>` (for example
`REPOGRAMMAR_INSTRUCTION_FILE_CODEX`) and only when it resolves to an absolute
path. When no path is resolved, the receipt records `instruction_action:
"deferred"` and no file is written. RepoGrammar never guesses an instruction-file
path.

Users may inspect or refresh instruction guidance independently from native MCP
registration with:

```text
repogrammar instructions status --file <explicit-path> [--json]
repogrammar instructions sync --file <explicit-path> [--dry-run] [--yes] [--json]
repogrammar instructions remove --file <explicit-path> [--dry-run] [--yes] [--json]
```

The path is mandatory and may be relative to the current directory or absolute;
the command never guesses a target-specific location. `status` is read-only and
reports `missing`, `current`, `outdated`, `foreign`, or `malformed`. A live
`sync` or `remove` requires `--yes`; `--dry-run` reports the exact
low-cardinality action without writing. JSON reports expected and detected
content versions, before/after state, action, refusal class, and change booleans
without echoing the file path. Sync creates/appends a missing block or refreshes
only an exact known legacy version. Remove strips only an exact known current or
legacy section and preserves all other content. Foreign or malformed sections
are preserved and fail closed. These commands do not create `.repogrammar/`,
run indexing, or require consuming repositories to mirror `AGENTS.md` and
`CLAUDE.md`; each file must be selected separately and explicitly.

Consuming repositories must not be forced to mirror RepoGrammar's own
`AGENTS.md` and `CLAUDE.md` policy.

## Current implementation status

The current implementation supports deterministic dry-run planning,
noninteractive live writes, and a dependency-light text wizard:

- stable and preview release packaging is defined by the release workflow for
  macOS arm64/x86_64, glibc 2.35+ Linux x86_64, and glibc 2.39+ Linux arm64
  only, each with a bundled Python worker asset and `.sha256` checksum. Docs
  decide availability through exact-version GitHub/npm checks rather than a
  statement that becomes stale when the tag publishes;
- `src/install/repogrammar-install.sh` is the macOS/Linux installer wrapper. By
  default it downloads a prebuilt release artifact instead of requiring Cargo,
  verifies the checksum, validates archive entry names before extraction,
  installs the bundled worker asset plus CLI, and can then launch agent wiring
  or uninstall flows. Install/update paths also prune stale PATH copies whose
  checksum differs from the managed executable authority. In a source checkout,
  its interactive menu makes the contributor source-build path first-class, and
  its noninteractive `--from-source` mode supports dogfood before release
  artifacts exist;
- `src/install/install.ps1` is a Windows contributor/source-dogfood wrapper,
  not a stable or preview release asset. It has no release-download branch and
  fails install actions unless `-FromSource` was passed explicitly. Its
  interactive and noninteractive source modes support contributor dogfood. Its
  `-Verify` switch is a read-only report that compares, by SHA256, the
  `repogrammar` copies on PATH, the configured agent MCP command targets, and
  any running serve processes
  against the managed authority binary, so a user can confirm that the command
  they invoke and the binary their agents run are the same build. `-Prune`
  additionally removes PATH copies whose hash differs from the authority, after
  confirmation unless `-Yes` is passed, and never deletes copies that match the
  authority. If removal is blocked, the script exits nonzero and reports the
  unremoved path. Install/update paths run the same stale PATH cleanup
  automatically.
  `-Purge` performs a full uninstall: it prints a plan, stops only the
  repogrammar processes that run the binaries it is about to delete, runs
  `uninstall --target all` to remove agent MCP entries and receipts, optionally
  runs `uninit` on `-Project` to remove that repository's `.repogrammar` state,
  and then deletes every repogrammar binary, worker asset, and the managed data
  directory, after confirmation unless `-Yes` is passed;
- `repogrammar install` with no flags launches a TUI-style wizard when running
  in an interactive terminal;
- the wizard presents Codex and Claude Code, supports multi-select in one run,
  detects existing RepoGrammar-owned receipts, uses `a` as the default
  automatic selection, selects detected not-yet-managed agents through that
  default, reports a no-op when that set is empty, leaves undetected unmanaged
  agents unselected unless explicitly chosen, and skips already managed agents
  during live writes;
- the wizard keeps anonymous telemetry opt-in default-no, but the final
  reviewed installation plan confirmation is default-yes so pressing Enter
  proceeds after the plan is shown;
- the installer has a target registry for Codex, Claude Code, Cursor,
  opencode, Hermes, Gemini, Antigravity, and Kiro, exposed through a per-target
  adapter contract (`TargetAdapter`) that consolidates scope support, live-writer
  status, the no-write config preview, and the native MCP plus instruction-file
  plan lines (`describe_paths`). The current registry exposes deferred targets
  through dry-run and `--print-config` snippets only; live writes remain
  implemented for global Codex and global Claude Code;
- re-running the wizard can add detected missing supported agents later or
  refresh the RepoGrammar-managed command path when every supported concrete
  agent is already managed. `OwnedCurrent` targets do not rerun native add;
  `OwnedOutdated` targets are explicitly reconfigured instead of being
  misclassified as already current;
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
  when possible and points agent MCP entries at the installed command binary.
  When a RepoGrammar-managed agent entry already exists but records an executable
  that no longer matches the current authority (for example after the install
  data directory changed), install re-points that entry at the authority by
  removing and re-adding the native entry and rewriting the receipt, instead of
  skipping it. A managed entry already pointing at the authority stays untouched.
  Install execution reports absent integrations created by the run under
  `configured_targets` and obsolete pre-existing integrations refreshed by the
  run under `reconfigured_targets`; only `configured_targets` is eligible for
  an outer setup rollback. Before refreshing an outdated target, install
  captures its native entry, receipt, receipt backup, and managed-instruction
  files so any refresh failure restores the pre-existing integration rather
  than deleting it.
  Repeated installs stage the new binary, delete the previous
  RepoGrammar-managed binary or managed command copy, and then activate the new
  file; if deletion is blocked by a running coding agent or MCP process, install
  fails with guidance to exit that agent before rerunning the install or build
  command.
  If the selected command path is the same executable currently running
  `repogrammar install` (for example a local Cargo-installed
  `repogrammar.exe` on PATH), the installer may copy that executable into
  RepoGrammar-managed user state instead of treating the command path as a
  foreign conflict. It must not overwrite that currently executing command path
  during the same run, including wrapper flows that set
  `REPOGRAMMAR_EXECUTABLE` to a managed data-directory binary while launching
  `repogrammar install` through the user-writable command path; unrelated
  existing command paths are still refused. The installer likewise skips
  replacing the managed installed executable under the data directory when that
  file is the currently running process, instead of attempting an overwrite the
  operating system would reject;
- the `@sioyooo/repogrammar` launcher supports `npx @sioyooo/repogrammar ...`
  after package publication by downloading and caching the matching prebuilt
  release artifact, then delegating all behavior to the Rust binary;
- install reports an advisory environment self-check: when multiple
  `repogrammar` executables are discoverable on PATH, or when the PATH-resolved
  `repogrammar` is not the RepoGrammar-managed command, it prints the conflicting
  paths and convergence guidance. The self-check is advisory only and never
  blocks or fails the install;
- install runs a read-only MCP self-test before native agent configuration, with
  a bounded timeout that kills and reaps a hanging self-test process;
- before command-path or native writes, install probes each selected live target
  with its bounded, read-only native `mcp get` command. Only the target's exact documented
  not-found response is absence. Unexpected failed output or unreadable output
  is unknown and fails closed. An unparseable successful response becomes a
  preserved malformed state: direct install refuses writes, while setup
  continues repository-only and recommends `repogrammar doctor`. Neither path
  echoes native output, paths, or credentials;
- ownership requires the native scope, executable, and `serve` arguments to
  match the RepoGrammar receipt. A same-name entry without a receipt is foreign;
  a receipt paired with a missing or mismatched native entry is drifted. Both
  cases block automatic repair and preserve the existing native entry, receipt,
  and command path;
- install writes one RepoGrammar-owned receipt per configured target under the
  user install data directory after native configuration succeeds, recording the
  resolved instruction-file path and instruction action;
- after writing each receipt, install re-probes the native entry for exact
  presence and runs the installed product binary's bounded exact-one-tool
  `tools/list` self-test. Failure removes entries, receipts, instruction
  sections, and command files newly created by that install run, while restoring
  any pre-existing owned target that was being refreshed from its snapshot;
- the managed instruction-file writer (create/append/replace/idempotent/remove,
  atomic temp+rename with re-read verification, version/content drift
  classification, and malformed/foreign refusal) is implemented and tested.
  Live install-time instruction writes stay deferred unless
  `REPOGRAMMAR_INSTRUCTION_FILE_<TARGET>` resolves to an absolute path; the
  independent explicit-file `instructions status|sync|remove` path provides
  dry-run, path-free JSON, confirmation, and reversible removal without native
  agent reconfiguration;
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
