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
- [x] Exact family with support at least three —
  `sql.schema.table_definition` forms from the `framework:sql.table_definition`
  role over the fixed `sql.ddl.create_table` anchor target, with a minimum
  support of three rather than the shared default of two. Support must carry the
  owned `repogrammar-sql-derived` origin; no other engine naming the same target
  can supply it.
- [x] Positive, lookalike, low-support, parse-degraded, and dialect fixtures —
  a committed corpus under `src/fixtures/sql/release/v0_1/` exercises the
  product CLI: three admitted definitions form exactly one family; two do not;
  comment, string-literal, `CREATE TEMP TABLE`, `CREATE TABLE … AS SELECT`,
  `CREATE VIEW`, and `CREATE INDEX` lookalikes form none; and a file whose
  three admitted definitions precede a dollar-quoted construct forms none,
  because the divergence unproves every boundary in it. No unresolved-to-
  resolved pair exists, since no provider can resolve a SQL claim.
- [x] Complete source-free readiness and leakage matrix across required public
  surfaces — SQL is registered in the repo-shape language scopes, so its units
  and families are counted rather than silently reported as zero; `status`,
  `doctor`, `stats`, `unknowns`, `families`, `files`, and
  the MCP `inspect_readiness` and `find_analogues` payloads are each asserted
  over an indexed SQL repository to expose no table, column, literal, or SQL
  keyword text and no absolute path. The assertions are non-vacuous: each
  command must exit zero and parse, the SQL lane must appear in the unknowns
  inventory by bounded language token and count, and the readiness surface must
  report the SQL family.
- [x] Four-part review record — this report records correctness, security,
  completeness, and performance findings; open findings remain blockers.
- [ ] Linked atomic prerequisites and final completion audit — the chain is
  `82ced893f81a954b64e20546dfc4ad81043b28e5` (discovery/configuration),
  `ccf2429` (ADR-0040 decision), `83194a5` (frontend, owned IR, typed
  `UNKNOWN`), `a7eb390` (family and fixtures), and the readiness/review commit
  that carries this report. Each is an independently coherent Conventional
  Commit with its own tests and documentation. It is left unchecked for one
  reason, stated rather than worked around: ADR-0020 G9 lists frontend/IR and
  `UNKNOWN`/provider as separate submodules, and they landed together in
  `83194a5`. For SQL they are not separable — ADR-0035's own argument is that a
  semantic `UNKNOWN` needs code-unit evidence, so the abstentions cannot exist
  before the frontend that produces the units, and no provider exists to form
  the other half. Whether that satisfies G9's structure is a maintainer ruling,
  not a self-grant.

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

The family lane encodes the same argument in the type system. SQL's typed
`UNKNOWN`s are scoped to three claims, not one: `sql_statement_boundary`
blocks, because a diverged token stream unproves the boundaries a claim rests
on; `sql_dialect_profile` is a standing non-blocking subclaim, because the
admitted parse is invariant across the declared set and so the dialect cannot
change the anchor; and `sql_statement_shape` is inventory. Widening the admitted
subset to a construct the members lex differently would falsify the middle one,
which is why ADR-0040 makes widening a decision rather than an implementation
detail.

The readiness audit also corrected a defect it was written to look for. The
dialect `UNKNOWN` reported `project_config_reader` as its required mechanism and
`add_project_config` as its recovery, inherited from the assembly lane where a
build-metadata file genuinely can prove the target profile. For SQL that is
false by ADR-0035's own finding: no repository-local evidence selects a dialect,
so there is no config to add and no provider to enable. The mechanism is now
`manual_dialect_declaration` recovering through `manual_review_required`, and a
regression assertion forbids `add_project_config` from reappearing on this
claim. Advertising a mechanism the product does not have is the same class of
invented certainty as a heuristic fact.

The remaining blocker is gate 9 alone. Migration order, catalog state, extension identity, cross-statement table
identity, dynamic SQL, procedures, and triggers remain `UNKNOWN` by decision,
not by omission. Family variation slots and context features, which other
languages carry, are absent for SQL: the family forms without them, and adding
them is a separate decision about which statement shapes are meaningfully
distinct.

