---
name: documentation-sync
description: Use for any code change, specification change, architecture change, or agent-rule change; do not use as a replacement for the canonical spec.
---

# Purpose

Keep implementation, canonical documentation, skills, memories, and mirrored
agent guides synchronized.

# Trigger conditions

Use when touching `src/`, `docs/`, `.agents/skills/`, `.agents/memories/`,
`AGENTS.md`, `CLAUDE.md`, README, CHANGELOG, or CI policy.

# Required reading

- `docs/README.md`
- `docs/development/documentation-policy.md`
- `docs/architecture/module-map.md`
- `.agents/memories/README.md`

# Preconditions

- Identify the canonical source for the changed topic.
- Check whether the change makes a memory stale.

# Step-by-step procedure

1. Update the canonical document first.
2. Update dependent references only where needed.
3. Keep requirements duplicated in multiple places byte- or meaning-consistent.
4. If either root guide changes, run guide sync immediately.
5. Validate relative links by inspection for every changed Markdown file.
6. Run repository guard.

# Required verification

```text
cargo run --quiet --bin repo-guard -- check
cmp -s AGENTS.md CLAUDE.md
```

# Documentation updates

Follow this minimum matrix:

| Code range | Required document |
|---|---|
| `src/rust/core/model/` | `docs/specifications/domain-model.md` |
| `src/rust/core/mining/` | `docs/specifications/indexing-pipeline.md` |
| `src/rust/core/policy/` | domain model, product spec, or related ADR |
| `src/rust/application/` | architecture overview and module map |
| `src/rust/ports/` | dependency rules and related specification |
| `src/rust/adapters/parsing/` | indexing pipeline and dependency rules |
| `src/rust/adapters/languages/` | indexing pipeline and roadmap |
| `src/rust/adapters/semantic_workers/` | semantic worker specification and ADR-0004 |
| `src/rust/adapters/frameworks/` | indexing pipeline and roadmap |
| `src/rust/adapters/persistence/` | storage specification and related ADR |
| `src/rust/interfaces/cli/` | README and CLI docs |
| `src/rust/interfaces/mcp/` | `docs/specifications/mcp-api.md` |
| `src/rust/bin/repo_guard.rs` | `docs/development/repository-guard.md` |
| `src/workers/` | semantic worker specification |
| `src/protocol/` | semantic worker specification |

# Commit requirements

Docs that describe the changed behavior must be in the same commit as the
behavior.

# Completion report

List canonical docs changed and any stale memories corrected.

# Failure and rollback handling

If documents conflict, use the precedence order in `docs/README.md` and correct
the lower-priority source.
