# TS/JS framework anchors from the Tree-sitter tree (ADR-0059 slice 2)

Status: **CLASSIFIED, accepted 2026-10-22** (preregistered in
`588ff4a`; see "Acceptance"). Base: `feat/tsjs-tree-sitter` at `32764f2`. Local only; no push
or merge. Authority:
[ADR-0059](../decisions/ADR-0059-tree-sitter-tsjs-frontend-in-slices.md)
(Accepted), D2 slice 2, D3 (policies stay), D4 (syntax errors), D5 (oracle),
D6 (legacy reader stays under `cfg(test)` until acceptance; deleted on acceptance). Slice 1:
[tsjs-scope-graph-tree](tsjs-scope-graph-tree.md). Engine:
[binding-engine](binding-engine.md) and [anchor-role-rows](anchor-role-rows.md).

## Acceptance

On 2026-10-22 the maintainer accepted all six decisions under "Decisions for
the maintainer": (1) the `drizzle_exact_repositories` UNKNOWN reason flip
`UnresolvedImport` to `FrameworkMagic` (the 2 capture mismatches); (2) every
`anchor_changed` on the branch corpus (Express 5, NestJS 5, Zod
`declarator_type_annotation`); (3) every `unknown_removed`; (4) every
`unknown_added` (`unit_not_code`); (5) the unlisted classes `keyword_receiver`,
`member_receiver`, `table_expression_prefix` and the widened `unit_not_code`;
(6) deletion of the `cfg(test)` oracle. The text readings (`legacy_text.rs`)
and the differential (`anchor_differential.rs`, `anchor_corpus.rs`) are
detached from the build, together with the `Reading` seam in `parts.rs` and
`scope_graph.rs`; the three files are deleted by the main session. The
accepted cases are kept as the plain test
`slice2_accepted_differences_hold_on_the_tree_reading` in `tsjs/mod.rs`, which
asserts the tree reading's anchors, key assumptions and UNKNOWN kinds on 39
minimal sources (Express, Fastify, Hono, Prisma, Drizzle, Zod, Jest/Vitest and
NestJS) with no legacy call. The measurements below were taken with the oracle
present. After the merge (`main@8cdefea`) the fixture-wide capture differs from
the pre-slice-2 baseline in exactly the two accepted
`drizzle_exact_repositories` rows (`generation`, `unknowns`); that capture is
the new baseline, SHA-256
`53d288682c17a4c8c1afab5c0d422d67502d0304bb4dd4a638c6766b6f00a389`, retained
with its predecessor
(`e493fdb9ae47e2f5e83a0b2a81d7f95de1e374fe4621eef3b9c6ad283423a85e`) outside
the repository under
`/Volumes/External/Development/repogrammar-evidence/capture-baselines-20261022/`.

## Objective

The framework anchor modules in `adapters/parsing/tsjs/` stop reading text
slices for their call parts and read the tree slice 1 already produces:

- (a) receiver classification: which top-level declarator is an Express,
  Fastify, Hono, Prisma or Drizzle receiver, and the Express router prefix
  mount;
- (b) route and query call parts: receiver, member chain, arguments and object
  fields of the called expression;
- (c) decorator roles: the NestJS decorator stack, its arguments, and the role
  each decorator proves, as `RoleRow`s over the binding engine where the
  evidence is an exact decorator binding.

Policies do not change (D3): which anchors exist, the UNKNOWN kinds, the
readiness labels and the unit boundaries (slice 3) stay. The outcome is
`EQUIVALENT`, `CLASSIFIED` or `BLOCKED` under the ADR-0058 tiers, per
framework and overall. A tree with `ERROR` or `MISSING` already yields the
slice 1 `tsjs_parse_degraded` UNKNOWN (D4); slice 2 adds no second refusal
path.

## Readings replaced

