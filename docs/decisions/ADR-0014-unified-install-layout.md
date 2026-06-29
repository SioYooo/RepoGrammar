# ADR-0014: Unified install layout and single command authority

- Status: Accepted
- Date: 2026-06-30

## Context

RepoGrammar ships several acquisition paths: the `install.ps1` and
`repogrammar-install.sh` wrappers, the direct `repogrammar install` agent-wiring
command, and contributor bypasses (`cargo install --path`, the npm package).
Historically these resolved different default directories. On Windows this
produced multiple `repogrammar.exe` copies across `~/.cargo/bin`, the npm bin,
`%LOCALAPPDATA%\Programs\RepoGrammar\bin`, `%LOCALAPPDATA%\RepoGrammar\bin`, and
`~/.local/share/repogrammar/bin`, at different versions. `install.ps1` defaulted
its install directory to `%LOCALAPPDATA%\RepoGrammar` while the Rust installer
and the Unix wrapper default to `~/.local/share/repogrammar`.

Because each path updated only one copy, agent MCP entries written by different
installers pointed at different executables and versions. Reinstalls therefore
looked like "installed but not in effect", and reinstalling over a copy that was
the running command (or a live MCP `serve` process) failed with a Windows
sharing violation (`os error 32`).

## Decision

- Single authority. `repogrammar install` installs the managed executable into
  the Rust-resolved data directory (`$XDG_DATA_HOME/repogrammar/bin`, otherwise
  `~/.local/share/repogrammar/bin`) and every agent MCP registration points at
  that managed executable. This is the one authority on all platforms.
- First-party installers align. `install.ps1` and `repogrammar-install.sh` must
  resolve the same default data directory and pass `REPOGRAMMAR_INSTALL_DIR`,
  `REPOGRAMMAR_COMMAND_DIR`, and `REPOGRAMMAR_EXECUTABLE` consistently, so a
  release install and a source build land on the same authority.
- `cargo install` and npm are contributor/bypass acquisition paths, not the
  managed authority. Documentation must state that mixing them with the managed
  installers creates multiple copies.
- Install-time self-check. `repogrammar install` reports when multiple
  `repogrammar` executables are discoverable or when an existing agent MCP entry
  points outside the authority, with convergence guidance. No new top-level
  command is added; the CLI stays pattern-family-first and the self-check rides
  on `install`.
- Self-lock safety stays within the existing replacement contract (stage, remove
  the previous managed file, then activate; fail with guidance when a live agent
  or MCP process holds the file). The installer additionally skips replacing any
  managed file that is the current process executable, and path-identity checks
  must stay correct on Windows when canonicalization fails. Rename-aside (leaving
  the old binary beside the new one) remains disallowed per ADR-0007 and
  `docs/specifications/installation.md`.

## Alternatives considered

- Force every installer to build from source: rejected; end users must not need
  Rust or Cargo (`installation.md`).
- Adopt rename-aside to overwrite a running executable: rejected; it conflicts
  with the replacement contract that forbids keeping an alternate new binary
  beside the old active one.
- Make `~/.cargo/bin` or the npm bin the authority: rejected; they are not
  reversible RepoGrammar-managed paths and cargo's location is fixed.
- Add a machine-level `doctor`/`audit` command for copy detection: rejected;
  `doctor` is repo-local and a new top-level command violates the
  pattern-family-first CLI rule.

## Consequences

- `install.ps1`'s Windows default layout changes to the shared authority.
  Existing users with binaries under `%LOCALAPPDATA%\RepoGrammar` are guided to
  converge by the install-time self-check.
- Agent MCP entries written by different installers now converge on the same
  managed executable.
- autosync run-state observability (last-sync timestamp) is tracked in the same
  workstream but is an independent change.

## Follow-up work

- Optional migration helper to remove stale unmanaged copies after confirmation.
- Revisit npm and cargo bypass messaging once public release artifacts exist.
