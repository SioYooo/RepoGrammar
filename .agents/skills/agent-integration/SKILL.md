---
name: agent-integration
description: Use for install, uninstall, MCP agent configuration, receipts, self-tests, or instruction-file integration changes.
---

# Purpose

Keep machine-level agent integration safe, reversible, and scoped.

# Trigger conditions

Use when editing `repogrammar install`, `repogrammar uninstall`, MCP
configuration writes, installation receipts, or agent instruction-file edits.

# Required reading

- `docs/specifications/installation.md`
- `docs/decisions/ADR-0007-safe-install-progress-telemetry.md`
- `docs/specifications/mcp-api.md`

# Preconditions

- Identify target agent and scope.
- Confirm dry-run and print-config behavior are available.
- Confirm consuming repositories are not forced to mirror RepoGrammar's own root
  guide policy.

# Step-by-step procedure

1. Detect supported agents.
2. Prefer native agent configuration commands where available.
3. Preserve unknown config fields.
4. Refuse malformed config by default.
5. Back up before approved repair.
6. Write atomically and reparse after writing.
7. Store a reversible receipt.
8. Validate MCP integration with a self-test.
9. Only modify instruction files with marker fences and explicit consent.
10. Never create or delete repository-local `.repogrammar/` indexes from
    `install` or `uninstall`; project state belongs to `init` and `uninit`.

# Required verification

Run installer parsing tests, self-test validation tests when implemented, and
the full repository verification suite.

# Documentation updates

Update `docs/specifications/installation.md`, README, CHANGELOG, and MCP docs
as needed.

# Commit requirements

Never commit machine-local receipts or generated user config. Commit tests and
docs with installer behavior.

# Completion report

Report target agents, scopes, dry-run behavior, receipt behavior, and any
configuration that remains unsupported.

# Failure and rollback handling

If config is malformed or unknown ownership cannot be proven, do not write.
Report the blocker and leave user configuration untouched.
