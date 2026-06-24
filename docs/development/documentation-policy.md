# Documentation Policy

Documentation is part of the same change as implementation.

## Document classes

- Root mirrored guides: compact mandatory rules for every agent.
- `docs/specifications/`: product and behavior contracts.
- `docs/architecture/`: module boundaries and dependency direction.
- `docs/development/`: workflows, quality gates, and contribution mechanics.
- `docs/decisions/`: accepted ADRs.
- `.agents/skills/`: reusable agent procedures.
- `.agents/memories/`: durable non-normative context.

## Update matrix

| Code range | Required documents to check |
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
| `src/rust/interfaces/cli/` | README and CLI documentation |
| `src/rust/interfaces/mcp/` | `docs/specifications/mcp-api.md` |
| `src/rust/bin/repogrammar.rs` | README and CLI documentation |
| `src/rust/bin/repo_guard.rs` | `docs/development/repository-guard.md` |
| `src/workers/` | semantic worker specification |
| `src/protocol/` | semantic worker specification |
| testing strategy | `docs/development/testing.md` |

## Drift prevention

Do not duplicate requirements casually. Update the canonical document first,
then update references. `repo-guard check-diff` enforces a minimum gate that
`src/` changes include a documentation or agent-material change, but semantic
alignment remains the agent's responsibility.

## Root guide synchronization

`AGENTS.md` and `CLAUDE.md` must stay byte-identical. After editing either one,
run:

```text
cargo run --quiet --bin repo-guard -- sync-agent-guides --from AGENTS.md
```

or:

```text
cargo run --quiet --bin repo-guard -- sync-agent-guides --from CLAUDE.md
```
