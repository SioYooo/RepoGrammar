# SQL language completion review

- Status: Incomplete — `discovered_only`
- Authority: ADR-0020 and ADR-0035
- Inventory prerequisite: `82ced893f81a954b64e20546dfc4ad81043b28e5`
- Last updated: 2026-08-01

## ADR-0020 gate

- [x] Discovery/configuration — exact `.sql`, deterministic artifact-role
  tokens, persistence, raw-byte bounds, and explicit refusal to select a
  dialect from filenames are covered.
- [ ] Authoritative versioned dialect frontend and selected project profile.
- [ ] RepoGrammar-owned SQL code units and IR.
- [ ] Semantic typed-`UNKNOWN` registry backed by SQL code-unit evidence.
- [ ] Exact family with support at least three; no qualified extension family
  or dependency model exists.
- [ ] Positive, lookalike, low-support, parse-degraded, dialect, and
  unresolved/resolved family fixtures.
- [ ] Complete source-free readiness and leakage matrix across required public
  surfaces.
- [x] Four-part review record — this report records correctness, security,
  completeness, and performance findings; open findings remain blockers.
- [ ] Linked atomic prerequisites and final completion audit.

## Current evidence and blocker

SQL files are raw-byte inventory only. The application does not call the source
store or parser, so it cannot retain query literals or touch a database. Path
roles are inventory labels and do not prove statement type, migration order,
dialect, extension, or runtime behavior. PostgreSQL and SQLite primary sources
show dialect-specific behavior, while no repository-local evidence selects one.

## Completion verdict

Not complete. SQL has no frontend, code unit, semantic fact, family, provider,
support, or readiness. Do not count SQL as supported.

## Final program audit fields

| Required field | Audited result |
|---|---|
| Language / rank | SQL / 8 |
| Dialect/version | None selected. Generic/migration/schema/catalog path labels do not choose PostgreSQL, SQLite, MySQL, SQL Server, Oracle, or a migration tool. |
| Provider/frontend/version | None; no grammar, database, client, catalog, or provider version. |
| Discovery/config / manifests | Exact `.sql` raw-byte inventory with path-role labels. No extension manifest, migration ordering model, schema catalog, lockfile, or database connection. |
| Owned source IR / external symbols | Both absent; tables, routines, extensions, schemas, and cross-file references are unresolved. |
| Library Contracts | Registry exists, production packs = 0; no `sql_extension` rows are emitted. |
| Exact family / fixtures | No family. Strong path precedence, binary/source-free, leakage, incremental, and no-execution tests; no dialect source-family corpus. |
| Primary UNKNOWN cases | Dialect/version, delimiter/quoting, migration ordering/tool, extension identity, search path, catalog state, dynamic SQL, procedures/triggers, and provider availability. |
| Source-free / security | Public inventory is source-free and SQL bytes never reach parser/source store; no database, client, migration tool, credential, child, or network access. |
| Completion state / counted | `discovered_only`; strict gate count `2/9`; Top-20 complete = no. |

Four-part review: correctness refuses path-based dialect inference; security
keeps SQL and credentials wholly unexecuted; completeness has no semantic
pipeline or dependencies; performance is deterministic metadata-only scanning
with no parser/catalog benchmark. Evidence: `src/rust/adapters/languages/sql.rs`,
filesystem/indexing/product tests, ADR-0035, and
`82ced893f81a954b64e20546dfc4ad81043b28e5`. Exact non-claim: discovering
a migration/schema-looking file proves neither dialect, order, validity,
idempotence, extension availability, nor database behavior.
