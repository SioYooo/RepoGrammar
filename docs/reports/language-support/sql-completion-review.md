# SQL language completion review

- Status: Incomplete — `discovered_only`
- Authority: ADR-0020 and ADR-0035
- Integration baseline: `abda602fe38db8549e4f318bd4638ddb94df7282`
- Last updated: 2026-08-01

## ADR-0020 gate

- [x] Source-free discovery and artifact inventory — exact `.sql`, deterministic
  migration/schema/catalog/generic tokens, persistence, CLI mode, incremental
  lifecycle, raw-byte boundaries, and leakage tests exist.
- [x] Dialect decision — source evidence rejects filename-based selection, so
  every SQL dialect remains explicitly UNKNOWN.
- [ ] Authoritative versioned dialect frontend and selected project profile.
- [ ] RepoGrammar-owned SQL code units and IR.
- [ ] Semantic typed-UNKNOWN registry backed by SQL code-unit evidence.
- [ ] Qualified extension/dependency manifest inventory.
- [ ] Exact family with support at least three.
- [ ] Source-free readiness plus independent correctness/security/performance review.
- [ ] Linked final completion audit.

## Current evidence and blocker

SQL files are raw-byte inventory only. The application does not call the source
store or parser, so it cannot retain query literals or touch a database. Path
roles are inventory labels and do not prove statement type, migration order,
dialect, extension, or runtime behavior. PostgreSQL and SQLite primary sources
show dialect-specific behavior, while no repository-local evidence selects one.

## Completion verdict

Not complete. SQL has no frontend, code unit, semantic fact, family, provider,
support, or readiness. Do not count SQL as supported.
