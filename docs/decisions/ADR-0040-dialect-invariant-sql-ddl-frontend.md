# ADR-0040: Dialect-invariant bounded SQL DDL frontend

- Status: Accepted
- Date: 2026-08-14
- Scope: SQL in ADR-0020; admits a family-bearing frontend
- Refines: ADR-0035 (SQL source-free artifact inventory and dialect abstention)
- Related: ADR-0020, ADR-0038,
  `docs/reports/language-support/sql-completion-review.md`

## Context

ADR-0035 left SQL as raw-byte inventory: `.sql` files were discovered, labeled
by path role, and never read. Its reasoning was that a filename cannot select a
dialect, and that dialect selection changes what a statement means. That
reasoning is still correct and this ADR does not overturn it.

But it proves less than it was used for. ADR-0035 concluded that because the
dialect is unproven, no SQL statement may be parsed at all. The premise only
supports a narrower conclusion: no SQL statement whose *parse depends on the
dialect* may be parsed. Some statement shapes lex and nest identically under
every dialect a repository plausibly targets, and for those the unproven dialect
is not a claim-relevant unknown — it is an unknown about something the anchor
never asserts.

ADR-0020 gate 5 already names "SQL statement/migration shapes" as eligible
exact-anchor shapes. Nothing in the gate requires a dialect oracle first; it
requires that the anchor be exact and source-visible, and that non-claims be
explicit. This ADR defines the subset where that is achievable without any
external artifact.

Primary specifications this ADR reads as authority:

- PostgreSQL 16, "SQL Syntax — Lexical Structure":
  <https://www.postgresql.org/docs/16/sql-syntax-lexical.html>, in particular
  identifiers and key words (§4.1.1), string constants with C-style escapes
  (§4.1.2.2), dollar-quoted string constants (§4.1.2.4), and comments (§4.1.5),
  which state that `/* */` block comments **nest**.
- SQLite 3, "Keywords and quoted identifiers":
  <https://www.sqlite.org/lang_keywords.html>, which admits `"…"`, `[…]`, and
  `` `…` `` as identifier quotes; and "SQL Comment Syntax":
  <https://www.sqlite.org/lang_comment.html>, which states that block comments
  do **not** nest.

## Decision

### D1. The declared scope is a dialect-invariance set, not one dialect

The bounded frontend declares the invariance set **{PostgreSQL 16, SQLite 3}**.
A construct is admitted only when both members lex and nest it the same way. The
frontend therefore never selects a dialect and never needs to: it claims only
what holds under either member.

MySQL, SQL Server, Oracle, and every migration tool's own directive syntax are
outside the set. Their presence is not detected and not claimed; a repository
targeting one of them gets the same abstention as any other unadmitted input.

This is a bounded scope under ADR-0020 D1 and must never be reported as SQL
coverage. Discovering a `.sql` file still proves nothing about dialect, order,
validity, idempotence, extension availability, or database behavior.

### D2. Why this is a different fidelity class from ADR-0038

ADR-0038 forbids the assembly scanner from ever supporting a family, and that
restriction is correct there for a reason that does not transfer. In assembly the
unproven target profile is **claim-relevant**: the same token sequence denotes
different instructions, operand orders, and symbol semantics under different
targets, so the scanner's own output changes meaning with the unknown. Its
candidates cannot be exact anchors because the profile they depend on is
unproven.

In the subset admitted here the unproven dialect is **claim-irrelevant**,
because the admitted parse is invariant across the declared set. `CREATE TABLE`
followed by a name token and a parenthesized definition list is that statement
under both members, with the same token boundaries and the same nesting. The
anchor asserts the statement shape and its source range; it asserts nothing that
a dialect could change.

The frontend keeps the per-file `unproven_dialect_profile` typed `UNKNOWN` that
ADR-0035 established, exactly as ADR-0038 keeps `unproven_target_profile`. The
difference is what that `UNKNOWN` blocks: here it blocks dialect-dependent
claims — catalog state, execution semantics, migration order, extension
identity — and not the shape anchor, because the shape anchor does not rest on
it.

This distinction is load-bearing. If a later change admits a construct whose
parse differs across the declared set, that construct's anchors become
profile-dependent and lose family eligibility. Widening the admitted subset is
therefore an ADR decision, not an implementation detail.

### D3. Admitted lexical core

Admitted, because PostgreSQL 16 and SQLite 3 agree:

- `--` line comments to end of line.
- `/* … */` block comments, **non-nested only**. A `/*` inside an open block
  comment is a divergence: PostgreSQL nests, SQLite terminates at the first
  `*/`. The two dialects disagree about where the statement resumes.
- Single-quoted string constants with `''` doubling, treated as one opaque
  token.
- Double-quoted tokens, treated as one opaque token. Both members lex `"…"` as a
  single token in the identifier positions this frontend scans; the frontend
  derives no name, case, or identity from it, so SQLite's string-literal
  fallback for unresolvable double-quoted tokens cannot change the parse.
