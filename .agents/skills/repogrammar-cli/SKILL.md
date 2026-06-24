---
name: repogrammar-cli
description: Use for any RepoGrammar CLI command, option, output contract, or command naming change; do not use for internal-only API changes with no CLI effect.
---

# Purpose

Keep the CLI aligned with RepoGrammar's implementation-pattern family identity.

# Trigger conditions

Use when editing `src/rust/interfaces/cli/`, README command examples, MCP-to-CLI
mapping, or any CLI command name or option.

# Required reading

- `docs/specifications/cli.md`
- `docs/specifications/mcp-api.md`
- `docs/decisions/ADR-0006-pattern-family-cli.md`

# Preconditions

- Confirm the command is pattern-family-first.
- Confirm it does not add `callers`, `callees`, `impact`, `affected`, `node`, or
  `explore` as a top-level v0.1 command.

# Step-by-step procedure

1. Map new query behavior to pattern-family concepts.
2. Keep graph navigation under a future secondary namespace if needed.
3. Add option parsing tests.
4. Add not-implemented behavior when storage or indexing is not ready.
5. Update README, CLI specification, MCP mapping, and CHANGELOG as needed.

# Required verification

Run the full Rust quality gates and `repo-guard check`.

# Documentation updates

Update `docs/specifications/cli.md` and every affected command example.

# Commit requirements

Commit CLI code, tests, and docs together.

# Completion report

Report changed commands, changed options, and whether behavior is implemented or
only a safe command contract.

# Failure and rollback handling

If a command would shift the product toward generic call-graph analysis, stop
and require an ADR before proceeding.
