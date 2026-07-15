# Install Layout Unification + Observability Plan

- Status: Active planning artifact
- Last updated: 2026-06-30
- Branch: `feat/install-layout-unify`
- Scope: Converge install authority on the Rust-managed data dir, add
  install-time copy/version self-check, harden the self-lock guard within the
  existing replacement contract, and surface autosync last-sync state.
- Related canonical docs: `docs/decisions/ADR-0014-unified-install-layout.md`,
  `docs/decisions/ADR-0007-safe-install-progress-telemetry.md`,
  `docs/specifications/installation.md`, `docs/specifications/cli.md`

This plan is not a replacement for the specifications or ADRs. If it conflicts
with an accepted ADR or specification, update the lower-priority text before
changing code.

## Motivation

On a real Windows machine five `repogrammar.exe` copies existed across
`.cargo\bin`, the npm bin, `%LOCALAPPDATA%\Programs\RepoGrammar\bin`,
`%LOCALAPPDATA%\RepoGrammar\bin`, and `~/.local/share/repogrammar/bin`, at three
versions. Codex MCP pointed at the `install.ps1` install dir; Claude MCP pointed
at the Rust default data dir; the PATH `repogrammar` resolved to `.cargo\bin`.
The root causes were (1) `install.ps1` and the Rust installer disagreeing on the
default data directory and (2) an older binary overwriting its own running
executable during refresh. ADR-0014 records the decision; this plan sequences
the work.

## Slices

Each slice is one atomic commit including its tests and documentation.

### Slice 1 — Unify install layout to the Rust authority

- `src/install/install.ps1`: default install dir and command dir resolve to the
  same authority the Rust installer uses (`~/.local/share/repogrammar`), instead
  of `%LOCALAPPDATA%\RepoGrammar` / `%LOCALAPPDATA%\Programs\RepoGrammar\bin`.
- `src/install/repogrammar-install.sh`: already `~/.local/share/repogrammar`;
  confirm and add a regression assertion only if needed.
- `docs/specifications/installation.md`: state the single authority and that
  first-party installers must resolve the same default data dir.
- `src/install/install.ps1.test.ps1`: update layout assertions.

### Slice 3 — Harden installed_executable self-lock guard (within contract)

- `src/rust/application/install.rs`: in `install_cli_command_with_current_process`,
  skip replacing `installed_executable` when it equals the current process
  executable; make `same_path` robust when `canonicalize` fails on Windows
  verbatim paths (lexical fallback).
- Tests beside the module for the current-exe-skip and fallback-compare cases.
- `docs/specifications/installation.md`: note the current-exe skip behavior.

### Slice 2 — Install-time environment self-check

- `src/rust/application/install.rs`: detect multiple discoverable `repogrammar`
  executables and agent MCP entries pointing outside the authority; carry typed
  warnings on `InstallExecutionOutcome`.
- `src/rust/interfaces/cli/mod.rs`: render warnings + convergence guidance in the
  `install` output (including `--dry-run`). No new top-level command.
- Tests + `docs/specifications/installation.md` update.

### Slice 4 — autosync last-sync timestamp

- `src/rust/application/autosync.rs`: persist run-state
  (`last_sync_unix`, `last_result`, `synced_generation`, `last_error`) written
  after each daemon sync; extend `AutosyncReport`; read it in
  `autosync_status_for_state`.
- `src/rust/bin/repogrammar.rs`: write run-state in `run_autosync_loop`.
- `src/rust/interfaces/cli/mod.rs`: render last-sync in `autosync status` and the
  top-level `status`.
- Tests + `docs/specifications/cli.md` / `installation.md` update.

### Slice 5 — install reconciles drifted managed agent entries

- `src/rust/application/install.rs`: `execute_install` re-points a managed agent
  whose receipt `executable_path` drifted from the current authority (native
  remove + add, receipt rewrite) instead of skipping it; an entry already at the
  authority stays skipped. Removes the need for a manual uninstall/reinstall to
  migrate agents after the authority moves.
- Tests + `docs/specifications/installation.md` + `ADR-0014` update.
- npm positioning (#2): the `@sioyooo/repogrammar` npm package is a launcher that
  downloads a release artifact and delegates to the Rust binary; it does not write
  a competing managed authority, and any on-PATH copy is already surfaced by the
  Slice 2 self-check. No npm code change is required; it stays a documented bypass
  like `cargo install`.

## Verification (gates completion of every code slice)

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace --all-features`
- `cargo run --quiet --bin repo-guard -- check`

## Risks / UNKNOWN

- Existing users under `%LOCALAPPDATA%\RepoGrammar` need convergence; the
  install-time self-check guides it, a destructive migration helper is deferred.
- `cargo install` / npm copies remain possible by design; only documented, not
  prevented.
- Windows `canonicalize` semantics on verbatim paths are the reason for the
  `same_path` fallback; the fallback is lexical and marked as best-effort.
