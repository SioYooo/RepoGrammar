# Known Constraints

- Status: Active
- Last updated: 2026-06-25
- Scope: Confirmed repository constraints that are easy to violate.
- Evidence: Mirrored root guide, architecture docs, product spec, and
  `repo-guard` checks.
- Related canonical docs: `AGENTS.md`, `docs/architecture/dependency-rules.md`, `docs/specifications/product.md`
- Supersedes: None
- Superseded by: None

## Context

RepoGrammar intentionally starts with strict repository hygiene and conservative
analysis rules.

## Durable knowledge

- All source, script, fixture, test, benchmark, migration-tool, and automation
  code belongs under `src/`.
- `AGENTS.md` and `CLAUDE.md` must remain byte-identical root files.
- The first version is local-only and does not use LLMs, embeddings, vector
  databases, or cloud APIs.
- Repository-derived analysis state belongs in repo-local `.repogrammar/` or
  `REPOGRAMMAR_DIR`; global state must not store code-derived family facts,
  evidence, source paths, symbols, prompts, query text, or repo-specific SQLite
  indexes.
- `REPOGRAMMAR_DIR` is a repo-local directory-name override only, not an
  arbitrary global database path.
- `install` and `uninstall` must not create or remove `.repogrammar/`; `init`
  and `uninit` own project state lifecycle.
- `index` and `sync` currently store repo-relative file metadata and
  syntax-only code units in generation-scoped SQLite. Source snippets, absolute
  paths, semantic facts, families, evidence, and query read-path state must not
  be assumed present.
- Tree-sitter is the intended syntax technology, but AST types must stay in
  adapters and Tree-sitter facts are not final semantic truth.
- v0.1 official language scope is TypeScript/JavaScript only. Python is planned
  second and remains experimental until accepted for v0.2.
- Pre-v0.2 Python work is experimental dogfooding only. It must not be
  documented or exposed as production Python support.
- v0.1 CLI is pattern-family-first; CodeGraph-style graph navigation command
  names are not top-level commands.
- CodeGraph is a possible optional lower-layer provider. It is not a
  dependency, not a wrapper target, and not a source of independent
  pattern-family proof.
- The default v0.1 MCP surface is one `repogrammar_context` tool with explicit
  operations, while CLI remains multi-command.
- Anonymous telemetry and research trace collection are separate consent paths.
- Static uncertainty must be represented as typed `UNKNOWN`; dynamic behavior,
  conflicting facts, stale evidence, and insufficient support must not be
  guessed away.

## Implications

When adding automation, implement it under `src/` instead of adding root
scripts. Rust remains the core implementation language, while language-native
workers may use their native runtime behind protocol boundaries. When adding
analysis behavior, keep domain claims evidence-backed.

## Revalidation conditions

Revalidate after any accepted ADR changes source layout, external services,
parser strategy, optional provider strategy, language support, or uncertainty
policy.
