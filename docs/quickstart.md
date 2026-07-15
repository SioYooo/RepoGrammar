# Quickstart

RepoGrammar does not have verified public prerelease assets or a published npm
package as of 2026-07-16. The current supported path is source-checkout
contributor dogfood. The future release path below is deliberately labeled as
unavailable until real assets are verified.

## 1. Acquire The Command From Source

```text
git clone https://github.com/SioYooo/RepoGrammar.git
cd RepoGrammar
cargo build --release
bash src/install/repogrammar-install.sh --install-cli-only --from-source --yes
repogrammar version
```

This acquisition step requires Rust/Cargo because it builds from source. The
installed end-user command itself does not require Node.js, npm, Docker, the
SQLite CLI, a local model, an embedding model, an OpenAI API key, or a cloud
API. The current Python preview requires `python3` at runtime.

## 2. Run One Setup Command

Change into the repository you want to analyze:

```text
cd /path/to/your/repo
repogrammar setup --target auto
```

Setup presents one plan and requests one confirmation. It then:

1. detects a supported agent with a reversible writer;
2. configures only RepoGrammar-owned MCP state when possible;
3. creates or reuses safe repository-local state;
4. builds the active index;
5. starts auto-sync unless `--no-autosync` is present;
6. runs a read-only product MCP self-test; and
7. prints readiness, one limitation or recovery action, and one question to ask.

No detected Codex or Claude Code CLI is not fatal: setup still completes the
repository-only path and tells you how to add a supported agent later. Setup
never enables telemetry.

Useful variants:

```text
# Preview the complete plan without any writes.
repogrammar setup --target auto --dry-run

# Explicit noninteractive authorization for automation.
repogrammar setup --target auto --yes

# Initialize and index without a background refresher.
repogrammar setup --target auto --yes --no-autosync

# Stable machine-readable final output and no progress stream.
repogrammar setup --target auto --yes --json --progress never
```

Re-running setup is idempotent. It preserves valid pre-existing machine and
repository state. If a later stage fails, rollback removes only machine-level
integration written by that setup attempt; it does not delete an existing
index or foreign agent configuration.

## 3. Ask For Repository Context

In a configured coding agent, try the question printed by setup:

```text
How are API routes implemented in this repository?
```

Or use the CLI directly:

```text
repogrammar families
repogrammar find --project . --token-budget 8000 <repo-relative-path-or-symbol>
repogrammar explain --project . --token-budget 8000 <repo-relative-path-or-symbol>
repogrammar check --project . --token-budget 8000 <repo-relative-path-or-symbol>
```

Default output is metadata-only. Add `--include-source-spans` only when bounded
line-numbered source is needed. `UNKNOWN` and `PARTIAL_CONTEXT` are truthful
outcomes, not setup failures; follow the single recovery action or use ordinary
source search for unsupported evidence.

## Supported Platforms

The planned public-preview binary matrix is:

- macOS arm64 and x86_64;
- Linux arm64 and x86_64; and
- Windows x86_64 preview.

Windows ARM64 is not in the preview matrix. Live agent writers currently cover
global Codex and global Claude Code only. Source-checkout installation has been
tested locally on Unix-like paths; Windows source acquisition uses
`src/install/install.ps1` and remains subject to the Windows proof recorded in
the [install proof matrix](reports/public-preview-install-proof-matrix.md).

## Verify The Journey

From the RepoGrammar checkout:

```text
cargo test --lib application::setup::tests
cargo test --lib interfaces::cli::tests
node src/npm/repogrammar.test.js
bash src/install/repogrammar-install.test.sh
cargo run --quiet --bin repo-guard -- check
```

For a safe product smoke, build the binary, use an isolated temporary HOME and
target repository, and run `setup --dry-run --json --progress never`. Verify
that stdout is one JSON value, stderr is empty, `.repogrammar/` is absent, and
the temporary HOME remains empty. The complete release gate is in the
[public-preview release checklist](release/public-preview-release-checklist.md).

## Future Published Path — Not Available Yet

After the exact preview release assets and npm package are independently
verified, a new user should not need the source checkout or Rust/Cargo:

```text
npx @sioyooo/repogrammar setup --project /path/to/your/repo --target auto
```

Today this command is a future path, not an installation claim: npm currently
returns package-not-found and no GitHub preview assets have been verified. The
npm wrapper is a thin downloader/launcher for checksummed release binaries; it
does not compile RepoGrammar or implement analysis in JavaScript.

See the [Codex quickstart](quickstart-codex.md) and
[Build Week demo](demo/build-week-demo.md) for the agent-specific and
submission-ready flows.
