# ADR-0002: SQLite and FTS5 local index

- Status: Accepted
- Date: 2026-06-24

## Context

RepoGrammar needs local persistence for repository revisions, content hashes,
code units, pattern families, and source evidence. The first version must not
depend on cloud services, vector databases, or embedding models.

## Decision

Use SQLite with FTS5 for the local index once persistence is implemented. The
database is repository-local: one SQLite database per project state directory,
not one global database for all repositories. Storage code and SQL migration
logic must stay in the persistence adapter. The Rust implementation uses
`rusqlite` with bundled SQLite so repository-local WAL, foreign-key, and
migration behavior does not depend on the host operating system's SQLite build.

## Alternatives considered

- Flat files: simpler bootstrap, but weaker query and migration support.
- External database server: unnecessary operational dependency for a local tool.
- Vector database: out of scope for the first version and not needed for
  structural pattern-family evidence.

## Consequences

Index metadata, provenance, and searchable source evidence can live in one
repository-local database file. ADR-0008 defines the `.repogrammar/` state
boundary, global-state limits, and generation/freshness requirements. The first
storage substrate creates generation-scoped databases and an active-generation
pointer. Current `index`, `sync`, `status`, and `doctor` wiring uses that
substrate for TS/JS discovery metadata and syntax-only code units. Query read
paths, family/evidence records, freshness manifests, FTS5 source-evidence
boundaries, and any top-level active database path remain future work.

## Follow-up work

Design freshness checks, query read paths, family/evidence storage, and FTS5
table boundaries before storing source evidence.
