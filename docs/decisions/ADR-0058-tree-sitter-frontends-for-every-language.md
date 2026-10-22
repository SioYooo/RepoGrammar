# ADR-0058: Tree-sitter frontends for every source language

- Status: Accepted (maintainer decision 2026-10-20)
- Date: 2026-10-20
- Amends: ADR-0020 (zero-external-dependency constraint, for grammar crates
  only); the frontend clauses of ADR-0021, ADR-0022, ADR-0024, ADR-0025,
  ADR-0038, ADR-0040, ADR-0041, ADR-0042, ADR-0044, ADR-0045, ADR-0046,
  ADR-0047, ADR-0048, ADR-0049, ADR-0050 and ADR-0051
- Related: ADR-0057, ADR-0004, ADR-0019,
  [migration protocol](../experiments/tree-sitter-frontend-migration.md)

## Context

Five lanes (C, C++, C#, Java, Rust) parse with Tree-sitter. TS/JS uses a
dependency-free bootstrap extractor. Go, PHP, Ruby, Swift, R, SQL, Fortran,
MATLAB, Delphi/Object Pascal, Ada, VB.NET and Assembly use hand-written
lexers and parsers over declared subsets. These exist because the 2026-08-14
zero-external-dependency constraint excluded grammar crates. They total about
30,000 lines and repeat the same lexing, nesting and declaration-boundary work
in each language. The declarative-pack spike (ADR-0057) showed that
per-language cost should go to framework-anchor semantics, not syntax.

## Decision

1. **Tree-sitter is the syntax frontend for every source language lane**,
   with three exceptions:
   - Python keeps the CPython AST worker. It is the language's own parser and
     more exact than any grammar.
   - VB.NET keeps its hand-written frontend because crates.io has no
     maintained VB.NET grammar.
   - Scratch stays `NO_GO` under ADR-0039 because it is a binary archive
     format.

   Project-configuration and lockfile readers (`go.mod`, `Gemfile.lock`,
   `composer.json` and similar) are not source frontends and are unchanged.
2. **The zero-external-dependency constraint gains exactly one exception**:
   the individually pinned grammar crates in the admission table below. Each
   uses an exact `=` version, an MIT license, depends at runtime only on
   `tree-sitter-language`, builds only with `cc`, and has an ABI compatible
   with `tree-sitter` 0.27. Adding, upgrading or replacing a grammar is a
   reviewed change to this table. The aggregate `tree-sitter-language-pack`
   is not admitted, because 371 grammars in one crate is not a bounded
   dependency.
3. **The evidence ladders are revised and not silently superseded.** For each
   amended ADR, the family-bearing frontend changes from a pinned external
   parser or a hand-written declared-subset parser to the pinned Tree-sitter
   grammar, with the lane's existing declared-subset policy applied over the
   syntax tree. Any `ERROR` or `MISSING` node makes the whole file abstain, so
   a recovered tree never yields an anchor. This is stricter than legacy
   frontends that skipped regions such as function bodies: a file whose
   skipped region is syntactically invalid now abstains. Only source that does
   not compile is affected. These stay in force: every refusal
   class, the anchor shape, import-binding exactness, standing obligations and
   all worker, provider, sandbox and execution clauses of those ADRs. Semantic
   claims those ADRs reserved for a real parser or worker remain typed
   `UNKNOWN`. Each lane's completion review must re-check its gate 2
   (authoritative frontend) evidence against this ADR.
4. **Lanes migrate one at a time under the migration protocol.** A lane whose
   fixture outputs and full generation fingerprint are byte-identical
   (`EQUIVALENT`) deletes its hand-written frontend in the same change. A lane
   with differences (`CLASSIFIED`) keeps the hand-written frontend as a
   test-only oracle until the maintainer accepts the classification. A lane
   the grammar cannot serve is recorded as `BLOCKED` with the exact reason.

## Admission table

Verified on 2026-10-20 with `tree-sitter` 0.27.1 on macOS arm64. Each grammar
parsed every committed fixture of its lane without `ERROR` nodes, except the
deliberately degraded fixtures listed.

| Crate | Version | ABI | Lane | Fixtures parsed | `ERROR` trees (all deliberate unless noted) |
| --- | --- | ---: | --- | ---: | --- |
| `tree-sitter-go` | 0.25.0 | 15 | Go | 9 | 2 (`testing_parse_degraded`) |
| `tree-sitter-ruby` | 0.23.1 | 14 | Ruby | 9 | 2 (`minitest_parse_degraded`) |
| `tree-sitter-php` | 0.25.1 | 15 | PHP | 7 | 0 |
| `tree-sitter-swift` | 0.7.4 | 15 | Swift | 8 | 0 |
| `tree-sitter-r` | 1.3.0 | 14 | R | 3 | 0 |
| `tree-sitter-fortran` | 0.6.0 | 15 | Fortran | 9 | 0; the `testdrive_parse_degraded` fixture parses cleanly and must be classified |
| `tree-sitter-matlab` | 1.3.1 | 15 | MATLAB | 5 | 1 (`unittest_degraded`) |
| `tree-sitter-pascal` | 0.10.2 | 14 | Delphi/Object Pascal | 3 | 0 |
| `tree-sitter-ada` | 0.1.0 | 14 | Ada | 3 | 0 |
| `tree-sitter-sequel` | 0.3.11 | 14 | SQL | 4 | 0 |
| `tree-sitter-asm` | 0.24.0 | 14 | Assembly | 0 (no committed `.s` fixtures) | n/a |
| `tree-sitter-typescript` | 0.23.2 | 14 | TypeScript/TSX | 76 | 0 |
| `tree-sitter-javascript` | 0.25.0 | 15 | JavaScript | 6 | 0 |

A crate enters `Cargo.toml` only in the change that migrates its lane, so an
unused grammar is never compiled into the product.

## Consequences

- Each migrated lane drops its lexer and parser, but keeps its anchor and
  refusal semantics. ADR-0057 packs and a future shared anchor-rule layer
  consume one uniform syntax tree.
- Release builds compile more C grammar code. Binary size and build time are
  measured as each lane lands and recorded in the migration report.
- Grammar quality varies. Community grammars (Ada 0.1.0, Pascal, SQL, MATLAB,
  Fortran) must pass the lane differential before any hand-written code is
  removed.
- No language, framework, provider, readiness label or public schema changes
  in this ADR. Strict Top-20 completion remains 0/20 until each lane's
  completion review is redone.
