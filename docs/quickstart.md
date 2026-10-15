# Quickstart

Published 0.5.0 uses the explicit CLI, agent, and repository steps below.
Follow Install → Initialize a repository → Connect a coding agent → Verify setup.
The [one-command candidate](#one-command-candidate-unreleased) is available
only with reviewed local artifacts until a new release is published.
Python 3.10+ within 3.x, Bash, curl, tar, and gzip are required for the
complete Python onboarding flow; Git identifies the selected repository.
Rust, Cargo, Node.js, npm, Docker, API keys, and cloud models are not required
for release-binary installation.

## Install

The commands below install the verified GitHub binary release `0.5.0`.
This version is not published to npm; use these GitHub assets or the
[source installation](#source-installation).

Copy this block on macOS or glibc-based Linux. It verifies the installer before
execution, then the installer verifies the matching native archive:

```bash
(
set -eu
install_tmp="$(mktemp -d)" || exit 1
trap 'rm -rf "$install_tmp"' EXIT
cd "$install_tmp" || exit 1
curl -fsSLO https://github.com/SioYooo/RepoGrammar/releases/download/v0.5.0/install.sh || exit 1
curl -fsSLO https://github.com/SioYooo/RepoGrammar/releases/download/v0.5.0/install.sh.sha256 || exit 1
if command -v sha256sum >/dev/null 2>&1; then
  sha256sum -c install.sh.sha256 || exit 1
else
  shasum -a 256 -c install.sh.sha256 || exit 1
fi
bash install.sh --version v0.5.0 --install-cli-only --yes
) &&
export PATH="$HOME/.local/bin:$PATH"
```

Run `repogrammar version` to check the installation. Add the `export PATH` line
once to your shell configuration if `$HOME/.local/bin` is not already on PATH.
Installation does not connect an agent or create a repository index.

Supported archives cover macOS arm64/x86_64, Linux x86_64 with glibc 2.35+, and
Linux arm64 with glibc 2.39+. Windows and musl Linux are not supported.

## Initialize a repository

```bash
cd /path/to/repository &&
repogrammar init &&
repogrammar status
```

Commands use the current directory by default. `init` creates `.repogrammar/`,
builds the first index, and starts that repository's autosync daemon. You do not
need a separate `index` command. There is no global repository scanner.

For scripts or CI, use `repogrammar init --yes --no-autosync --progress never`.
Use `--project /path/to/repository` when operating from another directory.

| Command | When to use it |
|---|---|
| `repogrammar init` | First use in a repository; creates state and builds the index. |
| `repogrammar sync` | Refresh ordinary edits immediately instead of waiting for autosync. |
| `repogrammar resync` | Rebuild analysis when recovery guidance requests it or after analysis changes. |

Autosync is best-effort. Follow `repogrammar status` or `repogrammar doctor`
recovery guidance if the index is unavailable or stale.

## Connect a coding agent (optional)

```bash
repogrammar install --target auto --scope global --yes --no-telemetry
```

This configures detected supported agents; it does not initialize repositories.
Restart the agent session afterward. For a specific client, follow the
[Codex](quickstart-codex.md), [Claude Code](quickstart-claude.md), or
[opencode](quickstart-opencode.md) guide. Global instruction-file synchronization
follows the version-specific guidance in those guides: current source can add
receipt-owned conditional Codex/Claude guidance, while published 0.5.0 retains
its shipped override-only behavior. Use current source's `--no-instructions`
to skip instruction writes.

## Verify setup

From the initialized repository, check the command selected by your shell and
the repository status:

```bash
command -v repogrammar
repogrammar version
repogrammar status
```

For the published installation above, the version should be `0.5.0`. If status
reports an unavailable or stale index, run `repogrammar doctor` and follow its
recovery guidance. For an agent integration, restart the agent and confirm
that `repogrammar_context` appears in its available MCP tools. CLI installation,
repository readiness and a working agent session are separate checks.

## Find repository conventions

```bash
repogrammar find "FastAPI route" --mode compact --verbosity minimal
repogrammar check "path/to/file.py:LINE" --mode compact --verbosity minimal
```

Read the returned `read_plan`. `UNKNOWN` and `PARTIAL_CONTEXT` identify evidence
limits or recovery steps; static alignment does not prove runtime equivalence.

## One-command candidate (unreleased)

`--quickstart` is implemented in current source but **not published in 0.5.0**.
The existing stable binary does not advertise `quickstart-v1`, so using the new
installer with that release stops before persistent installation changes.
Do not use the proposed raw-main URL as a working onboarding command yet.
Publication requires a new immutable installer and matching native binaries;
see the [release checklist](release/quickstart-release-checklist.md).

This route handles the separate Install, Initialize and Connect steps above.
After a successful quickstart, go straight to [Verify setup](#verify-setup).

To exercise a reviewed local candidate, obtain its native archive and matching
`.sha256` file, then enter the Git repository you intend to initialize:

```bash
REPOGRAMMAR_RELEASE_DIR=/absolute/path/to/candidate-artifacts \
  bash /absolute/path/to/RepoGrammar/src/install/repogrammar-install.sh --quickstart
```

The installer checks the platform and Python, downloads or reads the verified
archive into temporary storage, validates ownership, and prints one plan for
the CLI, global agent integration, instruction changes, repository index and
autosync. A nested working directory resolves to its Git root, displayed in
the plan. Confirming that plan authorizes those writes. EOF never grants
consent. When the script is piped into Bash, confirmation reads `/dev/tty` and
delegated commands cannot consume the script body as answers.

Options use the same installer path and candidate artifact environment above:

| Option | Effect |
|---|---|
| `--dry-run` | Show the plan without persistent writes. May download, verify and execute the candidate from temporary storage. |
| `--yes` | Explicit noninteractive authorization; no terminal is required. Without it, a live run requires a controlling terminal. |
| `--project "/path/to/repository"` | Select an existing Git worktree; its resolved root is shown before authorization. |
| `--target auto` | Default: select detected agents with live writers. |
| `--target codex`, `--target claude-code`, `--target opencode` | Select one supported agent explicitly. Missing agents are reported. |
| `--target none` | Skip agent configuration; CLI and repository setup remain available. |
| `--skip-repository` | Install the CLI and selected agents without repository initialization or autosync. |
| `--no-autosync` | Build or refresh the index without starting autosync. |
| `--no-instructions` | Configure MCP without adding or updating instruction-file sections. |
| `--scope global` | The supported live agent scope. Local/project scope is deferred and rejected before writes. |
| `--version TAG` | Pin a release that includes `quickstart-v1`; default acquisition selects GitHub's latest stable release. |
| `--command-dir PATH`, `--install-dir PATH` | Explicit absolute managed installation paths. |

For example, noninteractive repository-only setup uses:

```bash
REPOGRAMMAR_RELEASE_DIR=/absolute/path/to/candidate-artifacts \
  bash /absolute/path/to/RepoGrammar/src/install/repogrammar-install.sh \
  --quickstart --yes --project "/path/to/repository" --target none --no-autosync
```

Outside a Git worktree, the default flow skips repository setup, retains
CLI/agent installation, and prints the next repository command. An explicit
invalid project fails. No home-directory repository scan occurs. A missing or
unsupported Python interpreter blocks repository onboarding before installation
writes; the CLI-only path remains available. To use another installed
interpreter, set `REPOGRAMMAR_PYTHON_EXECUTABLE=/absolute/path/to/python3`.
RepoGrammar never installs Python automatically.

Only Codex, Claude Code and opencode have live global integrations. Other known
target names are not supported quickstart integrations. An empty detected-agent
set still allows CLI/index installation. Existing foreign, malformed or drifted
agent configuration is preserved and reported. To inspect deferred local
configuration, use `repogrammar install --scope local --print-config codex`
instead of requesting a live local write.

Quickstart preserves existing telemetry preferences; telemetry is off by
default. Product MCP self-test, native agent configuration, repository index,
autosync and family evidence have separate readiness results. A written config
file does not prove that a running agent session has loaded MCP. Restart the
agent and verify its tool availability. `UNKNOWN` or zero supported families
does not become a successful family claim.

## Bootstrap trust and a pinned future entrypoint

Piping an installer into Bash executes it before any archive verification.
You trust the bootstrap distribution channel and its current script. Archive
SHA-256 checks detect corruption or mismatched bytes after that code is already
running; a checksum fetched from the same compromised source does not prove
publisher authenticity.

After a new release passes publication verification, prefer its immutable
release-hosted installer and pin the matching archive version. The following
is a **future template**, not a published installation command. Replace the
placeholder only with a reviewed new release tag; do not replace it with
`v0.5.0`:

```bash
quickstart_tag='REPLACE_WITH_NEW_QUICKSTART_RELEASE_TAG'
curl -fsSL "https://github.com/SioYooo/RepoGrammar/releases/download/${quickstart_tag}/install.sh" \
  | bash -s -- --quickstart --version "$quickstart_tag"
```

For verification before bootstrap execution, keep the current project as the
working directory and use this alternative after publication:

```bash
(
set -eu
quickstart_tag='REPLACE_WITH_NEW_QUICKSTART_RELEASE_TAG'
install_tmp="$(mktemp -d)"
trap 'rm -rf "$install_tmp"' EXIT
asset_base="https://github.com/SioYooo/RepoGrammar/releases/download/${quickstart_tag}"
curl -fsSL "$asset_base/install.sh" -o "$install_tmp/install.sh"
curl -fsSL "$asset_base/install.sh.sha256" -o "$install_tmp/install.sh.sha256"
if command -v sha256sum >/dev/null 2>&1; then
  (cd "$install_tmp" && sha256sum -c install.sh.sha256)
else
  (cd "$install_tmp" && shasum -a 256 -c install.sh.sha256)
fi
# Review install.sh and verify the release provenance before this execution.
bash "$install_tmp/install.sh" --quickstart --version "$quickstart_tag"
)
```

Compare checksums through a trusted independent channel when available. GitHub
release/asset attestations provide additional provenance evidence; see the
release checklist. A moving-branch raw script is optional convenience, not the
only supported distribution path.

## Updates, PATH and partial setup

For published 0.5.0, repeat its pinned CLI-only installation block to reinstall
the verified bytes. For a quickstart-capable release, rerun the same quickstart
command to update the managed product and reconcile its owned integrations.
Pin `--version` when repeatable acquisition matters. Changing the default
release selection does not change the npm channel: the GitHub version must not
be inferred from `npm latest` or a local package manifest.

Current-source quickstart preserves unrelated PATH commands. If another
`repogrammar` is found first, the final output names it and prints the managed
command and PATH correction. Inspect `command -v repogrammar` and
`repogrammar version`; use the printed absolute managed command while fixing
your shell profile. `hash -r` refreshes Bash's command cache. Quickstart does not
edit shell profiles or delete another package manager's installation.

An unmanaged file at the chosen command path blocks installation even with
`--yes`. Move it yourself, select another `--command-dir`, or explicitly choose
`--replace-unmanaged-command` to preserve a backup before replacement. Unsafe
paths, receipt drift and malformed ownership remain blockers. Legacy
`--install-cli-only` and `--install-and-configure` retain their documented stale
PATH-copy cleanup behavior; review their plan and scope before using them.

| Result | What remains and what to do |
|---|---|
| Download, checksum, archive or capability failure | No persistent quickstart installation changes. Check the release/tag, platform, network and matching checksum; 0.5.0 cannot supply `quickstart-v1`. |
| Acquisition ownership failure | Existing installation is preserved. Resolve the named collision or drift; do not delete receipts to force ownership. |
| Product installation or self-test failure | Review the acquisition transaction's rollback result. A rollback failure must be resolved before retrying. |
| Agent setup failure after acquisition | The verified CLI remains installed. Existing valid integrations remain owned; review setup's rollback and preservation report, resolve the blocker, then rerun. |
| Index failure | Setup reports retained repository state and any previous valid generation. Fix the reported cause before rerunning; do not remove a working index to manufacture a clean install. |
| Autosync failure after indexing | The completed index remains available. Follow the reported recovery action, such as `repogrammar autosync start --project /path/to/repository`. |
| Final MCP self-test failure | CLI/index preservation is reported separately from MCP readiness. Review the failure and use `repogrammar doctor --project /path/to/repository` where applicable. |
| No supported agent or no repository selected | CLI installation can finish with explicit limitations. Install/select an agent or run `repogrammar setup --project /path/to/repository` later. |

Reruns must not duplicate managed instructions, receipts, native MCP entries or
autosync daemons. Setup rollback is limited to changes created by that setup
attempt; it does not uninstall a refreshed pre-existing valid integration or
remove a successfully activated index. Agent permissions unrelated to
RepoGrammar are never granted by this flow.

## Remove an installation or index

| Command | Effect |
|---|---|
| `repogrammar uninstall --dry-run` | Preview removal of the managed machine installation. |
| `repogrammar uninstall --yes` | Remove that installation and its owned agent integrations; preserve repository indexes. |
| `repogrammar disconnect --target all --yes` | Remove agent integrations while keeping the CLI. |
| `repogrammar uninit --yes` | Remove the current repository's local RepoGrammar state. |

Product removal uses receipt-owned files and a post-exit cleanup helper. A
`finalizer_pending` result is not proof that cleanup finished; inspect the
reported finalizer result for `complete` or `partial`. Foreign files, other
PATH copies and repository indexes are preserved. Use an explicit
`repogrammar uninit --project "/path/to/repository" --yes` only for the index
you intend to remove.

## Source installation

From a RepoGrammar source checkout:

```bash
cargo build --release &&
bash src/install/repogrammar-install.sh --install-cli-only --from-source --yes &&
export PATH="$HOME/.local/bin:$PATH"
```

See [CLI reference](specifications/cli.md), [installation details](specifications/installation.md),
and [limitations](limitations.md) for the full contracts.
