# ADR-0030: Default auto-wired agent instruction files

- Status: Accepted
- Date: 2026-07-22
- Refines: ADR-0013, ADR-0014, ADR-0026, ADR-0028

## Context

`repogrammar install` and `repogrammar setup` register RepoGrammar as a
read-only MCP server and write a reversible product receipt. They also carry a
managed, marker-fenced instruction writer (ADR-0013) that adds a short
pre-flight gate telling an agent to consult `repogrammar_context` before
grep/find/manual reads.

Until this decision that writer was effectively dormant. The installer resolved
a target's instruction-file path only from an explicit
`REPOGRAMMAR_INSTRUCTION_FILE_<TARGET>` environment override, and only when it
was absolute. With no override the receipt recorded
`instruction_action: "deferred"` and nothing was written, on the rationale that
real Codex/Claude global instruction-file locations were "not yet verified".

The observed consequence is that a normal install wires the MCP server but never
tells the agent to prefer RepoGrammar, so agents (for example Codex) keep
defaulting to broad grep/read loops. The known default locations are in fact
stable and single: Codex reads `~/.codex/AGENTS.md` and Claude Code reads
`~/.claude/CLAUDE.md`. CodeGraph and comparable tools write their consumer
pre-flight guidance to the agent's global instruction file at install time; not
doing so leaves the onboarding contract (ADR-0026) incomplete.

Two boundaries must be preserved. First, the write must remain reversible,
idempotent, and refusal-safe: it must never overwrite a foreign or malformed
managed section, and `disconnect`/uninstall must remove exactly what was
written. Second, writing the consumer pre-flight gate into an agent's global
instruction file is not the same as imposing RepoGrammar's own mirrored
`AGENTS.md`/`CLAUDE.md` repository-contract policy on a consuming repository,
which remains prohibited.

## Decision

Make instruction wiring a default, opt-out behavior of `install` and `setup`.

When no `REPOGRAMMAR_INSTRUCTION_FILE_<TARGET>` override is set, resolve each
live-writer target's known default global instruction file:

- `codex` -> `<home>/.codex/AGENTS.md`
- `claude-code` -> `<home>/.claude/CLAUDE.md`

`<home>` is resolved from `HOME` through the same injectable environment lookup
the installer already uses for its data and command directories, so tests and
sandboxes redirect it without touching a real home directory. Only the two
concrete live-writer targets (`supported_concrete_targets()`) have a default.
`cursor`, `opencode`, `hermes`, `gemini`, `antigravity`, and `kiro` stay
deferred/plan-only and write nothing. macOS and Linux are the only supported
layouts; no Windows path is produced.

Resolution precedence and safety:

- An absolute `REPOGRAMMAR_INSTRUCTION_FILE_<TARGET>` override always wins.
- A present-but-non-absolute override is refused and stays deferred; it never
  falls back to the default, preserving the explicit "do not guess" contract for
  a caller that set an invalid path.
- When neither a default nor an absolute override resolves (for example when
  `HOME` is unset), the receipt records `instruction_action: "deferred"` and
  nothing is written.

Opt-out and reversibility:

- `--no-instructions` on `install` and `setup` registers the native MCP server
  without any instruction write. The receipt records
  `instruction_action: "deferred"` and `instruction_file_path: null`.
- The resolved default path is recorded in the receipt exactly like an override,
  so `disconnect`, product-uninstall agent cleanup, and rollback strip the
  managed section from the same default file and preserve unrelated user
  content.

Planning surfaces the resolved default so consent stays informed: the install
dry-run plan prints `instruction: managed section -> <path>` for a resolved
default, `instruction: opt-out ...` under `--no-instructions`, and
`instruction: deferred ...` when nothing resolves. The setup plan states that
the pre-flight gate is added to each agent's global instruction file, or that it
is skipped under `--no-instructions`.

This changes only what path the existing managed writer targets by default. The
writer itself (create/append/replace/refuse, atomic write, ownership
preservation, exact-version refresh) is unchanged, as are the
`instructions status|sync|remove` explicit-file commands.

## Alternatives considered

- Keep deferring unless the environment override is set: rejected because it is
  the root cause of agents never learning to prefer RepoGrammar, and it defeats
  the zero-friction onboarding contract.
- Auto-wire but make it opt-in behind a `--instructions` flag: rejected because
  the desired default is that a normal install teaches the agent; an opt-in flag
  reproduces the dormant-writer problem for everyone who does not know to set
  it.
- Guess additional per-agent files for deferred targets (cursor, opencode,
  etc.): rejected because their global instruction-file locations are not a
  single verified path; those targets stay deferred/plan-only.
- Resolve `<home>` directly from `std::env::home_dir`/`dirs`: rejected because
  it is not injectable, which would force tests to read or write a real
  `~/.codex/AGENTS.md` / `~/.claude/CLAUDE.md`. The `HOME` environment lookup is
  the same mechanism install already uses and is fully injectable.
- Write RepoGrammar's mirrored repository contract instead of the short consumer
  gate: rejected and explicitly out of scope; imposing the mirror policy on a
  consuming repository remains prohibited.

## Consequences

- A default `install`/`setup` now leaves the agent's global instruction file
  carrying RepoGrammar's pre-flight gate, so agents consult `repogrammar_context`
  before grep/read.
- Receipts now commonly record a concrete `instruction_file_path` for live-writer
  targets even without an override; `disconnect`/uninstall reverse it through the
  existing receipt-driven path.
- `--no-instructions` is the documented escape hatch for users who want the MCP
  server without the instruction write; the environment override remains the way
  to redirect the path.
- The change is contained to path resolution and one write gate. The managed
  writer, refusal semantics, and the explicit `instructions` commands are
  unchanged.
- Only the two concrete live-writer targets gain a default; deferred targets are
  unaffected.
