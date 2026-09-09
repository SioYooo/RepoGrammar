# Quickstart

Install the CLI once, then run `repogrammar init` inside each repository.
Python 3.10+, Bash, curl, tar, and gzip are required; Rust and Node.js are not.

## Install

The commands below target the `0.5.0` release candidate. Until its publication
and public verification finish, `0.4.3` remains the verified public release;
use the [source installation](#source-installation) for candidate testing.

Copy this block on macOS or glibc-based Linux. It verifies the installer before
execution, then the installer verifies the matching native archive:

```bash
(
set -eu
install_tmp="$(mktemp -d)"
trap 'rm -rf "$install_tmp"' EXIT
cd "$install_tmp"
curl -fsSLO https://github.com/SioYooo/RepoGrammar/releases/download/v0.5.0/install.sh
curl -fsSLO https://github.com/SioYooo/RepoGrammar/releases/download/v0.5.0/install.sh.sha256
if command -v sha256sum >/dev/null 2>&1; then
  sha256sum -c install.sh.sha256
else
  shasum -a 256 -c install.sh.sha256
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
cd /path/to/repository
repogrammar init
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
is a separate explicit action described in those guides.

## Find repository conventions

```bash
repogrammar find "FastAPI route" --mode compact --verbosity minimal
repogrammar check "path/to/file.py:LINE" --mode compact --verbosity minimal
```

Read the returned `read_plan`. `UNKNOWN` and `PARTIAL_CONTEXT` identify evidence
limits or recovery steps; static alignment does not prove runtime equivalence.

## Remove an installation or index

| Command | Effect |
|---|---|
| `repogrammar uninstall --dry-run` | Preview removal of the managed machine installation. |
| `repogrammar uninstall --yes` | Remove that installation and its owned agent integrations; preserve repository indexes. |
| `repogrammar disconnect --target all --yes` | Remove agent integrations while keeping the CLI. |
| `repogrammar uninit --yes` | Remove the current repository's local RepoGrammar state. |

## Source installation

From a RepoGrammar source checkout:

```bash
cargo build --release
bash src/install/repogrammar-install.sh --install-cli-only --from-source --yes
export PATH="$HOME/.local/bin:$PATH"
```

See [CLI reference](specifications/cli.md), [installation details](specifications/installation.md),
and [limitations](limitations.md) for the full contracts.