Every reading below is a text heuristic over a unit slice (the line-based
extractor's byte range) or over one declarator or statement string. Each
becomes a read of the tree node named in the last column. The unit start byte
selects the node: the innermost `call_expression` that starts at the unit's
start byte (the leftmost call of a chain), the declaration that starts there,
or the `decorator` that starts there.

| Module | Function today | Text heuristic | Tree reading |
| --- | --- | --- | --- |
| `mod.rs` | `route_call_parts` | leading identifier, `.`, identifier, `(` | `call_expression.function` = `member_expression(object: identifier, property)`, no optional chain |
| `mod.rs` | `route_path_shape`, `first_quoted` | first quote character after the first `(`, anywhere | first `arguments` child is a `string`; raw text between its quotes |
| `mod.rs` | `call_arguments`, `route_handler_identifier`, `string_literal_arg` | character scan with depth and quote counters; `[path, handler]` or `[handler]`; "starts with a quote" | `arguments` named children without `comment`; `string` and `identifier` kinds |
| `express.rs` | `anchor` (via the above) | as above | as above |
| `hono.rs` | `anchor` (literal path test) | `call_arguments` + `string_literal_arg` | first argument kind `string` |
| `fastify.rs` | `anchor`, `register_anchor` (via `route_call_parts`) | as above | as above |
| `fastify.rs` | `fastify_full_route_anchor`, `object_literal_string_field`, `object_literal_has_field` | any `method`/`url`/`path`/`handler` followed by `:` anywhere in the slice, then the first quote after it | top-level `pair`, `shorthand_property_identifier`, `method_definition` of the first argument `object` |
| `fastify.rs` | `fastify_plugin_identifier`, `fastify_register_prefix_shape`, `contains_identifier` | first argument is a leading identifier; options start with `{`; `prefix` mentioned anywhere | first argument `identifier`; second argument kind `object`; top-level `prefix` key |
| `prisma.rs` | `prisma_query_parts`, `prisma_transaction_client`, `$transaction([` contains | leading identifier chain of three, `.$transaction(` literally, the text `$transaction([` | callee path `client.model.operation` or `client.$transaction`; first argument kind `array` |
| `drizzle.rs` | `drizzle_query_parts`, `drizzle_transaction_db` | leading identifier chain; first `.from(` anywhere in the slice; leading identifier of the first argument | callee path `db.op`, `db.query.table.op`, `db.transaction`; the `from` call in the same call chain; first argument `identifier` |
| `drizzle.rs` | `table_declaration_parts` (also receiver classification) | `const name = factory(` on one string | `variable_declarator` name `identifier`, value `call_expression` with `identifier` callee |
| `zod.rs` | `schema_builder_parts` | `const name = head.builder(` on one string | first declarator's value, the base call of its call chain: callee path `head.builder` |
| `jest_vitest.rs` | `test_call_name` | leading identifier then `(` | callee path of length one |
| `nest.rs` | `decorator_original`, `parse_decorators`, `matching_paren`, `arg_path_shape`, `module_shape`, `array_member_count`, `bracket_member_count`, `args.contains("forRoot")` | character scan of the leading `@Name(...)` stack; first byte of the arguments; comma counts at depth one; substring | consecutive leading `decorator` siblings; `call_expression` or `identifier` expression; first argument kind; `pair` key `providers`/`imports`/`controllers` with an `array` value (named child count); a call whose callee property starts with `forRoot` |
| `scope_graph.rs` | `express_receiver_declaration`, `express_receiver_from_rhs`, `fastify_receiver_declaration`, `hono_receiver_declaration`, `prisma_client_declaration`, `drizzle_db_declaration`, `top_level_declaration_assignment`, `express_router_prefix_mount` | `const name = ` prefix, then `rhs` text compared with `()` or `new X(` | `variable_declarator.value`: `call_expression` or `new_expression` with identifier head; the mount reads the statement's base call |

## Not read by slice 2 (named, with the reason)

- Shape features: `handler_shape`, `async_shape`, `reply_shape`,
  `schema_present`, `opts_handler_present`, `raw_sql_present`,
  `object_clause_shape`, `select_include_shape`, `drizzle_join_shape`,
  `returning_shape`, the Zod `field_count_shape` and `has_runtime_refinement`.
  They are substring features of the slice that fill assumptions and, for raw
  SQL, one UNKNOWN. ADR-0059 D2 lists receivers, call parts and decorator roles;
  these are neither. They stay text and are named here as a residual.
- `next.rs`: Next.js anchors are file-convention plus substring checks
  (`export default`, JSX-likeness). There is no receiver, call part or
  decorator in them.
- `syntax.rs` unit extraction, including its own receiver scans
  (`fastify_receivers_for_scan`, `static_route_call`): slice 3.

## Expected misreadings of the text readings

Every difference must be attributed to one of these, with the source that
shows it. The names are fixed before the first run.

- `type_arguments`: generic arguments on a call or `new`
  (`app.get<P>("/x", h)`, `new Hono<{ A: { B: C<D> } }>()`,
  `new PrismaClient<O>()`).
- `comment_in_call`: a comment between the callee tokens or among the
  arguments (`app.get("/", /* c */ h)`).
- `path_from_non_first_argument`: the route path read as the first quote
  anywhere after `(`, so a later literal or a string in a handler is taken for
  the path.
- `trailing_comma`: `app.get("/", h,)` splits into an empty extra argument.
- `string_prefix_expression`: an argument that starts with a quote but is an
  expression (`"/a" + x`) taken for a literal.
- `escaped_quote`: a literal containing an escaped quote read to the escaped
  quote.
- `delimiter_in_literal`: a quote, parenthesis or bracket inside a regular
  expression, a template substitution or a comment that unbalances the scan.
- `member_receiver`: a unit that starts at `app` inside `this.app.get(...)` or
  `obj.app.get(...)` is read as the top-level `app`.
- `unit_not_code`: the extractor made a unit from a line inside a comment,
  string or template; the legacy slice looks like a call, the tree has none.
- `declarator_type_annotation`: `const app: Express = express()`; the line
  reader required `=` right after the name.
- `object_field_lookalike`: a field name found in a nested object, a handler
  body, a string or a comment, and its value read from the first quote after it.
- `shorthand_or_method_field`: `{ handler }` or `handler() {}`, which the text
  reading did not count as the `handler` field.
- `prefix_identifier_lookalike`: `prefix` mentioned as a value or a nested key,
  taken for a dynamic prefix.
- `chain_scope`: the first `.from(` anywhere in the slice, or a member tail on
  a Zod or Prisma chain, taken for part of the call chain.
- `table_expression_prefix`: `db.insert(schema.users)` read as the table
  `schema`.
- `decorator_member_form`: `@ns.Name()` ends the legacy stack.
- `decorator_argument_shape`: a comment before the first decorator argument, a
  first argument that starts with a quote but is an expression, a trailing
  comma in a metadata array, a metadata key or value that is only a lookalike
  (`providers` inside a string).
- `for_root_lookalike`: `forRoot` found as a substring of a string, comment or
  longer identifier.
- `syntax_error` (D4): a tree with an `ERROR` or `MISSING` node.

A difference with no attribution is a regression and is fixed within at most
three rounds. A new class that is a correct tree reading is recorded as a
preregistration deviation, never folded into a listed class.

## Difference classes and the differential

For every unit the anchor pass considers, the decision is the anchor
(target and assumptions), the UNKNOWN kinds, or none. The legacy reading is the
text implementation kept under `cfg(test)`; the new reading is the production
tree reading. Each source is run without and with a test-runner project
context, exactly as in slice 1. A class is one of:

- `unknown_added`: anchor before, UNKNOWN after;
- `unknown_removed`: UNKNOWN before, anchor after (adds a support anchor);
- `anchor_changed`: the target or assumptions differ, or an anchor appears or
  vanishes without an UNKNOWN;
- `unknown_kind_changed`: UNKNOWN on both sides with a different kind.

`anchor_changed` and `unknown_removed` need explicit maintainer acceptance,
citing the legacy misreading for each case. Field level (diagnostics, not
gating): `receiver_changed`, the symmetric difference of the receiver sets
and prefix map.

Corpora:

- the 82 committed TS/JS fixture sources under `src/fixtures`;
- a branch corpus in the test module: every framework, every receiver form,
  every misreading above with its near-misses, and the syntax-error
  placements;
- the external corpus of slice 1: the TS/JS sources of `SurchargeCheck`
  (with its installed `vitest`), `SurchargeCheck-seo`, `askveil` (with its
  installed `zod`) and `poppet-public-seed` under `/Volumes/External/Development`,
  excluding `node_modules` elsewhere. No `fastify`, `express` or `hono` package
  is installed locally, so Fastify, Hono and Express are exercised there only
  through the projects' own sources.

## Oracle and baselines

All must hold on the final tree; the differential classifies, the oracle
confirms.

- `cargo build --release`.
- `python3 src/experiments/language_pack/capture.py ... --compare
  .../ts/base-capture-1.json`: 179 roots, 4,681 rows, mismatches ideally 0;
  every mismatch must be classified.
- `repo-guard sync-equivalence --all`: 14 PASS.
- `repo-guard product-eval` against the query corpus, compared with
  `compare_product_eval.py` to the frozen `base4` result: `equal: true`, or
  differing only in a timing field.

## Verdict rule

- `EQUIVALENT`: zero decision differences on every corpus, and zero
  `receiver_changed` that is not attributed.
- `CLASSIFIED`: every difference is attributed to a listed class; none on a
  committed fixture is unattributed; `unknown_removed` and `anchor_changed`
  are listed for acceptance.
- `BLOCKED`: an `anchor_changed` or `unknown_removed` on a committed fixture
  that no listed class explains, or a tree reading that cannot reach
  `CLASSIFIED` in three rounds. That framework keeps its text reading in
  production and the reason is recorded.

The verdict is reported per framework (Express, Hono, Fastify, Prisma,
Drizzle, Zod, NestJS, Jest/Vitest) and overall (the weakest).

## Stop conditions

1. Committed fixtures: an unattributed decision difference ends that
   framework as `BLOCKED`.
2. At most three rounds per framework, each round targeting one observed
   class with one change and a negative test.
3. No new dependency, no policy, readiness, schema or UNKNOWN-taxonomy change.
4. Every parse is shared: `ScopeGraphLite` carries the tree it read, and the
   anchor pass must not parse again.

## Measurements

Non-test lines of each touched module before and after; parse count per file
(the extractor's runner-name analysis and the anchor pass are the two
analyses; slice 2 must add none); parse and anchor time on the external
corpus in a release test build, before and after.

## Implementation log

- Shared seam and routes: `parts.rs` reads receivers and route call parts from the
  tree `ScopeGraphLite` keeps (one parse per analysis); `legacy_text.rs` keeps
  the text readings under `cfg(test)`; `anchor_differential.rs` and
  `anchor_corpus.rs` are the differential. Express, Hono and Fastify consume
  `CallParts`. Preregistration deviation: `keyword_receiver` (`this.get(...)`,
  `super.get(...)` read as an identifier receiver; UNKNOWN on both sides).
- Prisma and Drizzle: `PrismaQuery`, `PrismaTransaction` and `DrizzleQuery` are
  read from the callee path of the innermost call that starts at the unit
  start; the Drizzle `from` table is read from the same call chain. The
  committed fixture `drizzle_exact_repositories` gets two
  `unknown_kind_changed` units (`member_receiver`: the extractor starts a
  Prisma-shaped unit inside `db.query.users.findMany(`).
- Zod and Jest/Vitest: the Zod builder is read from the first call of the
  initializer's call chain; the test-runner call name is read from a
  one-segment callee. Preregistration deviation: `unit_not_code` also covers a
  unit that starts at a method declaration (`describe(d: string) {`), not only
  a comment, string or template.
- NestJS: `NEST_ROLES` (Controller, Injectable, Module, Route) are `RoleRow`s
  with `RoleEvidence::Annotation` on `@nestjs/common`; `ImportScope::first_role`
  decides the target. The scope graph still decides whether a decorator name is
  an exact binding (JS imports bind a local name to an export, which
  `ImportScope` does not model), so the decorators handed to the row are the
  exactly bound ones, renamed to their `@nestjs/common` export. Decorator
  arguments (path literal, module metadata counts, `forRoot` calls) are read
  from the call's arguments. No Nest, Express, Fastify, Hono, Prisma or
  Drizzle source exists in the external corpus, so those forms are covered by
  the committed fixtures and the branch corpus only.
- The differential times the whole anchor pass per reading (`text_pass`,
  `tree_pass`), analysis included.

## Results

Verdict: **`CLASSIFIED`** overall and for every framework (Express, Hono,
Fastify, Prisma, Drizzle, Zod, NestJS, Jest/Vitest). Pending maintainer
acceptance: `anchor_changed`, `unknown_removed` and `unknown_added`, listed
under "Decisions for the maintainer". Rounds taken: 0 of 3 (no unattributed
decision difference on a committed fixture, so no fix round). Commits:
`1e34b0c` (receivers and route calls: Express, Hono, Fastify), `3a757c0`
(Prisma, Drizzle), `c3f37f1` (Zod, Jest/Vitest), `af9baaa` (NestJS), `61e8ab2`
(differential timing).

### Decisions for the maintainer, first

1. **One product-visible change on a committed fixture.** Fixture
   `typescript/release/v0_2/drizzle_exact_repositories` (`db.query.users.findMany(`)
   gets two `unknown_kind_changed` units. The line-based extractor (slice 3)
   starts a Prisma-shaped unit at `query.users.findMany(` in the middle of the
   chain `db.query.users.findMany(`. The text reading took `query` for a Prisma
   client (`prisma_injected_client`, `UnresolvedImport`); the tree finds no call
   that starts there (`prisma_dynamic_model_or_operation`, `FrameworkMagic`).
   Both are UNKNOWN and neither anchors, but `repogrammar unknowns` on that
   fixture changes: `by_reason_code` and `by_obligation` flip
   (`UnresolvedImport` to `FrameworkMagic`, `symbol_binding` to
   `framework_identity`). This is the whole of the fixture-wide capture
   mismatch (below). The unit itself is a mis-extraction, so neither kind is
   "right" until slice 3 stops making it. The alternative that gives 0
   mismatches is to read the suffix of a callee chain when no call starts at
   the unit start, which would re-encode the `member_receiver` misreading in
   production; it was not done.
2. **`anchor_changed`** (each a legacy misreading; for Express,
   `comment_in_call` and `trailing_comma` add the `handler_binding=local`
   assumptions the text reading lost, and the others correct or drop a
   `route_path_shape` the text reading invented):
   - Express, 5 branch sources: `comment_in_call`, `path_from_non_first_argument`,
     `trailing_comma`, `string_prefix_expression`, `escaped_quote`.
   - NestJS, 5 branch sources: `decorator_argument_shape` (comment before the
     first argument; trailing comma in a metadata array; a metadata key
     mentioned inside a string; an array whose members are all object literals,
     which the text counted as 0; a sole string member, counted as 0) and
     `string_prefix_expression`.
   - Zod, `declarator_type_annotation` (`const A: ZodType = z.object(...)`;
     `AnchorOutcome::None` becomes `zod.object`): 1 branch source and, in the
     unpreregistered extension below, 4 distinct units (8 with both contexts)
     in an installed `zod`'s tests.
3. **`unknown_removed`** (adds a support anchor): Express 1, Fastify 4
   (`declarator_type_annotation`, `shorthand_or_method_field` x2,
   `prefix_identifier_lookalike`), Prisma 3 (`type_arguments`,
   `declarator_type_annotation`, spaced `$transaction( [`), Drizzle 2
   (`declarator_type_annotation`, `chain_scope`), NestJS 1 (`for_root_lookalike`).
   All branch sources; none on a committed fixture or in either external
   corpus.
4. **`unknown_added`**: 9 branch sources, and 196 differences (97 distinct units
   in the run without a context) in the external corpus, all `unit_not_code`: the text reading anchored commented-out
   `test(...)`, `describe(...)` and route lines in zod's tests; the tree refuses
   them. Residual: those units get `dynamic_test_call` / `dynamic_route_call`
   ("call shape is dynamic"), a misnomer for a unit that is not code, until
   slice 3 stops making the unit.
5. The unlisted classes (preregistration deviations): `keyword_receiver`
   (`this.get(`, `super.get(`: `this`/`super` read as an identifier receiver;
   UNKNOWN on both sides), `unit_not_code` widened to a method declaration
   (`describe(d) {`), `table_expression_prefix` made explicit for Drizzle
   (`db.delete(request("x"))`), and `member_receiver` observed for Prisma and
   Drizzle units (a unit that starts inside a longer chain, `drill.a.b.delete(`
   or `this.#ctes.delete(`). All are UNKNOWN to UNKNOWN. Accept them, or require
   them pinned back to the text reading.
6. Keep `legacy_text.rs`, `anchor_differential.rs` and `anchor_corpus.rs`
   under `cfg(test)` until this is accepted (D6).

### What reads the tree now

`ScopeGraphLite` keeps the tree it read (`tree`) and a `reading` field. The
new `tsjs/parts.rs` is the one reader of call parts, receivers and decorators;
the anchor modules keep every policy and consume its data types.

| Framework | Reads the tree now | Verdict |
| --- | --- | --- |
| Express | callee path and arguments of the unit's call (`CallParts`); receiver declarators and the router prefix mount | `CLASSIFIED` |
| Hono | the same call parts; `new Hono<...>()` receivers | `CLASSIFIED` |
| Fastify | call parts, full-route object fields, plugin and `prefix` options | `CLASSIFIED` |
| Prisma | `client.model.operation` and `client.$transaction` callee paths, the array argument; `new PrismaClient(...)` receivers | `CLASSIFIED` |
| Drizzle | `db.op`, `db.query.table.op` and `db.transaction` callee paths, the `from` table in the same call chain, table declarators, `drizzle(...)` db receivers | `CLASSIFIED` |
| Zod | the first call of the initializer's call chain | `CLASSIFIED` |
| NestJS | the leading `decorator` siblings and their arguments; roles are `RoleRow`s | `CLASSIFIED` |
| Jest/Vitest | the one-segment callee of the test call | `CLASSIFIED` |
| Next.js | not read (no receiver, call part or decorator) | unchanged |

Shape features stay text, as preregistered: `handler_shape`, `async_shape`,
`reply_shape`, `schema_present`, `opts_handler_present`, `raw_sql_present`,
`object_clause_shape`, `select_include_shape`, `drizzle_join_shape`, the
`returning`/`transaction` flags and the Zod field-count and refinement checks.

NestJS role table (`NEST_ROLES` in `nest.rs`): 4 roles, 11 evidence rows, all
`RoleEvidence::Annotation` on `@nestjs/common`.

| Role | Evidence (target) |
| --- | --- |
| Controller | `Controller` (`nestjs.common.Controller`) |
| Injectable | `Injectable` (`nestjs.common.Injectable`) |
| Module | `Module` (`nestjs.common.Module`) |
| Route | `Get`, `Post`, `Put`, `Delete`, `Patch`, `Head`, `Options`, `All` (`nestjs.common.<Name>`) |

The honest shape is rows over the engine's types with the gate upstream: the
scope graph decides whether a decorator name is an exact binding
(`imports.get` plus `name_is_unsafe_at`), because a JS named import binds a
local name to an export and `ImportScope` cannot model that (ADR-0059 D2).
`first_role` then only resolves the evidence to the target, and the first
decorator in source order that names a row's evidence is the one it is given,
so the legacy source-order precedence is kept (no `row_precedence` class).
`legacy_text::receivers` renders every declarator as `const ...`; that is
faithful to the legacy reader, which stripped `const`, `let` and `var`
identically.

### Counts

Decision level, per considered unit. Each source is run without and with a
test-runner project context, so a unit can count twice.

| Corpus | Sources | `anchor_changed` | `unknown_removed` | `unknown_added` | `unknown_kind_changed` |
| --- | --- | --- | --- | --- | --- |
| Committed fixtures | 82 | 0 | 0 | 0 | 4 (2 units, `member_receiver`) |
| Branch corpus | 65 (27 identical) | 11 sources | 11 sources | 9 sources | 7 sources |
| External (slice 1's roots) | 885 compared, 2 skipped, 11 degraded, 0 extractor panics | 0 | 0 | 196 | 162 |
| External extension (unpreregistered) | 3,031 compared, 32 skipped, 30 degraded | 8 | 0 | 216 | 532 |

External (slice 1's roots: `SurchargeCheck` with its installed `vitest`,
`SurchargeCheck-seo`, `askveil` with its installed `zod`,
`poppet-public-seed`; 6,522 considered units). Every difference is attributed
(0 unattributed). By misreading class, with both contexts counted, and in
parentheses the distinct units of the run without context:

| Class | `unknown_added` | `unknown_kind_changed` |
| --- | --- | --- |
| `unit_not_code` (comment, string or template line; a method named `describe`/`it`) | 196 (97 distinct) | 90 (46) |
| `keyword_receiver` | 0 | 64 (32) |
| `table_expression_prefix` | 0 | 6 (3) |
| `member_receiver` | 0 | 2 (1) |

The 11 degraded external files are slice 1's grammar gaps (TypeScript variance
modifiers in 4 `zod` `.ts` files; JavaScript keyword export names in 7 `zod`
`.js` files), refused once by D4 on both sides. The panic that blocked 3
external files in slice 1 is fixed on `main` (`dca43c0`), so none are excluded.

The extension was an unpreregistered run over the other local projects
(`AgentDesk`, `ClearPDF`, `PleaseRuinIt`, `Poppet`, `SeedWorld`, `appfitsme`
and its `dev`/`heldout` copies, `worthfuel`, `tlm-asset-tools` and eight
smaller ones, with the installed `vitest` and `zod` of two of them), run to
look for framework sources the preregistered corpus lacks. It found none for
Nest, Express, Fastify, Hono, Prisma or Drizzle either, and its 30 degraded
files were not individually attributed. Its 8 `anchor_changed` (4 distinct) are
the Zod `declarator_type_annotation` class; all its other differences are
`unit_not_code`, `keyword_receiver`, `member_receiver` and
`table_expression_prefix`. So the framework forms are exercised by the
committed fixtures and the branch corpus only.

`syntax_error` (D4) is not observable in this slice's differential: both
readings share the parse and degrade together, so a syntax error gives no
difference, and the branch source `error_route.ts` expects none. D4 is
evidenced by the branch assertion that exactly that source is degraded and by
slice 1's `syntax_errors_refuse_every_considered_unit_once`. Slice 2 adds no
second refusal path.

### Gates and oracle

- `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets
  --all-features -- -D warnings`, `cargo test --workspace --all-features`
  (Homebrew Python first): pass. `repo-guard check` is run by the main
  session.
- Fixture-wide capture against the frozen baseline: 179 roots, 4,681 rows,
  **2 mismatches**, both `drizzle_exact_repositories` and one cause: `unknowns`
  (the reason-code change above) and `generation`, which differs only in the
  `evidence` and `semantic_facts` table hashes (same row counts). The 179th root
  is an empty untracked directory `v0_1/stale-index/` that exists in the main
  checkout and not in a worktree (git does not track empty directories); in a
  worktree the capture reports 178 roots, 4,674 rows and 7 more `stale-index`
  mismatches until that directory is created.
- `sync-equivalence`: 14 of 14 PASS.
- `product-eval`: equal to the frozen result apart from `summary.latency_ms_p50`
  (44 to 33 ms), a timing field the preregistered exclusion list missed.

### Measurements

| Measure | Before (`32764f2`) | After |
| --- | --- | --- |
| Non-test lines, `tsjs/` modules touched (without `legacy_text.rs`) | 3,168 | 3,615 |
| `legacy_text.rs` (test-only) | 0 | 792 |
| `parts.rs` | 0 | 862 |
| `scope_graph.rs` | 768 | 568 |
| `mod.rs` | 499 | 421 |
| `drizzle.rs` / `nest.rs` / `zod.rs` | 274 / 339 / 132 | 212 / 286 / 114 |
| Release `repogrammar` size | 41,899,392 B (slice 1) | 41,905,248 B (+5,856) |

Production code grows (+447 lines, +14%): the tree readers are longer than the
text scanners they replace, which moved to the test-only oracle.

Release timing, anchor pass per reading, 885 external sources x 2 contexts:
text reading 1.149 s, tree reading 1.148 s; graph analysis alone 0.587 s and
0.559 s. They are equal because both readings parse the tree (slice 1 moved the
parse), so the claim is only that slice 2 adds no measurable cost over slice 1.

Parse sharing: `ScopeGraphLite` carries the `Tree`, and `parts.rs` reads
`bindings.tree`, so the anchor pass parses each file once. Slice 2 adds no
parse. The remaining duplication is slice 1's: the line-based extractor calls
`exact_test_runner_call_names`, which analyses the file once, and the anchor
pass analyses it again (two parses per TS/JS file). Sharing one graph needs the
extractor to hold it, which belongs to slice 3.

### Residuals

- Unit boundaries still come from the line-based extractor, so a unit that is
  not code, or starts mid-chain, is anchored or refused by a tree reading of a
  region it mis-bounded (`unit_not_code`, `keyword_receiver`, `member_receiver`
  above). Slice 3.
- The extractor's own text receiver scans (`fastify_receivers_for_scan`,
  `hono_receivers_for_scan`) still decide a route unit's kind. A typed or
  generic declarator that the tree reads as a Hono receiver therefore changes
  `receiver_changed` only: the unit stays `ExpressRoute`, so no decision
  changes (branch source `typed_hono.ts`).
- The tree reading is exact about type arguments on a receiver `new`, but the
  extractor makes no unit of `app.get<P>(`, so route calls with type arguments
  are not units yet (branch source `typed_route.ts`). Slice 3.
