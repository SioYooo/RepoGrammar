# Storage Specification

RepoGrammar will use local SQLite with FTS5 for index metadata and searchable
source evidence. The bootstrap defines only the adapter boundary.

## SQLite responsibilities

- Store repository revision and index metadata.
- Store code units, source ranges, content hashes, and provenance.
- Store family records, canonical templates, variation points, exceptions, and
  evidence links once implemented.
- Support local full-text search where useful through FTS5.

## Expected table boundaries

Future schema work should keep separate tables for:

- repository metadata;
- indexed files and content hashes;
- code units and source ranges;
- unified IR summaries;
- fingerprints and candidate groups;
- pattern families and templates;
- variations, exceptions, and counterexamples;
- source evidence.

## Repository revision and content hash

Every indexed conclusion must be tied to a repository revision and content hash.
Freshness checks must reject or mark stale evidence when file content changes.

## Migration strategy

Migration execution logic belongs under `src/rust/adapters/persistence/`. Migrations
must be deterministic, versioned, tested, and documented before storage is
implemented.

## Non-goals

RepoGrammar does not use a vector database in the first version. Embeddings are
not part of the bootstrap architecture.
