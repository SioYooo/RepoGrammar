# TS/JS unit extraction from the Tree-sitter tree (ADR-0059 slice 3)

Status: **accepted 2026-10-22** (see "Acceptance"; the preregistered rule
reported `BLOCKED` pending exactly these decisions; preregistered in
`fe45ee2`; see "Results"). Base: `main@5097d7a`
(`feat/tsjs-unit-extraction-slice3`). Authority:
[ADR-0059](../decisions/ADR-0059-tree-sitter-tsjs-frontend-in-slices.md)
(Accepted), D2 slice 3, D3 (policies stay), D4 (syntax errors), D5 ("Slice 3
changes unit boundaries, so its oracle is family-level and preregistered
separately before it runs"), D6 (the line-based extractor stays under
`cfg(test)` as the differential oracle). Slices 1 and 2:
[tsjs-scope-graph-tree](tsjs-scope-graph-tree.md),
[tsjs-framework-anchors-tree](tsjs-framework-anchors-tree.md). Tiers and
difference classes: [ADR-0058](../decisions/ADR-0058-tree-sitter-frontends-for-every-language.md).

## Acceptance

The maintainer accepted all five decisions of "Decisions for the maintainer"
on 2026-10-22: (1) the `drizzle_exact_repositories` `anchor_changed`
(`listOrders` `where_shape` `object_literal` to `none`; the `drizzle_query`
family loses the `tsjs_drizzle_query_shape` variation slot and the three select
members shrink to their statements); (2) the frozen product-eval query
`ts-member-express-get` follows the tree's unit end, `56-107` to `56-108`
(amended in `query-corpus-v1.json`, gold semantics unchanged); (3) a refused
tree may yield no `tsjs_parse_degraded` rows when error recovery leaves no
considered unit; (4) the attributed unit-level classes and the preregistration
deviations (`receiver_token_start`, `accessor_name`, `unit_kind_changed`,
`degraded_unit_set`, widened `unit_start_corrected`, member-call units end at
their chain's end); (5) deletion of the `cfg(test)` oracle.

The oracle is detached: `parsing/legacy_units.rs`,
`tsjs/unit_differential.rs` and `tsjs/unit_corpus.rs` are no longer compiled,
together with the `cfg(test)`-only `exact_framework_anchors`,
`exact_test_runner_call_names` and three unused re-exports; the files are
removed from the tree by the main session. The accepted readings are held by
plain tests with no legacy call: in `tsjs/units.rs`
(`classes_and_members_come_from_class_nodes`,
`nest_decorators_start_the_unit_at_the_earliest_decorator`,
`functions_and_arrows_start_at_their_keyword_and_end_at_the_node`,
`route_units_are_made_once_per_chain_at_the_chain_start`,
`prisma_and_drizzle_units_hold_the_whole_chain`,
`text_that_is_not_code_or_a_receiver_is_not_a_unit`,
`a_member_chain_unit_starts_at_the_chain_base_inside_a_function`,
`a_degraded_graph_still_sees_the_ambient_table_factories`), in `tsjs/mod.rs`
(`drizzle_select_does_not_read_the_next_functions_where_clause`) and in
`tsjs/scope_graph.rs` (`syntax_errors_refuse_every_considered_unit_once`,
which includes the tree that leaves no considered unit). The byte capture taken
after acceptance (`main@a6fb3b0`: 179 roots, 4,680 rows) is the new baseline,
SHA-256 `b90df67a55bee7a3ba56f71fec25881faab0454a8dac29ff10f2ffdc11878527`,
retained with its predecessors under
`/Volumes/External/Development/repogrammar-evidence/capture-baselines-20261022/`.
Against the post-slice-2 baseline it differs in 507 rows over 28 roots, every
one of which holds TS/JS sources, on the `generation`, `units`, `member`,
`explain`, `check`, `find`/`mcp_find`, `family`/`mcp_family` and `resync`
surfaces; the `files`, `families` and `unknowns` surfaces are identical.

## Objective

The TS/JS code units that `adapters/parsing/syntax.rs` makes (`SyntaxScanner`)
are read from the Tree-sitter tree that `ScopeGraphLite` already keeps, so a
file is parsed once for units, bindings and anchors. Unit kinds, framework
policies and the UNKNOWN taxonomy do not change (D3). The line-based extractor
moves under `cfg(test)` unchanged and is the differential oracle.

Because a unit boundary is a tree node boundary, unit ids (which embed the byte
range) and some unit sets change. The fixture-wide byte capture is therefore
not the pass criterion (item 4 below is a diagnostic). The outcome is decided
on families and the UNKNOWN inventory.

## Readings replaced

`start` is the byte the unit starts at. Where the line reader's start is the
right place it is kept (so the unit id changes only by its end byte); where the
tree reads a different node, the start follows the node. `end` is always the
end of the named syntax node (never the legacy "matching brace of the first
`{` after the start, else the end of the line including its newline"). The
legacy readings are all line scans: `lines_with_offsets`, `find_keyword`,
`parse_identifier_after`, substring `find`, and `declaration_extent`
(first `{` anywhere after the start, brace-counted over raw bytes).

| Unit kind (name) | Legacy scan and text heuristic | Tree node (start / end) |
| --- | --- | --- |
| `module` (`module`) | the whole file | unchanged |
| `class`, `nest_controller`, `nest_injectable`, `nest_module` | `scan_classes`: any line holding the keyword `class` then an identifier (comments, strings and `class X` text included); end by `declaration_extent`; Nest kind from `@Name` text found by `decorator_prefix_start` walking back over `)`/`export`, first of `Controller`/`Injectable`/`Module` | `class_declaration`, `abstract_class_declaration`, or a named `class` expression with a `name`. Start: the `class` keyword; for a Nest kind the earliest `decorator` of the class (children of the class or of its `export_statement`). Kind: first `decorator` whose expression is an identifier or a call of an identifier named `Controller`/`Injectable`/`Module`. End: the node end |
| `method`, `nest_route` | `scan_class_methods`: every line inside a class range for which `method_name_from_line` finds `[modifiers] ident ... (` with no `=` before the paren and no `=>` after it (so `this.x()`, `await f()` and `console.log()` statements are methods); Nest route from `@Get|Post|Put|Delete|Patch|Head|Options|All` text | `method_definition`, `method_signature`, `abstract_method_signature` that are direct members of a `class_body`, named by their `name` field. Start: the name (modifiers excluded as before), for a Nest route the earliest leading `decorator`. End: the node end |
| `function`, `react_component`, `react_hook` | per line outside classes: the keyword `function` then an identifier (`*` allowed); `classify_callable` by name (`use[A-Z]` hook; capitalised and (`.tsx`/`.jsx` path or the text contains `return <`, `</` or `/>`) component) | `function_declaration`, `generator_function_declaration`, `function_signature`, and named `function_expression`/`generator_function`, outside any class. Start: the `function` keyword. End: the node end. JSX presence: any `jsx_element`, `jsx_self_closing_element` or `jsx_fragment` node in the file |
| `arrow_function`, `react_component`, `react_hook` | per line outside classes: first `const`/`let`/`var`, an identifier, the first `=`, and a right-hand side that heads an arrow (`async`? `(...) =>` or `ident =>`) | `variable_declarator` with an `identifier` name whose `value` is an `arrow_function`, in a `lexical_declaration`/`variable_declaration` outside any class; every declarator, not only the first. Start: the declaration keyword for the first declarator, the declarator for later ones. End: the declarator end |
| `express_route`, `hono_route`, `fastify_route` (`get head post put delete options patch all use`) | `static_route_call`: first of those names (list order) for which the line holds `.name(`; receiver is the text back to the previous whitespace; one unit per line; kind from the extractor's own text receiver scans `fastify_receivers_for_scan`, `hono_receivers_for_scan` | `call_expression` whose `function` is a `member_expression` with a `property_identifier` in that set, taken once per call chain (the innermost matching call of the chain). Start: the call start (the chain start). Kind: receiver `identifier` in the file's declared Hono/Fastify receivers (see below), else Express. End: the call end |
| `fastify_route` (`route`), `fastify_plugin_registration` (`register`) | `fastify_full_route_call`, `fastify_register_call` (`.route(`/`.register(` on a Fastify receiver; register ends at the end of the line) | the same call node with property `route`/`register` on a Fastify receiver `identifier` |
| `express_route`, `fastify_route` (`dynamic_route`) | `dynamic_route_call`: `recv[...](` with a quote after the `]` on the line | `call_expression` whose `function` is a `subscript_expression` and whose arguments hold a `string`; receiver text must begin with an identifier character |
| `next_app_page`, `next_app_layout`, `next_pages_page`, `next_pages_api_route` (`default`/`handler`) | `next_default_export_unit`: path convention plus a line holding `export default` followed on that line by `function`, `=>` or `createElement` | `export_statement` with a `default` whose value is function-like, or contains a function-like node or a `createElement` call that starts on the statement's first line. Path convention (`next_file_convention_kind`) unchanged. Start: `export`. End: the statement end |
| `next_route_handler` (`GET` ... `OPTIONS`) | `next_route_handler_export`: line holds `export `, then the substring `function GET` or `export const GET = async ... =>` | `export_statement` declaring `function GET`... or `const GET = async` arrow. Start: `export`. End: the statement end |
| `prisma_query` (operation, `raw`) | `prisma_query_call`: `.$queryRaw(` etc. (identifier before the dot), or `.op(` for 13 operations preceded by `.model` and an identifier; start at that identifier; first match per line | `call_expression` whose callee is `ident.model.op` with `op` in the 13 operations (three identifier segments), or `x.$queryRaw|$executeRaw|$queryRawUnsafe|$executeRawUnsafe`. Start: the call start. End: the call end |
| `prisma_transaction` | `.$transaction(` with an identifier before the dot | `call_expression` with property `$transaction`. Start: the call start |
| `drizzle_schema_table` | `drizzle_schema_table_declaration`: a line `[export] const|let|var name = factory(` | `variable_declarator` with identifier name and a `call_expression` value whose callee is an `identifier` in the factory set (`pgTable`, `mysqlTable`, `sqliteTable` and their aliases imported from `drizzle-orm/*-core`). Start/end as for arrows |
| `drizzle_query` (`select insert update delete execute query_findMany query_findFirst`) | `drizzle_query_call`: `.query.<t>.findMany(` / `findFirst(`; `.execute(` and `.select/insert/update/delete(` whose receiver identifier is not preceded by `.`; `select` needs `.from(` later on the line; insert/update/delete skipped when the first argument starts with a quote | `call_expression` with callee `ident.query.table.findMany|findFirst`, or `ident.execute`, or `ident.select|insert|update|delete` with `identifier` object; `select` only when its call chain continues with a `.from(` call; `insert`/`update`/`delete` not when the first argument is a `string`. Start: the call start |
| `drizzle_transaction` | `.transaction(` with an identifier before the dot | `call_expression` with property `transaction`. Start: the call start |
| `zod_schema` | `zod_schema_declaration`: `[export] const name = z.builder(` with `builder` in `object union discriminatedUnion enum array` and `z` a named or default import of `zod`/`zod/v4` | `variable_declarator` whose value's call chain begins with `z.builder(...)`, `z` a named (`z`, aliased) or default import of `zod` or `zod/v4` (namespace imports stay excluded as before). Start/end as for arrows |
| `test_suite`, `test_case` | `call_offset`: the first bare `name(` of each runner name on the line (imported `describe`/`it`/`test` aliases from `exact_test_runner_call_names`, plus ambient `describe`, `it`, `test`), not preceded by `.` | `call_expression` whose `function` is an `identifier` of that name. The names come from the graph's own `imports` (`runner_call_names`), so there is no second analysis. Start/end: the call |

Receiver sets. The extractor's own loose scans (`fastify_receivers_for_scan`,
`hono_receivers_for_scan`, `zod_bindings_for_scan`,
`drizzle_table_factories_for_scan`) ignore the unsafe-name policy that the
anchor pass applies. The tree units keep that looseness: the graph computes a
second receiver classification with an empty unsafe-name set from the same
top-level declarators (one reader, `parts::receivers`, called twice). Unit kind
therefore does not depend on shadowing; the anchor still decides.

Unit order and ordinals. The legacy emission order (classes, then methods, then
per line the patterns in the order above) fixes the ordinal in each unit id.
The tree extractor sorts its candidates by (phase, line of the legacy anchor
offset, pattern rank), so two identical unit sets carry identical ids.

Not changed: the module unit, `tsjs_project_config_output`, import resolution
facts (`tsjs_import_resolution_facts`, which still read lines), and the
unbalanced-delimiter diagnostic (`delimiters_are_balanced`, a byte count). They
produce facts and diagnostics, not units; they are named here as residuals.

## Considered unit (D4)

A *considered unit* is a unit, extracted from the file's tree, of a kind that
`anchor_for_unit` dispatches on: the Express, Fastify, Hono, Next.js, Prisma,
Drizzle, Zod, NestJS and test suite/case kinds. Kinds that dispatch to no
anchor (module, class, method, function, arrow function, React component/hook,
project config) are not considered.

When the tree has an `ERROR` or `MISSING` node the extractor still walks it
(tree-sitter error recovery keeps well-formed subtrees; an `ERROR` node is
never itself a unit). Every considered unit it emits gets exactly one
`tsjs_parse_degraded` UNKNOWN and no anchor, as in slice 1. The same holds when
the file exceeds the scope-work bound (the tree is kept for the units, the
graph is degraded). If no tree can be produced at all, the file has only its
module unit. The set of units of a degraded file may differ from the legacy
set; that is class `degraded_unit_set` below, counted separately and never
folded into the other classes.

## Expected misreadings of the line reader

Every difference must be attributed to one of these, with the source that shows
it. Names are fixed before the first run.

- `unit_not_code`: a unit made from a line inside a comment, string, template
  literal or regular expression; or a method named `describe`/`it`/`test`
  taken for a test call; or a `class`/`function`/`const` keyword inside text.
- `statement_taken_for_method`: a statement inside a class body
  (`this.x()`, `await f()`, `console.log()`, an object-literal method) taken for
  a `method` unit.
- `member_receiver`: a unit that starts at an identifier in the middle of a
  member chain (`this.app.get(`, `a.b.findMany(`, the Prisma-shaped unit at
  `query.users.findMany(`). The tree starts it at the chain start or does not
  make it.
- `keyword_receiver`: `this`/`super` as receiver.
- `extent_by_first_brace`: `declaration_extent` stops at the matching brace of
  the first `{` after the start: a brace in a type literal, destructuring
  parameter or string, the next body for a body-less declaration, or the end of
  the line (newline included) for a one-line unit. The end becomes the node end.
- `one_unit_per_line`: the line reader makes at most one route/Prisma/Drizzle
  unit per line (first match in list order), so a second call on a line, a
  chain member (`.post(...)` after `.get(...)`) or a leading-dot chain line is
  missed or mis-started.
- `declarator_shape`: only the first declarator is read; a type annotation
  with `=` (`const f: (a) => void = ...`); a generic arrow
  (`<T,>(x: T) => x`); a declaration after other code on its line.
- `decorator_walk`: the backward walk over `)` and `export` misses a decorator
  with comments, a member decorator (`@ns.Name()`), or a decorator on a line
  that is not directly above.
- `jsx_text_lookalike`: `contains_jsx` is true because `</` or `/>` appears in a
  string, comment, regular expression or generic.
- `export_default_wrapper`: `export default` followed on its line by a
  function-like text that is not the exported value, or a wrapped value
  (`memo(function X() {})`) that the line reader accepted or rejected by
  substring.
- `degraded_unit_set` (D4): the file's tree has an `ERROR`/`MISSING` node.

A difference with no attribution is a regression and is fixed within at most
three rounds. A tree reading that is correct but belongs to no listed class is
recorded as a preregistration deviation, never folded into a listed class.

## Difference classes

Unit level (per unit, matched by kind, name and start byte):

- `unit_boundary_only`: same unit, the end byte differs.
- `unit_retired`: a legacy unit that has no tree counterpart (typically a
  `unit_not_code`).
- `unit_added`: a tree unit with no legacy counterpart.
- `unit_start_corrected`: same kind and name, the legacy start lies inside the
  tree unit (the mid-chain case).
- `unit_split` / `unit_merged`: one legacy unit against two or more tree units
  of its kind inside its range, or the reverse.
- `anchor_changed`: a matched unit whose anchor (target or assumptions)
  differs, or an anchor appears or vanishes without an UNKNOWN. **Blocks;
  maintainer gate.**
- `unknown_kind_changed`: UNKNOWN on both sides with a different kind.
- `degraded_unit_set`.

Family level (per fixture root, the fixture oracle):

- `family_changed`: a family appears or disappears, or its member set, anchor
  kind or support count changes. **Blocks, unless attributable to
  `unit_retired`**: every member that disappeared belongs to a retired unit,
  no member appeared, and support changes only in families that lost such a
  member.
- UNKNOWN inventory: a difference in (kind, path, line) is acceptable only when
  its unit is retired, added without an anchor, or classed
  `unknown_kind_changed` / `unit_start_corrected`; otherwise it blocks.

## Oracle

Baselines are produced from the release build of `main@5097d7a` before any
change. Evidence is copied to
`/Volumes/External/Development/repogrammar-evidence/tsjs-slice3-20261022/`.

Baseline binaries (SHA-256): `repogrammar` `5429d8d2e96b6d2ff0b7b1ebb64a4de1adba7fe74e8b1901a828797bba1e24f7`,
`repo-guard` `cfd1468e97df360c8d1ecf82212eaf1d26df1c5374f91715fb4cb8198a599a1a`.

1. **Family-level differential on the 179 fixture roots** (the roots and the
   isolated workspace of `capture.py`; the 179th is the empty untracked
   `src/fixtures/typescript/release/v0_1/stale-index`, created by `mkdir -p`
   in a worktree). The command, once with the baseline binary and once with the
   candidate:

   ```text
   python3 -I src/experiments/language_pack/family_diff.py snapshot --repo . \
     --bin <repogrammar> --python /opt/homebrew/bin/python3 --scratch <dir outside the checkout> --out <snapshot.json>
   python3 -I src/experiments/language_pack/family_diff.py diff --before <base> --after <candidate> --out <diff.json>
   ```

   A snapshot holds, per root, the TS/JS units (path, kind, name, start, end,
   start line, facts), the families (id, classification, support, members as
   path, start line, role, anchor kind) and the UNKNOWN inventory (kind, reason,
   path, line). Unit ids and content hashes are not compared. Baseline:
   `family-snapshot-base.json`, SHA-256
   `b9c65fbe46aba7901787b1241f8f999b56d4841e5b6557e0acdcec6f3daf876a`
   (179 roots; 81 TS/JS modules in 31 roots; 117 families; 358 UNKNOWN rows;
   TS/JS units by kind: arrow_function 3, drizzle_query 10,
   drizzle_schema_table 4, express_route 26, fastify_route 9, function 44,
   hono_route 3, module 81, nest_controller 4, nest_route 12, next_app_layout
   3, next_app_page 3, next_pages_api_route 3, next_pages_page 3,
   next_route_handler 4, prisma_query 15, prisma_transaction 2,
   react_component 13, react_hook 2, test_case 35, test_suite 17,
   zod_schema 3). Non-TS/JS roots must be `identical`.
2. **`repo-guard product-eval`** on `src/fixtures/evaluation/query-corpus-v1.json`
   with `env PATH=/opt/homebrew/bin:/usr/bin:/bin`. Baseline
   (`product-eval-base.json`, SHA-256
   `2fdb3ddb9d81f1f0850615346638bfa1f472cf42c94bf7f27b456d9882535b67`):
   110 queries, 97 match, 13 mismatch; `by_intent`: retrieval 34/47,
   abstention 36/36, context 27/27; `committed_precision` 42/42; hit@1 34/47;
   false family rate 0/55. Must hold exactly; only `latency_ms_*` may differ.
3. **`repo-guard sync-equivalence --fixture src/fixtures/incremental_equivalence/v1 --all`**:
   14/14 PASS (baseline `sync-equivalence-base.json`, SHA-256
   `54598b57e36cbccf87adbc90b36ab2927f94aad05e42f61d8902c738ea3e957f`).
4. **Byte capture, a diagnostic**:
   `capture.py --compare /Volumes/External/Development/repogrammar-evidence/capture-baselines-20261022/base-capture-main-8cdefea.json`
   (SHA-256 `53d288682c17a4c8c1afab5c0d422d67502d0304bb4dd4a638c6766b6f00a389`;
   the baseline binary must reproduce it with 0 mismatches). Its mismatch count
   is recorded and every mismatching root is attributed to a class above (a
   root with TS/JS units differs by unit id at least by the end byte).
5. **In-process unit differential** (`cfg(test)`, the legacy extractor against
   the tree extractor, each source run without and with a test-runner project
   context), classified into the unit-level classes, with the anchor decision
   per unit as in slice 2 (anchor, UNKNOWN kinds, or none):
   - the 82 committed TS/JS fixture sources;
   - a branch corpus (one source per misreading class with its near-misses,
     every unit kind, every syntax-error placement);
   - the external corpus of slices 1 and 2: the TS/JS sources of
     `SurchargeCheck` (with its installed `vitest`), `SurchargeCheck-seo`,
     `askveil` (with its installed `zod`) and `poppet-public-seed` under
     `/Volumes/External/Development`, `node_modules` excluded elsewhere, files
     over 400 kB or not UTF-8 skipped and counted.

Gates for each code commit: `cargo fmt --all -- --check`; `cargo clippy
--workspace --all-targets --all-features -- -D warnings`; `cargo test
--workspace --all-features`; `git diff --check`; `cargo run --quiet --bin
repo-guard -- check`.

## Decision rule

- `EQUIVALENT`: no family-level difference on any root, no UNKNOWN inventory
  difference, and the units differ at most by `unit_boundary_only`.
- `CLASSIFIED`: every difference is in a preregistered class; no
  `anchor_changed`; every `family_changed` is attributable to `unit_retired`;
  UNKNOWN inventory differences only as allowed above. `unit_retired`,
  `unit_added` that carry an anchor or UNKNOWN, `unknown_kind_changed` and
  `degraded_unit_set` are listed for maintainer acceptance with their legacy
  misreading.
- `BLOCKED`: an `anchor_changed`, or a `family_changed` not attributable to
  `unit_retired`, or an unattributed UNKNOWN difference, on a committed
  fixture; or a reading that cannot reach `CLASSIFIED` in five rounds. The
  branch is kept, nothing is accepted on the maintainer's behalf, and the
  oracle is not tuned.
- Product-eval or sync-equivalence not holding exactly is `BLOCKED` regardless
  of the class counts.

## Stop conditions

1. A committed-fixture `BLOCKED` condition ends the slice; report, keep the
   branch.
2. At most five rounds. A round targets one observed unattributed class with
   one change and a negative test.
3. No new dependency; no policy, readiness, schema or UNKNOWN-taxonomy change;
   no family-kind change; no change to a non-TS/JS lane.
4. One parse per file: `ScopeGraphLite` carries the tree; the extractor and the
   anchor pass share it, and `exact_test_runner_call_names` (a second analysis)
   leaves the production path.
5. Tree traversal is iterative (cursors and an explicit stack; `Node::parent()`
   is root-descending), and no byte slicing of the source at a non-boundary
   (`declaration_after_multi_byte_text_does_not_panic` keeps passing).

## Measurements

Non-test lines of `syntax.rs` and the new unit reader before and after; parse
count per file (two before: extractor runner names and anchor pass); release
extraction time per file on the external corpus before and after; release
binary size.

## Implementation log

- Shared parse. `TsJsAnalysis` (in `tsjs/mod.rs`) holds the one
  `ScopeGraphLite` and its `Tree`; `SyntaxScanner::scan` reads the units from
  it (`tsjs/units.rs`, one iterative cursor walk with an ancestor stack) and
  `finish` hands the same analysis to the anchor pass. `ScopeGraphLite` now keeps
  its tree when the graph is degraded (an `ERROR`/`MISSING` tree, or the scope
  work bound), so the unit reader still walks it. `exact_test_runner_call_names`
  (the second analysis) and `exact_framework_anchors` are `cfg(test)` only.
- Legacy oracle. The whole line-based extractor moved unchanged to
  `parsing/legacy_units.rs` (`#[cfg(test)]`); `parsing/tsjs/unit_differential.rs`
  runs it against the tree extractor; `unit_corpus.rs` is the branch corpus.
- Unit kinds. Every row of "Readings replaced" reads the node named there.
  Additions to the table that the first runs required, all recorded as
  preregistration deviations:
  - A member-call unit (`route`, Prisma, Drizzle, transaction) ends at the end
    of its call chain (the outermost `.member(...)` call that continues it), not
    at the inner call. With the inner call's end the Drizzle shape features
    (`where_shape`, `returning_shape`), which are read from the unit text, lost
    the `.where(...)` and `.returning()` of the same statement.
  - A class member's decorators are the preceding `decorator` siblings in the
    `class_body`, not children of the `method_definition`.
  - `ends_in_identifier`: the Prisma and `$transaction` receivers keep the line
    reader's "identifier before the dot" test (an identifier or a member chain
    ending in a property identifier), so `this.prisma.user.create(` is still a
    unit, now starting at `this`.
  - Unit kind in a degraded file. The loose receiver, Zod and table-factory
    names come from the graph's imports, which a degraded graph does not read
    (D4), so in a degraded file a Hono/Fastify route is an Express route unit and
    aliased runner names, Zod imports and aliased table factories are not seen
    (the three default Drizzle table factories are seeded, as the line reader
    always had them). Every dispatched unit there is refused once, as before;
    but a tree whose error recovery leaves no call node yields no unit to refuse
    (`error_unclosed.ts`), a D4 observability change reported under "Results".
