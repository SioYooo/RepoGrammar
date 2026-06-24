# Known Constraints

- Status: Active
- Last updated: 2026-06-24
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
- Tree-sitter is the intended syntax technology, but AST types must stay in
  adapters and Tree-sitter facts are not final semantic truth.
- v0.1 official language scope is TypeScript/JavaScript only. Python is planned
  second and remains experimental until accepted for v0.2.
- v0.1 CLI is pattern-family-first; CodeGraph-style graph navigation command
  names are not top-level commands.
- Anonymous telemetry and research trace collection are separate consent paths.
- Static uncertainty must be represented as `UNKNOWN`.

## Implications

When adding automation, implement it under `src/` instead of adding root
scripts. Rust remains the core implementation language, while language-native
workers may use their native runtime behind protocol boundaries. When adding
analysis behavior, keep domain claims evidence-backed.

## Revalidation conditions

Revalidate after any accepted ADR changes source layout, external services,
parser strategy, or uncertainty policy.
