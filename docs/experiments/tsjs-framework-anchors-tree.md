# TS/JS framework anchors from the Tree-sitter tree (ADR-0059 slice 2)

Status: **PREREGISTERED**. Base: `feat/tsjs-tree-sitter` at `32764f2`. Local
only; no push or merge. Authority:
[ADR-0059](../decisions/ADR-0059-tree-sitter-tsjs-frontend-in-slices.md)
(Accepted), D2 slice 2, D3 (policies stay), D4 (syntax errors), D5 (oracle),
D6 (legacy reader stays under `cfg(test)` until acceptance). Slice 1:
[tsjs-scope-graph-tree](tsjs-scope-graph-tree.md). Engine:
[binding-engine](binding-engine.md) and [anchor-role-rows](anchor-role-rows.md).

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