- Additional difference classes the differential uses, all inside the
  preregistered misreading list or stated here:
  - `receiver_token_start` (new misreading name): the line reader starts a route
    unit at the whitespace-delimited token before `.get(` (`(request.headers`,
    `mw)`); the tree starts it at the chain start. It is classed
    `unit_start_corrected` when the two units overlap, so the definition of
    `unit_start_corrected` is "same kind and name, overlapping ranges, different
    start", wider than "the legacy start lies inside the tree unit".
  - `unit_kind_changed` (new class): same name and start, another kind
    (`react_component` by `</` text against `function` by the tree,
    `jsx_text_lookalike`).
  - `accessor_name` (new misreading name): the line reader names a `get x()`
    accessor `get`; the tree names it `x`.
  - `string_prefix_expression` for the dynamic route: `console['warn']('a' + 'b')`
    had a quote on the line; the tree requires a string argument.
  - `one_unit_per_line`, `declarator_shape` and `decorator_walk` are read off
    the differential as `unit_added` reasons; `statement_taken_for_method` and
    `unit_not_code` as `unit_retired` reasons.

## Results

Verdict: **`BLOCKED` by the preregistered decision rule**, pending maintainer
acceptance of two attributed items. Both are legacy misreadings of the line
reader that the tree no longer makes; neither is a regression of the tree
reading, and no oracle was tuned. Fix rounds taken: 1 of 5 (the first full run
exposed the call-chain end, the decorator siblings and the degraded ambient
factories; each was one change with a test). Commits: `fe45ee2`
(preregistration), `72f7e98` (units read from the tree), `c4542e4` (ambient
Drizzle factories in a refused tree, differential reasons), and the results
commit. Binaries and JSON are retained in
`/Volumes/External/Development/repogrammar-evidence/tsjs-slice3-20261022/`.

