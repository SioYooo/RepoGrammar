# TS/JS scope graph from the Tree-sitter tree (ADR-0059 slice 1)

Status: **CLASSIFIED, accepted 2026-10-22** (preregistered earlier). Base: `feat/tsjs-tree-sitter` at `4f1574e`. Local
only; no push or merge. Authority:
[ADR-0059](../decisions/ADR-0059-tree-sitter-tsjs-frontend-in-slices.md)
(Accepted).

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

## Results

Verdict: **`CLASSIFIED`**, with `unknown_removed` and `unknown_added` that need
maintainer acceptance (below). No `anchor_changed` anywhere. Rounds taken: 0
of 3 (no unattributable decision difference on a committed fixture, so no
fix round). Branch `feat/tsjs-scope-graph-slice1`, base `cc12f4a`.

### What changed

- `ScopeGraphLite::analyze(text, path, language)` parses with the grammar
  chosen by extension (ADR-0059 D1) and reads the tree with an iterative
  cursor (no `Node::parent()` loop). The struct, its fields' meaning and the 31
  consumers are unchanged; one field, `degraded`, is added.
- The receiver classifiers (`express_receiver_from_rhs`, the Fastify, Hono,
  Prisma and Drizzle readers, `express_router_prefix_mount`,
  `drizzle::table_declaration_parts`) are untouched and still the only
  classifiers; the tree hands them one declarator (`const <declarator text>`)
  or one expression statement.
- D4: `degraded` (tree with `ERROR`/`MISSING`, or scope work above
  `MAX_SCOPE_WORK` = 4,000,000 name-to-scope insertions) reads no binding.
  `anchor_for_unit` returns one `tsjs_parse_degraded` UNKNOWN
  (`InsufficientSupport`, `affected_claim=tsjs_syntax`) after its single
  kind dispatch, so the considered kinds are written once (the `_ => None`
  arm).
- `syntax.rs` changed in one token: the call
  `exact_test_runner_call_names(self.document.text)` became
  `exact_test_runner_call_names(&self.document)`, because that helper builds a
  scope graph and now needs the path and language. No unit, range or extractor
  logic changed.
- (Historical, deleted on acceptance.) The legacy reader was `tsjs/legacy_scope_graph.rs` (test-only) and the
  differential is `tsjs/scope_graph_differential.rs`.

### Counts

Decision level, per considered unit (each source is run without and with a
test-runner project context):

| Corpus | Sources | `anchor_changed` | `unknown_removed` | `unknown_added` | `unknown_kind_changed` |
| --- | --- | --- | --- | --- | --- |
| Committed fixtures | 82 | 0 | 0 | 0 | 0 |
| Branch corpus | 43 | 0 | 13 sources | 6 non-error sources + 6 error sources | 0 |
| External | 882 compared (2 skipped: non-UTF-8 or over 400 kB; 3 more excluded by the extractor panic below) | 0 | 0 | 0 | 48 |

Branch attribution (each source asserted in
`branch_corpus_decisions_match_their_attribution`):

- `unknown_removed` (the legacy missed a binding, the tree finds it):
  `multi_line_clause` (multi-line import, `require`, declaration);
  `brace_in_literal_or_comment` (`"{"`, `// {`, `/* { */`, `/{/`,
  `` `{` `` before the import); `extra_declarator` (`const n = 1, app =
  express()`); `declaration_not_at_line_start` (`start(); const app =
  express();`, `start(); import express ...`); `keyword_lookalike` (a template
  literal that contains `const app = ...`).
- `unknown_added` (the tree finds a shadow or a non-import the legacy missed):
  `rest_or_destructured_param` (`...app`, `{ app }`, `[app]`, `{ b: app = 1 }`
  shadowing a receiver); `keyword_lookalike` (`require("express").default` is
  not an express import; the legacy took it as one); `extra_declarator`
  (`let n2 = 2, app2 = 3, app = 4` redeclares `app`; the legacy read
  only the first declarator and missed it); and an `async
  function app` the legacy did not read as a declaration.
- `syntax_error`: 6 sources (stray `}`, unclosed block, `const broken = ;`,
  error inside a function, error in a test body, JSX in `.ts`); every
  considered unit gets exactly one `tsjs_parse_degraded`
  (`syntax_errors_refuse_every_considered_unit_once`).

