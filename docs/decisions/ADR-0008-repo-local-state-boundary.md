# ADR-0008: Repo-local state boundary

- Status: Accepted; auto-sync default partially superseded by ADR-0027 and the
  machine-product uninstall wording partially superseded by ADR-0028
- Date: 2026-06-24

## Context

RepoGrammar's primary claims are repository-specific. Pattern families,
variation slots, exceptions, evidence ranges, source hashes, and freshness
state are meaningful only for the repository and worktree that produced them.

A global database would risk cross-repository pollution, unclear Git revision
binding, worktree and branch conflicts, ambiguous deletion behavior, weaker
privacy boundaries, harder MCP discovery, and unnecessary SQLite lock
contention across unrelated projects.

## Decision

Use a repository-local state directory for all repository-derived state:

```text
.repogrammar/
```

`REPOGRAMMAR_DIR` may override the state directory name for one checkout. This
supports cases such as Windows and WSL sharing one checkout, where SQLite locks
and daemon locks must not be shared across OS boundaries.

The repository-local state directory owns the SQLite database, WAL/SHM files,
manifest, index generations, caches, logs, locks, local telemetry rollups,
receipts, and temporary files.

Global user state may contain installation receipts, binary/cache metadata,
anonymous telemetry preference, anonymous machine id, downloaded non-repository
runtime artifacts, and global user preferences only. It must not contain
source-derived family facts, evidence text, repository paths, symbol names, raw
prompts, query text, or repository-specific SQLite indexes.

`repogrammar install` and `repogrammar disconnect` configure or remove
machine-level agent integration. ADR-0028 changes bare `repogrammar uninstall`
to receipt-gated removal of the first-party managed machine product. None of
these commands creates or deletes `.repogrammar/`. Repository-local state is
created by `repogrammar init` and removed only by `repogrammar uninit`.

The default v0.1 MCP surface should expose one primary tool,
`repogrammar_context`, with an `operation` field for `find_analogues`,
`show_family`, `explain_deviation`, and `check_conformance`. The CLI remains
multi-command for human discoverability.

The original v0.1 baseline made auto-sync opt-in. ADR-0027 supersedes only that
default: explicit `init` now starts a repo-local daemon after a successful
resync unless `--no-autosync` is present. The repo-local state and explicit
repository-authorization boundaries in this ADR remain unchanged.

## Alternatives considered

- Global SQLite database: simpler discovery for one user account, but unsafe for
  repository-specific family claims and weaker for deletion, privacy, freshness,
  and lock boundaries.
- Agent installation implicitly creates project indexes: convenient, but
  conflates machine-level integration with repository mutation.
- Multiple default MCP tools: explicit per-operation schemas, but higher risk of
  agent tool-selection mistakes. A single contextual tool with explicit
  operations keeps the default surface smaller.
- Default daemon auto-sync in the original v0.1 design: rejected before bounded
  fingerprinting, incremental invalidation, atomic activation, and startup
  readiness existed; ADR-0027 revisits and supersedes this alternative.

## Consequences

RepoGrammar must:

- create `.repogrammar/` or `REPOGRAMMAR_DIR` during project initialization;
- write `.repogrammar/` and `.repogrammar-*/` to `.git/info/exclude` by
  default;
- create `.repogrammar/.gitignore` as a second defense;
- modify root `.gitignore` only when the user opts in;
- store one SQLite database per project state directory using WAL mode;
- build new generations atomically and preserve the previous valid generation
  on failure;
- store manifest freshness and provenance for every active generation;
- keep logs and local telemetry rollups repo-local and redacted by default;
- implement safe lock inspection before `unlock` removes stale locks;
- return clean fallback guidance when `.repogrammar/` is missing;
- avoid imposing RepoGrammar's own `AGENTS.md` and `CLAUDE.md` mirroring policy
  on consuming repositories.

## Follow-up work

Extend the implemented repo-local lifecycle, Git hygiene, manifest writing,
SQLite migrations, generation activation, status/doctor checks, logs, locks,
safe unlock, and missing-index fallback with freshness manifests, query read
paths, family/evidence persistence, MCP serving, and installer wiring before
enabling production family queries or MCP serving.