- Unquoted ASCII identifiers and key words, matched case-insensitively for
  statement dispatch only.
- `;` as top-level statement terminator, and `(`/`)` nesting.

Refused, because the members disagree and the disagreement moves token
boundaries:

- Any `$`. Dollar-quoted string constants (`$tag$ … $tag$`) are PostgreSQL
  only, and the two members also spell dollar parameters differently, so the
  scanner refuses the character rather than deciding which reading applies.
- C-style escape string constants (`E'…'`) — PostgreSQL only.
- Backtick-quoted and bracket-quoted identifiers — SQLite only.
- Nested block comments.

A refused construct is a **file-level** degradation, not a statement-level one.
Once the token stream diverges, every following statement boundary in the file
is unproven, so the file yields its module unit, a `ConflictingFacts` typed
`UNKNOWN` naming the divergence class, and no statement anchor at all. Raw
divergent text is never copied into a fact target, note, or assumption.

### D4. Admitted statement shapes and the exact anchor

Within an admitted file, top-level statements are split on `;` outside strings,
quoted tokens, comments, and parentheses. Each statement becomes an owned code
unit.

One shape is an exact anchor: `CREATE TABLE [IF NOT EXISTS] <name-token> ( … )`
with a non-empty parenthesized definition list. It produces a
`sql_table_definition` code unit and one structural anchor fact whose target is
the fixed token `sql.ddl.create_table`.

Every other admitted statement is a generic `sql_statement` unit carrying an
`InsufficientSupport` typed `UNKNOWN` with the `unadmitted_statement_shape`
kind. It is inventory with a source range; it is not an anchor and cannot
support a family.

The exact family is `sql.schema.table_definition` under the role
`framework:sql.table_definition`, with a minimum support of three, matching the
threshold ADR-0020 already requires of SQL in the completion review.

### D5. Names are never emitted

No table name, column name, index name, string constant, or other repository
identifier reaches a fact target, note, assumption, code-unit id, or any public
surface. Code units are identified by byte range and ordinal. Support targets
come from a fixed vocabulary.

This is partly a gate-7 source-free requirement, and partly a correctness one:
unquoted identifiers fold to lowercase in PostgreSQL and are matched
case-insensitively in SQLite, so a name's identity is exactly the kind of
dialect-dependent fact this frontend has no authority to assert. Cross-statement
table identity, catalog resolution, and reference binding therefore remain
`UNKNOWN`.

### D6. Security posture change

ADR-0035 stated that SQL bytes never reach the parser or the source store. That
property ends here, and the completion review must say so rather than carry the
old sentence forward.

What does not change: no database connection, client, driver, credential,
migration tool, child process, or network access exists, and no SQL is executed,
prepared, planned, or validated against a catalog. The frontend consumes supplied
UTF-8 bytes in process under the same input-byte, line, statement, and fact
ceilings the other bounded frontends use, and non-UTF-8 input degrades rather
than producing a partial confident result.

### D7. No dependency is authorized

This ADR authorizes a hand-written bounded scanner in the existing Rust core and
nothing else. It does not authorize `sqlparser-rs`, a Tree-sitter SQL grammar,
libpq, the SQLite amalgamation, a migration tool, or any other external artifact,
and it does not reserve the right to add one later without a superseding ADR.

## Alternatives considered

- Keep SQL inventory-only until a dialect is proven: rejected because the
  premise proves only that dialect-dependent parses must wait, and because no
  repository-local evidence that selects a dialect is likely to appear, which
  makes the condition unreachable rather than conservative.
- Select PostgreSQL alone as the reference grammar: rejected because it would
  make every anchor depend on an unproven selection, which is the ADR-0038
  situation, and would forfeit family eligibility for no gain in the admitted
  subset.
- Admit a wide statement set with per-statement degradation: rejected because a
  lexical divergence invalidates later statement boundaries in the same file, so
  per-statement degradation would report confident boundaries derived from an
  unproven split.
- Emit table and column names as fact targets: rejected under D5.

## Consequences

- SQL moves from `discovered_only` toward a frontend, owned IR, typed
  `UNKNOWN`s, and one exact family. It is not `bounded_preview` until every
  ADR-0020 gate is linked; this ADR closes none of them by itself.
- ADR-0035's inventory and abstention rules remain in force except for the
  source-free property retired in D6.
- Migration ordering, dialect selection, extension identity, catalog state,
  dynamic SQL, procedures, and triggers remain typed `UNKNOWN`.
- A file containing one dialect-specific construct contributes no anchors, which
  is the intended conservative failure and must not be softened by falling back
  to a single dialect.

## Follow-up

- Add `ALTER TABLE` and `CREATE INDEX` anchors only after confirming their
  admitted spellings are invariant across the declared set.
- Widening the invariance set, admitting a refused construct, or adding a
  dialect-dependent claim each require a superseding ADR.
- Migration ordering remains out of scope until repository-local evidence, not
  filename convention, can establish it.