External `unknown_kind_changed` (48): 46 are `tsjs_parse_degraded` in the 11
degraded files. The 2 others are one vitest bundle unit
(`index.C4tKZ0yW.js`, `test`) whose `unsafe_test_runner_binding` became
`unresolved_test_runner` because the legacy reader scoped a brace pair the
tree does not (non-scope brace pairs, below).

Degraded files (11 of 882): grammar gaps, not defects of the reading.
`tree-sitter-typescript` 0.23.2 does not parse `in`/`out` variance modifiers on
a type parameter (`interface $ZodCheck<in T = never>`, 4 zod `.ts` files), and
`tree-sitter-javascript` 0.25.0 does not parse a keyword as an export name
(`export { _null as null }`, `export { ..., function, ... }`; 7 zod `.js`
files). Each is refused conservatively, as D4 specifies.

Field level, committed fixtures: `decl_added` 38 (`async function`
declarations, which the legacy prefix check did not see); `shadow_changed` 16,
all `true -> false` (a function or class name that the legacy line overlap
filed inside its own body: `class UsersController {`, `function register(app)
{`). Neither class is in the preregistered misreading list; see decisions.

Field level, external (882 sources, 11 degraded files not compared). The
buckets come from a script over the difference list; the counts are exact for
`decl_removed`, `import_*`, `unsafe_removed` and `event_removed` and
heuristic (regex over the source) for `decl_added`.

| Class | Count | Attribution |
| --- | --- | --- |
| `decl_added` | 1,352 | `async function` (357, `declaration_not_at_line_start`-adjacent: legacy prefix check); `require` nested in a call (`__importDefault(require("x"))`, 231, compiled output, `keyword_lookalike`); legacy brace drift leaving depth above 0 at a column-0 declaration (641, `brace_in_literal_or_comment`); extra declarators (50); declarations not at line start (36); destructuring (21); `abstract`/`declare` (3); 13 other (`function*`, `export /*@__NO_SIDE_EFFECTS__*/ function`, an `interface` name, comma-continued lines) |
| `decl_removed` | 3 | type-parameter lines `In extends X = ...` the legacy took for a declaration (`keyword_lookalike`) |
| `import_added` | 1,158 | multi-line clauses (528 + 35 `require`, `multi_line_clause`); `import type` and inline `type` specifiers (593); 2 `import worker, {` multi-line |
| `import_removed` | 401 | `type` read as a binding by the legacy (170, `keyword_lookalike`); `const x = __importDefault(require("m"))` taken for a require import (231, `keyword_lookalike`) |
| `unsafe_removed` | 106 | the same `type` lookalike (97); 9 others (TS overload signatures and similar; not a listed class, see decisions) |
| `event_removed` | 17 | `name = default,` parameter and property lines taken for a top-level reassignment (`keyword_lookalike`) |
| `shadow_changed` | 21,655 | 20,804 `true -> false` where the legacy opened a scope for a brace pair that is not a block, class body, object or switch body (type literals, interface and enum bodies, destructuring and import clauses, template substitutions); 375 `true -> false` from the `type` lookalike; 464 `true -> false` and 12 `false -> true` in a scope both sides have (the `false -> true` are same-line bare assignments such as `if (hit) { skelPick = hit; ... }` that the legacy line reader missed, `declaration_not_at_line_start`) |
| `receiver_changed`, `import_changed` | 0 | |

Two classes are not in the preregistered list and are reported rather than
folded in: `non_scope_brace_pair` (the preregistration says one interval per
block-like node; the legacy scoped every `{}`) and `declaration_line_overlap`
(the legacy filed a function or class name inside the interval of its own
body). Both only remove over-conservative shadowing, and neither changed a
decision on a committed fixture. The 9 `unsafe_removed` overload cases are a
third. The 464 same-scope `true -> false` shadow differences were not
individually attributed beyond that bucket.

### Pre-existing defect of the line-based extractor (not a slice difference)

`syntax.rs::text_ends_with_keyword` sliced `text[offset - 6..offset]` without a
char-boundary check, so the source `败败a class A {}` panicked and aborted
`init` (exit 101) in published 0.5.1. 3 of the external files hit it; the
harness counted them with `catch_unwind` and did not compare them. Fixed on
`main` in `dca43c0` (pinned by
`declaration_after_multi_byte_text_does_not_panic`), independently of slice 3.

### Gates

- `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets
  --all-features -- -D warnings`, `cargo test --workspace --all-features`
  (Homebrew Python first): pass.
