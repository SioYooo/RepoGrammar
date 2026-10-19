# Anchor role rows (Phase 2b)

Status: **QUALIFIED** (6 of 9 roles as rows; see Results). Preregistered
in `7cfe23b`. Base: the Phase 2a commit `eb50101` on
`feat/tree-sitter-frontends`. Local only; no push or merge. Context:
[binding engine](binding-engine.md) (Phase 2a, `QUALIFIED`), ADR-0057 and
ADR-0058.

## Question

How much of a lane's role decision (which anchor applies to a declaration)
can be expressed as data rows over the shared binding engine, with output
byte-identical? This is a measurement, not a threshold test. Precedence,
context propagation and conditional UNKNOWNs stay explicit Rust that reads
the rows. If the code that reads the rows starts replaying the lane's control
flow, that is the finding.

## Scope

C# only, in `adapters/parsing/csharp.rs`:

- class-like roles: ASP.NET Core controller (attribute or base),
  EF Core `DbContext` and FluentValidation validator;
- method roles: xUnit, NUnit and MSTest test attributes. ASP.NET Core actions
  already read the `HTTP_ROUTE_ATTRIBUTES` table.

The EF Core `DbSet` entity set (a property-type shape) and minimal-API routes
(receiver chain) are declared as code, not rows, before the experiment.

## Measurement

- Roles whose positive condition is a row: N of the 9 C# roles.
- Lines of role-decision code before and after (`scan_class_like`,
  `scan_method`, `test_method_anchor`), split into row data and remaining
  logic.
- The size of any row-reading code added.

## Oracle

Same as Phase 2a: zero capture mismatches over 179 roots;
`sync-equivalence` 14/14; `product-eval` equal apart from timing, clock and
commit fields; identical parse snapshots of the 4,818-file Unity corpus and
the committed C# fixtures; all C# unit tests passing. Any difference stops the
experiment. The outcome is `QUALIFIED` with measured coverage, or `NO_GO`
with the construct that could not be expressed.

## Results

Oracle: met. Zero capture mismatches over 179 roots and 4,681 rows;
`sync-equivalence` 14/14; `product-eval` equal apart from the excluded
fields; byte-identical parse snapshots of the 4,818-file Unity corpus and the
11 committed C# fixtures; 44/44 C# unit tests passing. The run was repeated
after the class rows alone and again after the test rows.

The engine side is `RoleEvidence`, `RoleRow`, `RoleMatch` and
`ImportScope::first_role` in `adapters/parsing/binding.rs` (76 lines, with a
unit test). Rows are tried in order and the first exactly bound evidence wins,
so row order is the lane's role precedence and evidence order breaks ties
inside a role.

| Role | Positive condition | Where |
| --- | --- | --- |
| ASP.NET Core controller | row (`ApiController`, `ControllerBase`, `Controller`) | `CLASS_ROLES` |
| EF Core `DbContext` | row | `CLASS_ROLES` |
| FluentValidation validator | row | `CLASS_ROLES` |
| xUnit test (`Theory`, `Fact`) | row | `TEST_ROLES` |
| NUnit test (`TestCase`, `Test`) | row | `TEST_ROLES` |
| MSTest test | row | `TEST_ROLES` |
| ASP.NET Core action | table, unchanged (`HTTP_ROUTE_ATTRIBUTES`) | frameworks table |
| EF Core `DbSet` entity set | code (property-type shape), preregistered | `scan_property` |
| Minimal-API route | code (receiver chain), preregistered | `scan_invocation` |

Six of the nine roles moved to rows; the action role was already a table.

| Code | Before (lines) | After (lines) |
| --- | --- | --- |
| `scan_class_like` | 191 | 105 |
| `test_method_anchor` | 111 | 110 |
| `scan_method` | 167 | 167 |
| Row data (`CLASS_ROLES`, `TEST_ROLES`) | 0 | 143 |
| Row types (`ClassRole`, `RoleUnknown`, `ClassContext`) | 0 | 33 |
| `csharp.rs` total | 2,714 | 2,810 |

## Findings

1. The class roles are fully data. The positive condition, the anchor's
   target, kind and note, the role's typed UNKNOWNs and the member context it
   opens are all fields of a row. One generic block in `scan_class_like` reads
   any row. The only role-specific switch left is `names_route_evidence`,
   which adds the controller's two extra assumptions.
2. For tests, the rows give only the positive condition. The data shape
   stays per-framework code that dispatches on the matched row:
   - the xUnit `MemberData` plugin;
   - the MSTest `[TestClass]` context gate and its blocked claim;
   - the MSTest `DataRow` probe.
   The dispatch replaced the three `if exact(...)` chains with a `match`, so
   `test_method_anchor` stayed the same size. This is the replay the question
   warned about. Moving it into rows would need a per-role data-shape hook,
   which is a function, not data.
3. Rows do not save C# lines (+96 net); what they buy is one declared
   precedence and evidence list per role. A lane adding a role with existing
   evidence kinds (an exact annotation or base type) adds a row, not a branch.
   A role whose evidence is a property type, a receiver chain or a context gate
   still needs code.

For Phase 3 (TS/JS), this means using rows for the positive conditions of
decorator roles, such as NestJS controllers and route handlers. Call-chain
roles stay code over the shared engine: Express routes and Jest/Vitest
`describe`/`it`. `RoleEvidence` should grow
only when a second lane needs the same new evidence kind.