## Completion verdict

Not complete. SQL has a frontend, owned code units, IR, typed `UNKNOWN`s, one
exact family, an adversarial fixture corpus, and an audited source-free
readiness matrix — eight of nine gates. Gate 9 is left to a maintainer ruling
for the reason recorded above, and no provider exists or is authorized. Do not
count SQL as supported and do not set `top20_complete`.

## Final program audit fields

| Required field | Audited result |
|---|---|
| Language / rank | SQL / 8 |
| Dialect/version | None selected, and none needed for the admitted subset. The declared invariance set is PostgreSQL 16 and SQLite 3; MySQL, SQL Server, Oracle, and migration-tool directive syntax are outside it and are neither detected nor claimed. |
| Provider/frontend/version | Bounded in-process `repogrammar-sql-ddl-scanner` / `bounded_pg16_sqlite3_invariant_ddl_v1`. No external grammar, database, client, driver, catalog, or provider. |
| Discovery/config / manifests | Exact `.sql` with path-role labels, now dispatched to the frontend. No extension manifest, migration ordering model, schema catalog, lockfile, or database connection. |
| Owned source IR / external symbols | Owned units, ranges, hashes, IR nodes, and containment edges exist. External symbols remain absent: tables, routines, extensions, schemas, and cross-file references are unresolved by decision. |
| Library Contracts | Registry exists, production packs = 0; no `sql_extension` rows are emitted. |
| Exact family / fixtures | `sql.schema.table_definition` over the fixed `sql.ddl.create_table` target, minimum support three, owned derived origin required. Fixtures cover admitted anchors, comment/string lookalikes, all six divergence classes, unadmitted `CREATE TABLE` spellings, semicolons inside strings/comments/parens, resource limits, IR edges, name/literal leakage at the parser, and a product-CLI corpus for positive, low-support, lookalike, and divergence cases. No unresolved-to-resolved pair exists, because no provider can resolve a SQL claim. |
| Primary UNKNOWN cases | Dialect/version, lexical divergence, unadmitted statement shape, migration ordering/tool, extension identity, search path, catalog state, cross-statement table identity, dynamic SQL, procedures/triggers, and provider availability. |
| Source-free / security | No table, column, index, or literal text reaches a unit id, fact target, note, assumption, or public surface, proven by leakage assertions at the frontend, the product CLI (`status`, `doctor`, `stats`, `unknowns`, `families`, `files`), and the MCP readiness and analogue payloads. No database, client, driver, migration tool, credential, child process, or network access exists, and nothing is executed, prepared, planned, or validated. |
| Completion state / counted | `bounded_preview` is not claimed; `structural_substrate` with strict gate count `8/9`; Top-20 complete = no. |

Four-part review: correctness rests on the dialect-invariance argument — the
admitted parse cannot change with the unproven dialect, which is exactly why
ADR-0038's assembly restriction does not transfer, and why widening the admitted
subset is an ADR decision rather than an implementation detail. Security retired
one property and kept the rest: SQL bytes now reach the frontend and source
store, while execution, database, credential, child-process, and network
boundaries are unchanged, and a non-UTF-8 file is skipped with a warning rather
than failing the run or reading as a clean empty parse. Completeness has a
frontend, units, IR, typed `UNKNOWN`s, one exact family, and fixtures, and lacks
an audited readiness matrix, a final audit, and any provider. Performance is a single bounded pass under input-byte, statement,
and fact ceilings; no representative large-dump or migration-corpus benchmark
exists. Evidence: `src/rust/adapters/languages/sql.rs`,
`src/rust/adapters/parsing/sql.rs`, `src/rust/adapters/frameworks/sql.rs`,
`src/rust/application/family.rs`, `src/rust/application/indexing.rs`,
`src/rust/bin/repogrammar.rs`, `src/fixtures/sql/release/v0_1/`, ADR-0035,
ADR-0040, and
`82ced893f81a954b64e20546dfc4ad81043b28e5`. Exact non-claim: parsing a
`CREATE TABLE` shape proves neither dialect, order, validity, idempotence,
extension availability, table identity, nor database behavior.
