# ADR-0059: Tree-sitter TS/JS frontend in slices

- Status: Accepted (maintainer decision 2026-10-22)
- Date: 2026-10-20
- Implements: [ADR-0058](ADR-0058-tree-sitter-frontends-for-every-language.md)
  for the TS/JS lane
- Related: ADR-0057,
  [binding engine](../experiments/binding-engine.md),
  [anchor role rows](../experiments/anchor-role-rows.md),
  [migration protocol](../experiments/tree-sitter-frontend-migration.md),
  [slice 1 preregistration](../experiments/tsjs-scope-graph-tree.md)

## Context

The TS/JS lane is the dependency-free bootstrap extractor:

- `adapters/parsing/syntax.rs` produces the TS/JS code units;
- `adapters/parsing/tsjs/` (5,190 lines in 23 files) decides the exact
  framework anchors of those units.

One text-level analysis, `ScopeGraphLite` (901 lines), is the import, scope
and shadowing authority for all ten framework modules (31 call sites). It
reads source line by line and counts raw `{` and `}` bytes. Its known
heuristic limits:

- an import clause that spans lines is not read;
- braces inside strings, comments, template literals and regular expressions
  move its scopes;
- only the first declarator of a declaration is read;
- rest and destructured parameters are ignored;
- a declaration that does not start its line is not seen.

The lane has no declared subset and no refusal classes, so the ADR-0058 lane
protocol, which reproduces every refusal class over the tree, has nothing to
reproduce. Output cannot be byte-identical wherever the heuristics misread.

ADR-0058 already admits the two grammars: `tree-sitter-typescript` 0.23.2 (its
TypeScript and TSX languages) and `tree-sitter-javascript` 0.25.0. Together
they parsed the 76 TypeScript and 6 JavaScript fixtures without an `ERROR`
tree.

## Decision

### D1. Grammar by extension

`.ts` uses `LANGUAGE_TYPESCRIPT`, `.tsx` uses `LANGUAGE_TSX`, and `.js` and
`.jsx` use `tree-sitter-javascript`. These are the four extensions the lane
admits. Each crate enters `Cargo.toml` in the slice that first reads it.

### D2. Three slices, in order

Each slice has its own preregistered experiment and lands on its own commits.

1. **Scope graph.** `ScopeGraphLite::analyze` reads the tree. The struct, its
   fields and its 31 consumers are unchanged, so there is one binding
   authority throughout.
2. **Framework anchors.** These read the tree:
   - receiver classification;
   - route and query call parts;
   - decorator roles.

   Decorator roles (NestJS) become role rows over the binding engine where
   their evidence is an exact decorator binding.
3. **Unit extraction.** The TS/JS units in `syntax.rs` come from the tree.

The binding engine's `ImportScope` models opened namespaces and aliases. A JS
import binds a local name to a module and an export name, so it does not fit
`ImportScope`, and slice 1 fills `ScopeGraphLite.imports` directly.

### D3. Policies stay; only readings change

Slices 1 and 2 keep every conservative policy of the lane:

- imports and framework receivers are read only at the top level;
- a name declared more than once at the top level is unsafe;
- a top-level reassignment makes a name unsafe from that byte on;
- a name declared anywhere inside an enclosing scope, or as a parameter of
  the function that owns it, is unsafe at every position in that scope.

A difference is allowed only where the tree reads the language correctly and
the legacy line reading did not.

### D4. Syntax errors

If a file's tree has an `ERROR` or `MISSING` node, no bindings are read from
it. Every unit the anchor pass would consider gets one typed UNKNOWN:

- reason `InsufficientSupport`;
- `affected_claim=tsjs_syntax`;
- `tsjs_unknown_kind=tsjs_parse_degraded`.

The legacy pass had no such refusal, and this is the only new UNKNOWN kind.

### D5. Oracle per slice

Slices 1 and 2 use the ADR-0058 tiers:

- an in-process differential of every unit's anchor decision (anchor, UNKNOWN
  kind or none), with field-level differences of `ScopeGraphLite` as
  diagnostics;
- the fixture-wide capture;
- `sync-equivalence`;
- `product-eval`.

`anchor_changed` needs explicit maintainer acceptance, citing the legacy
misreading for each case.

Slice 3 changes unit boundaries, so its oracle is family-level and
preregistered separately before it runs.

### D6. Legacy reader

The legacy implementation of each slice stays under `cfg(test)` as the
differential oracle until the maintainer accepts that slice's classification.
Slice 1's legacy reader was deleted on acceptance (2026-10-22); D6 continues
to govern slices 2 and 3.

## Consequences

- TS/JS stays transitional under the v0.1 Python-first scope. No language,
  framework, readiness label or public schema changes; D4 adds one UNKNOWN
  kind.
- The binary gains two grammars. Their size is measured in slice 1 and
  recorded in the migration report.
- Until slice 3, unit boundaries still come from the line-based extractor, so
  a unit can be anchored from a tree reading of a region the extractor bounded
  heuristically.