The rule I preregistered is stricter than ADR-0059 D5, which lets the
maintainer accept an `anchor_changed` that cites a legacy misreading. The two
items below would be accepted under D5; under this document's rule
("`CLASSIFIED` ... no `anchor_changed`", "product-eval not holding exactly is
`BLOCKED` regardless of the class counts") they block. The rule was not
rewritten after the run.

### Decisions for the maintainer, first

1. **One `anchor_changed` on a committed fixture** (attributed to
   `extent_by_first_brace`). `drizzle_exact_repositories/src/repositories.ts`,
   `listOrders`: `db.select().from(orders).where(eq(orders.id, 1))`. The line
   reader ended the unit at the matching brace of the first `{` after its
   start, which is the body of the next function (`findUsers`), so it read
   `where:` from `db.query.users.findMany({ where: ... })` and recorded
   `where_shape=object_literal` for `listOrders`. The tree reads
   `where_shape=none`, the right value. Product-visible consequences, all in
   that one root: the `drizzle_query` family loses the
   `variation:tsjs_drizzle_query_shape` slot and its
   `false,none,object_literal` profile (the misread was the only variation);
   the three `select` members shrink from 154/154/152 bytes (`311-465`,
   `410-564`, `513-665`: each ran into the next function) to the statement
   (`311-357`, `410-462`, `513-561`), so `read_plan` spans and
   `end_line` change (`20` to `15`). Members, support and the UNKNOWN
   inventory do not change.
2. **`product-eval` does not hold exactly on the committed corpus**: 96/110
   (baseline 97/110), `by_intent` retrieval 33/47 (34/47), hit@1 33/47,
   `committed_precision` 41/41 (42/42); abstention 36/36 and context 27/27
   hold. One query, `ts-member-express-get`, pins a unit id
   (`unit:express_exact_routes/app.ts#express_route:get:56-107:1`) whose end
   byte is the legacy brace end; the tree unit is `56-108` (the call's `)`),
   so the lookup answers `exact_lookup_unknown`. With that one `target`
   rewritten to `56-108` in a scratch copy of the corpus, every number returns
   to the baseline exactly (97/110, 34/47, 36/36, 27/27, 42/42; retained as
   `product-eval-new-idfollow.json`). `query-corpus-v1.json` is frozen gold and
   was not edited; changing that one `target` is the maintainer's call. No
   other TS/JS unit id is pinned in the corpus, the docs or the tests.
3. **`degraded_unit_set` (D4)**: a refused tree no longer gives the same unit
   set. On the 11 refused external files no dispatched unit appears or
   disappears except 2 `test_suite` rows from a comment, so the refusal count
   there is unchanged. But error recovery can leave no call node at all (the
   unit test `error_unclosed.ts`: an unterminated route call), in which case
   there is no unit to refuse and the file yields zero `tsjs_parse_degraded`
   rows where the line reader made and refused a unit. The alternative that
   keeps D4 exactly is to keep the line reader in production for refused trees
   only, which contradicts "units come from the tree". Accept the tree
   behaviour, or require that fallback.
4. **Accept the attributed unit-level classes** below (`unit_retired`,
   `unit_added`, `unit_start_corrected`, `unit_kind_changed`,
   `unknown_kind_changed`, `degraded_unit_set`), each with its legacy
   misreading.
5. **Preregistration deviations** (listed so none is folded into a listed
   class): the end of a member-call unit is its call chain's end;
   `receiver_token_start`, `accessor_name`, `unit_kind_changed` are new names;
   `unit_start_corrected` was widened from "the legacy start lies inside the
   tree unit" to "same kind and name, overlapping ranges"; the family
   differential's attribution rule is as preregistered.
6. Keep `legacy_units.rs`, `unit_differential.rs` and `unit_corpus.rs` under
   `cfg(test)` until this is accepted (D6).

### Oracle 1: family-level differential, 179 roots

`family_diff.py`, baseline `main@5097d7a`, candidate at `c4542e4`
(snapshots `family-snapshot-base.json` and `-final.json`, diff
`family-diff-final.json`, SHA-256
`e3c207ae1b09fb7670d8b131e1257d5955d178b5c03b57688edad9527b26496b`).

| Root label | Roots | Meaning |
| --- | --- | --- |
| `identical` | 151 | no unit, family or UNKNOWN difference (all 148 non-TS/JS roots and 3 TS/JS roots) |
| `unit_boundary_only` | 26 | same units and facts, the end byte differs |
| `unit_retired` | 1 | `express_exact_routes/lookalikes.ts`: the word `function` in a comment was a unit |
| `anchor_changed` | 1 | `drizzle_exact_repositories`, decision 1 (it also holds 2 `unit_start_corrected`) |
| `family_changed` | 0 | no family, member, anchor kind or support difference on any root |
| UNKNOWN inventory difference | 0 | no (kind, path, line) row appears or disappears |

Unit level, in-process, the 82 committed sources, each without and with a
test-runner context (570 legacy units, 568 tree units): `unit_boundary_only`
258, `unit_retired` 2 (`not_code`), `unit_start_corrected` 4 (the Prisma-shaped
unit at `query.users.findMany(` now starts at `db`; both sides are the same
UNKNOWN kind, `prisma_dynamic_model_or_operation`), `anchor_changed` 2 (decision
1). The branch corpus (30 sources, one per misreading class with near-misses and
the two syntax-error placements) differs exactly as attributed
(`branch_corpus_differences_match_their_attribution`).

### Oracle 5: external corpus (in-process)

885 sources compared (slice 1's roots), 2 skipped, 0 extractor panics, 11 refused
files. Both contexts counted; legacy 25,932 units, tree 22,800.

| Class | Count | Reasons |
| --- | --- | --- |
| `unit_boundary_only` | 7,554 | `test_case` 4,496, `express_route` 1,078, `function` 902, `arrow_function` 652, `prisma_query` 102, `method` 114, `test_suite` 88, `drizzle_query` 68, `zod_schema` 46, `class` 8 |
| `unit_retired` | 3,964 | `statement_in_class` 3,090 (`this.go()`, `await f()`, object-literal methods read as methods); `not_code:text` 794 (`class`/`function`/`describe` in comments and strings); `keyword_receiver` 58 (`this.items.delete(`, `this.get(`); `not_code:method_name` 8 (`describe(d) {`); `string_prefix_expression` 8 (`console['warn']('a' + 'b')`); `other` 6 (3 sources, `Map(this.get(x))` and `...(this.get(p) ?? {})`: the legacy start is the token `Map(this`, the same `keyword_receiver`/`receiver_token_start` reading, not paired by the differential) |
| `unit_added` | 808 | `declarator_shape` 330 (return-typed, generic and later-declarator arrows); `accessor_name` 266 (`get x()`); `line_reader_missed` 190 (minified bundles with several functions per line, class overloads); `second_unit_on_line` 22 |
| `unit_start_corrected` | 272 | `receiver_token_start` 270 (`express_route`), `member_receiver` 2 (`prisma_query`) |
| `unit_kind_changed` | 22 | `react_component` to `function`/`arrow_function` by `</` text in a string or comment (`jsx_text_lookalike`; acorn) |
| `unit_merged` / `unit_split` | 14 / 4 | `method`: overloads and adjacent members that the brace extent joined |
| `unknown_kind_changed` | 108 | on `unit_start_corrected` pairs, UNKNOWN to UNKNOWN: `unsupported_route_method` to `dynamic_route_call` 56, `dynamic_route_call` to `unresolved_express_receiver` 52 |
| `anchor_changed` | 0 | |

Refused files (`degraded_unit_set`, counted separately, one context): 8 of the 11
refused files differ; the rows are 338 retired and 43 added `method` units, 7 retired
`function` and 2 retired `test_suite` (comment text), 12 added `function`/
`arrow_function`, plus end-byte and merge/split rows. No dispatched unit is
gained or lost other than the 2 comment `test_suite` rows.

### Byte capture (diagnostic)

`capture.py --compare base-capture-main-8cdefea.json`: the baseline binary
(`main@5097d7a`) reproduces it with 0 mismatches over 179 roots and 4,681 rows.
The candidate: 179 roots, 4,680 rows (one `member:` row fewer: the retired
comment unit), **507 mismatches in 28 roots**, which are exactly the 28 roots
the family differential labels non-identical (26 `unit_boundary_only`, the
`lookalikes.ts` retirement, the Drizzle root); by surface: `member` 270,
`check` 54, `explain` 54, `generation` 28, `units` 28, `family`/`mcp_family` 23
each, `find`/`mcp_find` 13 each, `resync` 1 (`indexed_units` falls by the one
retired unit). No `files`, `families` or `unknowns` surface mismatch. The
unit ids embed the end byte, so every TS/JS unit that moved changes an id and
the rows keyed by it. `capture-final.json` SHA-256
`e4a6618d89bd64e763b33e8a597b35becf8cbafa7f9cdca067202bf1a3b6659e`.

### Product evaluation and sync

`product-eval`: decision 2. `sync-equivalence --all`: 14 of 14 PASS.

### Measurements

| Measure | Before (`5097d7a`) | After |
| --- | --- | --- |
| Parses per TS/JS file | 2 (extractor runner names, then the anchor pass) | 1 (`TsJsAnalysis`); `exact_test_runner_call_names` and `exact_framework_anchors` are `cfg(test)` only |
| Non-test lines, `syntax.rs` | 2,384 | 1,220 |
| Non-test lines, `tsjs/units.rs` | 0 | 969 |
| `legacy_units.rs` (test-only) | 0 | 1,281 |
| Release parse time, 885 external sources x 2 contexts (`parse` incl. anchors, one release test build) | line reader 3.90 s (2.2 ms per parse; includes its second analysis) | tree 1.47 s (0.83 ms per parse) |
| Release `repogrammar` | 41,905,248 B | 41,920,496 B (+15,248) |

### Residuals

- `delimiters_are_balanced` (the unbalanced-delimiter diagnostic) and
  `tsjs_import_resolution_facts` (import and `require` specifier facts) still
  read lines and are not units; they are the remaining text readings of
  `syntax.rs` for TS/JS.
- The shape features of ADR slice 2 (`handler_shape`, `object_clause_shape`,
  Zod field counts, ...) still read the unit's text; with tree-exact unit ends
  they now read the statement and nothing past it.
- The Prisma and Drizzle unit tests rely on the chain end; a statement whose
  call chain is broken by a parenthesised or `await`ed sub-expression ends at
  the inner chain.
