# SQL language completion review

- Status: Incomplete — `structural_substrate`
- Authority: ADR-0020, ADR-0035, and ADR-0040
- Inventory prerequisite: `82ced893f81a954b64e20546dfc4ad81043b28e5`
- Last updated: 2026-08-14

## ADR-0020 gate

- [x] Discovery/configuration — exact `.sql`, deterministic artifact-role
  tokens, persistence, raw-byte bounds, and explicit refusal to select a
  dialect from filenames are covered.
- [x] Authoritative frontend for the declared scope — ADR-0040's bounded
  in-process scanner reads only constructs PostgreSQL 16 and SQLite 3 lex
  identically, so the parse it admits does not depend on the dialect it cannot
  select. Anything outside that set degrades. This is not a versioned SQL
  grammar or a database frontend, and the ADR forbids widening the subset
  without a superseding decision.
- [x] RepoGrammar-owned SQL code units and IR — a `module` unit per admitted
  file, one `sql_statement` or `sql_table_definition` unit per top-level
  statement, source ranges, content hashes, IR nodes, and containment edges,
  all owned types with no parser-native object crossing the port boundary.
- [x] Semantic typed-`UNKNOWN` registry backed by SQL code-unit evidence — the
  per-file `unproven_dialect_profile`, the per-statement
  `unadmitted_statement_shape`, the six named lexical-divergence classes, and
  the scanner resource limit all carry code-unit evidence with a source range,
  which is what ADR-0035 could not do when no unit existed.
- [ ] Exact family with support at least three — `sql.schema.table_definition`
  has its frontend anchor but no framework-role registry, derived-support
  engine, or family wiring yet.
- [ ] Positive, lookalike, low-support, parse-degraded, dialect, and
  unresolved/resolved family fixtures — frontend-level positive, lookalike,
  divergence, limit, and leakage fixtures exist; the family-level matrix does
  not.
- [ ] Complete source-free readiness and leakage matrix across required public
  surfaces.
- [x] Four-part review record — this report records correctness, security,
  completeness, and performance findings; open findings remain blockers.
- [ ] Linked atomic prerequisites and final completion audit.

## Current evidence and blocker

SQL is no longer raw-byte inventory. ADR-0040 replaced ADR-0035's premise-too-
broad conclusion: because a filename cannot select a dialect, no statement whose
*parse depends on the dialect* may be parsed — not that no statement may be
parsed at all. The frontend declares the invariance set {PostgreSQL 16,
SQLite 3} and admits a construct only when both members lex and nest it the
same way, so it claims only what holds under either.

`CREATE TABLE [IF NOT EXISTS] <name> ( … )` with a non-empty definition list is
the one exact anchor, carrying the fixed target `sql.ddl.create_table`. Every
other admitted statement is a unit with `InsufficientSupport`. A dialect-
divergent construct — any `$`, `E'…'`, a backtick- or bracket-quoted token, or
a nested block comment — degrades the whole file to its module unit plus a
`ConflictingFacts` `UNKNOWN`, because a diverged token stream leaves every later
statement boundary unproven.

The remaining blocker is the family lane: no role registry, derived-support
engine, family key, or support threshold exists for SQL, so no family can form
and gate 5 cannot be claimed. Migration order, catalog state, extension
identity, cross-statement table identity, dynamic SQL, procedures, and triggers
remain `UNKNOWN` by decision, not by omission.

## Completion verdict

Not complete. SQL has a frontend, owned code units, IR, and typed `UNKNOWN`s,
and it has no family, support, provider, or readiness. Do not count SQL as
supported.

## Final program audit fields

| Required field | Audited result |
|---|---|
| Language / rank | SQL / 8 |
| Dialect/version | None selected, and none needed for the admitted subset. The declared invariance set is PostgreSQL 16 and SQLite 3; MySQL, SQL Server, Oracle, and migration-tool directive syntax are outside it and are neither detected nor claimed. |
| Provider/frontend/version | Bounded in-process `repogrammar-sql-ddl-scanner` / `bounded_pg16_sqlite3_invariant_ddl_v1`. No external grammar, database, client, driver, catalog, or provider. |
| Discovery/config / manifests | Exact `.sql` with path-role labels, now dispatched to the frontend. No extension manifest, migration ordering model, schema catalog, lockfile, or database connection. |
| Owned source IR / external symbols | Owned units, ranges, hashes, IR nodes, and containment edges exist. External symbols remain absent: tables, routines, extensions, schemas, and cross-file references are unresolved by decision. |
| Library Contracts | Registry exists, production packs = 0; no `sql_extension` rows are emitted. |
| Exact family / fixtures | Frontend anchor exists; family does not. Fixtures cover admitted anchors, comment/string lookalikes, all six divergence classes, unadmitted `CREATE TABLE` spellings, semicolons inside strings/comments/parens, resource limits, IR edges, and name/literal leakage. The family-level positive/low-support/resolved matrix is absent. |
| Primary UNKNOWN cases | Dialect/version, lexical divergence, unadmitted statement shape, migration ordering/tool, extension identity, search path, catalog state, cross-statement table identity, dynamic SQL, procedures/triggers, and provider availability. |
| Source-free / security | No table, column, index, or literal text reaches a unit id, fact target, note, assumption, or public surface, proven by leakage fixtures at both the frontend and the product CLI. No database, client, driver, migration tool, credential, child process, or network access exists, and nothing is executed, prepared, planned, or validated. |
| Completion state / counted | `structural_substrate`; strict gate count `5/9`; Top-20 complete = no. |

Four-part review: correctness rests on the dialect-invariance argument — the
admitted parse cannot change with the unproven dialect, which is exactly why
ADR-0038's assembly restriction does not transfer, and why widening the admitted
subset is an ADR decision rather than an implementation detail. Security retired
one property and kept the rest: SQL bytes now reach the frontend and source
store, while execution, database, credential, child-process, and network
boundaries are unchanged, and a non-UTF-8 file is skipped with a warning rather
than failing the run or reading as a clean empty parse. Completeness has a
frontend, units, IR, and typed `UNKNOWN`s but no family, support, provider, or
readiness. Performance is a single bounded pass under input-byte, statement,
and fact ceilings; no representative large-dump or migration-corpus benchmark
exists. Evidence: `src/rust/adapters/languages/sql.rs`,
`src/rust/adapters/parsing/sql.rs`, `src/rust/application/indexing.rs`,
`src/rust/bin/repogrammar.rs`, ADR-0035, ADR-0040, and
`82ced893f81a954b64e20546dfc4ad81043b28e5`. Exact non-claim: parsing a
`CREATE TABLE` shape proves neither dialect, order, validity, idempotence,
extension availability, table identity, nor database behavior.
