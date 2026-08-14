# ADR-0035: SQL source-free artifact inventory and dialect abstention

- Status: Accepted; partially superseded by ADR-0040
- Date: 2026-08-01
- Scope: SQL Round-4 discovery/inventory slice under ADR-0020
- Refines: ADR-0020 and ADR-0030
- Related: `docs/reports/language-support/sql-completion-review.md`,
  `docs/decisions/ADR-0040-dialect-invariant-sql-ddl-frontend.md`

ADR-0040 retires two clauses below and leaves the rest in force. SQL bytes now
reach a bounded in-process frontend and the source store, so "every SQL token is
inventory-only", "zero parser attempts", and "source text is never decoded" no
longer describe the product. And a frontend no longer waits on selecting one
dialect: ADR-0040 admits only constructs that parse identically under
PostgreSQL 16 and SQLite 3, so it needs no selection to be sound. Everything
else here — the path-role vocabulary, the refusal to infer a dialect, the ban on
databases, clients, migration tools, credentials, network access, and retained
query literals, and the unused `sql_extension` rows — is unchanged.

## Context

The `.sql` suffix does not establish one grammar. PostgreSQL's current SQL
syntax documentation explicitly says that rules are implemented inconsistently
among databases and that some are PostgreSQL-specific. SQLite's official
language documentation likewise says that SQLite omits some standard features
and adds its own. A filename, directory named `migrations`, or basename such as
`schema.sql` therefore cannot prove PostgreSQL, SQLite, another vendor dialect,
or standards conformance.

Primary sources reviewed on 2026-08-01:

- PostgreSQL 18, "SQL Syntax":
  <https://www.postgresql.org/docs/current/sql-syntax.html>;
- SQLite, "SQL As Understood By SQLite": <https://sqlite.org/lang.html>.

No primary source establishes a universal migration naming convention or a
static package/extension manifest shared by SQL repositories. Selecting a
primary dialect from repository prevalence, path words, or query tokens would
be a heuristic and would violate RepoGrammar's evidence policy.

## Decision

SQL remains `discovered_only` and unsupported. Exact case-sensitive `.sql`
paths are recorded without decoding their bytes. The authoritative pure path
classifier emits:

- `sql-migration` when an exact `migration` or `migrations` path component is
  present;
- `sql-schema` for exact basename `schema.sql` outside those components;
- `sql-catalog` for exact basename `catalog.sql` outside those components; and
- `sql` for every other exact `.sql` path.

These are source-free artifact inventory labels only. Migration precedence is
deterministic classification, not proof that a file is ordered, executable,
idempotent, complete, or associated with a migration tool. Schema/catalog
labels do not prove DDL, database metadata, or dialect.

The dialect for every admitted SQL path is explicitly `UNKNOWN`. The current
slice creates no SQL code unit, IR, semantic fact, dependency, extension row,
framework role, family, support, or readiness record. Because no SQL code unit
exists, dialect uncertainty is represented by the inventory contract and
completion review rather than by fabricating a semantic `UNKNOWN` with invalid
evidence. Every SQL token is inventory-only; the indexing loop must not call
the source store or parser for it.

RepoGrammar must not connect to a database, parse or execute a statement,
invoke a database client or migration tool, read credentials, inspect ambient
database configuration, retain query literals, or infer an extension from SQL
text. `sql_extension` dependency rows remain unused. A future exact static
extension manifest may emit them only after a separate source-backed format
qualification proves package identity and version without URL, path,
credential, execution, or database access.

No production dependency is authorized. A future dialect/frontend ADR must
choose an exact versioned grammar or provider, define dialect selection from
source-backed project metadata, bound recovery and resources, and complete the
ADR-0020 family/support gates before SQL can be called supported.

## Consequences

- SQL-only generations are `file_manifest_only` with zero parser attempts.
- Artifact changes remain incremental metadata replacement/removal and cannot
  copy forward legacy claim-bearing records.
- Binary and non-UTF-8 SQL files are safe to inventory because source text is
  never decoded.
- Path roles and dialect UNKNOWN must not be upgraded into semantics.

## Rejected alternatives

- PostgreSQL as an implicit primary dialect: rejected because the suffix and
  path do not prove it.
- Token scanning for vendor syntax: rejected because it reads query literals,
  remains ambiguous, and is not a semantic oracle.
- Running a database, migration tool, or client in default indexing: rejected
  because it crosses execution, credential, network, and mutation boundaries.
