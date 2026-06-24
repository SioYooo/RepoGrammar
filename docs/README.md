# Documentation Map

This directory is the canonical entry point for RepoGrammar design,
development, and governance documentation.

## Directory responsibilities

- `architecture/`: module boundaries, dependency direction, and code layout.
- `specifications/`: product, domain model, indexing, storage, and MCP
  behavior contracts.
- `development/`: agent workflow, commits, documentation policy, repository
  guard usage, and testing policy.
- `decisions/`: accepted architecture decisions. ADRs are normative once
  accepted.
- `roadmap.md`: current staged implementation plan and deferred work.

Repository-local skills live under `.agents/skills/`. Durable but non-normative
context lives under `.agents/memories/`.

## Canonical source by topic

- Agent-wide mandatory rules: `AGENTS.md` and `CLAUDE.md`.
- Module dependencies: `architecture/dependency-rules.md`.
- Module ownership: `architecture/module-map.md`.
- Product boundaries: `specifications/product.md`.
- Pattern-family vocabulary: `specifications/domain-model.md`.
- Indexing pipeline: `specifications/indexing-pipeline.md`.
- Storage boundaries: `specifications/storage.md`.
- MCP tool intent: `specifications/mcp-api.md`.
- Language-native semantic workers: `specifications/semantic-workers.md`.
- MVP language scope: `decisions/ADR-0005-ts-js-first-mvp.md`.
- Quality gates: `development/repository-guard.md` and `development/testing.md`.

## Task reading guide

- Code change: read the mirrored root guide, this file, the relevant skill under
  `.agents/skills/`, the module map, and the specification for the touched
  area.
- Documentation change: read `development/documentation-policy.md` and the
  canonical source for the affected topic.
- MCP contract change: read `.agents/skills/mcp-contract-change/SKILL.md` and
  `specifications/mcp-api.md`.
- Semantic worker change: read `specifications/semantic-workers.md` and
  `decisions/ADR-0004-rust-core-language-native-workers.md`.
- Language support change: read `decisions/ADR-0005-ts-js-first-mvp.md`,
  `specifications/product.md`, and `docs/roadmap.md`.
- Storage change: read `specifications/storage.md` and
  `decisions/ADR-0002-local-sqlite-index.md`.
- Core model change: read `specifications/domain-model.md` and
  `.agents/skills/repogrammar-domain/SKILL.md`.

## Precedence

When documents conflict, apply this order:

1. Current explicit maintainer request.
2. Identical content in `AGENTS.md` and `CLAUDE.md`.
3. Accepted ADRs.
4. `specifications/`.
5. `architecture/`.
6. `development/`.
7. `.agents/skills/`.
8. `.agents/memories/`.
9. Other notes.

If a memory conflicts with a normative document, the normative document wins.
Update, supersede, or delete the stale memory in the same commit that exposes
the conflict.
