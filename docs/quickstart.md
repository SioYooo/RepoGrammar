# Quickstart

This guide does not infer publication from its own version. Use the exact
availability gate below; if either registry check fails, use source-checkout
contributor dogfood.

## 1. Verify And Acquire Exact 0.2.0

```text
npm view @sioyooo/repogrammar@0.2.0 version
curl -fsSI https://github.com/SioYooo/RepoGrammar/releases/download/v0.2.0/install.sh.sha256
npx @sioyooo/repogrammar@0.2.0 version
```

Continue with the no-build path only when both registry checks succeed. The npm
launcher requires Node/npm for acquisition, but the installed product does not
require Rust/Cargo, Docker, the SQLite CLI, a local model, an embedding model,
an OpenAI API key, or a cloud API. The bounded Python analyzer requires Python
3.10 or newer as `python3` at runtime.

If either registry check fails, use contributor source acquisition:

```text
git clone https://github.com/SioYooo/RepoGrammar.git
cd RepoGrammar
cargo build --release
bash src/install/repogrammar-install.sh --install-cli-only --from-source --yes
repogrammar version
```

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
7. prints each current limitation or recovery action, and prints a coding-agent
   question only when at least one native agent integration is verified ready.

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

The stable `0.2.0` binary matrix is:

- macOS arm64 and x86_64;
- glibc 2.35+ Linux x86_64; and
- glibc 2.39+ Linux arm64.

Musl-based Linux, older glibc, and Linux systems where glibc cannot be confirmed
fail closed before download. Windows is not in the stable matrix. Live agent writers currently cover
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
[stable release checklist](release/stable-v0.2.0-release-checklist.md).

## Exact Published-Version Gate

First verify the exact npm version, complete npm channel mapping, and matching
GitHub asset:

```text
npm view @sioyooo/repogrammar@0.2.0 version
npm view @sioyooo/repogrammar dist-tags --json
curl -fsSI https://github.com/SioYooo/RepoGrammar/releases/download/v0.2.0/install.sh.sha256
```

Continue only when the version and GitHub checks succeed and the dist-tag object
contains exactly `"latest":"0.2.0"` and
`"preview":"0.2.0-preview.0"`. The pinned no-build path is:

```text
npx @sioyooo/repogrammar@0.2.0 setup --project /path/to/your/repo --target auto
```

After publication verification, unversioned `npx @sioyooo/repogrammar` uses
npm `latest` and must resolve the same `0.2.0`. The separate `@preview` tag
continues to resolve immutable `0.2.0-preview.0`; use the exact stable version
for reproducible automation.

If any check fails, stay on the source path above. The npm wrapper is a thin
downloader/launcher for checksummed release binaries; it does not compile
RepoGrammar or implement analysis in JavaScript.

See the [Codex quickstart](quickstart-codex.md) and
[Build Week demo](demo/build-week-demo.md) for the agent-specific and
submission-ready flows.