- Fixture-wide capture: 179 roots, 4,681 rows, 0 mismatches against the
  frozen baseline; `sync-equivalence` 14/14; `product-eval` equal.
- `repo-guard check`: passed.

### Measurements

| Measure | Before | After |
| --- | --- | --- |
| `scope_graph.rs` non-test lines | 901 | 778 |
| Release `repogrammar` size | 38,608,576 B (`33ed6a7`) | 41,899,392 B (+3,290,816, +8.5%) |
| Scope-graph analysis, 882 external sources | 0.50 s (legacy) | 2.68 s (tree, parse included) |

The size baseline is the `33ed6a7` measurement; later commits may account for a
little of the delta. The tree reader is about 5x slower than the line reader,
3 ms per source on average.

### Main-session review

- The preregistration missed a deviation channel. Besides the anchor pass,
  the line-based unit extractor reads the scope graph through
  `exact_test_runner_call_names`, which decides which aliased runner calls
  (`import { test as t } from "vitest"`) become `TestSuite`/`TestCase` units.
  So a graph difference can change the unit set, and the differential cannot
  see that change, because both of its sides take units from the same
  extractor.
- The harness therefore compares `jest_vitest::runner_call_names` of both
  graphs per source (`runner_names_changed`). It is the extractor's only
  dependency on the graph, so equal names mean equal units.
  - Committed fixtures: 0 (the exact-count assertion holds).
  - External: 0 of 943 sources, in a release test build over the five local
    projects and their installed `zod` and `vitest` packages (3 skipped,
    3 extractor panics, 11 degraded).
  - That run's decision classes are again only `unknown_kind_changed` (50), and
    its field counts are larger than the 882-source run's because the root
    list differs.
- Release timing on those 943 sources: 66 ms for the legacy reader and 653 ms
  for the tree reader, 0.7 ms per source including the parse. Each TS/JS file
  is analyzed twice: once for runner names in the extractor and once in the
  anchor pass. Sharing one graph belongs to slice 3, which owns the extractor.
- Residual risks:
  - In a degraded file, an aliased runner call is no longer a unit at all, so
    it is silently missing rather than carrying the degraded UNKNOWN. Calls to
    `describe`, `it` and `test` are still units and are refused. The corpus
    has no such file. Slice 3 should give these calls the UNKNOWN.
  - Class method parameters (`method_definition`) are not counted as
    shadowing names. The legacy reader also missed them (it searched for the
    `function` keyword), so this is not a regression. A method parameter named
    like a top-level receiver can still be read as that receiver.

### Decisions for the maintainer

1. Accept `unknown_removed` (13 branch sources, each a legacy misreading) and
   `unknown_added` (rest and destructured parameter shadows,
   `require(...).default`, `async function` redeclaration, syntax-error
   refusals). There is no `anchor_changed`, and none of either on a committed
   fixture.
2. Accept D4 refusal of the 11 external files the pinned grammars cannot parse
   (variance modifiers, keyword export names), or pick newer grammar versions
   in a later slice.
3. Accept the unlisted attribution classes (`non_scope_brace_pair`,
   `declaration_line_overlap`, overload signatures), or require them to be
   pinned back to the legacy reading.
4. Keep the legacy reader under `cfg(test)` until this is accepted (done: accepted 2026-10-22, see Acceptance).

## Acceptance

On 2026-10-22 the maintainer accepted the slice 1 classification: the
`unknown_removed` and `unknown_added` differences (branch-only, none on a
committed fixture, no `anchor_changed`); the D4 refusal of the 11 external
files with grammar gaps (TS variance `in`/`out` modifiers, JS keyword export
names); and the attribution classes the preregistration did not list
(`non_scope_brace_pair`, `declaration_line_overlap`, overload signatures).
The legacy reader (`legacy_scope_graph.rs`) and the differential
(`scope_graph_differential.rs`) are therefore removed from the build and
deleted. Their reproducers live on as plain tests in `scope_graph.rs` that
assert the tree reader's output on minimal inputs: `tree_reads_what_the_line_reader_misread`,
`tree_reads_shadows_and_non_imports`, `tree_scopes_only_block_like_nodes`,
`syntax_errors_refuse_every_considered_unit_once`, `runner_names_follow_the_graph`,
`deep_nesting_stays_bounded` and `tree_graph_reads_every_clause_form`. The
corpus-wide counts above are the recorded evidence and can no longer be
re-run.
