---
name: mcp-contract-change
description: Use for any MCP tool name, schema, parameter, return structure, serialization, or error-semantics change.
---

# Purpose

Protect the MCP contract while keeping transport concerns out of the domain
model.

# Trigger conditions

Use when changing `src/rust/interfaces/mcp/`, planned MCP tools, transport-neutral
contract types, serialization behavior, or documented MCP error semantics.

# Required reading

- `docs/specifications/mcp-api.md`
- `docs/architecture/dependency-rules.md`
- `docs/development/branching-and-commits.md`

# Preconditions

- Decide whether the change is a major feature.
- Confirm no MCP SDK type leaks into `core`.

# Step-by-step procedure

1. Update the MCP specification first.
2. Keep domain and transport types separated.
3. Add serialization and deserialization tests when concrete schemas exist.
4. Analyze compatibility with existing tool names and responses.
5. Update CHANGELOG for user-visible contract changes.
6. Add or update an ADR for breaking or durable contract decisions.

# Required verification

Run formatting, clippy, tests, repository guard, and guide equality checks.

# Documentation updates

Update `docs/specifications/mcp-api.md`, README, CHANGELOG, and ADRs as needed.

# Commit requirements

Use a major-feature branch for breaking or user-visible MCP changes.

# Completion report

Report changed tools, compatibility impact, tests, and any deferred schema work.

# Failure and rollback handling

If compatibility cannot be proven, mark the contract unstable and do not claim a
stable MCP API.
