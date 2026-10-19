# TS/JS scope graph from the Tree-sitter tree (ADR-0059 slice 1)

Status: **PREREGISTERED**. Base: `feat/tsjs-tree-sitter` at `4f1574e`. Local
only; no push or merge. Authority:
[ADR-0059](../decisions/ADR-0059-tree-sitter-tsjs-frontend-in-slices.md)
(Proposed).

## Objective

`ScopeGraphLite::analyze` reads the `tree-sitter-typescript` or
`tree-sitter-javascript` tree instead of lines and raw braces. The struct, its
fields and its 31 consumers in `adapters/parsing/tsjs/` stay unchanged, and
`syntax.rs` unit extraction is not touched. The receiver right-hand-side
classifiers are kept: `express_receiver_from_rhs`, the Fastify, Hono, Prisma
and Drizzle declaration readers, and the Express prefix mount. The tree hands
them the declarator or statement text instead of a line. The outcome is
`EQUIVALENT`, `CLASSIFIED` or `BLOCKED` under the ADR-0058 tiers.

## What each field must keep meaning

| Field | Policy kept |
| --- | --- |
| `imports` | Top-level ES imports, all clause forms, and top-level `const`/`let`/`var` `require("…")` declarators, plain or destructured. Local name maps to module and kind (default, namespace, named original). Exported declarations count. |
| `local_decls` | Names declared at the top level by `const`/`let`/`var` (every declarator, destructuring included), `function` and `class`, excluding `require` declarators. |
| `unsafe_names` | Names declared more than once at the top level, imports included. |
| `unsafe_events` | Top-level bare reassignments `name = …` (not `==`, not `=>`), at the statement's start byte. |
| `scope_intervals` | One per block-like node: statement block, class body, object, switch body. Each holds every name declared anywhere inside it (transitively, as before) plus the parameters of the function whose body it is, now including rest, default and destructured parameters. |
| receiver maps | Top-level declarators and statements only, classified by the kept text classifiers. |

Whether a name declared in a nested block shadows the enclosing interval is
deliberately left over-conservative, as before. A more precise reading would
add anchors for policy reasons, not reading reasons, and is out of scope.

## Legacy misreadings that may explain a difference

Every difference must be attributed to one of these, with the source that
shows it:

- `multi_line_clause`: an import, `require` or declaration spanning lines.
- `brace_in_literal_or_comment`: `{` or `}` inside a string, template literal,
  regular expression or comment.
- `extra_declarator`: a second or later declarator (`const a = 1, b = 2`).
- `rest_or_destructured_param`: a `...rest`, `{ a }` or `[a]` parameter.
- `declaration_not_at_line_start`: a declaration or import after other code
  on its line.
- `keyword_lookalike`: a line the legacy reader took for a declaration or
  import (`const` inside a string, `import(...)` expression, `importFoo = …`).
- `syntax_error` (D4): a tree with an `ERROR` or `MISSING` node.

A difference with no attribution is a regression and is fixed, within at most
three rounds.

## Difference classes

Decision level, per unit the anchor pass considers:

- `unknown_added`: anchor before, UNKNOWN after.
- `unknown_removed`: UNKNOWN before, anchor after. Like `anchor_changed`, this
  needs maintainer acceptance, because it adds a support anchor.
- `anchor_changed`: the target or assumptions differ.
- `unknown_kind_changed`: UNKNOWN on both sides, with a different kind.

Field level (diagnostics, attributed but not gating):

- `import_added`, `import_removed` and `import_changed`;
- `decl_added` and `decl_removed`;
- `unsafe_added` and `unsafe_removed`;
- `event_added` and `event_removed`;
- `shadow_changed`: `name_is_unsafe_at` at a considered unit's start byte;
- `receiver_changed`.

## Corpora

- Every committed TS/JS fixture source: 82 files under `src/fixtures`.
- A branch corpus in the test module. It covers every field, every misreading
  class above and its near-misses, every framework receiver form, and every
  syntax-error placement.
- An external corpus: TypeScript and JavaScript sources of the local projects
  under `/Volumes/External/Development`, excluding `node_modules`, plus
  their installed `zod`, `vitest` and `fastify` packages. Decisions are
  compared only where a file has a unit the anchor pass considers. Field
  differences are counted everywhere.

## Gates and stop rules

1. Committed fixtures: the fixture-wide capture (179 roots), `sync-equivalence`
   14/14 and `product-eval` equal. A capture mismatch must be attributed like
   any other difference.
2. An `anchor_changed` or `unknown_removed` on a committed fixture that cannot
   be attributed to a listed misreading ends the slice as `BLOCKED`.
3. At most three rounds. Each round targets the observed difference class
   with one change and a negative test.

## Measurements

Non-test lines of `scope_graph.rs` before and after; release binary size
with the two grammar crates; and parse time on the external corpus,
before and after.
